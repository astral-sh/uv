use std::{fmt::Debug, num::NonZeroUsize, path::PathBuf};

use serde::{Deserialize, Serialize};

use distribution_types::{FlatIndexLocation, IndexUrl};
use install_wheel_rs::linker::LinkMode;
use pep508_rs::Requirement;
use pypi_types::{SupportedEnvironments, VerbatimParsedUrl};
use uv_cache_info::CacheKey;
use uv_configuration::{
    ConfigSettings, IndexStrategy, KeyringProviderType, PackageNameSpecifier, TargetTriple,
    TrustedHost,
};
use uv_macros::{CombineOptions, OptionsMetadata};
use uv_normalize::{ExtraName, PackageName};
use uv_python::{PythonDownloads, PythonPreference, PythonVersion};
use uv_resolver::{AnnotationStyle, ExcludeNewer, PrereleaseMode, ResolutionMode};

/// A `pyproject.toml` with an (optional) `[tool.uv]` section.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct PyProjectToml {
    pub(crate) tool: Option<Tools>,
}

/// A `[tool]` section.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct Tools {
    pub(crate) uv: Option<Options>,
}

/// A `[tool.uv]` section.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize, CombineOptions, OptionsMetadata)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[serde(flatten)]
    pub globals: GlobalOptions,
    #[serde(flatten)]
    pub top_level: ResolverInstallerOptions,
    #[option_group]
    pub pip: Option<PipOptions>,

    /// The keys to consider when caching builds for the project.
    ///
    /// Cache keys enable you to specify the files or directories that should trigger a rebuild when
    /// modified. By default, uv will rebuild a project whenever the `pyproject.toml`, `setup.py`,
    /// or `setup.cfg` files in the project directory are modified, i.e.:
    ///
    /// ```toml
    /// cache-keys = [{ file = "pyproject.toml" }, { file = "setup.py" }, { file = "setup.cfg" }]
    /// ```
    ///
    /// As an example: if a project uses dynamic metadata to read its dependencies from a
    /// `requirements.txt` file, you can specify `cache-keys = [{ file = "requirements.txt" }, { file = "pyproject.toml" }]`
    /// to ensure that the project is rebuilt whenever the `requirements.txt` file is modified (in
    /// addition to watching the `pyproject.toml`).
    ///
    /// Globs are supported, following the syntax of the [`glob`](https://docs.rs/glob/0.3.1/glob/struct.Pattern.html)
    /// crate. For example, to invalidate the cache whenever a `.toml` file in the project directory
    /// or any of its subdirectories is modified, you can specify `cache-keys = [{ file = "**/*.toml" }]`.
    /// Note that the use of globs can be expensive, as uv may need to walk the filesystem to
    /// determine whether any files have changed.
    ///
    /// Cache keys can also include version control information. For example, if a project uses
    /// `setuptools_scm` to read its version from a Git tag, you can specify `cache-keys = [{ git = true }, { file = "pyproject.toml" }]`
    /// to include the current Git commit hash in the cache key (in addition to the
    /// `pyproject.toml`).
    ///
    /// Cache keys only affect the project defined by the `pyproject.toml` in which they're
    /// specified (as opposed to, e.g., affecting all members in a workspace), and all paths and
    /// globs are interpreted as relative to the project directory.
    #[option(
        default = r#"[{ file = "pyproject.toml" }, { file = "setup.py" }, { file = "setup.cfg" }]"#,
        value_type = "list[dict]",
        example = r#"
            cache-keys = [{ file = "pyproject.toml" }, { file = "requirements.txt" }, { git = true }]
        "#
    )]
    #[serde(default, skip_serializing)]
    cache_keys: Option<Vec<CacheKey>>,

    // NOTE(charlie): These fields are shared with `ToolUv` in
    // `crates/uv-workspace/src/pyproject.rs`, and the documentation lives on that struct.
    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub override_dependencies: Option<Vec<Requirement<VerbatimParsedUrl>>>,

    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub constraint_dependencies: Option<Vec<Requirement<VerbatimParsedUrl>>>,

    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub environments: Option<SupportedEnvironments>,

    // NOTE(charlie): These fields should be kept in-sync with `ToolUv` in
    // `crates/uv-workspace/src/pyproject.rs`.
    #[serde(default, skip_serializing)]
    #[cfg_attr(feature = "schemars", schemars(skip))]
    workspace: serde::de::IgnoredAny,

    #[serde(default, skip_serializing)]
    #[cfg_attr(feature = "schemars", schemars(skip))]
    sources: serde::de::IgnoredAny,

    #[serde(default, skip_serializing)]
    #[cfg_attr(feature = "schemars", schemars(skip))]
    dev_dependencies: serde::de::IgnoredAny,

    #[serde(default, skip_serializing)]
    #[cfg_attr(feature = "schemars", schemars(skip))]
    managed: serde::de::IgnoredAny,

    #[serde(default, skip_serializing)]
    #[cfg_attr(feature = "schemars", schemars(skip))]
    r#package: serde::de::IgnoredAny,
}

