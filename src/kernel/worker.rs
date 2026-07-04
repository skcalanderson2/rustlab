//! Kernel lifecycle: launch a kernel process from a kernelspec, own its ZeroMQ
//! channels in background tasks, and bridge them to the app through mpsc channels.

use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use jupyter_protocol::{ConnectionInfo, JupyterMessage, ShutdownRequest, Transport};
use jupyter_zmq_client::connection::{
    create_client_control_connection, create_client_iopub_connection,
    create_client_shell_connection_with_identity, peek_ports_with_listeners,
    peer_identity_for_session,
};
use jupyter_zmq_client::kernelspec::KernelspecDir;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum KernelCommand {
    /// Send a message on the shell channel (execute_request, kernel_info_request, ...).
    Shell(JupyterMessage),
    /// Send a message on the control channel (interrupt_request, ...).
    Control(JupyterMessage),
    /// Graceful shutdown: shutdown_request on control, then kill after a grace period.
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum KernelEvent {
    IoPub(JupyterMessage),
    ShellReply(JupyterMessage),
    Exited(Option<i32>),
}

#[derive(Clone)]
pub struct KernelHandle {
    pub commands: mpsc::Sender<KernelCommand>,
    pub session_id: String,
    pub connection_info: ConnectionInfo,
}

/// Launch a kernel from its kernelspec. Returns a handle for sending commands
/// and a receiver of everything the kernel emits.
pub async fn launch(spec: KernelspecDir) -> Result<(KernelHandle, mpsc::Receiver<KernelEvent>)> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let ip: IpAddr = "127.0.0.1".parse().expect("valid ip");

    let (ports, listeners) = peek_ports_with_listeners(ip, 5).await?;
    let connection_info = ConnectionInfo {
        ip: "127.0.0.1".to_string(),
        transport: Transport::TCP,
        shell_port: ports[0],
        iopub_port: ports[1],
        stdin_port: ports[2],
        control_port: ports[3],
        hb_port: ports[4],
        key: uuid::Uuid::new_v4().to_string(),
        signature_scheme: "hmac-sha256".to_string(),
        kernel_name: Some(spec.kernel_name.clone()),
    };

    let connection_path = write_connection_file(&session_id, &connection_info).await?;

    let kernel_name = spec.kernel_name.clone();
    let mut cmd = spec.command(&connection_path, None, None)?;
    // If the app dies without a graceful shutdown, take the kernel with us.
    cmd.kill_on_drop(true);
    let child = cmd
        .spawn()
        .with_context(|| format!("spawning kernel process for `{kernel_name}`"))?;
    // The kernel binds these ports itself as its first action; holding the
    // listeners any longer would block it.
    drop(listeners);

    // The kernel takes a moment to bind its sockets; retry until reachable.
    let identity = peer_identity_for_session(&session_id)?;
    let shell = connect_with_retry(|| {
        create_client_shell_connection_with_identity(&connection_info, &session_id, identity.clone())
    })
    .await
    .context("connecting to kernel shell channel")?;
    let iopub = connect_with_retry(|| create_client_iopub_connection(&connection_info, "", &session_id))
        .await
        .context("connecting to kernel iopub channel")?;
    let control = connect_with_retry(|| create_client_control_connection(&connection_info, &session_id))
        .await
        .context("connecting to kernel control channel")?;

    let (cmd_tx, cmd_rx) = mpsc::channel::<KernelCommand>(64);
    let (evt_tx, evt_rx) = mpsc::channel::<KernelEvent>(256);

    // IOPub reader: forward every broadcast message to the app.
    let mut iopub = iopub;
    let iopub_tx = evt_tx.clone();
    tokio::spawn(async move {
        loop {
            match iopub.read().await {
                Ok(msg) => {
                    if iopub_tx.send(KernelEvent::IoPub(msg)).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Shell: split so replies can be read while requests are sent.
    let (shell_send, mut shell_recv) = shell.split();
    let shell_tx = evt_tx.clone();
    tokio::spawn(async move {
        loop {
            match shell_recv.read().await {
                Ok(msg) => {
                    if shell_tx.send(KernelEvent::ShellReply(msg)).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    tokio::spawn(command_loop(
        child,
        shell_send,
        control,
        cmd_rx,
        evt_tx,
        connection_path,
    ));

    Ok((
        KernelHandle {
            commands: cmd_tx,
            session_id,
            connection_info,
        },
        evt_rx,
    ))
}

async fn command_loop(
    mut child: tokio::process::Child,
    mut shell_send: jupyter_zmq_client::connection::DealerSendConnection,
    mut control: jupyter_zmq_client::connection::ClientControlConnection,
    mut cmd_rx: mpsc::Receiver<KernelCommand>,
    evt_tx: mpsc::Sender<KernelEvent>,
    connection_path: PathBuf,
) {
    loop {
        tokio::select! {
            status = child.wait() => {
                let code = status.ok().and_then(|s| s.code());
                let _ = evt_tx.send(KernelEvent::Exited(code)).await;
                break;
            }
            cmd = cmd_rx.recv() => match cmd {
                Some(KernelCommand::Shell(msg)) => {
                    let _ = shell_send.send(msg).await;
                }
                Some(KernelCommand::Control(msg)) => {
                    let _ = control.send(msg).await;
                }
                Some(KernelCommand::Shutdown) | None => {
                    let shutdown: JupyterMessage = ShutdownRequest { restart: false }.into();
                    let _ = control.send(shutdown).await;
                    let code = match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
                        Ok(status) => status.ok().and_then(|s| s.code()),
                        Err(_elapsed) => {
                            let _ = child.kill().await;
                            None
                        }
                    };
                    let _ = evt_tx.send(KernelEvent::Exited(code)).await;
                    break;
                }
            }
        }
    }
    let _ = tokio::fs::remove_file(&connection_path).await;
}

async fn write_connection_file(session_id: &str, info: &ConnectionInfo) -> Result<PathBuf> {
    let runtime_dir = jupyter_zmq_client::dirs::runtime_dir();
    tokio::fs::create_dir_all(&runtime_dir)
        .await
        .with_context(|| format!("creating runtime dir {}", runtime_dir.display()))?;
    let path = runtime_dir.join(format!("kernel-rustlab-{session_id}.json"));
    tokio::fs::write(&path, serde_json::to_vec_pretty(info)?)
        .await
        .with_context(|| format!("writing connection file {}", path.display()))?;
    Ok(path)
}

async fn connect_with_retry<T, F, Fut>(mut connect: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = jupyter_zmq_client::Result<T>>,
{
    const ATTEMPTS: u32 = 100;
    let mut last_err = None;
    for _ in 0..ATTEMPTS {
        match connect().await {
            Ok(conn) => return Ok(conn),
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
    Err(last_err.expect("at least one attempt").into())
}
