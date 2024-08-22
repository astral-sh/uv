use std::ffi::OsString;
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{anyhow, Result};
use clap::builder::styling::Style;
use clap::{Args, Parser, Subcommand};

use distribution_types::{FlatIndexLocation, IndexUrl};
use pep508_rs::Requirement;
use pypi_types::VerbatimParsedUrl;
use uv_cache::CacheArgs;
use uv_configuration::{
    ConfigSettingEntry, IndexStrategy, KeyringProviderType, PackageNameSpecifier, TargetTriple,
};
use uv_normalize::{ExtraName, PackageName};
use uv_python::{PythonDownloads, PythonPreference, PythonVersion};
use uv_resolver::{AnnotationStyle, ExcludeNewer, PrereleaseMode, ResolutionMode};

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
#[command(name = "uv", author, long_version = crate::version::version())]
#[command(about = "An extremely fast Python package manager.")]
#[command(propagate_version = true)]
#[command(
    after_help = "Use `uv help` for more details.",
    after_long_help = "",
    disable_help_flag = true,
    disable_help_subcommand = true,
    disable_version_flag = true
)]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Box<Commands>,

    #[command(flatten)]
    pub cache_args: Box<CacheArgs>,

    #[command(flatten)]
    pub global_args: Box<GlobalArgs>,

    /// The path to a `uv.toml` file to use for configuration.
    ///
    /// While uv configuration can be included in a `pyproject.toml` file, it is
    /// not allowed in this context.
    #[arg(
        global = true,
        long,
        env = "UV_CONFIG_FILE",
        help_heading = "Global options"
    )]
    pub config_file: Option<PathBuf>,

    /// Avoid discovering configuration files (`pyproject.toml`, `uv.toml`).
    ///
    /// Normally, configuration files are discovered in the current directory,
    /// parent directories, or user configuration directories.
    #[arg(global = true, long, env = "UV_NO_CONFIG", value_parser = clap::builder::BoolishValueParser::new(), help_heading = "Global options")]
    pub no_config: bool,

    /// Display the concise help for this command.
    #[arg(global = true, short, long, action = clap::ArgAction::HelpShort, help_heading = "Global options")]
    help: Option<bool>,

    /// Display the uv version.
    #[arg(global = true, short = 'V', long, action = clap::ArgAction::Version, help_heading = "Global options")]
    version: Option<bool>,
}

