pub use jupyter_zmq_client::kernelspec::KernelspecDir;

/// All installed kernelspecs, including ones only visible via `jupyter --paths`
/// (e.g. conda/virtualenv installs).
pub async fn list_kernelspecs() -> Vec<KernelspecDir> {
    let mut specs = jupyter_zmq_client::kernelspec::list_kernelspecs_with_jupyter_paths().await;
    specs.sort_by(|a, b| a.kernel_name.cmp(&b.kernel_name));
    specs.dedup_by(|a, b| a.kernel_name == b.kernel_name);
    specs
}
