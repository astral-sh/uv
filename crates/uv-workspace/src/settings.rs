use std::{num::NonZeroUsize, path::PathBuf};

use serde::Deserialize;

use distribution_types::{FlatIndexLocation, IndexUrl};
use install_wheel_rs::linker::LinkMode;
use uv_configuration::{
    ConfigSettings, IndexStrategy, KeyringProviderType, PackageNameSpecifier, TargetTriple,
};
use uv_interpreter::PythonVersion;
use uv_normalize::{ExtraName, PackageName};
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
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    pub native_tls: Option<bool>,
    pub no_cache: Option<bool>,
    pub preview: Option<bool>,
    pub cache_dir: Option<PathBuf>,
    pub pip: Option<PipOptions>,
}

/// A `[tool.uv.pip]` section.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PipOptions {
    pub python: Option<String>,
    pub system: Option<bool>,
    pub break_system_packages: Option<bool>,
    pub target: Option<PathBuf>,
    pub offline: Option<bool>,
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
    pub no_annotate: Option<bool>,
    pub no_header: Option<bool>,
    pub custom_compile_command: Option<String>,
    pub generate_hashes: Option<bool>,
    pub legacy_setup_py: Option<bool>,
    pub config_settings: Option<ConfigSettings>,
    pub python_version: Option<PythonVersion>,
    pub python_platform: Option<TargetTriple>,
    pub exclude_newer: Option<ExcludeNewer>,
    pub no_emit_package: Option<Vec<PackageName>>,
    pub emit_index_url: Option<bool>,
    pub emit_find_links: Option<bool>,
    pub emit_marker_expression: Option<bool>,
    pub emit_index_annotation: Option<bool>,
    pub annotation_style: Option<AnnotationStyle>,
    pub link_mode: Option<LinkMode>,
    pub compile_bytecode: Option<bool>,
    pub require_hashes: Option<bool>,
    pub concurrent_downloads: Option<NonZeroUsize>,
    pub concurrent_builds: Option<NonZeroUsize>,
    pub concurrent_installs: Option<NonZeroUsize>,
}
