use std::env;
use std::io::stdout;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use anstream::eprintln;
use anyhow::Result;
use chrono::{DateTime, Days, NaiveDate, NaiveTime, Utc};
use clap::error::{ContextKind, ContextValue};
use clap::{Args, CommandFactory, Parser, Subcommand};
use owo_colors::OwoColorize;
use tracing::instrument;

use distribution_types::{FlatIndexLocation, IndexLocations, IndexUrl};
use requirements::ExtrasSpecification;
use uv_cache::{Cache, CacheArgs, Refresh};
use uv_client::Connectivity;
use uv_installer::{NoBinary, Reinstall};
use uv_interpreter::PythonVersion;
use uv_normalize::{ExtraName, PackageName};
use uv_resolver::{AnnotationStyle, DependencyMode, PreReleaseMode, ResolutionMode};
use uv_traits::{
    ConfigSettingEntry, ConfigSettings, NoBuild, PackageNameSpecifier, SetupPyStrategy,
};

use crate::commands::{extra_name_with_clap_error, ExitStatus, ListFormat, Upgrade, VersionFormat};
use crate::compat::CompatArgs;
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
mod shell;
mod version;

const DEFAULT_VENV_NAME: &str = ".venv";

#[derive(Parser)]
#[command(author, version, long_version = crate::version::version(), about)]
#[command(propagate_version = true)]
#[allow(clippy::struct_excessive_bools)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Do not print any output.
    #[arg(global = true, long, short, conflicts_with = "verbose")]
    quiet: bool,

    /// Use verbose output.
    ///
    /// You can configure fine-grained logging using the `RUST_LOG` environment variable.
    /// (<https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives>)
    #[arg(global = true, action = clap::ArgAction::Count, long, short, conflicts_with = "quiet")]
    verbose: u8,

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

    /// Whether to load TLS certificates from the platform's native certificate store.
    ///
    /// By default, `uv` loads certificates from the bundled `webpki-roots` crate. The
    /// `webpki-roots` are a reliable set of trust roots from Mozilla, and including them in `uv`
    /// improves portability and performance (especially on macOS).
    ///
    /// However, in some cases, you may want to use the platform's native certificate store,
    /// especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's
    /// included in your system's certificate store.
    #[arg(global = true, long)]
    native_tls: bool,

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
            ColorChoice::Auto => Self::Auto,
            ColorChoice::Always => Self::Always,
            ColorChoice::Never => Self::Never,
        }
    }
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Resolve and install Python packages.
    Pip(PipNamespace),
    /// Create a virtual environment.
    #[clap(alias = "virtualenv", alias = "v")]
    Venv(VenvArgs),
    /// Manage the cache.
    Cache(CacheNamespace),
    /// Remove all items from the cache.
    #[clap(hide = true)]
    Clean(CleanArgs),
    /// Display uv's version
    Version {
        #[arg(long, value_enum, default_value = "text")]
        output_format: VersionFormat,
    },
    /// Generate shell completion
    #[clap(alias = "--generate-shell-completion", hide = true)]
    GenerateShellCompletion { shell: clap_complete_command::Shell },
}

#[derive(Args)]
struct CacheNamespace {
    #[clap(subcommand)]
    command: CacheCommand,
}

