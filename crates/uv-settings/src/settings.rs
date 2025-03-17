use std::{fmt::Debug, num::NonZeroUsize, path::Path, path::PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

use uv_cache_info::CacheKey;
use uv_configuration::{
    ConfigSettings, IndexStrategy, KeyringProviderType, PackageNameSpecifier, RequiredVersion,
    TargetTriple, TrustedHost, TrustedPublishing,
};
use uv_distribution_types::{
    Index, IndexUrl, IndexUrlError, PipExtraIndex, PipFindLinks, PipIndex, StaticMetadata,
};
use uv_install_wheel::LinkMode;
use uv_macros::{CombineOptions, OptionsMetadata};
use uv_normalize::{ExtraName, PackageName, PipGroupName};
use uv_pep508::Requirement;
use uv_pypi_types::{SupportedEnvironments, VerbatimParsedUrl};
use uv_python::{PythonDownloads, PythonPreference, PythonVersion};
use uv_resolver::{AnnotationStyle, ExcludeNewer, ForkStrategy, PrereleaseMode, ResolutionMode};
use uv_static::EnvVars;

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
#[serde(from = "OptionsWire", rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[serde(flatten)]
    pub globals: GlobalOptions,

    #[serde(flatten)]
    pub top_level: ResolverInstallerOptions,

    #[serde(flatten)]
    pub install_mirrors: PythonInstallMirrors,

    #[serde(flatten)]
    pub publish: PublishOptions,

    #[option_group]
    pub pip: Option<PipOptions>,

    /// The keys to consider when caching builds for the project.
    ///
    /// Cache keys enable you to specify the files or directories that should trigger a rebuild when
    /// modified. By default, uv will rebuild a project whenever the `pyproject.toml`, `setup.py`,
    /// or `setup.cfg` files in the project directory are modified, or if a `src` directory is
    /// added or removed, i.e.:
    ///
    /// ```toml
    /// cache-keys = [{ file = "pyproject.toml" }, { file = "setup.py" }, { file = "setup.cfg" }, { dir = "src" }]
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
    /// `setuptools_scm` to read its version from a Git commit, you can specify `cache-keys = [{ git = { commit = true }, { file = "pyproject.toml" }]`
    /// to include the current Git commit hash in the cache key (in addition to the
    /// `pyproject.toml`). Git tags are also supported via `cache-keys = [{ git = { commit = true, tags = true } }]`.
    ///
    /// Cache keys can also include environment variables. For example, if a project relies on
    /// `MACOSX_DEPLOYMENT_TARGET` or other environment variables to determine its behavior, you can
    /// specify `cache-keys = [{ env = "MACOSX_DEPLOYMENT_TARGET" }]` to invalidate the cache
    /// whenever the environment variable changes.
    ///
    /// Cache keys only affect the project defined by the `pyproject.toml` in which they're
    /// specified (as opposed to, e.g., affecting all members in a workspace), and all paths and
    /// globs are interpreted as relative to the project directory.
    #[option(
        default = r#"[{ file = "pyproject.toml" }, { file = "setup.py" }, { file = "setup.cfg" }]"#,
        value_type = "list[dict]",
        example = r#"
            cache-keys = [{ file = "pyproject.toml" }, { file = "requirements.txt" }, { git = { commit = true } }]
        "#
    )]
    cache_keys: Option<Vec<CacheKey>>,

    // NOTE(charlie): These fields are shared with `ToolUv` in
    // `crates/uv-workspace/src/pyproject.rs`. The documentation lives on that struct.
    // They're respected in both `pyproject.toml` and `uv.toml` files.
    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub override_dependencies: Option<Vec<Requirement<VerbatimParsedUrl>>>,

    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub constraint_dependencies: Option<Vec<Requirement<VerbatimParsedUrl>>>,

    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub build_constraint_dependencies: Option<Vec<Requirement<VerbatimParsedUrl>>>,

    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub environments: Option<SupportedEnvironments>,

    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub required_environments: Option<SupportedEnvironments>,

    // NOTE(charlie): These fields should be kept in-sync with `ToolUv` in
    // `crates/uv-workspace/src/pyproject.rs`. The documentation lives on that struct.
    // They're only respected in `pyproject.toml` files, and should be rejected in `uv.toml` files.
    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub conflicts: Option<serde::de::IgnoredAny>,

    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub workspace: Option<serde::de::IgnoredAny>,

    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub sources: Option<serde::de::IgnoredAny>,

    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub dev_dependencies: Option<serde::de::IgnoredAny>,

    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub default_groups: Option<serde::de::IgnoredAny>,

    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub managed: Option<serde::de::IgnoredAny>,

    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub r#package: Option<serde::de::IgnoredAny>,
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

    /// Resolve the [`Options`] relative to the given root directory.
    pub fn relative_to(self, root_dir: &Path) -> Result<Self, IndexUrlError> {
        Ok(Self {
            top_level: self.top_level.relative_to(root_dir)?,
            pip: self.pip.map(|pip| pip.relative_to(root_dir)).transpose()?,
            ..self
        })
    }
}