#[derive(Parser, Debug, Clone)]
#[command(next_help_heading = "Global options", next_display_order = 1000)]
#[allow(clippy::struct_excessive_bools)]
pub struct GlobalArgs {
    /// Whether to prefer uv-managed or system Python installations.
    ///
    /// By default, uv prefers using Python versions it manages. However, it
    /// will use system Python installations if a uv-managed Python is not
    /// installed. This option allows prioritizing or ignoring system Python
    /// installations.
    #[arg(
        global = true,
        long,
        help_heading = "Python options",
        display_order = 700
    )]
    pub python_preference: Option<PythonPreference>,

    /// Allow automatically downloading Python when required.
    #[arg(global = true, long, help_heading = "Python options", hide = true)]
    pub allow_python_downloads: bool,

    /// Disable automatic downloads of Python.
    #[arg(global = true, long, help_heading = "Python options")]
    pub no_python_downloads: bool,

    /// Deprecated version of [`Self::python_downloads`].
    #[arg(global = true, long, hide = true)]
    pub python_fetch: Option<PythonDownloads>,

    /// Do not print any output.
    #[arg(global = true, long, short, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Use verbose output.
    ///
    /// You can configure fine-grained logging using the `RUST_LOG` environment variable.
    /// (<https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives>)
    #[arg(global = true, action = clap::ArgAction::Count, long, short, conflicts_with = "quiet")]
    pub verbose: u8,

    /// Disable colors.
    ///
    /// Provided for compatibility with `pip`, use `--color` instead.
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

    /// Disable network access.
    ///
    /// When disabled, uv will only use locally cached data and locally available files.
    #[arg(global = true, long, overrides_with("no_offline"))]
    pub offline: bool,

    #[arg(global = true, long, overrides_with("offline"), hide = true)]
    pub no_offline: bool,

    /// Whether to enable experimental, preview features.
    ///
    /// Preview features may change without warning.
    #[arg(global = true, long, hide = true, env = "UV_PREVIEW", value_parser = clap::builder::BoolishValueParser::new(), overrides_with("no_preview"))]
    pub preview: bool,

    #[arg(global = true, long, overrides_with("preview"), hide = true)]
    pub no_preview: bool,

    /// Avoid discovering a `pyproject.toml` or `uv.toml` file.
    ///
    /// Normally, configuration files are discovered in the current directory,
    /// parent directories, or user configuration directories.
    ///
    /// This option is deprecated in favor of `--no-config`.
    #[arg(global = true, long, hide = true)]
    pub isolated: bool,

    /// Show the resolved settings for the current command.
    ///
    /// This option is used for debugging and development purposes.
    #[arg(global = true, long, hide = true)]
    pub show_settings: bool,

    /// Hide all progress outputs.
    ///
    /// For example, spinners or progress bars.
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
    /// Manage Python projects.
    #[command(flatten)]
    Project(Box<ProjectCommand>),

    /// Run and install commands provided by Python packages.
    #[command(
        after_help = "Use `uv help tool` for more details.",
        after_long_help = ""
    )]
    Tool(ToolNamespace),

    /// Manage Python versions and installations
    ///
    /// Generally, uv first searches for Python in a virtual environment, either
    /// active or in a `.venv` directory  in the current working directory or
    /// any parent directory. If a virtual environment is not required, uv will
    /// then search for a Python interpreter. Python interpreters are found by
    /// searching for Python executables in the `PATH` environment variable.
    ///
    /// On Windows, the `py` launcher is also invoked to find Python
    /// executables.
    ///
    /// By default, uv will download Python if a version cannot be found. This
    /// behavior can be disabled with the `--python-downloads` option.
    ///
    /// The `--python` option allows requesting a different interpreter.
    ///
    /// The following Python version request formats are supported:
    ///
    /// - `<version>` e.g. `3`, `3.12`, `3.12.3`
    /// - `<version-specifier>` e.g. `>=3.12,<3.13`
    /// - `<implementation>` e.g. `cpython` or `cp`
    /// - `<implementation>@<version>` e.g. `cpython@3.12`
    /// - `<implementation><version>` e.g. `cpython3.12` or `cp312`
    /// - `<implementation><version-specifier>` e.g. `cpython>=3.12,<3.13`
    /// - `<implementation>-<version>-<os>-<arch>-<libc>` e.g.
    ///   `cpython-3.12.3-macos-aarch64-none`
    ///
    /// Additionally, a specific system Python interpreter can often be
    /// requested with:
    ///
    /// - `<executable-path>` e.g. `/opt/homebrew/bin/python3`
    /// - `<executable-name>` e.g. `mypython3`
    /// - `<install-dir>` e.g. `/some/environment/`
    ///
    /// When the `--python` option is used, normal discovery rules apply but
    /// discovered interpreters are checked for compatibility with the request,
    /// e.g., if `pypy` is requested, uv will first check if the virtual
    /// environment contains a PyPy interpreter then check if each executable in
    /// the path is a PyPy interpreter.
    ///
    /// uv supports discovering CPython, PyPy, and GraalPy interpreters.
    /// Unsupported interpreters will be skipped during discovery. If an
    /// unsupported interpreter implementation is requested, uv will exit with
    /// an error.
    #[clap(verbatim_doc_comment)]
    #[command(
        after_help = "Use `uv help python` for more details.",
        after_long_help = ""
    )]
    Python(PythonNamespace),
    /// Manage Python packages with a pip-compatible interface.
    #[command(
        after_help = "Use `uv help pip` for more details.",
        after_long_help = ""
    )]
    Pip(PipNamespace),
    /// Create a virtual environment.
    ///
    /// By default, creates a virtual environment named `.venv` in the working
    /// directory. An alternative path may be provided positionally.
    ///
    /// If a virtual environment exists at the target path, it will be removed
    /// and a new, empty virtual environment will be created.
    ///
    /// When using uv, the virtual environment does not need to be activated. uv
    /// will find a virtual environment (named `.venv`) in the working directory
    /// or any parent directories.
    #[command(
        alias = "virtualenv",
        alias = "v",
        after_help = "Use `uv help venv` for more details.",
        after_long_help = ""
    )]
    Venv(VenvArgs),
    /// Manage uv's cache.
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
    GenerateShellCompletion(GenerateShellCompletionArgs),
    /// Display documentation for a command.
    // To avoid showing the global options when displaying help for the help command, we are
    // responsible for maintaining the options using the `after_help`.
    #[command(help_template = "\
{about-with-newline}
{usage-heading} {usage}{after-help}
",
        after_help = format!("\
{heading}Options:{heading:#}
  {option}--no-pager{option:#}  Disable pager when printing help
",
            heading = Style::new().bold().underline(),
            option = Style::new().bold(),
        ),
    )]
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
    ///
    ///
    /// By default, the cache is stored in  `$XDG_CACHE_HOME/uv` or `$HOME/.cache/uv` on Unix and
    /// `{FOLDERID_LocalAppData}\uv\cache` on Windows.
    ///
    /// When `--no-cache` is used, the cache is stored in a temporary directory and discarded when
    /// the process exits.
    ///
    /// An alternative cache directory may be specified via the `cache-dir` setting, the
    /// `--cache-dir` option, or the `$UV_CACHE_DIR` environment variable.
    ///
    /// Note that it is important for performance for the cache directory to be located on the same
    /// file system as the Python environment uv is operating on.
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
    /// Run a command or script.
    ///
    /// Ensures that the command runs in a Python environment.
    ///
    /// When used with a file ending in `.py`, the file will be treated as a
    /// script and run with a Python interpreter, i.e., `uv run file.py` is
    /// equivalent to `uv run python file.py`. If the script contains inline
    /// dependency metadata, it will be installed into an isolated, ephemeral
    /// environment.
    ///
    /// When used in a project, the project environment will be created and
    /// updated before invoking the command.
    ///
    /// When used outside a project, if a virtual environment can be found in
    /// the current directory or a parent directory, the command will be run in
    /// that environment. Otherwise, the command will be run in the environment
    /// of the discovered interpreter.
    ///
    /// Arguments following the command (or script) are not interpreted as
    /// arguments to uv. All options to uv must be provided before the command,
    /// e.g., `uv run --verbose foo`. A `--` can be used to separate the command
    /// from uv options for clarity, e.g., `uv run --python 3.12 -- python`.
    #[command(
        after_help = "Use `uv help run` for more details.",
        after_long_help = ""
    )]
    Run(RunArgs),
    /// Create a new project.
    ///
    /// Follows the `pyproject.toml` specification.
    ///
    /// If a `pyproject.toml` already exists at the target, uv will exit with an
    /// error.
    ///
    /// If a `pyproject.toml` is found in any of the parent directories of the
    /// target path, the project will be added as a workspace member of
    /// the parent.
    ///
    /// Some project state is not created until needed, e.g., the project
    /// virtual environment (`.venv`) and lockfile (`uv.lock`) are lazily
    /// created during the first sync.
    Init(InitArgs),
    /// Add dependencies to the project.
    ///
    /// Dependencies are added to the project's `pyproject.toml` file.
    ///
    /// If a given dependency exists already, it will be updated to the new version specifier unless
    /// it includes markers that differ from the existing specifier in which case another entry for
    /// the depenedency will be added.
    ///
    /// If no constraint or URL is provided for a dependency, a lower bound is added equal to the
    /// latest compatible version of the package, e.g., `>=1.2.3`, unless `--frozen` is provided, in
    /// which case no resolution is performed.
    ///
    /// The lockfile and project environment will be updated to reflect the added dependencies. To
    /// skip updating the lockfile, use `--frozen`. To skip updating the environment, use
    /// `--no-sync`.
    ///
    /// If any of the requested dependencies cannot be found, uv will exit with an error, unless the
    /// `--frozen` flag is provided, in which case uv will add the dependencies verbatim without
    /// checking that they exist or are compatible with the project.
    ///
    /// uv will search for a project in the current directory or any parent directory. If a project
    /// cannot be found, uv will exit with an error.
    #[command(
        after_help = "Use `uv help add` for more details.",
        after_long_help = ""
    )]
    Add(AddArgs),
    /// Remove dependencies from the project.
    ///
    /// Dependencies are removed from the project's `pyproject.toml` file.
    ///
    /// If multiple entries exist for a given dependency, i.e., each with different markers, all of
    /// the entries will be removed.
    ///
    /// The lockfile and project environment will be updated to reflect the
    /// removed dependencies. To skip updating the lockfile, use `--frozen`. To
    /// skip updating the environment, use `--no-sync`.
    ///
    /// If any of the requested dependencies are not present in the project, uv
    /// will exit with an error.
    ///
    /// If a package has been manually installed in the environment, i.e., with
    /// `uv pip install`, it will not be removed by `uv remove`.
    ///
    /// uv will search for a project in the current directory or any parent
    /// directory. If a project cannot be found, uv will exit with an error.
    #[command(
        after_help = "Use `uv help remove` for more details.",
        after_long_help = ""
    )]
    Remove(RemoveArgs),
    /// Update the project's environment.
    ///
    /// Syncing ensures that all project dependencies are installed and up-to-date with the
    /// lockfile.
    ///
    /// By default, an exact sync is performed: uv removes packages that are not declared as
    /// dependencies of the project. Use the `--inexact` flag to keep extraneous packages. Note that
    /// if an extraneous package conflicts with a project dependency, it will still be removed.
    /// Additionally, if `--no-build-isolation` is used, uv will not remove extraneous packages to
    /// avoid removing possible build dependencies.
    ///
    /// If the project virtual environment (`.venv`) does not exist, it will be created.
    ///
    /// The project is re-locked before syncing unless the `--locked` or `--frozen` flag is
    /// provided.
    ///
    /// uv will search for a project in the current directory or any parent directory. If a project
    /// cannot be found, uv will exit with an error.
    ///
    /// Note that, when installing from a lockfile, uv will not provide warnings for yanked package
    /// versions.
    #[command(
        after_help = "Use `uv help sync` for more details.",
        after_long_help = ""
    )]
    Sync(SyncArgs),
    /// Update the project's lockfile.
    ///
    /// If the project lockfile (`uv.lock`) does not exist, it will be created.
    /// If a lockfile is present, its contents will be used as preferences for
    /// the resolution.
    ///
    /// If there are no changes to the project's dependencies, locking will have
    /// no effect unless the `--upgrade` flag is provided.
    #[command(
        after_help = "Use `uv help lock` for more details.",
        after_long_help = ""
    )]
    Lock(LockArgs),
    /// Display the project's dependency tree.
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
    /// If a `pyproject.toml`, `setup.py`, or `setup.cfg` file is provided, uv will extract the
    /// requirements for the relevant project.
    ///
    /// If `-` is provided, then requirements will be read from stdin.
    ///
    /// The order of the requirements files and the requirements in them is used to determine
    /// priority during resolution.
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

    /// Constrain build dependencies using the given requirements files when building source
    /// distributions.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    #[arg(long, short, env = "UV_BUILD_CONSTRAINT", value_delimiter = ' ', value_parser = parse_maybe_file_path)]
    pub build_constraint: Vec<Maybe<PathBuf>>,

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

    /// The Python interpreter to use during resolution.
    ///
    /// A Python interpreter is required for building source distributions to
    /// determine package metadata when there are not wheels.
    ///
    /// The interpreter is also used to determine the default minimum Python
    /// version, unless `--python-version` is provided.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(long, verbatim_doc_comment, help_heading = "Python options")]
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

    /// The Python version to use for resolution.
    ///
    /// For example, `3.8` or `3.8.17`.
    ///
    /// Defaults to the version of the Python interpreter used for resolution.
    ///
    /// Defines the minimum Python version that must be supported by the
    /// resolved requirements.
    ///
    /// If a patch version is omitted, the minimum patch version is assumed. For
    /// example, `3.8` is mapped to `3.8.0`.
    #[arg(long, short, help_heading = "Python options")]
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

    /// Constrain build dependencies using the given requirements files when building source
    /// distributions.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    #[arg(long, short, env = "UV_BUILD_CONSTRAINT", value_delimiter = ' ', value_parser = parse_maybe_file_path)]
    pub build_constraint: Vec<Maybe<PathBuf>>,

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
    /// By default, syncing requires a virtual environment. An path to an
    /// alternative Python can be provided, but it is only recommended in
    /// continuous integration (CI) environments and should be used with
    /// caution, as it can modify the system Python installation.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
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
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
#[allow(clippy::struct_excessive_bools)]
pub struct PipInstallArgs {
    /// Install all listed packages.
    ///
    /// The order of the packages is used to determine priority during resolution.
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