impl Options {
    /// Construct an [`Options`] with the given global and top-level settings.
    pub fn simple(globals: GlobalOptions, top_level: ResolverInstallerOptions) -> Self {
        Self {
            globals,
            top_level,
            ..Default::default()
        }
    }
}

/// Global settings, relevant to all invocations.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize, CombineOptions, OptionsMetadata)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GlobalOptions {
    /// Whether to load TLS certificates from the platform's native certificate store.
    ///
    /// By default, uv loads certificates from the bundled `webpki-roots` crate. The
    /// `webpki-roots` are a reliable set of trust roots from Mozilla, and including them in uv
    /// improves portability and performance (especially on macOS).
    ///
    /// However, in some cases, you may want to use the platform's native certificate store,
    /// especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's
    /// included in your system's certificate store.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            native-tls = true
        "#
    )]
    pub native_tls: Option<bool>,
    /// Disable network access, relying only on locally cached data and locally available files.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            offline = true
        "#
    )]
    pub offline: Option<bool>,
    /// Avoid reading from or writing to the cache, instead using a temporary directory for the
    /// duration of the operation.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-cache = true
        "#
    )]
    pub no_cache: Option<bool>,
    /// Path to the cache directory.
    ///
    /// Defaults to `$HOME/Library/Caches/uv` on macOS, `$XDG_CACHE_HOME/uv` or `$HOME/.cache/uv` on
    /// Linux, and `%LOCALAPPDATA%\uv\cache` on Windows.
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            cache-dir = "./.uv_cache"
        "#
    )]
    pub cache_dir: Option<PathBuf>,
    /// Whether to enable experimental, preview features.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            preview = true
        "#
    )]
    pub preview: Option<bool>,
    /// Whether to prefer using Python installations that are already present on the system, or
    /// those that are downloaded and installed by uv.
    #[option(
        default = "\"managed\"",
        value_type = "str",
        example = r#"
            python-preference = "managed"
        "#,
        possible_values = true
    )]
    pub python_preference: Option<PythonPreference>,
    /// Whether to allow Python downloads.
    #[option(
        default = "\"automatic\"",
        value_type = "str",
        example = r#"
            python-downloads = "manual"
        "#,
        possible_values = true
    )]
    pub python_downloads: Option<PythonDownloads>,
    /// The maximum number of in-flight concurrent downloads that uv will perform at any given
    /// time.
    #[option(
        default = "50",
        value_type = "int",
        example = r#"
            concurrent-downloads = 4
        "#
    )]
    pub concurrent_downloads: Option<NonZeroUsize>,
    /// The maximum number of source distributions that uv will build concurrently at any given
    /// time.
    ///
    /// Defaults to the number of available CPU cores.
    #[option(
        default = "None",
        value_type = "int",
        example = r#"
            concurrent-builds = 4
        "#
    )]
    pub concurrent_builds: Option<NonZeroUsize>,
    /// The number of threads used when installing and unzipping packages.
    ///
    /// Defaults to the number of available CPU cores.
    #[option(
        default = "None",
        value_type = "int",
        example = r#"
            concurrent-installs = 4
        "#
    )]
    pub concurrent_installs: Option<NonZeroUsize>,
}

/// Settings relevant to all installer operations.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize, CombineOptions)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct InstallerOptions {
    pub index_url: Option<IndexUrl>,
    pub extra_index_url: Option<Vec<IndexUrl>>,
    pub no_index: Option<bool>,
    pub find_links: Option<Vec<FlatIndexLocation>>,
    pub index_strategy: Option<IndexStrategy>,
    pub keyring_provider: Option<KeyringProviderType>,
    pub allow_insecure_host: Option<Vec<TrustedHost>>,
    pub config_settings: Option<ConfigSettings>,
    pub exclude_newer: Option<ExcludeNewer>,
    pub link_mode: Option<LinkMode>,
    pub compile_bytecode: Option<bool>,
    pub reinstall: Option<bool>,
    pub reinstall_package: Option<Vec<PackageName>>,
    pub no_build: Option<bool>,
    pub no_build_package: Option<Vec<PackageName>>,
    pub no_binary: Option<bool>,
    pub no_binary_package: Option<Vec<PackageName>>,
    pub no_build_isolation: Option<bool>,
    pub no_sources: Option<bool>,
}

/// Settings relevant to all resolver operations.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize, CombineOptions)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ResolverOptions {
    pub index_url: Option<IndexUrl>,
    pub extra_index_url: Option<Vec<IndexUrl>>,
    pub no_index: Option<bool>,
    pub find_links: Option<Vec<FlatIndexLocation>>,
    pub index_strategy: Option<IndexStrategy>,
    pub keyring_provider: Option<KeyringProviderType>,
    pub allow_insecure_host: Option<Vec<TrustedHost>>,
    pub resolution: Option<ResolutionMode>,
    pub prerelease: Option<PrereleaseMode>,
    pub config_settings: Option<ConfigSettings>,
    pub exclude_newer: Option<ExcludeNewer>,
    pub link_mode: Option<LinkMode>,
    pub upgrade: Option<bool>,
    pub upgrade_package: Option<Vec<Requirement<VerbatimParsedUrl>>>,
    pub no_build: Option<bool>,
    pub no_build_package: Option<Vec<PackageName>>,
    pub no_binary: Option<bool>,
    pub no_binary_package: Option<Vec<PackageName>>,
    pub no_build_isolation: Option<bool>,
    pub no_build_isolation_package: Option<Vec<PackageName>>,
    pub no_sources: Option<bool>,
}

