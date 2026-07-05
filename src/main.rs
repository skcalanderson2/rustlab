// GUI app: no console window on Windows. `--headless-test` reattaches to the
// parent console below so its output still reaches an interactive terminal.
#![windows_subsystem = "windows"]

mod app;
mod kernel;
mod notebook;
mod output;
mod ui;

use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--headless-test") {
        #[cfg(windows)]
        unsafe {
            // With windows_subsystem = "windows" there is no console; attach
            // to the parent's so println! works when run from a terminal.
            // Piped/redirected output works without this. Failure is fine
            // (e.g. launched from Explorer) — output just goes nowhere.
            let _ = windows_sys::Win32::System::Console::AttachConsole(
                windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
            );
        }
        let rt = tokio::runtime::Runtime::new()?;
        return rt.block_on(kernel::headless_test());
    }
    let path = args.first().map(std::path::PathBuf::from);
    app::run(path)?;
    Ok(())
}