/// Global settings, relevant to all invocations.
#[derive(Debug, Clone, Default, Deserialize, CombineOptions, OptionsMetadata)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GlobalOptions {
    /// Enforce a requirement on the version of uv.
    ///
    /// If the version of uv does not meet the requirement at runtime, uv will exit
    /// with an error.
    ///
    /// Accepts a [PEP 440](https://peps.python.org/pep-0440/) specifier, like `==0.5.0` or `>=0.5.0`.
    #[option(
        default = "null",
        value_type = "str",
        example = r#"
            required-version = ">=0.5.0"
        "#
    )]
    pub required_version: Option<RequiredVersion>,
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
    /// Defaults to `$XDG_CACHE_HOME/uv` or `$HOME/.cache/uv` on Linux and macOS, and
    /// `%LOCALAPPDATA%\uv\cache` on Windows.
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
}

/// Settings relevant to all installer operations.
#[derive(Debug, Clone, Default, CombineOptions)]
pub struct InstallerOptions {
    pub index: Option<Vec<Index>>,
    pub index_url: Option<PipIndex>,
    pub extra_index_url: Option<Vec<PipExtraIndex>>,
    pub no_index: Option<bool>,
    pub find_links: Option<Vec<PipFindLinks>>,
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
    pub no_build_isolation: Option<bool>,
    pub no_sources: Option<bool>,
}