/// Shared settings, relevant to all operations that must resolve and install dependencies. The
/// union of [`InstallerOptions`] and [`ResolverOptions`].
#[allow(dead_code)]
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, CombineOptions, OptionsMetadata,
)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ResolverInstallerOptions {
    /// The URL of the Python package index (by default: <https://pypi.org/simple>).
    ///
    /// Accepts either a repository compliant with [PEP 503](https://peps.python.org/pep-0503/)
    /// (the simple repository API), or a local directory laid out in the same format.
    ///
    /// The index provided by this setting is given lower priority than any indexes specified via
    /// [`extra_index_url`](#extra-index-url).
    #[option(
        default = "\"https://pypi.org/simple\"",
        value_type = "str",
        example = r#"
            index-url = "https://test.pypi.org/simple"
        "#
    )]
    pub index_url: Option<IndexUrl>,
    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    ///
    /// Accepts either a repository compliant with [PEP 503](https://peps.python.org/pep-0503/)
    /// (the simple repository API), or a local directory laid out in the same format.
    ///
    /// All indexes provided via this flag take priority over the index specified by
    /// [`index_url`](#index-url). When multiple indexes are provided, earlier values take priority.
    ///
    /// To control uv's resolution strategy when multiple indexes are present, see
    /// [`index_strategy`](#index-strategy).
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            extra-index-url = ["https://download.pytorch.org/whl/cpu"]
        "#
    )]
    pub extra_index_url: Option<Vec<IndexUrl>>,
    /// Ignore all registry indexes (e.g., PyPI), instead relying on direct URL dependencies and
    /// those provided via `--find-links`.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-index = true
        "#
    )]
    pub no_index: Option<bool>,
    /// Locations to search for candidate distributions, in addition to those found in the registry
    /// indexes.
    ///
    /// If a path, the target must be a directory that contains packages as wheel files (`.whl`) or
    /// source distributions (e.g., `.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files adhering to the
    /// formats described above.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            find-links = ["https://download.pytorch.org/whl/torch_stable.html"]
        "#
    )]
    pub find_links: Option<Vec<FlatIndexLocation>>,
    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-match`). This prevents
    /// "dependency confusion" attacks, whereby an attack can upload a malicious package under the
    /// same name to a secondary.
    #[option(
        default = "\"first-index\"",
        value_type = "str",
        example = r#"
            index-strategy = "unsafe-best-match"
        "#,
        possible_values = true
    )]
    pub index_strategy: Option<IndexStrategy>,
    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to
    /// use the `keyring` CLI to handle authentication.
    #[option(
        default = "\"disabled\"",
        value_type = "str",
        example = r#"
            keyring-provider = "subprocess"
        "#
    )]
    pub keyring_provider: Option<KeyringProviderType>,
    /// Allow insecure connections to host.
    ///
    /// Expects to receive either a hostname (e.g., `localhost`), a host-port pair (e.g.,
    /// `localhost:8080`), or a URL (e.g., `https://localhost`).
    ///
    /// WARNING: Hosts included in this list will not be verified against the system's certificate
    /// store. Only use `--allow-insecure-host` in a secure network with verified sources, as it
    /// bypasses SSL verification and could expose you to MITM attacks.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            allow-insecure-host = ["localhost:8080"]
        "#
    )]
    pub allow_insecure_host: Option<Vec<TrustedHost>>,
    /// The strategy to use when selecting between the different compatible versions for a given
    /// package requirement.
    ///
    /// By default, uv will use the latest compatible version of each package (`highest`).
    #[option(
        default = "\"highest\"",
        value_type = "str",
        example = r#"
            resolution = "lowest-direct"
        "#,
        possible_values = true
    )]
    pub resolution: Option<ResolutionMode>,
    /// The strategy to use when considering pre-release versions.
    ///
    /// By default, uv will accept pre-releases for packages that _only_ publish pre-releases,
    /// along with first-party requirements that contain an explicit pre-release marker in the
    /// declared specifiers (`if-necessary-or-explicit`).
    #[option(
        default = "\"if-necessary-or-explicit\"",
        value_type = "str",
        example = r#"
            prerelease = "allow"
        "#,
        possible_values = true
    )]
    pub prerelease: Option<PrereleaseMode>,
    /// Settings to pass to the [PEP 517](https://peps.python.org/pep-0517/) build backend,
    /// specified as `KEY=VALUE` pairs.
    #[option(
        default = "{}",
        value_type = "dict",
        example = r#"
            config-settings = { editable_mode = "compat" }
        "#
    )]
    pub config_settings: Option<ConfigSettings>,
    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by [PEP 518](https://peps.python.org/pep-0518/)
    /// are already installed.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-build-isolation = true
        "#
    )]
    pub no_build_isolation: Option<bool>,
    /// Disable isolation when building source distributions for a specific package.
    ///
    /// Assumes that the packages' build dependencies specified by [PEP 518](https://peps.python.org/pep-0518/)
    /// are already installed.
    #[option(
        default = "[]",
        value_type = "Vec<PackageName>",
        example = r#"
        no-build-isolation-package = ["package1", "package2"]
    "#
    )]
    pub no_build_isolation_package: Option<Vec<PackageName>>,
    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) timestamps (e.g.,
    /// `2006-12-02T02:07:43Z`) and local dates in the same format (e.g., `2006-12-02`) in your
    /// system's configured time zone.
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            exclude-newer = "2006-12-02"
        "#
    )]
    pub exclude_newer: Option<ExcludeNewer>,
    /// The method to use when installing packages from the global cache.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[option(
        default = "\"clone\" (macOS) or \"hardlink\" (Linux, Windows)",
        value_type = "str",
        example = r#"
            link-mode = "copy"
        "#,
        possible_values = true
    )]
    pub link_mode: Option<LinkMode>,
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
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            compile-bytecode = true
        "#
    )]
    pub compile_bytecode: Option<bool>,
    /// Ignore the `tool.uv.sources` table when resolving dependencies. Used to lock against the
    /// standards-compliant, publishable package metadata, as opposed to using any local or Git
    /// sources.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-sources = true
        "#
    )]
    pub no_sources: Option<bool>,
    /// Allow package upgrades, ignoring pinned versions in any existing output file.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            upgrade = true
        "#
    )]
    pub upgrade: Option<bool>,
    /// Allow upgrades for a specific package, ignoring pinned versions in any existing output
    /// file.
    ///
    /// Accepts both standalone package names (`ruff`) and version specifiers (`ruff<0.5.0`).
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            upgrade-package = ["ruff"]
        "#
    )]
    pub upgrade_package: Option<Vec<Requirement<VerbatimParsedUrl>>>,
    /// Reinstall all packages, regardless of whether they're already installed. Implies `refresh`.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            reinstall = true
        "#
    )]
    pub reinstall: Option<bool>,
    /// Reinstall a specific package, regardless of whether it's already installed. Implies
    /// `refresh-package`.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            reinstall-package = ["ruff"]
        "#
    )]
    pub reinstall_package: Option<Vec<PackageName>>,
    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary Python code. The cached wheels of
    /// already-built source distributions will be reused, but operations that require building
    /// distributions will exit with an error.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-build = true
        "#
    )]
    pub no_build: Option<bool>,
    /// Don't build source distributions for a specific package.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            no-build-package = ["ruff"]
        "#
    )]
    pub no_build_package: Option<Vec<PackageName>>,
    /// Don't install pre-built wheels.
    ///
    /// The given packages will be built and installed from source. The resolver will still use
    /// pre-built wheels to extract package metadata, if available.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-binary = true
        "#
    )]
    pub no_binary: Option<bool>,
    /// Don't install pre-built wheels for a specific package.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            no-binary-package = ["ruff"]
        "#
    )]
    pub no_binary_package: Option<Vec<PackageName>>,
}

