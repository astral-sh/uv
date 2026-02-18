use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Result, anyhow};
use clap::builder::styling::{AnsiColor, Effects, Style};
use clap::builder::{PossibleValue, Styles, TypedValueParser, ValueParserFactory};
use clap::error::ErrorKind;
use clap::{Args, Parser, Subcommand};
use clap::{ValueEnum, ValueHint};

use uv_auth::Service;
use uv_cache::CacheArgs;
use uv_configuration::{
    ExportFormat, IndexStrategy, KeyringProviderType, PackageNameSpecifier, PipCompileFormat,
    ProjectBuildBackend, TargetTriple, TrustedHost, TrustedPublishing, VersionControlSystem,
};
use uv_distribution_types::{
    ConfigSettingEntry, ConfigSettingPackageEntry, Index, IndexUrl, Origin, PipExtraIndex,
    PipFindLinks, PipIndex,
};
use uv_normalize::{ExtraName, GroupName, PackageName, PipGroupName};
use uv_pep508::{MarkerTree, Requirement};
use uv_preview::PreviewFeature;
use uv_pypi_types::VerbatimParsedUrl;
use uv_python::{PythonDownloads, PythonPreference, PythonVersion};
use uv_redacted::DisplaySafeUrl;
use uv_resolver::{
    AnnotationStyle, ExcludeNewerPackageEntry, ExcludeNewerValue, ForkStrategy, PrereleaseMode,
    ResolutionMode,
};
use uv_settings::PythonInstallMirrors;
use uv_static::EnvVars;
use uv_torch::TorchMode;
use uv_workspace::pyproject_mut::AddBoundsKind;

pub mod comma;
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

#[derive(Debug, Default, Clone, Copy, clap::ValueEnum)]
pub enum PythonListFormat {
    /// Plain text (for humans).
    #[default]
    Text,
    /// JSON (for computers).
    Json,
}

#[derive(Debug, Default, Clone, Copy, clap::ValueEnum)]
pub enum SyncFormat {
    /// Display the result in a human-readable format.
    #[default]
    Text,
    /// Display the result in JSON format.
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

// Configures Clap v3-style help menu colors
const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default());

#[derive(Parser)]
#[command(name = "uv", author, long_version = crate::version::uv_self_version())]
#[command(about = "An extremely fast Python package manager.")]
#[command(
    after_help = "Use `uv help` for more details.",
    after_long_help = "",
    disable_help_flag = true,
    disable_help_subcommand = true,
    disable_version_flag = true
)]
#[command(styles=STYLES)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Box<Commands>,

    #[command(flatten)]
    pub top_level: TopLevelArgs,
}

#[derive(Parser)]
#[command(disable_help_flag = true, disable_version_flag = true)]
pub struct TopLevelArgs {
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
        env = EnvVars::UV_CONFIG_FILE,
        help_heading = "Global options",
        value_hint = ValueHint::FilePath,
    )]
    pub config_file: Option<PathBuf>,

    /// Avoid discovering configuration files (`pyproject.toml`, `uv.toml`).
    ///
    /// Normally, configuration files are discovered in the current directory,
    /// parent directories, or user configuration directories.
    #[arg(global = true, long, env = EnvVars::UV_NO_CONFIG, value_parser = clap::builder::BoolishValueParser::new(), help_heading = "Global options")]
    pub no_config: bool,

    /// Display the concise help for this command.
    #[arg(global = true, short, long, action = clap::ArgAction::HelpShort, help_heading = "Global options")]
    help: Option<bool>,

    /// Display the uv version.
    #[arg(short = 'V', long, action = clap::ArgAction::Version)]
    version: Option<bool>,
}

#[derive(Parser, Debug, Clone)]
#[command(next_help_heading = "Global options", next_display_order = 1000)]
pub struct GlobalArgs {
    #[arg(
        global = true,
        long,
        help_heading = "Python options",
        display_order = 700,
        env = EnvVars::UV_PYTHON_PREFERENCE,
        hide = true
    )]
    pub python_preference: Option<PythonPreference>,

    /// Require use of uv-managed Python versions [env: UV_MANAGED_PYTHON=]
    ///
    /// By default, uv prefers using Python versions it manages. However, it will use system Python
    /// versions if a uv-managed Python is not installed. This option disables use of system Python
    /// versions.
    #[arg(
        global = true,
        long,
        help_heading = "Python options",
        overrides_with = "no_managed_python"
    )]
    pub managed_python: bool,

    /// Disable use of uv-managed Python versions [env: UV_NO_MANAGED_PYTHON=]
    ///
    /// Instead, uv will search for a suitable Python version on the system.
    #[arg(
        global = true,
        long,
        help_heading = "Python options",
        overrides_with = "managed_python"
    )]
    pub no_managed_python: bool,

    #[expect(clippy::doc_markdown)]
    /// Allow automatically downloading Python when required. [env: "UV_PYTHON_DOWNLOADS=auto"]
    #[arg(global = true, long, help_heading = "Python options", hide = true)]
    pub allow_python_downloads: bool,

    #[expect(clippy::doc_markdown)]
    /// Disable automatic downloads of Python. [env: "UV_PYTHON_DOWNLOADS=never"]
    #[arg(global = true, long, help_heading = "Python options")]
    pub no_python_downloads: bool,

    /// Deprecated version of [`Self::python_downloads`].
    #[arg(global = true, long, hide = true)]
    pub python_fetch: Option<PythonDownloads>,

    /// Use quiet output.
    ///
    /// Repeating this option, e.g., `-qq`, will enable a silent mode in which
    /// uv will write no output to stdout.
    #[arg(global = true, action = clap::ArgAction::Count, long, short, conflicts_with = "verbose")]
    pub quiet: u8,

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

    /// Control the use of color in output.
    ///
    /// By default, uv will automatically detect support for colors when writing to a terminal.
    #[arg(
        global = true,
        long,
        value_enum,
        conflicts_with = "no_color",
        value_name = "COLOR_CHOICE"
    )]
    pub color: Option<ColorChoice>,

    /// Whether to load TLS certificates from the platform's native store [env: UV_NATIVE_TLS=]
    ///
    /// By default, uv loads certificates from the bundled `webpki-roots` crate. The
    /// `webpki-roots` are a reliable set of trust roots from Mozilla, and including them in uv
    /// improves portability and performance (especially on macOS).
    ///
    /// However, in some cases, you may want to use the platform's native certificate store,
    /// especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's
    /// included in your system's certificate store.
    #[arg(global = true, long, value_parser = clap::builder::BoolishValueParser::new(), overrides_with("no_native_tls"))]
    pub native_tls: bool,

    #[arg(global = true, long, overrides_with("native_tls"), hide = true)]
    pub no_native_tls: bool,

    /// Disable network access [env: UV_OFFLINE=]
    ///
    /// When disabled, uv will only use locally cached data and locally available files.
    #[arg(global = true, long, overrides_with("no_offline"))]
    pub offline: bool,

    #[arg(global = true, long, overrides_with("offline"), hide = true)]
    pub no_offline: bool,

    /// Allow insecure connections to a host.
    ///
    /// Can be provided multiple times.
    ///
    /// Expects to receive either a hostname (e.g., `localhost`), a host-port pair (e.g.,
    /// `localhost:8080`), or a URL (e.g., `https://localhost`).
    ///
    /// WARNING: Hosts included in this list will not be verified against the system's certificate
    /// store. Only use `--allow-insecure-host` in a secure network with verified sources, as it
    /// bypasses SSL verification and could expose you to MITM attacks.
    #[arg(
        global = true,
        long,
        alias = "trusted-host",
        env = EnvVars::UV_INSECURE_HOST,
        value_delimiter = ' ',
        value_parser = parse_insecure_host,
        value_hint = ValueHint::Url,
    )]
    pub allow_insecure_host: Option<Vec<Maybe<TrustedHost>>>,

    /// Whether to enable all experimental preview features [env: UV_PREVIEW=]
    ///
    /// Preview features may change without warning.
    #[arg(global = true, long, hide = true, value_parser = clap::builder::BoolishValueParser::new(), overrides_with("no_preview"))]
    pub preview: bool,

    #[arg(global = true, long, overrides_with("preview"), hide = true)]
    pub no_preview: bool,

    /// Enable experimental preview features.
    ///
    /// Preview features may change without warning.
    ///
    /// Use comma-separated values or pass multiple times to enable multiple features.
    ///
    /// The following features are available: `python-install-default`, `python-upgrade`,
    /// `json-output`, `pylock`, `add-bounds`.
    #[arg(
        global = true,
        long = "preview-features",
        env = EnvVars::UV_PREVIEW_FEATURES,
        value_delimiter = ',',
        hide = true,
        alias = "preview-feature",
        value_enum,
    )]
    pub preview_features: Vec<PreviewFeature>,

    /// Avoid discovering a `pyproject.toml` or `uv.toml` file [env: UV_ISOLATED=]
    ///
    /// Normally, configuration files are discovered in the current directory,
    /// parent directories, or user configuration directories.
    ///
    /// This option is deprecated in favor of `--no-config`.
    #[arg(global = true, long, hide = true, value_parser = clap::builder::BoolishValueParser::new())]
    pub isolated: bool,

    /// Show the resolved settings for the current command.
    ///
    /// This option is used for debugging and development purposes.
    #[arg(global = true, long, hide = true)]
    pub show_settings: bool,

    /// Hide all progress outputs [env: UV_NO_PROGRESS=]
    ///
    /// For example, spinners or progress bars.
    #[arg(global = true, long, value_parser = clap::builder::BoolishValueParser::new())]
    pub no_progress: bool,

    /// Skip writing `uv` installer metadata files (e.g., `INSTALLER`, `REQUESTED`, and
    /// `direct_url.json`) to site-packages `.dist-info` directories [env: UV_NO_INSTALLER_METADATA=]
    #[arg(global = true, long, hide = true, value_parser = clap::builder::BoolishValueParser::new())]
    pub no_installer_metadata: bool,

    /// Change to the given directory prior to running the command.
    ///
    /// Relative paths are resolved with the given directory as the base.
    ///
    /// See `--project` to only change the project root directory.
    #[arg(global = true, long, env = EnvVars::UV_WORKING_DIR, value_hint = ValueHint::DirPath)]
    pub directory: Option<PathBuf>,

    /// Discover a project in the given directory.
    ///
    /// All `pyproject.toml`, `uv.toml`, and `.python-version` files will be discovered by walking
    /// up the directory tree from the project root, as will the project's virtual environment
    /// (`.venv`).
    ///
    /// Other command-line arguments (such as relative paths) will be resolved relative
    /// to the current working directory.
    ///
    /// See `--directory` to change the working directory entirely.
    ///
    /// This setting has no effect when used in the `uv pip` interface.
    #[arg(global = true, long, env = EnvVars::UV_PROJECT, value_hint = ValueHint::DirPath)]
    pub project: Option<PathBuf>,
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

impl ColorChoice {
    /// Combine self (higher priority) with an [`anstream::ColorChoice`] (lower priority).
    ///
    /// This method allows prioritizing the user choice, while using the inferred choice for a
    /// stream as default.
    #[must_use]
    pub fn and_colorchoice(self, next: anstream::ColorChoice) -> Self {
        match self {
            Self::Auto => match next {
                anstream::ColorChoice::Auto => Self::Auto,
                anstream::ColorChoice::Always | anstream::ColorChoice::AlwaysAnsi => Self::Always,
                anstream::ColorChoice::Never => Self::Never,
            },
            Self::Always | Self::Never => self,
        }
    }
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
pub enum Commands {
    /// Manage authentication.
    #[command(
        after_help = "Use `uv help auth` for more details.",
        after_long_help = ""
    )]
    Auth(AuthNamespace),

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
    /// Generally, uv first searches for Python in a virtual environment, either active or in a
    /// `.venv` directory in the current working directory or any parent directory. If a virtual
    /// environment is not required, uv will then search for a Python interpreter. Python
    /// interpreters are found by searching for Python executables in the `PATH` environment
    /// variable.
    ///
    /// On Windows, the registry is also searched for Python executables.
    ///
    /// By default, uv will download Python if a version cannot be found. This behavior can be
    /// disabled with the `--no-python-downloads` flag or the `python-downloads` setting.
    ///
    /// The `--python` option allows requesting a different interpreter.
    ///
    /// The following Python version request formats are supported:
    ///
    /// - `<version>` e.g. `3`, `3.12`, `3.12.3`
    /// - `<version-specifier>` e.g. `>=3.12,<3.13`
    /// - `<version><short-variant>` (e.g., `3.13t`, `3.12.0d`)
    /// - `<version>+<variant>` (e.g., `3.13+freethreaded`, `3.12.0+debug`)
    /// - `<implementation>` e.g. `cpython` or `cp`
    /// - `<implementation>@<version>` e.g. `cpython@3.12`
    /// - `<implementation><version>` e.g. `cpython3.12` or `cp312`
    /// - `<implementation><version-specifier>` e.g. `cpython>=3.12,<3.13`
    /// - `<implementation>-<version>-<os>-<arch>-<libc>` e.g. `cpython-3.12.3-macos-aarch64-none`
    ///
    /// Additionally, a specific system Python interpreter can often be requested with:
    ///
    /// - `<executable-path>` e.g. `/opt/homebrew/bin/python3`
    /// - `<executable-name>` e.g. `mypython3`
    /// - `<install-dir>` e.g. `/some/environment/`
    ///
    /// When the `--python` option is used, normal discovery rules apply but discovered interpreters
    /// are checked for compatibility with the request, e.g., if `pypy` is requested, uv will first
    /// check if the virtual environment contains a PyPy interpreter then check if each executable
    /// in the path is a PyPy interpreter.
    ///
    /// uv supports discovering CPython, PyPy, and GraalPy interpreters. Unsupported interpreters
    /// will be skipped during discovery. If an unsupported interpreter implementation is requested,
    /// uv will exit with an error.
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
    /// If in a project, the default environment name can be changed with
    /// the `UV_PROJECT_ENVIRONMENT` environment variable; this only applies
    /// when run from the project root directory.
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
    /// Build Python packages into source distributions and wheels.
    ///
    /// `uv build` accepts a path to a directory or source distribution,
    /// which defaults to the current working directory.
    ///
    /// By default, if passed a directory, `uv build` will build a source
    /// distribution ("sdist") from the source directory, and a binary
    /// distribution ("wheel") from the source distribution.
    ///
    /// `uv build --sdist` can be used to build only the source distribution,
    /// `uv build --wheel` can be used to build only the binary distribution,
    /// and `uv build --sdist --wheel` can be used to build both distributions
    /// from source.
    ///
    /// If passed a source distribution, `uv build --wheel` will build a wheel
    /// from the source distribution.
    #[command(
        after_help = "Use `uv help build` for more details.",
        after_long_help = ""
    )]
    Build(BuildArgs),
    /// Upload distributions to an index.
    Publish(PublishArgs),
    /// Inspect uv workspaces.
    #[command(
        after_help = "Use `uv help workspace` for more details.",
        after_long_help = "",
        hide = true
    )]
    Workspace(WorkspaceNamespace),
    /// The implementation of the build backend.
    ///
    /// These commands are not directly exposed to the user, instead users invoke their build
    /// frontend (PEP 517) which calls the Python shims which calls back into uv with this method.
    #[command(hide = true)]
    BuildBackend {
        #[command(subcommand)]
        command: BuildBackendCommand,
    },
    /// Manage uv's cache.
    #[command(
        after_help = "Use `uv help cache` for more details.",
        after_long_help = ""
    )]
    Cache(CacheNamespace),
    /// Manage the uv executable.
    #[command(name = "self")]
    Self_(SelfNamespace),
    /// Clear the cache, removing all entries or those linked to specific packages.
    #[command(hide = true)]
    Clean(CleanArgs),
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
  {option}--no-pager{option:#} Disable pager when printing help
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

    #[arg(value_hint = ValueHint::Other)]
    pub command: Option<Vec<String>>,
}

#[derive(Args)]
#[command(group = clap::ArgGroup::new("operation"))]
pub struct VersionArgs {
    /// Set the project version to this value
    ///
    /// To update the project using semantic versioning components instead, use `--bump`.
    #[arg(group = "operation", value_hint = ValueHint::Other)]
    pub value: Option<String>,

    /// Update the project version using the given semantics
    ///
    /// This flag can be passed multiple times.
    #[arg(group = "operation", long, value_name = "BUMP[=VALUE]")]
    pub bump: Vec<VersionBumpSpec>,

    /// Don't write a new version to the `pyproject.toml`
    ///
    /// Instead, the version will be displayed.
    #[arg(long)]
    pub dry_run: bool,

    /// Only show the version
    ///
    /// By default, uv will show the project name before the version.
    #[arg(long)]
    pub short: bool,

    /// The format of the output
    #[arg(long, value_enum, default_value = "text")]
    pub output_format: VersionFormat,

    /// Avoid syncing the virtual environment after re-locking the project [env: UV_NO_SYNC=]
    #[arg(long)]
    pub no_sync: bool,

    /// Prefer the active virtual environment over the project's virtual environment.
    ///
    /// If the project virtual environment is active or no virtual environment is active, this has
    /// no effect.
    #[arg(long, overrides_with = "no_active")]
    pub active: bool,

    /// Prefer project's virtual environment over an active environment.
    ///
    /// This is the default behavior.
    #[arg(long, overrides_with = "active", hide = true)]
    pub no_active: bool,

    /// Assert that the `uv.lock` will remain unchanged [env: UV_LOCKED=]
    ///
    /// Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated,
    /// uv will exit with an error.
    #[arg(long, conflicts_with_all = ["frozen", "upgrade"])]
    pub locked: bool,

    /// Update the version without re-locking the project [env: UV_FROZEN=]
    ///
    /// The project environment will not be synced.
    #[arg(long, conflicts_with_all = ["locked", "upgrade", "no_sources"])]
    pub frozen: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildOptionsArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Update the version of a specific package in the workspace.
    #[arg(long, conflicts_with = "isolated", value_hint = ValueHint::Other)]
    pub package: Option<PackageName>,

    /// The Python interpreter to use for resolving and syncing.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,
}

// Note that the ordering of the variants is significant, as when given a list of operations
// to perform, we sort them and apply them in order, so users don't have to think too hard about it.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum VersionBump {
    /// Increase the major version (e.g., 1.2.3 => 2.0.0)
    Major,
    /// Increase the minor version (e.g., 1.2.3 => 1.3.0)
    Minor,
    /// Increase the patch version (e.g., 1.2.3 => 1.2.4)
    Patch,
    /// Move from a pre-release to stable version (e.g., 1.2.3b4.post5.dev6 => 1.2.3)
    ///
    /// Removes all pre-release components, but will not remove "local" components.
    Stable,
    /// Increase the alpha version (e.g., 1.2.3a4 => 1.2.3a5)
    ///
    /// To move from a stable to a pre-release version, combine this with a stable component, e.g.,
    /// for 1.2.3 => 2.0.0a1, you'd also include [`VersionBump::Major`].
    Alpha,
    /// Increase the beta version (e.g., 1.2.3b4 => 1.2.3b5)
    ///
    /// To move from a stable to a pre-release version, combine this with a stable component, e.g.,
    /// for 1.2.3 => 2.0.0b1, you'd also include [`VersionBump::Major`].
    Beta,
    /// Increase the rc version (e.g., 1.2.3rc4 => 1.2.3rc5)
    ///
    /// To move from a stable to a pre-release version, combine this with a stable component, e.g.,
    /// for 1.2.3 => 2.0.0rc1, you'd also include [`VersionBump::Major`].]
    Rc,
    /// Increase the post version (e.g., 1.2.3.post5 => 1.2.3.post6)
    Post,
    /// Increase the dev version (e.g., 1.2.3a4.dev6 => 1.2.3.dev7)
    Dev,
}

impl Display for VersionBump {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let string = match self {
            Self::Major => "major",
            Self::Minor => "minor",
            Self::Patch => "patch",
            Self::Stable => "stable",
            Self::Alpha => "alpha",
            Self::Beta => "beta",
            Self::Rc => "rc",
            Self::Post => "post",
            Self::Dev => "dev",
        };
        string.fmt(f)
    }
}

impl FromStr for VersionBump {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "major" => Ok(Self::Major),
            "minor" => Ok(Self::Minor),
            "patch" => Ok(Self::Patch),
            "stable" => Ok(Self::Stable),
            "alpha" => Ok(Self::Alpha),
            "beta" => Ok(Self::Beta),
            "rc" => Ok(Self::Rc),
            "post" => Ok(Self::Post),
            "dev" => Ok(Self::Dev),
            _ => Err(format!("invalid bump component `{value}`")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VersionBumpSpec {
    pub bump: VersionBump,
    pub value: Option<u64>,
}

impl Display for VersionBumpSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.value {
            Some(value) => write!(f, "{}={value}", self.bump),
            None => self.bump.fmt(f),
        }
    }
}

impl FromStr for VersionBumpSpec {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (name, value) = match input.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (input, None),
        };

        let bump = name.parse::<VersionBump>()?;

        if bump == VersionBump::Stable && value.is_some() {
            return Err("`--bump stable` does not accept a value".to_string());
        }

        let value = match value {
            Some("") => {
                return Err("`--bump` values cannot be empty".to_string());
            }
            Some(raw) => Some(
                raw.parse::<u64>()
                    .map_err(|_| format!("invalid numeric value `{raw}` for `--bump {name}`"))?,
            ),
            None => None,
        };

        Ok(Self { bump, value })
    }
}

impl ValueParserFactory for VersionBumpSpec {
    type Parser = VersionBumpSpecValueParser;

    fn value_parser() -> Self::Parser {
        VersionBumpSpecValueParser
    }
}

#[derive(Clone, Debug)]
pub struct VersionBumpSpecValueParser;

impl TypedValueParser for VersionBumpSpecValueParser {
    type Value = VersionBumpSpec;

    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let raw = value.to_str().ok_or_else(|| {
            clap::Error::raw(
                ErrorKind::InvalidUtf8,
                "`--bump` values must be valid UTF-8",
            )
        })?;

        VersionBumpSpec::from_str(raw)
            .map_err(|message| clap::Error::raw(ErrorKind::InvalidValue, message))
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(
            VersionBump::value_variants()
                .iter()
                .filter_map(ValueEnum::to_possible_value),
        ))
    }
}

#[derive(Args)]
pub struct SelfNamespace {
    #[command(subcommand)]
    pub command: SelfCommand,
}

#[derive(Subcommand)]
pub enum SelfCommand {
    /// Update uv.
    Update(SelfUpdateArgs),
    /// Display uv's version
    Version {
        /// Only print the version
        #[arg(long)]
        short: bool,
        #[arg(long, value_enum, default_value = "text")]
        output_format: VersionFormat,
    },
}

#[derive(Args, Debug)]
pub struct SelfUpdateArgs {
    /// Update to the specified version. If not provided, uv will update to the latest version.
    #[arg(value_hint = ValueHint::Other)]
    pub target_version: Option<String>,

    /// A GitHub token for authentication.
    /// A token is not required but can be used to reduce the chance of encountering rate limits.
    #[arg(long, env = EnvVars::UV_GITHUB_TOKEN, value_hint = ValueHint::Other)]
    pub token: Option<String>,

    /// Run without performing the update.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
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
    /// By default, the cache is stored in `$XDG_CACHE_HOME/uv` or `$HOME/.cache/uv` on Unix and
    /// `%LOCALAPPDATA%\uv\cache` on Windows.
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
    /// Show the cache size.
    ///
    /// Displays the total size of the cache directory. This includes all downloaded and built
    /// wheels, source distributions, and other cached data. By default, outputs the size in raw
    /// bytes; use `--human` for human-readable output.
    Size(SizeArgs),
}

#[derive(Args, Debug)]
pub struct CleanArgs {
    /// The packages to remove from the cache.
    #[arg(value_hint = ValueHint::Other)]
    pub package: Vec<PackageName>,

    /// Force removal of the cache, ignoring in-use checks.
    ///
    /// By default, `uv cache clean` will block until no process is reading the cache. When
    /// `--force` is used, `uv cache clean` will proceed without taking a lock.
    #[arg(long)]
    pub force: bool,
}

#[derive(Args, Debug)]
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

    /// Force removal of the cache, ignoring in-use checks.
    ///
    /// By default, `uv cache prune` will block until no process is reading the cache. When
    /// `--force` is used, `uv cache prune` will proceed without taking a lock.
    #[arg(long)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct SizeArgs {
    /// Display the cache size in human-readable format (e.g., `1.2 GiB` instead of raw bytes).
    #[arg(long = "human", short = 'H', alias = "human-readable")]
    pub human: bool,
}

#[derive(Args)]
pub struct PipNamespace {
    #[command(subcommand)]
    pub command: PipCommand,
}

#[derive(Subcommand)]
pub enum PipCommand {
    /// Compile a `requirements.in` file to a `requirements.txt` or `pylock.toml` file.
    #[command(
        after_help = "Use `uv help pip compile` for more details.",
        after_long_help = ""
    )]
    Compile(PipCompileArgs),
    /// Sync an environment with a `requirements.txt` or `pylock.toml` file.
    ///
    /// When syncing an environment, any packages not listed in the `requirements.txt` or
    /// `pylock.toml` file will be removed. To retain extraneous packages, use `uv pip install`
    /// instead.
    ///
    /// The input file is presumed to be the output of a `pip compile` or `uv export` operation,
    /// in which it will include all transitive dependencies. If transitive dependencies are not
    /// present in the file, they will not be installed. Use `--strict` to warn if any transitive
    /// dependencies are missing.
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
        after_long_help = "",
        alias = "ls"
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
    /// Display debug information (unsupported)
    #[command(hide = true)]
    Debug(PipDebugArgs),
}