    /// Constrain build dependencies using the given requirements files when building source
    /// distributions.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    #[arg(long, short, env = "UV_BUILD_CONSTRAINT", value_delimiter = ' ', value_parser = parse_maybe_file_path)]
    pub build_constraint: Vec<Maybe<PathBuf>>,

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
    /// By default, installation requires a virtual environment. An path to an
    /// alternative Python can be provided, but it is only recommended in
    /// continuous integration (CI) environments and should be used with
    /// caution, as it can modify the system Python installation.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
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
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
#[allow(clippy::struct_excessive_bools)]
pub struct PipUninstallArgs {
    /// Uninstall all listed packages.
    #[arg(group = "sources")]
    pub package: Vec<String>,

    /// Uninstall all packages listed in the given requirements files.
    #[arg(long, short, group = "sources", value_parser = parse_file_path)]
    pub requirement: Vec<PathBuf>,

    /// The Python interpreter from which packages should be uninstalled.
    ///
    /// By default, uninstallation requires a virtual environment. An path to an
    /// alternative Python can be provided, but it is only recommended in
    /// continuous integration (CI) environments and should be used with
    /// caution, as it can modify the system Python installation.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
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
    /// By default, uv lists packages in a virtual environment but will show
    /// packages in a system Python environment if no virtual environment is
    /// found.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
    pub python: Option<String>,

    /// List packages in the system Python environment.
    ///
    /// Disables discovery of virtual environments.
    ///
    /// See `uv help python` for details on Python discovery.
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
    /// By default, uv lists packages in a virtual environment but will show
    /// packages in a system Python environment if no virtual environment is
    /// found.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
    pub python: Option<String>,

    /// List packages in the system Python environment.
    ///
    /// Disables discovery of virtual environments.
    ///
    /// See `uv help python` for details on Python discovery.
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
    /// The Python interpreter for which packages should be checked.
    ///
    /// By default, uv checks packages in a virtual environment but will check
    /// packages in a system Python environment if no virtual environment is
    /// found.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
    pub python: Option<String>,

    /// Check packages in the system Python environment.
    ///
    /// Disables discovery of virtual environments.
    ///
    /// See `uv help python` for details on Python discovery.
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

    /// The Python interpreter to find the package in.
    ///
    /// By default, uv looks for packages in a virtual environment but will look
    /// for packages in a system Python environment if no virtual environment is
    /// found.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
    pub python: Option<String>,

    /// Show a package in the system Python environment.
    ///
    /// Disables discovery of virtual environments.
    ///
    /// See `uv help python` for details on Python discovery.
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
    /// Show the version constraint(s) imposed on each package.
    #[arg(long)]
    pub show_version_specifiers: bool,

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
    /// By default, uv lists packages in a virtual environment but will show
    /// packages in a system Python environment if no virtual environment is
    /// found.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
    pub python: Option<String>,

    /// List packages in the system Python environment.
    ///
    /// Disables discovery of virtual environments.
    ///
    /// See `uv help python` for details on Python discovery.
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
    /// During virtual environment creation, uv will not look for Python
    /// interpreters in virtual environments.
    ///
    /// See `uv python help` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
    pub python: Option<String>,

