use std::ffi::OsString;
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Result;

use clap::{Args, Parser, Subcommand};

use distribution_types::{FlatIndexLocation, IndexUrl};
use uv_cache::CacheArgs;
use uv_configuration::{
    ConfigSettingEntry, IndexStrategy, KeyringProviderType, PackageNameSpecifier, TargetTriple,
};
use uv_normalize::{ExtraName, PackageName};
use uv_resolver::{AnnotationStyle, ExcludeNewer, PreReleaseMode, ResolutionMode};
use uv_toolchain::{PythonVersion, ToolchainPreference};

use crate::commands::{extra_name_with_clap_error, ListFormat, VersionFormat};
use crate::compat;

#[derive(Parser)]
#[command(author, version, long_version = crate::version::version(), about)]
#[command(propagate_version = true)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,

    #[command(flatten)]
    pub(crate) global_args: GlobalArgs,

    #[command(flatten)]
    pub(crate) cache_args: CacheArgs,

    /// The path to a `uv.toml` file to use for configuration.
    #[arg(global = true, long, env = "UV_CONFIG_FILE")]
    pub(crate) config_file: Option<PathBuf>,
}

#[derive(Parser, Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct GlobalArgs {
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
        conflicts_with = "no_color",
        value_name = "COLOR_CHOICE"
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
    #[arg(global = true, long, env = "UV_NATIVE_TLS", value_parser = clap::builder::BoolishValueParser::new(), overrides_with("no_native_tls"))]
    pub(crate) native_tls: bool,

    #[arg(global = true, long, overrides_with("native_tls"), hide = true)]
    pub(crate) no_native_tls: bool,

    /// Disable network access, relying only on locally cached data and locally available files.
    #[arg(global = true, long, overrides_with("no_offline"))]
    pub(crate) offline: bool,

    #[arg(global = true, long, overrides_with("offline"), hide = true)]
    pub(crate) no_offline: bool,

    /// Whether to use system or uv-managed Python toolchains.
    #[arg(global = true, long)]
    pub(crate) toolchain_preference: Option<ToolchainPreference>,

    /// Whether to enable experimental, preview features.
    #[arg(global = true, long, hide = true, env = "UV_PREVIEW", value_parser = clap::builder::BoolishValueParser::new(), overrides_with("no_preview"))]
    pub(crate) preview: bool,

    #[arg(global = true, long, overrides_with("preview"), hide = true)]
    pub(crate) no_preview: bool,

    /// Avoid discovering a `pyproject.toml` or `uv.toml` file in the current directory or any
    /// parent directories.
    #[arg(global = true, long, hide = true)]
    pub(crate) isolated: bool,

    /// Show the resolved settings for the current command.
    #[arg(global = true, long, hide = true)]
    pub(crate) show_settings: bool,
}

#[derive(Debug, Copy, Clone, clap::ValueEnum)]
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
    /// Run and manage executable Python packages.
    Tool(ToolNamespace),
    /// Manage Python installations.
    Toolchain(ToolchainNamespace),
    /// Manage Python projects.
    #[command(flatten)]
    Project(ProjectCommand),
    /// Create a virtual environment.
    #[command(alias = "virtualenv", alias = "v")]
    Venv(VenvArgs),
    /// Manage the cache.
    Cache(CacheNamespace),
    /// Manage the `uv` executable.
    #[command(name = "self")]
    #[cfg(feature = "self-update")]
    Self_(SelfNamespace),
    /// Clear the cache, removing all entries or those linked to specific packages.
    #[command(hide = true)]
    Clean(CleanArgs),
    /// Display uv's version
    Version {
        #[arg(long, value_enum, default_value = "text")]
        output_format: VersionFormat,
    },
    /// Generate shell completion
    #[command(alias = "--generate-shell-completion", hide = true)]
    GenerateShellCompletion { shell: clap_complete_command::Shell },
}

#[derive(Args)]
#[cfg(feature = "self-update")]
pub(crate) struct SelfNamespace {
    #[command(subcommand)]
    pub(crate) command: SelfCommand,
}

