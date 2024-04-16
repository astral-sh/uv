use std::path::PathBuf;

use serde::Deserialize;

use distribution_types::{FlatIndexLocation, IndexUrl};
use install_wheel_rs::linker::LinkMode;
use uv_configuration::{ConfigSettings, IndexStrategy, KeyringProviderType, PackageNameSpecifier};
use uv_normalize::PackageName;
use uv_resolver::{AnnotationStyle, ExcludeNewer, PreReleaseMode, ResolutionMode};
use uv_toolchain::PythonVersion;

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
pub struct Options {
    pub quiet: Option<bool>,
    pub verbose: Option<bool>,
    pub native_tls: Option<bool>,
    pub no_cache: bool,
    pub cache_dir: Option<PathBuf>,
    pub pip: Option<PipOptions>,
}

/// A `[tool.uv.pip]` section.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct PipOptions {
    pub system: Option<bool>,
    pub offline: Option<bool>,
    pub index_url: Option<IndexUrl>,
    pub extra_index_url: Option<IndexUrl>,
    pub no_index: Option<bool>,
    pub find_links: Option<Vec<FlatIndexLocation>>,
    pub index_strategy: Option<IndexStrategy>,
    pub keyring_provider: Option<KeyringProviderType>,
    pub no_build: Option<bool>,
    pub no_binary: Option<Vec<PackageNameSpecifier>>,
    pub only_binary: Option<Vec<PackageNameSpecifier>>,
    pub no_build_isolation: Option<bool>,
    pub resolver: Option<ResolverOptions>,
    pub installer: Option<InstallerOptions>,
}

/// A `[tool.uv.pip.resolver]` section.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ResolverOptions {
    pub resolution: Option<ResolutionMode>,
    pub prerelease: Option<PreReleaseMode>,
    pub no_strip_extras: Option<bool>,
    pub no_annotate: Option<bool>,
    pub no_header: Option<bool>,
    pub generate_hashes: Option<bool>,
    pub legacy_setup_py: Option<bool>,
    pub config_setting: Option<ConfigSettings>,
    pub python_version: Option<PythonVersion>,
    pub exclude_newer: Option<ExcludeNewer>,
    pub no_emit_package: Option<Vec<PackageName>>,
    pub emit_index_url: Option<bool>,
    pub emit_find_links: Option<bool>,
    pub annotation_style: Option<AnnotationStyle>,
}

/// A `[tool.uv.pip.installer]` section.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct InstallerOptions {
    pub link_mode: Option<LinkMode>,
    pub compile_bytecode: Option<bool>,
}