#[derive(Subcommand)]
pub enum ProjectCommand {
    /// Run a command or script.
    ///
    /// Ensures that the command runs in a Python environment.
    ///
    /// When used with a file ending in `.py` or an HTTP(S) URL, the file will be treated as a
    /// script and run with a Python interpreter, i.e., `uv run file.py` is equivalent to `uv run
    /// python file.py`. For URLs, the script is temporarily downloaded before execution. If the
    /// script contains inline dependency metadata, it will be installed into an isolated, ephemeral
    /// environment. When used with `-`, the input will be read from stdin, and treated as a Python
    /// script.
    ///
    /// When used in a project, the project environment will be created and updated before invoking
    /// the command.
    ///
    /// When used outside a project, if a virtual environment can be found in the current directory
    /// or a parent directory, the command will be run in that environment. Otherwise, the command
    /// will be run in the environment of the discovered interpreter.
    ///
    /// By default, the project or workspace is discovered from the current working directory.
    /// However, when using `--preview-features target-workspace-discovery`, the project or
    /// workspace is instead discovered from the target script's directory.
    ///
    /// Arguments following the command (or script) are not interpreted as arguments to uv. All
    /// options to uv must be provided before the command, e.g., `uv run --verbose foo`. A `--` can
    /// be used to separate the command from uv options for clarity, e.g., `uv run --python 3.12 --
    /// python`.
    #[command(
        after_help = "Use `uv help run` for more details.",
        after_long_help = ""
    )]
    Run(RunArgs),
    /// Create a new project.
    ///
    /// Follows the `pyproject.toml` specification.
    ///
    /// If a `pyproject.toml` already exists at the target, uv will exit with an error.
    ///
    /// If a `pyproject.toml` is found in any of the parent directories of the target path, the
    /// project will be added as a workspace member of the parent.
    ///
    /// Some project state is not created until needed, e.g., the project virtual environment
    /// (`.venv`) and lockfile (`uv.lock`) are lazily created during the first sync.
    Init(InitArgs),
    /// Add dependencies to the project.
    ///
    /// Dependencies are added to the project's `pyproject.toml` file.
    ///
    /// If a given dependency exists already, it will be updated to the new version specifier unless
    /// it includes markers that differ from the existing specifier in which case another entry for
    /// the dependency will be added.
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
    /// The lockfile and project environment will be updated to reflect the removed dependencies. To
    /// skip updating the lockfile, use `--frozen`. To skip updating the environment, use
    /// `--no-sync`.
    ///
    /// If any of the requested dependencies are not present in the project, uv will exit with an
    /// error.
    ///
    /// If a package has been manually installed in the environment, i.e., with `uv pip install`, it
    /// will not be removed by `uv remove`.
    ///
    /// uv will search for a project in the current directory or any parent directory. If a project
    /// cannot be found, uv will exit with an error.
    #[command(
        after_help = "Use `uv help remove` for more details.",
        after_long_help = ""
    )]
    Remove(RemoveArgs),
    /// Read or update the project's version.
    Version(VersionArgs),
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
    /// If the project lockfile (`uv.lock`) does not exist, it will be created. If a lockfile is
    /// present, its contents will be used as preferences for the resolution.
    ///
    /// If there are no changes to the project's dependencies, locking will have no effect unless
    /// the `--upgrade` flag is provided.
    #[command(
        after_help = "Use `uv help lock` for more details.",
        after_long_help = ""
    )]
    Lock(LockArgs),
    /// Export the project's lockfile to an alternate format.
    ///
    /// At present, `requirements.txt`, `pylock.toml` (PEP 751) and CycloneDX v1.5 JSON output
    /// formats are supported.
    ///
    /// The project is re-locked before exporting unless the `--locked` or `--frozen` flag is
    /// provided.
    ///
    /// uv will search for a project in the current directory or any parent directory. If a project
    /// cannot be found, uv will exit with an error.
    ///
    /// If operating in a workspace, the root will be exported by default; however, specific
    /// members can be selected using the `--package` option.
    #[command(
        after_help = "Use `uv help export` for more details.",
        after_long_help = ""
    )]
    Export(ExportArgs),
    /// Display the project's dependency tree.
    Tree(TreeArgs),
    /// Format Python code in the project.
    ///
    /// Formats Python code using the Ruff formatter. By default, all Python files in the project
    /// are formatted. This command has the same behavior as running `ruff format` in the project
    /// root.
    ///
    /// To check if files are formatted without modifying them, use `--check`. To see a diff of
    /// formatting changes, use `--diff`.
    ///
    /// Additional arguments can be passed to Ruff after `--`.
    #[command(
        after_help = "Use `uv help format` for more details.",
        after_long_help = ""
    )]
    Format(FormatArgs),
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
            Self::Some(value) => Some(value),
            Self::None => None,
        }
    }

    pub fn is_some(&self) -> bool {
        matches!(self, Self::Some(_))
    }
}

/// Parse an `--index-url` argument into an [`PipIndex`], mapping the empty string to `None`.
fn parse_index_url(input: &str) -> Result<Maybe<PipIndex>, String> {
    if input.is_empty() {
        Ok(Maybe::None)
    } else {
        IndexUrl::from_str(input)
            .map(Index::from_index_url)
            .map(|index| Index {
                origin: Some(Origin::Cli),
                ..index
            })
            .map(PipIndex::from)
            .map(Maybe::Some)
            .map_err(|err| err.to_string())
    }
}

/// Parse an `--extra-index-url` argument into an [`PipExtraIndex`], mapping the empty string to `None`.
fn parse_extra_index_url(input: &str) -> Result<Maybe<PipExtraIndex>, String> {
    if input.is_empty() {
        Ok(Maybe::None)
    } else {
        IndexUrl::from_str(input)
            .map(Index::from_extra_index_url)
            .map(|index| Index {
                origin: Some(Origin::Cli),
                ..index
            })
            .map(PipExtraIndex::from)
            .map(Maybe::Some)
            .map_err(|err| err.to_string())
    }
}

/// Parse a `--find-links` argument into an [`PipFindLinks`], mapping the empty string to `None`.
fn parse_find_links(input: &str) -> Result<Maybe<PipFindLinks>, String> {
    if input.is_empty() {
        Ok(Maybe::None)
    } else {
        IndexUrl::from_str(input)
            .map(Index::from_find_links)
            .map(|index| Index {
                origin: Some(Origin::Cli),
                ..index
            })
            .map(PipFindLinks::from)
            .map(Maybe::Some)
            .map_err(|err| err.to_string())
    }
}

/// Parse an `--index` argument into a [`Vec<Index>`], mapping the empty string to an empty Vec.
///
/// This function splits the input on all whitespace characters rather than a single delimiter,
/// which is necessary to parse environment variables like `PIP_EXTRA_INDEX_URL`.
/// The standard `clap::Args` `value_delimiter` only supports single-character delimiters.
fn parse_indices(input: &str) -> Result<Vec<Maybe<Index>>, String> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut indices = Vec::new();
    for token in input.split_whitespace() {
        match Index::from_str(token) {
            Ok(index) => indices.push(Maybe::Some(Index {
                default: false,
                origin: Some(Origin::Cli),
                ..index
            })),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(indices)
}

/// Parse a `--default-index` argument into an [`Index`], mapping the empty string to `None`.
fn parse_default_index(input: &str) -> Result<Maybe<Index>, String> {
    if input.is_empty() {
        Ok(Maybe::None)
    } else {
        match Index::from_str(input) {
            Ok(index) => Ok(Maybe::Some(Index {
                default: true,
                origin: Some(Origin::Cli),
                ..index
            })),
            Err(err) => Err(err.to_string()),
        }
    }
}

/// Parse a string into an [`Url`], mapping the empty string to `None`.
fn parse_insecure_host(input: &str) -> Result<Maybe<TrustedHost>, String> {
    if input.is_empty() {
        Ok(Maybe::None)
    } else {
        match TrustedHost::from_str(input) {
            Ok(host) => Ok(Maybe::Some(host)),
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
        Ok(PathBuf::from(input))
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

// Parse a string, mapping the empty string to `None`.
#[expect(clippy::unnecessary_wraps)]
fn parse_maybe_string(input: &str) -> Result<Maybe<String>, String> {
    if input.is_empty() {
        Ok(Maybe::None)
    } else {
        Ok(Maybe::Some(input.to_string()))
    }
}

#[derive(Args)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
pub struct PipCompileArgs {
    /// Include the packages listed in the given files.
    ///
    /// The following formats are supported: `requirements.txt`, `.py` files with inline metadata,
    /// `pylock.toml`, `pyproject.toml`, `setup.py`, and `setup.cfg`.
    ///
    /// If a `pyproject.toml`, `setup.py`, or `setup.cfg` file is provided, uv will extract the
    /// requirements for the relevant project.
    ///
    /// If `-` is provided, then requirements will be read from stdin.
    ///
    /// The order of the requirements files and the requirements in them is used to determine
    /// priority during resolution.
    #[arg(group = "sources", value_parser = parse_file_path, value_hint = ValueHint::FilePath)]
    pub src_file: Vec<PathBuf>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[arg(
        long,
        short,
        alias = "constraint",
        env = EnvVars::UV_CONSTRAINT,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub constraints: Vec<Maybe<PathBuf>>,

    /// Override versions using the given requirements files.
    ///
    /// Overrides files are `requirements.txt`-like files that force a specific version of a
    /// requirement to be installed, regardless of the requirements declared by any constituent
    /// package, and regardless of whether this would be considered an invalid resolution.
    ///
    /// While constraints are _additive_, in that they're combined with the requirements of the
    /// constituent packages, overrides are _absolute_, in that they completely replace the
    /// requirements of the constituent packages.
    #[arg(
        long,
        alias = "override",
        env = EnvVars::UV_OVERRIDE,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub overrides: Vec<Maybe<PathBuf>>,

    /// Exclude packages from resolution using the given requirements files.
    ///
    /// Excludes files are `requirements.txt`-like files that specify packages to exclude
    /// from the resolution. When a package is excluded, it will be omitted from the
    /// dependency list entirely and its own dependencies will be ignored during the resolution
    /// phase. Excludes are unconditional in that requirement specifiers and markers are ignored;
    /// any package listed in the provided file will be omitted from all resolved environments.
    #[arg(
        long,
        alias = "exclude",
        env = EnvVars::UV_EXCLUDE,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub excludes: Vec<Maybe<PathBuf>>,

    /// Constrain build dependencies using the given requirements files when building source
    /// distributions.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    #[arg(
        long,
        short,
        alias = "build-constraint",
        env = EnvVars::UV_BUILD_CONSTRAINT,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub build_constraints: Vec<Maybe<PathBuf>>,

    /// Include optional dependencies from the specified extra name; may be provided more than once.
    ///
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, value_delimiter = ',', conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    ///
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "extra")]
    pub all_extras: bool,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub no_all_extras: bool,

    /// Install the specified dependency group from a `pyproject.toml`.
    ///
    /// If no path is provided, the `pyproject.toml` in the working directory is used.
    ///
    /// May be provided multiple times.
    #[arg(long, group = "sources")]
    pub group: Vec<PipGroupName>,

    #[command(flatten)]
    pub resolver: ResolverArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Ignore package dependencies, instead only add those packages explicitly listed
    /// on the command line to the resulting requirements file.
    #[arg(long)]
    pub no_deps: bool,

    #[arg(long, overrides_with("no_deps"), hide = true)]
    pub deps: bool,

    /// Write the compiled requirements to the given `requirements.txt` or `pylock.toml` file.
    ///
    /// If the file already exists, the existing versions will be preferred when resolving
    /// dependencies, unless `--upgrade` is also specified.
    #[arg(long, short, value_hint = ValueHint::FilePath)]
    pub output_file: Option<PathBuf>,

    /// The format in which the resolution should be output.
    ///
    /// Supports both `requirements.txt` and `pylock.toml` (PEP 751) output formats.
    ///
    /// uv will infer the output format from the file extension of the output file, if
    /// provided. Otherwise, defaults to `requirements.txt`.
    #[arg(long, value_enum)]
    pub format: Option<PipCompileFormat>,

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
    #[arg(long, env = EnvVars::UV_CUSTOM_COMPILE_COMMAND, value_hint = ValueHint::Other)]
    pub custom_compile_command: Option<String>,

    /// The Python interpreter to use during resolution.
    ///
    /// A Python interpreter is required for building source distributions to determine package
    /// metadata when there are not wheels.
    ///
    /// The interpreter is also used to determine the default minimum Python version, unless
    /// `--python-version` is provided.
    ///
    /// This option respects `UV_PYTHON`, but when set via environment variable, it is overridden
    /// by `--python-version`.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// Install packages into the system Python environment.
    ///
    /// By default, uv uses the virtual environment in the current working directory or any parent
    /// directory, falling back to searching for a Python executable in `PATH`. The `--system`
    /// option instructs uv to avoid using a virtual environment Python and restrict its search to
    /// the system path.
    #[arg(
        long,
        env = EnvVars::UV_SYSTEM_PYTHON,
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
    #[arg(long, value_delimiter = ',', conflicts_with = "no_build")]
    pub no_binary: Option<Vec<PackageNameSpecifier>>,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[arg(long, value_delimiter = ',', conflicts_with = "no_build")]
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
    #[arg(long, help_heading = "Python options")]
    pub python_version: Option<PythonVersion>,

    /// The platform for which requirements should be resolved.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aarch64-apple-darwin`.
    ///
    /// When targeting macOS (Darwin), the default minimum version is `13.0`. Use
    /// `MACOSX_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting iOS, the default minimum version is `13.0`. Use
    /// `IPHONEOS_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting Android, the default minimum Android API level is `24`. Use
    /// `ANDROID_API_LEVEL` to specify a different minimum version, e.g., `26`.
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
    #[arg(long, alias = "unsafe-package", value_hint = ValueHint::Other)]
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

    /// The backend to use when fetching packages in the PyTorch ecosystem (e.g., `cpu`, `cu126`, or `auto`).
    ///
    /// When set, uv will ignore the configured index URLs for packages in the PyTorch ecosystem,
    /// and will instead use the defined backend.
    ///
    /// For example, when set to `cpu`, uv will use the CPU-only PyTorch index; when set to `cu126`,
    /// uv will use the PyTorch index for CUDA 12.6.
    ///
    /// The `auto` mode will attempt to detect the appropriate PyTorch index based on the currently
    /// installed CUDA drivers.
    ///
    /// This option is in preview and may change in any future release.
    #[arg(long, value_enum, env = EnvVars::UV_TORCH_BACKEND)]
    pub torch_backend: Option<TorchMode>,

    #[command(flatten)]
    pub compat_args: compat::PipCompileCompatArgs,
}

#[derive(Args)]
pub struct PipSyncArgs {
    /// Include the packages listed in the given files.
    ///
    /// The following formats are supported: `requirements.txt`, `.py` files with inline metadata,
    /// `pylock.toml`, `pyproject.toml`, `setup.py`, and `setup.cfg`.
    ///
    /// If a `pyproject.toml`, `setup.py`, or `setup.cfg` file is provided, uv will
    /// extract the requirements for the relevant project.
    ///
    /// If `-` is provided, then requirements will be read from stdin.
    #[arg(required(true), value_parser = parse_file_path, value_hint = ValueHint::FilePath)]
    pub src_file: Vec<PathBuf>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[arg(
        long,
        short,
        alias = "constraint",
        env = EnvVars::UV_CONSTRAINT,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub constraints: Vec<Maybe<PathBuf>>,

    /// Constrain build dependencies using the given requirements files when building source
    /// distributions.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    #[arg(
        long,
        short,
        alias = "build-constraint",
        env = EnvVars::UV_BUILD_CONSTRAINT,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub build_constraints: Vec<Maybe<PathBuf>>,

    /// Include optional dependencies from the specified extra name; may be provided more than once.
    ///
    /// Only applies to `pylock.toml`, `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, value_delimiter = ',', conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    ///
    /// Only applies to `pylock.toml`, `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "extra", overrides_with = "no_all_extras")]
    pub all_extras: bool,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub no_all_extras: bool,

    /// Install the specified dependency group from a `pylock.toml` or `pyproject.toml`.
    ///
    /// If no path is provided, the `pylock.toml` or `pyproject.toml` in the working directory is
    /// used.
    ///
    /// May be provided multiple times.
    #[arg(long, group = "sources")]
    pub group: Vec<PipGroupName>,

    #[command(flatten)]
    pub installer: InstallerArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Require a matching hash for each requirement.
    ///
    /// By default, uv will verify any available hashes in the requirements file, but will not
    /// require that all requirements have an associated hash.
    ///
    /// When `--require-hashes` is enabled, _all_ requirements must include a hash or set of hashes,
    /// and _all_ requirements must either be pinned to exact versions (e.g., `==1.0.0`), or be
    /// specified via direct URL.
    ///
    /// Hash-checking mode introduces a number of additional constraints:
    ///
    /// - Git dependencies are not supported.
    /// - Editable installations are not supported.
    /// - Local dependencies are not supported, unless they point to a specific wheel (`.whl`) or
    ///   source archive (`.zip`, `.tar.gz`), as opposed to a directory.
    #[arg(
        long,
        env = EnvVars::UV_REQUIRE_HASHES,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_require_hashes"),
    )]
    pub require_hashes: bool,

    #[arg(long, overrides_with("require_hashes"), hide = true)]
    pub no_require_hashes: bool,

    #[arg(long, overrides_with("no_verify_hashes"), hide = true)]
    pub verify_hashes: bool,

    /// Disable validation of hashes in the requirements file.
    ///
    /// By default, uv will verify any available hashes in the requirements file, but will not
    /// require that all requirements have an associated hash. To enforce hash validation, use
    /// `--require-hashes`.
    #[arg(
        long,
        env = EnvVars::UV_NO_VERIFY_HASHES,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("verify_hashes"),
    )]
    pub no_verify_hashes: bool,

    /// The Python interpreter into which packages should be installed.
    ///
    /// By default, syncing requires a virtual environment. A path to an alternative Python can be
    /// provided, but it is only recommended in continuous integration (CI) environments and should
    /// be used with caution, as it can modify the system Python installation.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// Install packages into the system Python environment.
    ///
    /// By default, uv installs into the virtual environment in the current working directory or any
    /// parent directory. The `--system` option instructs uv to instead use the first Python found
    /// in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[arg(
        long,
        env = EnvVars::UV_SYSTEM_PYTHON,
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
        env = EnvVars::UV_BREAK_SYSTEM_PACKAGES,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_break_system_packages")
    )]
    pub break_system_packages: bool,

    #[arg(long, overrides_with("break_system_packages"))]
    pub no_break_system_packages: bool,

    /// Install packages into the specified directory, rather than into the virtual or system Python
    /// environment. The packages will be installed at the top-level of the directory.
    ///
    /// Unlike other install operations, this command does not require discovery of an existing Python
    /// environment and only searches for a Python interpreter to use for package resolution.
    /// If a suitable Python interpreter cannot be found, uv will install one.
    /// To disable this, add `--no-python-downloads`.
    #[arg(short = 't', long, conflicts_with = "prefix", value_hint = ValueHint::DirPath)]
    pub target: Option<PathBuf>,

    /// Install packages into `lib`, `bin`, and other top-level folders under the specified
    /// directory, as if a virtual environment were present at that location.
    ///
    /// In general, prefer the use of `--python` to install into an alternate environment, as
    /// scripts and other artifacts installed via `--prefix` will reference the installing
    /// interpreter, rather than any interpreter added to the `--prefix` directory, rendering them
    /// non-portable.
    ///
    /// Unlike other install operations, this command does not require discovery of an existing Python
    /// environment and only searches for a Python interpreter to use for package resolution.
    /// If a suitable Python interpreter cannot be found, uv will install one.
    /// To disable this, add `--no-python-downloads`.
    #[arg(long, conflicts_with = "target", value_hint = ValueHint::DirPath)]
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
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`. Clear
    /// previously specified packages with `:none:`.
    #[arg(long, value_delimiter = ',', conflicts_with = "no_build")]
    pub no_binary: Option<Vec<PackageNameSpecifier>>,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of
    /// already-built source distributions will be reused, but operations that require building
    /// distributions will exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`. Clear
    /// previously specified packages with `:none:`.
    #[arg(long, value_delimiter = ',', conflicts_with = "no_build")]
    pub only_binary: Option<Vec<PackageNameSpecifier>>,

    /// Allow sync of empty requirements, which will clear the environment of all packages.
    #[arg(long, overrides_with("no_allow_empty_requirements"))]
    pub allow_empty_requirements: bool,

    #[arg(long, overrides_with("allow_empty_requirements"))]
    pub no_allow_empty_requirements: bool,

    /// The minimum Python version that should be supported by the requirements (e.g., `3.7` or
    /// `3.7.9`).
    ///
    /// If a patch version is omitted, the minimum patch version is assumed. For example, `3.7` is
    /// mapped to `3.7.0`.
    #[arg(long)]
    pub python_version: Option<PythonVersion>,

    /// The platform for which requirements should be installed.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aarch64-apple-darwin`.
    ///
    /// When targeting macOS (Darwin), the default minimum version is `13.0`. Use
    /// `MACOSX_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting iOS, the default minimum version is `13.0`. Use
    /// `IPHONEOS_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting Android, the default minimum Android API level is `24`. Use
    /// `ANDROID_API_LEVEL` to specify a different minimum version, e.g., `26`.
    ///
    /// WARNING: When specified, uv will select wheels that are compatible with the _target_
    /// platform; as a result, the installed distributions may not be compatible with the _current_
    /// platform. Conversely, any distributions that are built from source may be incompatible with
    /// the _target_ platform, as they will be built for the _current_ platform. The
    /// `--python-platform` option is intended for advanced use cases.
    #[arg(long)]
    pub python_platform: Option<TargetTriple>,

    /// Validate the Python environment after completing the installation, to detect packages with
    /// missing dependencies or other issues.
    #[arg(long, overrides_with("no_strict"))]
    pub strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub no_strict: bool,

    /// Perform a dry run, i.e., don't actually install anything but resolve the dependencies and
    /// print the resulting plan.
    #[arg(long)]
    pub dry_run: bool,

    /// The backend to use when fetching packages in the PyTorch ecosystem (e.g., `cpu`, `cu126`, or `auto`).
    ///
    /// When set, uv will ignore the configured index URLs for packages in the PyTorch ecosystem,
    /// and will instead use the defined backend.
    ///
    /// For example, when set to `cpu`, uv will use the CPU-only PyTorch index; when set to `cu126`,
    /// uv will use the PyTorch index for CUDA 12.6.
    ///
    /// The `auto` mode will attempt to detect the appropriate PyTorch index based on the currently
    /// installed CUDA drivers.
    ///
    /// This option is in preview and may change in any future release.
    #[arg(long, value_enum, env = EnvVars::UV_TORCH_BACKEND)]
    pub torch_backend: Option<TorchMode>,

    #[command(flatten)]
    pub compat_args: compat::PipSyncCompatArgs,
}

#[derive(Args)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
pub struct PipInstallArgs {
    /// Install all listed packages.
    ///
    /// The order of the packages is used to determine priority during resolution.
    #[arg(group = "sources", value_hint = ValueHint::Other)]
    pub package: Vec<String>,

    /// Install the packages listed in the given files.
    ///
    /// The following formats are supported: `requirements.txt`, `.py` files with inline metadata,
    /// `pylock.toml`, `pyproject.toml`, `setup.py`, and `setup.cfg`.
    ///
    /// If a `pyproject.toml`, `setup.py`, or `setup.cfg` file is provided, uv will extract the
    /// requirements for the relevant project.
    ///
    /// If `-` is provided, then requirements will be read from stdin.
    #[arg(
        long,
        short,
        alias = "requirement",
        group = "sources",
        value_parser = parse_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub requirements: Vec<PathBuf>,

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
    #[arg(
        long,
        short,
        alias = "constraint",
        env = EnvVars::UV_CONSTRAINT,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub constraints: Vec<Maybe<PathBuf>>,

    /// Override versions using the given requirements files.
    ///
    /// Overrides files are `requirements.txt`-like files that force a specific version of a
    /// requirement to be installed, regardless of the requirements declared by any constituent
    /// package, and regardless of whether this would be considered an invalid resolution.
    ///
    /// While constraints are _additive_, in that they're combined with the requirements of the
    /// constituent packages, overrides are _absolute_, in that they completely replace the
    /// requirements of the constituent packages.
    #[arg(
        long,
        alias = "override",
        env = EnvVars::UV_OVERRIDE,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub overrides: Vec<Maybe<PathBuf>>,

    /// Exclude packages from resolution using the given requirements files.
    ///
    /// Excludes files are `requirements.txt`-like files that specify packages to exclude
    /// from the resolution. When a package is excluded, it will be omitted from the
    /// dependency list entirely and its own dependencies will be ignored during the resolution
    /// phase. Excludes are unconditional in that requirement specifiers and markers are ignored;
    /// any package listed in the provided file will be omitted from all resolved environments.
    #[arg(
        long,
        alias = "exclude",
        env = EnvVars::UV_EXCLUDE,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub excludes: Vec<Maybe<PathBuf>>,

    /// Constrain build dependencies using the given requirements files when building source
    /// distributions.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    #[arg(
        long,
        short,
        alias = "build-constraint",
        env = EnvVars::UV_BUILD_CONSTRAINT,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub build_constraints: Vec<Maybe<PathBuf>>,

    /// Include optional dependencies from the specified extra name; may be provided more than once.
    ///
    /// Only applies to `pylock.toml`, `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, value_delimiter = ',', conflicts_with = "all_extras", value_parser = extra_name_with_clap_error)]
    pub extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    ///
    /// Only applies to `pylock.toml`, `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[arg(long, conflicts_with = "extra", overrides_with = "no_all_extras")]
    pub all_extras: bool,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub no_all_extras: bool,