#[derive(Subcommand)]
#[cfg(feature = "self-update")]
pub(crate) enum SelfCommand {
    /// Update `uv` to the latest version.
    Update,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct CacheNamespace {
    #[command(subcommand)]
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

#[derive(Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct CleanArgs {
    /// The packages to remove from the cache.
    pub(crate) package: Vec<PackageName>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PipNamespace {
    #[command(subcommand)]
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

#[derive(Subcommand)]
pub(crate) enum ProjectCommand {
    /// Run a command in the project environment.
    #[clap(hide = true)]
    Run(RunArgs),
    /// Sync the project's dependencies with the environment.
    #[clap(hide = true)]
    Sync(SyncArgs),
    /// Resolve the project requirements into a lockfile.
    #[clap(hide = true)]
    Lock(LockArgs),
    /// Add one or more packages to the project requirements.
    #[clap(hide = true)]
    Add(AddArgs),
    /// Remove one or more packages from the project requirements.
    #[clap(hide = true)]
    Remove(RemoveArgs),
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

/// Parse a string into a [`PathBuf`]. The string can represent a file, either as a path or a
/// `file://` URL.
fn parse_file_path(input: &str) -> Result<PathBuf, String> {
    if input.starts_with("file://") {
        let url = match url::Url::from_str(input) {
            Ok(url) => url,
            Err(err) => return Err(err.to_string()),
        };
        url.to_file_path()
            .map_err(|()| "invalid file URL".to_string())
    } else {
        match PathBuf::from_str(input) {
            Ok(path) => Ok(path),
            Err(err) => Err(err.to_string()),
        }
    }
}

/// Parse a string into a [`PathBuf`], mapping the empty string to `None`.
fn parse_maybe_file_path(input: &str) -> Result<Maybe<PathBuf>, String> {
    if input.is_empty() {
        Ok(Maybe::None)
    } else {
        parse_file_path(input).map(Maybe::Some)
    }
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PipCompileArgs {
    /// Include all packages listed in the given `requirements.in` files.
    ///
    /// If a `pyproject.toml`, `setup.py`, or `setup.cfg` file is provided, `uv` will
    /// extract the requirements for the relevant project.
    ///
    /// If `-` is provided, then requirements will be read from stdin.
    #[arg(required(true), value_parser = parse_file_path)]
    pub(crate) src_file: Vec<PathBuf>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[arg(long, short, env = "UV_CONSTRAINT", value_delimiter = ' ', value_parser = parse_maybe_file_path)]
    pub(crate) constraint: Vec<Maybe<PathBuf>>,

    /// Override versions using the given requirements files.
    ///
    /// Overrides files are `requirements.txt`-like files that force a specific version of a
    /// requirement to be installed, regardless of the requirements declared by any constituent
    /// package, and regardless of whether this would be considered an invalid resolution.
    ///
    /// While constraints are _additive_, in that they're combined with the requirements of the
    /// constituent packages, overrides are _absolute_, in that they completely replace the
    /// requirements of the constituent packages.
    #[arg(long, value_parser = parse_file_path)]
    pub(crate) r#override: Vec<PathBuf>,

    /// Include optional dependencies from the extra group name; may be provided more than once.
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub(crate) extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "extra")]
    pub(crate) all_extras: bool,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub(crate) no_all_extras: bool,

    #[command(flatten)]
    pub(crate) resolver: ResolverArgs,

    #[command(flatten)]
    pub(crate) refresh: RefreshArgs,

    /// Ignore package dependencies, instead only add those packages explicitly listed
    /// on the command line to the resulting the requirements file.
    #[arg(long)]
    pub(crate) no_deps: bool,

    #[arg(long, overrides_with("no_deps"), hide = true)]
    pub(crate) deps: bool,

    /// Write the compiled requirements to the given `requirements.txt` file.
    #[arg(long, short)]
    pub(crate) output_file: Option<PathBuf>,

    /// Include extras in the output file.
    ///
    /// By default, `uv` strips extras, as any packages pulled in by the extras are already included
    /// as dependencies in the output file directly. Further, output files generated with
    /// `--no-strip-extras` cannot be used as constraints files in `install` and `sync` invocations.
    #[arg(long, overrides_with("strip_extras"))]
    pub(crate) no_strip_extras: bool,

    #[arg(long, overrides_with("no_strip_extras"), hide = true)]
    pub(crate) strip_extras: bool,

    /// Exclude comment annotations indicating the source of each package.
    #[arg(long, overrides_with("annotate"))]
    pub(crate) no_annotate: bool,

    #[arg(long, overrides_with("no_annotate"), hide = true)]
    pub(crate) annotate: bool,

    /// Exclude the comment header at the top of the generated output file.
    #[arg(long, overrides_with("header"))]
    pub(crate) no_header: bool,

    #[arg(long, overrides_with("no_header"), hide = true)]
    pub(crate) header: bool,

    /// Choose the style of the annotation comments, which indicate the source of each package.
    ///
    /// Defaults to `split`.
    #[arg(long, value_enum)]
    pub(crate) annotation_style: Option<AnnotationStyle>,

    /// Change header comment to reflect custom command wrapping `uv pip compile`.
    #[arg(long, env = "UV_CUSTOM_COMPILE_COMMAND")]
    pub(crate) custom_compile_command: Option<String>,

    /// The Python interpreter against which to compile the requirements.
    ///
    /// By default, `uv` uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--python`
    /// option allows you to specify a different interpreter.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, verbatim_doc_comment)]
    pub(crate) python: Option<String>,

    /// Install packages into the system Python.
    ///
    /// By default, `uv` uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--system`
    /// option instructs `uv` to avoid using a virtual environment Python and restrict its search to
    /// the system path.
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub(crate) system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub(crate) no_system: bool,

    /// Include distribution hashes in the output file.
    #[arg(long, overrides_with("no_generate_hashes"))]
    pub(crate) generate_hashes: bool,

    #[arg(long, overrides_with("generate_hashes"), hide = true)]
    pub(crate) no_generate_hashes: bool,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[arg(long, overrides_with("no_legacy_setup_py"))]
    pub(crate) legacy_setup_py: bool,

    #[arg(long, overrides_with("legacy_setup_py"), hide = true)]
    pub(crate) no_legacy_setup_py: bool,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[arg(
        long,
        env = "UV_NO_BUILD_ISOLATION",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("build_isolation")
    )]
    pub(crate) no_build_isolation: bool,

    #[arg(long, overrides_with("no_build_isolation"), hide = true)]
    pub(crate) build_isolation: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary code. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Alias for `--only-binary :all:`.
    #[arg(
        long,
        conflicts_with = "no_binary",
        conflicts_with = "only_binary",
        overrides_with("build")
    )]
    pub(crate) no_build: bool,

    #[arg(
        long,
        conflicts_with = "no_binary",
        conflicts_with = "only_binary",
        overrides_with("no_build"),
        hide = true
    )]
    pub(crate) build: bool,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be installed from a source distribution. The resolver
    /// will still use pre-built wheels for metadata.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[arg(long, conflicts_with = "no_build")]
    pub(crate) no_binary: Option<Vec<PackageNameSpecifier>>,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[arg(long, conflicts_with = "no_build")]
    pub(crate) only_binary: Option<Vec<PackageNameSpecifier>>,

    /// The minimum Python version that should be supported by the compiled requirements (e.g.,
    /// `3.7` or `3.7.9`).
    ///
    /// If a patch version is omitted, the minimum patch version is assumed. For example, `3.7` is
    /// mapped to `3.7.0`.
    #[arg(long, short)]
    pub(crate) python_version: Option<PythonVersion>,

    /// The platform for which requirements should be resolved.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aaarch64-apple-darwin`.
    #[arg(long)]
    pub(crate) python_platform: Option<TargetTriple>,

    /// Specify a package to omit from the output resolution. Its dependencies will still be
    /// included in the resolution. Equivalent to pip-compile's `--unsafe-package` option.
    #[arg(long, alias = "unsafe-package")]
    pub(crate) no_emit_package: Option<Vec<PackageName>>,

    /// Include `--index-url` and `--extra-index-url` entries in the generated output file.
    #[arg(long, overrides_with("no_emit_index_url"))]
    pub(crate) emit_index_url: bool,

    #[arg(long, overrides_with("emit_index_url"), hide = true)]
    pub(crate) no_emit_index_url: bool,

    /// Include `--find-links` entries in the generated output file.
    #[arg(long, overrides_with("no_emit_find_links"))]
    pub(crate) emit_find_links: bool,

    #[arg(long, overrides_with("emit_find_links"), hide = true)]
    pub(crate) no_emit_find_links: bool,

    /// Whether to emit a marker string indicating when it is known that the
    /// resulting set of pinned dependencies is valid.
    ///
    /// The pinned dependencies may be valid even when the marker expression is
    /// false, but when the expression is true, the requirements are known to
    /// be correct.
    #[arg(long, overrides_with("no_emit_marker_expression"), hide = true)]
    pub(crate) emit_marker_expression: bool,

    #[arg(long, overrides_with("emit_marker_expression"), hide = true)]
    pub(crate) no_emit_marker_expression: bool,

    /// Include comment annotations indicating the index used to resolve each package (e.g.,
    /// `# from https://pypi.org/simple`).
    #[arg(long, overrides_with("no_emit_index_annotation"))]
    pub(crate) emit_index_annotation: bool,

    #[arg(long, overrides_with("emit_index_annotation"), hide = true)]
    pub(crate) no_emit_index_annotation: bool,

    #[command(flatten)]
    pub(crate) compat_args: compat::PipCompileCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PipSyncArgs {
    /// Include all packages listed in the given `requirements.txt` files.
    ///
    /// If a `pyproject.toml`, `setup.py`, or `setup.cfg` file is provided, `uv` will
    /// extract the requirements for the relevant project.
    ///
    /// If `-` is provided, then requirements will be read from stdin.
    #[arg(required(true), value_parser = parse_file_path)]
    pub(crate) src_file: Vec<PathBuf>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[arg(long, short, env = "UV_CONSTRAINT", value_delimiter = ' ', value_parser = parse_maybe_file_path)]
    pub(crate) constraint: Vec<Maybe<PathBuf>>,

    #[command(flatten)]
    pub(crate) installer: InstallerArgs,

    #[command(flatten)]
    pub(crate) refresh: RefreshArgs,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, env = "UV_EXCLUDE_NEWER")]
    pub(crate) exclude_newer: Option<ExcludeNewer>,

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
    #[arg(long,         env = "UV_REQUIRE_HASHES",
        value_parser = clap::builder::BoolishValueParser::new(), overrides_with("no_require_hashes"))]
    pub(crate) require_hashes: bool,

    #[arg(long, overrides_with("require_hashes"), hide = true)]
    pub(crate) no_require_hashes: bool,

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
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub(crate) python: Option<String>,

    /// Install packages into the system Python.
    ///
    /// By default, `uv` installs into the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs `uv` to instead use the first Python
    /// found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub(crate) system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub(crate) no_system: bool,

    /// Allow `uv` to modify an `EXTERNALLY-MANAGED` Python installation.
    ///
    /// WARNING: `--break-system-packages` is intended for use in continuous integration (CI)
    /// environments, when installing into Python installations that are managed by an external
    /// package manager, like `apt`. It should be used with caution, as such Python installations
    /// explicitly recommend against modifications by other package managers (like `uv` or `pip`).
    #[arg(
        long,
        env = "UV_BREAK_SYSTEM_PACKAGES",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_break_system_packages")
    )]
    pub(crate) break_system_packages: bool,

    #[arg(long, overrides_with("break_system_packages"))]
    pub(crate) no_break_system_packages: bool,

    /// Install packages into the specified directory, rather than into the virtual environment
    /// or system Python interpreter. The packages will be installed at the top-level of the
    /// directory
    #[arg(long, conflicts_with = "prefix")]
    pub(crate) target: Option<PathBuf>,

    /// Install packages into `lib`, `bin`, and other top-level folders under the specified
    /// directory, as if a virtual environment were created at the specified location.
    ///
    /// In general, prefer the use of `--python` to install into an alternate environment, as
    /// scripts and other artifacts installed via `--prefix` will reference the installing
    /// interpreter, rather than any interpreter added to the `--prefix` directory, rendering them
    /// non-portable.
    #[arg(long, conflicts_with = "target")]
    pub(crate) prefix: Option<PathBuf>,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[arg(long, overrides_with("no_legacy_setup_py"))]
    pub(crate) legacy_setup_py: bool,

    #[arg(long, overrides_with("legacy_setup_py"), hide = true)]
    pub(crate) no_legacy_setup_py: bool,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[arg(
        long,
        env = "UV_NO_BUILD_ISOLATION",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("build_isolation")
    )]
    pub(crate) no_build_isolation: bool,

    #[arg(long, overrides_with("no_build_isolation"), hide = true)]
    pub(crate) build_isolation: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary code. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Alias for `--only-binary :all:`.
    #[arg(
        long,
        conflicts_with = "no_binary",
        conflicts_with = "only_binary",
        overrides_with("build")
    )]
    pub(crate) no_build: bool,

    #[arg(
        long,
        conflicts_with = "no_binary",
        conflicts_with = "only_binary",
        overrides_with("no_build"),
        hide = true
    )]
    pub(crate) build: bool,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be installed from a source distribution. The resolver
    /// will still use pre-built wheels for metadata.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[arg(long, conflicts_with = "no_build")]
    pub(crate) no_binary: Option<Vec<PackageNameSpecifier>>,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[arg(long, conflicts_with = "no_build")]
    pub(crate) only_binary: Option<Vec<PackageNameSpecifier>>,

    /// The minimum Python version that should be supported by the requirements (e.g.,
    /// `3.7` or `3.7.9`).
    ///
    /// If a patch version is omitted, the minimum patch version is assumed. For example, `3.7` is
    /// mapped to `3.7.0`.
    #[arg(long)]
    pub(crate) python_version: Option<PythonVersion>,

    /// The platform for which requirements should be installed.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aaarch64-apple-darwin`.
    ///
    /// WARNING: When specified, uv will select wheels that are compatible with the _target_
    /// platform; as a result, the installed distributions may not be compatible with the _current_
    /// platform. Conversely, any distributions that are built from source may be incompatible with
    /// the _target_ platform, as they will be built for the _current_ platform. The
    /// `--python-platform` option is intended for advanced use cases.
    #[arg(long)]
    pub(crate) python_platform: Option<TargetTriple>,

    /// Validate the virtual environment after completing the installation, to detect packages with
    /// missing dependencies or other issues.
    #[arg(long, overrides_with("no_strict"))]
    pub(crate) strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub(crate) no_strict: bool,

    /// Perform a dry run, i.e., don't actually install anything but resolve the dependencies and
    /// print the resulting plan.
    #[arg(long)]
    pub(crate) dry_run: bool,

    #[command(flatten)]
    pub(crate) compat_args: compat::PipSyncCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
