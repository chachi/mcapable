use std::io::Write;

use super::CliResult;

pub fn run(library: bool) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    run_with_output(&mut stdout, library)
}

pub fn run_with_output<W: Write>(stdout: &mut W, library: bool) -> Result<(), String> {
    if library {
        writeln!(stdout, "mcapable-core v{}", mcapable_core::VERSION).cli()?;
    } else {
        writeln!(stdout, "v{}", env!("CARGO_PKG_VERSION")).cli()?;
    }
    Ok(())
}