#[derive(Subcommand)]
enum CacheCommand {
    /// Remove all items from the cache.
    Clean(CleanArgs),
    /// Show the cache directory.
    Dir,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct CleanArgs {
    /// The packages to remove from the cache.
    package: Vec<PackageName>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct DirArgs {
    /// The packages to remove from the cache.
    package: Vec<PackageName>,
}

#[derive(Args)]
struct PipNamespace {
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
    /// Enumerate the installed packages in the current environment.
    List(PipListArgs),
    /// Show information about one or more installed packages.
    Show(PipShowArgs),
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

/// A re-implementation of `Option`, used to avoid Clap's automatic `Option` flattening in
/// [`parse_index_url`].
#[derive(Debug, Clone)]
enum Maybe<T> {
    Some(T),
    None,
}

impl<T> Maybe<T> {
    fn into_option(self) -> Option<T> {
        match self {
            Maybe::Some(value) => Some(value),
            Maybe::None => None,
        }
    }
}

/// Parse a string into an [`IndexUrl`], mapping the empty string to `None`.
fn parse_index_url(input: &str) -> Result<Maybe<IndexUrl>, String> {
    if input.is_empty() {
        Ok(Maybe::None)
    } else {
        match IndexUrl::from_str(input) {
            Ok(url) => Ok(Maybe::Some(url)),
            Err(err) => Err(err.to_string()),
        }
    }
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct PipCompileArgs {
    /// Include all packages listed in the given `requirements.in` files.
    ///
    /// When the path is `-`, then requirements are read from stdin.
    #[clap(required(true))]
    src_file: Vec<PathBuf>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[clap(long, short)]
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

    /// Ignore package dependencies, instead only add those packages explicitly listed
    /// on the command line to the resulting the requirements file.
    #[clap(long)]
    no_deps: bool,

    #[clap(long, value_enum, default_value_t = ResolutionMode::default())]
    resolution: ResolutionMode,

    #[clap(long, value_enum, default_value_t = PreReleaseMode::default(), conflicts_with = "pre", env = "UV_PRERELEASE")]
    prerelease: PreReleaseMode,

    #[clap(long, hide = true, conflicts_with = "prerelease")]
    pre: bool,

    /// Write the compiled requirements to the given `requirements.txt` file.
    #[clap(long, short)]
    output_file: Option<PathBuf>,

    /// Exclude comment annotations indicating the source of each package.
    #[clap(long)]
    no_annotate: bool,

    /// Exclude the comment header at the top of the generated output file.
    #[clap(long)]
    no_header: bool,

    /// Run offline, i.e., without accessing the network.
    #[arg(
        global = true,
        long,
        conflicts_with = "refresh",
        conflicts_with = "refresh_package"
    )]
    offline: bool,

    /// Refresh all cached data.
    #[clap(long)]
    refresh: bool,

    /// Refresh cached data for a specific package.
    #[clap(long)]
    refresh_package: Vec<PackageName>,

    /// The URL of the Python package index (by default: <https://pypi.org/simple>).
    ///
    /// The index given by this flag is given lower priority than all other
    /// indexes specified via the `--extra-index-url` flag.
    ///
    /// Unlike `pip`, `uv` will stop looking for versions of a package as soon
    /// as it finds it in an index. That is, it isn't possible for `uv` to
    /// consider versions of the same package across multiple indexes.
    #[clap(long, short, env = "UV_INDEX_URL", value_parser = parse_index_url)]
    index_url: Option<Maybe<IndexUrl>>,

    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    ///
    /// All indexes given via this flag take priority over the index
    /// in `--index-url` (which defaults to PyPI). And when multiple
    /// `--extra-index-url` flags are given, earlier values take priority.
    ///
    /// Unlike `pip`, `uv` will stop looking for versions of a package as soon
    /// as it finds it in an index. That is, it isn't possible for `uv` to
    /// consider versions of the same package across multiple indexes.
    #[clap(long, env = "UV_EXTRA_INDEX_URL", value_delimiter = ' ', value_parser = parse_index_url)]
    extra_index_url: Vec<Maybe<IndexUrl>>,

    /// Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those
    /// discovered via `--find-links`.
    #[clap(long, conflicts_with = "index_url", conflicts_with = "extra_index_url")]
    no_index: bool,

    /// Locations to search for candidate distributions, beyond those found in the indexes.
    ///
    /// If a path, the target must be a directory that contains package as wheel files (`.whl`) or
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files.
    #[clap(long, short)]
    find_links: Vec<FlatIndexLocation>,

    /// Allow package upgrades, ignoring pinned versions in the existing output file.
    #[clap(long, short = 'U')]
    upgrade: bool,

    /// Allow upgrades for a specific package, ignoring pinned versions in the existing output
    /// file.
    #[clap(long, short = 'P')]
    upgrade_package: Vec<PackageName>,

    /// Include distribution hashes in the output file.
    #[clap(long)]
    generate_hashes: bool,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[clap(long)]
    legacy_setup_py: bool,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[clap(long)]
    no_build_isolation: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary code. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Alias for `--only-binary :all:`.
    #[clap(long, conflicts_with = "only_binary")]
    no_build: bool,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[clap(long, conflicts_with = "no_build")]
    only_binary: Vec<PackageNameSpecifier>,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[clap(long, short = 'C', alias = "config-settings")]
    config_setting: Vec<ConfigSettingEntry>,

    /// The minimum Python version that should be supported by the compiled requirements (e.g.,
    /// `3.7` or `3.7.9`).
    ///
    /// If a patch version is omitted, the most recent known patch version for that minor version
    /// is assumed. For example, `3.7` is mapped to `3.7.17`.
    #[arg(long, short)]
    python_version: Option<PythonVersion>,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, value_parser = date_or_datetime)]
    exclude_newer: Option<DateTime<Utc>>,

    /// Specify a package to omit from the output resolution. Its dependencies will still be
    /// included in the resolution. Equivalent to pip-compile's `--unsafe-package` option.
    #[clap(long, alias = "unsafe-package")]
    no_emit_package: Vec<PackageName>,

