use std::io::Write;

pub(crate) fn run() -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    run_with_output(&mut stdout)
}

pub(crate) fn run_with_output<W: Write>(stdout: &mut W) -> Result<(), String> {
    writeln!(stdout, "v{}", env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?;
    Ok(())
}
