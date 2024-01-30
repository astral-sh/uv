use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use anstream::eprintln;
use anyhow::Result;
use chrono::{DateTime, Days, NaiveDate, NaiveTime, Utc};
use clap::{Args, Parser, Subcommand};
use owo_colors::OwoColorize;
use tracing::instrument;

use distribution_types::{FlatIndexLocation, IndexLocations, IndexUrl};
use puffin_cache::{Cache, CacheArgs, Refresh};
use puffin_installer::{NoBinary, Reinstall};
use puffin_interpreter::PythonVersion;
use puffin_normalize::{ExtraName, PackageName};
use puffin_resolver::{DependencyMode, PreReleaseMode, ResolutionMode};
use puffin_traits::SetupPyStrategy;
use requirements::ExtrasSpecification;

use crate::commands::{extra_name_with_clap_error, ExitStatus, Upgrade};
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
mod compat;
mod confirm;
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

    /// Disable colors; provided for compatibility with `pip`.
    #[arg(global = true, long, hide = true, conflicts_with = "color")]
    no_color: bool,

    /// Control colors in output.
    #[arg(
        global = true,
        long,
        value_enum,
        default_value = "auto",
        conflicts_with = "no_color"
    )]
    color: ColorChoice,

    #[command(flatten)]
    cache_args: CacheArgs,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum ColorChoice {
    /// Enables colored output only when the output is going to a terminal or TTY with support.
    Auto,

    /// Enables colored output regardless of the detected environment.
    Always,

    /// Disables colored output.
    Never,
}

impl From<ColorChoice> for anstream::ColorChoice {
    fn from(value: ColorChoice) -> Self {
        match value {
            ColorChoice::Auto => anstream::ColorChoice::Auto,
            ColorChoice::Always => anstream::ColorChoice::Always,
            ColorChoice::Never => anstream::ColorChoice::Never,
        }
    }
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Resolve and install Python packages.
    Pip(PipArgs),
    /// Create a virtual environment.
    #[clap(alias = "virtualenv", alias = "v")]
    Venv(VenvArgs),
    /// Clear the cache.
    Clean(CleanArgs),
    /// Add a dependency to the workspace.
    #[clap(hide = true)]
    Add(AddArgs),
    /// Remove a dependency from the workspace.
    #[clap(hide = true)]
    Remove(RemoveArgs),
}

#[derive(Args)]
struct PipArgs {
    #[clap(subcommand)]
    command: PipCommand,
}

#[derive(Subcommand)]
enum PipCommand {
    /// Compile a `requirements.in` file to a `requirements.txt` file.
    Compile(PipCompileArgs),
    /// Sync dependencies from a `requirements.txt` file.
    Sync(PipSyncArgs),
    /// Install packages into the current environment.
    Install(PipInstallArgs),
    /// Uninstall packages from the current environment.
    Uninstall(PipUninstallArgs),
    /// Enumerate the installed packages in the current environment.
    Freeze(PipFreezeArgs),
}

