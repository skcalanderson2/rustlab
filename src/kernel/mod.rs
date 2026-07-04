pub mod discovery;
pub mod worker;

use anyhow::{Context, Result};
use jupyter_protocol::{
    ExecuteRequest, ExecutionState, JupyterMessage, JupyterMessageContent, KernelInfoRequest,
};

use worker::{KernelCommand, KernelEvent};

/// End-to-end smoke test of the kernel plumbing, no GUI:
/// discover kernelspecs, launch a Python kernel, run `2+2`, shut down.
pub async fn headless_test() -> Result<()> {
    let specs = discovery::list_kernelspecs().await;
    println!("Discovered {} kernelspecs:", specs.len());
    for s in &specs {
        println!(
            "  {:<30} lang={:<10} \"{}\"",
            s.kernel_name, s.kernelspec.language, s.kernelspec.display_name
        );
    }

    let spec = specs
        .iter()
        .find(|s| s.kernel_name == "python3")
        .or_else(|| specs.iter().find(|s| s.kernelspec.language == "python"))
        .cloned()
        .context("no python kernelspec found")?;
    println!("\nLaunching kernel `{}`...", spec.kernel_name);

    let (handle, mut events) = worker::launch(spec).await?;

    // Handshake: kernel_info_request until the kernel replies.
    let info_req: JupyterMessage = KernelInfoRequest {}.into();
    handle.commands.send(KernelCommand::Shell(info_req)).await?;

    let exec: JupyterMessage = ExecuteRequest::new("2 + 2".to_string()).into();
    let exec_msg_id = exec.header.msg_id.clone();
    handle.commands.send(KernelCommand::Shell(exec)).await?;

    let mut got_result = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let event = tokio::time::timeout_at(deadline, events.recv())
            .await
            .context("timed out waiting for kernel events")?
            .context("kernel event channel closed")?;
        match event {
            KernelEvent::IoPub(msg) => {
                let parent = msg.parent_header.as_ref().map(|h| h.msg_id.as_str());
                match &msg.content {
                    JupyterMessageContent::Status(s) => {
                        println!("iopub status: {}", s.execution_state.as_str());
                        if got_result
                            && parent == Some(exec_msg_id.as_str())
                            && s.execution_state == ExecutionState::Idle
                        {
                            break;
                        }
                    }
                    JupyterMessageContent::ExecuteResult(r) => {
                        println!("execute_result: {:?}", r.data.richest(|_| 1));
                        got_result = true;
                    }
                    JupyterMessageContent::StreamContent(s) => {
                        println!("stream [{:?}]: {}", s.name, s.text);
                    }
                    JupyterMessageContent::ErrorOutput(e) => {
                        println!("error: {}: {}", e.ename, e.evalue);
                    }
                    other => println!("iopub: {}", other.message_type()),
                }
            }
            KernelEvent::ShellReply(msg) => {
                println!("shell reply: {}", msg.content.message_type());
            }
            KernelEvent::Exited(code) => {
                anyhow::bail!("kernel exited prematurely (code {code:?})");
            }
        }
    }

    println!("\nShutting down kernel...");
    handle.commands.send(KernelCommand::Shutdown).await?;
    while let Some(event) = events.recv().await {
        if let KernelEvent::Exited(code) = event {
            println!("kernel exited (code {code:?})");
            break;
        }
    }
    println!("headless test passed");
    Ok(())
}