    /// Install the specified dependency group from a `pylock.toml` or `pyproject.toml`.
    ///
    /// If no path is provided, the `pylock.toml` or `pyproject.toml` in the working directory is
    /// used.
    ///
    /// May be provided multiple times.
    #[arg(long, group = "sources")]
    pub group: Vec<PipGroupName>,

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
    /// By default, uv will verify any available hashes in the requirements file, but will not
    /// require that all requirements have an associated hash.
    ///
    /// When `--require-hashes` is enabled, _all_ requirements must include a hash or set of hashes,
    /// and _all_ requirements must either be pinned to exact versions (e.g., `==1.0.0`), or be
    /// specified via direct URL.
    ///
    /// Hash-checking mode introduces a number of additional constraints:
    ///
    /// - Git dependencies are not supported.
    /// - Editable installations are not supported.
    /// - Local dependencies are not supported, unless they point to a specific wheel (`.whl`) or
    ///   source archive (`.zip`, `.tar.gz`), as opposed to a directory.
    #[arg(
        long,
        env = EnvVars::UV_REQUIRE_HASHES,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_require_hashes"),
    )]
    pub require_hashes: bool,

    #[arg(long, overrides_with("require_hashes"), hide = true)]
    pub no_require_hashes: bool,

    #[arg(long, overrides_with("no_verify_hashes"), hide = true)]
    pub verify_hashes: bool,

    /// Disable validation of hashes in the requirements file.
    ///
    /// By default, uv will verify any available hashes in the requirements file, but will not
    /// require that all requirements have an associated hash. To enforce hash validation, use
    /// `--require-hashes`.
    #[arg(
        long,
        env = EnvVars::UV_NO_VERIFY_HASHES,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("verify_hashes"),
    )]
    pub no_verify_hashes: bool,

    /// The Python interpreter into which packages should be installed.
    ///
    /// By default, installation requires a virtual environment. A path to an alternative Python can
    /// be provided, but it is only recommended in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// Install packages into the system Python environment.
    ///
    /// By default, uv installs into the virtual environment in the current working directory or any
    /// parent directory. The `--system` option instructs uv to instead use the first Python found
    /// in the system `PATH`.
    ///
    /// WARNING: `--system` is intended for use in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    #[arg(
        long,
        env = EnvVars::UV_SYSTEM_PYTHON,
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
        env = EnvVars::UV_BREAK_SYSTEM_PACKAGES,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_break_system_packages")
    )]
    pub break_system_packages: bool,

    #[arg(long, overrides_with("break_system_packages"))]
    pub no_break_system_packages: bool,

    /// Install packages into the specified directory, rather than into the virtual or system Python
    /// environment. The packages will be installed at the top-level of the directory.
    ///
    /// Unlike other install operations, this command does not require discovery of an existing Python
    /// environment and only searches for a Python interpreter to use for package resolution.
    /// If a suitable Python interpreter cannot be found, uv will install one.
    /// To disable this, add `--no-python-downloads`.
    #[arg(short = 't', long, conflicts_with = "prefix", value_hint = ValueHint::DirPath)]
    pub target: Option<PathBuf>,

    /// Install packages into `lib`, `bin`, and other top-level folders under the specified
    /// directory, as if a virtual environment were present at that location.
    ///
    /// In general, prefer the use of `--python` to install into an alternate environment, as
    /// scripts and other artifacts installed via `--prefix` will reference the installing
    /// interpreter, rather than any interpreter added to the `--prefix` directory, rendering them
    /// non-portable.
    ///
    /// Unlike other install operations, this command does not require discovery of an existing Python
    /// environment and only searches for a Python interpreter to use for package resolution.
    /// If a suitable Python interpreter cannot be found, uv will install one.
    /// To disable this, add `--no-python-downloads`.
    #[arg(long, conflicts_with = "target", value_hint = ValueHint::DirPath)]
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
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`. Clear
    /// previously specified packages with `:none:`.
    #[arg(long, value_delimiter = ',', conflicts_with = "no_build")]
    pub no_binary: Option<Vec<PackageNameSpecifier>>,

    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of
    /// already-built source distributions will be reused, but operations that require building
    /// distributions will exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`. Clear
    /// previously specified packages with `:none:`.
    #[arg(long, value_delimiter = ',', conflicts_with = "no_build")]
    pub only_binary: Option<Vec<PackageNameSpecifier>>,

    /// The minimum Python version that should be supported by the requirements (e.g., `3.7` or
    /// `3.7.9`).
    ///
    /// If a patch version is omitted, the minimum patch version is assumed. For example, `3.7` is
    /// mapped to `3.7.0`.
    #[arg(long)]
    pub python_version: Option<PythonVersion>,

    /// The platform for which requirements should be installed.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aarch64-apple-darwin`.
    ///
    /// When targeting macOS (Darwin), the default minimum version is `13.0`. Use
    /// `MACOSX_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting iOS, the default minimum version is `13.0`. Use
    /// `IPHONEOS_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting Android, the default minimum Android API level is `24`. Use
    /// `ANDROID_API_LEVEL` to specify a different minimum version, e.g., `26`.
    ///
    /// WARNING: When specified, uv will select wheels that are compatible with the _target_
    /// platform; as a result, the installed distributions may not be compatible with the _current_
    /// platform. Conversely, any distributions that are built from source may be incompatible with
    /// the _target_ platform, as they will be built for the _current_ platform. The
    /// `--python-platform` option is intended for advanced use cases.
    #[arg(long)]
    pub python_platform: Option<TargetTriple>,

    /// Do not remove extraneous packages present in the environment.
    #[arg(long, overrides_with("exact"), alias = "no-exact", hide = true)]
    pub inexact: bool,

    /// Perform an exact sync, removing extraneous packages.
    ///
    /// By default, installing will make the minimum necessary changes to satisfy the requirements.
    /// When enabled, uv will update the environment to exactly match the requirements, removing
    /// packages that are not included in the requirements.
    #[arg(long, overrides_with("inexact"))]
    pub exact: bool,

    /// Validate the Python environment after completing the installation, to detect packages with
    /// missing dependencies or other issues.
    #[arg(long, overrides_with("no_strict"))]
    pub strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub no_strict: bool,

    /// Perform a dry run, i.e., don't actually install anything but resolve the dependencies and
    /// print the resulting plan.
    #[arg(long)]
    pub dry_run: bool,

    /// The backend to use when fetching packages in the PyTorch ecosystem (e.g., `cpu`, `cu126`, or `auto`)
    ///
    /// When set, uv will ignore the configured index URLs for packages in the PyTorch ecosystem,
    /// and will instead use the defined backend.
    ///
    /// For example, when set to `cpu`, uv will use the CPU-only PyTorch index; when set to `cu126`,
    /// uv will use the PyTorch index for CUDA 12.6.
    ///
    /// The `auto` mode will attempt to detect the appropriate PyTorch index based on the currently
    /// installed CUDA drivers.
    ///
    /// This option is in preview and may change in any future release.
    #[arg(long, value_enum, env = EnvVars::UV_TORCH_BACKEND)]
    pub torch_backend: Option<TorchMode>,

    #[command(flatten)]
    pub compat_args: compat::PipInstallCompatArgs,
}

#[derive(Args)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
pub struct PipUninstallArgs {
    /// Uninstall all listed packages.
    #[arg(group = "sources", value_hint = ValueHint::Other)]
    pub package: Vec<String>,

    /// Uninstall the packages listed in the given files.
    ///
    /// The following formats are supported: `requirements.txt`, `.py` files with inline metadata,
    /// `pylock.toml`, `pyproject.toml`, `setup.py`, and `setup.cfg`.
    #[arg(long, short, alias = "requirement", group = "sources", value_parser = parse_file_path, value_hint = ValueHint::FilePath)]
    pub requirements: Vec<PathBuf>,

    /// The Python interpreter from which packages should be uninstalled.
    ///
    /// By default, uninstallation requires a virtual environment. A path to an alternative Python
    /// can be provided, but it is only recommended in continuous integration (CI) environments and
    /// should be used with caution, as it can modify the system Python installation.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// Attempt to use `keyring` for authentication for remote requirements files.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to use
    /// the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(long, value_enum, env = EnvVars::UV_KEYRING_PROVIDER)]
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
        env = EnvVars::UV_SYSTEM_PYTHON,
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
        env = EnvVars::UV_BREAK_SYSTEM_PACKAGES,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_break_system_packages")
    )]
    pub break_system_packages: bool,

    #[arg(long, overrides_with("break_system_packages"))]
    pub no_break_system_packages: bool,

    /// Uninstall packages from the specified `--target` directory.
    #[arg(short = 't', long, conflicts_with = "prefix", value_hint = ValueHint::DirPath)]
    pub target: Option<PathBuf>,

    /// Uninstall packages from the specified `--prefix` directory.
    #[arg(long, conflicts_with = "target", value_hint = ValueHint::DirPath)]
    pub prefix: Option<PathBuf>,

    /// Perform a dry run, i.e., don't actually uninstall anything but print the resulting plan.
    #[arg(long)]
    pub dry_run: bool,

    #[command(flatten)]
    pub compat_args: compat::PipGlobalCompatArgs,
}

#[derive(Args)]
pub struct PipFreezeArgs {
    /// Exclude any editable packages from output.
    #[arg(long)]
    pub exclude_editable: bool,

    /// Exclude the specified package(s) from the output.
    #[arg(long)]
    pub r#exclude: Vec<PackageName>,

    /// Validate the Python environment, to detect packages with missing dependencies and other
    /// issues.
    #[arg(long, overrides_with("no_strict"))]
    pub strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub no_strict: bool,

    /// The Python interpreter for which packages should be listed.
    ///
    /// By default, uv lists packages in a virtual environment but will show packages in a system
    /// Python environment if no virtual environment is found.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// Restrict to the specified installation path for listing packages (can be used multiple times).
    #[arg(long("path"), value_parser = parse_file_path, value_hint = ValueHint::DirPath)]
    pub paths: Option<Vec<PathBuf>>,

    /// List packages in the system Python environment.
    ///
    /// Disables discovery of virtual environments.
    ///
    /// See `uv help python` for details on Python discovery.
    #[arg(
        long,
        env = EnvVars::UV_SYSTEM_PYTHON,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    /// List packages from the specified `--target` directory.
    #[arg(short = 't', long, conflicts_with_all = ["prefix", "paths"], value_hint = ValueHint::DirPath)]
    pub target: Option<PathBuf>,

    /// List packages from the specified `--prefix` directory.
    #[arg(long, conflicts_with_all = ["target", "paths"], value_hint = ValueHint::DirPath)]
    pub prefix: Option<PathBuf>,

    #[command(flatten)]
    pub compat_args: compat::PipGlobalCompatArgs,
}

#[derive(Args)]
pub struct PipListArgs {
    /// Only include editable projects.
    #[arg(short, long)]
    pub editable: bool,

    /// Exclude any editable packages from output.
    #[arg(long, conflicts_with = "editable")]
    pub exclude_editable: bool,

    /// Exclude the specified package(s) from the output.
    #[arg(long, value_hint = ValueHint::Other)]
    pub r#exclude: Vec<PackageName>,

    /// Select the output format.
    #[arg(long, value_enum, default_value_t = ListFormat::default())]
    pub format: ListFormat,

    /// List outdated packages.
    ///
    /// The latest version of each package will be shown alongside the installed version. Up-to-date
    /// packages will be omitted from the output.
    #[arg(long, overrides_with("no_outdated"))]
    pub outdated: bool,

    #[arg(long, overrides_with("outdated"), hide = true)]
    pub no_outdated: bool,

    /// Validate the Python environment, to detect packages with missing dependencies and other
    /// issues.
    #[arg(long, overrides_with("no_strict"))]
    pub strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub no_strict: bool,

    #[command(flatten)]
    pub fetch: FetchArgs,

    /// The Python interpreter for which packages should be listed.
    ///
    /// By default, uv lists packages in a virtual environment but will show packages in a system
    /// Python environment if no virtual environment is found.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// List packages in the system Python environment.
    ///
    /// Disables discovery of virtual environments.
    ///
    /// See `uv help python` for details on Python discovery.
    #[arg(
        long,
        env = EnvVars::UV_SYSTEM_PYTHON,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    /// List packages from the specified `--target` directory.
    #[arg(short = 't', long, conflicts_with = "prefix", value_hint = ValueHint::DirPath)]
    pub target: Option<PathBuf>,

    /// List packages from the specified `--prefix` directory.
    #[arg(long, conflicts_with = "target", value_hint = ValueHint::DirPath)]
    pub prefix: Option<PathBuf>,

    #[command(flatten)]
    pub compat_args: compat::PipListCompatArgs,
}

#[derive(Args)]
pub struct PipCheckArgs {
    /// The Python interpreter for which packages should be checked.
    ///
    /// By default, uv checks packages in a virtual environment but will check packages in a system
    /// Python environment if no virtual environment is found.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// Check packages in the system Python environment.
    ///
    /// Disables discovery of virtual environments.
    ///
    /// See `uv help python` for details on Python discovery.
    #[arg(
        long,
        env = EnvVars::UV_SYSTEM_PYTHON,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    /// The Python version against which packages should be checked.
    ///
    /// By default, the installed packages are checked against the version of the current
    /// interpreter.
    #[arg(long)]
    pub python_version: Option<PythonVersion>,

    /// The platform for which packages should be checked.
    ///
    /// By default, the installed packages are checked against the platform of the current
    /// interpreter.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aarch64-apple-darwin`.
    ///
    /// When targeting macOS (Darwin), the default minimum version is `13.0`. Use
    /// `MACOSX_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting iOS, the default minimum version is `13.0`. Use
    /// `IPHONEOS_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting Android, the default minimum Android API level is `24`. Use
    /// `ANDROID_API_LEVEL` to specify a different minimum version, e.g., `26`.
    #[arg(long)]
    pub python_platform: Option<TargetTriple>,
}

#[derive(Args)]
pub struct PipShowArgs {
    /// The package(s) to display.
    #[arg(value_hint = ValueHint::Other)]
    pub package: Vec<PackageName>,

    /// Validate the Python environment, to detect packages with missing dependencies and other
    /// issues.
    #[arg(long, overrides_with("no_strict"))]
    pub strict: bool,

    #[arg(long, overrides_with("strict"), hide = true)]
    pub no_strict: bool,

    /// Show the full list of installed files for each package.
    #[arg(short, long)]
    pub files: bool,

    /// The Python interpreter to find the package in.
    ///
    /// By default, uv looks for packages in a virtual environment but will look for packages in a
    /// system Python environment if no virtual environment is found.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// Show a package in the system Python environment.
    ///
    /// Disables discovery of virtual environments.
    ///
    /// See `uv help python` for details on Python discovery.
    #[arg(
        long,
        env = EnvVars::UV_SYSTEM_PYTHON,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    /// Show a package from the specified `--target` directory.
    #[arg(short = 't', long, conflicts_with = "prefix", value_hint = ValueHint::DirPath)]
    pub target: Option<PathBuf>,

    /// Show a package from the specified `--prefix` directory.
    #[arg(long, conflicts_with = "target", value_hint = ValueHint::DirPath)]
    pub prefix: Option<PathBuf>,

    #[command(flatten)]
    pub compat_args: compat::PipGlobalCompatArgs,
}

#[derive(Args)]
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

    #[command(flatten)]
    pub fetch: FetchArgs,

    /// The Python interpreter for which packages should be listed.
    ///
    /// By default, uv lists packages in a virtual environment but will show packages in a system
    /// Python environment if no virtual environment is found.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// List packages in the system Python environment.
    ///
    /// Disables discovery of virtual environments.
    ///
    /// See `uv help python` for details on Python discovery.
    #[arg(
        long,
        env = EnvVars::UV_SYSTEM_PYTHON,
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
pub struct PipDebugArgs {
    #[arg(long, hide = true)]
    pub platform: Option<String>,

    #[arg(long, hide = true)]
    pub python_version: Option<String>,

    #[arg(long, hide = true)]
    pub implementation: Option<String>,

    #[arg(long, hide = true)]
    pub abi: Option<String>,
}

#[derive(Args)]
pub struct BuildArgs {
    /// The directory from which distributions should be built, or a source
    /// distribution archive to build into a wheel.
    ///
    /// Defaults to the current working directory.
    #[arg(value_parser = parse_file_path, value_hint = ValueHint::DirPath)]
    pub src: Option<PathBuf>,

    /// Build a specific package in the workspace.
    ///
    /// The workspace will be discovered from the provided source directory, or the current
    /// directory if no source directory is provided.
    ///
    /// If the workspace member does not exist, uv will exit with an error.
    #[arg(long, conflicts_with("all_packages"), value_hint = ValueHint::Other)]
    pub package: Option<PackageName>,

    /// Builds all packages in the workspace.
    ///
    /// The workspace will be discovered from the provided source directory, or the current
    /// directory if no source directory is provided.
    ///
    /// If the workspace member does not exist, uv will exit with an error.
    #[arg(long, alias = "all", conflicts_with("package"))]
    pub all_packages: bool,

    /// The output directory to which distributions should be written.
    ///
    /// Defaults to the `dist` subdirectory within the source directory, or the
    /// directory containing the source distribution archive.
    #[arg(long, short, value_parser = parse_file_path, value_hint = ValueHint::DirPath)]
    pub out_dir: Option<PathBuf>,

    /// Build a source distribution ("sdist") from the given directory.
    #[arg(long)]
    pub sdist: bool,

    /// Build a binary distribution ("wheel") from the given directory.
    #[arg(long)]
    pub wheel: bool,

    /// When using the uv build backend, list the files that would be included when building.
    ///
    /// Skips building the actual distribution, except when the source distribution is needed to
    /// build the wheel. The file list is collected directly without a PEP 517 environment. It only
    /// works with the uv build backend, there is no PEP 517 file list build hook.
    ///
    /// This option can be combined with `--sdist` and `--wheel` for inspecting different build
    /// paths.
    // Hidden while in preview.
    #[arg(long, hide = true)]
    pub list: bool,

    #[arg(long, overrides_with("no_build_logs"), hide = true)]
    pub build_logs: bool,

    /// Hide logs from the build backend.
    #[arg(long, overrides_with("build_logs"))]
    pub no_build_logs: bool,

    /// Always build through PEP 517, don't use the fast path for the uv build backend.
    ///
    /// By default, uv won't create a PEP 517 build environment for packages using the uv build
    /// backend, but use a fast path that calls into the build backend directly. This option forces
    /// always using PEP 517.
    #[arg(long, conflicts_with = "list")]
    pub force_pep517: bool,

    /// Clear the output directory before the build, removing stale artifacts.
    #[arg(long)]
    pub clear: bool,

    #[arg(long, overrides_with("no_create_gitignore"), hide = true)]
    pub create_gitignore: bool,

    /// Do not create a `.gitignore` file in the output directory.
    ///
    /// By default, uv creates a `.gitignore` file in the output directory to exclude build
    /// artifacts from version control. When this flag is used, the file will be omitted.
    #[arg(long, overrides_with("create_gitignore"))]
    pub no_create_gitignore: bool,

    /// Constrain build dependencies using the given requirements files when building distributions.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// build dependency that's installed. However, including a package in a constraints file will
    /// _not_ trigger the inclusion of that package on its own.
    #[arg(
        long,
        short,
        alias = "build-constraint",
        env = EnvVars::UV_BUILD_CONSTRAINT,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub build_constraints: Vec<Maybe<PathBuf>>,

    /// Require a matching hash for each requirement.
    ///
    /// By default, uv will verify any available hashes in the requirements file, but will not
    /// require that all requirements have an associated hash.
    ///
    /// When `--require-hashes` is enabled, _all_ requirements must include a hash or set of hashes,
    /// and _all_ requirements must either be pinned to exact versions (e.g., `==1.0.0`), or be
    /// specified via direct URL.
    ///
    /// Hash-checking mode introduces a number of additional constraints:
    ///
    /// - Git dependencies are not supported.
    /// - Editable installations are not supported.
    /// - Local dependencies are not supported, unless they point to a specific wheel (`.whl`) or
    ///   source archive (`.zip`, `.tar.gz`), as opposed to a directory.
    #[arg(
        long,
        env = EnvVars::UV_REQUIRE_HASHES,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_require_hashes"),
    )]
    pub require_hashes: bool,

    #[arg(long, overrides_with("require_hashes"), hide = true)]
    pub no_require_hashes: bool,

    #[arg(long, overrides_with("no_verify_hashes"), hide = true)]
    pub verify_hashes: bool,

    /// Disable validation of hashes in the requirements file.
    ///
    /// By default, uv will verify any available hashes in the requirements file, but will not
    /// require that all requirements have an associated hash. To enforce hash validation, use
    /// `--require-hashes`.
    #[arg(
        long,
        env = EnvVars::UV_NO_VERIFY_HASHES,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("verify_hashes"),
    )]
    pub no_verify_hashes: bool,

    /// The Python interpreter to use for the build environment.
    ///
    /// By default, builds are executed in isolated virtual environments. The discovered interpreter
    /// will be used to create those environments, and will be symlinked or copied in depending on
    /// the platform.
    ///
    /// See `uv help python` to view supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    #[command(flatten)]
    pub resolver: ResolverArgs,

    #[command(flatten)]
    pub build: BuildOptionsArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,
}

#[derive(Args)]
pub struct VenvArgs {
    /// The Python interpreter to use for the virtual environment.
    ///
    /// During virtual environment creation, uv will not look for Python interpreters in virtual
    /// environments.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// Ignore virtual environments when searching for the Python interpreter.
    ///
    /// This is the default behavior and has no effect.
    #[arg(
        long,
        env = EnvVars::UV_SYSTEM_PYTHON,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system"),
        hide = true,
    )]
    pub system: bool,

    /// This flag is included for compatibility only, it has no effect.
    ///
    /// uv will never search for interpreters in virtual environments when creating a virtual
    /// environment.
    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    /// Avoid discovering a project or workspace.
    ///
    /// By default, uv searches for projects in the current directory or any parent directory to
    /// determine the default path of the virtual environment and check for Python version
    /// constraints, if any.
    #[arg(long, alias = "no-workspace")]
    pub no_project: bool,

    /// Install seed packages (one or more of: `pip`, `setuptools`, and `wheel`) into the virtual
    /// environment [env: UV_VENV_SEED=]
    ///
    /// Note that `setuptools` and `wheel` are not included in Python 3.12+ environments.
    #[arg(long, value_parser = clap::builder::BoolishValueParser::new())]
    pub seed: bool,

    /// Remove any existing files or directories at the target path [env: UV_VENV_CLEAR=]
    ///
    /// By default, `uv venv` will exit with an error if the given path is non-empty. The
    /// `--clear` option will instead clear a non-empty path before creating a new virtual
    /// environment.
    #[clap(long, short, overrides_with = "allow_existing", value_parser = clap::builder::BoolishValueParser::new())]
    pub clear: bool,

    /// Fail without prompting if any existing files or directories are present at the target path.
    ///
    /// By default, when a TTY is available, `uv venv` will prompt to clear a non-empty directory.
    /// When `--no-clear` is used, the command will exit with an error instead of prompting.
    #[clap(
        long,
        overrides_with = "clear",
        conflicts_with = "allow_existing",
        hide = true
    )]
    pub no_clear: bool,

    /// Preserve any existing files or directories at the target path.
    ///
    /// By default, `uv venv` will exit with an error if the given path is non-empty. The
    /// `--allow-existing` option will instead write to the given path, regardless of its contents,
    /// and without clearing it beforehand.
    ///
    /// WARNING: This option can lead to unexpected behavior if the existing virtual environment and
    /// the newly-created virtual environment are linked to different Python interpreters.
    #[clap(long, overrides_with = "clear")]
    pub allow_existing: bool,

    /// The path to the virtual environment to create.
    ///
    /// Default to `.venv` in the working directory.
    ///
    /// Relative paths are resolved relative to the working directory.
    #[arg(value_hint = ValueHint::DirPath)]
    pub path: Option<PathBuf>,

    /// Provide an alternative prompt prefix for the virtual environment.
    ///
    /// By default, the prompt is dependent on whether a path was provided to `uv venv`. If provided
    /// (e.g, `uv venv project`), the prompt is set to the directory name. If not provided
    /// (`uv venv`), the prompt is set to the current directory's name.
    ///
    /// If "." is provided, the current directory name will be used regardless of whether a path was
    /// provided to `uv venv`.
    #[arg(long, verbatim_doc_comment, value_hint = ValueHint::Other)]
    pub prompt: Option<String>,

    /// Give the virtual environment access to the system site packages directory.
    ///
    /// Unlike `pip`, when a virtual environment is created with `--system-site-packages`, uv will
    /// _not_ take system site packages into account when running commands like `uv pip list` or `uv
    /// pip install`. The `--system-site-packages` flag will provide the virtual environment with
    /// access to the system site packages directory at runtime, but will not affect the behavior of
    /// uv commands.
    #[arg(long)]
    pub system_site_packages: bool,

    /// Make the virtual environment relocatable.
    ///
    /// A relocatable virtual environment can be moved around and redistributed without invalidating
    /// its associated entrypoint and activation scripts.
    ///
    /// Note that this can only be guaranteed for standard `console_scripts` and `gui_scripts`.
    /// Other scripts may be adjusted if they ship with a generic `#!python[w]` shebang, and
    /// binaries are left as-is.
    ///
    /// As a result of making the environment relocatable (by way of writing relative, rather than
    /// absolute paths), the entrypoints and scripts themselves will _not_ be relocatable. In other
    /// words, copying those entrypoints and scripts to a location outside the environment will not
    /// work, as they reference paths relative to the environment itself.
    #[arg(long, overrides_with("no_relocatable"))]
    pub relocatable: bool,

    /// Don't make the virtual environment relocatable.
    ///
    /// Disables the default relocatable behavior when the `relocatable-envs-default` preview
    /// feature is enabled.
    #[arg(long, overrides_with("relocatable"), hide = true)]
    pub no_relocatable: bool,

    #[command(flatten)]
    pub index_args: IndexArgs,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-index`). This prevents
    /// "dependency confusion" attacks, whereby an attacker can upload a malicious package under the
    /// same name to an alternate index.
    #[arg(long, value_enum, env = EnvVars::UV_INDEX_STRATEGY)]
    pub index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to use
    /// the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(long, value_enum, env = EnvVars::UV_KEYRING_PROVIDER)]
    pub keyring_provider: Option<KeyringProviderType>,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`), local dates in the same format
    /// (e.g., `2006-12-02`) resolved based on your system's configured time zone, a "friendly"
    /// duration (e.g., `24 hours`, `1 week`, `30 days`), or an ISO 8601 duration (e.g., `PT24H`,
    /// `P7D`, `P30D`).
    ///
    /// Durations do not respect semantics of the local time zone and are always resolved to a fixed
    /// number of seconds assuming that a day is 24 hours (e.g., DST transitions are ignored).
    /// Calendar units such as months and years are not allowed.
    #[arg(long, env = EnvVars::UV_EXCLUDE_NEWER)]
    pub exclude_newer: Option<ExcludeNewerValue>,

    /// Limit candidate packages for a specific package to those that were uploaded prior to the
    /// given date.
    ///
    /// Accepts package-date pairs in the format `PACKAGE=DATE`, where `DATE` is an RFC 3339
    /// timestamp (e.g., `2006-12-02T02:07:43Z`), a local date in the same format (e.g.,
    /// `2006-12-02`) resolved based on your system's configured time zone, a "friendly" duration
    /// (e.g., `24 hours`, `1 week`, `30 days`), or a ISO 8601 duration (e.g., `PT24H`, `P7D`,
    /// `P30D`).
    ///
    /// Durations do not respect semantics of the local time zone and are always resolved to a fixed
    /// number of seconds assuming that a day is 24 hours (e.g., DST transitions are ignored).
    /// Calendar units such as months and years are not allowed.
    ///
    /// Can be provided multiple times for different packages.
    #[arg(long)]
    pub exclude_newer_package: Option<Vec<ExcludeNewerPackageEntry>>,

    /// The method to use when installing packages from the global cache.
    ///
    /// This option is only used for installing seed packages.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    ///
    /// WARNING: The use of symlink link mode is discouraged, as they create tight coupling between
    /// the cache and the target environment. For example, clearing the cache (`uv cache clean`)
    /// will break all installed packages by way of removing the underlying source files. Use
    /// symlinks with caution.
    #[arg(long, value_enum, env = EnvVars::UV_LINK_MODE)]
    pub link_mode: Option<uv_install_wheel::LinkMode>,

    #[command(flatten)]
    pub refresh: RefreshArgs,

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

