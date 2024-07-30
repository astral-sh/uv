use std::ffi::OsString;
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{anyhow, Result};
use clap::{Args, Parser, Subcommand};

use distribution_types::{FlatIndexLocation, IndexUrl};
use pep508_rs::Requirement;
use pypi_types::VerbatimParsedUrl;
use uv_cache::CacheArgs;
use uv_configuration::{
    ConfigSettingEntry, IndexStrategy, KeyringProviderType, PackageNameSpecifier, TargetTriple,
};
use uv_normalize::{ExtraName, PackageName};
use uv_python::{PythonFetch, PythonPreference, PythonVersion};
use uv_resolver::{AnnotationStyle, ExcludeNewer, PreReleaseMode, ResolutionMode};

pub mod compat;
pub mod options;
pub mod version;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum VersionFormat {
    /// Display the version as plain text.
    Text,
    /// Display the version as JSON.
    Json,
}

#[derive(Debug, Default, Clone, clap::ValueEnum)]
pub enum ListFormat {
    /// Display the list of packages in a human-readable table.
    #[default]
    Columns,
    /// Display the list of packages in a `pip freeze`-like format, with one package per line
    /// alongside its version.
    Freeze,
    /// Display the list of packages in a machine-readable JSON format.
    Json,
}

fn extra_name_with_clap_error(arg: &str) -> Result<ExtraName> {
    ExtraName::from_str(arg).map_err(|_err| {
        anyhow!(
            "Extra names must start and end with a letter or digit and may only \
            contain -, _, ., and alphanumeric characters"
        )
    })
}

#[derive(Parser)]
#[command(name = "uv", author, version = uv_version::version(), long_version = crate::version::version())]
#[command(about = "An extremely fast Python package manager.")]
#[command(propagate_version = true)]
#[command(
    after_help = "Use `uv help` for more details.",
    after_long_help = "",
    disable_help_flag = true,
    disable_help_subcommand = true
)]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Box<Commands>,

    #[command(flatten)]
    pub global_args: Box<GlobalArgs>,

    #[command(flatten)]
    pub cache_args: Box<CacheArgs>,

    /// The path to a `uv.toml` file to use for configuration.
    #[arg(global = true, long, env = "UV_CONFIG_FILE")]
    pub config_file: Option<PathBuf>,

    /// Avoid discovering configuration files (`pyproject.toml`, `uv.toml`) in the current directory,
    /// parent directories, or user configuration directories.
    #[arg(global = true, long, env = "UV_NO_CONFIG", value_parser = clap::builder::BoolishValueParser::new())]
    pub no_config: bool,

    /// Print help.
    #[arg(global = true, short, long, action = clap::ArgAction::HelpShort)]
    help: Option<bool>,
}

#[derive(Parser, Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct GlobalArgs {
    /// Do not print any output.
    #[arg(global = true, long, short, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Use verbose output.
    ///
    /// You can configure fine-grained logging using the `RUST_LOG` environment variable.
    /// (<https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives>)
    #[arg(global = true, action = clap::ArgAction::Count, long, short, conflicts_with = "quiet")]
    pub verbose: u8,

    /// Disable colors; provided for compatibility with `pip`.
    #[arg(global = true, long, hide = true, conflicts_with = "color")]
    pub no_color: bool,

    /// Control colors in output.
    #[arg(
        global = true,
        long,
        value_enum,
        default_value = "auto",
        conflicts_with = "no_color",
        value_name = "COLOR_CHOICE"
    )]
    pub color: ColorChoice,

    /// Whether to load TLS certificates from the platform's native certificate store.
    ///
    /// By default, uv loads certificates from the bundled `webpki-roots` crate. The
    /// `webpki-roots` are a reliable set of trust roots from Mozilla, and including them in uv
    /// improves portability and performance (especially on macOS).
    ///
    /// However, in some cases, you may want to use the platform's native certificate store,
    /// especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's
    /// included in your system's certificate store.
    #[arg(global = true, long, env = "UV_NATIVE_TLS", value_parser = clap::builder::BoolishValueParser::new(), overrides_with("no_native_tls"))]
    pub native_tls: bool,

    #[arg(global = true, long, overrides_with("native_tls"), hide = true)]
    pub no_native_tls: bool,

    /// Disable network access, relying only on locally cached data and locally available files.
    #[arg(global = true, long, overrides_with("no_offline"))]
    pub offline: bool,

    #[arg(global = true, long, overrides_with("offline"), hide = true)]
    pub no_offline: bool,

    /// Whether to prefer using Python installations that are already present on the system, or
    /// those that are downloaded and installed by uv.
    #[arg(global = true, long)]
    pub python_preference: Option<PythonPreference>,

    /// Whether to automatically download Python when required.
    #[arg(global = true, long)]
    pub python_fetch: Option<PythonFetch>,

    /// Whether to enable experimental, preview features.
    #[arg(global = true, long, hide = true, env = "UV_PREVIEW", value_parser = clap::builder::BoolishValueParser::new(), overrides_with("no_preview"))]
    pub preview: bool,

    #[arg(global = true, long, overrides_with("preview"), hide = true)]
    pub no_preview: bool,

    /// Avoid discovering a `pyproject.toml` or `uv.toml` file in the current directory or any
    /// parent directories.
    #[arg(global = true, long, hide = true)]
    pub isolated: bool,

    /// Show the resolved settings for the current command.
    #[arg(global = true, long, hide = true)]
    pub show_settings: bool,

    /// Hides all progress outputs when set
    #[arg(global = true, long)]
    pub no_progress: bool,

    /// Change to the given directory prior to running the command.
    #[arg(global = true, long, hide = true)]
    pub directory: Option<PathBuf>,
}

#[derive(Debug, Copy, Clone, clap::ValueEnum)]
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
pub enum Commands {
    /// Resolve and install Python packages.
    #[command(
        after_help = "Use `uv help pip`` for more details.",
        after_long_help = ""
    )]
    Pip(PipNamespace),
    /// Run and manage executable Python packages.
    #[command(
        after_help = "Use `uv help tool` for more details.",
        after_long_help = ""
    )]
    Tool(ToolNamespace),
    /// Manage Python installations.
    #[command(
        after_help = "Use `uv help python` for more details.",
        after_long_help = ""
    )]
    Python(PythonNamespace),
    /// Manage Python projects.
    #[command(flatten)]
    Project(Box<ProjectCommand>),
    /// Create a virtual environment.
    #[command(
        alias = "virtualenv",
        alias = "v",
        after_help = "Use `uv help venv` for more details.",
        after_long_help = ""
    )]
    Venv(VenvArgs),
    /// Manage the cache.
    #[command(
        after_help = "Use `uv help cache` for more details.",
        after_long_help = ""
    )]
    Cache(CacheNamespace),
    /// Manage the uv executable.
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
    /// Display documentation for a command.
    #[command(help_template = "\
{about-with-newline}
{usage-heading} {usage}
")]
    Help(HelpArgs),
}

#[derive(Args, Debug)]
pub struct HelpArgs {
    /// Disable pager when printing help
    #[arg(long)]
    pub no_pager: bool,

    pub command: Option<Vec<String>>,
}

#[derive(Args)]
#[cfg(feature = "self-update")]
pub struct SelfNamespace {
    #[command(subcommand)]
    pub command: SelfCommand,
}

#[derive(Subcommand)]
#[cfg(feature = "self-update")]
pub enum SelfCommand {
    /// Update uv to the latest version.
    Update,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct CacheNamespace {
    #[command(subcommand)]
    pub command: CacheCommand,
}

#[derive(Subcommand)]
pub enum CacheCommand {
    /// Clear the cache, removing all entries or those linked to specific packages.
    Clean(CleanArgs),
    /// Prune all unreachable objects from the cache.
    Prune(PruneArgs),
    /// Show the cache directory.
    Dir,
}

#[derive(Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct CleanArgs {
    /// The packages to remove from the cache.
    pub package: Vec<PackageName>,
}

#[derive(Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct PruneArgs {
    /// Optimize the cache for persistence in a continuous integration environment, like GitHub
    /// Actions.
    ///
    /// By default, uv caches both the wheels that it builds from source and the pre-built wheels
    /// that it downloads directly, to enable high-performance package installation. In some
    /// scenarios, though, persisting pre-built wheels may be undesirable. For example, in GitHub
    /// Actions, it's faster to omit pre-built wheels from the cache and instead have re-download
    /// them on each run. However, it typically _is_ faster to cache wheels that are built from
    /// source, since the wheel building process can be expensive, especially for extension
    /// modules.
    ///
    /// In `--ci` mode, uv will prune any pre-built wheels from the cache, but retain any wheels
    /// that were built from source.
    #[arg(long)]
    pub ci: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PipNamespace {
    #[command(subcommand)]
    pub command: PipCommand,
}