/// Settings that are specific to the `uv pip` command-line interface.
///
/// These values will be ignored when running commands outside the `uv pip` namespace (e.g.,
/// `uv lock`, `uvx`).
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize, CombineOptions, OptionsMetadata)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PipOptions {
    /// The Python interpreter into which packages should be installed.
    ///
    /// By default, uv installs into the virtual environment in the current working directory or
    /// any parent directory. The `--python` option allows you to specify a different interpreter,
    /// which is intended for use in continuous integration (CI) environments or other automated
    /// workflows.
    ///
    /// Supported formats:
    /// - `3.10` looks for an installed Python 3.10 in the registry on Windows (see
    ///   `py --list-paths`), or `python3.10` on Linux and macOS.
    /// - `python3.10` or `python.exe` looks for a binary with the given name in `PATH`.
    /// - `/home/ferris/.local/bin/python3.10` uses the exact Python at the given path.
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            python = "3.10"
        "#
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
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            system = true
        "#
    )]
    pub system: Option<bool>,
    /// Allow uv to modify an `EXTERNALLY-MANAGED` Python installation.
    ///
    /// WARNING: `--break-system-packages` is intended for use in continuous integration (CI)
    /// environments, when installing into Python installations that are managed by an external
    /// package manager, like `apt`. It should be used with caution, as such Python installations
    /// explicitly recommend against modifications by other package managers (like uv or pip).
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            break-system-packages = true
        "#
    )]
    pub break_system_packages: Option<bool>,
    /// Install packages into the specified directory, rather than into the virtual or system Python
    /// environment. The packages will be installed at the top-level of the directory.
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            target = "./target"
        "#
    )]
    pub target: Option<PathBuf>,
    /// Install packages into `lib`, `bin`, and other top-level folders under the specified
    /// directory, as if a virtual environment were present at that location.
    ///
    /// In general, prefer the use of `--python` to install into an alternate environment, as
    /// scripts and other artifacts installed via `--prefix` will reference the installing
    /// interpreter, rather than any interpreter added to the `--prefix` directory, rendering them
    /// non-portable.
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            prefix = "./prefix"
        "#
    )]
    pub prefix: Option<PathBuf>,
    /// The URL of the Python package index (by default: <https://pypi.org/simple>).
    ///
    /// Accepts either a repository compliant with [PEP 503](https://peps.python.org/pep-0503/)
    /// (the simple repository API), or a local directory laid out in the same format.
    ///
    /// The index provided by this setting is given lower priority than any indexes specified via
    /// [`extra_index_url`](#extra-index-url).
    #[option(
        default = "\"https://pypi.org/simple\"",
        value_type = "str",
        example = r#"
            index-url = "https://test.pypi.org/simple"
        "#
    )]
    pub index_url: Option<IndexUrl>,
    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    ///
    /// Accepts either a repository compliant with [PEP 503](https://peps.python.org/pep-0503/)
    /// (the simple repository API), or a local directory laid out in the same format.
    ///
    /// All indexes provided via this flag take priority over the index specified by
    /// [`index_url`](#index-url). When multiple indexes are provided, earlier values take priority.
    ///
    /// To control uv's resolution strategy when multiple indexes are present, see
    /// [`index_strategy`](#index-strategy).
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            extra-index-url = ["https://download.pytorch.org/whl/cpu"]
        "#
    )]
    pub extra_index_url: Option<Vec<IndexUrl>>,
    /// Ignore all registry indexes (e.g., PyPI), instead relying on direct URL dependencies and
    /// those provided via `--find-links`.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-index = true
        "#
    )]
    pub no_index: Option<bool>,
    /// Locations to search for candidate distributions, in addition to those found in the registry
    /// indexes.
    ///
    /// If a path, the target must be a directory that contains packages as wheel files (`.whl`) or
    /// source distributions (e.g., `.tar.gz` or `.zip`) at the top level.
    ///
    /// If a URL, the page must contain a flat list of links to package files adhering to the
    /// formats described above.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            find-links = ["https://download.pytorch.org/whl/torch_stable.html"]
        "#
    )]
    pub find_links: Option<Vec<FlatIndexLocation>>,
    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-match`). This prevents
    /// "dependency confusion" attacks, whereby an attack can upload a malicious package under the
    /// same name to a secondary.
    #[option(
        default = "\"first-index\"",
        value_type = "str",
        example = r#"
            index-strategy = "unsafe-best-match"
        "#,
        possible_values = true
    )]
    pub index_strategy: Option<IndexStrategy>,
    /// Attempt to use `keyring` for authentication for index URLs.
    ///
    /// At present, only `--keyring-provider subprocess` is supported, which configures uv to
    /// use the `keyring` CLI to handle authentication.
    #[option(
        default = "disabled",
        value_type = "str",
        example = r#"
            keyring-provider = "subprocess"
        "#
    )]
    pub keyring_provider: Option<KeyringProviderType>,
    /// Allow insecure connections to host.
    ///
    /// Expects to receive either a hostname (e.g., `localhost`), a host-port pair (e.g.,
    /// `localhost:8080`), or a URL (e.g., `https://localhost`).
    ///
    /// WARNING: Hosts included in this list will not be verified against the system's certificate
    /// store. Only use `--allow-insecure-host` in a secure network with verified sources, as it
    /// bypasses SSL verification and could expose you to MITM attacks.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            allow-insecure-host = ["localhost:8080"]
        "#
    )]
    pub allow_insecure_host: Option<Vec<TrustedHost>>,
    /// Don't build source distributions.
    ///
    /// When enabled, resolving will not run arbitrary Python code. The cached wheels of
    /// already-built source distributions will be reused, but operations that require building
    /// distributions will exit with an error.
    ///
    /// Alias for `--only-binary :all:`.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-build = true
        "#
    )]
    pub no_build: Option<bool>,
    /// Don't install pre-built wheels.
    ///
    /// The given packages will be built and installed from source. The resolver will still use
    /// pre-built wheels to extract package metadata, if available.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            no-binary = ["ruff"]
        "#
    )]
    pub no_binary: Option<Vec<PackageNameSpecifier>>,
    /// Only use pre-built wheels; don't build source distributions.
    ///
    /// When enabled, resolving will not run code from the given packages. The cached wheels of already-built
    /// source distributions will be reused, but operations that require building distributions will
    /// exit with an error.
    ///
    /// Multiple packages may be provided. Disable binaries for all packages with `:all:`.
    /// Clear previously specified packages with `:none:`.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            only-binary = ["ruff"]
        "#
    )]
    pub only_binary: Option<Vec<PackageNameSpecifier>>,
    /// Disable isolation when building source distributions.
    ///
    /// Assumes that build dependencies specified by [PEP 518](https://peps.python.org/pep-0518/)
    /// are already installed.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-build-isolation = true
        "#
    )]
    pub no_build_isolation: Option<bool>,
    /// Disable isolation when building source distributions for a specific package.
    ///
    /// Assumes that the packages' build dependencies specified by [PEP 518](https://peps.python.org/pep-0518/)
    /// are already installed.
    #[option(
        default = "[]",
        value_type = "Vec<PackageName>",
        example = r#"
            no-build-isolation-package = ["package1", "package2"]
        "#
    )]
    pub no_build_isolation_package: Option<Vec<PackageName>>,
    /// Validate the Python environment, to detect packages with missing dependencies and other
    /// issues.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            strict = true
        "#
    )]
    pub strict: Option<bool>,
    /// Include optional dependencies from the extra group name; may be provided more than once.
    ///
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            extra = ["dev", "docs"]
        "#
    )]
    pub extra: Option<Vec<ExtraName>>,
    /// Include all optional dependencies.
    ///
    /// Only applies to `pyproject.toml`, `setup.py`, and `setup.cfg` sources.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            all-extras = true
        "#
    )]
    pub all_extras: Option<bool>,
    /// Ignore package dependencies, instead only add those packages explicitly listed
    /// on the command line to the resulting the requirements file.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-deps = true
        "#
    )]
    pub no_deps: Option<bool>,
    /// Allow `uv pip sync` with empty requirements, which will clear the environment of all
    /// packages.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            allow-empty-requirements = true
        "#
    )]
    pub allow_empty_requirements: Option<bool>,
    /// The strategy to use when selecting between the different compatible versions for a given
    /// package requirement.
    ///
    /// By default, uv will use the latest compatible version of each package (`highest`).
    #[option(
        default = "\"highest\"",
        value_type = "str",
        example = r#"
            resolution = "lowest-direct"
        "#,
        possible_values = true
    )]
    pub resolution: Option<ResolutionMode>,
    /// The strategy to use when considering pre-release versions.
    ///
    /// By default, uv will accept pre-releases for packages that _only_ publish pre-releases,
    /// along with first-party requirements that contain an explicit pre-release marker in the
    /// declared specifiers (`if-necessary-or-explicit`).
    #[option(
        default = "\"if-necessary-or-explicit\"",
        value_type = "str",
        example = r#"
            prerelease = "allow"
        "#,
        possible_values = true
    )]
    pub prerelease: Option<PrereleaseMode>,
    /// Write the requirements generated by `uv pip compile` to the given `requirements.txt` file.
    ///
    /// If the file already exists, the existing versions will be preferred when resolving
    /// dependencies, unless `--upgrade` is also specified.
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            output-file = "requirements.txt"
        "#
    )]
    pub output_file: Option<PathBuf>,
    /// Include extras in the output file.
    ///
    /// By default, uv strips extras, as any packages pulled in by the extras are already included
    /// as dependencies in the output file directly. Further, output files generated with
    /// `--no-strip-extras` cannot be used as constraints files in `install` and `sync` invocations.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-strip-extras = true
        "#
    )]
    pub no_strip_extras: Option<bool>,
    /// Include environment markers in the output file generated by `uv pip compile`.
    ///
    /// By default, uv strips environment markers, as the resolution generated by `compile` is
    /// only guaranteed to be correct for the target environment.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-strip-markers = true
        "#
    )]
    pub no_strip_markers: Option<bool>,
    /// Exclude comment annotations indicating the source of each package from the output file
    /// generated by `uv pip compile`.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-annotate = true
        "#
    )]
    pub no_annotate: Option<bool>,
    /// Exclude the comment header at the top of output file generated by `uv pip compile`.
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            no-header = true
        "#
    )]
    pub no_header: Option<bool>,
    /// The header comment to include at the top of the output file generated by `uv pip compile`.
    ///
    /// Used to reflect custom build scripts and commands that wrap `uv pip compile`.
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            custom-compile-command = "./custom-uv-compile.sh"
        "#
    )]
    pub custom_compile_command: Option<String>,
    /// Include distribution hashes in the output file.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            generate-hashes = true
        "#
    )]
    pub generate_hashes: Option<bool>,
    /// Settings to pass to the [PEP 517](https://peps.python.org/pep-0517/) build backend,
    /// specified as `KEY=VALUE` pairs.
    #[option(
        default = "{}",
        value_type = "dict",
        example = r#"
            config-settings = { editable_mode = "compat" }
        "#
    )]
    pub config_settings: Option<ConfigSettings>,
    /// The minimum Python version that should be supported by the resolved requirements (e.g.,
    /// `3.8` or `3.8.17`).
    ///
    /// If a patch version is omitted, the minimum patch version is assumed. For example, `3.8` is
    /// mapped to `3.8.0`.
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            python-version = "3.8"
        "#
    )]
    pub python_version: Option<PythonVersion>,
    /// The platform for which requirements should be resolved.
    ///
    /// Represented as a "target triple", a string that describes the target platform in terms of
    /// its CPU, vendor, and operating system name, like `x86_64-unknown-linux-gnu` or
    /// `aarch64-apple-darwin`.
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            python-platform = "x86_64-unknown-linux-gnu"
        "#
    )]
    pub python_platform: Option<TargetTriple>,
    /// Perform a universal resolution, attempting to generate a single `requirements.txt` output
    /// file that is compatible with all operating systems, architectures, and Python
    /// implementations.
    ///
    /// In universal mode, the current Python version (or user-provided `--python-version`) will be
    /// treated as a lower bound. For example, `--universal --python-version 3.7` would produce a
    /// universal resolution for Python 3.7 and later.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            universal = true
        "#
    )]
    pub universal: Option<bool>,
    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) timestamps (e.g.,
    /// `2006-12-02T02:07:43Z`) and local dates in the same format (e.g., `2006-12-02`) in your
    /// system's configured time zone.
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            exclude-newer = "2006-12-02"
        "#
    )]
    pub exclude_newer: Option<ExcludeNewer>,
    /// Specify a package to omit from the output resolution. Its dependencies will still be
    /// included in the resolution. Equivalent to pip-compile's `--unsafe-package` option.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            no-emit-package = ["ruff"]
        "#
    )]
    pub no_emit_package: Option<Vec<PackageName>>,
    /// Include `--index-url` and `--extra-index-url` entries in the output file generated by `uv pip compile`.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            emit-index-url = true
        "#
    )]
    pub emit_index_url: Option<bool>,
    /// Include `--find-links` entries in the output file generated by `uv pip compile`.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            emit-find-links = true
        "#
    )]
    pub emit_find_links: Option<bool>,
    /// Include `--no-binary` and `--only-binary` entries in the output file generated by `uv pip compile`.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            emit-build-options = true
        "#
    )]
    pub emit_build_options: Option<bool>,
    /// Whether to emit a marker string indicating the conditions under which the set of pinned
    /// dependencies is valid.
    ///
    /// The pinned dependencies may be valid even when the marker expression is
    /// false, but when the expression is true, the requirements are known to
    /// be correct.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            emit-marker-expression = true
        "#
    )]
    pub emit_marker_expression: Option<bool>,
    /// Include comment annotations indicating the index used to resolve each package (e.g.,
    /// `# from https://pypi.org/simple`).
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            emit-index-annotation = true
        "#
    )]
    pub emit_index_annotation: Option<bool>,
    /// The style of the annotation comments included in the output file, used to indicate the
    /// source of each package.
    #[option(
        default = "\"split\"",
        value_type = "str",
        example = r#"
            annotation-style = "line"
        "#,
        possible_values = true
    )]
    pub annotation_style: Option<AnnotationStyle>,
    /// The method to use when installing packages from the global cache.
    ///
    /// Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
    /// Windows.
    #[option(
        default = "\"clone\" (macOS) or \"hardlink\" (Linux, Windows)",
        value_type = "str",
        example = r#"
            link-mode = "copy"
        "#,
        possible_values = true
    )]
    pub link_mode: Option<LinkMode>,
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
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            compile-bytecode = true
        "#
    )]
    pub compile_bytecode: Option<bool>,
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
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            require-hashes = true
        "#
    )]
    pub require_hashes: Option<bool>,
    /// Validate any hashes provided in the requirements file.
    ///
    /// Unlike `--require-hashes`, `--verify-hashes` does not require that all requirements have
    /// hashes; instead, it will limit itself to verifying the hashes of those requirements that do
    /// include them.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            verify-hashes = true
        "#
    )]
    pub verify_hashes: Option<bool>,
    /// Ignore the `tool.uv.sources` table when resolving dependencies. Used to lock against the
    /// standards-compliant, publishable package metadata, as opposed to using any local or Git
    /// sources.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-sources = true
        "#
    )]
    pub no_sources: Option<bool>,
    /// Allow package upgrades, ignoring pinned versions in any existing output file.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            upgrade = true
        "#
    )]
    pub upgrade: Option<bool>,
    /// Allow upgrades for a specific package, ignoring pinned versions in any existing output
    /// file.
    ///
    /// Accepts both standalone package names (`ruff`) and version specifiers (`ruff<0.5.0`).
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            upgrade-package = ["ruff"]
        "#
    )]
    pub upgrade_package: Option<Vec<Requirement<VerbatimParsedUrl>>>,
    /// Reinstall all packages, regardless of whether they're already installed. Implies `refresh`.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            reinstall = true
        "#
    )]
    pub reinstall: Option<bool>,
    /// Reinstall a specific package, regardless of whether it's already installed. Implies
    /// `refresh-package`.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            reinstall-package = ["ruff"]
        "#
    )]
    pub reinstall_package: Option<Vec<PackageName>>,
}