impl DerefMut for ExternalCommand {
    fn deref_mut(&mut self) -> &mut Self::Target {
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

#[derive(Debug, Default, Copy, Clone, clap::ValueEnum)]
pub enum AuthorFrom {
    /// Fetch the author information from some sources (e.g., Git) automatically.
    #[default]
    Auto,
    /// Fetch the author information from Git configuration only.
    Git,
    /// Do not infer the author information.
    None,
}

#[derive(Args)]
pub struct InitArgs {
    /// The path to use for the project/script.
    ///
    /// Defaults to the current working directory when initializing an app or library; required when
    /// initializing a script. Accepts relative and absolute paths.
    ///
    /// If a `pyproject.toml` is found in any of the parent directories of the target path, the
    /// project will be added as a workspace member of the parent, unless `--no-workspace` is
    /// provided.
    #[arg(required_if_eq("script", "true"), value_hint = ValueHint::DirPath)]
    pub path: Option<PathBuf>,

    /// The name of the project.
    ///
    /// Defaults to the name of the directory.
    #[arg(long, conflicts_with = "script", value_hint = ValueHint::Other)]
    pub name: Option<PackageName>,

    /// Only create a `pyproject.toml`.
    ///
    /// Disables creating extra files like `README.md`, the `src/` tree, `.python-version` files,
    /// etc.
    ///
    /// When combined with `--script`, the script will only contain the inline metadata header.
    #[arg(long)]
    pub bare: bool,

    /// Create a virtual project, rather than a package.
    ///
    /// This option is deprecated and will be removed in a future release.
    #[arg(long, hide = true, conflicts_with = "package")]
    pub r#virtual: bool,

    /// Set up the project to be built as a Python package.
    ///
    /// Defines a `[build-system]` for the project.
    ///
    /// This is the default behavior when using `--lib` or `--build-backend`.
    ///
    /// When using `--app`, this will include a `[project.scripts]` entrypoint and use a `src/`
    /// project structure.
    #[arg(long, overrides_with = "no_package")]
    pub r#package: bool,

    /// Do not set up the project to be built as a Python package.
    ///
    /// Does not include a `[build-system]` for the project.
    ///
    /// This is the default behavior when using `--app`.
    #[arg(long, overrides_with = "package", conflicts_with_all = ["lib", "build_backend"])]
    pub r#no_package: bool,

    /// Create a project for an application.
    ///
    /// This is the default behavior if `--lib` is not requested.
    ///
    /// This project kind is for web servers, scripts, and command-line interfaces.
    ///
    /// By default, an application is not intended to be built and distributed as a Python package.
    /// The `--package` option can be used to create an application that is distributable, e.g., if
    /// you want to distribute a command-line interface via PyPI.
    #[arg(long, alias = "application", conflicts_with_all = ["lib", "script"])]
    pub r#app: bool,

    /// Create a project for a library.
    ///
    /// A library is a project that is intended to be built and distributed as a Python package.
    #[arg(long, alias = "library", conflicts_with_all=["app", "script"])]
    pub r#lib: bool,

    /// Create a script.
    ///
    /// A script is a standalone file with embedded metadata enumerating its dependencies, along
    /// with any Python version requirements, as defined in the PEP 723 specification.
    ///
    /// PEP 723 scripts can be executed directly with `uv run`.
    ///
    /// By default, adds a requirement on the system Python version; use `--python` to specify an
    /// alternative Python version requirement.
    #[arg(long, conflicts_with_all=["app", "lib", "package", "build_backend", "description"])]
    pub r#script: bool,

    /// Set the project description.
    #[arg(long, conflicts_with = "script", overrides_with = "no_description", value_hint = ValueHint::Other)]
    pub description: Option<String>,

    /// Disable the description for the project.
    #[arg(long, conflicts_with = "script", overrides_with = "description")]
    pub no_description: bool,

    /// Initialize a version control system for the project.
    ///
    /// By default, uv will initialize a Git repository (`git`). Use `--vcs none` to explicitly
    /// avoid initializing a version control system.
    #[arg(long, value_enum, conflicts_with = "script")]
    pub vcs: Option<VersionControlSystem>,

    /// Initialize a build-backend of choice for the project.
    ///
    /// Implicitly sets `--package`.
    #[arg(long, value_enum, conflicts_with_all=["script", "no_package"], env = EnvVars::UV_INIT_BUILD_BACKEND)]
    pub build_backend: Option<ProjectBuildBackend>,

    /// Invalid option name for build backend.
    #[arg(
        long,
        required(false),
        action(clap::ArgAction::SetTrue),
        value_parser=clap::builder::UnknownArgumentValueParser::suggest_arg("--build-backend"),
        hide(true)
    )]
    backend: Option<String>,

    /// Do not create a `README.md` file.
    #[arg(long)]
    pub no_readme: bool,

    /// Fill in the `authors` field in the `pyproject.toml`.
    ///
    /// By default, uv will attempt to infer the author information from some sources (e.g., Git)
    /// (`auto`). Use `--author-from git` to only infer from Git configuration. Use `--author-from
    /// none` to avoid inferring the author information.
    #[arg(long, value_enum)]
    pub author_from: Option<AuthorFrom>,

    /// Do not create a `.python-version` file for the project.
    ///
    /// By default, uv will create a `.python-version` file containing the minor version of the
    /// discovered Python interpreter, which will cause subsequent uv commands to use that version.
    #[arg(long)]
    pub no_pin_python: bool,

    /// Create a `.python-version` file for the project.
    ///
    /// This is the default.
    #[arg(long, hide = true)]
    pub pin_python: bool,

    /// Avoid discovering a workspace and create a standalone project.
    ///
    /// By default, uv searches for workspaces in the current directory or any parent directory.
    #[arg(long, alias = "no-project")]
    pub no_workspace: bool,

    /// The Python interpreter to use to determine the minimum supported Python version.
    ///
    /// See `uv help python` to view supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,
}

#[derive(Args)]
pub struct RunArgs {
    /// Include optional dependencies from the specified extra name.
    ///
    /// May be provided more than once.
    ///
    /// Optional dependencies are defined via `project.optional-dependencies` in a `pyproject.toml`.
    ///
    /// This option is only available when running in a project.
    #[arg(
        long,
        conflicts_with = "all_extras",
        conflicts_with = "only_group",
        value_delimiter = ',',
        value_parser = extra_name_with_clap_error,
        value_hint = ValueHint::Other,
    )]
    pub extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    ///
    /// Optional dependencies are defined via `project.optional-dependencies` in a `pyproject.toml`.
    ///
    /// This option is only available when running in a project.
    #[arg(long, conflicts_with = "extra", conflicts_with = "only_group")]
    pub all_extras: bool,

    /// Exclude the specified optional dependencies, if `--all-extras` is supplied.
    ///
    /// May be provided multiple times.
    #[arg(long, value_hint = ValueHint::Other)]
    pub no_extra: Vec<ExtraName>,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub no_all_extras: bool,

    /// Include the development dependency group [env: UV_DEV=]
    ///
    /// Development dependencies are defined via `dependency-groups.dev` or
    /// `tool.uv.dev-dependencies` in a `pyproject.toml`.
    ///
    /// This option is an alias for `--group dev`.
    ///
    /// This option is only available when running in a project.
    #[arg(long, overrides_with("no_dev"), hide = true, value_parser = clap::builder::BoolishValueParser::new())]
    pub dev: bool,

    /// Disable the development dependency group [env: UV_NO_DEV=]
    ///
    /// This option is an alias of `--no-group dev`.
    /// See `--no-default-groups` to disable all default groups instead.
    ///
    /// This option is only available when running in a project.
    #[arg(long, overrides_with("dev"), value_parser = clap::builder::BoolishValueParser::new())]
    pub no_dev: bool,

    /// Include dependencies from the specified dependency group.
    ///
    /// May be provided multiple times.
    #[arg(long, conflicts_with_all = ["only_group", "only_dev"], value_hint = ValueHint::Other)]
    pub group: Vec<GroupName>,

    /// Disable the specified dependency group.
    ///
    /// This option always takes precedence over default groups,
    /// `--all-groups`, and `--group`.
    ///
    /// May be provided multiple times.
    #[arg(long, env = EnvVars::UV_NO_GROUP, value_delimiter = ' ', value_hint = ValueHint::Other)]
    pub no_group: Vec<GroupName>,

    /// Ignore the default dependency groups.
    ///
    /// uv includes the groups defined in `tool.uv.default-groups` by default.
    /// This disables that option, however, specific groups can still be included with `--group`.
    #[arg(long, env = EnvVars::UV_NO_DEFAULT_GROUPS, value_parser = clap::builder::BoolishValueParser::new())]
    pub no_default_groups: bool,

    /// Only include dependencies from the specified dependency group.
    ///
    /// The project and its dependencies will be omitted.
    ///
    /// May be provided multiple times. Implies `--no-default-groups`.
    #[arg(long, conflicts_with_all = ["group", "dev", "all_groups"], value_hint = ValueHint::Other)]
    pub only_group: Vec<GroupName>,

    /// Include dependencies from all dependency groups.
    ///
    /// `--no-group` can be used to exclude specific groups.
    #[arg(long, conflicts_with_all = ["only_group", "only_dev"])]
    pub all_groups: bool,

    /// Run a Python module.
    ///
    /// Equivalent to `python -m <module>`.
    #[arg(short, long, conflicts_with_all = ["script", "gui_script"])]
    pub module: bool,

    /// Only include the development dependency group.
    ///
    /// The project and its dependencies will be omitted.
    ///
    /// This option is an alias for `--only-group dev`. Implies `--no-default-groups`.
    #[arg(long, conflicts_with_all = ["group", "all_groups", "no_dev"])]
    pub only_dev: bool,

    /// Install any non-editable dependencies, including the project and any workspace members, as
    /// editable.
    #[arg(long, overrides_with = "no_editable", hide = true)]
    pub editable: bool,

    /// Install any editable dependencies, including the project and any workspace members, as
    /// non-editable [env: UV_NO_EDITABLE=]
    #[arg(long, overrides_with = "editable", value_parser = clap::builder::BoolishValueParser::new())]
    pub no_editable: bool,

    /// Do not remove extraneous packages present in the environment.
    #[arg(long, overrides_with("exact"), alias = "no-exact", hide = true)]
    pub inexact: bool,

    /// Perform an exact sync, removing extraneous packages.
    ///
    /// When enabled, uv will remove any extraneous packages from the environment. By default, `uv
    /// run` will make the minimum necessary changes to satisfy the requirements.
    #[arg(long, overrides_with("inexact"))]
    pub exact: bool,

    /// Load environment variables from a `.env` file.
    ///
    /// Can be provided multiple times, with subsequent files overriding values defined in previous
    /// files.
    #[arg(long, env = EnvVars::UV_ENV_FILE, value_hint = ValueHint::FilePath)]
    pub env_file: Vec<String>,

    /// Avoid reading environment variables from a `.env` file [env: UV_NO_ENV_FILE=]
    #[arg(long, value_parser = clap::builder::BoolishValueParser::new())]
    pub no_env_file: bool,

    /// The command to run.
    ///
    /// If the path to a Python script (i.e., ending in `.py`), it will be
    /// executed with the Python interpreter.
    #[command(subcommand)]
    pub command: Option<ExternalCommand>,

    /// Run with the given packages installed.
    ///
    /// When used in a project, these dependencies will be layered on top of the project environment
    /// in a separate, ephemeral environment. These dependencies are allowed to conflict with those
    /// specified by the project.
    #[arg(short = 'w', long, value_hint = ValueHint::Other)]
    pub with: Vec<comma::CommaSeparatedRequirements>,

    /// Run with the given packages installed in editable mode.
    ///
    /// When used in a project, these dependencies will be layered on top of the project environment
    /// in a separate, ephemeral environment. These dependencies are allowed to conflict with those
    /// specified by the project.
    #[arg(long, value_hint = ValueHint::DirPath)]
    pub with_editable: Vec<comma::CommaSeparatedRequirements>,

    /// Run with the packages listed in the given files.
    ///
    /// The following formats are supported: `requirements.txt`, `.py` files with inline metadata,
    /// and `pylock.toml`.
    ///
    /// The same environment semantics as `--with` apply.
    ///
    /// Using `pyproject.toml`, `setup.py`, or `setup.cfg` files is not allowed.
    #[arg(long, value_delimiter = ',', value_parser = parse_maybe_file_path, value_hint = ValueHint::FilePath)]
    pub with_requirements: Vec<Maybe<PathBuf>>,

    /// Run the command in an isolated virtual environment [env: UV_ISOLATED=]
    ///
    /// Usually, the project environment is reused for performance. This option forces a fresh
    /// environment to be used for the project, enforcing strict isolation between dependencies and
    /// declaration of requirements.
    ///
    /// An editable installation is still used for the project.
    ///
    /// When used with `--with` or `--with-requirements`, the additional dependencies will still be
    /// layered in a second environment.
    #[arg(long, value_parser = clap::builder::BoolishValueParser::new())]
    pub isolated: bool,

    /// Prefer the active virtual environment over the project's virtual environment.
    ///
    /// If the project virtual environment is active or no virtual environment is active, this has
    /// no effect.
    #[arg(long, overrides_with = "no_active")]
    pub active: bool,

    /// Prefer project's virtual environment over an active environment.
    ///
    /// This is the default behavior.
    #[arg(long, overrides_with = "active", hide = true)]
    pub no_active: bool,

    /// Avoid syncing the virtual environment [env: UV_NO_SYNC=]
    ///
    /// Implies `--frozen`, as the project dependencies will be ignored (i.e., the lockfile will not
    /// be updated, since the environment will not be synced regardless).
    #[arg(long, value_parser = clap::builder::BoolishValueParser::new())]
    pub no_sync: bool,

    /// Assert that the `uv.lock` will remain unchanged [env: UV_LOCKED=]
    ///
    /// Requires that the lockfile is up-to-date. If the lockfile is missing or
    /// needs to be updated, uv will exit with an error.
    #[arg(long, conflicts_with_all = ["frozen", "upgrade"])]
    pub locked: bool,

    /// Run without updating the `uv.lock` file [env: UV_FROZEN=]
    ///
    /// Instead of checking if the lockfile is up-to-date, uses the versions in the lockfile as the
    /// source of truth. If the lockfile is missing, uv will exit with an error. If the
    /// `pyproject.toml` includes changes to dependencies that have not been included in the
    /// lockfile yet, they will not be present in the environment.
    #[arg(long, conflicts_with_all = ["locked", "upgrade", "no_sources"])]
    pub frozen: bool,

    /// Run the given path as a Python script.
    ///
    /// Using `--script` will attempt to parse the path as a PEP 723 script,
    /// irrespective of its extension.
    #[arg(long, short, conflicts_with_all = ["module", "gui_script"])]
    pub script: bool,

    /// Run the given path as a Python GUI script.
    ///
    /// Using `--gui-script` will attempt to parse the path as a PEP 723 script and run it with
    /// `pythonw.exe`, irrespective of its extension. Only available on Windows.
    #[arg(long, conflicts_with_all = ["script", "module"])]
    pub gui_script: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildOptionsArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Run the command with all workspace members installed.
    ///
    /// The workspace's environment (`.venv`) is updated to include all workspace members.
    ///
    /// Any extras or groups specified via `--extra`, `--group`, or related options will be applied
    /// to all workspace members.
    #[arg(long, conflicts_with = "package")]
    pub all_packages: bool,

    /// Run the command in a specific package in the workspace.
    ///
    /// If the workspace member does not exist, uv will exit with an error.
    #[arg(long, conflicts_with = "all_packages", value_hint = ValueHint::Other)]
    pub package: Option<PackageName>,

    /// Avoid discovering the project or workspace.
    ///
    /// Instead of searching for projects in the current directory and parent directories, run in an
    /// isolated, ephemeral environment populated by the `--with` requirements.
    ///
    /// If a virtual environment is active or found in a current or parent directory, it will be
    /// used as if there was no project or workspace.
    #[arg(long, alias = "no_workspace", conflicts_with = "package")]
    pub no_project: bool,

    /// The Python interpreter to use for the run environment.
    ///
    /// If the interpreter request is satisfied by a discovered environment, the environment will be
    /// used.
    ///
    /// See `uv help python` to view supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// Whether to show resolver and installer output from any environment modifications [env:
    /// UV_SHOW_RESOLUTION=]
    ///
    /// By default, environment modifications are omitted, but enabled under `--verbose`.
    #[arg(long, value_parser = clap::builder::BoolishValueParser::new(), hide = true)]
    pub show_resolution: bool,

    /// Number of times that `uv run` will allow recursive invocations.
    ///
    /// The current recursion depth is tracked by environment variable. If environment variables are
    /// cleared, uv will fail to detect the recursion depth.
    ///
    /// If uv reaches the maximum recursion depth, it will exit with an error.
    #[arg(long, hide = true, env = EnvVars::UV_RUN_MAX_RECURSION_DEPTH)]
    pub max_recursion_depth: Option<u32>,

    /// The platform for which requirements should be installed.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aarch64-apple-darwin`.
    ///
    /// When targeting macOS (Darwin), the default minimum version is `13.0`. Use
    /// `MACOSX_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting iOS, the default minimum version is `13.0`. Use
    /// `IPHONEOS_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting Android, the default minimum Android API level is `24`. Use
    /// `ANDROID_API_LEVEL` to specify a different minimum version, e.g., `26`.
    ///
    /// WARNING: When specified, uv will select wheels that are compatible with the _target_
    /// platform; as a result, the installed distributions may not be compatible with the _current_
    /// platform. Conversely, any distributions that are built from source may be incompatible with
    /// the _target_ platform, as they will be built for the _current_ platform. The
    /// `--python-platform` option is intended for advanced use cases.
    #[arg(long)]
    pub python_platform: Option<TargetTriple>,
}

#[derive(Args)]
pub struct SyncArgs {
    /// Include optional dependencies from the specified extra name.
    ///
    /// May be provided more than once.
    ///
    /// When multiple extras or groups are specified that appear in `tool.uv.conflicts`, uv will
    /// report an error.
    ///
    /// Note that all optional dependencies are always included in the resolution; this option only
    /// affects the selection of packages to install.
    #[arg(
        long,
        conflicts_with = "all_extras",
        conflicts_with = "only_group",
        value_delimiter = ',',
        value_parser = extra_name_with_clap_error,
        value_hint = ValueHint::Other,
    )]
    pub extra: Option<Vec<ExtraName>>,

    /// Select the output format.
    #[arg(long, value_enum, default_value_t = SyncFormat::default())]
    pub output_format: SyncFormat,

    /// Include all optional dependencies.
    ///
    /// When two or more extras are declared as conflicting in `tool.uv.conflicts`, using this flag
    /// will always result in an error.
    ///
    /// Note that all optional dependencies are always included in the resolution; this option only
    /// affects the selection of packages to install.
    #[arg(long, conflicts_with = "extra", conflicts_with = "only_group")]
    pub all_extras: bool,

    /// Exclude the specified optional dependencies, if `--all-extras` is supplied.
    ///
    /// May be provided multiple times.
    #[arg(long, value_hint = ValueHint::Other)]
    pub no_extra: Vec<ExtraName>,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub no_all_extras: bool,

    /// Include the development dependency group [env: UV_DEV=]
    ///
    /// This option is an alias for `--group dev`.
    #[arg(long, overrides_with("no_dev"), hide = true, value_parser = clap::builder::BoolishValueParser::new())]
    pub dev: bool,

    /// Disable the development dependency group [env: UV_NO_DEV=]
    ///
    /// This option is an alias of `--no-group dev`.
    /// See `--no-default-groups` to disable all default groups instead.
    #[arg(long, overrides_with("dev"), value_parser = clap::builder::BoolishValueParser::new())]
    pub no_dev: bool,

    /// Only include the development dependency group.
    ///
    /// The project and its dependencies will be omitted.
    ///
    /// This option is an alias for `--only-group dev`. Implies `--no-default-groups`.
    #[arg(long, conflicts_with_all = ["group", "all_groups", "no_dev"])]
    pub only_dev: bool,

    /// Include dependencies from the specified dependency group.
    ///
    /// When multiple extras or groups are specified that appear in
    /// `tool.uv.conflicts`, uv will report an error.
    ///
    /// May be provided multiple times.
    #[arg(long, conflicts_with_all = ["only_group", "only_dev"], value_hint = ValueHint::Other)]
    pub group: Vec<GroupName>,

    /// Disable the specified dependency group.
    ///
    /// This option always takes precedence over default groups,
    /// `--all-groups`, and `--group`.
    ///
    /// May be provided multiple times.
    #[arg(long, env = EnvVars::UV_NO_GROUP, value_delimiter = ' ', value_hint = ValueHint::Other)]
    pub no_group: Vec<GroupName>,

    /// Ignore the default dependency groups.
    ///
    /// uv includes the groups defined in `tool.uv.default-groups` by default.
    /// This disables that option, however, specific groups can still be included with `--group`.
    #[arg(long, env = EnvVars::UV_NO_DEFAULT_GROUPS, value_parser = clap::builder::BoolishValueParser::new())]
    pub no_default_groups: bool,

    /// Only include dependencies from the specified dependency group.
    ///
    /// The project and its dependencies will be omitted.
    ///
    /// May be provided multiple times. Implies `--no-default-groups`.
    #[arg(long, conflicts_with_all = ["group", "dev", "all_groups"], value_hint = ValueHint::Other)]
    pub only_group: Vec<GroupName>,

    /// Include dependencies from all dependency groups.
    ///
    /// `--no-group` can be used to exclude specific groups.
    #[arg(long, conflicts_with_all = ["only_group", "only_dev"])]
    pub all_groups: bool,

    /// Install any non-editable dependencies, including the project and any workspace members, as
    /// editable.
    #[arg(long, overrides_with = "no_editable", hide = true)]
    pub editable: bool,

    /// Install any editable dependencies, including the project and any workspace members, as
    /// non-editable [env: UV_NO_EDITABLE=]
    #[arg(long, overrides_with = "editable", value_parser = clap::builder::BoolishValueParser::new())]
    pub no_editable: bool,

    /// Do not remove extraneous packages present in the environment.
    ///
    /// When enabled, uv will make the minimum necessary changes to satisfy the requirements.
    /// By default, syncing will remove any extraneous packages from the environment
    #[arg(long, overrides_with("exact"), alias = "no-exact")]
    pub inexact: bool,

    /// Perform an exact sync, removing extraneous packages.
    #[arg(long, overrides_with("inexact"), hide = true)]
    pub exact: bool,

    /// Sync dependencies to the active virtual environment.
    ///
    /// Instead of creating or updating the virtual environment for the project or script, the
    /// active virtual environment will be preferred, if the `VIRTUAL_ENV` environment variable is
    /// set.
    #[arg(long, overrides_with = "no_active")]
    pub active: bool,

    /// Prefer project's virtual environment over an active environment.
    ///
    /// This is the default behavior.
    #[arg(long, overrides_with = "active", hide = true)]
    pub no_active: bool,

    /// Do not install the current project.
    ///
    /// By default, the current project is installed into the environment with all of its
    /// dependencies. The `--no-install-project` option allows the project to be excluded, but all
    /// of its dependencies are still installed. This is particularly useful in situations like
    /// building Docker images where installing the project separately from its dependencies allows
    /// optimal layer caching.
    ///
    /// The inverse `--only-install-project` can be used to install _only_ the project itself,
    /// excluding all dependencies.
    #[arg(long, conflicts_with = "only_install_project")]
    pub no_install_project: bool,

    /// Only install the current project.
    #[arg(long, conflicts_with = "no_install_project", hide = true)]
    pub only_install_project: bool,

    /// Do not install any workspace members, including the root project.
    ///
    /// By default, all workspace members and their dependencies are installed into the
    /// environment. The `--no-install-workspace` option allows exclusion of all the workspace
    /// members while retaining their dependencies. This is particularly useful in situations like
    /// building Docker images where installing the workspace separately from its dependencies
    /// allows optimal layer caching.
    ///
    /// The inverse `--only-install-workspace` can be used to install _only_ workspace members,
    /// excluding all other dependencies.
    #[arg(long, conflicts_with = "only_install_workspace")]
    pub no_install_workspace: bool,

    /// Only install workspace members, including the root project.
    #[arg(long, conflicts_with = "no_install_workspace", hide = true)]
    pub only_install_workspace: bool,

    /// Do not install local path dependencies
    ///
    /// Skips the current project, workspace members, and any other local (path or editable)
    /// packages. Only remote/indexed dependencies are installed. Useful in Docker builds to cache
    /// heavy third-party dependencies first and layer local packages separately.
    ///
    /// The inverse `--only-install-local` can be used to install _only_ local packages, excluding
    /// all remote dependencies.
    #[arg(long, conflicts_with = "only_install_local")]
    pub no_install_local: bool,

    /// Only install local path dependencies
    #[arg(long, conflicts_with = "no_install_local", hide = true)]
    pub only_install_local: bool,

    /// Do not install the given package(s).
    ///
    /// By default, all of the project's dependencies are installed into the environment. The
    /// `--no-install-package` option allows exclusion of specific packages. Note this can result
    /// in a broken environment, and should be used with caution.
    ///
    /// The inverse `--only-install-package` can be used to install _only_ the specified packages,
    /// excluding all others.
    #[arg(long, conflicts_with = "only_install_package", value_hint = ValueHint::Other)]
    pub no_install_package: Vec<PackageName>,

    /// Only install the given package(s).
    #[arg(long, conflicts_with = "no_install_package", hide = true, value_hint = ValueHint::Other)]
    pub only_install_package: Vec<PackageName>,

    /// Assert that the `uv.lock` will remain unchanged [env: UV_LOCKED=]
    ///
    /// Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated,
    /// uv will exit with an error.
    #[arg(long, conflicts_with_all = ["frozen", "upgrade"])]
    pub locked: bool,

    /// Sync without updating the `uv.lock` file [env: UV_FROZEN=]
    ///
    /// Instead of checking if the lockfile is up-to-date, uses the versions in the lockfile as the
    /// source of truth. If the lockfile is missing, uv will exit with an error. If the
    /// `pyproject.toml` includes changes to dependencies that have not been included in the
    /// lockfile yet, they will not be present in the environment.
    #[arg(long, conflicts_with_all = ["locked", "upgrade", "no_sources"])]
    pub frozen: bool,

    /// Perform a dry run, without writing the lockfile or modifying the project environment.
    ///
    /// In dry-run mode, uv will resolve the project's dependencies and report on the resulting
    /// changes to both the lockfile and the project environment, but will not modify either.
    #[arg(long)]
    pub dry_run: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildOptionsArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Sync all packages in the workspace.
    ///
    /// The workspace's environment (`.venv`) is updated to include all workspace members.
    ///
    /// Any extras or groups specified via `--extra`, `--group`, or related options will be applied
    /// to all workspace members.
    #[arg(long, conflicts_with = "package")]
    pub all_packages: bool,

    /// Sync for specific packages in the workspace.
    ///
    /// The workspace's environment (`.venv`) is updated to reflect the subset of dependencies
    /// declared by the specified workspace member packages.
    ///
    /// If any workspace member does not exist, uv will exit with an error.
    #[arg(long, conflicts_with = "all_packages", value_hint = ValueHint::Other)]
    pub package: Vec<PackageName>,

    /// Sync the environment for a Python script, rather than the current project.
    ///
    /// If provided, uv will sync the dependencies based on the script's inline metadata table, in
    /// adherence with PEP 723.
    #[arg(
        long,
        conflicts_with = "all_packages",
        conflicts_with = "package",
        conflicts_with = "no_install_project",
        conflicts_with = "no_install_workspace",
        conflicts_with = "no_install_local",
        conflicts_with = "extra",
        conflicts_with = "all_extras",
        conflicts_with = "no_extra",
        conflicts_with = "no_all_extras",
        conflicts_with = "dev",
        conflicts_with = "no_dev",
        conflicts_with = "only_dev",
        conflicts_with = "group",
        conflicts_with = "no_group",
        conflicts_with = "no_default_groups",
        conflicts_with = "only_group",
        conflicts_with = "all_groups",
        value_hint = ValueHint::FilePath,
    )]
    pub script: Option<PathBuf>,

    /// The Python interpreter to use for the project environment.
    ///
    /// By default, the first interpreter that meets the project's `requires-python` constraint is
    /// used.
    ///
    /// If a Python interpreter in a virtual environment is provided, the packages will not be
    /// synced to the given environment. The interpreter will be used to create a virtual
    /// environment in the project.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// The platform for which requirements should be installed.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aarch64-apple-darwin`.
    ///
    /// When targeting macOS (Darwin), the default minimum version is `13.0`. Use
    /// `MACOSX_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting iOS, the default minimum version is `13.0`. Use
    /// `IPHONEOS_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting Android, the default minimum Android API level is `24`. Use
    /// `ANDROID_API_LEVEL` to specify a different minimum version, e.g., `26`.
    ///
    /// WARNING: When specified, uv will select wheels that are compatible with the _target_
    /// platform; as a result, the installed distributions may not be compatible with the _current_
    /// platform. Conversely, any distributions that are built from source may be incompatible with
    /// the _target_ platform, as they will be built for the _current_ platform. The
    /// `--python-platform` option is intended for advanced use cases.
    #[arg(long)]
    pub python_platform: Option<TargetTriple>,

    /// Check if the Python environment is synchronized with the project.
    ///
    /// If the environment is not up to date, uv will exit with an error.
    #[arg(long, overrides_with("no_check"))]
    pub check: bool,

    #[arg(long, overrides_with("check"), hide = true)]
    pub no_check: bool,
}