    /// Include `--index-url` and `--extra-index-url` entries in the generated output file.
    #[clap(long, hide = true)]
    emit_index_url: bool,

    /// Include `--find-links` entries in the generated output file.
    #[clap(long, hide = true)]
    emit_find_links: bool,

    /// Choose the style of the annotation comments, which indicate the source of each package.
    #[clap(long, default_value_t=AnnotationStyle::Split, value_enum)]
    annotation_style: AnnotationStyle,

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

    /// Run offline, i.e., without accessing the network.
    #[arg(
        global = true,
        long,
        conflicts_with = "refresh",
        conflicts_with = "refresh_package"
    )]
    offline: bool,

    /// Refresh all cached data.
    #[clap(long)]
    refresh: bool,

    /// Refresh cached data for a specific package.
    #[clap(long)]
    refresh_package: Vec<PackageName>,

    /// The method to use when installing packages from the global cache.
    #[clap(long, value_enum, default_value_t = install_wheel_rs::linker::LinkMode::default())]
    link_mode: install_wheel_rs::linker::LinkMode,

    /// The URL of the Python package index (by default: <https://pypi.org/simple>).
    ///
    /// The index given by this flag is given lower priority than all other
    /// indexes specified via the `--extra-index-url` flag.
    ///
    /// Unlike `pip`, `uv` will stop looking for versions of a package as soon
    /// as it finds it in an index. That is, it isn't possible for `uv` to
    /// consider versions of the same package across multiple indexes.
    #[clap(long, short, env = "UV_INDEX_URL", value_parser = parse_index_url)]
    index_url: Option<Maybe<IndexUrl>>,

    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    ///
    /// All indexes given via this flag take priority over the index
    /// in `--index-url` (which defaults to PyPI). And when multiple
    /// `--extra-index-url` flags are given, earlier values take priority.
    ///
    /// Unlike `pip`, `uv` will stop looking for versions of a package as soon
    /// as it finds it in an index. That is, it isn't possible for `uv` to
    /// consider versions of the same package across multiple indexes.
    #[clap(long, env = "UV_EXTRA_INDEX_URL", value_delimiter = ' ', value_parser = parse_index_url)]
    extra_index_url: Vec<Maybe<IndexUrl>>,

    /// Locations to search for candidate distributions, beyond those found in the indexes.
    ///
    /// If a path, the target must be a directory that contains package as wheel files (`.whl`) or
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files.
    #[clap(long, short)]
    find_links: Vec<FlatIndexLocation>,

    /// Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those
    /// discovered via `--find-links`.
    #[clap(long, conflicts_with = "index_url", conflicts_with = "extra_index_url")]
    no_index: bool,

    /// The Python interpreter into which packages should be installed.
    ///
    /// By default, `uv` installs into the virtual environment in the current working directory or
    /// any parent directory. The `--python` option allows you to specify a different interpreter,
    /// which is intended for use in continuous integration (CI) environments or other automated
    /// workflows.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[clap(
        long,
        short,
        verbatim_doc_comment,
        conflicts_with = "system",
        group = "discovery"
    )]
    python: Option<String>,

    /// Install packages into the system Python.
    ///
    /// By default, `uv` installs into the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs `uv` to instead use the first Python
    /// found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[clap(
        long,
        conflicts_with = "python",
        env = "UV_SYSTEM_PYTHON",
        group = "discovery"
    )]
    system: bool,

    /// Allow `uv` to modify an `EXTERNALLY-MANAGED` Python installation.
    ///
    /// WARNING: `--break-system-packages` is intended for use in continuous integration (CI)
    /// environments, when installing into Python installations that are managed by an external
    /// package manager, like `apt`. It should be used with caution, as such Python installations
    /// explicitly recommend against modifications by other package managers (like `uv` or `pip`).
    #[clap(long, requires = "discovery")]
    break_system_packages: bool,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[clap(long)]
    legacy_setup_py: bool,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[clap(long)]
    no_build_isolation: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary code. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Alias for `--only-binary :all:`.
    #[clap(long, conflicts_with = "no_binary", conflicts_with = "only_binary")]
    no_build: bool,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be installed from a source distribution. The resolver
    /// will still use pre-built wheels for metadata.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[clap(long, conflicts_with = "no_build")]
    no_binary: Vec<PackageNameSpecifier>,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[clap(long, conflicts_with = "no_build")]
    only_binary: Vec<PackageNameSpecifier>,

    /// Compile Python files to bytecode.
    ///
    /// By default, does not compile Python (`.py`) files to bytecode (`__pycache__/*.pyc`), instead
    /// Python lazily does the compilation the first time a module is imported. In cases where the
    /// first start time matters, such as CLI applications and docker containers, this option can
    /// trade longer install time for faster startup.
    ///
    /// The compile option will process the entire site-packages directory for consistency and
    /// (like pip) ignore all errors.
    #[clap(long)]
    compile: bool,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[clap(long, short = 'C', alias = "config-settings")]
    config_setting: Vec<ConfigSettingEntry>,

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
    #[clap(long, short, group = "sources")]
    requirement: Vec<PathBuf>,

    /// Install the editable package based on the provided local file path.
    #[clap(long, short, group = "sources")]
    editable: Vec<String>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[clap(long, short)]
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

    /// Allow package upgrades.
    #[clap(long, short = 'U')]
    upgrade: bool,

    /// Allow upgrade of a specific package.
    #[clap(long, short = 'P')]
    upgrade_package: Vec<PackageName>,

    /// Reinstall all packages, regardless of whether they're already installed.
    #[clap(long, alias = "force-reinstall")]
    reinstall: bool,

    /// Reinstall a specific package, regardless of whether it's already installed.
    #[clap(long)]
    reinstall_package: Vec<PackageName>,

    /// Run offline, i.e., without accessing the network.
    #[arg(
        global = true,
        long,
        conflicts_with = "refresh",
        conflicts_with = "refresh_package"
    )]
    offline: bool,

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

    #[clap(long, value_enum, default_value_t = PreReleaseMode::default(), conflicts_with = "pre", env = "UV_PRERELEASE")]
    prerelease: PreReleaseMode,

    #[clap(long, hide = true, conflicts_with = "prerelease")]
    pre: bool,

    /// Write the compiled requirements to the given `requirements.txt` file.
    #[clap(long, short)]
    output_file: Option<PathBuf>,

    /// The URL of the Python package index (by default: <https://pypi.org/simple>).
    ///
    /// The index given by this flag is given lower priority than all other
    /// indexes specified via the `--extra-index-url` flag.
    ///
    /// Unlike `pip`, `uv` will stop looking for versions of a package as soon
    /// as it finds it in an index. That is, it isn't possible for `uv` to
    /// consider versions of the same package across multiple indexes.
    #[clap(long, short, env = "UV_INDEX_URL", value_parser = parse_index_url)]
    index_url: Option<Maybe<IndexUrl>>,

    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    ///
    /// All indexes given via this flag take priority over the index
    /// in `--index-url` (which defaults to PyPI). And when multiple
    /// `--extra-index-url` flags are given, earlier values take priority.
    ///
    /// Unlike `pip`, `uv` will stop looking for versions of a package as soon
    /// as it finds it in an index. That is, it isn't possible for `uv` to
    /// consider versions of the same package across multiple indexes.
    #[clap(long, env = "UV_EXTRA_INDEX_URL", value_delimiter = ' ', value_parser = parse_index_url)]
    extra_index_url: Vec<Maybe<IndexUrl>>,

    /// Locations to search for candidate distributions, beyond those found in the indexes.
    ///
    /// If a path, the target must be a directory that contains package as wheel files (`.whl`) or
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files.
    #[clap(long, short)]
    find_links: Vec<FlatIndexLocation>,

    /// Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those
    /// discovered via `--find-links`.
    #[clap(long, conflicts_with = "index_url", conflicts_with = "extra_index_url")]
    no_index: bool,

    /// The Python interpreter into which packages should be installed.
    ///
    /// By default, `uv` installs into the virtual environment in the current working directory or
    /// any parent directory. The `--python` option allows you to specify a different interpreter,
    /// which is intended for use in continuous integration (CI) environments or other automated
    /// workflows.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[clap(
        long,
        short,
        verbatim_doc_comment,
        conflicts_with = "system",
        group = "discovery"
    )]
    python: Option<String>,

    /// Install packages into the system Python.
    ///
    /// By default, `uv` installs into the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs `uv` to instead use the first Python
    /// found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[clap(
        long,
        conflicts_with = "python",
        env = "UV_SYSTEM_PYTHON",
        group = "discovery"
    )]
    system: bool,

    /// Allow `uv` to modify an `EXTERNALLY-MANAGED` Python installation.
    ///
    /// WARNING: `--break-system-packages` is intended for use in continuous integration (CI)
    /// environments, when installing into Python installations that are managed by an external
    /// package manager, like `apt`. It should be used with caution, as such Python installations
    /// explicitly recommend against modifications by other package managers (like `uv` or `pip`).
    #[clap(long, requires = "discovery")]
    break_system_packages: bool,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[clap(long)]
    legacy_setup_py: bool,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[clap(long)]
    no_build_isolation: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary code. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Alias for `--only-binary :all:`.
    #[clap(long, conflicts_with = "no_binary", conflicts_with = "only_binary")]
    no_build: bool,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be installed from a source distribution. The resolver
    /// will still use pre-built wheels for metadata.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[clap(long, conflicts_with = "no_build")]
    no_binary: Vec<PackageNameSpecifier>,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[clap(long, conflicts_with = "no_build")]
    only_binary: Vec<PackageNameSpecifier>,

    /// Compile Python files to bytecode.
    ///
    /// By default, does not compile Python (`.py`) files to bytecode (`__pycache__/*.pyc`), instead
    /// Python lazily does the compilation the first time a module is imported. In cases where the
    /// first start time matters, such as CLI applications and docker containers, this option can
    /// trade longer install time for faster startup.
    ///
    /// The compile option will process the entire site-packages directory for consistency and
    /// (like pip) ignore all errors.
    #[clap(long)]
    compile: bool,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[clap(long, short = 'C', alias = "config-settings")]
    config_setting: Vec<ConfigSettingEntry>,

    /// Validate the virtual environment after completing the installation, to detect packages with
    /// missing dependencies or other issues.
    #[clap(long)]
    strict: bool,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, value_parser = date_or_datetime)]
    exclude_newer: Option<DateTime<Utc>>,

    /// Perform a dry run, i.e., don't actually install anything but resolve the dependencies and
    /// print the resulting plan.
    #[clap(long)]
    dry_run: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
