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
        "#
    )]
    pub python_preference: Option<PythonPreference>,
    /// Whether to automatically download Python when required.
    #[option(
        default = "\"automatic\"",
        value_type = "str",
        example = r#"
            python-fetch = \"automatic\"
        "#
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
    /// Accepts either a repository compliant with PEP 503 (the simple repository API), or a local
    /// directory laid out in the same format.
    ///
    /// The index provided by this setting is given lower priority than any indexes specified via
    /// [`extra_index_url`](#extra-index-url).
    #[option(
        default = "\"https://pypi.org/simple\"",
        value_type = "str",
        example = r#"
            index-url = "https://pypi.org/simple"
        "#
    )]
    pub index_url: Option<IndexUrl>,
    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    ///
    /// Accepts either a repository compliant with PEP 503 (the simple repository API), or a local
    /// directory laid out in the same format.
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
    ///
    /// Possible values:
    ///
    /// - `"first-index"`:        Only use results from the first index that returns a match for a given package name.
    /// - `"unsafe-first-match"`: Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next.
    /// - `"unsafe-best-match"`:  Search for every package name across all indexes, preferring the "best" version found. If a package version is in multiple indexes, only look at the entry for the first index.
    #[option(
        default = "\"first-index\"",
        value_type = "str",
        example = r#"
            index-strategy = "unsafe-best-match"
        "#
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
        "#
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
        "#
    )]
    pub prerelease: Option<PreReleaseMode>,
    /// Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.
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
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
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
        "#
    )]
    pub link_mode: Option<LinkMode>,
    /// Compile Python files to bytecode after installation.
    ///
    /// By default, does not compile Python (`.py`) files to bytecode (`__pycache__/*.pyc`), instead
    /// Python lazily does the compilation the first time a module is imported. In cases where the
    /// first start time matters, such as CLI applications and docker containers, this option can
    /// trade longer install time for faster startup.
    ///
    /// The compile option will process the entire site-packages directory for consistency and
    /// (like pip) ignore all errors.
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
    /// Reinstall all packages, regardless of whether they're already installed.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            reinstall = true
        "#
    )]
    pub reinstall: Option<bool>,
    /// Reinstall a specific package, regardless of whether it's already installed.
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
    pub python: Option<String>,
    pub system: Option<bool>,
    pub break_system_packages: Option<bool>,
    pub target: Option<PathBuf>,
    pub prefix: Option<PathBuf>,
    pub index_url: Option<IndexUrl>,
    pub extra_index_url: Option<Vec<IndexUrl>>,
    pub no_index: Option<bool>,
    pub find_links: Option<Vec<FlatIndexLocation>>,
    pub index_strategy: Option<IndexStrategy>,
    pub keyring_provider: Option<KeyringProviderType>,
    pub no_build: Option<bool>,
    pub no_binary: Option<Vec<PackageNameSpecifier>>,
    pub only_binary: Option<Vec<PackageNameSpecifier>>,
    pub no_build_isolation: Option<bool>,
    pub strict: Option<bool>,
    pub extra: Option<Vec<ExtraName>>,
    pub all_extras: Option<bool>,
    pub no_deps: Option<bool>,
    pub allow_empty_requirements: Option<bool>,
    pub resolution: Option<ResolutionMode>,
    pub prerelease: Option<PreReleaseMode>,
    pub output_file: Option<PathBuf>,
    pub no_strip_extras: Option<bool>,
    pub no_strip_markers: Option<bool>,
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
    pub custom_compile_command: Option<String>,
    pub generate_hashes: Option<bool>,
    pub legacy_setup_py: Option<bool>,
    pub config_settings: Option<ConfigSettings>,
    pub python_version: Option<PythonVersion>,
    pub python_platform: Option<TargetTriple>,
    pub universal: Option<bool>,
    pub exclude_newer: Option<ExcludeNewer>,
    pub no_emit_package: Option<Vec<PackageName>>,
    pub emit_index_url: Option<bool>,
    pub emit_find_links: Option<bool>,
    pub emit_build_options: Option<bool>,
    pub emit_marker_expression: Option<bool>,
    pub emit_index_annotation: Option<bool>,
    pub annotation_style: Option<AnnotationStyle>,
    pub link_mode: Option<LinkMode>,
    pub compile_bytecode: Option<bool>,
    pub require_hashes: Option<bool>,
    pub upgrade: Option<bool>,
    pub upgrade_package: Option<Vec<Requirement<VerbatimParsedUrl>>>,
    pub reinstall: Option<bool>,
    pub reinstall_package: Option<Vec<PackageName>>,
    pub concurrent_downloads: Option<NonZeroUsize>,
    pub concurrent_builds: Option<NonZeroUsize>,
    pub concurrent_installs: Option<NonZeroUsize>,
}