impl From<ResolverInstallerOptions> for ResolverOptions {
    fn from(value: ResolverInstallerOptions) -> Self {
        Self {
            index_url: value.index_url,
            extra_index_url: value.extra_index_url,
            no_index: value.no_index,
            find_links: value.find_links,
            index_strategy: value.index_strategy,
            keyring_provider: value.keyring_provider,
            allow_insecure_host: value.allow_insecure_host,
            resolution: value.resolution,
            prerelease: value.prerelease,
            config_settings: value.config_settings,
            exclude_newer: value.exclude_newer,
            link_mode: value.link_mode,
            upgrade: value.upgrade,
            upgrade_package: value.upgrade_package,
            no_build: value.no_build,
            no_build_package: value.no_build_package,
            no_binary: value.no_binary,
            no_binary_package: value.no_binary_package,
            no_build_isolation: value.no_build_isolation,
            no_build_isolation_package: value.no_build_isolation_package,
            no_sources: value.no_sources,
        }
    }
}

impl From<ResolverInstallerOptions> for InstallerOptions {
    fn from(value: ResolverInstallerOptions) -> Self {
        Self {
            index_url: value.index_url,
            extra_index_url: value.extra_index_url,
            no_index: value.no_index,
            find_links: value.find_links,
            index_strategy: value.index_strategy,
            keyring_provider: value.keyring_provider,
            allow_insecure_host: value.allow_insecure_host,
            config_settings: value.config_settings,
            exclude_newer: value.exclude_newer,
            link_mode: value.link_mode,
            compile_bytecode: value.compile_bytecode,
            reinstall: value.reinstall,
            reinstall_package: value.reinstall_package,
            no_build: value.no_build,
            no_build_package: value.no_build_package,
            no_binary: value.no_binary,
            no_binary_package: value.no_binary_package,
            no_build_isolation: value.no_build_isolation,
            no_sources: value.no_sources,
        }
    }
}