/// Settings relevant to all resolver operations.
#[derive(Debug, Clone, Default, CombineOptions)]
pub struct ResolverOptions {
    pub index: Option<Vec<Index>>,
    pub index_url: Option<PipIndex>,
    pub extra_index_url: Option<Vec<PipExtraIndex>>,
    pub no_index: Option<bool>,
    pub find_links: Option<Vec<PipFindLinks>>,
    pub index_strategy: Option<IndexStrategy>,
    pub keyring_provider: Option<KeyringProviderType>,
    pub resolution: Option<ResolutionMode>,
    pub prerelease: Option<PrereleaseMode>,
    pub fork_strategy: Option<ForkStrategy>,
    pub dependency_metadata: Option<Vec<StaticMetadata>>,
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, CombineOptions, OptionsMetadata)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ResolverInstallerOptions {
    /// The package indexes to use when resolving dependencies.
    ///
    /// Accepts either a repository compliant with [PEP 503](https://peps.python.org/pep-0503/)
    /// (the simple repository API), or a local directory laid out in the same format.
    ///
    /// Indexes are considered in the order in which they're defined, such that the first-defined
    /// index has the highest priority. Further, the indexes provided by this setting are given
    /// higher priority than any indexes specified via [`index_url`](#index-url) or
    /// [`extra_index_url`](#extra-index-url). uv will only consider the first index that contains
    /// a given package, unless an alternative [index strategy](#index-strategy) is specified.
    ///
    /// If an index is marked as `explicit = true`, it will be used exclusively for those
    /// dependencies that select it explicitly via `[tool.uv.sources]`, as in:
    ///
    /// ```toml
    /// [[tool.uv.index]]
    /// name = "pytorch"
    /// url = "https://download.pytorch.org/whl/cu121"
    /// explicit = true
    ///
    /// [tool.uv.sources]
    /// torch = { index = "pytorch" }
    /// ```
    ///
    /// If an index is marked as `default = true`, it will be moved to the end of the prioritized list, such that it is
    /// given the lowest priority when resolving packages. Additionally, marking an index as default will disable the
    /// PyPI default index.
    #[option(
        default = "\"[]\"",
        value_type = "dict",
        example = r#"
            [[tool.uv.index]]
            name = "pytorch"
            url = "https://download.pytorch.org/whl/cu121"
        "#
    )]
    pub index: Option<Vec<Index>>,
    /// The URL of the Python package index (by default: <https://pypi.org/simple>).
    ///
    /// Accepts either a repository compliant with [PEP 503](https://peps.python.org/pep-0503/)
    /// (the simple repository API), or a local directory laid out in the same format.
    ///
    /// The index provided by this setting is given lower priority than any indexes specified via
    /// [`extra_index_url`](#extra-index-url) or [`index`](#index).
    ///
    /// (Deprecated: use `index` instead.)
    #[option(
        default = "\"https://pypi.org/simple\"",
        value_type = "str",
        example = r#"
            index-url = "https://test.pypi.org/simple"
        "#
    )]
    pub index_url: Option<PipIndex>,
    /// Extra URLs of package indexes to use, in addition to `--index-url`.
    ///
    /// Accepts either a repository compliant with [PEP 503](https://peps.python.org/pep-0503/)
    /// (the simple repository API), or a local directory laid out in the same format.
    ///
    /// All indexes provided via this flag take priority over the index specified by
    /// [`index_url`](#index-url) or [`index`](#index) with `default = true`. When multiple indexes
    /// are provided, earlier values take priority.
    ///
    /// To control uv's resolution strategy when multiple indexes are present, see
    /// [`index_strategy`](#index-strategy).
    ///
    /// (Deprecated: use `index` instead.)
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            extra-index-url = ["https://download.pytorch.org/whl/cpu"]
        "#
    )]
    pub extra_index_url: Option<Vec<PipExtraIndex>>,
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
    pub find_links: Option<Vec<PipFindLinks>>,
    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-index`). This prevents
    /// "dependency confusion" attacks, whereby an attacker can upload a malicious package under the
    /// same name to an alternate index.
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
    pub prerelease: Option<PrereleaseMode>,
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
    #[option(
        default = "\"requires-python\"",
        value_type = "str",
        example = r#"
            fork-strategy = "fewest"
        "#,
        possible_values = true
    )]
    pub fork_strategy: Option<ForkStrategy>,
    /// Pre-defined static metadata for dependencies of the project (direct or transitive). When
    /// provided, enables the resolver to use the specified metadata instead of querying the
    /// registry or building the relevant package from source.
    ///
    /// Metadata should be provided in adherence with the [Metadata 2.3](https://packaging.python.org/en/latest/specifications/core-metadata/)
    /// standard, though only the following fields are respected:
    ///
    /// - `name`: The name of the package.
    /// - (Optional) `version`: The version of the package. If omitted, the metadata will be applied
    ///   to all versions of the package.
    /// - (Optional) `requires-dist`: The dependencies of the package (e.g., `werkzeug>=0.14`).
    /// - (Optional) `requires-python`: The Python version required by the package (e.g., `>=3.10`).
    /// - (Optional) `provides-extras`: The extras provided by the package.
    #[option(
        default = r#"[]"#,
        value_type = "list[dict]",
        example = r#"
            dependency-metadata = [
                { name = "flask", version = "1.0.0", requires-dist = ["werkzeug"], requires-python = ">=3.6" },
            ]
        "#
    )]
    pub dependency_metadata: Option<Vec<StaticMetadata>>,
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
        value_type = "list[str]",
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

impl ResolverInstallerOptions {
    /// Resolve the [`ResolverInstallerOptions`] relative to the given root directory.
    pub fn relative_to(self, root_dir: &Path) -> Result<Self, IndexUrlError> {
        Ok(Self {
            index: self
                .index
                .map(|index| {
                    index
                        .into_iter()
                        .map(|index| index.relative_to(root_dir))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?,
            index_url: self
                .index_url
                .map(|index_url| index_url.relative_to(root_dir))
                .transpose()?,
            extra_index_url: self
                .extra_index_url
                .map(|extra_index_url| {
                    extra_index_url
                        .into_iter()
                        .map(|extra_index_url| extra_index_url.relative_to(root_dir))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?,
            find_links: self
                .find_links
                .map(|find_links| {
                    find_links
                        .into_iter()
                        .map(|find_link| find_link.relative_to(root_dir))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?,
            ..self
        })
    }
}

/// Shared settings, relevant to all operations that might create managed python installations.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, CombineOptions, OptionsMetadata)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PythonInstallMirrors {
    /// Mirror URL for downloading managed Python installations.
    ///
    /// By default, managed Python installations are downloaded from [`python-build-standalone`](https://github.com/astral-sh/python-build-standalone).
    /// This variable can be set to a mirror URL to use a different source for Python installations.
    /// The provided URL will replace `https://github.com/astral-sh/python-build-standalone/releases/download` in, e.g., `https://github.com/astral-sh/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-aarch64-apple-darwin-install_only.tar.gz`.
    ///
    /// Distributions can be read from a local directory by using the `file://` URL scheme.
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            python-install-mirror = "https://github.com/astral-sh/python-build-standalone/releases/download"
        "#
    )]
    pub python_install_mirror: Option<String>,
    /// Mirror URL to use for downloading managed PyPy installations.
    ///
    /// By default, managed PyPy installations are downloaded from [downloads.python.org](https://downloads.python.org/).
    /// This variable can be set to a mirror URL to use a different source for PyPy installations.
    /// The provided URL will replace `https://downloads.python.org/pypy` in, e.g., `https://downloads.python.org/pypy/pypy3.8-v7.3.7-osx64.tar.bz2`.
    ///
    /// Distributions can be read from a
    /// local directory by using the `file://` URL scheme.
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            pypy-install-mirror = "https://downloads.python.org/pypy"
        "#
    )]
    pub pypy_install_mirror: Option<String>,
}

