use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use colored::Colorize;
use directories::ProjectDirs;

use crate::commands::ExitStatus;

mod commands;
mod logging;

#[derive(Parser)]
#[command(author, version, about)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Do not print any output.
    #[arg(global = true, long, short, conflicts_with = "verbose")]
    quiet: bool,

    /// Use verbose output.
    #[arg(global = true, long, short, conflicts_with = "quiet")]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a `requirements.in` file to a `requirements.txt` file.
    Compile(CompileArgs),
    /// Sync dependencies from a `requirements.txt` file.
    Sync(SyncArgs),
    /// Clear the cache.
    Clean,
    /// Enumerate the installed packages in the current environment.
    Freeze(FreezeArgs),
}

#[derive(Args)]
struct CompileArgs {
    /// Path to the `requirements.txt` file to compile.
    src: PathBuf,

    /// Avoid reading from or writing to the cache.
    #[arg(long)]
    no_cache: bool,
}

#[derive(Args)]
struct SyncArgs {
    /// Path to the `requirements.txt` file to install.
    src: PathBuf,

    /// Avoid reading from or writing to the cache.
    #[arg(long)]
    no_cache: bool,

    /// Ignore any installed packages, forcing a re-installation.
    #[arg(long)]
    ignore_installed: bool,
}

#[derive(Args)]
struct FreezeArgs {
    /// Avoid reading from or writing to the cache.
    #[arg(long)]
    no_cache: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    logging::setup_logging(if cli.quiet {
        logging::Level::Quiet
    } else if cli.verbose {
        logging::Level::Verbose
    } else {
        logging::Level::Default
    });

    let dirs = ProjectDirs::from("", "", "puffin");

    let result = match &cli.command {
        Commands::Compile(args) => {
            commands::compile(
                &args.src,
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !args.no_cache),
            )
            .await
        }
        Commands::Sync(args) => {
            commands::sync(
                &args.src,
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !args.no_cache),
                if args.ignore_installed {
                    commands::SyncFlags::IGNORE_INSTALLED
                } else {
                    commands::SyncFlags::empty()
                },
            )
            .await
        }
        Commands::Clean => commands::clean(dirs.as_ref().map(ProjectDirs::cache_dir)).await,
        Commands::Freeze(args) => {
            commands::freeze(
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !args.no_cache),
            )
            .await
        }
    };

    match result {
        Ok(code) => code.into(),
        Err(err) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("{}: {}", "error".red().bold(), err);
            }
            ExitStatus::Error.into()
        }
    }
}
