use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use colored::Colorize;
use directories::ProjectDirs;
use tempfile::tempdir;
use url::Url;

use puffin_normalize::{ExtraName, PackageName};
use puffin_resolver::{PreReleaseMode, ResolutionMode};
use requirements::ExtrasSpecification;

use crate::commands::{extra_name_with_clap_error, ExitStatus};
use crate::index_urls::IndexUrls;
use crate::python_version::PythonVersion;
use crate::requirements::RequirementsSource;

#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "openbsd"),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64"
    )
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod commands;
mod index_urls;
mod logging;
mod printer;
mod python_version;
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
#[allow(clippy::struct_excessive_bools)]
struct PipCompileArgs {
    /// Include all packages listed in the given `requirements.in` files.
    #[clap(required(true))]
    src_file: Vec<PathBuf>,

    /// Constrain versions using the given constraints files.
    #[clap(short, long)]
    constraint: Vec<PathBuf>,

    /// Include optional dependencies in the given extra group name; may be provided more than once.
    #[clap(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    extra: Vec<ExtraName>,

    /// Include all optional dependencies.
    #[clap(long, conflicts_with = "extra")]
    all_extras: bool,

    #[clap(long, value_enum)]
    resolution: Option<ResolutionMode>,

    #[clap(long, value_enum)]
    prerelease: Option<PreReleaseMode>,

    /// Write the compiled requirements to the given `requirements.txt` file.
    #[clap(short, long)]
    output_file: Option<PathBuf>,

    /// The URL of the Python Package Index (default: <https://pypi.org/simple>).
    #[clap(long, short)]
    index_url: Option<Url>,

    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    #[clap(long)]
    extra_index_url: Vec<Url>,

    /// Ignore the package index, instead relying on local archives and caches.
    #[clap(long, conflicts_with = "index_url", conflicts_with = "extra_index_url")]
    no_index: bool,

    /// Allow package upgrades, ignoring pinned versions in the existing output file.
    #[clap(long)]
    upgrade: bool,

    /// Don't build source distributions. This means resolving will not run arbitrary code. The
    /// cached wheels of already built source distributions will be reused.
    #[clap(long)]
    no_build: bool,

    /// The minimum Python version that should be supported.
    #[arg(long, short, value_enum)]
    python_version: Option<PythonVersion>,
}

#[derive(Args)]
struct PipSyncArgs {
    /// Include all packages listed in the given `requirements.txt` files.
    #[clap(required(true))]
    src_file: Vec<PathBuf>,

    /// The method to use when installing packages from the global cache.
    #[clap(long, value_enum)]
    link_mode: Option<install_wheel_rs::linker::LinkMode>,

    /// The URL of the Python Package Index (default: <https://pypi.org/simple>).
    #[clap(long, short)]
    index_url: Option<Url>,

    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    #[clap(long)]
    extra_index_url: Vec<Url>,

    /// Ignore the package index, instead relying on local archives and caches.
    #[clap(long, conflicts_with = "index_url", conflicts_with = "extra_index_url")]
    no_index: bool,

    /// Don't build source distributions. This means resolving will not run arbitrary code. The
    /// cached wheels of already built source distributions will be reused.
    #[clap(long)]
    no_build: bool,
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
    #[clap(default_value = ".venv")]
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
    name: PackageName,
}

async fn inner() -> Result<ExitStatus> {
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

    // Prefer, in order:
    // 1. A temporary cache directory, if the user requested `--no-cache`.
    // 2. The specific cache directory specified by the user via `--cache-dir` or `PUFFIN_CACHE_DIR`.
    // 3. The system-appropriate cache directory.
    // 4. A `.puffin_cache` directory in the current working directory.
    let project_dirs = ProjectDirs::from("", "", "puffin");
    let cache_dir = if cli.no_cache {
        Cow::Owned(tempdir()?.into_path())
    } else if let Some(cache_dir) = cli.cache_dir {
        Cow::Owned(cache_dir)
    } else if let Some(project_dirs) = project_dirs.as_ref() {
        Cow::Borrowed(project_dirs.cache_dir())
    } else {
        Cow::Borrowed(Path::new(".puffin_cache"))
    };

    match cli.command {
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
            let index_urls =
                IndexUrls::from_args(args.index_url, args.extra_index_url, args.no_index);

            let extras = if args.all_extras {
                ExtrasSpecification::All
            } else if args.extra.is_empty() {
                ExtrasSpecification::None
            } else {
                ExtrasSpecification::Some(&args.extra)
            };

            commands::pip_compile(
                &requirements,
                &constraints,
                extras,
                args.output_file.as_deref(),
                args.resolution.unwrap_or_default(),
                args.prerelease.unwrap_or_default(),
                args.upgrade.into(),
                index_urls,
                args.no_build,
                args.python_version,
                &cache_dir,
                printer,
            )
            .await
        }
        Commands::PipSync(args) => {
            let index_urls =
                IndexUrls::from_args(args.index_url, args.extra_index_url, args.no_index);
            let sources = args
                .src_file
                .into_iter()
                .map(RequirementsSource::from)
                .collect::<Vec<_>>();
            commands::pip_sync(
                &sources,
                args.link_mode.unwrap_or_default(),
                index_urls,
                args.no_build,
                &cache_dir,
                printer,
            )
            .await
        }
        Commands::PipUninstall(args) => {
            let sources = args
                .package
                .into_iter()
                .map(RequirementsSource::from)
                .chain(args.requirement.into_iter().map(RequirementsSource::from))
                .collect::<Vec<_>>();
            commands::pip_uninstall(&sources, &cache_dir, printer).await
        }
        Commands::Clean => commands::clean(&cache_dir, printer),
        Commands::Freeze => commands::freeze(&cache_dir, printer),
        Commands::Venv(args) => commands::venv(&args.name, args.python.as_deref(), printer),
        Commands::Add(args) => commands::add(&args.name, printer),
        Commands::Remove(args) => commands::remove(&args.name, printer),
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    match inner().await {
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
