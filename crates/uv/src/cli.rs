use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Result;
use chrono::{DateTime, Days, NaiveDate, NaiveTime, Utc};
use clap::{Args, Parser, Subcommand};

use distribution_types::{FlatIndexLocation, IndexUrl};
use uv_auth::KeyringProvider;
use uv_cache::CacheArgs;
use uv_configuration::IndexStrategy;
use uv_configuration::{ConfigSettingEntry, PackageNameSpecifier};
use uv_normalize::{ExtraName, PackageName};
use uv_resolver::{AnnotationStyle, PreReleaseMode, ResolutionMode};
use uv_toolchain::PythonVersion;

use crate::commands::{extra_name_with_clap_error, ListFormat, VersionFormat};
use crate::compat;

#[derive(Parser)]
#[command(author, version, long_version = crate::version::version(), about)]
#[command(propagate_version = true)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,

    /// Do not print any output.
    #[arg(global = true, long, short, conflicts_with = "verbose")]
    pub(crate) quiet: bool,

    /// Use verbose output.
    ///
    /// You can configure fine-grained logging using the `RUST_LOG` environment variable.
    /// (<https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives>)
    #[arg(global = true, action = clap::ArgAction::Count, long, short, conflicts_with = "quiet")]
    pub(crate) verbose: u8,

    /// Disable colors; provided for compatibility with `pip`.
    #[arg(global = true, long, hide = true, conflicts_with = "color")]
    pub(crate) no_color: bool,

    /// Control colors in output.
    #[arg(
        global = true,
        long,
        value_enum,
        default_value = "auto",
        conflicts_with = "no_color"
    )]
    pub(crate) color: ColorChoice,

    /// Whether to load TLS certificates from the platform's native certificate store.
    ///
    /// By default, `uv` loads certificates from the bundled `webpki-roots` crate. The
    /// `webpki-roots` are a reliable set of trust roots from Mozilla, and including them in `uv`
    /// improves portability and performance (especially on macOS).
    ///
    /// However, in some cases, you may want to use the platform's native certificate store,
    /// especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's
    /// included in your system's certificate store.
    #[arg(global = true, long, env = "UV_NATIVE_TLS")]
    pub(crate) native_tls: bool,

    #[command(flatten)]
    pub(crate) cache_args: CacheArgs,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub(crate) enum ColorChoice {
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
pub(crate) enum Commands {
    /// Resolve and install Python packages.
    Pip(PipNamespace),
    /// Create a virtual environment.
    #[clap(alias = "virtualenv", alias = "v")]
    Venv(VenvArgs),
    /// Manage the cache.
    Cache(CacheNamespace),
    /// Manage the `uv` executable.
    #[clap(name = "self")]
    #[cfg(feature = "self-update")]
    Self_(SelfNamespace),
    /// Clear the cache, removing all entries or those linked to specific packages.
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
#[cfg(feature = "self-update")]
pub(crate) struct SelfNamespace {
    #[clap(subcommand)]
    pub(crate) command: SelfCommand,
}

#[derive(Subcommand)]
#[cfg(feature = "self-update")]
pub(crate) enum SelfCommand {
    /// Update `uv` to the latest version.
    Update,
}

#[derive(Args)]
pub(crate) struct CacheNamespace {
    #[clap(subcommand)]
    pub(crate) command: CacheCommand,
}

#[derive(Subcommand)]
pub(crate) enum CacheCommand {
    /// Clear the cache, removing all entries or those linked to specific packages.
    Clean(CleanArgs),
    /// Prune all unreachable objects from the cache.
    Prune,
    /// Show the cache directory.
    Dir,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct CleanArgs {
    /// The packages to remove from the cache.
    pub(crate) package: Vec<PackageName>,
}

#[derive(Args)]
pub(crate) struct PipNamespace {
    #[clap(subcommand)]
    pub(crate) command: PipCommand,
}

#[derive(Subcommand)]
pub(crate) enum PipCommand {
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
    /// Verify installed packages have compatible dependencies.
    Check(PipCheckArgs),
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
pub(crate) enum Maybe<T> {
    Some(T),
    None,
}

impl<T> Maybe<T> {
    pub(crate) fn into_option(self) -> Option<T> {
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
pub(crate) struct PipCompileArgs {
    /// Include all packages listed in the given `requirements.in` files.
    ///
    /// When the path is `-`, then requirements are read from stdin.
    #[clap(required(true))]
    pub(crate) src_file: Vec<PathBuf>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[clap(long, short)]
    pub(crate) constraint: Vec<PathBuf>,

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
    pub(crate) r#override: Vec<PathBuf>,

    /// Include optional dependencies in the given extra group name; may be provided more than once.
    #[clap(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub(crate) extra: Vec<ExtraName>,

    /// Include all optional dependencies.
    #[clap(long, conflicts_with = "extra")]
    pub(crate) all_extras: bool,

    /// Ignore package dependencies, instead only add those packages explicitly listed
    /// on the command line to the resulting the requirements file.
    #[clap(long)]
    pub(crate) no_deps: bool,

    #[clap(long, value_enum, default_value_t = ResolutionMode::default(), env = "UV_RESOLUTION")]
    pub(crate) resolution: ResolutionMode,

    #[clap(long, value_enum, default_value_t = PreReleaseMode::default(), env = "UV_PRERELEASE")]
    pub(crate) prerelease: PreReleaseMode,

    #[clap(long, hide = true)]
    pub(crate) pre: bool,

    /// Write the compiled requirements to the given `requirements.txt` file.
    #[clap(long, short)]
    pub(crate) output_file: Option<PathBuf>,

    /// Include extras in the output file.
    ///
    /// By default, `uv` strips extras, as any packages pulled in by the extras are already included
    /// as dependencies in the output file directly. Further, output files generated with
    /// `--no-strip-extras` cannot be used as constraints files in `install` and `sync` invocations.
    #[clap(long)]
    pub(crate) no_strip_extras: bool,

    /// Exclude comment annotations indicating the source of each package.
    #[clap(long)]
    pub(crate) no_annotate: bool,

    /// Exclude the comment header at the top of the generated output file.
    #[clap(long)]
    pub(crate) no_header: bool,

    /// Choose the style of the annotation comments, which indicate the source of each package.
    #[clap(long, default_value_t=AnnotationStyle::Split, value_enum)]
    pub(crate) annotation_style: AnnotationStyle,

    /// Change header comment to reflect custom command wrapping `uv pip compile`.
    #[clap(long, env = "UV_CUSTOM_COMPILE_COMMAND")]
    pub(crate) custom_compile_command: Option<String>,

    /// Run offline, i.e., without accessing the network.
    #[arg(
        global = true,
        long,
        conflicts_with = "refresh",
        conflicts_with = "refresh_package"
    )]
    pub(crate) offline: bool,

    /// Refresh all cached data.
    #[clap(long)]
    pub(crate) refresh: bool,

    /// Refresh cached data for a specific package.
    #[clap(long)]
    pub(crate) refresh_package: Vec<PackageName>,

    /// The method to use when installing packages from the global cache.
    ///
    /// This option is only used when creating build environments for source distributions.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[clap(long, value_enum, default_value_t = install_wheel_rs::linker::LinkMode::default())]
    pub(crate) link_mode: install_wheel_rs::linker::LinkMode,

    /// The URL of the Python package index (by default: <https://pypi.org/simple>).
    ///
    /// The index given by this flag is given lower priority than all other
    /// indexes specified via the `--extra-index-url` flag.
    ///
    /// Unlike `pip`, `uv` will stop looking for versions of a package as soon
    /// as it finds it in an index. That is, it isn't possible for `uv` to
    /// consider versions of the same package across multiple indexes.
    #[clap(long, short, env = "UV_INDEX_URL", value_parser = parse_index_url)]
    pub(crate) index_url: Option<Maybe<IndexUrl>>,

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
    pub(crate) extra_index_url: Vec<Maybe<IndexUrl>>,

    /// Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those
    /// discovered via `--find-links`.
    #[clap(long)]
    pub(crate) no_index: bool,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, `uv` will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index. This prevents "dependency confusion"
    /// attacks, whereby an attack can upload a malicious package under the same name to a secondary
    /// index.
    #[clap(long, default_value_t, value_enum, env = "UV_INDEX_STRATEGY")]
    pub(crate) index_strategy: IndexStrategy,

    /// Attempt to use `keyring` for authentication for index urls
    ///
    /// Due to not having Python imports, only `--keyring-provider subprocess` argument is currently
    /// implemented `uv` will try to use `keyring` via CLI when this flag is used.
    #[clap(long, default_value_t, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub(crate) keyring_provider: KeyringProvider,

    /// Locations to search for candidate distributions, beyond those found in the indexes.
    ///
    /// If a path, the target must be a directory that contains package as wheel files (`.whl`) or
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files.
    #[clap(long, short)]
    pub(crate) find_links: Vec<FlatIndexLocation>,

    /// Allow package upgrades, ignoring pinned versions in the existing output file.
    #[clap(long, short = 'U')]
    pub(crate) upgrade: bool,

    /// Allow upgrades for a specific package, ignoring pinned versions in the existing output
    /// file.
    #[clap(long, short = 'P')]
    pub(crate) upgrade_package: Vec<PackageName>,

    /// Include distribution hashes in the output file.
    #[clap(long)]
    pub(crate) generate_hashes: bool,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[clap(long)]
    pub(crate) legacy_setup_py: bool,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[clap(long)]
    pub(crate) no_build_isolation: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary code. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Alias for `--only-binary :all:`.
    #[clap(long, conflicts_with = "only_binary")]
    pub(crate) no_build: bool,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[clap(long, conflicts_with = "no_build")]
    pub(crate) only_binary: Vec<PackageNameSpecifier>,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[clap(long, short = 'C', alias = "config-settings")]
    pub(crate) config_setting: Vec<ConfigSettingEntry>,

    /// The minimum Python version that should be supported by the compiled requirements (e.g.,
    /// `3.7` or `3.7.9`).
    ///
    /// If a patch version is omitted, the most recent known patch version for that minor version
    /// is assumed. For example, `3.7` is mapped to `3.7.17`.
    #[arg(long, short)]
    pub(crate) python_version: Option<PythonVersion>,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, value_parser = date_or_datetime)]
    pub(crate) exclude_newer: Option<DateTime<Utc>>,

    /// Specify a package to omit from the output resolution. Its dependencies will still be
    /// included in the resolution. Equivalent to pip-compile's `--unsafe-package` option.
    #[clap(long, alias = "unsafe-package")]
    pub(crate) no_emit_package: Vec<PackageName>,

    /// Include `--index-url` and `--extra-index-url` entries in the generated output file.
    #[clap(long)]
    pub(crate) emit_index_url: bool,

    /// Include `--find-links` entries in the generated output file.
    #[clap(long)]
    pub(crate) emit_find_links: bool,

    /// Whether to emit a marker string indicating when it is known that the
    /// resulting set of pinned dependencies is valid.
    ///
    /// The pinned dependencies may be valid even when the marker expression is
    /// false, but when the expression is true, the requirements are known to
    /// be correct.
    #[clap(long, hide = true)]
    pub(crate) emit_marker_expression: bool,

    /// Include comment annotations indicating the index used to resolve each package (e.g.,
    /// `# from https://pypi.org/simple`).
    #[clap(long)]
    pub(crate) emit_index_annotation: bool,

    #[command(flatten)]
    pub(crate) compat_args: compat::PipCompileCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PipSyncArgs {
    /// Include all packages listed in the given `requirements.txt` files.
    #[clap(required(true))]
    pub(crate) src_file: Vec<PathBuf>,

    /// Reinstall all packages, regardless of whether they're already installed.
    #[clap(long, alias = "force-reinstall")]
    pub(crate) reinstall: bool,

    /// Reinstall a specific package, regardless of whether it's already installed.
    #[clap(long)]
    pub(crate) reinstall_package: Vec<PackageName>,

    /// Run offline, i.e., without accessing the network.
    #[arg(
        global = true,
        long,
        conflicts_with = "refresh",
        conflicts_with = "refresh_package"
    )]
    pub(crate) offline: bool,

    /// Refresh all cached data.
    #[clap(long)]
    pub(crate) refresh: bool,

    /// Refresh cached data for a specific package.
    #[clap(long)]
    pub(crate) refresh_package: Vec<PackageName>,

    /// The method to use when installing packages from the global cache.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[clap(long, value_enum, default_value_t = install_wheel_rs::linker::LinkMode::default())]
    pub(crate) link_mode: install_wheel_rs::linker::LinkMode,

    /// The URL of the Python package index (by default: <https://pypi.org/simple>).
    ///
    /// The index given by this flag is given lower priority than all other
    /// indexes specified via the `--extra-index-url` flag.
    ///
    /// Unlike `pip`, `uv` will stop looking for versions of a package as soon
    /// as it finds it in an index. That is, it isn't possible for `uv` to
    /// consider versions of the same package across multiple indexes.
    #[clap(long, short, env = "UV_INDEX_URL", value_parser = parse_index_url)]
    pub(crate) index_url: Option<Maybe<IndexUrl>>,

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
    pub(crate) extra_index_url: Vec<Maybe<IndexUrl>>,

    /// Locations to search for candidate distributions, beyond those found in the indexes.
    ///
    /// If a path, the target must be a directory that contains package as wheel files (`.whl`) or
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files.
    #[clap(long, short)]
    pub(crate) find_links: Vec<FlatIndexLocation>,

    /// Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those
    /// discovered via `--find-links`.
    #[clap(long)]
    pub(crate) no_index: bool,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, `uv` will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index. This prevents "dependency confusion"
    /// attacks, whereby an attack can upload a malicious package under the same name to a secondary
    /// index.
    #[clap(long, default_value_t, value_enum, env = "UV_INDEX_STRATEGY")]
    pub(crate) index_strategy: IndexStrategy,

    /// Require a matching hash for each requirement.
    ///
    /// Hash-checking mode is all or nothing. If enabled, _all_ requirements must be provided
    /// with a corresponding hash or set of hashes. Additionally, if enabled, _all_ requirements
    /// must either be pinned to exact versions (e.g., `==1.0.0`), or be specified via direct URL.
    ///
    /// Hash-checking mode introduces a number of additional constraints:
    /// - Git dependencies are not supported.
    /// - Editable installs are not supported.
    /// - Local dependencies are not supported, unless they point to a specific wheel (`.whl`) or
    ///   source archive (`.zip`, `.tar.gz`), as opposed to a directory.
    #[clap(long, hide = true)]
    pub(crate) require_hashes: bool,

    /// Attempt to use `keyring` for authentication for index urls
    ///
    /// Function's similar to `pip`'s `--keyring-provider subprocess` argument,
    /// `uv` will try to use `keyring` via CLI when this flag is used.
    #[clap(long, default_value_t, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub(crate) keyring_provider: KeyringProvider,

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
    #[clap(long, short, verbatim_doc_comment, group = "discovery")]
    pub(crate) python: Option<String>,

    /// Install packages into the system Python.
    ///
    /// By default, `uv` installs into the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs `uv` to instead use the first Python
    /// found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[clap(long, env = "UV_SYSTEM_PYTHON", group = "discovery")]
    pub(crate) system: bool,

    /// Allow `uv` to modify an `EXTERNALLY-MANAGED` Python installation.
    ///
    /// WARNING: `--break-system-packages` is intended for use in continuous integration (CI)
    /// environments, when installing into Python installations that are managed by an external
    /// package manager, like `apt`. It should be used with caution, as such Python installations
    /// explicitly recommend against modifications by other package managers (like `uv` or `pip`).
    #[clap(long, env = "UV_BREAK_SYSTEM_PACKAGES", requires = "discovery")]
    pub(crate) break_system_packages: bool,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[clap(long)]
    pub(crate) legacy_setup_py: bool,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[clap(long)]
    pub(crate) no_build_isolation: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary code. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Alias for `--only-binary :all:`.
    #[clap(long, conflicts_with = "no_binary", conflicts_with = "only_binary")]
    pub(crate) no_build: bool,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be installed from a source distribution. The resolver
    /// will still use pre-built wheels for metadata.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[clap(long, conflicts_with = "no_build")]
    pub(crate) no_binary: Vec<PackageNameSpecifier>,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[clap(long, conflicts_with = "no_build")]
    pub(crate) only_binary: Vec<PackageNameSpecifier>,

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
    pub(crate) compile: bool,

    /// Don't compile Python files to bytecode.
    #[clap(long, hide = true, conflicts_with = "compile")]
    pub(crate) no_compile: bool,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[clap(long, short = 'C', alias = "config-settings")]
    pub(crate) config_setting: Vec<ConfigSettingEntry>,

    /// Validate the virtual environment after completing the installation, to detect packages with
    /// missing dependencies or other issues.
    #[clap(long)]
    pub(crate) strict: bool,

    #[command(flatten)]
    pub(crate) compat_args: compat::PipSyncCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
pub(crate) struct PipInstallArgs {
    /// Install all listed packages.
    #[clap(group = "sources")]
    pub(crate) package: Vec<String>,

    /// Install all packages listed in the given requirements files.
    #[clap(long, short, group = "sources")]
    pub(crate) requirement: Vec<PathBuf>,

    /// Install the editable package based on the provided local file path.
    #[clap(long, short, group = "sources")]
    pub(crate) editable: Vec<String>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[clap(long, short)]
    pub(crate) constraint: Vec<PathBuf>,

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
    pub(crate) r#override: Vec<PathBuf>,

    /// Include optional dependencies in the given extra group name; may be provided more than once.
    #[clap(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub(crate) extra: Vec<ExtraName>,

    /// Include all optional dependencies.
    #[clap(long, conflicts_with = "extra")]
    pub(crate) all_extras: bool,

    /// Allow package upgrades.
    #[clap(long, short = 'U')]
    pub(crate) upgrade: bool,

    /// Allow upgrade of a specific package.
    #[clap(long, short = 'P')]
    pub(crate) upgrade_package: Vec<PackageName>,

    /// Reinstall all packages, regardless of whether they're already installed.
    #[clap(long, alias = "force-reinstall")]
    pub(crate) reinstall: bool,

    /// Reinstall a specific package, regardless of whether it's already installed.
    #[clap(long)]
    pub(crate) reinstall_package: Vec<PackageName>,

    /// Run offline, i.e., without accessing the network.
    #[arg(
        global = true,
        long,
        conflicts_with = "refresh",
        conflicts_with = "refresh_package"
    )]
    pub(crate) offline: bool,

    /// Refresh all cached data.
    #[clap(long)]
    pub(crate) refresh: bool,

    /// Refresh cached data for a specific package.
    #[clap(long)]
    pub(crate) refresh_package: Vec<PackageName>,

    /// Ignore package dependencies, instead only installing those packages explicitly listed
    /// on the command line or in the requirements files.
    #[clap(long)]
    pub(crate) no_deps: bool,

    /// The method to use when installing packages from the global cache.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[clap(long, value_enum, default_value_t = install_wheel_rs::linker::LinkMode::default())]
    pub(crate) link_mode: install_wheel_rs::linker::LinkMode,

    #[clap(long, value_enum, default_value_t = ResolutionMode::default(), env = "UV_RESOLUTION")]
    pub(crate) resolution: ResolutionMode,

    #[clap(long, value_enum, default_value_t = PreReleaseMode::default(), env = "UV_PRERELEASE")]
    pub(crate) prerelease: PreReleaseMode,

    #[clap(long, hide = true)]
    pub(crate) pre: bool,

    /// The URL of the Python package index (by default: <https://pypi.org/simple>).
    ///
    /// The index given by this flag is given lower priority than all other
    /// indexes specified via the `--extra-index-url` flag.
    ///
    /// Unlike `pip`, `uv` will stop looking for versions of a package as soon
    /// as it finds it in an index. That is, it isn't possible for `uv` to
    /// consider versions of the same package across multiple indexes.
    #[clap(long, short, env = "UV_INDEX_URL", value_parser = parse_index_url)]
    pub(crate) index_url: Option<Maybe<IndexUrl>>,

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
    pub(crate) extra_index_url: Vec<Maybe<IndexUrl>>,

    /// Locations to search for candidate distributions, beyond those found in the indexes.
    ///
    /// If a path, the target must be a directory that contains package as wheel files (`.whl`) or
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files.
    #[clap(long, short)]
    pub(crate) find_links: Vec<FlatIndexLocation>,

    /// Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those
    /// discovered via `--find-links`.
    #[clap(long)]
    pub(crate) no_index: bool,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, `uv` will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index. This prevents "dependency confusion"
    /// attacks, whereby an attack can upload a malicious package under the same name to a secondary
    /// index.
    #[clap(long, default_value_t, value_enum, env = "UV_INDEX_STRATEGY")]
    pub(crate) index_strategy: IndexStrategy,

    /// Require a matching hash for each requirement.
    ///
    /// Hash-checking mode is all or nothing. If enabled, _all_ requirements must be provided
    /// with a corresponding hash or set of hashes. Additionally, if enabled, _all_ requirements
    /// must either be pinned to exact versions (e.g., `==1.0.0`), or be specified via direct URL.
    ///
    /// Hash-checking mode introduces a number of additional constraints:
    /// - Git dependencies are not supported.
    /// - Editable installs are not supported.
    /// - Local dependencies are not supported, unless they point to a specific wheel (`.whl`) or
    ///   source archive (`.zip`, `.tar.gz`), as opposed to a directory.
    #[clap(long, hide = true)]
    pub(crate) require_hashes: bool,

    /// Attempt to use `keyring` for authentication for index urls
    ///
    /// Due to not having Python imports, only `--keyring-provider subprocess` argument is currently
    /// implemented `uv` will try to use `keyring` via CLI when this flag is used.
    #[clap(long, default_value_t, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub(crate) keyring_provider: KeyringProvider,

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
    #[clap(long, short, verbatim_doc_comment, group = "discovery")]
    pub(crate) python: Option<String>,

    /// Install packages into the system Python.
    ///
    /// By default, `uv` installs into the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs `uv` to instead use the first Python
    /// found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[clap(long, env = "UV_SYSTEM_PYTHON", group = "discovery")]
    pub(crate) system: bool,

    /// Allow `uv` to modify an `EXTERNALLY-MANAGED` Python installation.
    ///
    /// WARNING: `--break-system-packages` is intended for use in continuous integration (CI)
    /// environments, when installing into Python installations that are managed by an external
    /// package manager, like `apt`. It should be used with caution, as such Python installations
    /// explicitly recommend against modifications by other package managers (like `uv` or `pip`).
    #[clap(long, env = "UV_BREAK_SYSTEM_PACKAGES", requires = "discovery")]
    pub(crate) break_system_packages: bool,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[clap(long)]
    pub(crate) legacy_setup_py: bool,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[clap(long)]
    pub(crate) no_build_isolation: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary code. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Alias for `--only-binary :all:`.
    #[clap(long, conflicts_with = "no_binary", conflicts_with = "only_binary")]
    pub(crate) no_build: bool,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be installed from a source distribution. The resolver
    /// will still use pre-built wheels for metadata.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[clap(long, conflicts_with = "no_build")]
    pub(crate) no_binary: Vec<PackageNameSpecifier>,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[clap(long, conflicts_with = "no_build")]
    pub(crate) only_binary: Vec<PackageNameSpecifier>,

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
    pub(crate) compile: bool,

    /// Don't compile Python files to bytecode.
    #[clap(long, hide = true, conflicts_with = "compile")]
    pub(crate) no_compile: bool,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[clap(long, short = 'C', alias = "config-settings")]
    pub(crate) config_setting: Vec<ConfigSettingEntry>,

    /// Validate the virtual environment after completing the installation, to detect packages with
    /// missing dependencies or other issues.
    #[clap(long)]
    pub(crate) strict: bool,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, value_parser = date_or_datetime)]
    pub(crate) exclude_newer: Option<DateTime<Utc>>,

    /// Perform a dry run, i.e., don't actually install anything but resolve the dependencies and
    /// print the resulting plan.
    #[clap(long)]
    pub(crate) dry_run: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