/// The options persisted alongside an installed tool.
///
/// A mirror of [`ResolverInstallerOptions`], without upgrades and reinstalls, which shouldn't be
/// persisted in a tool receipt.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, CombineOptions, OptionsMetadata,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ToolOptions {
    pub index_url: Option<IndexUrl>,
    pub extra_index_url: Option<Vec<IndexUrl>>,
    pub no_index: Option<bool>,
    pub find_links: Option<Vec<FlatIndexLocation>>,
    pub index_strategy: Option<IndexStrategy>,
    pub keyring_provider: Option<KeyringProviderType>,
    pub allow_insecure_host: Option<Vec<TrustedHost>>,
    pub resolution: Option<ResolutionMode>,
    pub prerelease: Option<PrereleaseMode>,
    pub config_settings: Option<ConfigSettings>,
    pub no_build_isolation: Option<bool>,
    pub no_build_isolation_package: Option<Vec<PackageName>>,
    pub exclude_newer: Option<ExcludeNewer>,
    pub link_mode: Option<LinkMode>,
    pub compile_bytecode: Option<bool>,
    pub no_sources: Option<bool>,
    pub no_build: Option<bool>,
    pub no_build_package: Option<Vec<PackageName>>,
    pub no_binary: Option<bool>,
    pub no_binary_package: Option<Vec<PackageName>>,
}