struct PipUninstallArgs {
    /// Uninstall all listed packages.
    #[clap(group = "sources")]
    package: Vec<String>,

    /// Uninstall all packages listed in the given requirements files.
    #[clap(long, short, group = "sources")]
    requirement: Vec<PathBuf>,

    /// Uninstall the editable package based on the provided local file path.
    #[clap(long, short, group = "sources")]
    editable: Vec<String>,

    /// The Python interpreter from which packages should be uninstalled.
    ///
    /// By default, `uv` uninstalls from the virtual environment in the current working directory or
    /// any parent directory. The `--python` option allows you to specify a different interpreter,
    /// which is intended for use in continuous integration (CI) environments or other automated
    /// workflows.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[clap(
        long,
        short,
        verbatim_doc_comment,
        conflicts_with = "system",
        group = "discovery"
    )]
    python: Option<String>,

    /// Use the system Python to uninstall packages.
    ///
    /// By default, `uv` uninstalls from the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs `uv` to instead use the first Python
    /// found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[clap(
        long,
        conflicts_with = "python",
        env = "UV_SYSTEM_PYTHON",
        group = "discovery"
    )]
    system: bool,

    /// Allow `uv` to modify an `EXTERNALLY-MANAGED` Python installation.
    ///
    /// WARNING: `--break-system-packages` is intended for use in continuous integration (CI)
    /// environments, when installing into Python installations that are managed by an external
    /// package manager, like `apt`. It should be used with caution, as such Python installations
    /// explicitly recommend against modifications by other package managers (like `uv` or `pip`).
    #[clap(long, requires = "discovery")]
    break_system_packages: bool,

    /// Run offline, i.e., without accessing the network.
    #[arg(global = true, long)]
    offline: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct PipFreezeArgs {
    /// Validate the virtual environment, to detect packages with missing dependencies or other
    /// issues.
    #[clap(long)]
    strict: bool,

    /// The Python interpreter for which packages should be listed.
    ///
    /// By default, `uv` lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[clap(
        long,
        short,
        verbatim_doc_comment,
        conflicts_with = "system",
        group = "discovery"
    )]
    python: Option<String>,

    /// List packages for the system Python.
    ///
    /// By default, `uv` lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found. The `--system` option
    /// instructs `uv` to use the first Python found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution.
    #[clap(
        long,
        conflicts_with = "python",
        env = "UV_SYSTEM_PYTHON",
        group = "discovery"
    )]
    system: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct PipListArgs {
    /// Validate the virtual environment, to detect packages with missing dependencies or other
    /// issues.
    #[clap(long)]
    strict: bool,

    /// Only include editable projects.
    #[clap(short, long)]
    editable: bool,

    /// Exclude any editable packages from output.
    #[clap(long)]
    exclude_editable: bool,

    /// Exclude the specified package(s) from the output.
    #[clap(long)]
    r#exclude: Vec<PackageName>,

    /// Select the output format between: `columns` (default), `freeze`, or `json`.
    #[clap(long, value_enum, default_value_t = ListFormat::default())]
    format: ListFormat,

    /// The Python interpreter for which packages should be listed.
    ///
    /// By default, `uv` lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[clap(
        long,
        short,
        verbatim_doc_comment,
        conflicts_with = "system",
        group = "discovery"
    )]
    python: Option<String>,

    /// List packages for the system Python.
    ///
    /// By default, `uv` lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found. The `--system` option
    /// instructs `uv` to use the first Python found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution.
    #[clap(
        long,
        conflicts_with = "python",
        env = "UV_SYSTEM_PYTHON",
        group = "discovery"
    )]
    system: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct PipShowArgs {
    /// The package(s) to display.
    package: Vec<PackageName>,

    /// Validate the virtual environment, to detect packages with missing dependencies or other
    /// issues.
    #[clap(long)]
    strict: bool,

    /// The Python interpreter for which packages should be listed.
    ///
    /// By default, `uv` lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[clap(
        long,
        short,
        verbatim_doc_comment,
        conflicts_with = "system",
        group = "discovery"
    )]
    python: Option<String>,

    /// List packages for the system Python.
    ///
    /// By default, `uv` lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found. The `--system` option
    /// instructs `uv` to use the first Python found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution.
    #[clap(
        long,
        conflicts_with = "python",
        env = "UV_SYSTEM_PYTHON",
        group = "discovery"
    )]
    system: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