    /// Ignore virtual environments when searching for the Python interpreter.
    ///
    /// This is the default behavior and has no effect.
    #[arg(
        long,
        env = "UV_SYSTEM_PYTHON",
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system"),
        hide = true,
    )]
    pub system: bool,

    /// This flag is included for compatibility only, it has no effect.
    ///
    /// uv will never search for interpreters in virtual environments when
    /// creating a virtual environment.
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
    /// By default, the prompt is dependent on whether a path was provided to
    /// `uv venv`. If provided (e.g, `uv venv project`), the prompt is set to
    /// the directory name. If not provided (`uv venv`), the prompt is set to
    /// the current directory's name.
    ///
    /// If "." is provided, the the current directory name will be used
    /// regardless of whether a path was provided to `uv venv`.
    #[arg(long, verbatim_doc_comment)]
    pub prompt: Option<String>,

    /// Give the virtual environment access to the system site packages directory.
    ///
    /// Unlike `pip`, when a virtual environment is created with `--system-site-packages`, uv will
    /// _not_ take system site packages into account when running commands like `uv pip list` or
    /// `uv pip install`. The `--system-site-packages` flag will provide the virtual environment
    /// with access to the system site packages directory at runtime, but will not affect the
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
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and local dates in the same
    /// format (e.g., `2006-12-02`) in your system's configured time zone.
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
    /// The path to use for the project.
    ///
    /// Defaults to the current working directory. Accepts relative and absolute
    /// paths.
    ///
    /// If a `pyproject.toml` is found in any of the parent directories of the
    /// target path, the project will be added as a workspace member of the
    /// parent, unless `--no-workspace` is provided.
    pub path: Option<String>,

    /// The name of the project.
    ///
    /// Defaults to the name of the directory.
    #[arg(long)]
    pub name: Option<PackageName>,

    /// Create a virtual workspace instead of a project.
    ///
    /// A virtual workspace does not define project dependencies and cannot be
    /// published. Instead, workspace members declare project dependencies.
    /// Development dependencies may still be declared.
    #[arg(long)]
    pub r#virtual: bool,

    /// Do not create a `README.md` file.
    #[arg(long)]
    pub no_readme: bool,

    /// Avoid discovering a workspace.
    ///
    /// Instead, create a standalone project.
    ///
    /// By default, uv searches for workspaces in the current directory or any
    /// parent directory.
    #[arg(long, alias = "no_project")]
    pub no_workspace: bool,

    /// The Python interpreter to use to determine the minimum supported Python version.
    ///
    /// See `uv help python` to view supported request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
    pub python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct RunArgs {
    /// Include optional dependencies from the extra group name.
    ///
    /// May be provided more than once.
    ///
    /// Optional dependencies are defined via `project.optional-dependencies` in
    /// a `pyproject.toml`.
    ///
    /// This option is only available when running in a project.
    #[arg(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    ///
    /// Optional dependencies are defined via `project.optional-dependencies` in
    /// a `pyproject.toml`.
    ///
    /// This option is only available when running in a project.
    #[arg(long, conflicts_with = "extra")]
    pub all_extras: bool,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub no_all_extras: bool,

    /// Include development dependencies.
    ///
    /// Development dependencies are defined via `tool.uv.dev-dependencies` in a
    /// `pyproject.toml`.
    ///
    /// This option is only available when running in a project.
    #[arg(long, overrides_with("no_dev"), hide = true)]
    pub dev: bool,

    /// Omit development dependencies.
    ///
    /// This option is only available when running in a project.
    #[arg(long, overrides_with("dev"))]
    pub no_dev: bool,

    /// The command to run.
    ///
    /// If the path to a Python script (i.e., ending in `.py`), it will be
    /// executed with the Python interpreter.
    #[command(subcommand)]
    pub command: ExternalCommand,

    /// Run with the given packages installed.
    ///
    /// When used in a project, these dependencies will be layered on top of
    /// the project environment in a separate, ephemeral environment. These
    /// dependencies are allowed to conflict with those specified by the project.
    #[arg(long)]
    pub with: Vec<String>,

    /// Run with the given packages installed as editables
    ///
    /// When used in a project, these dependencies will be layered on top of
    /// the project environment in a separate, ephemeral environment. These
    /// dependencies are allowed to conflict with those specified by the project.
    #[arg(long)]
    pub with_editable: Vec<String>,

    /// Run with all packages listed in the given `requirements.txt` files.
    ///
    /// The same environment semantics as `--with` apply.
    ///
    /// Using `pyproject.toml`, `setup.py`, or `setup.cfg` files is not allowed.
    #[arg(long, value_parser = parse_maybe_file_path)]
    pub with_requirements: Vec<Maybe<PathBuf>>,

    /// Run the command in an isolated virtual environment.
    ///
    /// Usually, the project environment is reused for performance. This option
    /// forces a fresh environment to be used for the project, enforcing strict
    /// isolation between dependencies and declaration of requirements.
    ///
    /// An editable installation is still used for the project.
    ///
    /// When used with `--with` or `--with-requirements`, the additional
    /// dependencies will still be layered in a second environment.
    #[arg(long)]
    pub isolated: bool,

    /// Assert that the `uv.lock` will remain unchanged.
    ///
    /// Requires that the lockfile is up-to-date. If the lockfile is missing or
    /// needs to be updated, uv will exit with an error.
    #[arg(long, conflicts_with = "frozen")]
    pub locked: bool,

    /// Run without updating the `uv.lock` file.
    ///
    /// Instead of checking if the lockfile is up-to-date, uses the versions in
    /// the lockfile as the source of truth. If the lockfile is missing, uv will
    /// exit with an error. If the `pyproject.toml` includes changes to
    /// dependencies that have not been included in the lockfile yet, they will
    /// not be present in the environment.
    #[arg(long, conflicts_with = "locked")]
    pub frozen: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Run the command in a specific package in the workspace.
    ///
    /// If not in a workspace, or if the workspace member does not exist, uv
    /// will exit with an error.
    #[arg(long)]
    pub package: Option<PackageName>,

    /// Avoid discovering the project or workspace.
    ///
    /// Instead of searching for projects in the current directory and parent
    /// directories, run in an isolated, ephemeral environment populated by the
    /// `--with` requirements.
    ///
    /// If a virtual environment is active or found in a current or parent
    /// directory, it will be used as if there was no project or workspace.
    #[arg(long, alias = "no_workspace", conflicts_with = "package")]
    pub no_project: bool,

    /// The Python interpreter to use for the run environment.
    ///
    /// If the interpreter request is satisfied by a discovered environment, the
    /// environment will be used.
    ///
    /// See `uv help python` to view supported request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
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
    /// Include optional dependencies from the extra group name.
    ///
    /// May be provided more than once.
    #[arg(long, conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
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

    /// Do not remove extraneous packages present in the environment.
    ///
    /// When enabled, uv will make the minimum necessary changes to satisfy the requirements.
    ///
    /// By default, syncing will remove any extraneous packages from the environment, unless
    /// `--no-build-isolation` is enabled, in which case extra packages are considered necessary for
    /// builds.
    #[arg(long, overrides_with("exact"), alias = "no-exact")]
    pub inexact: bool,

    /// Perform an exact sync, removing extraneous packages.
    #[arg(long, overrides_with("inexact"), hide = true)]
    pub exact: bool,

    /// Assert that the `uv.lock` will remain unchanged.
    ///
    /// Requires that the lockfile is up-to-date. If the lockfile is missing or
    /// needs to be updated, uv will exit with an error.
    #[arg(long, conflicts_with = "frozen")]
    pub locked: bool,

    /// Sync without updating the `uv.lock` file.
    ///
    /// Instead of checking if the lockfile is up-to-date, uses the versions in
    /// the lockfile as the source of truth. If the lockfile is missing, uv will
    /// exit with an error. If the `pyproject.toml` includes changes to dependencies
    /// that have not been included in the lockfile yet, they will not be
    /// present in the environment.
    #[arg(long, conflicts_with = "locked")]
    pub frozen: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Sync for a specific package in the workspace.
    ///
    /// The workspace's environment (`.venv`) is updated to reflect the subset
    /// of dependencies declared by the specified workspace member package.
    ///
    /// If not in a workspace, or if the workspace member does not exist, uv
    /// will exit with an error.
    #[arg(long)]
    pub package: Option<PackageName>,

    /// The Python interpreter to use for the project environment.
    ///
    /// By default, the first interpreter that meets the project's
    /// `requires-python` constraint is used.
    ///
    /// If a Python interpreter in a virtual environment is provided, the
    /// packages will not be synced to the given environment. The interpreter
    /// will be used to create a virtual environment in the project.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
    pub python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct LockArgs {
    /// Assert that the `uv.lock` will remain unchanged.
    ///
    /// Requires that the lockfile is up-to-date. If the lockfile is missing or
    /// needs to be updated, uv will exit with an error.
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

    /// The Python interpreter to use during resolution.
    ///
    /// A Python interpreter is required for building source distributions to
    /// determine package metadata when there are not wheels.
    ///
    /// The interpreter is also used as the fallback value for the minimum
    /// Python version if `requires-python` is not set.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
    pub python: Option<String>,
}

#[derive(Args)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
#[allow(clippy::struct_excessive_bools)]
pub struct AddArgs {
    /// The packages to add, as PEP 508 requirements (e.g., `ruff==0.5.0`).
    #[arg(group = "sources")]
    pub packages: Vec<String>,

    /// Add all packages listed in the given `requirements.txt` files.
    #[arg(long, short, group = "sources", value_parser = parse_file_path)]
    pub requirements: Vec<PathBuf>,

    /// Add the requirements as development dependencies.
    #[arg(long, conflicts_with("optional"))]
    pub dev: bool,

    /// Add the requirements to the specified optional dependency group.
    ///
    /// The group may then be activated when installing the project with the
    /// `--extra` flag.
    ///
    /// To enable an optional dependency group for this requirement instead, see
    /// `--extra`.
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

    /// Commit to use when adding a dependency from Git.
    #[arg(long, group = "git-ref", action = clap::ArgAction::Set)]
    pub rev: Option<String>,

    /// Tag to use when adding a dependency from Git.
    #[arg(long, group = "git-ref", action = clap::ArgAction::Set)]
    pub tag: Option<String>,

    /// Branch to use when adding a dependency from Git.
    #[arg(long, group = "git-ref", action = clap::ArgAction::Set)]
    pub branch: Option<String>,

    /// Extras to enable for the dependency.
    ///
    /// May be provided more than once.
    ///
    /// To add this dependency to an optional group in the current project
    /// instead, see `--optional`.
    #[arg(long)]
    pub extra: Option<Vec<ExtraName>>,

    /// Avoid syncing the virtual environment after re-locking the project.
    #[arg(long, conflicts_with = "frozen")]
    pub no_sync: bool,

    /// Assert that the `uv.lock` will remain unchanged.
    ///
    /// Requires that the lockfile is up-to-date. If the lockfile is missing or
    /// needs to be updated, uv will exit with an error.
    #[arg(long, conflicts_with = "frozen")]
    pub locked: bool,

    /// Add dependencies without re-locking the project.
    ///
    /// The project environment will not be synced.
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

    /// Add the dependency to the specified Python script, rather than to a project.
    ///
    /// If provided, uv will add the dependency to the script's inline metadata
    /// table, in adhere with PEP 723. If no such inline metadata table is present,
    /// a new one will be created and added to the script. When executed via `uv run`,
    /// uv will create a temporary environment for the script with all inline
    /// dependencies installed.
    #[arg(long, conflicts_with = "dev", conflicts_with = "optional")]
    pub script: Option<PathBuf>,

    /// The Python interpreter to use for resolving and syncing.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
    pub python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct RemoveArgs {
    /// The names of the dependencies to remove (e.g., `ruff`).
    #[arg(required = true)]
    pub packages: Vec<PackageName>,

    /// Remove the packages from the development dependencies.
    #[arg(long, conflicts_with("optional"))]
    pub dev: bool,

    /// Remove the packages from the specified optional dependency group.
    #[arg(long, conflicts_with("dev"))]
    pub optional: Option<ExtraName>,

    /// Avoid syncing the virtual environment after re-locking the project.
    #[arg(long, conflicts_with = "frozen")]
    pub no_sync: bool,

    /// Assert that the `uv.lock` will remain unchanged.
    ///
    /// Requires that the lockfile is up-to-date. If the lockfile is missing or
    /// needs to be updated, uv will exit with an error.
    #[arg(long, conflicts_with = "frozen")]
    pub locked: bool,

    /// Remove dependencies without re-locking the project.
    ///
    /// The project environment will not be synced.
    #[arg(long, conflicts_with = "locked")]
    pub frozen: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Remove the dependencies from a specific package in the workspace.
    #[arg(long, conflicts_with = "isolated")]
    pub package: Option<PackageName>,

    /// Remove the dependency from the specified Python script, rather than from a project.
    ///
    /// If provided, uv will remove the dependency from the script's inline metadata
    /// table, in adhere with PEP 723.
    #[arg(long)]
    pub script: Option<PathBuf>,

    /// The Python interpreter to use for resolving and syncing.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
    pub python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct TreeArgs {
    /// Show a platform-independent dependency tree.
    ///
    /// Shows resolved package versions for all Python versions and platforms,
    /// rather than filtering to those that are relevant for the current
    /// environment.
    ///
    /// Multiple versions may be shown for a each package.
    #[arg(long)]
    pub universal: bool,

    #[command(flatten)]
    pub tree: DisplayTreeArgs,

    /// Assert that the `uv.lock` will remain unchanged.
    ///
    /// Requires that the lockfile is up-to-date. If the lockfile is missing or
    /// needs to be updated, uv will exit with an error.
    #[arg(long, conflicts_with = "frozen")]
    pub locked: bool,

    /// Display the requirements without locking the project.
    ///
    /// If the lockfile is missing, uv will exit with an error.
    #[arg(long, conflicts_with = "locked")]
    pub frozen: bool,

    #[command(flatten)]
    pub build: BuildArgs,

    #[command(flatten)]
    pub resolver: ResolverArgs,

    /// The Python version to use when filtering the tree.
    ///
    /// For example, pass `--python-version 3.10` to display the dependencies
    /// that would be included when installing on Python 3.10.
    ///
    /// Defaults to the version of the discovered Python interpreter.
    #[arg(long, conflicts_with = "universal")]
    pub python_version: Option<PythonVersion>,

    /// The platform to use when filtering the tree.
    ///
    /// For example, pass `--platform windows` to display the dependencies that
    /// would be included when installing on Windows.
    ///
    /// Represented as a "target triple", a string that describes the target
    /// platform in terms of its CPU, vendor, and operating system name, like
    /// `x86_64-unknown-linux-gnu` or `aaarch64-apple-darwin`.
    #[arg(long, conflicts_with = "universal")]
    pub python_platform: Option<TargetTriple>,

    /// The Python interpreter to use for locking and filtering.
    ///
    /// By default, the tree is filtered to match the platform as reported by
    /// the Python interpreter. Use `--universal` to display the tree for all
    /// platforms, or use `--python-version` or `--python-platform` to override
    /// a subset of markers.
    ///
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
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
    /// Run a command provided by a Python package.
    ///
    /// By default, the package to install is assumed to match the command name.
    ///
    /// The name of the command can include an exact version in the format
    /// `<package>@<version>`, e.g., `uv run ruff@0.3.0`. If more complex
    /// version specification is desired or if the command is provided by a
    /// different package, use `--from`.
    ///
    /// If the tool was previously installed, i.e., via `uv tool install`, the
    /// installed version will be used unless a version is requested or the
    /// `--isolated` flag is used.
    ///
    /// `uvx` is provided as a convenient alias for `uv tool run`, their
    /// behavior is identical.
    ///
    /// If no command is provided, the installed tools are displayed.
    ///
    /// Packages are installed into an ephemeral virtual environment in the uv
    /// cache directory.
    Run(ToolRunArgs),
    /// Hidden alias for `uv tool run` for the `uvx` command
    #[command(
        hide = true,
        override_usage = "uvx [OPTIONS] [COMMAND]",
        about = "Run a command provided by a Python package.",
        after_help = "Use `uv help tool run` for more details.",
        after_long_help = ""
    )]
    Uvx(ToolRunArgs),
    /// Install commands provided by a Python package.
    ///
    /// Packages are installed into an isolated virtual environment in the uv
    /// tools directory. The executables are linked the tool executable
    /// directory, which is determined according to the XDG standard and can be
    /// retrieved with `uv tool dir --bin`.
    ///
    /// If the tool was previously installed, the existing tool will generally
    /// be replaced.
    Install(ToolInstallArgs),
    /// Upgrade installed tools.
    ///
    /// If a tool was installed with version constraints, they will be respected
    /// on upgrade — to upgrade a tool beyond the originally provided
    /// constraints, use `uv tool install` again.
    ///
    /// If a tool was installed with specific settings, they will be respected
    /// on upgraded. For example, if `--prereleases allow` was provided during
    /// installation, it will continue to be respected in upgrades.
    #[command(alias = "update")]
    Upgrade(ToolUpgradeArgs),
    /// List installed tools.
    List(ToolListArgs),
    /// Uninstall a tool.
    Uninstall(ToolUninstallArgs),
    /// Ensure that the tool executable directory is on the `PATH`.
    ///
    /// If the tool executable directory is not present on the `PATH`, uv will
    /// attempt to add it to the relevant shell configuration files.
    ///
    /// If the shell configuration files already include a blurb to add the
    /// executable directory to the path, but the directory is not present on
    /// the `PATH`, uv will exit with an error.
    ///
    /// The tool executable directory is determined according to the XDG standard
    /// and can be retrieved with `uv tool dir --bin`.
    #[command(alias = "ensurepath")]
    UpdateShell,
    /// Show the path to the uv tools directory.
    ///
    /// The tools directory is used to store environments and metadata for installed tools.
    ///
    /// By default, tools are stored in the uv data directory at `$XDG_DATA_HOME/uv/tools` or
    /// `$HOME/.local/share/uv/tools` on Unix and `{FOLDERID_RoamingAppData}\uv\data\tools` on
    /// Windows.
    ///
    /// The tool installation directory may be overridden with `$UV_TOOL_DIR`.
    ///
    /// To instead view the directory uv installs executables into, use the `--bin` flag.
    Dir(ToolDirArgs),
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ToolRunArgs {
    /// The command to run.
    ///
    /// WARNING: The documentation for [`Self::command`] is not included in help output
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
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
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
    /// See `uv help python` for details on Python discovery and supported
    /// request formats.
    #[arg(
        long,
        short,
        env = "UV_PYTHON",
        verbatim_doc_comment,
        help_heading = "Python options"
    )]
    pub python: Option<String>,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ToolListArgs {
    /// Whether to display the path to each tool environment and installed executable.
    #[arg(long)]
    pub show_paths: bool,

    // Hide unused global Python options.
    #[arg(long, hide = true)]
    pub python_preference: Option<PythonPreference>,
    #[arg(long, hide = true)]
    pub no_python_downloads: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ToolDirArgs {
    /// Show the directory into which `uv tool` will install executables.
    ///
    /// By default, `uv tool dir` shows the directory into which the tool Python environments
    /// themselves are installed, rather than the directory containing the linked executables.
    ///
    /// The tool executable directory is determined according to the XDG standard and is derived
    /// from the following environment variables, in order of preference:
    ///
    /// - `$UV_TOOL_BIN_DIR`
    /// - `$XDG_BIN_HOME`
    /// - `$XDG_DATA_HOME/../bin`
    /// - `$HOME/.local/bin`
    #[arg(long, verbatim_doc_comment)]
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
pub struct ToolUpgradeArgs {
    /// The name of the tool to upgrade.
    #[arg(required = true)]
    pub name: Option<PackageName>,

    /// Upgrade all tools.
    #[arg(long, conflicts_with("name"))]
    pub all: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildArgs,
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
    ///
    /// By default, installed Python versions and the downloads for latest
    /// available patch version of each supported Python major version are
    /// shown.
    ///
    /// The displayed versions are filtered by the `--python-preference` option,
    /// i.e., if using `only-system`, no managed Python versions will be shown.
    ///
    /// Use `--all-versions` to view all available patch versions.
    ///
    /// Use `--only-installed` to omit available downloads.
    List(PythonListArgs),

    /// Download and install Python versions.
    ///
    /// Multiple Python versions may be requested.
    ///
    /// Supports CPython and PyPy.
    ///
    /// CPython distributions are downloaded from the `python-build-standalone` project.
    ///
    /// Python versions are installed into the uv Python directory, which can be
    /// retrieved with `uv python dir`. A `python` executable is not made
    /// globally available, managed Python versions are only used in uv
    /// commands or in active virtual environments.
    ///
    /// See `uv help python` to view supported request formats.
    Install(PythonInstallArgs),

    /// Search for a Python installation.
    ///
    /// Displays the path to the Python executable.
    ///
    /// See `uv help python` to view supported request formats and details on
    /// discovery behavior.
    Find(PythonFindArgs),

    /// Pin to a specific Python version.
    ///
    /// Writes the pinned version to a `.python-version` file, which is then
    /// read by other uv commands when determining the required Python version.
    ///
    /// See `uv help python` to view supported request formats.
    Pin(PythonPinArgs),

    /// Show the uv Python installation directory.
    ///
    /// By default, Python installations are stored in the uv data directory at
    /// `$XDG_DATA_HOME/uv/python` or `$HOME/.local/share/uv/python` on Unix and
    /// `{FOLDERID_RoamingAppData}\uv\data\python` on Windows.
    ///
    /// The Python installation directory may be overridden with `$UV_PYTHON_INSTALL_DIR`.
    Dir,

    /// Uninstall Python versions.
    Uninstall(PythonUninstallArgs),
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PythonListArgs {
    /// List all Python versions, including old patch versions.
    ///
    /// By default, only the latest patch version is shown for each minor version.
    #[arg(long)]
    pub all_versions: bool,

    /// List Python downloads for all platforms.
    ///
    /// By default, only downloads for the current platform are shown.
    #[arg(long)]
    pub all_platforms: bool,

    /// Only show installed Python versions, exclude available downloads.
    ///
    /// By default, available downloads for the current platform are shown.
    #[arg(long)]
    pub only_installed: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PythonInstallArgs {
    /// The Python version(s) to install.
    ///
    /// If not provided, the requested Python version(s) will be read from the
    /// `.python-versions` or `.python-version` files. If neither file is
    /// present, uv will check if it has installed any Python versions. If not,
    /// it will install the latest stable version of Python.
    ///
    /// See `uv help python` to view supported request formats.
    pub targets: Vec<String>,

    /// Reinstall the requested Python version, if it's already installed.
    ///
    /// By default, uv will exit successfully if the version is already
    /// installed.
    #[arg(long, short, alias = "force")]
    pub reinstall: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PythonUninstallArgs {
    /// The Python version(s) to uninstall.
    ///
    /// See `uv help python` to view supported request formats.
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
    ///
    /// See `uv help python` to view supported request formats.
    pub request: Option<String>,

    /// Avoid discovering a project or workspace.
    ///
    /// Otherwise, when no request is provided, the Python requirement of a project in the current
    /// directory or parent directories will be used.
    #[arg(long, alias = "no_workspace")]
    pub no_project: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PythonPinArgs {
    /// The Python version request.
    ///
    /// uv supports more formats than other tools that read `.python-version`
    /// files, i.e., `pyenv`. If compatibility with those tools is needed, only
    /// use version numbers instead of complex requests such as `cpython@3.10`.
    ///
    /// See `uv help python` to view supported request formats.
    pub request: Option<String>,

    /// Write the resolved Python interpreter path instead of the request.
    ///
    /// Ensures that the exact same interpreter is used.
    ///
    /// This option is usually not safe to use when committing the
    /// `.python-version` file to version control.
    #[arg(long, overrides_with("resolved"))]
    pub resolved: bool,

    #[arg(long, overrides_with("no_resolved"), hide = true)]
    pub no_resolved: bool,

    /// Avoid validating the Python pin is compatible with the workspace.
    ///
    /// By default, a workspace is discovered in the current directory or any parent
    /// directory. If a workspace is found, the Python pin is validated against
    /// the workspace's `requires-python` constraint.
    #[arg(long)]
    pub no_workspace: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct GenerateShellCompletionArgs {
    /// The shell to generate the completion script for
    pub shell: clap_complete_command::Shell,

    // Hide unused global options.
    #[arg(long, short, hide = true)]
    pub no_cache: bool,
    #[arg(long, hide = true)]
    pub cache_dir: Option<PathBuf>,

    #[arg(long, hide = true)]
    pub python_preference: Option<PythonPreference>,
    #[arg(long, hide = true)]
    pub no_python_downloads: bool,

    #[arg(long, short, conflicts_with = "verbose", hide = true)]
    pub quiet: bool,
    #[arg(long, short, action = clap::ArgAction::Count, conflicts_with = "quiet", hide = true)]
    pub verbose: u8,
    #[arg(long, default_value = "auto", conflicts_with = "no_color", hide = true)]
    pub color: ColorChoice,
    #[arg(long, hide = true)]
    pub native_tls: bool,
    #[arg(long, hide = true)]
    pub offline: bool,
    #[arg(long, hide = true)]
    pub no_progress: bool,
    #[arg(long, hide = true)]
    pub config_file: Option<PathBuf>,
    #[arg(long, hide = true)]
    pub no_config: bool,
    #[arg(long, short, action = clap::ArgAction::HelpShort, hide = true)]
    pub help: Option<bool>,
    #[arg(short = 'V', long, hide = true)]
    pub version: bool,
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
    #[arg(long, short, env = "UV_INDEX_URL", value_parser = parse_index_url, help_heading = "Index options")]
    pub index_url: Option<Maybe<IndexUrl>>,

    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    ///
    /// Accepts either a repository compliant with PEP 503 (the simple repository API), or a local
    /// directory laid out in the same format.
    ///
    /// All indexes provided via this flag take priority over the index specified by
    /// `--index-url` (which defaults to PyPI). When multiple `--extra-index-url` flags are
    /// provided, earlier values take priority.
    #[arg(long, env = "UV_EXTRA_INDEX_URL", value_delimiter = ' ', value_parser = parse_index_url, help_heading = "Index options")]
    pub extra_index_url: Option<Vec<Maybe<IndexUrl>>>,

    /// Locations to search for candidate distributions, in addition to those found in the registry
    /// indexes.
    ///
    /// If a path, the target must be a directory that contains packages as wheel files (`.whl`) or
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files adhering to the
    /// formats described above.
    #[arg(long, short, help_heading = "Index options")]
    pub find_links: Option<Vec<FlatIndexLocation>>,

    /// Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those
    /// provided via `--find-links`.
    #[arg(long, help_heading = "Index options")]
    pub no_index: bool,
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct RefreshArgs {
    /// Refresh all cached data.
    #[arg(
        long,
        conflicts_with("offline"),
        overrides_with("no_refresh"),
        help_heading = "Cache options"
    )]
    pub refresh: bool,

    #[arg(
        long,
        conflicts_with("offline"),
        overrides_with("refresh"),
        hide = true,
        help_heading = "Cache options"
    )]
    pub no_refresh: bool,

    /// Refresh cached data for a specific package.
    #[arg(long, help_heading = "Cache options")]
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
    #[arg(long, overrides_with("build"), help_heading = "Build options")]
    pub no_build: bool,

    #[arg(
        long,
        overrides_with("no_build"),
        hide = true,
        help_heading = "Build options"
    )]
    pub build: bool,

    /// Don't build source distributions for a specific package.
    #[arg(long, help_heading = "Build options")]
    pub no_build_package: Vec<PackageName>,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be built and installed from source. The resolver will still use
    /// pre-built wheels to extract package metadata, if available.
    #[arg(long, overrides_with("binary"), help_heading = "Build options")]
    pub no_binary: bool,

    #[arg(
        long,
        overrides_with("no_binary"),
        hide = true,
        help_heading = "Build options"
    )]
    pub binary: bool,

    /// Don't install pre-built wheels for a specific package.
    #[arg(long, help_heading = "Build options")]
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
    #[arg(
        long,
        alias = "force-reinstall",
        overrides_with("no_reinstall"),
        help_heading = "Installer options"
    )]
    pub reinstall: bool,

    #[arg(
        long,
        overrides_with("reinstall"),
        hide = true,
        help_heading = "Installer options"
    )]
    pub no_reinstall: bool,

    /// Reinstall a specific package, regardless of whether it's already installed. Implies
    /// `--refresh-package`.
    #[arg(long, help_heading = "Installer options")]
    pub reinstall_package: Vec<PackageName>,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-match`). This prevents
    /// "dependency confusion" attacks, whereby an attack can upload a malicious package under the
    /// same name to a secondary.
    #[arg(
        long,
        value_enum,
        env = "UV_INDEX_STRATEGY",
        help_heading = "Index options"
    )]
    pub index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to
    /// use the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(
        long,
        value_enum,
        env = "UV_KEYRING_PROVIDER",
        help_heading = "Index options"
    )]
    pub keyring_provider: Option<KeyringProviderType>,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[arg(
        long,
        short = 'C',
        alias = "config-settings",
        help_heading = "Build options"
    )]
    pub config_setting: Option<Vec<ConfigSettingEntry>>,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[arg(
        long,
        overrides_with("build_isolation"),
        help_heading = "Build options",
        env = "UV_NO_BUILD_ISOLATION",
        value_parser = clap::builder::BoolishValueParser::new(),
    )]
    pub no_build_isolation: bool,

    #[arg(
        long,
        overrides_with("no_build_isolation"),
        hide = true,
        help_heading = "Build options"
    )]
    pub build_isolation: bool,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and local dates in the same
    /// format (e.g., `2006-12-02`) in your system's configured time zone.
    #[arg(long, env = "UV_EXCLUDE_NEWER", help_heading = "Resolver options")]
    pub exclude_newer: Option<ExcludeNewer>,

    /// The method to use when installing packages from the global cache.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[arg(
        long,
        value_enum,
        env = "UV_LINK_MODE",
        help_heading = "Installer options"
    )]
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
    #[arg(
        long,
        alias = "compile",
        overrides_with("no_compile_bytecode"),
        help_heading = "Installer options"
    )]
    pub compile_bytecode: bool,

    #[arg(
        long,
        alias = "no-compile",
        overrides_with("compile_bytecode"),
        hide = true,
        help_heading = "Installer options"
    )]
    pub no_compile_bytecode: bool,

    /// Ignore the `tool.uv.sources` table when resolving dependencies. Used to lock against the
    /// standards-compliant, publishable package metadata, as opposed to using any local or Git
    /// sources.
    #[arg(long, help_heading = "Resolver options")]
    pub no_sources: bool,
}

/// Arguments that are used by commands that need to resolve (but not install) packages.
#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ResolverArgs {
    #[command(flatten)]
    pub index_args: IndexArgs,

    /// Allow package upgrades, ignoring pinned versions in any existing output file. Implies
    /// `--refresh`.
    #[arg(
        long,
        short = 'U',
        overrides_with("no_upgrade"),
        help_heading = "Resolver options"
    )]
    pub upgrade: bool,

    #[arg(
        long,
        overrides_with("upgrade"),
        hide = true,
        help_heading = "Resolver options"
    )]
    pub no_upgrade: bool,

    /// Allow upgrades for a specific package, ignoring pinned versions in any existing output
    /// file. Implies `--refresh-package`.
    #[arg(long, short = 'P', help_heading = "Resolver options")]
    pub upgrade_package: Vec<Requirement<VerbatimParsedUrl>>,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-match`). This prevents
    /// "dependency confusion" attacks, whereby an attack can upload a malicious package under the
    /// same name to a secondary.
    #[arg(
        long,
        value_enum,
        env = "UV_INDEX_STRATEGY",
        help_heading = "Index options"
    )]
    pub index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to
    /// use the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(
        long,
        value_enum,
        env = "UV_KEYRING_PROVIDER",
        help_heading = "Index options"
    )]
    pub keyring_provider: Option<KeyringProviderType>,

    /// The strategy to use when selecting between the different compatible versions for a given
    /// package requirement.
    ///
    /// By default, uv will use the latest compatible version of each package (`highest`).
    #[arg(
        long,
        value_enum,
        env = "UV_RESOLUTION",
        help_heading = "Resolver options"
    )]
    pub resolution: Option<ResolutionMode>,

    /// The strategy to use when considering pre-release versions.
    ///
    /// By default, uv will accept pre-releases for packages that _only_ publish pre-releases,
    /// along with first-party requirements that contain an explicit pre-release marker in the
    /// declared specifiers (`if-necessary-or-explicit`).
    #[arg(
        long,
        value_enum,
        env = "UV_PRERELEASE",
        help_heading = "Resolver options"
    )]
    pub prerelease: Option<PrereleaseMode>,

    #[arg(long, hide = true, help_heading = "Resolver options")]
    pub pre: bool,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[arg(
        long,
        short = 'C',
        alias = "config-settings",
        help_heading = "Build options"
    )]
    pub config_setting: Option<Vec<ConfigSettingEntry>>,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[arg(
        long,
        overrides_with("build_isolation"),
        help_heading = "Build options",
        env = "UV_NO_BUILD_ISOLATION",
        value_parser = clap::builder::BoolishValueParser::new(),
    )]
    pub no_build_isolation: bool,

    /// Disable isolation when building source distributions for a specific package.
    ///
    /// Assumes that the packages' build dependencies specified by PEP 518  are already installed.
    #[arg(long, help_heading = "Build options")]
    pub no_build_isolation_package: Vec<PackageName>,

    #[arg(
        long,
        overrides_with("no_build_isolation"),
        hide = true,
        help_heading = "Build options"
    )]
    pub build_isolation: bool,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and local dates in the same
    /// format (e.g., `2006-12-02`) in your system's configured time zone.
    #[arg(long, env = "UV_EXCLUDE_NEWER", help_heading = "Resolver options")]
    pub exclude_newer: Option<ExcludeNewer>,

    /// The method to use when installing packages from the global cache.
    ///
    /// This option is only used when building source distributions.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[arg(
        long,
        value_enum,
        env = "UV_LINK_MODE",
        help_heading = "Installer options"
    )]
    pub link_mode: Option<install_wheel_rs::linker::LinkMode>,

    /// Ignore the `tool.uv.sources` table when resolving dependencies. Used to lock against the
    /// standards-compliant, publishable package metadata, as opposed to using any local or Git
    /// sources.
    #[arg(long, help_heading = "Resolver options")]
    pub no_sources: bool,
}

