mod app;
mod kernel;
mod notebook;
mod output;

use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--headless-test") {
        let rt = tokio::runtime::Runtime::new()?;
        return rt.block_on(kernel::headless_test());
    }
    let path = args.first().map(std::path::PathBuf::from);
    app::run(path)?;
    Ok(())
}