impl From<ResolverInstallerOptions> for ToolOptions {
    fn from(value: ResolverInstallerOptions) -> Self {
        Self {
            index_url: value.index_url,
            extra_index_url: value.extra_index_url,
            no_index: value.no_index,
            find_links: value.find_links,
            index_strategy: value.index_strategy,
            keyring_provider: value.keyring_provider,
            allow_insecure_host: value.allow_insecure_host,
            resolution: value.resolution,
            prerelease: value.prerelease,
            config_settings: value.config_settings,
            no_build_isolation: value.no_build_isolation,
            no_build_isolation_package: value.no_build_isolation_package,
            exclude_newer: value.exclude_newer,
            link_mode: value.link_mode,
            compile_bytecode: value.compile_bytecode,
            no_sources: value.no_sources,
            no_build: value.no_build,
            no_build_package: value.no_build_package,
            no_binary: value.no_binary,
            no_binary_package: value.no_binary_package,
        }
    }
}

impl From<ToolOptions> for ResolverInstallerOptions {
    fn from(value: ToolOptions) -> Self {
        Self {
            index_url: value.index_url,
            extra_index_url: value.extra_index_url,
            no_index: value.no_index,
            find_links: value.find_links,
            index_strategy: value.index_strategy,
            keyring_provider: value.keyring_provider,
            allow_insecure_host: value.allow_insecure_host,
            resolution: value.resolution,
            prerelease: value.prerelease,
            config_settings: value.config_settings,
            no_build_isolation: value.no_build_isolation,
            no_build_isolation_package: value.no_build_isolation_package,
            exclude_newer: value.exclude_newer,
            link_mode: value.link_mode,
            compile_bytecode: value.compile_bytecode,
            no_sources: value.no_sources,
            upgrade: None,
            upgrade_package: None,
            reinstall: None,
            reinstall_package: None,
            no_build: value.no_build,
            no_build_package: value.no_build_package,
            no_binary: value.no_binary,
            no_binary_package: value.no_binary_package,
        }
    }
}