#[derive(Args)]
pub struct LockArgs {
    /// Check if the lockfile is up-to-date.
    ///
    /// Asserts that the `uv.lock` would remain unchanged after a resolution. If the lockfile is
    /// missing or needs to be updated, uv will exit with an error.
    ///
    /// Equivalent to `--locked`.
    #[arg(long, value_parser = clap::builder::BoolishValueParser::new(), conflicts_with_all = ["check_exists", "upgrade"], overrides_with = "check")]
    pub check: bool,

    /// Check if the lockfile is up-to-date [env: UV_LOCKED=]
    ///
    /// Asserts that the `uv.lock` would remain unchanged after a resolution. If the lockfile is
    /// missing or needs to be updated, uv will exit with an error.
    ///
    /// Equivalent to `--check`.
    #[arg(long, conflicts_with_all = ["check_exists", "upgrade"], hide = true)]
    pub locked: bool,

    /// Assert that a `uv.lock` exists without checking if it is up-to-date [env: UV_FROZEN=]
    ///
    /// Equivalent to `--frozen`.
    #[arg(long, alias = "frozen", conflicts_with_all = ["check", "locked"])]
    pub check_exists: bool,

    /// Perform a dry run, without writing the lockfile.
    ///
    /// In dry-run mode, uv will resolve the project's dependencies and report on the resulting
    /// changes, but will not write the lockfile to disk.
    #[arg(
        long,
        conflicts_with = "check_exists",
        conflicts_with = "check",
        conflicts_with = "locked"
    )]
    pub dry_run: bool,

    /// Lock the specified Python script, rather than the current project.
    ///
    /// If provided, uv will lock the script (based on its inline metadata table, in adherence with
    /// PEP 723) to a `.lock` file adjacent to the script itself.
    #[arg(long, value_hint = ValueHint::FilePath)]
    pub script: Option<PathBuf>,

    #[command(flatten)]
    pub resolver: ResolverArgs,

    #[command(flatten)]
    pub build: BuildOptionsArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// The Python interpreter to use during resolution.
    ///
    /// A Python interpreter is required for building source distributions to determine package
    /// metadata when there are not wheels.
    ///
    /// The interpreter is also used as the fallback value for the minimum Python version if
    /// `requires-python` is not set.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,
}

#[derive(Args)]
#[command(group = clap::ArgGroup::new("sources").required(true).multiple(true))]
pub struct AddArgs {
    /// The packages to add, as PEP 508 requirements (e.g., `ruff==0.5.0`).
    #[arg(group = "sources", value_hint = ValueHint::Other)]
    pub packages: Vec<String>,

    /// Add the packages listed in the given files.
    ///
    /// The following formats are supported: `requirements.txt`, `.py` files with inline metadata,
    /// `pylock.toml`, `pyproject.toml`, `setup.py`, and `setup.cfg`.
    #[arg(
        long,
        short,
        alias = "requirement",
        group = "sources",
        value_parser = parse_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub requirements: Vec<PathBuf>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. The constraints will _not_ be added to the project's
    /// `pyproject.toml` file, but _will_ be respected during dependency resolution.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[arg(
        long,
        short,
        alias = "constraint",
        env = EnvVars::UV_CONSTRAINT,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub constraints: Vec<Maybe<PathBuf>>,

    /// Apply this marker to all added packages.
    #[arg(long, short, value_parser = MarkerTree::from_str, value_hint = ValueHint::Other)]
    pub marker: Option<MarkerTree>,

    /// Add the requirements to the development dependency group [env: UV_DEV=]
    ///
    /// This option is an alias for `--group dev`.
    #[arg(
        long,
        conflicts_with("optional"),
        conflicts_with("group"),
        conflicts_with("script"),
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    pub dev: bool,

    /// Add the requirements to the package's optional dependencies for the specified extra.
    ///
    /// The group may then be activated when installing the project with the `--extra` flag.
    ///
    /// To enable an optional extra for this requirement instead, see `--extra`.
    #[arg(long, conflicts_with("dev"), conflicts_with("group"), value_hint = ValueHint::Other)]
    pub optional: Option<ExtraName>,

    /// Add the requirements to the specified dependency group.
    ///
    /// These requirements will not be included in the published metadata for the project.
    #[arg(
        long,
        conflicts_with("dev"),
        conflicts_with("optional"),
        conflicts_with("script"),
        value_hint = ValueHint::Other,
    )]
    pub group: Option<GroupName>,

    /// Add the requirements as editable.
    #[arg(long, overrides_with = "no_editable")]
    pub editable: bool,

    /// Don't add the requirements as editable [env: UV_NO_EDITABLE=]
    #[arg(long, overrides_with = "editable", hide = true, value_parser = clap::builder::BoolishValueParser::new())]
    pub no_editable: bool,

    /// Add a dependency as provided.
    ///
    /// By default, uv will use the `tool.uv.sources` section to record source information for Git,
    /// local, editable, and direct URL requirements. When `--raw` is provided, uv will add source
    /// requirements to `project.dependencies`, rather than `tool.uv.sources`.
    ///
    /// Additionally, by default, uv will add bounds to your dependency, e.g., `foo>=1.0.0`. When
    /// `--raw` is provided, uv will add the dependency without bounds.
    #[arg(
        long,
        conflicts_with = "editable",
        conflicts_with = "no_editable",
        conflicts_with = "rev",
        conflicts_with = "tag",
        conflicts_with = "branch",
        alias = "raw-sources"
    )]
    pub raw: bool,

    /// The kind of version specifier to use when adding dependencies.
    ///
    /// When adding a dependency to the project, if no constraint or URL is provided, a constraint
    /// is added based on the latest compatible version of the package. By default, a lower bound
    /// constraint is used, e.g., `>=1.2.3`.
    ///
    /// When `--frozen` is provided, no resolution is performed, and dependencies are always added
    /// without constraints.
    ///
    /// This option is in preview and may change in any future release.
    #[arg(long, value_enum)]
    pub bounds: Option<AddBoundsKind>,

    /// Commit to use when adding a dependency from Git.
    #[arg(long, group = "git-ref", action = clap::ArgAction::Set, value_hint = ValueHint::Other)]
    pub rev: Option<String>,

    /// Tag to use when adding a dependency from Git.
    #[arg(long, group = "git-ref", action = clap::ArgAction::Set, value_hint = ValueHint::Other)]
    pub tag: Option<String>,

    /// Branch to use when adding a dependency from Git.
    #[arg(long, group = "git-ref", action = clap::ArgAction::Set, value_hint = ValueHint::Other)]
    pub branch: Option<String>,

    /// Whether to use Git LFS when adding a dependency from Git.
    #[arg(long)]
    pub lfs: bool,

    /// Extras to enable for the dependency.
    ///
    /// May be provided more than once.
    ///
    /// To add this dependency to an optional extra instead, see `--optional`.
    #[arg(long, value_hint = ValueHint::Other)]
    pub extra: Option<Vec<ExtraName>>,

    /// Avoid syncing the virtual environment [env: UV_NO_SYNC=]
    #[arg(long)]
    pub no_sync: bool,

    /// Assert that the `uv.lock` will remain unchanged [env: UV_LOCKED=]
    ///
    /// Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated,
    /// uv will exit with an error.
    #[arg(long, conflicts_with_all = ["frozen", "upgrade"])]
    pub locked: bool,

    /// Add dependencies without re-locking the project [env: UV_FROZEN=]
    ///
    /// The project environment will not be synced.
    #[arg(long, conflicts_with_all = ["locked", "upgrade", "no_sources"])]
    pub frozen: bool,

    /// Prefer the active virtual environment over the project's virtual environment.
    ///
    /// If the project virtual environment is active or no virtual environment is active, this has
    /// no effect.
    #[arg(long, overrides_with = "no_active")]
    pub active: bool,

    /// Prefer project's virtual environment over an active environment.
    ///
    /// This is the default behavior.
    #[arg(long, overrides_with = "active", hide = true)]
    pub no_active: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildOptionsArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Add the dependency to a specific package in the workspace.
    #[arg(long, conflicts_with = "isolated", value_hint = ValueHint::Other)]
    pub package: Option<PackageName>,

    /// Add the dependency to the specified Python script, rather than to a project.
    ///
    /// If provided, uv will add the dependency to the script's inline metadata table, in adherence
    /// with PEP 723. If no such inline metadata table is present, a new one will be created and
    /// added to the script. When executed via `uv run`, uv will create a temporary environment for
    /// the script with all inline dependencies installed.
    #[arg(
        long,
        conflicts_with = "dev",
        conflicts_with = "optional",
        conflicts_with = "package",
        conflicts_with = "workspace",
        value_hint = ValueHint::FilePath,
    )]
    pub script: Option<PathBuf>,

    /// The Python interpreter to use for resolving and syncing.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// Add the dependency as a workspace member.
    ///
    /// By default, uv will add path dependencies that are within the workspace directory
    /// as workspace members. When used with a path dependency, the package will be added
    /// to the workspace's `members` list in the root `pyproject.toml` file.
    #[arg(long, overrides_with = "no_workspace")]
    pub workspace: bool,

    /// Don't add the dependency as a workspace member.
    ///
    /// By default, when adding a dependency that's a local path and is within the workspace
    /// directory, uv will add it as a workspace member; pass `--no-workspace` to add the package
    /// as direct path dependency instead.
    #[arg(long, overrides_with = "workspace")]
    pub no_workspace: bool,

    /// Do not install the current project.
    ///
    /// By default, the current project is installed into the environment with all of its
    /// dependencies. The `--no-install-project` option allows the project to be excluded, but all of
    /// its dependencies are still installed. This is particularly useful in situations like building
    /// Docker images where installing the project separately from its dependencies allows optimal
    /// layer caching.
    ///
    /// The inverse `--only-install-project` can be used to install _only_ the project itself,
    /// excluding all dependencies.
    #[arg(
        long,
        conflicts_with = "frozen",
        conflicts_with = "no_sync",
        conflicts_with = "only_install_project"
    )]
    pub no_install_project: bool,

    /// Only install the current project.
    #[arg(
        long,
        conflicts_with = "frozen",
        conflicts_with = "no_sync",
        conflicts_with = "no_install_project",
        hide = true
    )]
    pub only_install_project: bool,

    /// Do not install any workspace members, including the current project.
    ///
    /// By default, all workspace members and their dependencies are installed into the
    /// environment. The `--no-install-workspace` option allows exclusion of all the workspace
    /// members while retaining their dependencies. This is particularly useful in situations like
    /// building Docker images where installing the workspace separately from its dependencies
    /// allows optimal layer caching.
    ///
    /// The inverse `--only-install-workspace` can be used to install _only_ workspace members,
    /// excluding all other dependencies.
    #[arg(
        long,
        conflicts_with = "frozen",
        conflicts_with = "no_sync",
        conflicts_with = "only_install_workspace"
    )]
    pub no_install_workspace: bool,

    /// Only install workspace members, including the current project.
    #[arg(
        long,
        conflicts_with = "frozen",
        conflicts_with = "no_sync",
        conflicts_with = "no_install_workspace",
        hide = true
    )]
    pub only_install_workspace: bool,

    /// Do not install local path dependencies
    ///
    /// Skips the current project, workspace members, and any other local (path or editable)
    /// packages. Only remote/indexed dependencies are installed. Useful in Docker builds to cache
    /// heavy third-party dependencies first and layer local packages separately.
    ///
    /// The inverse `--only-install-local` can be used to install _only_ local packages, excluding
    /// all remote dependencies.
    #[arg(
        long,
        conflicts_with = "frozen",
        conflicts_with = "no_sync",
        conflicts_with = "only_install_local"
    )]
    pub no_install_local: bool,

    /// Only install local path dependencies
    #[arg(
        long,
        conflicts_with = "frozen",
        conflicts_with = "no_sync",
        conflicts_with = "no_install_local",
        hide = true
    )]
    pub only_install_local: bool,

    /// Do not install the given package(s).
    ///
    /// By default, all project's dependencies are installed into the environment. The
    /// `--no-install-package` option allows exclusion of specific packages. Note this can result
    /// in a broken environment, and should be used with caution.
    ///
    /// The inverse `--only-install-package` can be used to install _only_ the specified packages,
    /// excluding all others.
    #[arg(
        long,
        conflicts_with = "frozen",
        conflicts_with = "no_sync",
        conflicts_with = "only_install_package",
        value_hint = ValueHint::Other,
    )]
    pub no_install_package: Vec<PackageName>,

    /// Only install the given package(s).
    #[arg(
        long,
        conflicts_with = "frozen",
        conflicts_with = "no_sync",
        conflicts_with = "no_install_package",
        hide = true,
        value_hint = ValueHint::Other,
    )]
    pub only_install_package: Vec<PackageName>,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// The names of the dependencies to remove (e.g., `ruff`).
    #[arg(required = true, value_hint = ValueHint::Other)]
    pub packages: Vec<Requirement<VerbatimParsedUrl>>,

    /// Remove the packages from the development dependency group [env: UV_DEV=]
    ///
    /// This option is an alias for `--group dev`.
    #[arg(long, conflicts_with("optional"), conflicts_with("group"), value_parser = clap::builder::BoolishValueParser::new())]
    pub dev: bool,

    /// Remove the packages from the project's optional dependencies for the specified extra.
    #[arg(
        long,
        conflicts_with("dev"),
        conflicts_with("group"),
        conflicts_with("script"),
        value_hint = ValueHint::Other,
    )]
    pub optional: Option<ExtraName>,

    /// Remove the packages from the specified dependency group.
    #[arg(
        long,
        conflicts_with("dev"),
        conflicts_with("optional"),
        conflicts_with("script"),
        value_hint = ValueHint::Other,
    )]
    pub group: Option<GroupName>,

    /// Avoid syncing the virtual environment after re-locking the project [env: UV_NO_SYNC=]
    #[arg(long)]
    pub no_sync: bool,

    /// Prefer the active virtual environment over the project's virtual environment.
    ///
    /// If the project virtual environment is active or no virtual environment is active, this has
    /// no effect.
    #[arg(long, overrides_with = "no_active")]
    pub active: bool,

    /// Prefer project's virtual environment over an active environment.
    ///
    /// This is the default behavior.
    #[arg(long, overrides_with = "active", hide = true)]
    pub no_active: bool,

    /// Assert that the `uv.lock` will remain unchanged [env: UV_LOCKED=]
    ///
    /// Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated,
    /// uv will exit with an error.
    #[arg(long, conflicts_with_all = ["frozen", "upgrade"])]
    pub locked: bool,

    /// Remove dependencies without re-locking the project [env: UV_FROZEN=]
    ///
    /// The project environment will not be synced.
    #[arg(long, conflicts_with_all = ["locked", "upgrade", "no_sources"])]
    pub frozen: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildOptionsArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Remove the dependencies from a specific package in the workspace.
    #[arg(long, conflicts_with = "isolated", value_hint = ValueHint::Other)]
    pub package: Option<PackageName>,

    /// Remove the dependency from the specified Python script, rather than from a project.
    ///
    /// If provided, uv will remove the dependency from the script's inline metadata table, in
    /// adherence with PEP 723.
    #[arg(long, value_hint = ValueHint::FilePath)]
    pub script: Option<PathBuf>,

    /// The Python interpreter to use for resolving and syncing.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,
}

#[derive(Args)]
pub struct TreeArgs {
    /// Show a platform-independent dependency tree.
    ///
    /// Shows resolved package versions for all Python versions and platforms, rather than filtering
    /// to those that are relevant for the current environment.
    ///
    /// Multiple versions may be shown for a each package.
    #[arg(long)]
    pub universal: bool,

    #[command(flatten)]
    pub tree: DisplayTreeArgs,

    /// Include the development dependency group [env: UV_DEV=]
    ///
    /// Development dependencies are defined via `dependency-groups.dev` or
    /// `tool.uv.dev-dependencies` in a `pyproject.toml`.
    ///
    /// This option is an alias for `--group dev`.
    #[arg(long, overrides_with("no_dev"), hide = true, value_parser = clap::builder::BoolishValueParser::new())]
    pub dev: bool,

    /// Only include the development dependency group.
    ///
    /// The project and its dependencies will be omitted.
    ///
    /// This option is an alias for `--only-group dev`. Implies `--no-default-groups`.
    #[arg(long, conflicts_with_all = ["group", "all_groups", "no_dev"])]
    pub only_dev: bool,

    /// Disable the development dependency group [env: UV_NO_DEV=]
    ///
    /// This option is an alias of `--no-group dev`.
    /// See `--no-default-groups` to disable all default groups instead.
    #[arg(long, overrides_with("dev"), value_parser = clap::builder::BoolishValueParser::new())]
    pub no_dev: bool,

    /// Include dependencies from the specified dependency group.
    ///
    /// May be provided multiple times.
    #[arg(long, conflicts_with_all = ["only_group", "only_dev"])]
    pub group: Vec<GroupName>,

    /// Disable the specified dependency group.
    ///
    /// This option always takes precedence over default groups,
    /// `--all-groups`, and `--group`.
    ///
    /// May be provided multiple times.
    #[arg(long, env = EnvVars::UV_NO_GROUP, value_delimiter = ' ')]
    pub no_group: Vec<GroupName>,

    /// Ignore the default dependency groups.
    ///
    /// uv includes the groups defined in `tool.uv.default-groups` by default.
    /// This disables that option, however, specific groups can still be included with `--group`.
    #[arg(long, env = EnvVars::UV_NO_DEFAULT_GROUPS, value_parser = clap::builder::BoolishValueParser::new())]
    pub no_default_groups: bool,

    /// Only include dependencies from the specified dependency group.
    ///
    /// The project and its dependencies will be omitted.
    ///
    /// May be provided multiple times. Implies `--no-default-groups`.
    #[arg(long, conflicts_with_all = ["group", "dev", "all_groups"])]
    pub only_group: Vec<GroupName>,

    /// Include dependencies from all dependency groups.
    ///
    /// `--no-group` can be used to exclude specific groups.
    #[arg(long, conflicts_with_all = ["only_group", "only_dev"])]
    pub all_groups: bool,

    /// Assert that the `uv.lock` will remain unchanged [env: UV_LOCKED=]
    ///
    /// Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated,
    /// uv will exit with an error.
    #[arg(long, conflicts_with_all = ["frozen", "upgrade"])]
    pub locked: bool,

    /// Display the requirements without locking the project [env: UV_FROZEN=]
    ///
    /// If the lockfile is missing, uv will exit with an error.
    #[arg(long, conflicts_with_all = ["locked", "upgrade", "no_sources"])]
    pub frozen: bool,

    #[command(flatten)]
    pub build: BuildOptionsArgs,

    #[command(flatten)]
    pub resolver: ResolverArgs,

    /// Show the dependency tree the specified PEP 723 Python script, rather than the current
    /// project.
    ///
    /// If provided, uv will resolve the dependencies based on its inline metadata table, in
    /// adherence with PEP 723.
    #[arg(long, value_hint = ValueHint::FilePath)]
    pub script: Option<PathBuf>,

    /// The Python version to use when filtering the tree.
    ///
    /// For example, pass `--python-version 3.10` to display the dependencies that would be included
    /// when installing on Python 3.10.
    ///
    /// Defaults to the version of the discovered Python interpreter.
    #[arg(long, conflicts_with = "universal")]
    pub python_version: Option<PythonVersion>,

    /// The platform to use when filtering the tree.
    ///
    /// For example, pass `--platform windows` to display the dependencies that would be included
    /// when installing on Windows.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aarch64-apple-darwin`.
    #[arg(long, conflicts_with = "universal")]
    pub python_platform: Option<TargetTriple>,

    /// The Python interpreter to use for locking and filtering.
    ///
    /// By default, the tree is filtered to match the platform as reported by the Python
    /// interpreter. Use `--universal` to display the tree for all platforms, or use
    /// `--python-version` or `--python-platform` to override a subset of markers.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,
}

#[derive(Args)]
pub struct ExportArgs {
    /// The format to which `uv.lock` should be exported.
    ///
    /// Supports `requirements.txt`, `pylock.toml` (PEP 751) and CycloneDX v1.5 JSON output formats.
    ///
    /// uv will infer the output format from the file extension of the output file, if
    /// provided. Otherwise, defaults to `requirements.txt`.
    #[arg(long, value_enum)]
    pub format: Option<ExportFormat>,

    /// Export the entire workspace.
    ///
    /// The dependencies for all workspace members will be included in the exported requirements
    /// file.
    ///
    /// Any extras or groups specified via `--extra`, `--group`, or related options will be applied
    /// to all workspace members.
    #[arg(long, conflicts_with = "package")]
    pub all_packages: bool,

    /// Export the dependencies for specific packages in the workspace.
    ///
    /// If any workspace member does not exist, uv will exit with an error.
    #[arg(long, conflicts_with = "all_packages", value_hint = ValueHint::Other)]
    pub package: Vec<PackageName>,

    /// Prune the given package from the dependency tree.
    ///
    /// Pruned packages will be excluded from the exported requirements file, as will any
    /// dependencies that are no longer required after the pruned package is removed.
    #[arg(long, conflicts_with = "all_packages", value_name = "PACKAGE")]
    pub prune: Vec<PackageName>,

    /// Include optional dependencies from the specified extra name.
    ///
    /// May be provided more than once.
    #[arg(long, value_delimiter = ',', conflicts_with = "all_extras", conflicts_with = "only_group", value_parser = extra_name_with_clap_error)]
    pub extra: Option<Vec<ExtraName>>,

    /// Include all optional dependencies.
    #[arg(long, conflicts_with = "extra", conflicts_with = "only_group")]
    pub all_extras: bool,

    /// Exclude the specified optional dependencies, if `--all-extras` is supplied.
    ///
    /// May be provided multiple times.
    #[arg(long)]
    pub no_extra: Vec<ExtraName>,

    #[arg(long, overrides_with("all_extras"), hide = true)]
    pub no_all_extras: bool,

    /// Include the development dependency group [env: UV_DEV=]
    ///
    /// This option is an alias for `--group dev`.
    #[arg(long, overrides_with("no_dev"), hide = true, value_parser = clap::builder::BoolishValueParser::new())]
    pub dev: bool,

    /// Disable the development dependency group [env: UV_NO_DEV=]
    ///
    /// This option is an alias of `--no-group dev`.
    /// See `--no-default-groups` to disable all default groups instead.
    #[arg(long, overrides_with("dev"), value_parser = clap::builder::BoolishValueParser::new())]
    pub no_dev: bool,

    /// Only include the development dependency group.
    ///
    /// The project and its dependencies will be omitted.
    ///
    /// This option is an alias for `--only-group dev`. Implies `--no-default-groups`.
    #[arg(long, conflicts_with_all = ["group", "all_groups", "no_dev"])]
    pub only_dev: bool,

    /// Include dependencies from the specified dependency group.
    ///
    /// May be provided multiple times.
    #[arg(long, conflicts_with_all = ["only_group", "only_dev"])]
    pub group: Vec<GroupName>,

    /// Disable the specified dependency group.
    ///
    /// This option always takes precedence over default groups,
    /// `--all-groups`, and `--group`.
    ///
    /// May be provided multiple times.
    #[arg(long, env = EnvVars::UV_NO_GROUP, value_delimiter = ' ')]
    pub no_group: Vec<GroupName>,

    /// Ignore the default dependency groups.
    ///
    /// uv includes the groups defined in `tool.uv.default-groups` by default.
    /// This disables that option, however, specific groups can still be included with `--group`.
    #[arg(long, env = EnvVars::UV_NO_DEFAULT_GROUPS, value_parser = clap::builder::BoolishValueParser::new())]
    pub no_default_groups: bool,

    /// Only include dependencies from the specified dependency group.
    ///
    /// The project and its dependencies will be omitted.
    ///
    /// May be provided multiple times. Implies `--no-default-groups`.
    #[arg(long, conflicts_with_all = ["group", "dev", "all_groups"])]
    pub only_group: Vec<GroupName>,

    /// Include dependencies from all dependency groups.
    ///
    /// `--no-group` can be used to exclude specific groups.
    #[arg(long, conflicts_with_all = ["only_group", "only_dev"])]
    pub all_groups: bool,

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

    /// Export any non-editable dependencies, including the project and any workspace members, as
    /// editable.
    #[arg(long, overrides_with = "no_editable", hide = true)]
    pub editable: bool,

    /// Export any editable dependencies, including the project and any workspace members, as
    /// non-editable [env: UV_NO_EDITABLE=]
    #[arg(long, overrides_with = "editable", value_parser = clap::builder::BoolishValueParser::new())]
    pub no_editable: bool,

    /// Include hashes for all dependencies.
    #[arg(long, overrides_with("no_hashes"), hide = true)]
    pub hashes: bool,

    /// Omit hashes in the generated output.
    #[arg(long, overrides_with("hashes"))]
    pub no_hashes: bool,

    /// Write the exported requirements to the given file.
    #[arg(long, short, value_hint = ValueHint::FilePath)]
    pub output_file: Option<PathBuf>,