pub(crate) struct PipUninstallArgs {
    /// Uninstall all listed packages.
    #[clap(group = "sources")]
    pub(crate) package: Vec<String>,

    /// Uninstall all packages listed in the given requirements files.
    #[clap(long, short, group = "sources")]
    pub(crate) requirement: Vec<PathBuf>,

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
    #[clap(long, short, verbatim_doc_comment, group = "discovery")]
    pub(crate) python: Option<String>,

    /// Attempt to use `keyring` for authentication for remote requirements files.
    ///
    /// Due to not having Python imports, only `--keyring-provider subprocess` argument is currently
    /// implemented `uv` will try to use `keyring` via CLI when this flag is used.
    #[clap(long, default_value_t, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub(crate) keyring_provider: KeyringProvider,

    /// Use the system Python to uninstall packages.
    ///
    /// By default, `uv` uninstalls from the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs `uv` to instead use the first Python
    /// found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[clap(long, env = "UV_SYSTEM_PYTHON", group = "discovery")]
    pub(crate) system: bool,

    /// Allow `uv` to modify an `EXTERNALLY-MANAGED` Python installation.
    ///
    /// WARNING: `--break-system-packages` is intended for use in continuous integration (CI)
    /// environments, when installing into Python installations that are managed by an external
    /// package manager, like `apt`. It should be used with caution, as such Python installations
    /// explicitly recommend against modifications by other package managers (like `uv` or `pip`).
    #[clap(long, env = "UV_BREAK_SYSTEM_PACKAGES", requires = "discovery")]
    pub(crate) break_system_packages: bool,

