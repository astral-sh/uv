use std::{fmt::Debug, num::NonZeroUsize, path::PathBuf};

use serde::Deserialize;

use distribution_types::{FlatIndexLocation, IndexUrl};
use install_wheel_rs::linker::LinkMode;
use pep508_rs::Requirement;
use pypi_types::VerbatimParsedUrl;
use uv_configuration::{
    ConfigSettings, IndexStrategy, KeyringProviderType, PackageNameSpecifier, TargetTriple,
};
use uv_macros::{CombineOptions, OptionsMetadata};
use uv_normalize::{ExtraName, PackageName};
use uv_python::{PythonFetch, PythonPreference, PythonVersion};
use uv_resolver::{AnnotationStyle, ExcludeNewer, PreReleaseMode, ResolutionMode};

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
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[serde(flatten)]
    pub globals: GlobalOptions,
    #[serde(flatten)]
    pub top_level: ResolverInstallerOptions,
    #[option_group]
    pub pip: Option<PipOptions>,
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<Vec<String>>",
            description = "PEP 508 style requirements, e.g. `ruff==0.5.0`, or `ruff @ https://...`."
        )
    )]
    pub override_dependencies: Option<Vec<Requirement<VerbatimParsedUrl>>>,
    pub constraint_dependencies: Option<Vec<Requirement<VerbatimParsedUrl>>>,
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
    /// Linux, and `{FOLDERID_LocalAppData}\uv\cache` on Windows.
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
        default = "\"installed\"",
        value_type = "str",
        example = r#"
            python-preference = "managed"
        "#,
        possible_values = true
    )]
    pub python_preference: Option<PythonPreference>,
    /// Whether to automatically download Python when required.
    #[option(
        default = "\"automatic\"",
        value_type = "str",
        example = r#"
            python-fetch = "manual"
        "#,
        possible_values = true
    )]
    pub python_fetch: Option<PythonFetch>,
}

/// Settings relevant to all installer operations.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct InstallerOptions {
    pub index_url: Option<IndexUrl>,
    pub extra_index_url: Option<Vec<IndexUrl>>,
    pub no_index: Option<bool>,
    pub find_links: Option<Vec<FlatIndexLocation>>,
    pub index_strategy: Option<IndexStrategy>,
    pub keyring_provider: Option<KeyringProviderType>,
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
}

/// Settings relevant to all resolver operations.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ResolverOptions {
    pub index_url: Option<IndexUrl>,
    pub extra_index_url: Option<Vec<IndexUrl>>,
    pub no_index: Option<bool>,
    pub find_links: Option<Vec<FlatIndexLocation>>,
    pub index_strategy: Option<IndexStrategy>,
    pub keyring_provider: Option<KeyringProviderType>,
    pub resolution: Option<ResolutionMode>,
    pub prerelease: Option<PreReleaseMode>,
    pub config_settings: Option<ConfigSettings>,
    pub exclude_newer: Option<ExcludeNewer>,
    pub link_mode: Option<LinkMode>,
    pub upgrade: Option<bool>,
    pub upgrade_package: Option<Vec<Requirement<VerbatimParsedUrl>>>,
    pub no_build: Option<bool>,
    pub no_build_package: Option<Vec<PackageName>>,
    pub no_binary: Option<bool>,
    pub no_binary_package: Option<Vec<PackageName>>,
}

/// Shared settings, relevant to all operations that must resolve and install dependencies. The
/// union of [`InstallerOptions`] and [`ResolverOptions`].
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize, CombineOptions, OptionsMetadata)]
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
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
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
    pub prerelease: Option<PreReleaseMode>,
    /// Settings to pass to the [PEP 517](https://peps.python.org/pep-0517/) build backend,
    /// specified as `KEY=VALUE` pairs.
    #[option(
        default = "{}",
        value_type = "dict",
        example = r#"
            config-settings = { "editable_mode": "compat" }
        "#
    )]
    pub config_settings: Option<ConfigSettings>,
    /// Limit candidate packages to those that were uploaded prior to the given date.
    ///
    /// Accepts both [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) timestamps (e.g.,
    /// `2006-12-02T02:07:43Z`) and UTC dates in the same format (e.g., `2006-12-02`).
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
    /// - `3.10` looks for an installed Python 3.10 using `py --list-paths` on Windows, or
    ///   `python3.10` on Linux and macOS.
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
    /// source distributions (`.tar.gz` or `.zip`) at the top level.
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
    pub prerelease: Option<PreReleaseMode>,
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
    /// Use legacy `setuptools` behavior when building source distributions without a
    /// `pyproject.toml`.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            legacy-setup-py = true
        "#
    )]
    pub legacy_setup_py: Option<bool>,
    /// Settings to pass to the [PEP 517](https://peps.python.org/pep-0517/) build backend,
    /// specified as `KEY=VALUE` pairs.
    #[option(
        default = "{}",
        value_type = "dict",
        example = r#"
            config-settings = { "editable_mode": "compat" }
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
    /// `aaarch64-apple-darwin`.
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
    /// `2006-12-02T02:07:43Z`) and UTC dates in the same format (e.g., `2006-12-02`).
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