    /// Do not emit the current project.
    ///
    /// By default, the current project is included in the exported requirements file with all of
    /// its dependencies. The `--no-emit-project` option allows the project to be excluded, but all
    /// of its dependencies to remain included.
    ///
    /// The inverse `--only-emit-project` can be used to emit _only_ the project itself, excluding
    /// all dependencies.
    #[arg(
        long,
        alias = "no-install-project",
        conflicts_with = "only_emit_project"
    )]
    pub no_emit_project: bool,

    /// Only emit the current project.
    #[arg(
        long,
        alias = "only-install-project",
        conflicts_with = "no_emit_project",
        hide = true
    )]
    pub only_emit_project: bool,

    /// Do not emit any workspace members, including the root project.
    ///
    /// By default, all workspace members and their dependencies are included in the exported
    /// requirements file, with all of their dependencies. The `--no-emit-workspace` option allows
    /// exclusion of all the workspace members while retaining their dependencies.
    ///
    /// The inverse `--only-emit-workspace` can be used to emit _only_ workspace members, excluding
    /// all other dependencies.
    #[arg(
        long,
        alias = "no-install-workspace",
        conflicts_with = "only_emit_workspace"
    )]
    pub no_emit_workspace: bool,

    /// Only emit workspace members, including the root project.
    #[arg(
        long,
        alias = "only-install-workspace",
        conflicts_with = "no_emit_workspace",
        hide = true
    )]
    pub only_emit_workspace: bool,

    /// Do not include local path dependencies in the exported requirements.
    ///
    /// Omits the current project, workspace members, and any other local (path or editable)
    /// packages from the export. Only remote/indexed dependencies are written. Useful for Docker
    /// and CI flows that want to export and cache third-party dependencies first.
    ///
    /// The inverse `--only-emit-local` can be used to emit _only_ local packages, excluding all
    /// remote dependencies.
    #[arg(long, alias = "no-install-local", conflicts_with = "only_emit_local")]
    pub no_emit_local: bool,

    /// Only include local path dependencies in the exported requirements.
    #[arg(
        long,
        alias = "only-install-local",
        conflicts_with = "no_emit_local",
        hide = true
    )]
    pub only_emit_local: bool,

    /// Do not emit the given package(s).
    ///
    /// By default, all project's dependencies are included in the exported requirements
    /// file. The `--no-emit-package` option allows exclusion of specific packages.
    ///
    /// The inverse `--only-emit-package` can be used to emit _only_ the specified packages,
    /// excluding all others.
    #[arg(
        long,
        alias = "no-install-package",
        conflicts_with = "only_emit_package",
        value_hint = ValueHint::Other,
    )]
    pub no_emit_package: Vec<PackageName>,

    /// Only emit the given package(s).
    #[arg(
        long,
        alias = "only-install-package",
        conflicts_with = "no_emit_package",
        hide = true,
        value_hint = ValueHint::Other,
    )]
    pub only_emit_package: Vec<PackageName>,

    /// Assert that the `uv.lock` will remain unchanged [env: UV_LOCKED=]
    ///
    /// Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated,
    /// uv will exit with an error.
    #[arg(long, conflicts_with_all = ["frozen", "upgrade"])]
    pub locked: bool,

    /// Do not update the `uv.lock` before exporting [env: UV_FROZEN=]
    ///
    /// If a `uv.lock` does not exist, uv will exit with an error.
    #[arg(long, conflicts_with_all = ["locked", "upgrade", "no_sources"])]
    pub frozen: bool,

    #[command(flatten)]
    pub resolver: ResolverArgs,

    #[command(flatten)]
    pub build: BuildOptionsArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Export the dependencies for the specified PEP 723 Python script, rather than the current
    /// project.
    ///
    /// If provided, uv will resolve the dependencies based on its inline metadata table, in
    /// adherence with PEP 723.
    #[arg(
        long,
        conflicts_with_all = ["all_packages", "package", "no_emit_project", "no_emit_workspace"],
        value_hint = ValueHint::FilePath,
    )]
    pub script: Option<PathBuf>,

    /// The Python interpreter to use during resolution.
    ///
    /// A Python interpreter is required for building source distributions to determine package
    /// metadata when there are not wheels.
    ///
    /// The interpreter is also used as the fallback value for the minimum Python version if
    /// `requires-python` is not set.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,
}

#[derive(Args)]
pub struct FormatArgs {
    /// Check if files are formatted without applying changes.
    #[arg(long)]
    pub check: bool,

    /// Show a diff of formatting changes without applying them.
    ///
    /// Implies `--check`.
    #[arg(long)]
    pub diff: bool,

    /// The version of Ruff to use for formatting.
    ///
    /// Accepts either a version (e.g., `0.8.2`) which will be treated as an exact pin,
    /// a version specifier (e.g., `>=0.8.0`), or `latest` to use the latest available version.
    ///
    /// By default, a constrained version range of Ruff will be used (e.g., `>=0.15,<0.16`).
    #[arg(long, value_hint = ValueHint::Other)]
    pub version: Option<String>,

    /// Limit candidate Ruff versions to those released prior to the given date.
    ///
    /// Accepts a superset of [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) (e.g.,
    /// `2006-12-02T02:07:43Z`) or local date in the same format (e.g. `2006-12-02`), as well as
    /// durations relative to "now" (e.g., `-1 week`).
    #[arg(long, env = EnvVars::UV_EXCLUDE_NEWER)]
    pub exclude_newer: Option<ExcludeNewerValue>,

    /// Additional arguments to pass to Ruff.
    ///
    /// For example, use `uv format -- --line-length 100` to set the line length or
    /// `uv format -- src/module/foo.py` to format a specific file.
    #[arg(last = true, value_hint = ValueHint::Other)]
    pub extra_args: Vec<String>,

    /// Avoid discovering a project or workspace.
    ///
    /// Instead of running the formatter in the context of the current project, run it in the
    /// context of the current directory. This is useful when the current directory is not a
    /// project.
    #[arg(long)]
    pub no_project: bool,

    /// Display the version of Ruff that will be used for formatting.
    ///
    /// This is useful for verifying which version was resolved when using version constraints
    /// (e.g., `--version ">=0.8.0"`) or `--version latest`.
    #[arg(long, hide = true)]
    pub show_version: bool,
}

#[derive(Args)]
pub struct AuthNamespace {
    #[command(subcommand)]
    pub command: AuthCommand,
}

#[derive(Subcommand)]
pub enum AuthCommand {
    /// Login to a service
    Login(AuthLoginArgs),
    /// Logout of a service
    Logout(AuthLogoutArgs),
    /// Show the authentication token for a service
    Token(AuthTokenArgs),
    /// Show the path to the uv credentials directory.
    ///
    /// By default, credentials are stored in the uv data directory at
    /// `$XDG_DATA_HOME/uv/credentials` or `$HOME/.local/share/uv/credentials` on Unix and
    /// `%APPDATA%\uv\data\credentials` on Windows.
    ///
    /// The credentials directory may be overridden with `$UV_CREDENTIALS_DIR`.
    ///
    /// Credentials are only stored in this directory when the plaintext backend is used, as
    /// opposed to the native backend, which uses the system keyring.
    Dir(AuthDirArgs),
    /// Act as a credential helper for external tools.
    ///
    /// Implements the Bazel credential helper protocol to provide credentials
    /// to external tools via JSON over stdin/stdout.
    ///
    /// This command is typically invoked by external tools.
    #[command(hide = true)]
    Helper(AuthHelperArgs),
}

#[derive(Args)]
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
    /// The name of the command can include an exact version in the format `<package>@<version>`,
    /// e.g., `uv tool run ruff@0.3.0`. If more complex version specification is desired or if the
    /// command is provided by a different package, use `--from`.
    ///
    /// `uvx` can be used to invoke Python, e.g., with `uvx python` or `uvx python@<version>`. A
    /// Python interpreter will be started in an isolated virtual environment.
    ///
    /// If the tool was previously installed, i.e., via `uv tool install`, the installed version
    /// will be used unless a version is requested or the `--isolated` flag is used.
    ///
    /// `uvx` is provided as a convenient alias for `uv tool run`, their behavior is identical.
    ///
    /// If no command is provided, the installed tools are displayed.
    ///
    /// Packages are installed into an ephemeral virtual environment in the uv cache directory.
    #[command(
        after_help = "Use `uvx` as a shortcut for `uv tool run`.\n\n\
        Use `uv help tool run` for more details.",
        after_long_help = ""
    )]
    Run(ToolRunArgs),
    /// Hidden alias for `uv tool run` for the `uvx` command
    #[command(
        hide = true,
        override_usage = "uvx [OPTIONS] [COMMAND]",
        about = "Run a command provided by a Python package.",
        after_help = "Use `uv help tool run` for more details.",
        after_long_help = "",
        display_name = "uvx",
        long_version = crate::version::uv_self_version()
    )]
    Uvx(UvxArgs),
    /// Install commands provided by a Python package.
    ///
    /// Packages are installed into an isolated virtual environment in the uv tools directory. The
    /// executables are linked the tool executable directory, which is determined according to the
    /// XDG standard and can be retrieved with `uv tool dir --bin`.
    ///
    /// If the tool was previously installed, the existing tool will generally be replaced.
    Install(ToolInstallArgs),
    /// Upgrade installed tools.
    ///
    /// If a tool was installed with version constraints, they will be respected on upgrade — to
    /// upgrade a tool beyond the originally provided constraints, use `uv tool install` again.
    ///
    /// If a tool was installed with specific settings, they will be respected on upgraded. For
    /// example, if `--prereleases allow` was provided during installation, it will continue to be
    /// respected in upgrades.
    #[command(alias = "update")]
    Upgrade(ToolUpgradeArgs),
    /// List installed tools.
    #[command(alias = "ls")]
    List(ToolListArgs),
    /// Uninstall a tool.
    Uninstall(ToolUninstallArgs),
    /// Ensure that the tool executable directory is on the `PATH`.
    ///
    /// If the tool executable directory is not present on the `PATH`, uv will attempt to add it to
    /// the relevant shell configuration files.
    ///
    /// If the shell configuration files already include a blurb to add the executable directory to
    /// the path, but the directory is not present on the `PATH`, uv will exit with an error.
    ///
    /// The tool executable directory is determined according to the XDG standard and can be
    /// retrieved with `uv tool dir --bin`.
    #[command(alias = "ensurepath")]
    UpdateShell,
    /// Show the path to the uv tools directory.
    ///
    /// The tools directory is used to store environments and metadata for installed tools.
    ///
    /// By default, tools are stored in the uv data directory at `$XDG_DATA_HOME/uv/tools` or
    /// `$HOME/.local/share/uv/tools` on Unix and `%APPDATA%\uv\data\tools` on Windows.
    ///
    /// The tool installation directory may be overridden with `$UV_TOOL_DIR`.
    ///
    /// To instead view the directory uv installs executables into, use the `--bin` flag.
    Dir(ToolDirArgs),
}

#[derive(Args)]
pub struct ToolRunArgs {
    /// The command to run.
    ///
    /// WARNING: The documentation for [`Self::command`] is not included in help output
    #[command(subcommand)]
    pub command: Option<ExternalCommand>,

    /// Use the given package to provide the command.
    ///
    /// By default, the package name is assumed to match the command name.
    #[arg(long, value_hint = ValueHint::Other)]
    pub from: Option<String>,

    /// Run with the given packages installed.
    #[arg(short = 'w', long, value_hint = ValueHint::Other)]
    pub with: Vec<comma::CommaSeparatedRequirements>,

    /// Run with the given packages installed in editable mode
    ///
    /// When used in a project, these dependencies will be layered on top of the uv tool's
    /// environment in a separate, ephemeral environment. These dependencies are allowed to conflict
    /// with those specified.
    #[arg(long, value_hint = ValueHint::DirPath)]
    pub with_editable: Vec<comma::CommaSeparatedRequirements>,

    /// Run with the packages listed in the given files.
    ///
    /// The following formats are supported: `requirements.txt`, `.py` files with inline metadata,
    /// and `pylock.toml`.
    #[arg(
        long,
        value_delimiter = ',',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub with_requirements: Vec<Maybe<PathBuf>>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[arg(
        long,
        short,
        alias = "constraint",
        env = EnvVars::UV_CONSTRAINT,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub constraints: Vec<Maybe<PathBuf>>,

    /// Constrain build dependencies using the given requirements files when building source
    /// distributions.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    #[arg(
        long,
        short,
        alias = "build-constraint",
        env = EnvVars::UV_BUILD_CONSTRAINT,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub build_constraints: Vec<Maybe<PathBuf>>,

    /// Override versions using the given requirements files.
    ///
    /// Overrides files are `requirements.txt`-like files that force a specific version of a
    /// requirement to be installed, regardless of the requirements declared by any constituent
    /// package, and regardless of whether this would be considered an invalid resolution.
    ///
    /// While constraints are _additive_, in that they're combined with the requirements of the
    /// constituent packages, overrides are _absolute_, in that they completely replace the
    /// requirements of the constituent packages.
    #[arg(
        long,
        alias = "override",
        env = EnvVars::UV_OVERRIDE,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub overrides: Vec<Maybe<PathBuf>>,

    /// Run the tool in an isolated virtual environment, ignoring any already-installed tools [env:
    /// UV_ISOLATED=]
    #[arg(long, value_parser = clap::builder::BoolishValueParser::new())]
    pub isolated: bool,

    /// Load environment variables from a `.env` file.
    ///
    /// Can be provided multiple times, with subsequent files overriding values defined in previous
    /// files.
    #[arg(long, value_delimiter = ' ', env = EnvVars::UV_ENV_FILE, value_hint = ValueHint::FilePath)]
    pub env_file: Vec<PathBuf>,

    /// Avoid reading environment variables from a `.env` file [env: UV_NO_ENV_FILE=]
    #[arg(long, value_parser = clap::builder::BoolishValueParser::new())]
    pub no_env_file: bool,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildOptionsArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Whether to use Git LFS when adding a dependency from Git.
    #[arg(long)]
    pub lfs: bool,

    /// The Python interpreter to use to build the run environment.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// Whether to show resolver and installer output from any environment modifications [env:
    /// UV_SHOW_RESOLUTION=]
    ///
    /// By default, environment modifications are omitted, but enabled under `--verbose`.
    #[arg(long, value_parser = clap::builder::BoolishValueParser::new(), hide = true)]
    pub show_resolution: bool,

    /// The platform for which requirements should be installed.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aarch64-apple-darwin`.
    ///
    /// When targeting macOS (Darwin), the default minimum version is `13.0`. Use
    /// `MACOSX_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting iOS, the default minimum version is `13.0`. Use
    /// `IPHONEOS_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting Android, the default minimum Android API level is `24`. Use
    /// `ANDROID_API_LEVEL` to specify a different minimum version, e.g., `26`.
    ///
    /// WARNING: When specified, uv will select wheels that are compatible with the _target_
    /// platform; as a result, the installed distributions may not be compatible with the _current_
    /// platform. Conversely, any distributions that are built from source may be incompatible with
    /// the _target_ platform, as they will be built for the _current_ platform. The
    /// `--python-platform` option is intended for advanced use cases.
    #[arg(long)]
    pub python_platform: Option<TargetTriple>,

    /// The backend to use when fetching packages in the PyTorch ecosystem (e.g., `cpu`, `cu126`, or `auto`)
    ///
    /// When set, uv will ignore the configured index URLs for packages in the PyTorch ecosystem,
    /// and will instead use the defined backend.
    ///
    /// For example, when set to `cpu`, uv will use the CPU-only PyTorch index; when set to `cu126`,
    /// uv will use the PyTorch index for CUDA 12.6.
    ///
    /// The `auto` mode will attempt to detect the appropriate PyTorch index based on the currently
    /// installed CUDA drivers.
    ///
    /// This option is in preview and may change in any future release.
    #[arg(long, value_enum, env = EnvVars::UV_TORCH_BACKEND)]
    pub torch_backend: Option<TorchMode>,

    #[arg(long, hide = true)]
    pub generate_shell_completion: Option<clap_complete_command::Shell>,
}

#[derive(Args)]
pub struct UvxArgs {
    #[command(flatten)]
    pub tool_run: ToolRunArgs,

    /// Display the uvx version.
    #[arg(short = 'V', long, action = clap::ArgAction::Version)]
    pub version: Option<bool>,
}

#[derive(Args)]
pub struct ToolInstallArgs {
    /// The package to install commands from.
    #[arg(value_hint = ValueHint::Other)]
    pub package: String,

    /// The package to install commands from.
    ///
    /// This option is provided for parity with `uv tool run`, but is redundant with `package`.
    #[arg(long, hide = true, value_hint = ValueHint::Other)]
    pub from: Option<String>,

    /// Include the following additional requirements.
    #[arg(short = 'w', long, value_hint = ValueHint::Other)]
    pub with: Vec<comma::CommaSeparatedRequirements>,

    /// Run with the packages listed in the given files.
    ///
    /// The following formats are supported: `requirements.txt`, `.py` files with inline metadata,
    /// and `pylock.toml`.
    #[arg(long, value_delimiter = ',', value_parser = parse_maybe_file_path, value_hint = ValueHint::FilePath)]
    pub with_requirements: Vec<Maybe<PathBuf>>,

    /// Install the target package in editable mode, such that changes in the package's source
    /// directory are reflected without reinstallation.
    #[arg(short, long)]
    pub editable: bool,

    /// Include the given packages in editable mode.
    #[arg(long, value_hint = ValueHint::DirPath)]
    pub with_editable: Vec<comma::CommaSeparatedRequirements>,

    /// Install executables from the following packages.
    #[arg(long, value_hint = ValueHint::Other)]
    pub with_executables_from: Vec<comma::CommaSeparatedRequirements>,

    /// Constrain versions using the given requirements files.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    ///
    /// This is equivalent to pip's `--constraint` option.
    #[arg(
        long,
        short,
        alias = "constraint",
        env = EnvVars::UV_CONSTRAINT,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub constraints: Vec<Maybe<PathBuf>>,

    /// Override versions using the given requirements files.
    ///
    /// Overrides files are `requirements.txt`-like files that force a specific version of a
    /// requirement to be installed, regardless of the requirements declared by any constituent
    /// package, and regardless of whether this would be considered an invalid resolution.
    ///
    /// While constraints are _additive_, in that they're combined with the requirements of the
    /// constituent packages, overrides are _absolute_, in that they completely replace the
    /// requirements of the constituent packages.
    #[arg(
        long,
        alias = "override",
        env = EnvVars::UV_OVERRIDE,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub overrides: Vec<Maybe<PathBuf>>,

    /// Exclude packages from resolution using the given requirements files.
    ///
    /// Excludes files are `requirements.txt`-like files that specify packages to exclude
    /// from the resolution. When a package is excluded, it will be omitted from the
    /// dependency list entirely and its own dependencies will be ignored during the resolution
    /// phase. Excludes are unconditional in that requirement specifiers and markers are ignored;
    /// any package listed in the provided file will be omitted from all resolved environments.
    #[arg(
        long,
        alias = "exclude",
        env = EnvVars::UV_EXCLUDE,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub excludes: Vec<Maybe<PathBuf>>,

    /// Constrain build dependencies using the given requirements files when building source
    /// distributions.
    ///
    /// Constraints files are `requirements.txt`-like files that only control the _version_ of a
    /// requirement that's installed. However, including a package in a constraints file will _not_
    /// trigger the installation of that package.
    #[arg(
        long,
        short,
        alias = "build-constraint",
        env = EnvVars::UV_BUILD_CONSTRAINT,
        value_delimiter = ' ',
        value_parser = parse_maybe_file_path,
        value_hint = ValueHint::FilePath,
    )]
    pub build_constraints: Vec<Maybe<PathBuf>>,

    #[command(flatten)]
    pub installer: ResolverInstallerArgs,

    #[command(flatten)]
    pub build: BuildOptionsArgs,

    #[command(flatten)]
    pub refresh: RefreshArgs,

    /// Force installation of the tool.
    ///
    /// Will replace any existing entry points with the same name in the executable directory.
    #[arg(long)]
    pub force: bool,

    /// Whether to use Git LFS when adding a dependency from Git.
    #[arg(long)]
    pub lfs: bool,

    /// The Python interpreter to use to build the tool environment.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// The platform for which requirements should be installed.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aarch64-apple-darwin`.
    ///
    /// When targeting macOS (Darwin), the default minimum version is `13.0`. Use
    /// `MACOSX_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting iOS, the default minimum version is `13.0`. Use
    /// `IPHONEOS_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting Android, the default minimum Android API level is `24`. Use
    /// `ANDROID_API_LEVEL` to specify a different minimum version, e.g., `26`.
    ///
    /// WARNING: When specified, uv will select wheels that are compatible with the _target_
    /// platform; as a result, the installed distributions may not be compatible with the _current_
    /// platform. Conversely, any distributions that are built from source may be incompatible with
    /// the _target_ platform, as they will be built for the _current_ platform. The
    /// `--python-platform` option is intended for advanced use cases.
    #[arg(long)]
    pub python_platform: Option<TargetTriple>,

    /// The backend to use when fetching packages in the PyTorch ecosystem (e.g., `cpu`, `cu126`, or `auto`)
    ///
    /// When set, uv will ignore the configured index URLs for packages in the PyTorch ecosystem,
    /// and will instead use the defined backend.
    ///
    /// For example, when set to `cpu`, uv will use the CPU-only PyTorch index; when set to `cu126`,
    /// uv will use the PyTorch index for CUDA 12.6.
    ///
    /// The `auto` mode will attempt to detect the appropriate PyTorch index based on the currently
    /// installed CUDA drivers.
    ///
    /// This option is in preview and may change in any future release.
    #[arg(long, value_enum, env = EnvVars::UV_TORCH_BACKEND)]
    pub torch_backend: Option<TorchMode>,
}

#[derive(Args)]
pub struct ToolListArgs {
    /// Whether to display the path to each tool environment and installed executable.
    #[arg(long)]
    pub show_paths: bool,

    /// Whether to display the version specifier(s) used to install each tool.
    #[arg(long)]
    pub show_version_specifiers: bool,

    /// Whether to display the additional requirements installed with each tool.
    #[arg(long)]
    pub show_with: bool,

    /// Whether to display the extra requirements installed with each tool.
    #[arg(long)]
    pub show_extras: bool,

    /// Whether to display the Python version associated with each tool.
    #[arg(long)]
    pub show_python: bool,

    // Hide unused global Python options.
    #[arg(long, hide = true)]
    pub python_preference: Option<PythonPreference>,

    #[arg(long, hide = true)]
    pub no_python_downloads: bool,
}

#[derive(Args)]
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
pub struct ToolUninstallArgs {
    /// The name of the tool to uninstall.
    #[arg(required = true, value_hint = ValueHint::Other)]
    pub name: Vec<PackageName>,

    /// Uninstall all tools.
    #[arg(long, conflicts_with("name"))]
    pub all: bool,
}

#[derive(Args)]
pub struct ToolUpgradeArgs {
    /// The name of the tool to upgrade, along with an optional version specifier.
    #[arg(required = true, value_hint = ValueHint::Other)]
    pub name: Vec<String>,

    /// Upgrade all tools.
    #[arg(long, conflicts_with("name"))]
    pub all: bool,

    /// Upgrade a tool, and specify it to use the given Python interpreter to build its environment.
    /// Use with `--all` to apply to all tools.
    ///
    /// See `uv help python` for details on Python discovery and supported request formats.
    #[arg(
        long,
        short,
        env = EnvVars::UV_PYTHON,
        verbatim_doc_comment,
        help_heading = "Python options",
        value_parser = parse_maybe_string,
        value_hint = ValueHint::Other,
    )]
    pub python: Option<Maybe<String>>,

    /// The platform for which requirements should be installed.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aarch64-apple-darwin`.
    ///
    /// When targeting macOS (Darwin), the default minimum version is `13.0`. Use
    /// `MACOSX_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting iOS, the default minimum version is `13.0`. Use
    /// `IPHONEOS_DEPLOYMENT_TARGET` to specify a different minimum version, e.g., `14.0`.
    ///
    /// When targeting Android, the default minimum Android API level is `24`. Use
    /// `ANDROID_API_LEVEL` to specify a different minimum version, e.g., `26`.
    ///
    /// WARNING: When specified, uv will select wheels that are compatible with the _target_
    /// platform; as a result, the installed distributions may not be compatible with the _current_
    /// platform. Conversely, any distributions that are built from source may be incompatible with
    /// the _target_ platform, as they will be built for the _current_ platform. The
    /// `--python-platform` option is intended for advanced use cases.
    #[arg(long)]
    pub python_platform: Option<TargetTriple>,

    // The following is equivalent to flattening `ResolverInstallerArgs`, with the `--upgrade`, and
    // `--upgrade-package` options hidden, and the `--no-upgrade` option removed.
    /// Allow package upgrades, ignoring pinned versions in any existing output file. Implies
    /// `--refresh`.
    #[arg(hide = true, long, short = 'U', help_heading = "Resolver options")]
    pub upgrade: bool,

    /// Allow upgrades for a specific package, ignoring pinned versions in any existing output
    /// file. Implies `--refresh-package`.
    #[arg(hide = true, long, short = 'P', help_heading = "Resolver options")]
    pub upgrade_package: Vec<Requirement<VerbatimParsedUrl>>,

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
    #[arg(long, help_heading = "Installer options", value_hint = ValueHint::Other)]
    pub reinstall_package: Vec<PackageName>,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and limit
    /// resolutions to those present on that first index (`first-index`). This prevents "dependency
    /// confusion" attacks, whereby an attacker can upload a malicious package under the same name
    /// to an alternate index.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_INDEX_STRATEGY,
        help_heading = "Index options"
    )]
    pub index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to use
    /// the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_KEYRING_PROVIDER,
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
        env = EnvVars::UV_RESOLUTION,
        help_heading = "Resolver options"
    )]
    pub resolution: Option<ResolutionMode>,

    /// The strategy to use when considering pre-release versions.
    ///
    /// By default, uv will accept pre-releases for packages that _only_ publish pre-releases, along
    /// with first-party requirements that contain an explicit pre-release marker in the declared
    /// specifiers (`if-necessary-or-explicit`).
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_PRERELEASE,
        help_heading = "Resolver options"
    )]
    pub prerelease: Option<PrereleaseMode>,

    #[arg(long, hide = true)]
    pub pre: bool,

    /// The strategy to use when selecting multiple versions of a given package across Python
    /// versions and platforms.
    ///
    /// By default, uv will optimize for selecting the latest version of each package for each
    /// supported Python version (`requires-python`), while minimizing the number of selected
    /// versions across platforms.
    ///
    /// Under `fewest`, uv will minimize the number of selected versions for each package,
    /// preferring older versions that are compatible with a wider range of supported Python
    /// versions or platforms.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_FORK_STRATEGY,
        help_heading = "Resolver options"
    )]
    pub fork_strategy: Option<ForkStrategy>,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[arg(
        long,
        short = 'C',
        alias = "config-settings",
        help_heading = "Build options"
    )]
    pub config_setting: Option<Vec<ConfigSettingEntry>>,

    /// Settings to pass to the PEP 517 build backend for a specific package, specified as `PACKAGE:KEY=VALUE` pairs.
    #[arg(
        long,
        alias = "config-settings-package",
        help_heading = "Build options"
    )]
    pub config_setting_package: Option<Vec<ConfigSettingPackageEntry>>,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[arg(
        long,
        overrides_with("build_isolation"),
        help_heading = "Build options",
        env = EnvVars::UV_NO_BUILD_ISOLATION,
        value_parser = clap::builder::BoolishValueParser::new(),
    )]
    pub no_build_isolation: bool,

    /// Disable isolation when building source distributions for a specific package.
    ///
    /// Assumes that the packages' build dependencies specified by PEP 518 are already installed.
    #[arg(long, help_heading = "Build options", value_hint = ValueHint::Other)]
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
    /// Accepts RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`), local dates in the same format
    /// (e.g., `2006-12-02`) resolved based on your system's configured time zone, a "friendly"
    /// duration (e.g., `24 hours`, `1 week`, `30 days`), or an ISO 8601 duration (e.g., `PT24H`,
    /// `P7D`, `P30D`).
    ///
    /// Durations do not respect semantics of the local time zone and are always resolved to a fixed
    /// number of seconds assuming that a day is 24 hours (e.g., DST transitions are ignored).
    /// Calendar units such as months and years are not allowed.
    #[arg(long, env = EnvVars::UV_EXCLUDE_NEWER, help_heading = "Resolver options")]
    pub exclude_newer: Option<ExcludeNewerValue>,

    /// Limit candidate packages for specific packages to those that were uploaded prior to the
    /// given date.
    ///
    /// Accepts package-date pairs in the format `PACKAGE=DATE`, where `DATE` is an RFC 3339
    /// timestamp (e.g., `2006-12-02T02:07:43Z`), a local date in the same format (e.g.,
    /// `2006-12-02`) resolved based on your system's configured time zone, a "friendly" duration
    /// (e.g., `24 hours`, `1 week`, `30 days`), or an ISO 8601 duration (e.g., `PT24H`, `P7D`,
    /// `P30D`).
    ///
    /// Durations do not respect semantics of the local time zone and are always resolved to a fixed
    /// number of seconds assuming that a day is 24 hours (e.g., DST transitions are ignored).
    /// Calendar units such as months and years are not allowed.
    ///
    /// Can be provided multiple times for different packages.
    #[arg(long, help_heading = "Resolver options")]
    pub exclude_newer_package: Option<Vec<ExcludeNewerPackageEntry>>,

    /// The method to use when installing packages from the global cache.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    ///
    /// WARNING: The use of symlink link mode is discouraged, as they create tight coupling between
    /// the cache and the target environment. For example, clearing the cache (`uv cache clean`)
    /// will break all installed packages by way of removing the underlying source files. Use
    /// symlinks with caution.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_LINK_MODE,
        help_heading = "Installer options"
    )]
    pub link_mode: Option<uv_install_wheel::LinkMode>,

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
        help_heading = "Installer options",
        env = EnvVars::UV_COMPILE_BYTECODE,
        value_parser = clap::builder::BoolishValueParser::new(),
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
    /// standards-compliant, publishable package metadata, as opposed to using any workspace, Git,
    /// URL, or local path sources.
    #[arg(
        long,
        env = EnvVars::UV_NO_SOURCES,
        value_parser = clap::builder::BoolishValueParser::new(),
        help_heading = "Resolver options",
    )]
    pub no_sources: bool,

    /// Don't use sources from the `tool.uv.sources` table for the specified packages.
    #[arg(long, help_heading = "Resolver options", env = EnvVars::UV_NO_SOURCES_PACKAGE, value_delimiter = ' ')]
    pub no_sources_package: Vec<PackageName>,

    #[command(flatten)]
    pub build: BuildOptionsArgs,
}

