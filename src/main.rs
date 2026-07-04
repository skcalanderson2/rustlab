mod kernel;

use anyhow::Result;

fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--headless-test") {
        let rt = tokio::runtime::Runtime::new()?;
        return rt.block_on(kernel::headless_test());
    }
    println!("GUI not yet implemented; run with --headless-test");
    Ok(())
}