#[derive(Subcommand)]
pub enum PipCommand {
    /// Compile a `requirements.in` file to a `requirements.txt` file.
    #[command(
        after_help = "Use `uv help pip compile` for more details.",
        after_long_help = ""
    )]
    Compile(PipCompileArgs),
    /// Sync an environment with a `requirements.txt` file.
    #[command(
        after_help = "Use `uv help pip sync` for more details.",
        after_long_help = ""
    )]
    Sync(Box<PipSyncArgs>),
    /// Install packages into an environment.
    #[command(
        after_help = "Use `uv help pip install` for more details.",
        after_long_help = ""
    )]
    Install(PipInstallArgs),
    /// Uninstall packages from an environment.
    #[command(
        after_help = "Use `uv help pip uninstall` for more details.",
        after_long_help = ""
    )]
    Uninstall(PipUninstallArgs),
    /// List, in requirements format, packages installed in an environment.
    #[command(
        after_help = "Use `uv help pip freeze` for more details.",
        after_long_help = ""
    )]
    Freeze(PipFreezeArgs),
    /// List, in tabular format, packages installed in an environment.
    #[command(
        after_help = "Use `uv help pip list` for more details.",
        after_long_help = ""
    )]
    List(PipListArgs),
    /// Show information about one or more installed packages.
    #[command(
        after_help = "Use `uv help pip show` for more details.",
        after_long_help = ""
    )]
    Show(PipShowArgs),
    /// Display the dependency tree for an environment.
    #[command(
        after_help = "Use `uv help pip tree` for more details.",
        after_long_help = ""
    )]
    Tree(PipTreeArgs),
    /// Verify installed packages have compatible dependencies.
    #[command(
        after_help = "Use `uv help pip check` for more details.",
        after_long_help = ""
    )]
    Check(PipCheckArgs),
}

#[derive(Subcommand)]
pub enum ProjectCommand {
    /// Initialize a project.
    #[clap(hide = true)]
    Init(InitArgs),
    /// Run a command in the project environment.
    #[clap(hide = true)]
    #[command(
        after_help = "Use `uv help run` for more details.",
        after_long_help = ""
    )]
    Run(RunArgs),
    /// Sync the project's dependencies with the environment.
    #[clap(hide = true)]
    #[command(
        after_help = "Use `uv help sync` for more details.",
        after_long_help = ""
    )]
    Sync(SyncArgs),
    /// Resolve the project requirements into a lockfile.
    #[clap(hide = true)]
    #[command(
        after_help = "Use `uv help lock` for more details.",
        after_long_help = ""
    )]
    Lock(LockArgs),
    /// Add one or more packages to the project requirements.
    #[clap(hide = true)]
    #[command(
        after_help = "Use `uv help add` for more details.",
        after_long_help = ""
    )]
    Add(AddArgs),
    /// Remove one or more packages from the project requirements.
    #[clap(hide = true)]
    #[command(
        after_help = "Use `uv help remove` for more details.",
        after_long_help = ""
    )]
    Remove(RemoveArgs),
    /// Display the dependency tree for the project.
    #[clap(hide = true)]
    Tree(TreeArgs),
}

/// A re-implementation of `Option`, used to avoid Clap's automatic `Option` flattening in
/// [`parse_index_url`].
#[derive(Debug, Clone)]
pub enum Maybe<T> {
    Some(T),
    None,
}

impl<T> Maybe<T> {
    pub fn into_option(self) -> Option<T> {
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
pub struct PipCompileArgs {
    /// Include all packages listed in the given `requirements.in` files.
    ///
    /// If a `pyproject.toml`, `setup.py`, or `setup.cfg` file is provided, uv will
    /// extract the requirements for the relevant project.
    ///
    /// If `-` is provided, then requirements will be read from stdin.
    #[arg(required(true), value_parser = parse_file_path)]
    pub src_file: Vec<PathBuf>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[arg(long, short, env = "UV_CONSTRAINT", value_delimiter = ' ', value_parser = parse_maybe_file_path)]
    pub constraint: Vec<Maybe<PathBuf>>,

    /// Override versions using the given requirements files.
    ///
    /// Overrides files are `requirements.txt`-like files that force a specific version of a
    /// requirement to be installed, regardless of the requirements declared by any constituent
    /// package, and regardless of whether this would be considered an invalid resolution.
    ///
    /// While constraints are _additive_, in that they're combined with the requirements of the
    /// constituent packages, overrides are _absolute_, in that they completely replace the
    /// requirements of the constituent packages.
    #[arg(long, env = "UV_OVERRIDE", value_delimiter = ' ', value_parser = parse_maybe_file_path)]
    pub r#override: Vec<Maybe<PathBuf>>,

    /// Include optional dependencies from the extra group name; may be provided more than once.
    ///
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    ///
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "extra")]
    pub all_extras: bool,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub no_all_extras: bool,

    #[command(flatten)]
    pub resolver: ResolverArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Ignore package dependencies, instead only add those packages explicitly listed
    /// on the command line to the resulting the requirements file.
    #[arg(long)]
    pub no_deps: bool,

    #[arg(long, overrides_with("no_deps"), hide = true)]
    pub deps: bool,

    /// Write the compiled requirements to the given `requirements.txt` file.
    ///
    /// If the file already exists, the existing versions will be preferred when resolving
    /// dependencies, unless `--upgrade` is also specified.
    #[arg(long, short)]
    pub output_file: Option<PathBuf>,

    /// Include extras in the output file.
    ///
    /// By default, uv strips extras, as any packages pulled in by the extras are already included
    /// as dependencies in the output file directly. Further, output files generated with
    /// `--no-strip-extras` cannot be used as constraints files in `install` and `sync` invocations.
    #[arg(long, overrides_with("strip_extras"))]
    pub no_strip_extras: bool,

    #[arg(long, overrides_with("no_strip_extras"), hide = true)]
    pub strip_extras: bool,

    /// Include environment markers in the output file.
    ///
    /// By default, uv strips environment markers, as the resolution generated by `compile` is
    /// only guaranteed to be correct for the target environment.
    #[arg(long, overrides_with("strip_markers"))]
    pub no_strip_markers: bool,

    #[arg(long, overrides_with("no_strip_markers"), hide = true)]
    pub strip_markers: bool,

    /// Exclude comment annotations indicating the source of each package.
    #[arg(long, overrides_with("annotate"))]
    pub no_annotate: bool,

    #[arg(long, overrides_with("no_annotate"), hide = true)]
    pub annotate: bool,

    /// Exclude the comment header at the top of the generated output file.
    #[arg(long, overrides_with("header"))]
    pub no_header: bool,

    #[arg(long, overrides_with("no_header"), hide = true)]
    pub header: bool,

    /// The style of the annotation comments included in the output file, used to indicate the
    /// source of each package.
    ///
    /// Defaults to `split`.
    #[arg(long, value_enum)]
    pub annotation_style: Option<AnnotationStyle>,

    /// The header comment to include at the top of the output file generated by `uv pip compile`.
    ///
    /// Used to reflect custom build scripts and commands that wrap `uv pip compile`.
    #[arg(long, env = "UV_CUSTOM_COMPILE_COMMAND")]
    pub custom_compile_command: Option<String>,

    /// The Python interpreter against which to compile the requirements.
    ///
    /// By default, uv uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--python`
    /// option allows you to specify a different interpreter.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, verbatim_doc_comment)]
    pub python: Option<String>,

    /// Install packages into the system Python environment.
    ///
    /// By default, uv uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--system`
    /// option instructs uv to avoid using a virtual environment Python and restrict its search to
    /// the system path.
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    /// Include distribution hashes in the output file.
    #[arg(long, overrides_with("no_generate_hashes"))]
    pub generate_hashes: bool,

    #[arg(long, overrides_with("generate_hashes"), hide = true)]
    pub no_generate_hashes: bool,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[arg(long, overrides_with("no_legacy_setup_py"))]
    pub legacy_setup_py: bool,

    #[arg(long, overrides_with("legacy_setup_py"), hide = true)]
    pub no_legacy_setup_py: bool,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[arg(
        long,
        env = "UV_NO_BUILD_ISOLATION",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("build_isolation")
    )]
    pub no_build_isolation: bool,

