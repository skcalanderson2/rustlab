fn main() {
    // Embed icon + version info into the Windows exe. winresource fills
    // version fields from Cargo.toml automatically.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("packaging/windows/RustLab.ico");
        res.set("ProductName", "RustLab");
        res.set("FileDescription", "RustLab — native Jupyter notebook app");
        if let Err(e) = res.compile() {
            println!("cargo:warning=failed to embed Windows resources: {e}");
        }
    }
}