#[derive(Args)]
pub struct PythonNamespace {
    #[command(subcommand)]
    pub command: PythonCommand,
}

#[derive(Subcommand)]
pub enum PythonCommand {
    /// List the available Python installations.
    ///
    /// By default, installed Python versions and the downloads for latest available patch version
    /// of each supported Python major version are shown.
    ///
    /// Use `--managed-python` to view only managed Python versions.
    ///
    /// Use `--no-managed-python` to omit managed Python versions.
    ///
    /// Use `--all-versions` to view all available patch versions.
    ///
    /// Use `--only-installed` to omit available downloads.
    #[command(alias = "ls")]
    List(PythonListArgs),

    /// Download and install Python versions.
    ///
    /// Supports CPython and PyPy. CPython distributions are downloaded from the Astral
    /// `python-build-standalone` project. PyPy distributions are downloaded from `python.org`. The
    /// available Python versions are bundled with each uv release. To install new Python versions,
    /// you may need upgrade uv.
    ///
    /// Python versions are installed into the uv Python directory, which can be retrieved with `uv
    /// python dir`.
    ///
    /// By default, Python executables are added to a directory on the path with a minor version
    /// suffix, e.g., `python3.13`. To install `python3` and `python`, use the `--default` flag. Use
    /// `uv python dir --bin` to see the target directory.
    ///
    /// Multiple Python versions may be requested.
    ///
    /// See `uv help python` to view supported request formats.
    Install(PythonInstallArgs),

    /// Upgrade installed Python versions.
    ///
    /// Upgrades versions to the latest supported patch release. Requires the `python-upgrade`
    /// preview feature.
    ///
    /// A target Python minor version to upgrade may be provided, e.g., `3.13`. Multiple versions
    /// may be provided to perform more than one upgrade.
    ///
    /// If no target version is provided, then uv will upgrade all managed CPython versions.
    ///
    /// During an upgrade, uv will not uninstall outdated patch versions.
    ///
    /// When an upgrade is performed, virtual environments created by uv will automatically
    /// use the new version. However, if the virtual environment was created before the
    /// upgrade functionality was added, it will continue to use the old Python version; to enable
    /// upgrades, the environment must be recreated.
    ///
    /// Upgrades are not yet supported for alternative implementations, like PyPy.
    Upgrade(PythonUpgradeArgs),

    /// Search for a Python installation.
    ///
    /// Displays the path to the Python executable.
    ///
    /// See `uv help python` to view supported request formats and details on discovery behavior.
    Find(PythonFindArgs),

    /// Pin to a specific Python version.
    ///
    /// Writes the pinned Python version to a `.python-version` file, which is used by other uv
    /// commands to determine the required Python version.
    ///
    /// If no version is provided, uv will look for an existing `.python-version` file and display
    /// the currently pinned version. If no `.python-version` file is found, uv will exit with an
    /// error.
    ///
    /// See `uv help python` to view supported request formats.
    Pin(PythonPinArgs),

    /// Show the uv Python installation directory.
    ///
    /// By default, Python installations are stored in the uv data directory at
    /// `$XDG_DATA_HOME/uv/python` or `$HOME/.local/share/uv/python` on Unix and
    /// `%APPDATA%\uv\data\python` on Windows.
    ///
    /// The Python installation directory may be overridden with `$UV_PYTHON_INSTALL_DIR`.
    ///
    /// To view the directory where uv installs Python executables instead, use the `--bin` flag.
    /// The Python executable directory may be overridden with `$UV_PYTHON_BIN_DIR`. Note that
    /// Python executables are only installed when preview mode is enabled.
    Dir(PythonDirArgs),

    /// Uninstall Python versions.
    Uninstall(PythonUninstallArgs),

    /// Ensure that the Python executable directory is on the `PATH`.
    ///
    /// If the Python executable directory is not present on the `PATH`, uv will attempt to add it to
    /// the relevant shell configuration files.
    ///
    /// If the shell configuration files already include a blurb to add the executable directory to
    /// the path, but the directory is not present on the `PATH`, uv will exit with an error.
    ///
    /// The Python executable directory is determined according to the XDG standard and can be
    /// retrieved with `uv python dir --bin`.
    #[command(alias = "ensurepath")]
    UpdateShell,
}

#[derive(Args)]
pub struct PythonListArgs {
    /// A Python request to filter by.
    ///
    /// See `uv help python` to view supported request formats.
    pub request: Option<String>,

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

    /// List Python downloads for all architectures.
    ///
    /// By default, only downloads for the current architecture are shown.
    #[arg(long, alias = "all_architectures")]
    pub all_arches: bool,

    /// Only show installed Python versions.
    ///
    /// By default, installed distributions and available downloads for the current platform are shown.
    #[arg(long, conflicts_with("only_downloads"))]
    pub only_installed: bool,

    /// Only show available Python downloads.
    ///
    /// By default, installed distributions and available downloads for the current platform are shown.
    #[arg(long, conflicts_with("only_installed"))]
    pub only_downloads: bool,

    /// Show the URLs of available Python downloads.
    ///
    /// By default, these display as `<download available>`.
    #[arg(long)]
    pub show_urls: bool,

    /// Select the output format.
    #[arg(long, value_enum, default_value_t = PythonListFormat::default())]
    pub output_format: PythonListFormat,

    /// URL pointing to JSON of custom Python installations.
    #[arg(long, value_hint = ValueHint::Other)]
    pub python_downloads_json_url: Option<String>,
}

#[derive(Args)]
pub struct PythonDirArgs {
    /// Show the directory into which `uv python` will install Python executables.
    ///
    /// Note that this directory is only used when installing Python with preview mode enabled.
    ///
    /// The Python executable directory is determined according to the XDG standard and is derived
    /// from the following environment variables, in order of preference:
    ///
    /// - `$UV_PYTHON_BIN_DIR`
    /// - `$XDG_BIN_HOME`
    /// - `$XDG_DATA_HOME/../bin`
    /// - `$HOME/.local/bin`
    #[arg(long, verbatim_doc_comment)]
    pub bin: bool,
}

#[derive(Args)]
pub struct PythonInstallCompileBytecodeArgs {
    /// Compile Python's standard library to bytecode after installation.
    ///
    /// By default, uv does not compile Python (`.py`) files to bytecode (`__pycache__/*.pyc`);
    /// instead, compilation is performed lazily the first time a module is imported. For use-cases
    /// in which start time is important, such as CLI applications and Docker containers, this
    /// option can be enabled to trade longer installation times and some additional disk space for
    /// faster start times.
    ///
    /// When enabled, uv will process the Python version's `stdlib` directory. It will ignore any
    /// compilation errors.
    #[arg(
        long,
        alias = "compile",
        overrides_with("no_compile_bytecode"),
        env = EnvVars::UV_COMPILE_BYTECODE,
        value_parser = clap::builder::BoolishValueParser::new(),
    )]
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
pub struct PythonInstallArgs {
    /// The directory to store the Python installation in.
    ///
    /// If provided, `UV_PYTHON_INSTALL_DIR` will need to be set for subsequent operations for uv to
    /// discover the Python installation.
    ///
    /// See `uv python dir` to view the current Python installation directory. Defaults to
    /// `~/.local/share/uv/python`.
    #[arg(long, short, env = EnvVars::UV_PYTHON_INSTALL_DIR, value_hint = ValueHint::DirPath)]
    pub install_dir: Option<PathBuf>,

    /// Install a Python executable into the `bin` directory.
    ///
    /// This is the default behavior. If this flag is provided explicitly, uv will error if the
    /// executable cannot be installed.
    ///
    /// This can also be set with `UV_PYTHON_INSTALL_BIN=1`.
    ///
    /// See `UV_PYTHON_BIN_DIR` to customize the target directory.
    #[arg(long, overrides_with("no_bin"), hide = true)]
    pub bin: bool,

    /// Do not install a Python executable into the `bin` directory.
    ///
    /// This can also be set with `UV_PYTHON_INSTALL_BIN=0`.
    #[arg(long, overrides_with("bin"), conflicts_with("default"))]
    pub no_bin: bool,

    /// Register the Python installation in the Windows registry.
    ///
    /// This is the default behavior on Windows. If this flag is provided explicitly, uv will error if the
    /// registry entry cannot be created.
    ///
    /// This can also be set with `UV_PYTHON_INSTALL_REGISTRY=1`.
    #[arg(long, overrides_with("no_registry"), hide = true)]
    pub registry: bool,

    /// Do not register the Python installation in the Windows registry.
    ///
    /// This can also be set with `UV_PYTHON_INSTALL_REGISTRY=0`.
    #[arg(long, overrides_with("registry"))]
    pub no_registry: bool,

    /// The Python version(s) to install.
    ///
    /// If not provided, the requested Python version(s) will be read from the `UV_PYTHON`
    /// environment variable then `.python-versions` or `.python-version` files. If none of the
    /// above are present, uv will check if it has installed any Python versions. If not, it will
    /// install the latest stable version of Python.
    ///
    /// See `uv help python` to view supported request formats.
    #[arg(env = EnvVars::UV_PYTHON)]
    pub targets: Vec<String>,

    /// Set the URL to use as the source for downloading Python installations.
    ///
    /// The provided URL will replace
    /// `https://github.com/astral-sh/python-build-standalone/releases/download` in, e.g.,
    /// `https://github.com/astral-sh/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-aarch64-apple-darwin-install_only.tar.gz`.
    ///
    /// Distributions can be read from a local directory by using the `file://` URL scheme.
    #[arg(long, value_hint = ValueHint::Url)]
    pub mirror: Option<String>,

    /// Set the URL to use as the source for downloading PyPy installations.
    ///
    /// The provided URL will replace `https://downloads.python.org/pypy` in, e.g.,
    /// `https://downloads.python.org/pypy/pypy3.8-v7.3.7-osx64.tar.bz2`.
    ///
    /// Distributions can be read from a local directory by using the `file://` URL scheme.
    #[arg(long, value_hint = ValueHint::Url)]
    pub pypy_mirror: Option<String>,

    /// URL pointing to JSON of custom Python installations.
    #[arg(long, value_hint = ValueHint::Other)]
    pub python_downloads_json_url: Option<String>,

    /// Reinstall the requested Python version, if it's already installed.
    ///
    /// By default, uv will exit successfully if the version is already
    /// installed.
    #[arg(long, short)]
    pub reinstall: bool,

    /// Replace existing Python executables during installation.
    ///
    /// By default, uv will refuse to replace executables that it does not manage.
    ///
    /// Implies `--reinstall`.
    #[arg(long, short)]
    pub force: bool,

    /// Upgrade existing Python installations to the latest patch version.
    ///
    /// By default, uv will not upgrade already-installed Python versions to newer patch releases.
    /// With `--upgrade`, uv will upgrade to the latest available patch version for the specified
    /// minor version(s).
    ///
    /// If the requested versions are not yet installed, uv will install them.
    ///
    /// This option is only supported for minor version requests, e.g., `3.12`; uv will exit with an
    /// error if a patch version, e.g., `3.12.2`, is requested.
    #[arg(long, short = 'U')]
    pub upgrade: bool,

    /// Use as the default Python version.
    ///
    /// By default, only a `python{major}.{minor}` executable is installed, e.g., `python3.10`. When
    /// the `--default` flag is used, `python{major}`, e.g., `python3`, and `python` executables are
    /// also installed.
    ///
    /// Alternative Python variants will still include their tag. For example, installing
    /// 3.13+freethreaded with `--default` will include `python3t` and `pythont` instead of
    /// `python3` and `python`.
    ///
    /// If multiple Python versions are requested, uv will exit with an error.
    #[arg(long, conflicts_with("no_bin"))]
    pub default: bool,

    #[command(flatten)]
    pub compile_bytecode: PythonInstallCompileBytecodeArgs,
}

impl PythonInstallArgs {
    #[must_use]
    pub fn install_mirrors(&self) -> PythonInstallMirrors {
        PythonInstallMirrors {
            python_install_mirror: self.mirror.clone(),
            pypy_install_mirror: self.pypy_mirror.clone(),
            python_downloads_json_url: self.python_downloads_json_url.clone(),
        }
    }
}

#[derive(Args)]
pub struct PythonUpgradeArgs {
    /// The directory Python installations are stored in.
    ///
    /// If provided, `UV_PYTHON_INSTALL_DIR` will need to be set for subsequent operations for uv to
    /// discover the Python installation.
    ///
    /// See `uv python dir` to view the current Python installation directory. Defaults to
    /// `~/.local/share/uv/python`.
    #[arg(long, short, env = EnvVars::UV_PYTHON_INSTALL_DIR, value_hint = ValueHint::DirPath)]
    pub install_dir: Option<PathBuf>,

    /// The Python minor version(s) to upgrade.
    ///
    /// If no target version is provided, then uv will upgrade all managed CPython versions.
    #[arg(env = EnvVars::UV_PYTHON)]
    pub targets: Vec<String>,

    /// Set the URL to use as the source for downloading Python installations.
    ///
    /// The provided URL will replace
    /// `https://github.com/astral-sh/python-build-standalone/releases/download` in, e.g.,
    /// `https://github.com/astral-sh/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-aarch64-apple-darwin-install_only.tar.gz`.
    ///
    /// Distributions can be read from a local directory by using the `file://` URL scheme.
    #[arg(long, value_hint = ValueHint::Url)]
    pub mirror: Option<String>,

    /// Set the URL to use as the source for downloading PyPy installations.
    ///
    /// The provided URL will replace `https://downloads.python.org/pypy` in, e.g.,
    /// `https://downloads.python.org/pypy/pypy3.8-v7.3.7-osx64.tar.bz2`.
    ///
    /// Distributions can be read from a local directory by using the `file://` URL scheme.
    #[arg(long, value_hint = ValueHint::Url)]
    pub pypy_mirror: Option<String>,

    /// Reinstall the latest Python patch, if it's already installed.
    ///
    /// By default, uv will exit successfully if the latest patch is already
    /// installed.
    #[arg(long, short)]
    pub reinstall: bool,

    /// URL pointing to JSON of custom Python installations.
    #[arg(long, value_hint = ValueHint::Other)]
    pub python_downloads_json_url: Option<String>,

    #[command(flatten)]
    pub compile_bytecode: PythonInstallCompileBytecodeArgs,
}

impl PythonUpgradeArgs {
    #[must_use]
    pub fn install_mirrors(&self) -> PythonInstallMirrors {
        PythonInstallMirrors {
            python_install_mirror: self.mirror.clone(),
            pypy_install_mirror: self.pypy_mirror.clone(),
            python_downloads_json_url: self.python_downloads_json_url.clone(),
        }
    }
}

#[derive(Args)]
pub struct PythonUninstallArgs {
    /// The directory where the Python was installed.
    #[arg(long, short, env = EnvVars::UV_PYTHON_INSTALL_DIR, value_hint = ValueHint::DirPath)]
    pub install_dir: Option<PathBuf>,

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

    /// Only find system Python interpreters.
    ///
    /// By default, uv will report the first Python interpreter it would use, including those in an
    /// active virtual environment or a virtual environment in the current working directory or any
    /// parent directory.
    ///
    /// The `--system` option instructs uv to skip virtual environment Python interpreters and
    /// restrict its search to the system path.
    #[arg(
        long,
        env = EnvVars::UV_SYSTEM_PYTHON,
        value_parser = clap::builder::BoolishValueParser::new(),
        overrides_with("no_system")
    )]
    pub system: bool,

    #[arg(long, overrides_with("system"), hide = true)]
    pub no_system: bool,

    /// Find the environment for a Python script, rather than the current project.
    #[arg(
        long,
        conflicts_with = "request",
        conflicts_with = "no_project",
        conflicts_with = "system",
        conflicts_with = "no_system",
        value_hint = ValueHint::FilePath,
    )]
    pub script: Option<PathBuf>,

    /// Show the Python version that would be used instead of the path to the interpreter.
    #[arg(long)]
    pub show_version: bool,

    /// Resolve symlinks in the output path.
    ///
    /// When enabled, the output path will be canonicalized, resolving any symlinks.
    #[arg(long)]
    pub resolve_links: bool,

    /// URL pointing to JSON of custom Python installations.
    #[arg(long, value_hint = ValueHint::Other)]
    pub python_downloads_json_url: Option<String>,
}

#[derive(Args)]
pub struct PythonPinArgs {
    /// The Python version request.
    ///
    /// uv supports more formats than other tools that read `.python-version` files, i.e., `pyenv`.
    /// If compatibility with those tools is needed, only use version numbers instead of complex
    /// requests such as `cpython@3.10`.
    ///
    /// If no request is provided, the currently pinned version will be shown.
    ///
    /// See `uv help python` to view supported request formats.
    pub request: Option<String>,

    /// Write the resolved Python interpreter path instead of the request.
    ///
    /// Ensures that the exact same interpreter is used.
    ///
    /// This option is usually not safe to use when committing the `.python-version` file to version
    /// control.
    #[arg(long, overrides_with("resolved"))]
    pub resolved: bool,

    #[arg(long, overrides_with("no_resolved"), hide = true)]
    pub no_resolved: bool,

    /// Avoid validating the Python pin is compatible with the project or workspace.
    ///
    /// By default, a project or workspace is discovered in the current directory or any parent
    /// directory. If a workspace is found, the Python pin is validated against the workspace's
    /// `requires-python` constraint.
    #[arg(long, alias = "no-workspace")]
    pub no_project: bool,

    /// Update the global Python version pin.
    ///
    /// Writes the pinned Python version to a `.python-version` file in the uv user configuration
    /// directory: `XDG_CONFIG_HOME/uv` on Linux/macOS and `%APPDATA%/uv` on Windows.
    ///
    /// When a local Python version pin is not found in the working directory or an ancestor
    /// directory, this version will be used instead.
    #[arg(long)]
    pub global: bool,

    /// Remove the Python version pin.
    #[arg(long, conflicts_with = "request", conflicts_with = "resolved")]
    pub rm: bool,
}

#[derive(Args)]
pub struct AuthLogoutArgs {
    /// The domain or URL of the service to logout from.
    pub service: Service,

    /// The username to logout.
    #[arg(long, short, value_hint = ValueHint::Other)]
    pub username: Option<String>,

    /// The keyring provider to use for storage of credentials.
    ///
    /// Only `--keyring-provider native` is supported for `logout`, which uses the system keyring
    /// via an integration built into uv.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_KEYRING_PROVIDER,
    )]
    pub keyring_provider: Option<KeyringProviderType>,
}

#[derive(Args)]
pub struct AuthLoginArgs {
    /// The domain or URL of the service to log into.
    #[arg(value_hint = ValueHint::Url)]
    pub service: Service,

    /// The username to use for the service.
    #[arg(long, short, conflicts_with = "token", value_hint = ValueHint::Other)]
    pub username: Option<String>,

    /// The password to use for the service.
    ///
    /// Use `-` to read the password from stdin.
    #[arg(long, conflicts_with = "token", value_hint = ValueHint::Other)]
    pub password: Option<String>,

    /// The token to use for the service.
    ///
    /// The username will be set to `__token__`.
    ///
    /// Use `-` to read the token from stdin.
    #[arg(long, short, conflicts_with = "username", conflicts_with = "password", value_hint = ValueHint::Other)]
    pub token: Option<String>,

    /// The keyring provider to use for storage of credentials.
    ///
    /// Only `--keyring-provider native` is supported for `login`, which uses the system keyring via
    /// an integration built into uv.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_KEYRING_PROVIDER,
    )]
    pub keyring_provider: Option<KeyringProviderType>,
}

#[derive(Args)]
pub struct AuthTokenArgs {
    /// The domain or URL of the service to lookup.
    #[arg(value_hint = ValueHint::Url)]
    pub service: Service,

    /// The username to lookup.
    #[arg(long, short, value_hint = ValueHint::Other)]
    pub username: Option<String>,

    /// The keyring provider to use for reading credentials.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_KEYRING_PROVIDER,
    )]
    pub keyring_provider: Option<KeyringProviderType>,
}

#[derive(Args)]
pub struct AuthDirArgs {
    /// The domain or URL of the service to lookup.
    #[arg(value_hint = ValueHint::Url)]
    pub service: Option<Service>,
}

#[derive(Args)]
pub struct AuthHelperArgs {
    #[command(subcommand)]
    pub command: AuthHelperCommand,

    /// The credential helper protocol to use
    #[arg(long, value_enum, required = true)]
    pub protocol: AuthHelperProtocol,
}

/// Credential helper protocols supported by uv
#[derive(Debug, Copy, Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum AuthHelperProtocol {
    /// Bazel credential helper protocol as described in [the
    /// spec](https://github.com/bazelbuild/proposals/blob/main/designs/2022-06-07-bazel-credential-helpers.md)
    Bazel,
}

#[derive(Subcommand)]
pub enum AuthHelperCommand {
    /// Retrieve credentials for a URI
    Get,
}

#[derive(Args)]
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

    #[arg(long, short, action = clap::ArgAction::Count, conflicts_with = "verbose", hide = true)]
    pub quiet: u8,
    #[arg(long, short, action = clap::ArgAction::Count, conflicts_with = "quiet", hide = true)]
    pub verbose: u8,
    #[arg(long, conflicts_with = "no_color", hide = true)]
    pub color: Option<ColorChoice>,
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
pub struct IndexArgs {
    /// The URLs to use when resolving dependencies, in addition to the default index.
    ///
    /// Accepts either a repository compliant with PEP 503 (the simple repository API), or a local
    /// directory laid out in the same format.
    ///
    /// All indexes provided via this flag take priority over the index specified by
    /// `--default-index` (which defaults to PyPI). When multiple `--index` flags are provided,
    /// earlier values take priority.
    ///
    /// Index names are not supported as values. Relative paths must be disambiguated from index
    /// names with `./` or `../` on Unix or `.\\`, `..\\`, `./` or `../` on Windows.
    //
    // The nested Vec structure (`Vec<Vec<Maybe<Index>>>`) is required for clap's
    // value parsing mechanism, which processes one value at a time, in order to handle
    // `UV_INDEX` the same way pip handles `PIP_EXTRA_INDEX_URL`.
    #[arg(
        long,
        env = EnvVars::UV_INDEX,
        hide_env_values = true,
        value_parser = parse_indices,
        help_heading = "Index options"
    )]
    pub index: Option<Vec<Vec<Maybe<Index>>>>,

    /// The URL of the default package index (by default: <https://pypi.org/simple>).
    ///
    /// Accepts either a repository compliant with PEP 503 (the simple repository API), or a local
    /// directory laid out in the same format.
    ///
    /// The index given by this flag is given lower priority than all other indexes specified via
    /// the `--index` flag.
    #[arg(
        long,
        env = EnvVars::UV_DEFAULT_INDEX,
        hide_env_values = true,
        value_parser = parse_default_index,
        help_heading = "Index options"
    )]
    pub default_index: Option<Maybe<Index>>,

    /// (Deprecated: use `--default-index` instead) The URL of the Python package index (by default:
    /// <https://pypi.org/simple>).
    ///
    /// Accepts either a repository compliant with PEP 503 (the simple repository API), or a local
    /// directory laid out in the same format.
    ///
    /// The index given by this flag is given lower priority than all other indexes specified via
    /// the `--extra-index-url` flag.
    #[arg(
        long,
        short,
        env = EnvVars::UV_INDEX_URL,
        hide_env_values = true,
        value_parser = parse_index_url,
        help_heading = "Index options"
    )]
    pub index_url: Option<Maybe<PipIndex>>,

    /// (Deprecated: use `--index` instead) Extra URLs of package indexes to use, in addition to
    /// `--index-url`.
    ///
    /// Accepts either a repository compliant with PEP 503 (the simple repository API), or a local
    /// directory laid out in the same format.
    ///
    /// All indexes provided via this flag take priority over the index specified by `--index-url`
    /// (which defaults to PyPI). When multiple `--extra-index-url` flags are provided, earlier
    /// values take priority.
    #[arg(
        long,
        env = EnvVars::UV_EXTRA_INDEX_URL,
        hide_env_values = true,
        value_delimiter = ' ',
        value_parser = parse_extra_index_url,
        help_heading = "Index options"
    )]
    pub extra_index_url: Option<Vec<Maybe<PipExtraIndex>>>,

    /// Locations to search for candidate distributions, in addition to those found in the registry
    /// indexes.
    ///
    /// If a path, the target must be a directory that contains packages as wheel files (`.whl`) or
    /// source distributions (e.g., `.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files adhering to the
    /// formats described above.
    #[arg(
        long,
        short,
        env = EnvVars::UV_FIND_LINKS,
        hide_env_values = true,
        value_delimiter = ',',
        value_parser = parse_find_links,
        help_heading = "Index options"
    )]
    pub find_links: Option<Vec<Maybe<PipFindLinks>>>,

    /// Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those
    /// provided via `--find-links`.
    #[arg(long, help_heading = "Index options")]
    pub no_index: bool,
}

#[derive(Args)]
pub struct RefreshArgs {
    /// Refresh all cached data.
    #[arg(long, overrides_with("no_refresh"), help_heading = "Cache options")]
    pub refresh: bool,

    #[arg(
        long,
        overrides_with("refresh"),
        hide = true,
        help_heading = "Cache options"
    )]
    pub no_refresh: bool,

    /// Refresh cached data for a specific package.
    #[arg(long, help_heading = "Cache options", value_hint = ValueHint::Other)]
    pub refresh_package: Vec<PackageName>,
}