    #[arg(long, overrides_with("no_build_isolation"), hide = true)]
    pub build_isolation: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary Python code. The cached wheels of
    /// already-built source distributions will be reused, but operations that require building
    /// distributions will exit with an error.
    ///
    /// Alias for `--only-binary :all:`.
    #[arg(
        long,
        conflicts_with = "no_binary",
        conflicts_with = "only_binary",
        overrides_with("build")
    )]
    pub no_build: bool,

    #[arg(
        long,
        conflicts_with = "no_binary",
        conflicts_with = "only_binary",
        overrides_with("no_build"),
        hide = true
    )]
    pub build: bool,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be built and installed from source. The resolver will still use
    /// pre-built wheels to extract package metadata, if available.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[arg(long, conflicts_with = "no_build")]
    pub no_binary: Option<Vec<PackageNameSpecifier>>,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[arg(long, conflicts_with = "no_build")]
    pub only_binary: Option<Vec<PackageNameSpecifier>>,

    /// The minimum Python version that should be supported by the resolved requirements (e.g.,
    /// `3.8` or `3.8.17`).
    ///
    /// If a patch version is omitted, the minimum patch version is assumed. For example, `3.8` is
    /// mapped to `3.8.0`.
    #[arg(long, short)]
    pub python_version: Option<PythonVersion>,

    /// The platform for which requirements should be resolved.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aaarch64-apple-darwin`.
    #[arg(long)]
    pub python_platform: Option<TargetTriple>,

    /// Perform a universal resolution, attempting to generate a single `requirements.txt` output
    /// file that is compatible with all operating systems, architectures, and Python
    /// implementations.
    ///
    /// In universal mode, the current Python version (or user-provided `--python-version`) will be
    /// treated as a lower bound. For example, `--universal --python-version 3.7` would produce a
    /// universal resolution for Python 3.7 and later.
    ///
    /// Implies `--no-strip-markers`.
    #[arg(
        long,
        overrides_with("no_universal"),
        conflicts_with("python_platform"),
        conflicts_with("strip_markers")
    )]
    pub universal: bool,

    #[arg(long, overrides_with("universal"), hide = true)]
    pub no_universal: bool,

    /// Specify a package to omit from the output resolution. Its dependencies will still be
    /// included in the resolution. Equivalent to pip-compile's `--unsafe-package` option.
    #[arg(long, alias = "unsafe-package")]
    pub no_emit_package: Option<Vec<PackageName>>,

    /// Include `--index-url` and `--extra-index-url` entries in the generated output file.
    #[arg(long, overrides_with("no_emit_index_url"))]
    pub emit_index_url: bool,

    #[arg(long, overrides_with("emit_index_url"), hide = true)]
    pub no_emit_index_url: bool,

    /// Include `--find-links` entries in the generated output file.
    #[arg(long, overrides_with("no_emit_find_links"))]
    pub emit_find_links: bool,

    #[arg(long, overrides_with("emit_find_links"), hide = true)]
    pub no_emit_find_links: bool,

    /// Include `--no-binary` and `--only-binary` entries in the generated output file.
    #[arg(long, overrides_with("no_emit_build_options"))]
    pub emit_build_options: bool,

    #[arg(long, overrides_with("emit_build_options"), hide = true)]
    pub no_emit_build_options: bool,

    /// Whether to emit a marker string indicating when it is known that the
    /// resulting set of pinned dependencies is valid.
    ///
    /// The pinned dependencies may be valid even when the marker expression is
    /// false, but when the expression is true, the requirements are known to
    /// be correct.
    #[arg(long, overrides_with("no_emit_marker_expression"), hide = true)]
    pub emit_marker_expression: bool,

    #[arg(long, overrides_with("emit_marker_expression"), hide = true)]
    pub no_emit_marker_expression: bool,

    /// Include comment annotations indicating the index used to resolve each package (e.g.,
    /// `# from https://pypi.org/simple`).
    #[arg(long, overrides_with("no_emit_index_annotation"))]
    pub emit_index_annotation: bool,

    #[arg(long, overrides_with("emit_index_annotation"), hide = true)]
    pub no_emit_index_annotation: bool,

    #[command(flatten)]
    pub compat_args: compat::PipCompileCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PipSyncArgs {
    /// Include all packages listed in the given `requirements.txt` files.
    ///
    /// If a `pyproject.toml`, `setup.py`, or `setup.cfg` file is provided, uv will
    /// extract the requirements for the relevant project.
    ///
    /// If `-` is provided, then requirements will be read from stdin.
    #[arg(required(true), value_parser = parse_file_path)]
    pub src_file: Vec<PathBuf>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[arg(long, short, env = "UV_CONSTRAINT", value_delimiter = ' ', value_parser = parse_maybe_file_path)]
    pub constraint: Vec<Maybe<PathBuf>>,

    #[command(flatten)]
    pub installer: InstallerArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Require a matching hash for each requirement.
    ///
    /// Hash-checking mode is all or nothing. If enabled, _all_ requirements must be provided
    /// with a corresponding hash or set of hashes. Additionally, if enabled, _all_ requirements
    /// must either be pinned to exact versions (e.g., `==1.0.0`), or be specified via direct URL.
    ///
    /// Hash-checking mode introduces a number of additional constraints:
    ///
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
    pub require_hashes: bool,

    #[arg(long, overrides_with("require_hashes"), hide = true)]
    pub no_require_hashes: bool,

    /// Validate any hashes provided in the requirements file.
    ///
    /// Unlike `--require-hashes`, `--verify-hashes` does not require that all requirements have
    /// hashes; instead, it will limit itself to verifying the hashes of those requirements that do
    /// include them.
    #[arg(
        long,
        env = "UV_VERIFY_HASHES",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_verify_hashes"),
    )]
    pub verify_hashes: bool,

    #[arg(long, overrides_with("verify_hashes"), hide = true)]
    pub no_verify_hashes: bool,

    /// The Python interpreter into which packages should be installed.
    ///
    /// By default, uv installs into the virtual environment in the current working directory or
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
    pub python: Option<String>,

    /// Install packages into the system Python environment.
    ///
    /// By default, uv installs into the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs uv to instead use the first Python
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
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    /// Allow uv to modify an `EXTERNALLY-MANAGED` Python installation.
    ///
    /// WARNING: `--break-system-packages` is intended for use in continuous integration (CI)
    /// environments, when installing into Python installations that are managed by an external
    /// package manager, like `apt`. It should be used with caution, as such Python installations
    /// explicitly recommend against modifications by other package managers (like uv or `pip`).
    #[arg(
        long,
        env = "UV_BREAK_SYSTEM_PACKAGES",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_break_system_packages")
    )]
    pub break_system_packages: bool,

    #[arg(long, overrides_with("break_system_packages"))]
    pub no_break_system_packages: bool,

    /// Install packages into the specified directory, rather than into the virtual or system Python
    /// environment. The packages will be installed at the top-level of the directory.
    #[arg(long, conflicts_with = "prefix")]
    pub target: Option<PathBuf>,

    /// Install packages into `lib`, `bin`, and other top-level folders under the specified
    /// directory, as if a virtual environment were present at that location.
    ///
    /// In general, prefer the use of `--python` to install into an alternate environment, as
    /// scripts and other artifacts installed via `--prefix` will reference the installing
    /// interpreter, rather than any interpreter added to the `--prefix` directory, rendering them
    /// non-portable.
    #[arg(long, conflicts_with = "target")]
    pub prefix: Option<PathBuf>,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[arg(long, overrides_with("no_legacy_setup_py"))]
    pub legacy_setup_py: bool,

    #[arg(long, overrides_with("legacy_setup_py"), hide = true)]
    pub no_legacy_setup_py: bool,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[arg(
        long,
        env = "UV_NO_BUILD_ISOLATION",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("build_isolation")
    )]
    pub no_build_isolation: bool,

    #[arg(long, overrides_with("no_build_isolation"), hide = true)]
    pub build_isolation: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary Python code. The cached wheels of
    /// already-built source distributions will be reused, but operations that require building
    /// distributions will exit with an error.
    ///
    /// Alias for `--only-binary :all:`.
    #[arg(
        long,
        conflicts_with = "no_binary",
        conflicts_with = "only_binary",
        overrides_with("build")
    )]
    pub no_build: bool,

    #[arg(
        long,
        conflicts_with = "no_binary",
        conflicts_with = "only_binary",
        overrides_with("no_build"),
        hide = true
    )]
    pub build: bool,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be built and installed from source. The resolver will still use
    /// pre-built wheels to extract package metadata, if available.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[arg(long, conflicts_with = "no_build")]
    pub no_binary: Option<Vec<PackageNameSpecifier>>,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[arg(long, conflicts_with = "no_build")]
    pub only_binary: Option<Vec<PackageNameSpecifier>>,

    /// Allow sync of empty requirements, which will clear the environment of all packages.
    #[arg(long, overrides_with("no_allow_empty_requirements"))]
    pub allow_empty_requirements: bool,

    #[arg(long, overrides_with("allow_empty_requirements"))]
    pub no_allow_empty_requirements: bool,

    /// The minimum Python version that should be supported by the requirements (e.g.,
    /// `3.7` or `3.7.9`).
    ///
    /// If a patch version is omitted, the minimum patch version is assumed. For example, `3.7` is
    /// mapped to `3.7.0`.
    #[arg(long)]
    pub python_version: Option<PythonVersion>,

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
    pub python_platform: Option<TargetTriple>,

    /// Validate the Python environment after completing the installation, to detect and with
    /// missing dependencies or other issues.
    #[arg(long, overrides_with("no_strict"))]
    pub strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub no_strict: bool,

    /// Perform a dry run, i.e., don't actually install anything but resolve the dependencies and
    /// print the resulting plan.
    #[arg(long)]
    pub dry_run: bool,

    #[command(flatten)]
    pub compat_args: compat::PipSyncCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