/// Arguments that are used by commands that need to resolve and install packages.
#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ResolverInstallerArgs {
    #[command(flatten)]
    pub index_args: IndexArgs,

    /// Allow package upgrades, ignoring pinned versions in any existing output file. Implies
    /// `--refresh`.
    #[arg(
        long,
        short = 'U',
        overrides_with("no_upgrade"),
        help_heading = "Resolver options"
    )]
    pub upgrade: bool,

    #[arg(
        long,
        overrides_with("upgrade"),
        hide = true,
        help_heading = "Resolver options"
    )]
    pub no_upgrade: bool,

    /// Allow upgrades for a specific package, ignoring pinned versions in any existing output
    /// file. Implies `--refresh-package`.
    #[arg(long, short = 'P', help_heading = "Resolver options")]
    pub upgrade_package: Vec<Requirement<VerbatimParsedUrl>>,

    /// Reinstall all packages, regardless of whether they're already installed. Implies
    /// `--refresh`.
    #[arg(
        long,
        alias = "force-reinstall",
        overrides_with("no_reinstall"),
        help_heading = "Installer options"
    )]
    pub reinstall: bool,

    #[arg(
        long,
        overrides_with("reinstall"),
        hide = true,
        help_heading = "Installer options"
    )]
    pub no_reinstall: bool,

    /// Reinstall a specific package, regardless of whether it's already installed. Implies
    /// `--refresh-package`.
    #[arg(long, help_heading = "Installer options")]
    pub reinstall_package: Vec<PackageName>,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-match`). This prevents
    /// "dependency confusion" attacks, whereby an attack can upload a malicious package under the
    /// same name to a secondary.
    #[arg(
        long,
        value_enum,
        env = "UV_INDEX_STRATEGY",
        help_heading = "Index options"
    )]
    pub index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to
    /// use the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(
        long,
        value_enum,
        env = "UV_KEYRING_PROVIDER",
        help_heading = "Index options"
    )]
    pub keyring_provider: Option<KeyringProviderType>,

    /// The strategy to use when selecting between the different compatible versions for a given
    /// package requirement.
    ///
    /// By default, uv will use the latest compatible version of each package (`highest`).
    #[arg(
        long,
        value_enum,
        env = "UV_RESOLUTION",
        help_heading = "Resolver options"
    )]
    pub resolution: Option<ResolutionMode>,

    /// The strategy to use when considering pre-release versions.
    ///
    /// By default, uv will accept pre-releases for packages that _only_ publish pre-releases,
    /// along with first-party requirements that contain an explicit pre-release marker in the
    /// declared specifiers (`if-necessary-or-explicit`).
    #[arg(
        long,
        value_enum,
        env = "UV_PRERELEASE",
        help_heading = "Resolver options"
    )]
    pub prerelease: Option<PrereleaseMode>,

    #[arg(long, hide = true)]
    pub pre: bool,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[arg(
        long,
        short = 'C',
        alias = "config-settings",
        help_heading = "Build options"
    )]
    pub config_setting: Option<Vec<ConfigSettingEntry>>,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[arg(
        long,
        overrides_with("build_isolation"),
        help_heading = "Build options",
        env = "UV_NO_BUILD_ISOLATION",
        value_parser = clap::builder::BoolishValueParser::new(),
    )]
    pub no_build_isolation: bool,

    /// Disable isolation when building source distributions for a specific package.
    ///
    /// Assumes that the packages' build dependencies specified by PEP 518  are already installed.
    #[arg(long, help_heading = "Build options")]
    pub no_build_isolation_package: Vec<PackageName>,

    #[arg(
        long,
        overrides_with("no_build_isolation"),
        hide = true,
        help_heading = "Build options"
    )]
    pub build_isolation: bool,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and local dates in the same
    /// format (e.g., `2006-12-02`) in your system's configured time zone.
    #[arg(long, env = "UV_EXCLUDE_NEWER", help_heading = "Resolver options")]
    pub exclude_newer: Option<ExcludeNewer>,

    /// The method to use when installing packages from the global cache.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[arg(
        long,
        value_enum,
        env = "UV_LINK_MODE",
        help_heading = "Installer options"
    )]
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
    #[arg(
        long,
        alias = "compile",
        overrides_with("no_compile_bytecode"),
        help_heading = "Installer options"
    )]
    pub compile_bytecode: bool,

    #[arg(
        long,
        alias = "no-compile",
        overrides_with("compile_bytecode"),
        hide = true,
        help_heading = "Installer options"
    )]
    pub no_compile_bytecode: bool,

    /// Ignore the `tool.uv.sources` table when resolving dependencies. Used to lock against the
    /// standards-compliant, publishable package metadata, as opposed to using any local or Git
    /// sources.
    #[arg(long, help_heading = "Resolver options")]
    pub no_sources: bool,
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
}