/// Clap parser for the union of date and datetime
fn date_or_datetime(input: &str) -> Result<DateTime<Utc>, String> {
    let date_err = match NaiveDate::from_str(input) {
        Ok(date) => {
            // Midnight that day is 00:00:00 the next day
            return Ok((date + Days::new(1)).and_time(NaiveTime::MIN).and_utc());
        }
        Err(err) => err,
    };
    let datetime_err = match DateTime::parse_from_rfc3339(input) {
        Ok(datetime) => return Ok(datetime.with_timezone(&Utc)),
        Err(err) => err,
    };
    Err(format!(
        "Neither a valid date ({date_err}) not a valid datetime ({datetime_err})"
    ))
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct PipCompileArgs {
    /// Include all packages listed in the given `requirements.in` files.
    #[clap(required(true))]
    src_file: Vec<PathBuf>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[clap(short, long)]
    constraint: Vec<PathBuf>,

    /// Override versions using the given requirements files.
    ///
    /// Overrides files are `requirements.txt`-like files that force a specific version of a
    /// requirement to be installed, regardless of the requirements declared by any constituent
    /// package, and regardless of whether this would be considered an invalid resolution.
    ///
    /// While constraints are _additive_, in that they're combined with the requirements of the
    /// constituent packages, overrides are _absolute_, in that they completely replace the
    /// requirements of the constituent packages.
    #[clap(long)]
    r#override: Vec<PathBuf>,

    /// Include optional dependencies in the given extra group name; may be provided more than once.
    #[clap(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    extra: Vec<ExtraName>,

    /// Include all optional dependencies.
    #[clap(long, conflicts_with = "extra")]
    all_extras: bool,

    #[clap(long, value_enum, default_value_t = ResolutionMode::default())]
    resolution: ResolutionMode,

    #[clap(long, value_enum, default_value_t = PreReleaseMode::default())]
    prerelease: PreReleaseMode,

    /// Write the compiled requirements to the given `requirements.txt` file.
    #[clap(short, long)]
    output_file: Option<PathBuf>,

    /// Exclude comment annotations indicating the source of each package.
    #[clap(long)]
    no_annotate: bool,

    /// Exclude the comment header at the top of the generated output file.
    #[clap(long)]
    no_header: bool,

    /// Refresh all cached data.
    #[clap(long)]
    refresh: bool,

    /// Refresh cached data for a specific package.
    #[clap(long)]
    refresh_package: Vec<PackageName>,

    /// The URL of the Python Package Index.
    #[clap(long, short, default_value = IndexUrl::Pypi.as_str(), env = "PUFFIN_INDEX_URL")]
    index_url: IndexUrl,

    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    #[clap(long)]
    extra_index_url: Vec<IndexUrl>,

    /// Ignore the package index, instead relying on local archives and caches.
    #[clap(long, conflicts_with = "index_url", conflicts_with = "extra_index_url")]
    no_index: bool,

    /// Locations to search for candidate distributions, beyond those found in the indexes.
    ///
    /// If a path, the target must be a directory that contains package as wheel files (`.whl`) or
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files.
    #[clap(long)]
    find_links: Vec<FlatIndexLocation>,

    /// Allow package upgrades, ignoring pinned versions in the existing output file.
    #[clap(long)]
    upgrade: bool,

    /// Allow upgrades for a specific package, ignoring pinned versions in the existing output
    /// file.
    #[clap(long)]
    upgrade_package: Vec<PackageName>,

    /// Include distribution hashes in the output file.
    #[clap(long)]
    generate_hashes: bool,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[clap(long)]
    legacy_setup_py: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary code. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    #[clap(long)]
    no_build: bool,

    /// The minimum Python version that should be supported by the compiled requirements (e.g.,
    /// `3.7` or `3.7.9`).
    ///
    /// If a patch version is omitted, the most recent known patch version for that minor version
    /// is assumed. For example, `3.7` is mapped to `3.7.17`.
    #[arg(long, short)]
    python_version: Option<PythonVersion>,

    /// Try to resolve at a past time.
    ///
    /// This works by filtering out files with a more recent upload time, so if the index you use
    /// does not provide upload times, the results might be inaccurate. pypi provides upload times
    /// for all files.
    ///
    /// Timestamps are given either as RFC 3339 timestamps such as `2006-12-02T02:07:43Z` or as
    /// UTC dates in the same format such as `2006-12-02`. Dates are interpreted as including this
    /// day, i.e. until midnight UTC that day.
    #[arg(long, value_parser = date_or_datetime)]
    exclude_newer: Option<DateTime<Utc>>,

    /// Include `--index-url` and `--extra-index-url` entries in the generated output file.
    #[clap(long, hide = true)]
    emit_index_url: bool,

    /// Include `--find-links` entries in the generated output file.
    #[clap(long, hide = true)]
    emit_find_links: bool,

    #[command(flatten)]
    compat_args: compat::PipCompileCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct PipSyncArgs {
    /// Include all packages listed in the given `requirements.txt` files.
    #[clap(required(true))]
    src_file: Vec<PathBuf>,

    /// Reinstall all packages, regardless of whether they're already installed.
    #[clap(long, alias = "force-reinstall")]
    reinstall: bool,

    /// Reinstall a specific package, regardless of whether it's already installed.
    #[clap(long)]
    reinstall_package: Vec<PackageName>,

    /// Refresh all cached data.
    #[clap(long)]
    refresh: bool,

    /// Refresh cached data for a specific package.
    #[clap(long)]
    refresh_package: Vec<PackageName>,

    /// The method to use when installing packages from the global cache.
    #[clap(long, value_enum, default_value_t = install_wheel_rs::linker::LinkMode::default())]
    link_mode: install_wheel_rs::linker::LinkMode,

    /// The URL of the Python Package Index.
    #[clap(long, short, default_value = IndexUrl::Pypi.as_str(), env = "PUFFIN_INDEX_URL")]
    index_url: IndexUrl,

    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    #[clap(long)]
    extra_index_url: Vec<IndexUrl>,

    /// Locations to search for candidate distributions, beyond those found in the indexes.
    ///
    /// If a path, the target must be a directory that contains package as wheel files (`.whl`) or
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files.
    #[clap(long)]
    find_links: Vec<FlatIndexLocation>,

    /// Ignore the registry index (e.g., PyPI), instead relying on local caches and `--find-links`
    /// directories and URLs.
    #[clap(long, conflicts_with = "index_url", conflicts_with = "extra_index_url")]
    no_index: bool,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[clap(long)]
    legacy_setup_py: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary code. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    #[clap(long)]
    no_build: bool,

    /// Don't install pre-built wheels.
    ///
    /// When enabled, all installed packages will be installed from a source distribution. The resolver
    /// will still use pre-built wheels for metadata.
    #[clap(long)]
    no_binary: bool,

    /// Don't install pre-built wheels for a specific package.
    ///
    /// When enabled, the specified packages will be installed from a source distribution. The resolver
    /// will still use pre-built wheels for metadata.
    #[clap(long)]
    no_binary_package: Vec<PackageName>,

    /// Validate the virtual environment after completing the installation, to detect packages with
    /// missing dependencies or other issues.
    #[clap(long)]
    strict: bool,

    #[command(flatten)]
    compat_args: compat::PipSyncCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
struct PipInstallArgs {
    /// Install all listed packages.
    #[clap(group = "sources")]
    package: Vec<String>,

    /// Install all packages listed in the given requirements files.
    #[clap(short, long, group = "sources")]
    requirement: Vec<PathBuf>,

    /// Install the editable package based on the provided local file path.
    #[clap(short, long, group = "sources")]
    editable: Vec<String>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[clap(short, long)]
    constraint: Vec<PathBuf>,

    /// Override versions using the given requirements files.
    ///
    /// Overrides files are `requirements.txt`-like files that force a specific version of a
    /// requirement to be installed, regardless of the requirements declared by any constituent
    /// package, and regardless of whether this would be considered an invalid resolution.
    ///
    /// While constraints are _additive_, in that they're combined with the requirements of the
    /// constituent packages, overrides are _absolute_, in that they completely replace the
    /// requirements of the constituent packages.
    #[clap(long)]
    r#override: Vec<PathBuf>,

    /// Include optional dependencies in the given extra group name; may be provided more than once.
    #[clap(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    extra: Vec<ExtraName>,

    /// Include all optional dependencies.
    #[clap(long, conflicts_with = "extra")]
    all_extras: bool,

    /// Reinstall all packages, regardless of whether they're already installed.
    #[clap(long, alias = "force-reinstall")]
    reinstall: bool,

    /// Reinstall a specific package, regardless of whether it's already installed.
    #[clap(long)]
    reinstall_package: Vec<PackageName>,

    /// Refresh all cached data.
    #[clap(long)]
    refresh: bool,

    /// Refresh cached data for a specific package.
    #[clap(long)]
    refresh_package: Vec<PackageName>,

    /// Ignore package dependencies, instead only installing those packages explicitly listed
    /// on the command line or in the requirements files.
    #[clap(long)]
    no_deps: bool,

    /// The method to use when installing packages from the global cache.
    #[clap(long, value_enum, default_value_t = install_wheel_rs::linker::LinkMode::default())]
    link_mode: install_wheel_rs::linker::LinkMode,

    #[clap(long, value_enum, default_value_t = ResolutionMode::default())]
    resolution: ResolutionMode,

    #[clap(long, value_enum, default_value_t = PreReleaseMode::default())]
    prerelease: PreReleaseMode,

    /// Write the compiled requirements to the given `requirements.txt` file.
    #[clap(short, long)]
    output_file: Option<PathBuf>,

    /// The URL of the Python Package Index.
    #[clap(long, short, default_value = IndexUrl::Pypi.as_str(), env = "PUFFIN_INDEX_URL")]
    index_url: IndexUrl,

    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    #[clap(long)]
    extra_index_url: Vec<IndexUrl>,

    /// Locations to search for candidate distributions, beyond those found in the indexes.
    ///
    /// If a path, the target must be a directory that contains package as wheel files (`.whl`) or
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files.
    #[clap(long)]
    find_links: Vec<FlatIndexLocation>,

    /// Ignore the package index, instead relying on local archives and caches.
    #[clap(long, conflicts_with = "index_url", conflicts_with = "extra_index_url")]
    no_index: bool,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[clap(long)]
    legacy_setup_py: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary code. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    #[clap(long)]
    no_build: bool,

    /// Don't install pre-built wheels.
    ///
    /// When enabled, all installed packages will be installed from a source distribution. The resolver
    /// will still use pre-built wheels for metadata.
    #[clap(long)]
    no_binary: bool,

    /// Don't install pre-built wheels for a specific package.
    ///
    /// When enabled, the specified packages will be installed from a source distribution. The resolver
    /// will still use pre-built wheels for metadata.
    #[clap(long)]
    no_binary_package: Vec<PackageName>,

    /// Validate the virtual environment after completing the installation, to detect packages with
    /// missing dependencies or other issues.
    #[clap(long)]
    strict: bool,

    /// Try to resolve at a past time.
    ///
    /// This works by filtering out files with a more recent upload time, so if the index you use
    /// does not provide upload times, the results might be inaccurate. pypi provides upload times
    /// for all files.
    ///
    /// Timestamps are given either as RFC 3339 timestamps such as `2006-12-02T02:07:43Z` or as
    /// UTC dates in the same format such as `2006-12-02`. Dates are interpreted as including this
    /// day, i.e. until midnight UTC that day.
    #[arg(long, value_parser = date_or_datetime, hide = true)]
    exclude_newer: Option<DateTime<Utc>>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
struct PipUninstallArgs {
    /// Uninstall all listed packages.
    #[clap(group = "sources")]
    package: Vec<String>,

    /// Uninstall all packages listed in the given requirements files.
    #[clap(short, long, group = "sources")]
    requirement: Vec<PathBuf>,

    /// Uninstall the editable package based on the provided local file path.
    #[clap(short, long, group = "sources")]
    editable: Vec<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct PipFreezeArgs {
    /// Validate the virtual environment, to detect packages with missing dependencies or other
    /// issues.
    #[clap(long)]
    strict: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct CleanArgs {
    /// The packages to remove from the cache.
    package: Vec<PackageName>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct VenvArgs {
    /// The Python interpreter to use for the virtual environment.
    ///
    /// Supported formats:
    /// * `-p 3.10` searches for an installed Python 3.10 (`py --list-paths` on Windows, `python3.10` on Linux/Mac).
    ///   Specifying a patch version is not supported.
    /// * `-p python3.10` or `-p python.exe` looks for a binary in `PATH`.
    /// * `-p /home/ferris/.local/bin/python3.10` uses this exact Python.
    ///
    /// Note that this is different from `--python-version` in `pip compile`, which takes `3.10` or `3.10.13` and
    /// doesn't look for a Python interpreter on disk.
    // Short `-p` to match `virtualenv`
    #[clap(short, long)]
    python: Option<String>,

    /// Install seed packages (`pip`, `setuptools`, and `wheel`) into the virtual environment.
    #[clap(long)]
    seed: bool,

    /// The path to the virtual environment to create.
    #[clap(default_value = ".venv")]
    name: PathBuf,

    /// The URL of the Python Package Index.
    #[clap(long, short, default_value = IndexUrl::Pypi.as_str(), env = "PUFFIN_INDEX_URL")]
    index_url: IndexUrl,

    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    #[clap(long)]
    extra_index_url: Vec<IndexUrl>,

    /// Ignore the package index, instead relying on local archives and caches.
    #[clap(long, conflicts_with = "index_url", conflicts_with = "extra_index_url")]
    no_index: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct AddArgs {
    /// The name of the package to add (e.g., `Django==4.2.6`).
    name: String,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct RemoveArgs {
    /// The name of the package to remove (e.g., `Django`).
    name: PackageName,
}

#[instrument] // Anchor span to check for overhead
async fn run() -> Result<ExitStatus> {
    let cli = Cli::parse();

    // Configure the `tracing` crate, which controls internal logging.
    #[cfg(feature = "tracing-durations-export")]
    let (duration_layer, _duration_guard) = logging::setup_duration();
    #[cfg(not(feature = "tracing-durations-export"))]
    let duration_layer = None::<tracing_subscriber::layer::Identity>;
    logging::setup_logging(
        if cli.verbose {
            logging::Level::Verbose
        } else {
            logging::Level::Default
        },
        duration_layer,
    );

    // Configure the `Printer`, which controls user-facing output in the CLI.
    let printer = if cli.quiet {
        printer::Printer::Quiet
    } else if cli.verbose {
        printer::Printer::Verbose
    } else {
        printer::Printer::Default
    };

    // Configure the `warn!` macros, which control user-facing warnings in the CLI.
    if !cli.quiet {
        puffin_warnings::enable();
    }

    if cli.no_color {
        anstream::ColorChoice::write_global(anstream::ColorChoice::Never);
    } else {
        anstream::ColorChoice::write_global(cli.color.into());
    }

    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .break_words(false)
                .word_separator(textwrap::WordSeparator::AsciiSpace)
                .word_splitter(textwrap::WordSplitter::NoHyphenation)
                .wrap_lines(env::var("PUFFIN_NO_WRAP").map(|_| false).unwrap_or(true))
                .build(),
        )
    }))?;

    let cache = Cache::try_from(cli.cache_args)?;

    match cli.command {
        Commands::Pip(PipArgs {
            command: PipCommand::Compile(args),
        }) => {
            args.compat_args.validate()?;

            let cache = cache.with_refresh(Refresh::from_args(args.refresh, args.refresh_package));
            let requirements = args
                .src_file
                .into_iter()
                .map(RequirementsSource::from_path)
                .collect::<Vec<_>>();
            let constraints = args
                .constraint
                .into_iter()
                .map(RequirementsSource::from_path)
                .collect::<Vec<_>>();
            let overrides = args
                .r#override
                .into_iter()
                .map(RequirementsSource::from_path)
                .collect::<Vec<_>>();
            let index_urls = IndexLocations::from_args(
                args.index_url,
                args.extra_index_url,
                args.find_links,
                args.no_index,
            );
            let extras = if args.all_extras {
                ExtrasSpecification::All
            } else if args.extra.is_empty() {
                ExtrasSpecification::None
            } else {
                ExtrasSpecification::Some(&args.extra)
            };
            let upgrade = Upgrade::from_args(args.upgrade, args.upgrade_package);
            commands::pip_compile(
                &requirements,
                &constraints,
                &overrides,
                extras,
                args.output_file.as_deref(),
                args.resolution,
                args.prerelease,
                upgrade,
                args.generate_hashes,
                !args.no_annotate,
                !args.no_header,
                args.emit_index_url,
                args.emit_find_links,
                index_urls,
                if args.legacy_setup_py {
                    SetupPyStrategy::Setuptools
                } else {
                    SetupPyStrategy::Pep517
                },
                args.no_build,
                args.python_version,
                args.exclude_newer,
                cache,
                printer,
            )
            .await
        }
        Commands::Pip(PipArgs {
            command: PipCommand::Sync(args),
        }) => {
            args.compat_args.validate()?;

            let cache = cache.with_refresh(Refresh::from_args(args.refresh, args.refresh_package));
            let index_urls = IndexLocations::from_args(
                args.index_url,
                args.extra_index_url,
                args.find_links,
                args.no_index,
            );
            let sources = args
                .src_file
                .into_iter()
                .map(RequirementsSource::from_path)
                .collect::<Vec<_>>();
            let reinstall = Reinstall::from_args(args.reinstall, args.reinstall_package);
            let no_binary = NoBinary::from_args(args.no_binary, args.no_binary_package);
            commands::pip_sync(
                &sources,
                &reinstall,
                args.link_mode,
                index_urls,
                if args.legacy_setup_py {
                    SetupPyStrategy::Setuptools
                } else {
                    SetupPyStrategy::Pep517
                },
                args.no_build,
                &no_binary,
                args.strict,
                cache,
                printer,
            )
            .await
        }
        Commands::Pip(PipArgs {
            command: PipCommand::Install(args),
        }) => {
            let cache = cache.with_refresh(Refresh::from_args(args.refresh, args.refresh_package));
            let requirements = args
                .package
                .into_iter()
                .map(RequirementsSource::from_package)
                .chain(args.editable.into_iter().map(RequirementsSource::Editable))
                .chain(
                    args.requirement
                        .into_iter()
                        .map(RequirementsSource::from_path),
                )
                .collect::<Vec<_>>();
            let constraints = args
                .constraint
                .into_iter()
                .map(RequirementsSource::from_path)
                .collect::<Vec<_>>();
            let overrides = args
                .r#override
                .into_iter()
                .map(RequirementsSource::from_path)
                .collect::<Vec<_>>();
            let index_urls = IndexLocations::from_args(
                args.index_url,
                args.extra_index_url,
                args.find_links,
                args.no_index,
            );
            let extras = if args.all_extras {
                ExtrasSpecification::All
            } else if args.extra.is_empty() {
                ExtrasSpecification::None
            } else {
                ExtrasSpecification::Some(&args.extra)
            };
            let reinstall = Reinstall::from_args(args.reinstall, args.reinstall_package);
            let no_binary = NoBinary::from_args(args.no_binary, args.no_binary_package);
            let dependency_mode = if args.no_deps {
                DependencyMode::Direct
            } else {
                DependencyMode::Transitive
            };
            commands::pip_install(
                &requirements,
                &constraints,
                &overrides,
                &extras,
                args.resolution,
                args.prerelease,
                dependency_mode,
                index_urls,
                &reinstall,
                args.link_mode,
                if args.legacy_setup_py {
                    SetupPyStrategy::Setuptools
                } else {
                    SetupPyStrategy::Pep517
                },
                args.no_build,
                &no_binary,
                args.strict,
                args.exclude_newer,
                cache,
                printer,
            )
            .await
        }
        Commands::Pip(PipArgs {
            command: PipCommand::Uninstall(args),
        }) => {
            let sources = args
                .package
                .into_iter()
                .map(RequirementsSource::from_package)
                .chain(args.editable.into_iter().map(RequirementsSource::Editable))
                .chain(
                    args.requirement
                        .into_iter()
                        .map(RequirementsSource::from_path),
                )
                .collect::<Vec<_>>();
            commands::pip_uninstall(&sources, cache, printer).await
        }
        Commands::Pip(PipArgs {
            command: PipCommand::Freeze(args),
        }) => commands::freeze(&cache, args.strict, printer),
        Commands::Clean(args) => commands::clean(&cache, &args.package, printer),
        Commands::Venv(args) => {
            let index_locations = IndexLocations::from_args(
                args.index_url,
                args.extra_index_url,
                // No find links for the venv subcommand, to keep things simple
                Vec::new(),
                args.no_index,
            );
            commands::venv(
                &args.name,
                args.python.as_deref(),
                &index_locations,
                args.seed,
                &cache,
                printer,
            )
            .await
        }
        Commands::Add(args) => commands::add(&args.name, printer),
        Commands::Remove(args) => commands::remove(&args.name, printer),
    }
}

fn main() -> ExitCode {
    let result = if let Ok(stack_size) = env::var("PUFFIN_STACK_SIZE") {
        // Artificially limit the stack size to test for stack overflows. Windows has a default stack size of 1MB,
        // which is lower than the linux and mac default.
        // https://learn.microsoft.com/en-us/cpp/build/reference/stack-stack-allocations?view=msvc-170
        let stack_size = stack_size.parse().expect("Invalid stack size");
        let tokio_main = move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_stack_size(stack_size)
                .build()
                .expect("Failed building the Runtime")
                .block_on(run())
        };
        std::thread::Builder::new()
            .stack_size(stack_size)
            .spawn(tokio_main)
            .expect("Tokio executor failed, was there a panic?")
            .join()
            .expect("Tokio executor failed, was there a panic?")
    } else {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed building the Runtime")
            .block_on(run())
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
