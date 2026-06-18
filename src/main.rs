mod ls;
fn main() -> anyhow::Result<()> {
    ls::main()?;
    println!();
    Ok(())
}
