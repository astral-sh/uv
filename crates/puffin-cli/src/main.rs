use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use directories::ProjectDirs;
use owo_colors::OwoColorize;

use crate::commands::ExitStatus;
use crate::requirements::RequirementsSource;

mod commands;
mod logging;
mod printer;
mod requirements;

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
    #[arg(global = true, long, short)]
    no_cache: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a `requirements.in` file to a `requirements.txt` file.
    PipCompile(PipCompileArgs),
    /// Sync dependencies from a `requirements.txt` file.
    PipSync(PipSyncArgs),
    /// Clear the cache.
    Clean,
    /// Enumerate the installed packages in the current environment.
    Freeze,
    /// Uninstall packages from the current environment.
    PipUninstall(PipUninstallArgs),
    /// Create a virtual environment.
    Venv(VenvArgs),
    /// Add a dependency to the workspace.
    Add(AddArgs),
    /// Remove a dependency from the workspace.
    Remove(RemoveArgs),
}

#[derive(Args)]
struct PipCompileArgs {
    /// Include all packages listed in the given `requirements.in` files.
    #[clap(required(true))]
    src_file: Vec<PathBuf>,

    /// Write the compiled requirements to the given `requirements.txt` file.
    #[clap(short, long)]
    output_file: Option<PathBuf>,
}

#[derive(Args)]
struct PipSyncArgs {
    /// Include all packages listed in the given `requirements.txt` files.
    #[clap(required(true))]
    src_file: Vec<PathBuf>,
}

#[derive(Args)]
#[command(group = clap::ArgGroup::new("sources").required(true))]
struct PipUninstallArgs {
    /// Uninstall all listed packages.
    #[clap(group = "sources")]
    package: Vec<String>,

    /// Uninstall all packages listed in the given requirements files.
    #[clap(short, long, group = "sources")]
    requirement: Vec<PathBuf>,
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

#[derive(Args)]
struct AddArgs {
    /// The name of the package to add (e.g., `Django==4.2.6`).
    name: String,
}

#[derive(Args)]
struct RemoveArgs {
    /// The name of the package to remove (e.g., `Django`).
    name: String,
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

    let result = match cli.command {
        Commands::PipCompile(args) => {
            let dirs = ProjectDirs::from("", "", "puffin");
            let sources = args
                .src_file
                .into_iter()
                .map(RequirementsSource::from)
                .collect::<Vec<_>>();
            commands::pip_compile(
                &sources,
                args.output_file.as_deref(),
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !cli.no_cache),
                printer,
            )
            .await
        }
        Commands::PipSync(args) => {
            let dirs = ProjectDirs::from("", "", "puffin");
            let sources = args
                .src_file
                .into_iter()
                .map(RequirementsSource::from)
                .collect::<Vec<_>>();
            commands::pip_sync(
                &sources,
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !cli.no_cache),
                printer,
            )
            .await
        }
        Commands::PipUninstall(args) => {
            let dirs = ProjectDirs::from("", "", "puffin");
            let sources = args
                .package
                .into_iter()
                .map(RequirementsSource::from)
                .chain(args.requirement.into_iter().map(RequirementsSource::from))
                .collect::<Vec<_>>();
            commands::pip_uninstall(
                &sources,
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !cli.no_cache),
                printer,
            )
            .await
        }
        Commands::Clean => {
            let dirs = ProjectDirs::from("", "", "puffin");
            commands::clean(dirs.as_ref().map(ProjectDirs::cache_dir), printer).await
        }
        Commands::Freeze => {
            let dirs = ProjectDirs::from("", "", "puffin");
            commands::freeze(
                dirs.as_ref()
                    .map(ProjectDirs::cache_dir)
                    .filter(|_| !cli.no_cache),
                printer,
            )
            .await
        }
        Commands::Venv(args) => commands::venv(&args.name, args.python.as_deref(), printer).await,
        Commands::Add(args) => commands::add(&args.name, printer),
        Commands::Remove(args) => commands::remove(&args.name, printer),
    };

    match result {
        Ok(code) => code.into(),
        Err(err) => {
            #[allow(clippy::print_stderr)]
            {
                let mut causes = err.chain();
                eprintln!("{}: {}", "error".red().bold(), causes.next().unwrap());
                for err in causes {
                    eprintln!("  {}: {}", "Caused by".red().bold(), err);
                }
            }
            ExitStatus::Error.into()
        }
    }
}
