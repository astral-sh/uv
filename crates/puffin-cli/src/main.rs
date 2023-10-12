use std::path::PathBuf;
use std::process::ExitCode;

use crate::commands::ExitStatus;
use clap::{Args, Parser, Subcommand};
use colored::Colorize;
use directories::ProjectDirs;

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

    /// Avoid reading from or writing to the cache.
    #[arg(long)]
    no_cache: bool,
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
    Freeze,
    /// Uninstall a package.
    Uninstall(UninstallArgs),
    /// Create a virtual environment.
    Venv(VenvArgs),
}

#[derive(Args)]
struct CompileArgs {
    /// Output `requirements.txt` file
    #[clap(short, long)]
    output_file: Option<PathBuf>,
    /// Path to the `requirements.txt` file to compile.
    src: PathBuf,
}

#[derive(Args)]
struct SyncArgs {
    /// Path to the `requirements.txt` file to install.
    src: PathBuf,

    /// Ignore any installed packages, forcing a re-installation.
    #[arg(long)]
    ignore_installed: bool,
}

#[derive(Args)]
struct UninstallArgs {
    /// The name of the package to uninstall.
    name: String,
}

#[derive(Args)]
struct VenvArgs {
    /// The python interpreter to use for the virtual environment
    // Short `-p` to match `virtualenv`
    // TODO(konstin): Support e.g. `-p 3.10`
    #[clap(short, long)]
    python: Option<PathBuf>,
    /// The path to the virtual environment to create.
    name: PathBuf,
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
                args.output_file.as_deref(),
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !cli.no_cache),
                printer,
            )
            .await
        }
        Commands::Sync(args) => {
            commands::sync(
                &args.src,
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !cli.no_cache),
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
        Commands::Freeze => {
            commands::freeze(
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !cli.no_cache),
                printer,
            )
            .await
        }
        Commands::Uninstall(args) => {
            commands::uninstall(
                &args.name,
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !cli.no_cache),
                printer,
            )
            .await
        }
        Commands::Venv(args) => commands::venv(&args.name, args.python.as_deref(), printer).await,
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