struct VenvArgs {
    /// The Python interpreter to use for the virtual environment.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    ///
    /// Note that this is different from `--python-version` in `pip compile`, which takes `3.10` or `3.10.13` and
    /// doesn't look for a Python interpreter on disk.
    #[clap(
        long,
        short,
        verbatim_doc_comment,
        conflicts_with = "system",
        group = "discovery"
    )]
    python: Option<String>,

    /// Use the system Python to uninstall packages.
    ///
    /// By default, `uv` uninstalls from the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs `uv` to use the first Python found in
    /// the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[clap(
        long,
        conflicts_with = "python",
        env = "UV_SYSTEM_PYTHON",
        group = "discovery"
    )]
    system: bool,

    /// Install seed packages (`pip`, `setuptools`, and `wheel`) into the virtual environment.
    #[clap(long)]
    seed: bool,

    /// The path to the virtual environment to create.
    #[clap(default_value = DEFAULT_VENV_NAME)]
    name: PathBuf,

    /// Provide an alternative prompt prefix for the virtual environment.
    ///
    /// The default behavior depends on whether the virtual environment path is provided:
    /// - If provided (`uv venv project`), the prompt is set to the virtual environment's directory name.
    /// - If not provided (`uv venv`), the prompt is set to the current directory's name.
    ///
    /// Possible values:
    /// - `.`: Use the current directory name.
    /// - Any string: Use the given string.
    #[clap(long, verbatim_doc_comment)]
    prompt: Option<String>,

    /// Give the virtual environment access to the system site packages directory.
    ///
    /// Unlike `pip`, when a virtual environment is created with `--system-site-packages`, `uv` will
    /// _not_ take system site packages into account when running commands like `uv pip list` or
    /// `uv pip install`. The `--system-site-packages` flag will provide the virtual environment
    /// with access to the system site packages directory at runtime, but it will not affect the
    /// behavior of `uv` commands.
    #[clap(long)]
    system_site_packages: bool,

    /// The URL of the Python package index (by default: <https://pypi.org/simple>).
    ///
    /// The index given by this flag is given lower priority than all other
    /// indexes specified via the `--extra-index-url` flag.
    ///
    /// Unlike `pip`, `uv` will stop looking for versions of a package as soon
    /// as it finds it in an index. That is, it isn't possible for `uv` to
    /// consider versions of the same package across multiple indexes.
    #[clap(long, short, env = "UV_INDEX_URL", value_parser = parse_index_url)]
    index_url: Option<Maybe<IndexUrl>>,

    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    ///
    /// All indexes given via this flag take priority over the index
    /// in `--index-url` (which defaults to PyPI). And when multiple
    /// `--extra-index-url` flags are given, earlier values take priority.
    ///
    /// Unlike `pip`, `uv` will stop looking for versions of a package as soon
    /// as it finds it in an index. That is, it isn't possible for `uv` to
    /// consider versions of the same package across multiple indexes.
    #[clap(long, env = "UV_EXTRA_INDEX_URL", value_delimiter = ' ', value_parser = parse_index_url)]
    extra_index_url: Vec<Maybe<IndexUrl>>,

    /// Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those
    /// discovered via `--find-links`.
    #[clap(long, conflicts_with = "index_url", conflicts_with = "extra_index_url")]
    no_index: bool,

    /// Run offline, i.e., without accessing the network.
    #[arg(global = true, long)]
    offline: bool,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, value_parser = date_or_datetime)]
    exclude_newer: Option<DateTime<Utc>>,

    #[command(flatten)]
    compat_args: compat::VenvCompatArgs,
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
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(mut err) => {
            if let Some(ContextValue::String(subcommand)) = err.get(ContextKind::InvalidSubcommand)
            {
                match subcommand.as_str() {
                    "compile" | "lock" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip compile".to_string()),
                        );
                    }
                    "sync" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip sync".to_string()),
                        );
                    }
                    "install" | "add" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip install".to_string()),
                        );
                    }
                    "uninstall" | "remove" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip uninstall".to_string()),
                        );
                    }
                    "freeze" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip freeze".to_string()),
                        );
                    }
                    "list" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip list".to_string()),
                        );
                    }
                    "show" => {
                        err.insert(
                            ContextKind::SuggestedSubcommand,
                            ContextValue::String("uv pip show".to_string()),
                        );
                    }
                    _ => {}
                }
            }
            err.exit()
        }
    };

    // Configure the `tracing` crate, which controls internal logging.
    #[cfg(feature = "tracing-durations-export")]
    let (duration_layer, _duration_guard) = logging::setup_duration();
    #[cfg(not(feature = "tracing-durations-export"))]
    let duration_layer = None::<tracing_subscriber::layer::Identity>;
    logging::setup_logging(
        match cli.verbose {
            0 => logging::Level::Default,
            1 => logging::Level::Verbose,
            2.. => logging::Level::ExtraVerbose,
        },
        duration_layer,
    )?;

    // Configure the `Printer`, which controls user-facing output in the CLI.
    let printer = if cli.quiet {
        printer::Printer::Quiet
    } else if cli.verbose > 0 {
        printer::Printer::Verbose
    } else {
        printer::Printer::Default
    };

    // Configure the `warn!` macros, which control user-facing warnings in the CLI.
    if !cli.quiet {
        uv_warnings::enable();
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
                .wrap_lines(env::var("UV_NO_WRAP").map(|_| false).unwrap_or(true))
                .build(),
        )
    }))?;

    let cache = Cache::try_from(cli.cache_args)?;

    match cli.command {
        Commands::Pip(PipNamespace {
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
            let index_urls = IndexLocations::new(
                args.index_url.and_then(Maybe::into_option),
                args.extra_index_url
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect(),
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
            let no_build = NoBuild::from_args(args.only_binary, args.no_build);
            let dependency_mode = if args.no_deps {
                DependencyMode::Direct
            } else {
                DependencyMode::Transitive
            };
            let prerelease = if args.pre {
                PreReleaseMode::Allow
            } else {
                args.prerelease
            };
            let setup_py = if args.legacy_setup_py {
                SetupPyStrategy::Setuptools
            } else {
                SetupPyStrategy::Pep517
            };
            let config_settings = args.config_setting.into_iter().collect::<ConfigSettings>();
            commands::pip_compile(
                &requirements,
                &constraints,
                &overrides,
                extras,
                args.output_file.as_deref(),
                args.resolution,
                prerelease,
                dependency_mode,
                upgrade,
                args.generate_hashes,
                args.no_emit_package,
                !args.no_annotate,
                !args.no_header,
                args.emit_index_url,
                args.emit_find_links,
                index_urls,
                setup_py,
                config_settings,
                if args.offline {
                    Connectivity::Offline
                } else {
                    Connectivity::Online
                },
                args.no_build_isolation,
                &no_build,
                args.python_version,
                args.exclude_newer,
                args.annotation_style,
                cli.native_tls,
                cli.quiet,
                cache,
                printer,
            )
            .await
        }
        Commands::Pip(PipNamespace {
            command: PipCommand::Sync(args),
        }) => {
            args.compat_args.validate()?;

            let cache = cache.with_refresh(Refresh::from_args(args.refresh, args.refresh_package));
            let index_urls = IndexLocations::new(
                args.index_url.and_then(Maybe::into_option),
                args.extra_index_url
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect(),
                args.find_links,
                args.no_index,
            );
            let sources = args
                .src_file
                .into_iter()
                .map(RequirementsSource::from_path)
                .collect::<Vec<_>>();
            let reinstall = Reinstall::from_args(args.reinstall, args.reinstall_package);
            let no_binary = NoBinary::from_args(args.no_binary);
            let no_build = NoBuild::from_args(args.only_binary, args.no_build);
            let setup_py = if args.legacy_setup_py {
                SetupPyStrategy::Setuptools
            } else {
                SetupPyStrategy::Pep517
            };
            let config_settings = args.config_setting.into_iter().collect::<ConfigSettings>();

            commands::pip_sync(
                &sources,
                &reinstall,
                args.link_mode,
                args.compile,
                index_urls,
                setup_py,
                if args.offline {
                    Connectivity::Offline
                } else {
                    Connectivity::Online
                },
                &config_settings,
                args.no_build_isolation,
                &no_build,
                &no_binary,
                args.strict,
                args.python,
                args.system,
                args.break_system_packages,
                cli.native_tls,
                cache,
                printer,
            )
            .await
        }
        Commands::Pip(PipNamespace {
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
            let index_urls = IndexLocations::new(
                args.index_url.and_then(Maybe::into_option),
                args.extra_index_url
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect(),
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
            let upgrade = Upgrade::from_args(args.upgrade, args.upgrade_package);
            let no_binary = NoBinary::from_args(args.no_binary);
            let no_build = NoBuild::from_args(args.only_binary, args.no_build);
            let dependency_mode = if args.no_deps {
                DependencyMode::Direct
            } else {
                DependencyMode::Transitive
            };
            let prerelease = if args.pre {
                PreReleaseMode::Allow
            } else {
                args.prerelease
            };
            let setup_py = if args.legacy_setup_py {
                SetupPyStrategy::Setuptools
            } else {
                SetupPyStrategy::Pep517
            };
            let config_settings = args.config_setting.into_iter().collect::<ConfigSettings>();

            commands::pip_install(
                &requirements,
                &constraints,
                &overrides,
                &extras,
                args.resolution,
                prerelease,
                dependency_mode,
                upgrade,
                index_urls,
                &reinstall,
                args.link_mode,
                args.compile,
                setup_py,
                if args.offline {
                    Connectivity::Offline
                } else {
                    Connectivity::Online
                },
                &config_settings,
                args.no_build_isolation,
                &no_build,
                &no_binary,
                args.strict,
                args.exclude_newer,
                args.python,
                args.system,
                args.break_system_packages,
                cli.native_tls,
                cache,
                args.dry_run,
                printer,
            )
            .await
        }
        Commands::Pip(PipNamespace {
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
            commands::pip_uninstall(
                &sources,
                args.python,
                args.system,
                args.break_system_packages,
                cache,
                if args.offline {
                    Connectivity::Offline
                } else {
                    Connectivity::Online
                },
                printer,
            )
            .await
        }
        Commands::Pip(PipNamespace {
            command: PipCommand::Freeze(args),
        }) => commands::pip_freeze(
            args.strict,
            args.python.as_deref(),
            args.system,
            &cache,
            printer,
        ),
        Commands::Pip(PipNamespace {
            command: PipCommand::List(args),
        }) => commands::pip_list(
            args.strict,
            args.editable,
            args.exclude_editable,
            &args.exclude,
            &args.format,
            args.python.as_deref(),
            args.system,
            &cache,
            printer,
        ),
        Commands::Pip(PipNamespace {
            command: PipCommand::Show(args),
        }) => commands::pip_show(
            args.package,
            args.strict,
            args.python.as_deref(),
            args.system,
            &cache,
            printer,
        ),
        Commands::Cache(CacheNamespace {
            command: CacheCommand::Clean(args),
        })
        | Commands::Clean(args) => commands::cache_clean(&args.package, &cache, printer),
        Commands::Cache(CacheNamespace {
            command: CacheCommand::Dir,
        }) => {
            commands::cache_dir(&cache);
            Ok(ExitStatus::Success)
        }
        Commands::Venv(args) => {
            args.compat_args.validate()?;

            let index_locations = IndexLocations::new(
                args.index_url.and_then(Maybe::into_option),
                args.extra_index_url
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect(),
                // No find links for the venv subcommand, to keep things simple
                Vec::new(),
                args.no_index,
            );

            // Since we use ".venv" as the default name, we use "." as the default prompt.
            let prompt = args.prompt.or_else(|| {
                if args.name == PathBuf::from(DEFAULT_VENV_NAME) {
                    Some(".".to_string())
                } else {
                    None
                }
            });

            commands::venv(
                &args.name,
                args.python.as_deref(),
                &index_locations,
                uv_virtualenv::Prompt::from_args(prompt),
                args.system_site_packages,
                if args.offline {
                    Connectivity::Offline
                } else {
                    Connectivity::Online
                },
                args.seed,
                args.exclude_newer,
                &cache,
                printer,
            )
            .await
        }
        Commands::Version { output_format } => {
            commands::version(output_format, &mut stdout())?;
            Ok(ExitStatus::Success)
        }
        Commands::GenerateShellCompletion { shell } => {
            shell.generate(&mut Cli::command(), &mut stdout());
            Ok(ExitStatus::Success)
        }
    }
}

fn main() -> ExitCode {
    let result = if let Ok(stack_size) = env::var("UV_STACK_SIZE") {
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
            let mut causes = err.chain();
            eprintln!("{}: {}", "error".red().bold(), causes.next().unwrap());
            for err in causes {
                eprintln!("  {}: {}", "Caused by".red().bold(), err);
            }
            ExitStatus::Error.into()
        }
    }
}