pub struct PipInstallArgs {
    /// Install all listed packages.
    #[arg(group = "sources")]
    pub package: Vec<String>,

    /// Install all packages listed in the given `requirements.txt` files.
    ///
    /// If a `pyproject.toml`, `setup.py`, or `setup.cfg` file is provided, uv will
    /// extract the requirements for the relevant project.
    ///
    /// If `-` is provided, then requirements will be read from stdin.
    #[arg(long, short, group = "sources", value_parser = parse_file_path)]
    pub requirement: Vec<PathBuf>,

    /// Install the editable package based on the provided local file path.
    #[arg(long, short, group = "sources")]
    pub editable: Vec<String>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[arg(long, short, env = "UV_CONSTRAINT", value_delimiter = ' ', value_parser = parse_maybe_file_path)]
    pub constraint: Vec<Maybe<PathBuf>>,

    /// Override versions using the given requirements files.
    ///
    /// Overrides files are `requirements.txt`-like files that force a specific version of a
    /// requirement to be installed, regardless of the requirements declared by any constituent
    /// package, and regardless of whether this would be considered an invalid resolution.
    ///
    /// While constraints are _additive_, in that they're combined with the requirements of the
    /// constituent packages, overrides are _absolute_, in that they completely replace the
    /// requirements of the constituent packages.
    #[arg(long, env = "UV_OVERRIDE", value_delimiter = ' ', value_parser = parse_maybe_file_path)]
    pub r#override: Vec<Maybe<PathBuf>>,

    /// Include optional dependencies from the extra group name; may be provided more than once.
    ///
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    ///
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "extra", overrides_with = "no_all_extras")]
    pub all_extras: bool,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub no_all_extras: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Ignore package dependencies, instead only installing those packages explicitly listed
    /// on the command line or in the requirements files.
    #[arg(long, overrides_with("deps"))]
    pub no_deps: bool,

    #[arg(long, overrides_with("no_deps"), hide = true)]
    pub deps: bool,

    /// Require a matching hash for each requirement.
    ///
    /// Hash-checking mode is all or nothing. If enabled, _all_ requirements must be provided
    /// with a corresponding hash or set of hashes. Additionally, if enabled, _all_ requirements
    /// must either be pinned to exact versions (e.g., `==1.0.0`), or be specified via direct URL.
    ///
    /// Hash-checking mode introduces a number of additional constraints:
    ///
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
    pub require_hashes: bool,

    #[arg(long, overrides_with("require_hashes"), hide = true)]
    pub no_require_hashes: bool,

    /// Validate any hashes provided in the requirements file.
    ///
    /// Unlike `--require-hashes`, `--verify-hashes` does not require that all requirements have
    /// hashes; instead, it will limit itself to verifying the hashes of those requirements that do
    /// include them.
    #[arg(
        long,
        env = "UV_VERIFY_HASHES",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_verify_hashes"),
    )]
    pub verify_hashes: bool,

    #[arg(long, overrides_with("verify_hashes"), hide = true)]
    pub no_verify_hashes: bool,

    /// The Python interpreter into which packages should be installed.
    ///
    /// By default, uv installs into the virtual environment in the current working directory or
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
    pub python: Option<String>,

    /// Install packages into the system Python environment.
    ///
    /// By default, uv installs into the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs uv to instead use the first Python
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
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    /// Allow uv to modify an `EXTERNALLY-MANAGED` Python installation.
    ///
    /// WARNING: `--break-system-packages` is intended for use in continuous integration (CI)
    /// environments, when installing into Python installations that are managed by an external
    /// package manager, like `apt`. It should be used with caution, as such Python installations
    /// explicitly recommend against modifications by other package managers (like uv or `pip`).
    #[arg(
        long,
        env = "UV_BREAK_SYSTEM_PACKAGES",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_break_system_packages")
    )]
    pub break_system_packages: bool,

    #[arg(long, overrides_with("break_system_packages"))]
    pub no_break_system_packages: bool,

    /// Install packages into the specified directory, rather than into the virtual or system Python
    /// environment. The packages will be installed at the top-level of the directory.
    #[arg(long, conflicts_with = "prefix")]
    pub target: Option<PathBuf>,

    /// Install packages into `lib`, `bin`, and other top-level folders under the specified
    /// directory, as if a virtual environment were present at that location.
    ///
    /// In general, prefer the use of `--python` to install into an alternate environment, as
    /// scripts and other artifacts installed via `--prefix` will reference the installing
    /// interpreter, rather than any interpreter added to the `--prefix` directory, rendering them
    /// non-portable.
    #[arg(long, conflicts_with = "target")]
    pub prefix: Option<PathBuf>,

    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[arg(long, overrides_with("no_legacy_setup_py"))]
    pub legacy_setup_py: bool,

    #[arg(long, overrides_with("legacy_setup_py"), hide = true)]
    pub no_legacy_setup_py: bool,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[arg(
        long,
        env = "UV_NO_BUILD_ISOLATION",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("build_isolation")
    )]
    pub no_build_isolation: bool,

    #[arg(long, overrides_with("no_build_isolation"), hide = true)]
    pub build_isolation: bool,

    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary Python code. The cached wheels of
    /// already-built source distributions will be reused, but operations that require building
    /// distributions will exit with an error.
    ///
    /// Alias for `--only-binary :all:`.
    #[arg(
        long,
        conflicts_with = "no_binary",
        conflicts_with = "only_binary",
        overrides_with("build")
    )]
    pub no_build: bool,

    #[arg(
        long,
        conflicts_with = "no_binary",
        conflicts_with = "only_binary",
        overrides_with("no_build"),
        hide = true
    )]
    pub build: bool,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be built and installed from source. The resolver will still use
    /// pre-built wheels to extract package metadata, if available.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[arg(long, conflicts_with = "no_build")]
    pub no_binary: Option<Vec<PackageNameSpecifier>>,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[arg(long, conflicts_with = "no_build")]
    pub only_binary: Option<Vec<PackageNameSpecifier>>,

    /// The minimum Python version that should be supported by the requirements (e.g.,
    /// `3.7` or `3.7.9`).
    ///
    /// If a patch version is omitted, the minimum patch version is assumed. For example, `3.7` is
    /// mapped to `3.7.0`.
    #[arg(long)]
    pub python_version: Option<PythonVersion>,

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
    pub python_platform: Option<TargetTriple>,

    /// Validate the Python environment after completing the installation, to detect and with
    /// missing dependencies or other issues.
    #[arg(long, overrides_with("no_strict"))]
    pub strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub no_strict: bool,

    /// Perform a dry run, i.e., don't actually install anything but resolve the dependencies and
    /// print the resulting plan.
    #[arg(long)]
    pub dry_run: bool,

    #[command(flatten)]
    pub compat_args: compat::PipInstallCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
pub struct PipUninstallArgs {
    /// Uninstall all listed packages.
    #[arg(group = "sources")]
    pub package: Vec<String>,

    /// Uninstall all packages listed in the given requirements files.
    #[arg(long, short, group = "sources", value_parser = parse_file_path)]
    pub requirement: Vec<PathBuf>,

