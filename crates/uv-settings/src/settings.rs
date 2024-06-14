use std::{fmt::Debug, num::NonZeroUsize, path::PathBuf};

use serde::Deserialize;

use distribution_types::{FlatIndexLocation, IndexUrl};
use install_wheel_rs::linker::LinkMode;
use pypi_types::VerbatimParsedUrl;
use uv_configuration::{
    ConfigSettings, IndexStrategy, KeyringProviderType, PackageNameSpecifier, TargetTriple,
};
use uv_normalize::{ExtraName, PackageName};
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
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GlobalOptions {
    pub native_tls: Option<bool>,
    pub offline: Option<bool>,
    pub no_cache: Option<bool>,
    pub cache_dir: Option<PathBuf>,
    pub preview: Option<bool>,
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
}

/// Shared settings, relevant to all operations that must resolve and install dependencies. The
/// union of [`InstallerOptions`] and [`ResolverOptions`].
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
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
    pub upgrade: Option<bool>,
    pub upgrade_package: Option<Vec<PackageName>>,
    pub reinstall: Option<bool>,
    pub reinstall_package: Option<Vec<PackageName>>,
    pub concurrent_downloads: Option<NonZeroUsize>,
    pub concurrent_builds: Option<NonZeroUsize>,
    pub concurrent_installs: Option<NonZeroUsize>,
}

impl Options {
    /// Return the `pip` section, with any top-level options merged in. If options are repeated
    /// between the top-level and the `pip` section, the `pip` options are preferred.
    ///
    /// For example, prefers `tool.uv.pip.index-url` over `tool.uv.index-url`.
    pub fn pip(self) -> PipOptions {
        let PipOptions {
            python,
            system,
            break_system_packages,
            target,
            prefix,
            index_url,
            extra_index_url,
            no_index,
            find_links,
            index_strategy,
            keyring_provider,
            no_build,
            no_binary,
            only_binary,
            no_build_isolation,
            strict,
            extra,
            all_extras,
            no_deps,
            resolution,
            prerelease,
            output_file,
            no_strip_extras,
            no_annotate,
            no_header,
            custom_compile_command,
            generate_hashes,
            legacy_setup_py,
            config_settings,
            python_version,
            python_platform,
            exclude_newer,
            no_emit_package,
            emit_index_url,
            emit_find_links,
            emit_marker_expression,
            emit_index_annotation,
            annotation_style,
            link_mode,
            compile_bytecode,
            require_hashes,
            upgrade,
            upgrade_package,
            reinstall,
            reinstall_package,
            concurrent_builds,
            concurrent_downloads,
            concurrent_installs,
        } = self.pip.unwrap_or_default();

        let ResolverInstallerOptions {
            index_url: top_level_index_url,
            extra_index_url: top_level_extra_index_url,
            no_index: top_level_no_index,
            find_links: top_level_find_links,
            index_strategy: top_level_index_strategy,
            keyring_provider: top_level_keyring_provider,
            resolution: top_level_resolution,
            prerelease: top_level_prerelease,
            config_settings: top_level_config_settings,
            exclude_newer: top_level_exclude_newer,
            link_mode: top_level_link_mode,
            compile_bytecode: top_level_compile_bytecode,
            upgrade: top_level_upgrade,
            upgrade_package: top_level_upgrade_package,
            reinstall: top_level_reinstall,
            reinstall_package: top_level_reinstall_package,
        } = self.top_level;

        PipOptions {
            python,
            system,
            break_system_packages,
            target,
            prefix,
            index_url: index_url.or(top_level_index_url),
            extra_index_url: extra_index_url.or(top_level_extra_index_url),
            no_index: no_index.or(top_level_no_index),
            find_links: find_links.or(top_level_find_links),
            index_strategy: index_strategy.or(top_level_index_strategy),
            keyring_provider: keyring_provider.or(top_level_keyring_provider),
            no_build,
            no_binary,
            only_binary,
            no_build_isolation,
            strict,
            extra,
            all_extras,
            no_deps,
            resolution: resolution.or(top_level_resolution),
            prerelease: prerelease.or(top_level_prerelease),
            output_file,
            no_strip_extras,
            no_annotate,
            no_header,
            custom_compile_command,
            generate_hashes,
            legacy_setup_py,
            config_settings: config_settings.or(top_level_config_settings),
            python_version,
            python_platform,
            exclude_newer: exclude_newer.or(top_level_exclude_newer),
            no_emit_package,
            emit_index_url,
            emit_find_links,
            emit_marker_expression,
            emit_index_annotation,
            annotation_style,
            link_mode: link_mode.or(top_level_link_mode),
            compile_bytecode: compile_bytecode.or(top_level_compile_bytecode),
            require_hashes,
            upgrade: upgrade.or(top_level_upgrade),
            upgrade_package: upgrade_package.or(top_level_upgrade_package),
            reinstall: reinstall.or(top_level_reinstall),
            reinstall_package: reinstall_package.or(top_level_reinstall_package),
            concurrent_builds,
            concurrent_downloads,
            concurrent_installs,
        }
    }
}
