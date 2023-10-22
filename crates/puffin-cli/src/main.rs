use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use colored::Colorize;
use directories::ProjectDirs;
use puffin_resolver::ResolutionMode;

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

    /// Path to the cache directory.
    #[arg(global = true, long, env = "PUFFIN_CACHE_DIR")]
    cache_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a `requirements.in` file to a `requirements.txt` file.
    PipCompile(PipCompileArgs),
    /// Sync dependencies from a `requirements.txt` file.
    PipSync(PipSyncArgs),
    /// Uninstall packages from the current environment.
    PipUninstall(PipUninstallArgs),
    /// Clear the cache.
    Clean,
    /// Enumerate the installed packages in the current environment.
    Freeze,
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

    /// Constrain versions using the given constraints files.
    #[clap(short, long)]
    constraint: Vec<PathBuf>,

    #[clap(long, value_enum)]
    resolution: Option<ResolutionMode>,

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
    /// The Python interpreter to use for the virtual environment.
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

    let project_dirs = ProjectDirs::from("", "", "puffin");
    let cache_dir = (!cli.no_cache)
        .then(|| {
            cli.cache_dir
                .as_deref()
                .or_else(|| project_dirs.as_ref().map(ProjectDirs::cache_dir))
        })
        .flatten();

    let result = match cli.command {
        Commands::PipCompile(args) => {
            let requirements = args
                .src_file
                .into_iter()
                .map(RequirementsSource::from)
                .collect::<Vec<_>>();
            let constraints = args
                .constraint
                .into_iter()
                .map(RequirementsSource::from)
                .collect::<Vec<_>>();
            commands::pip_compile(
                &requirements,
                &constraints,
                args.output_file.as_deref(),
                args.resolution.unwrap_or_default(),
                cache_dir,
                printer,
            )
            .await
        }
        Commands::PipSync(args) => {
            let sources = args
                .src_file
                .into_iter()
                .map(RequirementsSource::from)
                .collect::<Vec<_>>();
            commands::pip_sync(&sources, cache_dir, printer).await
        }
        Commands::PipUninstall(args) => {
            let sources = args
                .package
                .into_iter()
                .map(RequirementsSource::from)
                .chain(args.requirement.into_iter().map(RequirementsSource::from))
                .collect::<Vec<_>>();
            commands::pip_uninstall(&sources, cache_dir, printer).await
        }
        Commands::Clean => commands::clean(cache_dir, printer).await,
        Commands::Freeze => commands::freeze(cache_dir, printer),
        Commands::Venv(args) => commands::venv(&args.name, args.python.as_deref(), printer),
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