pub(crate) struct PipInstallArgs {
    /// Install all listed packages.
    #[arg(group = "sources")]
    pub(crate) package: Vec<String>,

    /// Install all packages listed in the given `requirements.txt` files.
    ///
    /// If a `pyproject.toml`, `setup.py`, or `setup.cfg` file is provided, `uv` will
    /// extract the requirements for the relevant project.
    ///
    /// If `-` is provided, then requirements will be read from stdin.
    #[arg(long, short, group = "sources", value_parser = parse_file_path)]
    pub(crate) requirement: Vec<PathBuf>,

    /// Install the editable package based on the provided local file path.
    #[arg(long, short, group = "sources")]
    pub(crate) editable: Vec<String>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[arg(long, short, env = "UV_CONSTRAINT", value_delimiter = ' ', value_parser = parse_maybe_file_path)]
    pub(crate) constraint: Vec<Maybe<PathBuf>>,

    /// Override versions using the given requirements files.
    ///
    /// Overrides files are `requirements.txt`-like files that force a specific version of a
    /// requirement to be installed, regardless of the requirements declared by any constituent
    /// package, and regardless of whether this would be considered an invalid resolution.
    ///
    /// While constraints are _additive_, in that they're combined with the requirements of the
    /// constituent packages, overrides are _absolute_, in that they completely replace the
    /// requirements of the constituent packages.
    #[arg(long, value_parser = parse_file_path)]
    pub(crate) r#override: Vec<PathBuf>,