    /// The Python interpreter from which packages should be uninstalled.
    ///
    /// By default, uv uninstalls from the virtual environment in the current working directory or
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
    pub python: Option<String>,

    /// Attempt to use `keyring` for authentication for remote requirements files.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to
    /// use the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(long, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub keyring_provider: Option<KeyringProviderType>,

    /// Use the system Python to uninstall packages.
    ///
    /// By default, uv uninstalls from the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs uv to instead use the first Python
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
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    /// Allow uv to modify an `EXTERNALLY-MANAGED` Python installation.
    ///
    /// WARNING: `--break-system-packages` is intended for use in continuous integration (CI)
    /// environments, when installing into Python installations that are managed by an external
    /// package manager, like `apt`. It should be used with caution, as such Python installations
    /// explicitly recommend against modifications by other package managers (like uv or `pip`).
    #[arg(
        long,
        env = "UV_BREAK_SYSTEM_PACKAGES",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_break_system_packages")
    )]
    pub break_system_packages: bool,

    #[arg(long, overrides_with("break_system_packages"))]
    pub no_break_system_packages: bool,

    /// Uninstall packages from the specified `--target` directory.
    #[arg(long, conflicts_with = "prefix")]
    pub target: Option<PathBuf>,

    /// Uninstall packages from the specified `--prefix` directory.
    #[arg(long, conflicts_with = "target")]
    pub prefix: Option<PathBuf>,

    #[command(flatten)]
    pub compat_args: compat::PipGlobalCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PipFreezeArgs {
    /// Exclude any editable packages from output.
    #[arg(long)]
    pub exclude_editable: bool,

    /// Validate the Python environment, to detect packages with missing dependencies and other
    /// issues.
    #[arg(long, overrides_with("no_strict"))]
    pub strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub no_strict: bool,

    /// The Python interpreter for which packages should be listed.
    ///
    /// By default, uv lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub python: Option<String>,

    /// List packages for the system Python.
    ///
    /// By default, uv lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found. The `--system` option
    /// instructs uv to use the first Python found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution.
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    #[command(flatten)]
    pub compat_args: compat::PipGlobalCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PipListArgs {
    /// Only include editable projects.
    #[arg(short, long)]
    pub editable: bool,

    /// Exclude any editable packages from output.
    #[arg(long, conflicts_with = "editable")]
    pub exclude_editable: bool,

    /// Exclude the specified package(s) from the output.
    #[arg(long)]
    pub r#exclude: Vec<PackageName>,

    /// Select the output format between: `columns` (default), `freeze`, or `json`.
    #[arg(long, value_enum, default_value_t = ListFormat::default())]
    pub format: ListFormat,

    /// Validate the Python environment, to detect packages with missing dependencies and other
    /// issues.
    #[arg(long, overrides_with("no_strict"))]
    pub strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub no_strict: bool,

    /// The Python interpreter for which packages should be listed.
    ///
    /// By default, uv lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub python: Option<String>,

    /// List packages for the system Python.
    ///
    /// By default, uv lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found. The `--system` option
    /// instructs uv to use the first Python found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution.
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    #[command(flatten)]
    pub compat_args: compat::PipListCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PipCheckArgs {
    /// The Python interpreter for which packages should be listed.
    ///
    /// By default, uv lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub python: Option<String>,

    /// List packages for the system Python.
    ///
    /// By default, uv lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found. The `--system` option
    /// instructs uv to use the first Python found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution.
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PipShowArgs {
    /// The package(s) to display.
    pub package: Vec<PackageName>,

    /// Validate the Python environment, to detect packages with missing dependencies and other
    /// issues.
    #[arg(long, overrides_with("no_strict"))]
    pub strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub no_strict: bool,

    /// The Python interpreter for which packages should be listed.
    ///
    /// By default, uv lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub python: Option<String>,

    /// List packages for the system Python.
    ///
    /// By default, uv lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found. The `--system` option
    /// instructs uv to use the first Python found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution.
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    #[command(flatten)]
    pub compat_args: compat::PipGlobalCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PipTreeArgs {
    #[command(flatten)]
    pub tree: DisplayTreeArgs,

    /// Validate the Python environment, to detect packages with missing dependencies and other
    /// issues.
    #[arg(long, overrides_with("no_strict"))]
    pub strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub no_strict: bool,

    /// The Python interpreter for which packages should be listed.
    ///
    /// By default, uv lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub python: Option<String>,

    /// List packages for the system Python.
    ///
    /// By default, uv lists packages in the currently activated virtual environment, or a virtual
    /// environment (`.venv`) located in the current working directory or any parent directory,
    /// falling back to the system Python if no virtual environment is found. The `--system` option
    /// instructs uv to use the first Python found in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution.
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub system: bool,

    #[arg(long, overrides_with("system"))]
    pub no_system: bool,

    #[command(flatten)]
    pub compat_args: compat::PipGlobalCompatArgs,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct VenvArgs {
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
    pub python: Option<String>,

    /// Use the system Python to uninstall packages.
    ///
    /// By default, uv uninstalls from the virtual environment in the current working directory or
    /// any parent directory. The `--system` option instructs uv to use the first Python found in
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
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    /// Install seed packages (one or more of: `pip`, `setuptools`, and `wheel`) into the virtual environment.
    ///
    /// Note `setuptools` and `wheel` are not included in Python 3.12+ environments.
    #[arg(long)]
    pub seed: bool,

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
    pub allow_existing: bool,

    /// The path to the virtual environment to create.
    #[arg(default_value = ".venv")]
    pub name: PathBuf,

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
    pub prompt: Option<String>,

    /// Give the virtual environment access to the system site packages directory.
    ///
    /// Unlike `pip`, when a virtual environment is created with `--system-site-packages`, uv will
    /// _not_ take system site packages into account when running commands like `uv pip list` or
    /// `uv pip install`. The `--system-site-packages` flag will provide the virtual environment
    /// with access to the system site packages directory at runtime, but it will not affect the
    /// behavior of uv commands.
    #[arg(long)]
    pub system_site_packages: bool,

    /// Make the virtual environment relocatable.
    ///
    /// A relocatable virtual environment can be moved around and redistributed without
    /// invalidating its associated entrypoint and activation scripts.
    ///
    /// Note that this can only be guaranteed for standard `console_scripts` and `gui_scripts`.
    /// Other scripts may be adjusted if they ship with a generic `#!python[w]` shebang,
    /// and binaries are left as-is.
    ///
    /// As a result of making the environment relocatable (by way of writing relative, rather than
    /// absolute paths), the entrypoints and scripts themselves will _not_ be relocatable. In other
    /// words, copying those entrypoints and scripts to a location outside the environment will not
    /// work, as they reference paths relative to the environment itself.
    #[arg(long)]
    pub relocatable: bool,

    #[command(flatten)]
    pub index_args: IndexArgs,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-match`). This prevents
    /// "dependency confusion" attacks, whereby an attack can upload a malicious package under the
    /// same name to a secondary.
    #[arg(long, value_enum, env = "UV_INDEX_STRATEGY")]
    pub index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to
    /// use the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(long, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub keyring_provider: Option<KeyringProviderType>,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, env = "UV_EXCLUDE_NEWER")]
    pub exclude_newer: Option<ExcludeNewer>,

    /// The method to use when installing packages from the global cache.
    ///
    /// This option is only used for installing seed packages.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[arg(long, value_enum, env = "UV_LINK_MODE")]
    pub link_mode: Option<install_wheel_rs::linker::LinkMode>,

    #[command(flatten)]
    pub compat_args: compat::VenvCompatArgs,
}

#[derive(Parser, Debug, Clone)]
pub enum ExternalCommand {
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
    pub fn split(&self) -> (Option<&OsString>, &[OsString]) {
        match self.as_slice() {
            [] => (None, &[]),
            [cmd, args @ ..] => (Some(cmd), args),
        }
    }
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct InitArgs {
    /// The path of the project.
    pub path: Option<String>,

    /// The name of the project, defaults to the name of the directory.
    #[arg(long)]
    pub name: Option<PackageName>,

    /// Create a virtual workspace instead of a project.
    #[arg(long)]
    pub r#virtual: bool,

    /// Do not create a `README.md` file.
    #[arg(long)]
    pub no_readme: bool,

    /// Avoid discovering the workspace in the current directory or any parent directory. Instead,
    /// create a standalone project.
    #[arg(long, alias = "no_project")]
    pub no_workspace: bool,

    /// The Python interpreter to use to determine the minimum supported Python version.
    ///
    /// By default, uv uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--python`
    /// option allows you to specify a different interpreter.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct RunArgs {
    /// Include optional dependencies from the extra group name; may be provided more than once.
    ///
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    ///
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "extra")]
    pub all_extras: bool,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub no_all_extras: bool,

    /// Include development dependencies.
    #[arg(long, overrides_with("no_dev"), hide = true)]
    pub dev: bool,

    /// Omit development dependencies.
    #[arg(long, overrides_with("dev"))]
    pub no_dev: bool,

    /// The command to run.
    #[command(subcommand)]
    pub command: ExternalCommand,

    /// Run with the given packages installed.
    #[arg(long)]
    pub with: Vec<String>,

    /// Run with all packages listed in the given `requirements.txt` files.
    ///
    /// Using `pyproject.toml`, `setup.py`, or `setup.cfg` files is not allowed.
    #[arg(long, value_parser = parse_maybe_file_path)]
    pub with_requirements: Vec<Maybe<PathBuf>>,

    /// Run the tool in an isolated virtual environment, rather than leveraging the base environment
    /// for the current project, to enforce strict isolation between dependencies.
    #[arg(long)]
    pub isolated: bool,

    /// Assert that the `uv.lock` will remain unchanged.
    #[arg(long, conflicts_with = "frozen")]
    pub locked: bool,

    /// Install without updating the `uv.lock` file.
    #[arg(long, conflicts_with = "locked")]
    pub frozen: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Run the command in a specific package in the workspace.
    #[arg(long)]
    pub package: Option<PackageName>,

    /// Avoid discovering the project or workspace in the current directory or any parent directory.
    /// Instead, run in an isolated, ephemeral environment populated by the `--with` requirements.
    #[arg(long, alias = "no_workspace", conflicts_with = "package")]
    pub no_project: bool,

    /// The Python interpreter to use to build the run environment.
    ///
    /// By default, uv uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--python`
    /// option allows you to specify a different interpreter.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub python: Option<String>,

    /// Whether to show resolver and installer output from any environment modifications.
    ///
    /// By default, environment modifications are omitted, but enabled under `--verbose`.
    #[arg(long, env = "UV_SHOW_RESOLUTION", value_parser = clap::builder::BoolishValueParser::new(), hide = true)]
    pub show_resolution: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct SyncArgs {
    /// Include optional dependencies from the extra group name; may be provided more than once.
    ///
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    ///
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "extra")]
    pub all_extras: bool,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub no_all_extras: bool,

    /// Include development dependencies.
    #[arg(long, overrides_with("no_dev"), hide = true)]
    pub dev: bool,

    /// Omit development dependencies.
    #[arg(long, overrides_with("dev"))]
    pub no_dev: bool,

    /// Does not clean the environment.
    ///
    /// When omitted, any extraneous installations will be removed.
    #[arg(long)]
    pub no_clean: bool,

    /// Assert that the `uv.lock` will remain unchanged.
    #[arg(long, conflicts_with = "frozen")]
    pub locked: bool,

    /// Install without updating the `uv.lock` file.
    #[arg(long, conflicts_with = "locked")]
    pub frozen: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// The Python interpreter to use to build the run environment.
    ///
    /// By default, uv uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--python`
    /// option allows you to specify a different interpreter.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct LockArgs {
    /// Assert that the `uv.lock` will remain unchanged.
    #[arg(long, conflicts_with = "frozen")]
    pub locked: bool,

    /// Assert that a `uv.lock` exists, without updating it.
    #[arg(long, conflicts_with = "locked")]
    pub frozen: bool,

    #[command(flatten)]
    pub resolver: ResolverArgs,

    #[command(flatten)]
    pub build: BuildArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// The Python interpreter to use to build the run environment.
    ///
    /// By default, uv uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--python`
    /// option allows you to specify a different interpreter.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct AddArgs {
    /// The packages to add, as PEP 508 requirements (e.g., `ruff==0.5.0`).
    #[arg(required = true)]
    pub requirements: Vec<String>,

    /// Add the requirements as development dependencies.
    #[arg(long, conflicts_with("optional"))]
    pub dev: bool,

    /// Add the requirements to the specified optional dependency group.
    #[arg(long, conflicts_with("dev"))]
    pub optional: Option<ExtraName>,

    #[arg(long, overrides_with = "no_editable", hide = true)]
    pub editable: bool,

    /// Don't add the requirements as editables.
    #[arg(long, overrides_with = "editable")]
    pub no_editable: bool,

    /// Add source requirements to `project.dependencies`, rather than `tool.uv.sources`.
    ///
    /// By default, uv will use the `tool.uv.sources` section to record source information for Git,
    /// local, editable, and direct URL requirements.
    #[arg(
        long,
        conflicts_with = "editable",
        conflicts_with = "no_editable",
        conflicts_with = "rev",
        conflicts_with = "tag",
        conflicts_with = "branch"
    )]
    pub raw_sources: bool,

    /// Specific commit to use when adding from Git.
    #[arg(long, group = "git-ref", action = clap::ArgAction::Set)]
    pub rev: Option<String>,

    /// Tag to use when adding from git.
    #[arg(long, group = "git-ref", action = clap::ArgAction::Set)]
    pub tag: Option<String>,

    /// Branch to use when adding from git.
    #[arg(long, group = "git-ref", action = clap::ArgAction::Set)]
    pub branch: Option<String>,

    /// Extras to activate for the dependency; may be provided more than once.
    #[arg(long)]
    pub extra: Option<Vec<ExtraName>>,

    /// Assert that the `uv.lock` will remain unchanged.
    #[arg(long, conflicts_with = "frozen")]
    pub locked: bool,

    /// Add the requirements without updating the `uv.lock` file.
    #[arg(long, conflicts_with = "locked")]
    pub frozen: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Add the dependency to a specific package in the workspace.
    #[arg(long, conflicts_with = "isolated")]
    pub package: Option<PackageName>,

    /// The Python interpreter into which packages should be installed.
    ///
    /// By default, uv installs into the virtual environment in the current working directory or
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
    pub python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct RemoveArgs {
    /// The names of the packages to remove (e.g., `ruff`).
    #[arg(required = true)]
    pub requirements: Vec<PackageName>,

    /// Remove the requirements from development dependencies.
    #[arg(long, conflicts_with("optional"))]
    pub dev: bool,

    /// Remove the requirements from the specified optional dependency group.
    #[arg(long, conflicts_with("dev"))]
    pub optional: Option<ExtraName>,

    /// Assert that the `uv.lock` will remain unchanged.
    #[arg(long, conflicts_with = "frozen")]
    pub locked: bool,

    /// Remove the requirements without updating the `uv.lock` file.
    #[arg(long, conflicts_with = "locked")]
    pub frozen: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Remove the dependency from a specific package in the workspace.
    #[arg(long, conflicts_with = "isolated")]
    pub package: Option<PackageName>,

    /// The Python interpreter into which packages should be installed.
    ///
    /// By default, uv installs into the virtual environment in the current working directory or
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
    pub python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct TreeArgs {
    #[command(flatten)]
    pub tree: DisplayTreeArgs,

    /// Assert that the `uv.lock` will remain unchanged.
    #[arg(long, conflicts_with = "frozen")]
    pub locked: bool,

    /// Display the requirements without updating the `uv.lock` file.
    #[arg(long, conflicts_with = "locked")]
    pub frozen: bool,

    #[command(flatten)]
    pub build: BuildArgs,

    #[command(flatten)]
    pub resolver: ResolverArgs,

    /// The Python interpreter for which packages should be listed.
    ///
    /// By default, uv installs into the virtual environment in the current working directory or
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
    pub python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ToolNamespace {
    #[command(subcommand)]
    pub command: ToolCommand,
}

#[derive(Subcommand)]
pub enum ToolCommand {
    /// Run a tool.
    Run(ToolRunArgs),
    /// Hidden alias for `uv tool run` for invocation from the `uvx` command
    #[command(
        hide = true,
        override_usage = "uvx [OPTIONS] <COMMAND>",
        about = "Run a tool."
    )]
    Uvx(ToolRunArgs),
    /// Install a tool.
    Install(ToolInstallArgs),
    /// List installed tools.
    List(ToolListArgs),
    /// Uninstall a tool.
    Uninstall(ToolUninstallArgs),
    /// Ensure that the tool executable directory is on `PATH`.
    #[command(alias = "ensurepath")]
    UpdateShell,
    /// Show the tools directory.
    Dir(ToolDirArgs),
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ToolRunArgs {
    /// The command to run.
    ///
    /// By default, the package to install is assumed to match the command name.
    ///
    /// The name of the command can include an exact version in the format `<package>@<version>`.
    ///
    /// If more complex version specification is desired or if the command is provided by a different
    /// package, use `--from`.
    ///
    /// If omitted, lists the available tools.
    #[command(subcommand)]
    pub command: Option<ExternalCommand>,

    /// Use the given package to provide the command.
    ///
    /// By default, the package name is assumed to match the command name.
    #[arg(long)]
    pub from: Option<String>,

    /// Run with the given packages installed.
    #[arg(long)]
    pub with: Vec<String>,

    /// Run with all packages listed in the given `requirements.txt` files.
    #[arg(long, value_parser = parse_maybe_file_path)]
    pub with_requirements: Vec<Maybe<PathBuf>>,

    /// Run the tool in an isolated virtual environment, ignoring any already-installed tools.
    #[arg(long)]
    pub isolated: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// The Python interpreter to use to build the run environment.
    ///
    /// By default, uv uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--python`
    /// option allows you to specify a different interpreter.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub python: Option<String>,

    /// Whether to show resolver and installer output from any environment modifications.
    ///
    /// By default, environment modifications are omitted, but enabled under `--verbose`.
    #[arg(long, env = "UV_SHOW_RESOLUTION", value_parser = clap::builder::BoolishValueParser::new(), hide = true)]
    pub show_resolution: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ToolInstallArgs {
    /// The package to install commands from.
    pub package: String,

    #[arg(short, long)]
    pub editable: bool,

    /// The package to install commands from.
    ///
    /// This option is provided for parity with `uv tool run`, but is redundant with `package`.
    #[arg(long, hide = true)]
    pub from: Option<String>,

    /// Include the following extra requirements.
    #[arg(long)]
    pub with: Vec<String>,

    /// Run all requirements listed in the given `requirements.txt` files.
    #[arg(long, value_parser = parse_maybe_file_path)]
    pub with_requirements: Vec<Maybe<PathBuf>>,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Force installation of the tool.
    ///
    /// Will replace any existing entry points with the same name in the executable directory.
    #[arg(long)]
    pub force: bool,

    /// The Python interpreter to use to build the tool environment.
    ///
    /// By default, uv will search for a Python executable in the `PATH`. uv ignores virtual
    /// environments while looking for interpreter for tools. The `--python` option allows
    /// you to specify a different interpreter.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[arg(long, short, env = "UV_PYTHON", verbatim_doc_comment)]
    pub python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ToolListArgs {
    /// Whether to display the path to each tool environment and installed executable.
    #[arg(long)]
    pub show_paths: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ToolDirArgs {
    /// Show the directory into which `uv tool` will install executables.
    ///
    /// By default, `uv tool dir` shows the directory into which the tool Python environments
    /// themselves are installed, rather than the directory containing the linked executables.
    #[arg(long)]
    pub bin: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ToolUninstallArgs {
    /// The name of the tool to uninstall.
    #[arg(required = true)]
    pub name: Option<PackageName>,

    /// Uninstall all tools.
    #[arg(long, conflicts_with("name"))]
    pub all: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PythonNamespace {
    #[command(subcommand)]
    pub command: PythonCommand,
}

#[derive(Subcommand)]
pub enum PythonCommand {
    /// List the available Python installations.
    List(PythonListArgs),

    /// Download and install Python versions.
    Install(PythonInstallArgs),

    /// Search for a Python installation.
    Find(PythonFindArgs),

    /// Pin to a specific Python version.
    Pin(PythonPinArgs),

    /// Show the uv Python installation directory.
    Dir,

    /// Uninstall Python versions.
    Uninstall(PythonUninstallArgs),
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PythonListArgs {
    /// List all Python versions, including outdated patch versions.
    #[arg(long)]
    pub all_versions: bool,

    /// List Python installations for all platforms.
    #[arg(long)]
    pub all_platforms: bool,

    /// Only show installed Python versions, exclude available downloads.
    #[arg(long)]
    pub only_installed: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PythonInstallArgs {
    /// The Python version(s) to install.
    ///
    /// If not provided, the requested Python version(s) will be read from the `.python-versions`
    /// or `.python-version` files. If neither file is present, uv will check if it has
    /// installed any Python versions. If not, it will install the latest stable version of Python.
    pub targets: Vec<String>,

    /// Reinstall the requested Python version, if it's already installed.
    #[arg(long, short, alias = "force")]
    pub reinstall: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PythonUninstallArgs {
    /// The Python version(s) to uninstall.
    #[arg(required = true)]
    pub targets: Vec<String>,

    /// Uninstall all managed Python versions.
    #[arg(long, conflicts_with("targets"))]
    pub all: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PythonFindArgs {
    /// The Python request.
    pub request: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PythonPinArgs {
    /// The Python version.
    pub request: Option<String>,

    /// Write the resolved Python interpreter path instead of the request.
    ///
    /// Ensures that the exact same interpreter is used.
    #[arg(long, overrides_with("resolved"))]
    pub resolved: bool,

    #[arg(long, overrides_with("no_resolved"), hide = true)]
    pub no_resolved: bool,

    /// Avoid validating the Python pin against the workspace in the current directory or any parent
    /// directory.
    #[arg(long)]
    pub no_workspace: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct IndexArgs {
    /// The URL of the Python package index (by default: <https://pypi.org/simple>).
    ///
    /// Accepts either a repository compliant with PEP 503 (the simple repository API), or a local
    /// directory laid out in the same format.
    ///
    /// The index given by this flag is given lower priority than all other
    /// indexes specified via the `--extra-index-url` flag.
    #[arg(long, short, env = "UV_INDEX_URL", value_parser = parse_index_url)]
    pub index_url: Option<Maybe<IndexUrl>>,

    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    ///
    /// Accepts either a repository compliant with PEP 503 (the simple repository API), or a local
    /// directory laid out in the same format.
    ///
    /// All indexes provided via this flag take priority over the index specified by
    /// `--index-url` (which defaults to PyPI). When multiple `--extra-index-url` flags are
    /// provided, earlier values take priority.
    #[arg(long, env = "UV_EXTRA_INDEX_URL", value_delimiter = ' ', value_parser = parse_index_url)]
    pub extra_index_url: Option<Vec<Maybe<IndexUrl>>>,

    /// Locations to search for candidate distributions, in addition to those found in the registry
    /// indexes.
    ///
    /// If a path, the target must be a directory that contains packages as wheel files (`.whl`) or
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files adhering to the
    /// formats described above.
    #[arg(long, short)]
    pub find_links: Option<Vec<FlatIndexLocation>>,

    /// Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those
    /// provided via `--find-links`.
    #[arg(long)]
    pub no_index: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct RefreshArgs {
    /// Refresh all cached data.
    #[arg(long, conflicts_with("offline"), overrides_with("no_refresh"))]
    pub refresh: bool,

    #[arg(
        long,
        conflicts_with("offline"),
        overrides_with("refresh"),
        hide = true
    )]
    pub no_refresh: bool,

    /// Refresh cached data for a specific package.
    #[arg(long)]
    pub refresh_package: Vec<PackageName>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct BuildArgs {
    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary Python code. The cached wheels of
    /// already-built source distributions will be reused, but operations that require building
    /// distributions will exit with an error.
    #[arg(long, overrides_with("build"))]
    pub no_build: bool,

    #[arg(long, overrides_with("no_build"), hide = true)]
    pub build: bool,

    /// Don't build source distributions for a specific package.
    #[arg(long)]
    pub no_build_package: Vec<PackageName>,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be built and installed from source. The resolver will still use
    /// pre-built wheels to extract package metadata, if available.
    #[arg(long, overrides_with("binary"))]
    pub no_binary: bool,

    #[arg(long, overrides_with("no_binary"), hide = true)]
    pub binary: bool,

    /// Don't install pre-built wheels for a specific package.
    #[arg(long)]
    pub no_binary_package: Vec<PackageName>,
}

/// Arguments that are used by commands that need to install (but not resolve) packages.
#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct InstallerArgs {
    #[command(flatten)]
    pub index_args: IndexArgs,

    /// Reinstall all packages, regardless of whether they're already installed. Implies
    /// `--refresh`.
    #[arg(long, alias = "force-reinstall", overrides_with("no_reinstall"))]
    pub reinstall: bool,

    #[arg(long, overrides_with("reinstall"), hide = true)]
    pub no_reinstall: bool,

    /// Reinstall a specific package, regardless of whether it's already installed. Implies
    /// `--refresh-package`.
    #[arg(long)]
    pub reinstall_package: Vec<PackageName>,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-match`). This prevents
    /// "dependency confusion" attacks, whereby an attack can upload a malicious package under the
    /// same name to a secondary.
    #[arg(long, value_enum, env = "UV_INDEX_STRATEGY")]
    pub index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to
    /// use the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(long, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub keyring_provider: Option<KeyringProviderType>,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[arg(long, short = 'C', alias = "config-settings")]
    pub config_setting: Option<Vec<ConfigSettingEntry>>,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, env = "UV_EXCLUDE_NEWER")]
    pub exclude_newer: Option<ExcludeNewer>,

    /// The method to use when installing packages from the global cache.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[arg(long, value_enum, env = "UV_LINK_MODE")]
    pub link_mode: Option<install_wheel_rs::linker::LinkMode>,

    /// Compile Python files to bytecode after installation.
    ///
    /// By default, uv does not compile Python (`.py`) files to bytecode (`__pycache__/*.pyc`);
    /// instead, compilation is performed lazily the first time a module is imported. For use-cases
    /// in which start time is critical, such as CLI applications and Docker containers, this option
    /// can be enabled to trade longer installation times for faster start times.
    ///
    /// When enabled, uv will process the entire site-packages directory (including packages that
    /// are not being modified by the current operation) for consistency. Like pip, it will also
    /// ignore errors.
    #[arg(long, alias = "compile", overrides_with("no_compile_bytecode"))]
    pub compile_bytecode: bool,

    #[arg(
        long,
        alias = "no-compile",
        overrides_with("compile_bytecode"),
        hide = true
    )]
    pub no_compile_bytecode: bool,
}

/// Arguments that are used by commands that need to resolve (but not install) packages.
#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ResolverArgs {
    #[command(flatten)]
    pub index_args: IndexArgs,

    /// Allow package upgrades, ignoring pinned versions in any existing output file.
    #[arg(long, short = 'U', overrides_with("no_upgrade"))]
    pub upgrade: bool,

    #[arg(long, overrides_with("upgrade"), hide = true)]
    pub no_upgrade: bool,

    /// Allow upgrades for a specific package, ignoring pinned versions in any existing output
    /// file.
    #[arg(long, short = 'P')]
    pub upgrade_package: Vec<Requirement<VerbatimParsedUrl>>,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-match`). This prevents
    /// "dependency confusion" attacks, whereby an attack can upload a malicious package under the
    /// same name to a secondary.
    #[arg(long, value_enum, env = "UV_INDEX_STRATEGY")]
    pub index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to
    /// use the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(long, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub keyring_provider: Option<KeyringProviderType>,

    /// The strategy to use when selecting between the different compatible versions for a given
    /// package requirement.
    ///
    /// By default, uv will use the latest compatible version of each package (`highest`).
    #[arg(long, value_enum, env = "UV_RESOLUTION")]
    pub resolution: Option<ResolutionMode>,

    /// The strategy to use when considering pre-release versions.
    ///
    /// By default, uv will accept pre-releases for packages that _only_ publish pre-releases,
    /// along with first-party requirements that contain an explicit pre-release marker in the
    /// declared specifiers (`if-necessary-or-explicit`).
    #[arg(long, value_enum, env = "UV_PRERELEASE")]
    pub prerelease: Option<PreReleaseMode>,

    #[arg(long, hide = true)]
    pub pre: bool,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[arg(long, short = 'C', alias = "config-settings")]
    pub config_setting: Option<Vec<ConfigSettingEntry>>,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, env = "UV_EXCLUDE_NEWER")]
    pub exclude_newer: Option<ExcludeNewer>,

    /// The method to use when installing packages from the global cache.
    ///
    /// This option is only used when building source distributions.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[arg(long, value_enum, env = "UV_LINK_MODE")]
    pub link_mode: Option<install_wheel_rs::linker::LinkMode>,
}

/// Arguments that are used by commands that need to resolve and install packages.
#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ResolverInstallerArgs {
    #[command(flatten)]
    pub index_args: IndexArgs,

    /// Allow package upgrades, ignoring pinned versions in any existing output file.
    #[arg(long, short = 'U', overrides_with("no_upgrade"))]
    pub upgrade: bool,

    #[arg(long, overrides_with("upgrade"), hide = true)]
    pub no_upgrade: bool,

    /// Allow upgrades for a specific package, ignoring pinned versions in any existing output
    /// file.
    #[arg(long, short = 'P')]
    pub upgrade_package: Vec<Requirement<VerbatimParsedUrl>>,

    /// Reinstall all packages, regardless of whether they're already installed. Implies
    /// `--refresh`.
    #[arg(long, alias = "force-reinstall", overrides_with("no_reinstall"))]
    pub reinstall: bool,

    #[arg(long, overrides_with("reinstall"), hide = true)]
    pub no_reinstall: bool,

    /// Reinstall a specific package, regardless of whether it's already installed. Implies
    /// `--refresh-package`.
    #[arg(long)]
    pub reinstall_package: Vec<PackageName>,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-match`). This prevents
    /// "dependency confusion" attacks, whereby an attack can upload a malicious package under the
    /// same name to a secondary.
    #[arg(long, value_enum, env = "UV_INDEX_STRATEGY")]
    pub index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to
    /// use the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(long, value_enum, env = "UV_KEYRING_PROVIDER")]
    pub keyring_provider: Option<KeyringProviderType>,

    /// The strategy to use when selecting between the different compatible versions for a given
    /// package requirement.
    ///
    /// By default, uv will use the latest compatible version of each package (`highest`).
    #[arg(long, value_enum, env = "UV_RESOLUTION")]
    pub resolution: Option<ResolutionMode>,

    /// The strategy to use when considering pre-release versions.
    ///
    /// By default, uv will accept pre-releases for packages that _only_ publish pre-releases,
    /// along with first-party requirements that contain an explicit pre-release marker in the
    /// declared specifiers (`if-necessary-or-explicit`).
    #[arg(long, value_enum, env = "UV_PRERELEASE")]
    pub prerelease: Option<PreReleaseMode>,

    #[arg(long, hide = true)]
    pub pre: bool,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[arg(long, short = 'C', alias = "config-settings")]
    pub config_setting: Option<Vec<ConfigSettingEntry>>,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    #[arg(long, env = "UV_EXCLUDE_NEWER")]
    pub exclude_newer: Option<ExcludeNewer>,

    /// The method to use when installing packages from the global cache.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[arg(long, value_enum, env = "UV_LINK_MODE")]
    pub link_mode: Option<install_wheel_rs::linker::LinkMode>,

    /// Compile Python files to bytecode after installation.
    ///
    /// By default, uv does not compile Python (`.py`) files to bytecode (`__pycache__/*.pyc`);
    /// instead, compilation is performed lazily the first time a module is imported. For use-cases
    /// in which start time is critical, such as CLI applications and Docker containers, this option
    /// can be enabled to trade longer installation times for faster start times.
    ///
    /// When enabled, uv will process the entire site-packages directory (including packages that
    /// are not being modified by the current operation) for consistency. Like pip, it will also
    /// ignore errors.
    #[arg(long, alias = "compile", overrides_with("no_compile_bytecode"))]
    pub compile_bytecode: bool,

    #[arg(
        long,
        alias = "no-compile",
        overrides_with("compile_bytecode"),
        hide = true
    )]
    pub no_compile_bytecode: bool,
}

#[derive(Args)]
pub struct DisplayTreeArgs {
    /// Maximum display depth of the dependency tree
    #[arg(long, short, default_value_t = 255)]
    pub depth: u8,

    /// Prune the given package from the display of the dependency tree.
    #[arg(long)]
    pub prune: Vec<PackageName>,

    /// Display only the specified packages.
    #[arg(long)]
    pub package: Vec<PackageName>,

    /// Do not de-duplicate repeated dependencies.
    /// Usually, when a package has already displayed its dependencies,
    /// further occurrences will not re-display its dependencies,
    /// and will include a (*) to indicate it has already been shown.
    /// This flag will cause those duplicates to be repeated.
    #[arg(long)]
    pub no_dedupe: bool,

    /// Show the reverse dependencies for the given package. This flag will invert the tree and display the packages that depend on the given package.
    #[arg(long, alias = "reverse")]
    pub invert: bool,

    /// Show the version constraint(s) imposed on each package.
    #[arg(long)]
    pub show_version_specifiers: bool,
}
