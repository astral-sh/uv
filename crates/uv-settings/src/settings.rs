use std::{fmt::Debug, num::NonZeroUsize, path::PathBuf};

use serde::Deserialize;

use distribution_types::{FlatIndexLocation, IndexUrl};
use install_wheel_rs::linker::LinkMode;
use pypi_types::VerbatimParsedUrl;
use uv_configuration::{
    ConfigSettings, IndexStrategy, KeyringProviderType, PackageNameSpecifier, TargetTriple,
};
use uv_macros::CombineOptions;
use uv_normalize::{ExtraName, PackageName};
use uv_resolver::{AnnotationStyle, ExcludeNewer, PreReleaseMode, ResolutionMode};
use uv_toolchain::{PythonVersion, ToolchainPreference};

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
#[derive(Debug, Clone, Default, Deserialize, CombineOptions)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[serde(flatten)]
    pub globals: GlobalOptions,
    #[serde(flatten)]
    pub top_level: ResolverInstallerOptions,
    pub pip: Option<PipOptions>,
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<Vec<String>>",
            description = "PEP 508 style requirements, e.g. `flask==3.0.0`, or `black @ https://...`."
        )
    )]
    pub override_dependencies: Option<Vec<pep508_rs::Requirement<VerbatimParsedUrl>>>,
}

/// Global settings, relevant to all invocations.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize, CombineOptions)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GlobalOptions {
    pub native_tls: Option<bool>,
    pub offline: Option<bool>,
    pub no_cache: Option<bool>,
    pub cache_dir: Option<PathBuf>,
    pub preview: Option<bool>,
    pub toolchain_preference: Option<ToolchainPreference>,
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
    pub upgrade_package: Option<Vec<PackageName>>,
    pub no_build: Option<bool>,
    pub no_build_package: Option<Vec<PackageName>>,
    pub no_binary: Option<bool>,
    pub no_binary_package: Option<Vec<PackageName>>,
}

/// Shared settings, relevant to all operations that must resolve and install dependencies. The
/// union of [`InstallerOptions`] and [`ResolverOptions`].
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize, CombineOptions)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ResolverInstallerOptions {
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
    pub compile_bytecode: Option<bool>,
    pub upgrade: Option<bool>,
    pub upgrade_package: Option<Vec<PackageName>>,
    pub reinstall: Option<bool>,
    pub reinstall_package: Option<Vec<PackageName>>,
    pub no_build: Option<bool>,
    pub no_build_package: Option<Vec<PackageName>>,
    pub no_binary: Option<bool>,
    pub no_binary_package: Option<Vec<PackageName>>,
}

/// A `[tool.uv.pip]` section.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize, CombineOptions)]
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
    pub resolution: Option<ResolutionMode>,
    pub prerelease: Option<PreReleaseMode>,
    pub output_file: Option<PathBuf>,
    pub no_strip_extras: Option<bool>,
    pub no_strip_markers: Option<bool>,
    pub no_annotate: Option<bool>,
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
    pub upgrade_package: Option<Vec<PackageName>>,
    pub reinstall: Option<bool>,
    pub reinstall_package: Option<Vec<PackageName>>,
    pub concurrent_downloads: Option<NonZeroUsize>,
    pub concurrent_builds: Option<NonZeroUsize>,
    pub concurrent_installs: Option<NonZeroUsize>,
}