    /// Include optional dependencies from the extra group name; may be provided more than once.
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub(crate) extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "extra", overrides_with = "no_all_extras")]
    pub(crate) all_extras: bool,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub(crate) no_all_extras: bool,

    #[command(flatten)]
    pub(crate) installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub(crate) refresh: RefreshArgs,

    /// Ignore package dependencies, instead only installing those packages explicitly listed
    /// on the command line or in the requirements files.
    #[arg(long, overrides_with("deps"))]
    pub(crate) no_deps: bool,

    #[arg(long, overrides_with("no_deps"), hide = true)]
    pub(crate) deps: bool,

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
    #[arg(
        long,
        env = "UV_REQUIRE_HASHES",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_require_hashes"),
    )]
    pub(crate) require_hashes: bool,

    #[arg(long, overrides_with("require_hashes"), hide = true)]
    pub(crate) no_require_hashes: bool,

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
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub(crate) python: Option<String>,

    /// Install packages into the system Python.
    ///
    /// By default, `uv` installs into the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs `uv` to instead use the first Python
    /// found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub(crate) system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub(crate) no_system: bool,

    /// Allow `uv` to modify an `EXTERNALLY-MANAGED` Python installation.
    ///
    /// WARNING: `--break-system-packages` is intended for use in continuous integration (CI)
    /// environments, when installing into Python installations that are managed by an external
    /// package manager, like `apt`. It should be used with caution, as such Python installations
    /// explicitly recommend against modifications by other package managers (like `uv` or `pip`).
    #[arg(
        long,
        env = "UV_BREAK_SYSTEM_PACKAGES",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_break_system_packages")
    )]
    pub(crate) break_system_packages: bool,

    #[arg(long, overrides_with("break_system_packages"))]
    pub(crate) no_break_system_packages: bool,

    /// Install packages into the specified directory, rather than into the virtual environment
    /// or system Python interpreter. The packages will be installed at the top-level of the
    /// directory
    #[arg(long, conflicts_with = "prefix")]
    pub(crate) target: Option<PathBuf>,

    /// Install packages into `lib`, `bin`, and other top-level folders under the specified
    /// directory, as if a virtual environment were created at the specified location.
    ///
    /// In general, prefer the use of `--python` to install into an alternate environment, as
    /// scripts and other artifacts installed via `--prefix` will reference the installing
    /// interpreter, rather than any interpreter added to the `--prefix` directory, rendering them
    /// non-portable.
    #[arg(long, conflicts_with = "target")]
    pub(crate) prefix: Option<PathBuf>,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[arg(long, overrides_with("no_legacy_setup_py"))]
    pub(crate) legacy_setup_py: bool,

    #[arg(long, overrides_with("legacy_setup_py"), hide = true)]
    pub(crate) no_legacy_setup_py: bool,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[arg(
        long,
        env = "UV_NO_BUILD_ISOLATION",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("build_isolation")
    )]
    pub(crate) no_build_isolation: bool,

    #[arg(long, overrides_with("no_build_isolation"), hide = true)]
    pub(crate) build_isolation: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary code. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Alias for `--only-binary :all:`.
    #[arg(
        long,
        conflicts_with = "no_binary",
        conflicts_with = "only_binary",
        overrides_with("build")
    )]
    pub(crate) no_build: bool,

    #[arg(
        long,
        conflicts_with = "no_binary",
        conflicts_with = "only_binary",
        overrides_with("no_build"),
        hide = true
    )]
    pub(crate) build: bool,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be installed from a source distribution. The resolver
    /// will still use pre-built wheels for metadata.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[arg(long, conflicts_with = "no_build")]
    pub(crate) no_binary: Option<Vec<PackageNameSpecifier>>,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[arg(long, conflicts_with = "no_build")]
    pub(crate) only_binary: Option<Vec<PackageNameSpecifier>>,

    /// The minimum Python version that should be supported by the requirements (e.g.,
    /// `3.7` or `3.7.9`).
    ///
    /// If a patch version is omitted, the minimum patch version is assumed. For example, `3.7` is
    /// mapped to `3.7.0`.
    #[arg(long)]
    pub(crate) python_version: Option<PythonVersion>,

    /// The platform for which requirements should be installed.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aaarch64-apple-darwin`.
    ///
    /// WARNING: When specified, uv will select wheels that are compatible with the _target_
    /// platform; as a result, the installed distributions may not be compatible with the _current_
    /// platform. Conversely, any distributions that are built from source may be incompatible with
    /// the _target_ platform, as they will be built for the _current_ platform. The
    /// `--python-platform` option is intended for advanced use cases.
    #[arg(long)]
    pub(crate) python_platform: Option<TargetTriple>,

    /// Validate the virtual environment after completing the installation, to detect packages with
    /// missing dependencies or other issues.
    #[arg(long, overrides_with("no_strict"))]
    pub(crate) strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub(crate) no_strict: bool,

    /// Perform a dry run, i.e., don't actually install anything but resolve the dependencies and
    /// print the resulting plan.
    #[arg(long)]
    pub(crate) dry_run: bool,

    #[command(flatten)]
    pub(crate) compat_args: compat::PipInstallCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