    /// Run offline, i.e., without accessing the network.
    #[arg(global = true, long)]
    pub(crate) offline: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PipFreezeArgs {
    /// Exclude any editable packages from output.
    #[clap(long)]
    pub(crate) exclude_editable: bool,

    /// Validate the virtual environment, to detect packages with missing dependencies or other
    /// issues.
    #[clap(long)]
    pub(crate) strict: bool,

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
    #[clap(long, short, verbatim_doc_comment, group = "discovery")]
    pub(crate) python: Option<String>,

    /// List packages for the system Python.
    ///
    /// By default, `uv` lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found. The `--system` option
    /// instructs `uv` to use the first Python found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution.
    #[clap(long, env = "UV_SYSTEM_PYTHON", group = "discovery")]
    pub(crate) system: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PipListArgs {
    /// Only include editable projects.
    #[clap(short, long)]
    pub(crate) editable: bool,

    /// Exclude any editable packages from output.
    #[clap(long)]
    pub(crate) exclude_editable: bool,

    /// Exclude the specified package(s) from the output.
    #[clap(long)]
    pub(crate) r#exclude: Vec<PackageName>,

    /// Select the output format between: `columns` (default), `freeze`, or `json`.
    #[clap(long, value_enum, default_value_t = ListFormat::default())]
    pub(crate) format: ListFormat,

    /// Validate the virtual environment, to detect packages with missing dependencies or other
    /// issues.
    #[clap(long)]
    pub(crate) strict: bool,

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
    #[clap(long, short, verbatim_doc_comment, group = "discovery")]
    pub(crate) python: Option<String>,

    /// List packages for the system Python.
    ///
    /// By default, `uv` lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found. The `--system` option
    /// instructs `uv` to use the first Python found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution.
    #[clap(long, env = "UV_SYSTEM_PYTHON", group = "discovery")]
    pub(crate) system: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PipCheckArgs {
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
    #[clap(long, short, verbatim_doc_comment, group = "discovery")]
    pub(crate) python: Option<String>,

    /// List packages for the system Python.
    ///
    /// By default, `uv` lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found. The `--system` option
    /// instructs `uv` to use the first Python found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution.
    #[clap(long, env = "UV_SYSTEM_PYTHON", group = "discovery")]
    pub(crate) system: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PipShowArgs {
    /// The package(s) to display.
    pub(crate) package: Vec<PackageName>,

    /// Validate the virtual environment, to detect packages with missing dependencies or other
    /// issues.
    #[clap(long)]
    pub(crate) strict: bool,

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
    #[clap(long, short, verbatim_doc_comment, group = "discovery")]
    pub(crate) python: Option<String>,

    /// List packages for the system Python.
    ///
    /// By default, `uv` lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found. The `--system` option
    /// instructs `uv` to use the first Python found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution.
    #[clap(long, env = "UV_SYSTEM_PYTHON", group = "discovery")]
    pub(crate) system: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct VenvArgs {
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
    #[clap(long, short, verbatim_doc_comment, group = "discovery")]
    pub(crate) python: Option<String>,

    /// Use the system Python to uninstall packages.
    ///
    /// By default, `uv` uninstalls from the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs `uv` to use the first Python found in
    /// the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[clap(long, env = "UV_SYSTEM_PYTHON", group = "discovery")]
    system: bool,

    /// Install seed packages (`pip`, `setuptools`, and `wheel`) into the virtual environment.
    #[clap(long)]
    pub(crate) seed: bool,

    /// The path to the virtual environment to create.
    #[clap(default_value = ".venv")]
    pub(crate) name: PathBuf,

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
    pub(crate) prompt: Option<String>,

    /// Give the virtual environment access to the system site packages directory.
    ///
    /// Unlike `pip`, when a virtual environment is created with `--system-site-packages`, `uv` will
    /// _not_ take system site packages into account when running commands like `uv pip list` or
    /// `uv pip install`. The `--system-site-packages` flag will provide the virtual environment
    /// with access to the system site packages directory at runtime, but it will not affect the
    /// behavior of `uv` commands.
    #[clap(long)]
    pub(crate) system_site_packages: bool,

    /// The method to use when installing packages from the global cache.
    ///
    /// This option is only used for installing seed packages.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[clap(long, value_enum, default_value_t = install_wheel_rs::linker::LinkMode::default())]
    pub(crate) link_mode: install_wheel_rs::linker::LinkMode,

    /// The URL of the Python package index (by default: <https://pypi.org/simple>).
    ///
    /// The index given by this flag is given lower priority than all other
    /// indexes specified via the `--extra-index-url` flag.
    ///
    /// Unlike `pip`, `uv` will stop looking for versions of a package as soon
    /// as it finds it in an index. That is, it isn't possible for `uv` to
    /// consider versions of the same package across multiple indexes.
    #[clap(long, short, env = "UV_INDEX_URL", value_parser = parse_index_url)]
    pub(crate) index_url: Option<Maybe<IndexUrl>>,

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
    pub(crate) extra_index_url: Vec<Maybe<IndexUrl>>,

    /// Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those
    /// discovered via `--find-links`.
    #[clap(long)]
    pub(crate) no_index: bool,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, `uv` will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index. This prevents "dependency confusion"
    /// attacks, whereby an attack can upload a malicious package under the same name to a secondary
    /// index.
    #[clap(long, default_value_t, value_enum, env = "UV_INDEX_STRATEGY")]
    pub(crate) index_strategy: IndexStrategy,

    /// Attempt to use `keyring` for authentication for index urls
    ///
    /// Due to not having Python imports, only `--keyring-provider subprocess` argument is currently
    /// implemented `uv` will try to use `keyring` via CLI when this flag is used.
    #[clap(long, default_value_t, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub(crate) keyring_provider: KeyringProvider,

    /// Run offline, i.e., without accessing the network.
    #[arg(global = true, long)]
    pub(crate) offline: bool,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, value_parser = date_or_datetime)]
    pub(crate) exclude_newer: Option<DateTime<Utc>>,

    #[command(flatten)]
    pub(crate) compat_args: compat::VenvCompatArgs,
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