impl Default for PythonInstallMirrors {
    fn default() -> Self {
        PythonInstallMirrors::resolve(None, None)
    }
}

impl PythonInstallMirrors {
    pub fn resolve(python_mirror: Option<String>, pypy_mirror: Option<String>) -> Self {
        let python_mirror_env = std::env::var(EnvVars::UV_PYTHON_INSTALL_MIRROR).ok();
        let pypy_mirror_env = std::env::var(EnvVars::UV_PYPY_INSTALL_MIRROR).ok();
        PythonInstallMirrors {
            python_install_mirror: python_mirror_env.or(python_mirror),
            pypy_install_mirror: pypy_mirror_env.or(pypy_mirror),
        }
    }
}

/// Settings that are specific to the `uv pip` command-line interface.
///
/// These values will be ignored when running commands outside the `uv pip` namespace (e.g.,
/// `uv lock`, `uvx`).
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
    #[serde(skip)]
    #[cfg_attr(feature = "schemars", schemars(skip))]
    pub index: Option<Vec<Index>>,
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
    pub index_url: Option<PipIndex>,
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
    pub extra_index_url: Option<Vec<PipExtraIndex>>,
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
    pub find_links: Option<Vec<PipFindLinks>>,
    /// The strategy to use when resolving against multiple index URLs.
    ///
    /// By default, uv will stop at the first index on which a given package is available, and
    /// limit resolutions to those present on that first index (`first-index`). This prevents
    /// "dependency confusion" attacks, whereby an attacker can upload a malicious package under the
    /// same name to an alternate index.
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
    /// Disable isolation when building source distributions for a specific package.
    ///
    /// Assumes that the packages' build dependencies specified by [PEP 518](https://peps.python.org/pep-0518/)
    /// are already installed.
    #[option(
        default = "[]",
        value_type = "list[str]",
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
    /// Include optional dependencies from the specified extra; may be provided more than once.
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
    /// Exclude the specified optional dependencies if `all-extras` is supplied.
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            all-extras = true
            no-extra = ["dev", "docs"]
        "#
    )]
    pub no_extra: Option<Vec<ExtraName>>,
    /// Ignore package dependencies, instead only add those packages explicitly listed
    /// on the command line to the resulting requirements file.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            no-deps = true
        "#
    )]
    pub no_deps: Option<bool>,
    /// Include the following dependency groups.
    #[option(
        default = "None",
        value_type = "list[str]",
        example = r#"
            group = ["dev", "docs"]
        "#
    )]
    pub group: Option<Vec<PipGroupName>>,
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
    #[option(
        default = "\"requires-python\"",
        value_type = "str",
        example = r#"
            fork-strategy = "fewest"
        "#,
        possible_values = true
    )]
    pub fork_strategy: Option<ForkStrategy>,
    /// Pre-defined static metadata for dependencies of the project (direct or transitive). When
    /// provided, enables the resolver to use the specified metadata instead of querying the
    /// registry or building the relevant package from source.
    ///
    /// Metadata should be provided in adherence with the [Metadata 2.3](https://packaging.python.org/en/latest/specifications/core-metadata/)
    /// standard, though only the following fields are respected:
    ///
    /// - `name`: The name of the package.
    /// - (Optional) `version`: The version of the package. If omitted, the metadata will be applied
    ///   to all versions of the package.
    /// - (Optional) `requires-dist`: The dependencies of the package (e.g., `werkzeug>=0.14`).
    /// - (Optional) `requires-python`: The Python version required by the package (e.g., `>=3.10`).
    /// - (Optional) `provides-extras`: The extras provided by the package.
    #[option(
        default = r#"[]"#,
        value_type = "list[dict]",
        example = r#"
            dependency-metadata = [
                { name = "flask", version = "1.0.0", requires-dist = ["werkzeug"], requires-python = ">=3.6" },
            ]
        "#
    )]
    pub dependency_metadata: Option<Vec<StaticMetadata>>,
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
    /// Limit candidate packages to those that were uploaded prior to a given point in time.
    ///
    /// Accepts a superset of [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) (e.g.,
    /// `2006-12-02T02:07:43Z`). A full timestamp is required to ensure that the resolver will
    /// behave consistently across timezones.
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            exclude-newer = "2006-12-02T02:07:43Z"
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
        default = "true",
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

