use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use colored::Colorize;
use directories::ProjectDirs;

use crate::commands::ExitStatus;

mod commands;
mod logging;
mod printer;

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
    /// Uninstall a package.
    Uninstall(UninstallArgs),
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

#[derive(Args)]
struct UninstallArgs {
    /// The name of the package to uninstall.
    name: String,

    /// Avoid reading from or writing to the cache.
    #[arg(long)]
    no_cache: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    logging::setup_logging(if cli.verbose {
        logging::Level::Verbose
    } else {
        logging::Level::Default
    });

    let printer = if cli.quiet {
        printer::Printer::Quiet
    } else if cli.verbose {
        printer::Printer::Verbose
    } else {
        printer::Printer::Default
    };

    let dirs = ProjectDirs::from("", "", "puffin");

    let result = match &cli.command {
        Commands::Compile(args) => {
            commands::compile(
                &args.src,
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !args.no_cache),
                printer,
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
                printer,
            )
            .await
        }
        Commands::Clean => {
            commands::clean(dirs.as_ref().map(ProjectDirs::cache_dir), printer).await
        }
        Commands::Freeze(args) => {
            commands::freeze(
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !args.no_cache),
                printer,
            )
            .await
        }
        Commands::Uninstall(args) => {
            commands::uninstall(
                &args.name,
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !args.no_cache),
                printer,
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