#[derive(Args)]
pub struct BuildOptionsArgs {
    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary Python code. The cached wheels of
    /// already-built source distributions will be reused, but operations that require building
    /// distributions will exit with an error.
    #[arg(
        long,
        env = EnvVars::UV_NO_BUILD,
        overrides_with("build"),
        value_parser = clap::builder::BoolishValueParser::new(),
        help_heading = "Build options",
    )]
    pub no_build: bool,

    #[arg(
        long,
        overrides_with("no_build"),
        hide = true,
        help_heading = "Build options"
    )]
    pub build: bool,

    /// Don't build source distributions for a specific package.
    #[arg(
        long,
        help_heading = "Build options",
        env = EnvVars::UV_NO_BUILD_PACKAGE,
        value_delimiter = ' ',
        value_hint = ValueHint::Other,
    )]
    pub no_build_package: Vec<PackageName>,

    /// Don't install pre-built wheels.
    ///
    /// The given packages will be built and installed from source. The resolver will still use
    /// pre-built wheels to extract package metadata, if available.
    #[arg(
        long,
        env = EnvVars::UV_NO_BINARY,
        overrides_with("binary"),
        value_parser = clap::builder::BoolishValueParser::new(),
        help_heading = "Build options"
    )]
    pub no_binary: bool,

    #[arg(
        long,
        overrides_with("no_binary"),
        hide = true,
        help_heading = "Build options"
    )]
    pub binary: bool,

    /// Don't install pre-built wheels for a specific package.
    #[arg(
        long,
        help_heading = "Build options",
        env = EnvVars::UV_NO_BINARY_PACKAGE,
        value_delimiter = ' ',
        value_hint = ValueHint::Other,
    )]
    pub no_binary_package: Vec<PackageName>,
}

/// Arguments that are used by commands that need to install (but not resolve) packages.
#[derive(Args)]
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
    #[arg(long, help_heading = "Installer options", value_hint = ValueHint::Other)]
    pub reinstall_package: Vec<PackageName>,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and limit
    /// resolutions to those present on that first index (`first-index`). This prevents "dependency
    /// confusion" attacks, whereby an attacker can upload a malicious package under the same name
    /// to an alternate index.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_INDEX_STRATEGY,
        help_heading = "Index options"
    )]
    pub index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to use
    /// the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_KEYRING_PROVIDER,
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

    /// Settings to pass to the PEP 517 build backend for a specific package, specified as `PACKAGE:KEY=VALUE` pairs.
    #[arg(
        long,
        alias = "config-settings-package",
        help_heading = "Build options"
    )]
    pub config_settings_package: Option<Vec<ConfigSettingPackageEntry>>,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[arg(
        long,
        overrides_with("build_isolation"),
        help_heading = "Build options",
        env = EnvVars::UV_NO_BUILD_ISOLATION,
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
    /// Accepts RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`), local dates in the same format
    /// (e.g., `2006-12-02`) resolved based on your system's configured time zone, a "friendly"
    /// duration (e.g., `24 hours`, `1 week`, `30 days`), or an ISO 8601 duration (e.g., `PT24H`,
    /// `P7D`, `P30D`).
    ///
    /// Durations do not respect semantics of the local time zone and are always resolved to a fixed
    /// number of seconds assuming that a day is 24 hours (e.g., DST transitions are ignored).
    /// Calendar units such as months and years are not allowed.
    #[arg(long, env = EnvVars::UV_EXCLUDE_NEWER, help_heading = "Resolver options")]
    pub exclude_newer: Option<ExcludeNewerValue>,

    /// Limit candidate packages for specific packages to those that were uploaded prior to the
    /// given date.
    ///
    /// Accepts package-date pairs in the format `PACKAGE=DATE`, where `DATE` is an RFC 3339
    /// timestamp (e.g., `2006-12-02T02:07:43Z`), a local date in the same format (e.g.,
    /// `2006-12-02`) resolved based on your system's configured time zone, a "friendly" duration
    /// (e.g., `24 hours`, `1 week`, `30 days`), or an ISO 8601 duration (e.g., `PT24H`, `P7D`,
    /// `P30D`).
    ///
    /// Durations do not respect semantics of the local time zone and are always resolved to a fixed
    /// number of seconds assuming that a day is 24 hours (e.g., DST transitions are ignored).
    /// Calendar units such as months and years are not allowed.
    ///
    /// Can be provided multiple times for different packages.
    #[arg(long, help_heading = "Resolver options")]
    pub exclude_newer_package: Option<Vec<ExcludeNewerPackageEntry>>,

    /// The method to use when installing packages from the global cache.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    ///
    /// WARNING: The use of symlink link mode is discouraged, as they create tight coupling between
    /// the cache and the target environment. For example, clearing the cache (`uv cache clean`)
    /// will break all installed packages by way of removing the underlying source files. Use
    /// symlinks with caution.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_LINK_MODE,
        help_heading = "Installer options"
    )]
    pub link_mode: Option<uv_install_wheel::LinkMode>,

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
        help_heading = "Installer options",
        env = EnvVars::UV_COMPILE_BYTECODE,
        value_parser = clap::builder::BoolishValueParser::new(),
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
    /// standards-compliant, publishable package metadata, as opposed to using any workspace, Git,
    /// URL, or local path sources.
    #[arg(
        long,
        env = EnvVars::UV_NO_SOURCES,
        value_parser = clap::builder::BoolishValueParser::new(),
        help_heading = "Resolver options"
    )]
    pub no_sources: bool,

    /// Don't use sources from the `tool.uv.sources` table for the specified packages.
    #[arg(long, help_heading = "Resolver options", env = EnvVars::UV_NO_SOURCES_PACKAGE, value_delimiter = ' ')]
    pub no_sources_package: Vec<PackageName>,
}

/// Arguments that are used by commands that need to resolve (but not install) packages.
#[derive(Args)]
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
    /// By default, uv will stop at the first index on which a given package is available, and limit
    /// resolutions to those present on that first index (`first-index`). This prevents "dependency
    /// confusion" attacks, whereby an attacker can upload a malicious package under the same name
    /// to an alternate index.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_INDEX_STRATEGY,
        help_heading = "Index options"
    )]
    pub index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to use
    /// the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_KEYRING_PROVIDER,
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
        env = EnvVars::UV_RESOLUTION,
        help_heading = "Resolver options"
    )]
    pub resolution: Option<ResolutionMode>,

    /// The strategy to use when considering pre-release versions.
    ///
    /// By default, uv will accept pre-releases for packages that _only_ publish pre-releases, along
    /// with first-party requirements that contain an explicit pre-release marker in the declared
    /// specifiers (`if-necessary-or-explicit`).
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_PRERELEASE,
        help_heading = "Resolver options"
    )]
    pub prerelease: Option<PrereleaseMode>,

    #[arg(long, hide = true, help_heading = "Resolver options")]
    pub pre: bool,

    /// The strategy to use when selecting multiple versions of a given package across Python
    /// versions and platforms.
    ///
    /// By default, uv will optimize for selecting the latest version of each package for each
    /// supported Python version (`requires-python`), while minimizing the number of selected
    /// versions across platforms.
    ///
    /// Under `fewest`, uv will minimize the number of selected versions for each package,
    /// preferring older versions that are compatible with a wider range of supported Python
    /// versions or platforms.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_FORK_STRATEGY,
        help_heading = "Resolver options"
    )]
    pub fork_strategy: Option<ForkStrategy>,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[arg(
        long,
        short = 'C',
        alias = "config-settings",
        help_heading = "Build options"
    )]
    pub config_setting: Option<Vec<ConfigSettingEntry>>,

    /// Settings to pass to the PEP 517 build backend for a specific package, specified as `PACKAGE:KEY=VALUE` pairs.
    #[arg(
        long,
        alias = "config-settings-package",
        help_heading = "Build options"
    )]
    pub config_settings_package: Option<Vec<ConfigSettingPackageEntry>>,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[arg(
        long,
        overrides_with("build_isolation"),
        help_heading = "Build options",
        env = EnvVars::UV_NO_BUILD_ISOLATION,
        value_parser = clap::builder::BoolishValueParser::new(),
    )]
    pub no_build_isolation: bool,

    /// Disable isolation when building source distributions for a specific package.
    ///
    /// Assumes that the packages' build dependencies specified by PEP 518 are already installed.
    #[arg(long, help_heading = "Build options", value_hint = ValueHint::Other)]
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
    /// Accepts RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`), local dates in the same format
    /// (e.g., `2006-12-02`) resolved based on your system's configured time zone, a "friendly"
    /// duration (e.g., `24 hours`, `1 week`, `30 days`), or an ISO 8601 duration (e.g., `PT24H`,
    /// `P7D`, `P30D`).
    ///
    /// Durations do not respect semantics of the local time zone and are always resolved to a fixed
    /// number of seconds assuming that a day is 24 hours (e.g., DST transitions are ignored).
    /// Calendar units such as months and years are not allowed.
    #[arg(long, env = EnvVars::UV_EXCLUDE_NEWER, help_heading = "Resolver options")]
    pub exclude_newer: Option<ExcludeNewerValue>,

    /// Limit candidate packages for specific packages to those that were uploaded prior to the
    /// given date.
    ///
    /// Accepts package-date pairs in the format `PACKAGE=DATE`, where `DATE` is an RFC 3339
    /// timestamp (e.g., `2006-12-02T02:07:43Z`), a local date in the same format (e.g.,
    /// `2006-12-02`) resolved based on your system's configured time zone, a "friendly" duration
    /// (e.g., `24 hours`, `1 week`, `30 days`), or an ISO 8601 duration (e.g., `PT24H`, `P7D`,
    /// `P30D`).
    ///
    /// Durations do not respect semantics of the local time zone and are always resolved to a fixed
    /// number of seconds assuming that a day is 24 hours (e.g., DST transitions are ignored).
    /// Calendar units such as months and years are not allowed.
    ///
    /// Can be provided multiple times for different packages.
    #[arg(long, help_heading = "Resolver options")]
    pub exclude_newer_package: Option<Vec<ExcludeNewerPackageEntry>>,

    /// The method to use when installing packages from the global cache.
    ///
    /// This option is only used when building source distributions.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    ///
    /// WARNING: The use of symlink link mode is discouraged, as they create tight coupling between
    /// the cache and the target environment. For example, clearing the cache (`uv cache clean`)
    /// will break all installed packages by way of removing the underlying source files. Use
    /// symlinks with caution.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_LINK_MODE,
        help_heading = "Installer options"
    )]
    pub link_mode: Option<uv_install_wheel::LinkMode>,

    /// Ignore the `tool.uv.sources` table when resolving dependencies. Used to lock against the
    /// standards-compliant, publishable package metadata, as opposed to using any workspace, Git,
    /// URL, or local path sources.
    #[arg(
        long,
        env = EnvVars::UV_NO_SOURCES,
        value_parser = clap::builder::BoolishValueParser::new(),
        help_heading = "Resolver options",
    )]
    pub no_sources: bool,

    /// Don't use sources from the `tool.uv.sources` table for the specified packages.
    #[arg(long, help_heading = "Resolver options", env = EnvVars::UV_NO_SOURCES_PACKAGE, value_delimiter = ' ')]
    pub no_sources_package: Vec<PackageName>,
}

/// Arguments that are used by commands that need to resolve and install packages.
#[derive(Args)]
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

    /// Allow upgrades for a specific package, ignoring pinned versions in any existing output file.
    /// Implies `--refresh-package`.
    #[arg(long, short = 'P', help_heading = "Resolver options", value_hint = ValueHint::Other)]
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
    #[arg(long, help_heading = "Installer options", value_hint = ValueHint::Other)]
    pub reinstall_package: Vec<PackageName>,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and limit
    /// resolutions to those present on that first index (`first-index`). This prevents "dependency
    /// confusion" attacks, whereby an attacker can upload a malicious package under the same name
    /// to an alternate index.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_INDEX_STRATEGY,
        help_heading = "Index options"
    )]
    pub index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to use
    /// the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_KEYRING_PROVIDER,
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
        env = EnvVars::UV_RESOLUTION,
        help_heading = "Resolver options"
    )]
    pub resolution: Option<ResolutionMode>,

    /// The strategy to use when considering pre-release versions.
    ///
    /// By default, uv will accept pre-releases for packages that _only_ publish pre-releases, along
    /// with first-party requirements that contain an explicit pre-release marker in the declared
    /// specifiers (`if-necessary-or-explicit`).
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_PRERELEASE,
        help_heading = "Resolver options"
    )]
    pub prerelease: Option<PrereleaseMode>,

    #[arg(long, hide = true)]
    pub pre: bool,

    /// The strategy to use when selecting multiple versions of a given package across Python
    /// versions and platforms.
    ///
    /// By default, uv will optimize for selecting the latest version of each package for each
    /// supported Python version (`requires-python`), while minimizing the number of selected
    /// versions across platforms.
    ///
    /// Under `fewest`, uv will minimize the number of selected versions for each package,
    /// preferring older versions that are compatible with a wider range of supported Python
    /// versions or platforms.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_FORK_STRATEGY,
        help_heading = "Resolver options"
    )]
    pub fork_strategy: Option<ForkStrategy>,

    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
    #[arg(
        long,
        short = 'C',
        alias = "config-settings",
        help_heading = "Build options",
        value_hint = ValueHint::Other,
    )]
    pub config_setting: Option<Vec<ConfigSettingEntry>>,

    /// Settings to pass to the PEP 517 build backend for a specific package, specified as `PACKAGE:KEY=VALUE` pairs.
    #[arg(
        long,
        alias = "config-settings-package",
        help_heading = "Build options",
        value_hint = ValueHint::Other,
    )]
    pub config_settings_package: Option<Vec<ConfigSettingPackageEntry>>,

    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by PEP 518 are already installed.
    #[arg(
        long,
        overrides_with("build_isolation"),
        help_heading = "Build options",
        env = EnvVars::UV_NO_BUILD_ISOLATION,
        value_parser = clap::builder::BoolishValueParser::new(),
    )]
    pub no_build_isolation: bool,

    /// Disable isolation when building source distributions for a specific package.
    ///
    /// Assumes that the packages' build dependencies specified by PEP 518 are already installed.
    #[arg(long, help_heading = "Build options", value_hint = ValueHint::Other)]
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
    /// Accepts RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`), local dates in the same format
    /// (e.g., `2006-12-02`) resolved based on your system's configured time zone, a "friendly"
    /// duration (e.g., `24 hours`, `1 week`, `30 days`), or an ISO 8601 duration (e.g., `PT24H`,
    /// `P7D`, `P30D`).
    ///
    /// Durations do not respect semantics of the local time zone and are always resolved to a fixed
    /// number of seconds assuming that a day is 24 hours (e.g., DST transitions are ignored).
    /// Calendar units such as months and years are not allowed.
    #[arg(
        long,
        env = EnvVars::UV_EXCLUDE_NEWER,
        help_heading = "Resolver options",
        value_hint = ValueHint::Other,
    )]
    pub exclude_newer: Option<ExcludeNewerValue>,

    /// Limit candidate packages for specific packages to those that were uploaded prior to the
    /// given date.
    ///
    /// Accepts package-date pairs in the format `PACKAGE=DATE`, where `DATE` is an RFC 3339
    /// timestamp (e.g., `2006-12-02T02:07:43Z`), a local date in the same format (e.g.,
    /// `2006-12-02`) resolved based on your system's configured time zone, a "friendly" duration
    /// (e.g., `24 hours`, `1 week`, `30 days`), or an ISO 8601 duration (e.g., `PT24H`, `P7D`,
    /// `P30D`).
    ///
    /// Durations do not respect semantics of the local time zone and are always resolved to a fixed
    /// number of seconds assuming that a day is 24 hours (e.g., DST transitions are ignored).
    /// Calendar units such as months and years are not allowed.
    ///
    /// Can be provided multiple times for different packages.
    #[arg(long, help_heading = "Resolver options", value_hint = ValueHint::Other)]
    pub exclude_newer_package: Option<Vec<ExcludeNewerPackageEntry>>,

    /// The method to use when installing packages from the global cache.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    ///
    /// WARNING: The use of symlink link mode is discouraged, as they create tight coupling between
    /// the cache and the target environment. For example, clearing the cache (`uv cache clean`)
    /// will break all installed packages by way of removing the underlying source files. Use
    /// symlinks with caution.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_LINK_MODE,
        help_heading = "Installer options"
    )]
    pub link_mode: Option<uv_install_wheel::LinkMode>,

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
        help_heading = "Installer options",
        env = EnvVars::UV_COMPILE_BYTECODE,
        value_parser = clap::builder::BoolishValueParser::new(),
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
    /// standards-compliant, publishable package metadata, as opposed to using any workspace, Git,
    /// URL, or local path sources.
    #[arg(
        long,
        env = EnvVars::UV_NO_SOURCES,
        value_parser = clap::builder::BoolishValueParser::new(),
        help_heading = "Resolver options",
    )]
    pub no_sources: bool,

    /// Don't use sources from the `tool.uv.sources` table for the specified packages.
    #[arg(long, help_heading = "Resolver options", env = EnvVars::UV_NO_SOURCES_PACKAGE, value_delimiter = ' ')]
    pub no_sources_package: Vec<PackageName>,
}

/// Arguments that are used by commands that need to fetch from the Simple API.
#[derive(Args)]
pub struct FetchArgs {
    #[command(flatten)]
    pub index_args: IndexArgs,

    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and limit
    /// resolutions to those present on that first index (`first-index`). This prevents "dependency
    /// confusion" attacks, whereby an attacker can upload a malicious package under the same name
    /// to an alternate index.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_INDEX_STRATEGY,
        help_heading = "Index options"
    )]
    pub index_strategy: Option<IndexStrategy>,

    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to use
    /// the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(
        long,
        value_enum,
        env = EnvVars::UV_KEYRING_PROVIDER,
        help_heading = "Index options"
    )]
    pub keyring_provider: Option<KeyringProviderType>,

    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`), local dates in the same format
    /// (e.g., `2006-12-02`) resolved based on your system's configured time zone, a "friendly"
    /// duration (e.g., `24 hours`, `1 week`, `30 days`), or an ISO 8601 duration (e.g., `PT24H`,
    /// `P7D`, `P30D`).
    ///
    /// Durations do not respect semantics of the local time zone and are always resolved to a fixed
    /// number of seconds assuming that a day is 24 hours (e.g., DST transitions are ignored).
    /// Calendar units such as months and years are not allowed.
    #[arg(long, env = EnvVars::UV_EXCLUDE_NEWER, help_heading = "Resolver options")]
    pub exclude_newer: Option<ExcludeNewerValue>,
}

#[derive(Args)]
pub struct DisplayTreeArgs {
    /// Maximum display depth of the dependency tree
    #[arg(long, short, default_value_t = 255)]
    pub depth: u8,

    /// Prune the given package from the display of the dependency tree.
    #[arg(long, value_hint = ValueHint::Other)]
    pub prune: Vec<PackageName>,

    /// Display only the specified packages.
    #[arg(long, value_hint = ValueHint::Other)]
    pub package: Vec<PackageName>,

    /// Do not de-duplicate repeated dependencies. Usually, when a package has already displayed its
    /// dependencies, further occurrences will not re-display its dependencies, and will include a
    /// (*) to indicate it has already been shown. This flag will cause those duplicates to be
    /// repeated.
    #[arg(long)]
    pub no_dedupe: bool,

    /// Show the reverse dependencies for the given package. This flag will invert the tree and
    /// display the packages that depend on the given package.
    #[arg(long, alias = "reverse")]
    pub invert: bool,

    /// Show the latest available version of each package in the tree.
    #[arg(long)]
    pub outdated: bool,

    /// Show compressed wheel sizes for packages in the tree.
    #[arg(long)]
    pub show_sizes: bool,
}

#[derive(Args, Debug)]
pub struct PublishArgs {
    /// Paths to the files to upload. Accepts glob expressions.
    ///
    /// Defaults to the `dist` directory. Selects only wheels and source distributions
    /// and their attestations, while ignoring other files.
    #[arg(default_value = "dist/*", value_hint = ValueHint::FilePath)]
    pub files: Vec<String>,

    /// The name of an index in the configuration to use for publishing.
    ///
    /// The index must have a `publish-url` setting, for example:
    ///
    /// ```toml
    /// [[tool.uv.index]]
    /// name = "pypi"
    /// url = "https://pypi.org/simple"
    /// publish-url = "https://upload.pypi.org/legacy/"
    /// ```
    ///
    /// The index `url` will be used to check for existing files to skip duplicate uploads.
    ///
    /// With these settings, the following two calls are equivalent:
    ///
    /// ```shell
    /// uv publish --index pypi
    /// uv publish --publish-url https://upload.pypi.org/legacy/ --check-url https://pypi.org/simple
    /// ```
    #[arg(
        long,
        verbatim_doc_comment,
        env = EnvVars::UV_PUBLISH_INDEX,
        conflicts_with = "publish_url",
        conflicts_with = "check_url",
        value_hint = ValueHint::Other,
    )]
    pub index: Option<String>,

    /// The username for the upload.
    #[arg(
        short,
        long,
        env = EnvVars::UV_PUBLISH_USERNAME,
        hide_env_values = true,
        value_hint = ValueHint::Other
    )]
    pub username: Option<String>,

    /// The password for the upload.
    #[arg(
        short,
        long,
        env = EnvVars::UV_PUBLISH_PASSWORD,
        hide_env_values = true,
        value_hint = ValueHint::Other
    )]
    pub password: Option<String>,

    /// The token for the upload.
    ///
    /// Using a token is equivalent to passing `__token__` as `--username` and the token as
    /// `--password` password.
    #[arg(
        short,
        long,
        env = EnvVars::UV_PUBLISH_TOKEN,
        hide_env_values = true,
        conflicts_with = "username",
        conflicts_with = "password",
        value_hint = ValueHint::Other,
    )]
    pub token: Option<String>,

    /// Configure trusted publishing.
    ///
    /// By default, uv checks for trusted publishing when running in a supported environment, but
    /// ignores it if it isn't configured.
    ///
    /// uv's supported environments for trusted publishing include GitHub Actions and GitLab CI/CD.
    #[arg(long)]
    pub trusted_publishing: Option<TrustedPublishing>,

    /// Attempt to use `keyring` for authentication for remote requirements files.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to use
    /// the `keyring` CLI to handle authentication.
    ///
    /// Defaults to `disabled`.
    #[arg(long, value_enum, env = EnvVars::UV_KEYRING_PROVIDER)]
    pub keyring_provider: Option<KeyringProviderType>,

    /// The URL of the upload endpoint (not the index URL).
    ///
    /// Note that there are typically different URLs for index access (e.g., `https:://.../simple`)
    /// and index upload.
    ///
    /// Defaults to PyPI's publish URL (<https://upload.pypi.org/legacy/>).
    #[arg(long, env = EnvVars::UV_PUBLISH_URL, hide_env_values = true)]
    pub publish_url: Option<DisplaySafeUrl>,

    /// Check an index URL for existing files to skip duplicate uploads.
    ///
    /// This option allows retrying publishing that failed after only some, but not all files have
    /// been uploaded, and handles errors due to parallel uploads of the same file.
    ///
    /// Before uploading, the index is checked. If the exact same file already exists in the index,
    /// the file will not be uploaded. If an error occurred during the upload, the index is checked
    /// again, to handle cases where the identical file was uploaded twice in parallel.
    ///
    /// The exact behavior will vary based on the index. When uploading to PyPI, uploading the same
    /// file succeeds even without `--check-url`, while most other indexes error. When uploading to
    /// pyx, the index URL can be inferred automatically from the publish URL.
    ///
    /// The index must provide one of the supported hashes (SHA-256, SHA-384, or SHA-512).
    #[arg(long, env = EnvVars::UV_PUBLISH_CHECK_URL, hide_env_values = true)]
    pub check_url: Option<IndexUrl>,

    #[arg(long, hide = true)]
    pub skip_existing: bool,

    /// Perform a dry run without uploading files.
    ///
    /// When enabled, the command will check for existing files if `--check-url` is provided,
    /// and will perform validation against the index if supported, but will not upload any files.
    #[arg(long)]
    pub dry_run: bool,

    /// Do not upload attestations for the published files.
    ///
    /// By default, uv attempts to upload matching PEP 740 attestations with each distribution
    /// that is published.
    #[arg(long, env = EnvVars::UV_PUBLISH_NO_ATTESTATIONS)]
    pub no_attestations: bool,

    /// Use direct upload to the registry.
    ///
    /// When enabled, the publish command will use a direct two-phase upload protocol
    /// that uploads files directly to storage, bypassing the registry's upload endpoint.
    #[arg(long, hide = true)]
    pub direct: bool,
}

#[derive(Args)]
pub struct WorkspaceNamespace {
    #[command(subcommand)]
    pub command: WorkspaceCommand,
}

#[derive(Subcommand)]
pub enum WorkspaceCommand {
    /// View metadata about the current workspace.
    ///
    /// The output of this command is not yet stable.
    Metadata(MetadataArgs),
    /// Display the path of a workspace member.
    ///
    /// By default, the path to the workspace root directory is displayed.
    /// The `--package` option can be used to display the path to a workspace member instead.
    ///
    /// If used outside of a workspace, i.e., if a `pyproject.toml` cannot be found, uv will exit with an error.
    Dir(WorkspaceDirArgs),
    /// List the members of a workspace.
    ///
    /// Displays newline separated names of workspace members.
    #[command(hide = true)]
    List(WorkspaceListArgs),
}

#[derive(Args, Debug)]
pub struct MetadataArgs;

#[derive(Args, Debug)]
pub struct WorkspaceDirArgs {
    /// Display the path to a specific package in the workspace.
    #[arg(long, value_hint = ValueHint::Other)]
    pub package: Option<PackageName>,
}

#[derive(Args, Debug)]
pub struct WorkspaceListArgs {
    /// Show paths instead of names.
    #[arg(long)]
    pub paths: bool,
}

/// See [PEP 517](https://peps.python.org/pep-0517/) and
/// [PEP 660](https://peps.python.org/pep-0660/) for specifications of the parameters.
#[derive(Subcommand)]
pub enum BuildBackendCommand {
    /// PEP 517 hook `build_sdist`.
    BuildSdist { sdist_directory: PathBuf },
    /// PEP 517 hook `build_wheel`.
    BuildWheel {
        wheel_directory: PathBuf,
        #[arg(long)]
        metadata_directory: Option<PathBuf>,
    },
    /// PEP 660 hook `build_editable`.
    BuildEditable {
        wheel_directory: PathBuf,
        #[arg(long)]
        metadata_directory: Option<PathBuf>,
    },
    /// PEP 517 hook `get_requires_for_build_sdist`.
    GetRequiresForBuildSdist,
    /// PEP 517 hook `get_requires_for_build_wheel`.
    GetRequiresForBuildWheel,
    /// PEP 517 hook `prepare_metadata_for_build_wheel`.
    PrepareMetadataForBuildWheel { wheel_directory: PathBuf },
    /// PEP 660 hook `get_requires_for_build_editable`.
    GetRequiresForBuildEditable,
    /// PEP 660 hook `prepare_metadata_for_build_editable`.
    PrepareMetadataForBuildEditable { wheel_directory: PathBuf },
}