impl PipOptions {
    /// Resolve the [`PipOptions`] relative to the given root directory.
    pub fn relative_to(self, root_dir: &Path) -> Result<Self, IndexUrlError> {
        Ok(Self {
            index: self
                .index
                .map(|index| {
                    index
                        .into_iter()
                        .map(|index| index.relative_to(root_dir))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?,
            index_url: self
                .index_url
                .map(|index_url| index_url.relative_to(root_dir))
                .transpose()?,
            extra_index_url: self
                .extra_index_url
                .map(|extra_index_url| {
                    extra_index_url
                        .into_iter()
                        .map(|extra_index_url| extra_index_url.relative_to(root_dir))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?,
            find_links: self
                .find_links
                .map(|find_links| {
                    find_links
                        .into_iter()
                        .map(|find_link| find_link.relative_to(root_dir))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?,
            ..self
        })
    }
}

impl From<ResolverInstallerOptions> for ResolverOptions {
    fn from(value: ResolverInstallerOptions) -> Self {
        Self {
            index: value.index,
            index_url: value.index_url,
            extra_index_url: value.extra_index_url,
            no_index: value.no_index,
            find_links: value.find_links,
            index_strategy: value.index_strategy,
            keyring_provider: value.keyring_provider,
            resolution: value.resolution,
            prerelease: value.prerelease,
            fork_strategy: value.fork_strategy,
            dependency_metadata: value.dependency_metadata,
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
            index: value.index,
            index_url: value.index_url,
            extra_index_url: value.extra_index_url,
            no_index: value.no_index,
            find_links: value.find_links,
            index_strategy: value.index_strategy,
            keyring_provider: value.keyring_provider,
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
    pub index: Option<Vec<Index>>,
    pub index_url: Option<PipIndex>,
    pub extra_index_url: Option<Vec<PipExtraIndex>>,
    pub no_index: Option<bool>,
    pub find_links: Option<Vec<PipFindLinks>>,
    pub index_strategy: Option<IndexStrategy>,
    pub keyring_provider: Option<KeyringProviderType>,
    pub resolution: Option<ResolutionMode>,
    pub prerelease: Option<PrereleaseMode>,
    pub fork_strategy: Option<ForkStrategy>,
    pub dependency_metadata: Option<Vec<StaticMetadata>>,
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
            index: value.index,
            index_url: value.index_url,
            extra_index_url: value.extra_index_url,
            no_index: value.no_index,
            find_links: value.find_links,
            index_strategy: value.index_strategy,
            keyring_provider: value.keyring_provider,
            resolution: value.resolution,
            prerelease: value.prerelease,
            fork_strategy: value.fork_strategy,
            dependency_metadata: value.dependency_metadata,
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
            index: value.index,
            index_url: value.index_url,
            extra_index_url: value.extra_index_url,
            no_index: value.no_index,
            find_links: value.find_links,
            index_strategy: value.index_strategy,
            keyring_provider: value.keyring_provider,
            resolution: value.resolution,
            prerelease: value.prerelease,
            fork_strategy: value.fork_strategy,
            dependency_metadata: value.dependency_metadata,
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

/// Like [`Options]`, but with any `#[serde(flatten)]` fields inlined. This leads to far, far
/// better error messages when deserializing.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct OptionsWire {
    // #[serde(flatten)]
    // globals: GlobalOptions
    required_version: Option<RequiredVersion>,
    native_tls: Option<bool>,
    offline: Option<bool>,
    no_cache: Option<bool>,
    cache_dir: Option<PathBuf>,
    preview: Option<bool>,
    python_preference: Option<PythonPreference>,
    python_downloads: Option<PythonDownloads>,
    concurrent_downloads: Option<NonZeroUsize>,
    concurrent_builds: Option<NonZeroUsize>,
    concurrent_installs: Option<NonZeroUsize>,

    // #[serde(flatten)]
    // top_level: ResolverInstallerOptions
    index: Option<Vec<Index>>,
    index_url: Option<PipIndex>,
    extra_index_url: Option<Vec<PipExtraIndex>>,
    no_index: Option<bool>,
    find_links: Option<Vec<PipFindLinks>>,
    index_strategy: Option<IndexStrategy>,
    keyring_provider: Option<KeyringProviderType>,
    allow_insecure_host: Option<Vec<TrustedHost>>,
    resolution: Option<ResolutionMode>,
    prerelease: Option<PrereleaseMode>,
    fork_strategy: Option<ForkStrategy>,
    dependency_metadata: Option<Vec<StaticMetadata>>,
    config_settings: Option<ConfigSettings>,
    no_build_isolation: Option<bool>,
    no_build_isolation_package: Option<Vec<PackageName>>,
    exclude_newer: Option<ExcludeNewer>,
    link_mode: Option<LinkMode>,
    compile_bytecode: Option<bool>,
    no_sources: Option<bool>,
    upgrade: Option<bool>,
    upgrade_package: Option<Vec<Requirement<VerbatimParsedUrl>>>,
    reinstall: Option<bool>,
    reinstall_package: Option<Vec<PackageName>>,
    no_build: Option<bool>,
    no_build_package: Option<Vec<PackageName>>,
    no_binary: Option<bool>,
    no_binary_package: Option<Vec<PackageName>>,

    // #[serde(flatten)]
    // install_mirror: PythonInstallMirrors,
    python_install_mirror: Option<String>,
    pypy_install_mirror: Option<String>,

    // #[serde(flatten)]
    // publish: PublishOptions
    publish_url: Option<Url>,
    trusted_publishing: Option<TrustedPublishing>,
    check_url: Option<IndexUrl>,

    pip: Option<PipOptions>,
    cache_keys: Option<Vec<CacheKey>>,

    // NOTE(charlie): These fields are shared with `ToolUv` in
    // `crates/uv-workspace/src/pyproject.rs`. The documentation lives on that struct.
    // They're respected in both `pyproject.toml` and `uv.toml` files.
    override_dependencies: Option<Vec<Requirement<VerbatimParsedUrl>>>,
    constraint_dependencies: Option<Vec<Requirement<VerbatimParsedUrl>>>,
    build_constraint_dependencies: Option<Vec<Requirement<VerbatimParsedUrl>>>,
    environments: Option<SupportedEnvironments>,
    required_environments: Option<SupportedEnvironments>,

    // NOTE(charlie): These fields should be kept in-sync with `ToolUv` in
    // `crates/uv-workspace/src/pyproject.rs`. The documentation lives on that struct.
    // They're only respected in `pyproject.toml` files, and should be rejected in `uv.toml` files.
    conflicts: Option<serde::de::IgnoredAny>,
    workspace: Option<serde::de::IgnoredAny>,
    sources: Option<serde::de::IgnoredAny>,
    managed: Option<serde::de::IgnoredAny>,
    r#package: Option<serde::de::IgnoredAny>,
    default_groups: Option<serde::de::IgnoredAny>,
    dev_dependencies: Option<serde::de::IgnoredAny>,

    // Build backend
    #[allow(dead_code)]
    build_backend: Option<serde::de::IgnoredAny>,
}

impl From<OptionsWire> for Options {
    fn from(value: OptionsWire) -> Self {
        let OptionsWire {
            required_version,
            native_tls,
            offline,
            no_cache,
            cache_dir,
            preview,
            python_preference,
            python_downloads,
            python_install_mirror,
            pypy_install_mirror,
            concurrent_downloads,
            concurrent_builds,
            concurrent_installs,
            index,
            index_url,
            extra_index_url,
            no_index,
            find_links,
            index_strategy,
            keyring_provider,
            allow_insecure_host,
            resolution,
            prerelease,
            fork_strategy,
            dependency_metadata,
            config_settings,
            no_build_isolation,
            no_build_isolation_package,
            exclude_newer,
            link_mode,
            compile_bytecode,
            no_sources,
            upgrade,
            upgrade_package,
            reinstall,
            reinstall_package,
            no_build,
            no_build_package,
            no_binary,
            no_binary_package,
            pip,
            cache_keys,
            override_dependencies,
            constraint_dependencies,
            build_constraint_dependencies,
            environments,
            required_environments,
            conflicts,
            publish_url,
            trusted_publishing,
            check_url,
            workspace,
            sources,
            default_groups,
            dev_dependencies,
            managed,
            package,
            // Used by the build backend
            build_backend: _,
        } = value;

        Self {
            globals: GlobalOptions {
                required_version,
                native_tls,
                offline,
                no_cache,
                cache_dir,
                preview,
                python_preference,
                python_downloads,
                concurrent_downloads,
                concurrent_builds,
                concurrent_installs,
                // Used twice for backwards compatibility
                allow_insecure_host: allow_insecure_host.clone(),
            },
            top_level: ResolverInstallerOptions {
                index,
                index_url,
                extra_index_url,
                no_index,
                find_links,
                index_strategy,
                keyring_provider,
                resolution,
                prerelease,
                fork_strategy,
                dependency_metadata,
                config_settings,
                no_build_isolation,
                no_build_isolation_package,
                exclude_newer,
                link_mode,
                compile_bytecode,
                no_sources,
                upgrade,
                upgrade_package,
                reinstall,
                reinstall_package,
                no_build,
                no_build_package,
                no_binary,
                no_binary_package,
            },
            pip,
            cache_keys,
            override_dependencies,
            constraint_dependencies,
            build_constraint_dependencies,
            environments,
            required_environments,
            install_mirrors: PythonInstallMirrors::resolve(
                python_install_mirror,
                pypy_install_mirror,
            ),
            conflicts,
            publish: PublishOptions {
                publish_url,
                trusted_publishing,
                check_url,
            },
            workspace,
            sources,
            dev_dependencies,
            default_groups,
            managed,
            package,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, CombineOptions, OptionsMetadata)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PublishOptions {
    /// The URL for publishing packages to the Python package index (by default:
    /// <https://upload.pypi.org/legacy/>).
    #[option(
        default = "\"https://upload.pypi.org/legacy/\"",
        value_type = "str",
        example = r#"
            publish-url = "https://test.pypi.org/legacy/"
        "#
    )]
    pub publish_url: Option<Url>,

    /// Configure trusted publishing via GitHub Actions.
    ///
    /// By default, uv checks for trusted publishing when running in GitHub Actions, but ignores it
    /// if it isn't configured or the workflow doesn't have enough permissions (e.g., a pull request
    /// from a fork).
    #[option(
        default = "automatic",
        value_type = "str",
        example = r#"
            trusted-publishing = "always"
        "#
    )]
    pub trusted_publishing: Option<TrustedPublishing>,

    /// Check an index URL for existing files to skip duplicate uploads.
    ///
    /// This option allows retrying publishing that failed after only some, but not all files have
    /// been uploaded, and handles error due to parallel uploads of the same file.
    ///
    /// Before uploading, the index is checked. If the exact same file already exists in the index,
    /// the file will not be uploaded. If an error occurred during the upload, the index is checked
    /// again, to handle cases where the identical file was uploaded twice in parallel.
    ///
    /// The exact behavior will vary based on the index. When uploading to PyPI, uploading the same
    /// file succeeds even without `--check-url`, while most other indexes error.
    ///
    /// The index must provide one of the supported hashes (SHA-256, SHA-384, or SHA-512).
    #[option(
        default = "None",
        value_type = "str",
        example = r#"
            check-url = "https://test.pypi.org/simple"
        "#
    )]
    pub check_url: Option<IndexUrl>,
}
