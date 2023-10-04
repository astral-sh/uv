use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use colored::Colorize;

use crate::commands::ExitStatus;

mod commands;

#[derive(Parser)]
#[command(author, version, about)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install dependencies from a `requirements.text` file.
    Install(InstallArgs),
}

#[derive(Args)]
struct InstallArgs {
    /// Path to the `requirements.text` file to install.
    src: PathBuf,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Install(install) => commands::install(&install.src),
    };

    match result {
        Ok(code) => code.into(),
        Err(err) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("{}", "puffin failed".red().bold());
                for cause in err.chain() {
                    eprintln!("  {} {cause}", "Cause:".bold());
                }
            }
            ExitStatus::Error.into()
        }
    }
}
