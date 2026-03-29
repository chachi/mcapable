use clap::CommandFactory;
use clap_complete::Shell;
use std::io::Write;

use crate::cli::Cli;

pub(crate) fn run(shell: Shell) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    run_with_output(shell, &mut stdout)
}

pub(crate) fn run_with_output<W: Write>(shell: Shell, stdout: &mut W) -> Result<(), String> {
    let mut cmd = Cli::command();
    let bin = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, bin, stdout);
    Ok(())
}
