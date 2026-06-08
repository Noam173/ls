use anyhow::Result;
pub mod ls;
fn main() -> Result<()> {
    ls::main()?;
    Ok(())
}