pub(crate) struct PipUninstallArgs {
    /// Uninstall all listed packages.
    #[arg(group = "sources")]
    pub(crate) package: Vec<String>,

    /// Uninstall all packages listed in the given requirements files.
    #[arg(long, short, group = "sources", value_parser = parse_file_path)]
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
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub(crate) python: Option<String>,

    /// Attempt to use `keyring` for authentication for remote requirements files.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures `uv` to
    /// use the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(long, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub(crate) keyring_provider: Option<KeyringProviderType>,

    /// Use the system Python to uninstall packages.
    ///
    /// By default, `uv` uninstalls from the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs `uv` to instead use the first Python
    /// found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub(crate) system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub(crate) no_system: bool,

    /// Allow `uv` to modify an `EXTERNALLY-MANAGED` Python installation.
    ///
    /// WARNING: `--break-system-packages` is intended for use in continuous integration (CI)
    /// environments, when installing into Python installations that are managed by an external
    /// package manager, like `apt`. It should be used with caution, as such Python installations
    /// explicitly recommend against modifications by other package managers (like `uv` or `pip`).
    #[arg(
        long,
        env = "UV_BREAK_SYSTEM_PACKAGES",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_break_system_packages")
    )]
    pub(crate) break_system_packages: bool,

    #[arg(long, overrides_with("break_system_packages"))]
    pub(crate) no_break_system_packages: bool,

    /// Uninstall packages from the specified `--target` directory.
    #[arg(long, conflicts_with = "prefix")]
    pub(crate) target: Option<PathBuf>,

    /// Uninstall packages from the specified `--prefix` directory.
    #[arg(long, conflicts_with = "target")]
    pub(crate) prefix: Option<PathBuf>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PipFreezeArgs {
    /// Exclude any editable packages from output.
    #[arg(long)]
    pub(crate) exclude_editable: bool,

    /// Validate the virtual environment, to detect packages with missing dependencies or other
    /// issues.
    #[arg(long, overrides_with("no_strict"))]
    pub(crate) strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub(crate) no_strict: bool,

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
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
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
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub(crate) system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub(crate) no_system: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PipListArgs {
    /// Only include editable projects.
    #[arg(short, long)]
    pub(crate) editable: bool,

    /// Exclude any editable packages from output.
    #[arg(long)]
    pub(crate) exclude_editable: bool,

    /// Exclude the specified package(s) from the output.
    #[arg(long)]
    pub(crate) r#exclude: Vec<PackageName>,

    /// Select the output format between: `columns` (default), `freeze`, or `json`.
    #[arg(long, value_enum, default_value_t = ListFormat::default())]
    pub(crate) format: ListFormat,

    /// Validate the virtual environment, to detect packages with missing dependencies or other
    /// issues.
    #[arg(long, overrides_with("no_strict"))]
    pub(crate) strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub(crate) no_strict: bool,

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
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
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
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub(crate) system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub(crate) no_system: bool,

    #[command(flatten)]
    pub(crate) compat_args: compat::PipListCompatArgs,
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
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
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
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub(crate) system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub(crate) no_system: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PipShowArgs {
    /// The package(s) to display.
    pub(crate) package: Vec<PackageName>,

    /// Validate the virtual environment, to detect packages with missing dependencies or other
    /// issues.
    #[arg(long, overrides_with("no_strict"))]
    pub(crate) strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub(crate) no_strict: bool,

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
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
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
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub(crate) system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub(crate) no_system: bool,
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
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub(crate) python: Option<String>,

    /// Use the system Python to uninstall packages.
    ///
    /// By default, `uv` uninstalls from the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs `uv` to use the first Python found in
    /// the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub(crate) system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub(crate) no_system: bool,

    /// Install seed packages (`pip`, `setuptools`, and `wheel`) into the virtual environment.
    #[arg(long)]
    pub(crate) seed: bool,

    /// Preserve any existing files or directories at the target path.
    ///
    /// By default, `uv venv` will remove an existing virtual environment at the given path, and
    /// exit with an error if the path is non-empty but _not_ a virtual environment. The
    /// `--allow-existing` option will instead write to the given path, regardless of its contents,
    /// and without clearing it beforehand.
    ///
    /// WARNING: This option can lead to unexpected behavior if the existing virtual environment
    /// and the newly-created virtual environment are linked to different Python interpreters.
    #[clap(long)]
    pub(crate) allow_existing: bool,

    /// The path to the virtual environment to create.
    #[arg(default_value = ".venv")]
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
    #[arg(long, verbatim_doc_comment)]
    pub(crate) prompt: Option<String>,

    /// Give the virtual environment access to the system site packages directory.
    ///
    /// Unlike `pip`, when a virtual environment is created with `--system-site-packages`, `uv` will
    /// _not_ take system site packages into account when running commands like `uv pip list` or
    /// `uv pip install`. The `--system-site-packages` flag will provide the virtual environment
    /// with access to the system site packages directory at runtime, but it will not affect the
    /// behavior of `uv` commands.
    #[arg(long)]
    pub(crate) system_site_packages: bool,

    #[command(flatten)]
    pub(crate) index_args: IndexArgs,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, `uv` will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-match`. This prevents
    /// "dependency confusion" attacks, whereby an attack can upload a malicious package under the
    /// same name to a secondary
    #[arg(long, value_enum, env = "UV_INDEX_STRATEGY")]
    pub(crate) index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures `uv` to
    /// use the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(long, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub(crate) keyring_provider: Option<KeyringProviderType>,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, env = "UV_EXCLUDE_NEWER")]
    pub(crate) exclude_newer: Option<ExcludeNewer>,

    /// The method to use when installing packages from the global cache.
    ///
    /// This option is only used for installing seed packages.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[arg(long, value_enum, env = "UV_LINK_MODE")]
    pub(crate) link_mode: Option<install_wheel_rs::linker::LinkMode>,

    #[command(flatten)]
    pub(crate) compat_args: compat::VenvCompatArgs,
}

#[derive(Parser, Debug, Clone)]
pub(crate) enum ExternalCommand {
    #[command(external_subcommand)]
    Cmd(Vec<OsString>),
}

impl Deref for ExternalCommand {
    type Target = Vec<OsString>;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Cmd(cmd) => cmd,
        }
    }
}

impl ExternalCommand {
    pub(crate) fn split(&self) -> (Option<&OsString>, &[OsString]) {
        match self.as_slice() {
            [] => (None, &[]),
            [cmd, args @ ..] => (Some(cmd), args),
        }
    }
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct RunArgs {
    /// Include optional dependencies from the extra group name; may be provided more than once.
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub(crate) extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "extra")]
    pub(crate) all_extras: bool,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub(crate) no_all_extras: bool,

    /// Include development dependencies.
    #[arg(long, overrides_with("no_dev"), hide = true)]
    pub(crate) dev: bool,

    /// Omit development dependencies.
    #[arg(long, overrides_with("dev"))]
    pub(crate) no_dev: bool,

    /// The command to run.
    #[command(subcommand)]
    pub(crate) command: ExternalCommand,

    /// Run with the given packages installed.
    #[arg(long)]
    pub(crate) with: Vec<String>,

    #[command(flatten)]
    pub(crate) installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub(crate) build: BuildArgs,

    #[command(flatten)]
    pub(crate) refresh: RefreshArgs,

    /// The Python interpreter to use to build the run environment.
    ///
    /// By default, `uv` uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--python`
    /// option allows you to specify a different interpreter.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub(crate) python: Option<String>,

    /// Run the command in a different package in the workspace.
    #[arg(long, conflicts_with = "isolated")]
    pub(crate) package: Option<PackageName>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct SyncArgs {
    /// Include optional dependencies from the extra group name; may be provided more than once.
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub(crate) extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "extra")]
    pub(crate) all_extras: bool,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub(crate) no_all_extras: bool,

    /// Include development dependencies.
    #[arg(long, overrides_with("no_dev"), hide = true)]
    pub(crate) dev: bool,

    /// Omit development dependencies.
    #[arg(long, overrides_with("dev"))]
    pub(crate) no_dev: bool,

    /// Does not clean the environment.
    /// Without this flag any extraneous installations will be removed.
    #[arg(long)]
    pub(crate) no_clean: bool,

    #[command(flatten)]
    pub(crate) installer: InstallerArgs,

    #[command(flatten)]
    pub(crate) build: BuildArgs,

    #[command(flatten)]
    pub(crate) refresh: RefreshArgs,

    /// The Python interpreter to use to build the run environment.
    ///
    /// By default, `uv` uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--python`
    /// option allows you to specify a different interpreter.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub(crate) python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct LockArgs {
    #[command(flatten)]
    pub(crate) resolver: ResolverArgs,

    #[command(flatten)]
    pub(crate) build: BuildArgs,

    #[command(flatten)]
    pub(crate) refresh: RefreshArgs,

    /// The Python interpreter to use to build the run environment.
    ///
    /// By default, `uv` uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--python`
    /// option allows you to specify a different interpreter.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub(crate) python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct AddArgs {
    /// The packages to add, as PEP 508 requirements (e.g., `flask==2.2.3`).
    #[arg(required = true)]
    pub(crate) requirements: Vec<String>,

    /// Add the requirements as development dependencies.
    #[arg(long)]
    pub(crate) dev: bool,

    /// Add the requirements as workspace dependencies.
    #[arg(long)]
    pub(crate) workspace: bool,

    /// Add the requirements as editables.
    #[arg(long, default_missing_value = "true", num_args(0..=1))]
    pub(crate) editable: Option<bool>,

    /// Add source requirements to the `project.dependencies` section of the `pyproject.toml`.
    ///
    /// Without this flag uv will try to use `tool.uv.sources` for any sources.
    #[arg(long)]
    pub(crate) raw: bool,

    /// Specific commit to use when adding from Git.
    #[arg(long)]
    pub(crate) rev: Option<String>,

    /// Tag to use when adding from git.
    #[arg(long)]
    pub(crate) tag: Option<String>,

    /// Branch to use when adding from git.
    #[arg(long)]
    pub(crate) branch: Option<String>,

    #[command(flatten)]
    pub(crate) installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub(crate) build: BuildArgs,

    #[command(flatten)]
    pub(crate) refresh: RefreshArgs,

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
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub(crate) python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct RemoveArgs {
    /// The names of the packages to remove (e.g., `flask`).
    #[arg(required = true)]
    pub(crate) requirements: Vec<PackageName>,

    /// Remove the requirements from development dependencies.
    #[arg(long)]
    pub(crate) dev: bool,

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
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub(crate) python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ToolNamespace {
    #[command(subcommand)]
    pub(crate) command: ToolCommand,
}

#[derive(Subcommand)]
pub(crate) enum ToolCommand {
    /// Run a tool
    Run(ToolRunArgs),
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ToolRunArgs {
    /// The command to run.
    #[command(subcommand)]
    pub(crate) command: ExternalCommand,

    /// Use the given package to provide the command.
    ///
    /// By default, the package name is assumed to match the command name.
    #[arg(long)]
    pub(crate) from: Option<String>,

    /// Include the following extra requirements.
    #[arg(long)]
    pub(crate) with: Vec<String>,

    #[command(flatten)]
    pub(crate) installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub(crate) build: BuildArgs,

    #[command(flatten)]
    pub(crate) refresh: RefreshArgs,

    /// The Python interpreter to use to build the run environment.
    ///
    /// By default, `uv` uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--python`
    /// option allows you to specify a different interpreter.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub(crate) python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ToolchainNamespace {
    #[command(subcommand)]
    pub(crate) command: ToolchainCommand,
}

#[derive(Subcommand)]
pub(crate) enum ToolchainCommand {
    /// List the available toolchains.
    List(ToolchainListArgs),

    /// Download and install a specific toolchain.
    Install(ToolchainInstallArgs),

    /// Search for a toolchain
    #[command(disable_version_flag = true)]
    Find(ToolchainFindArgs),
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ToolchainListArgs {
    /// List all toolchain versions, including outdated patch versions.
    #[arg(long)]
    pub(crate) all_versions: bool,

    /// List toolchains for all platforms.
    #[arg(long)]
    pub(crate) all_platforms: bool,

    /// Only show installed toolchains, exclude available downloads.
    #[arg(long)]
    pub(crate) only_installed: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ToolchainInstallArgs {
    /// The toolchains to install.
    ///
    /// If not provided, the requested toolchain(s) will be read from the `.python-versions`
    ///  or `.python-version` files. If neither file is present, uv will check if it has
    /// installed any toolchains. If not, it will install the latest stable version of Python.
    pub(crate) targets: Vec<String>,

    /// Force the installation of the toolchain, even if it is already installed.
    #[arg(long, short)]
    pub(crate) force: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ToolchainFindArgs {
    /// The toolchain request.
    pub(crate) request: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct IndexArgs {
    /// The URL of the Python package index (by default: <https://pypi.org/simple>).
    ///
    /// Accepts either a repository compliant with PEP 503 (the simple repository API), or a local
    /// directory laid out in the same format.
    ///
    /// The index given by this flag is given lower priority than all other
    /// indexes specified via the `--extra-index-url` flag.
    #[arg(long, short, env = "UV_INDEX_URL", value_parser = parse_index_url)]
    pub(crate) index_url: Option<Maybe<IndexUrl>>,

    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    ///
    /// Accepts either a repository compliant with PEP 503 (the simple repository API), or a local
    /// directory laid out in the same format.
    ///
    /// All indexes given via this flag take priority over the index
    /// in `--index-url` (which defaults to PyPI). And when multiple
    /// `--extra-index-url` flags are given, earlier values take priority.
    #[arg(long, env = "UV_EXTRA_INDEX_URL", value_delimiter = ' ', value_parser = parse_index_url)]
    pub(crate) extra_index_url: Option<Vec<Maybe<IndexUrl>>>,

    /// Locations to search for candidate distributions, beyond those found in the indexes.
    ///
    /// If a path, the target must be a directory that contains package as wheel files (`.whl`) or
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files.
    #[arg(long, short)]
    pub(crate) find_links: Option<Vec<FlatIndexLocation>>,

    /// Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those
    /// discovered via `--find-links`.
    #[arg(long)]
    pub(crate) no_index: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct RefreshArgs {
    /// Refresh all cached data.
    #[arg(long, conflicts_with("offline"), overrides_with("no_refresh"))]
    pub(crate) refresh: bool,

    #[arg(
        long,
        conflicts_with("offline"),
        overrides_with("refresh"),
        hide = true
    )]
    pub(crate) no_refresh: bool,

    /// Refresh cached data for a specific package.
    #[arg(long)]
    pub(crate) refresh_package: Vec<PackageName>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct BuildArgs {
    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary code. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    #[arg(long, overrides_with("build"))]
    pub(crate) no_build: bool,

    #[arg(long, overrides_with("no_build"), hide = true)]
    pub(crate) build: bool,

    /// Don't build source distributions for a specific package.
    #[arg(long)]
    pub(crate) no_build_package: Vec<PackageName>,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be installed from a source distribution. The resolver
    /// will still use pre-built wheels for metadata.
    #[arg(long, overrides_with("binary"))]
    pub(crate) no_binary: bool,

    #[arg(long, overrides_with("no_binary"), hide = true)]
    pub(crate) binary: bool,

    /// Don't install pre-built wheels for a specific package.
    #[arg(long)]
    pub(crate) no_binary_package: Vec<PackageName>,
}

/// Arguments that are used by commands that need to install (but not resolve) packages.
#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct InstallerArgs {
    #[command(flatten)]
    pub(crate) index_args: IndexArgs,

    /// Reinstall all packages, regardless of whether they're already installed.
    #[arg(long, alias = "force-reinstall", overrides_with("no_reinstall"))]
    pub(crate) reinstall: bool,

    #[arg(long, overrides_with("reinstall"), hide = true)]
    pub(crate) no_reinstall: bool,

    /// Reinstall a specific package, regardless of whether it's already installed.
    #[arg(long)]
    pub(crate) reinstall_package: Vec<PackageName>,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, `uv` will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-match`. This prevents
    /// "dependency confusion" attacks, whereby an attack can upload a malicious package under the
    /// same name to a secondary
    #[arg(long, value_enum, env = "UV_INDEX_STRATEGY")]
    pub(crate) index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures `uv` to
    /// use the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(long, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub(crate) keyring_provider: Option<KeyringProviderType>,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[arg(long, short = 'C', alias = "config-settings")]
    pub(crate) config_setting: Option<Vec<ConfigSettingEntry>>,

    /// The method to use when installing packages from the global cache.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[arg(long, value_enum, env = "UV_LINK_MODE")]
    pub(crate) link_mode: Option<install_wheel_rs::linker::LinkMode>,

    /// Compile Python files to bytecode.
    ///
    /// By default, does not compile Python (`.py`) files to bytecode (`__pycache__/*.pyc`), instead
    /// Python lazily does the compilation the first time a module is imported. In cases where the
    /// first start time matters, such as CLI applications and docker containers, this option can
    /// trade longer install time for faster startup.
    ///
    /// The compile option will process the entire site-packages directory for consistency and
    /// (like pip) ignore all errors.
    #[arg(long, alias = "compile", overrides_with("no_compile_bytecode"))]
    pub(crate) compile_bytecode: bool,

    #[arg(
        long,
        alias = "no_compile",
        overrides_with("compile_bytecode"),
        hide = true
    )]
    pub(crate) no_compile_bytecode: bool,
}

/// Arguments that are used by commands that need to resolve (but not install) packages.
#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ResolverArgs {
    #[command(flatten)]
    pub(crate) index_args: IndexArgs,

    /// Allow package upgrades, ignoring pinned versions in any existing output file.
    #[arg(long, short = 'U', overrides_with("no_upgrade"))]
    pub(crate) upgrade: bool,

    #[arg(long, overrides_with("upgrade"), hide = true)]
    pub(crate) no_upgrade: bool,

    /// Allow upgrades for a specific package, ignoring pinned versions in any existing output
    /// file.
    #[arg(long, short = 'P')]
    pub(crate) upgrade_package: Vec<PackageName>,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, `uv` will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-match`. This prevents
    /// "dependency confusion" attacks, whereby an attack can upload a malicious package under the
    /// same name to a secondary
    #[arg(long, value_enum, env = "UV_INDEX_STRATEGY")]
    pub(crate) index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures `uv` to
    /// use the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(long, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub(crate) keyring_provider: Option<KeyringProviderType>,

    /// The strategy to use when selecting between the different compatible versions for a given
    /// package requirement.
    ///
    /// By default, `uv` will use the latest compatible version of each package (`highest`).
    #[arg(long, value_enum, env = "UV_RESOLUTION")]
    pub(crate) resolution: Option<ResolutionMode>,

    /// The strategy to use when considering pre-release versions.
    ///
    /// By default, `uv` will accept pre-releases for packages that _only_ publish pre-releases,
    /// along with first-party requirements that contain an explicit pre-release marker in the
    /// declared specifiers (`if-necessary-or-explicit`).
    #[arg(long, value_enum, env = "UV_PRERELEASE")]
    pub(crate) prerelease: Option<PreReleaseMode>,

    #[arg(long, hide = true)]
    pub(crate) pre: bool,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[arg(long, short = 'C', alias = "config-settings")]
    pub(crate) config_setting: Option<Vec<ConfigSettingEntry>>,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, env = "UV_EXCLUDE_NEWER")]
    pub(crate) exclude_newer: Option<ExcludeNewer>,

    /// The method to use when installing packages from the global cache.
    ///
    /// This option is only used when building source distributions.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[arg(long, value_enum, env = "UV_LINK_MODE")]
    pub(crate) link_mode: Option<install_wheel_rs::linker::LinkMode>,
}

/// Arguments that are used by commands that need to resolve and install packages.
#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ResolverInstallerArgs {
    #[command(flatten)]
    pub(crate) index_args: IndexArgs,

    /// Allow package upgrades, ignoring pinned versions in any existing output file.
    #[arg(long, short = 'U', overrides_with("no_upgrade"))]
    pub(crate) upgrade: bool,

    #[arg(long, overrides_with("upgrade"), hide = true)]
    pub(crate) no_upgrade: bool,

    /// Allow upgrades for a specific package, ignoring pinned versions in any existing output
    /// file.
    #[arg(long, short = 'P')]
    pub(crate) upgrade_package: Vec<PackageName>,

    /// Reinstall all packages, regardless of whether they're already installed.
    #[arg(long, alias = "force-reinstall", overrides_with("no_reinstall"))]
    pub(crate) reinstall: bool,

    #[arg(long, overrides_with("reinstall"), hide = true)]
    pub(crate) no_reinstall: bool,

    /// Reinstall a specific package, regardless of whether it's already installed.
    #[arg(long)]
    pub(crate) reinstall_package: Vec<PackageName>,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, `uv` will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-match`. This prevents
    /// "dependency confusion" attacks, whereby an attack can upload a malicious package under the
    /// same name to a secondary
    #[arg(long, value_enum, env = "UV_INDEX_STRATEGY")]
    pub(crate) index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures `uv` to
    /// use the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(long, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub(crate) keyring_provider: Option<KeyringProviderType>,

    /// The strategy to use when selecting between the different compatible versions for a given
    /// package requirement.
    ///
    /// By default, `uv` will use the latest compatible version of each package (`highest`).
    #[arg(long, value_enum, env = "UV_RESOLUTION")]
    pub(crate) resolution: Option<ResolutionMode>,

    /// The strategy to use when considering pre-release versions.
    ///
    /// By default, `uv` will accept pre-releases for packages that _only_ publish pre-releases,
    /// along with first-party requirements that contain an explicit pre-release marker in the
    /// declared specifiers (`if-necessary-or-explicit`).
    #[arg(long, value_enum, env = "UV_PRERELEASE")]
    pub(crate) prerelease: Option<PreReleaseMode>,

    #[arg(long, hide = true)]
    pub(crate) pre: bool,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[arg(long, short = 'C', alias = "config-settings")]
    pub(crate) config_setting: Option<Vec<ConfigSettingEntry>>,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, env = "UV_EXCLUDE_NEWER")]
    pub(crate) exclude_newer: Option<ExcludeNewer>,

    /// The method to use when installing packages from the global cache.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[arg(long, value_enum, env = "UV_LINK_MODE")]
    pub(crate) link_mode: Option<install_wheel_rs::linker::LinkMode>,

    /// Compile Python files to bytecode.
    ///
    /// By default, does not compile Python (`.py`) files to bytecode (`__pycache__/*.pyc`), instead
    /// Python lazily does the compilation the first time a module is imported. In cases where the
    /// first start time matters, such as CLI applications and docker containers, this option can
    /// trade longer install time for faster startup.
    ///
    /// The compile option will process the entire site-packages directory for consistency and
    /// (like pip) ignore all errors.
    #[arg(long, alias = "compile", overrides_with("no_compile_bytecode"))]
    pub(crate) compile_bytecode: bool,

    #[arg(
        long,
        alias = "no_compile",
        overrides_with("compile_bytecode"),
        hide = true
    )]
    pub(crate) no_compile_bytecode: bool,
}
