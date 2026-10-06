use uv_configuration::{
    BuildIsolation, BuildOptions, ExcludeNewer, ForkStrategy, IndexStrategy, KeyringProviderType,
    NoBinary, NoBuild, NoSources, Prerelease, PrereleaseMode, PrereleasePackage, Reinstall,
    ResolutionMode, Upgrade,
};
use uv_distribution_types::{
    ConfigSettings, DependencyMetadata, ExtraBuildVariables, IndexLocations, PackageConfigSettings,
};
use uv_install_wheel::LinkMode;
use uv_pep440::Version;
use uv_torch::{AmdGpuArchitecture, TorchMode};
use uv_warnings::warn_user_once;
use uv_workspace::pyproject::ExtraBuildDependencies;

use crate::{ResolverInstallerOptions, ResolverOptions};

/// The CLI flag that requested a lock check.
#[derive(Debug, Clone, Copy)]
pub enum LockedFlag {
    Locked,
    Check,
}

impl LockedFlag {
    /// Return the name of the command-line flag without its leading dashes.
    pub fn name(self) -> &'static str {
        match self {
            Self::Locked => "locked",
            Self::Check => "check",
        }
    }
}

impl std::fmt::Display for LockedFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "--{}", self.name())
    }
}

/// The source of a lock check operation.
#[derive(Debug, Clone, Copy)]
pub enum LockedSource {
    /// A lock check was requested on the CLI.
    Cli(LockedFlag),
    /// The `UV_LOCKED` environment variable was set.
    Env,
}

impl std::fmt::Display for LockedSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cli(flag) => flag.fmt(f),
            Self::Env => write!(f, "UV_LOCKED=1"),
        }
    }
}

/// Whether a lockfile check was requested, including the source of the request.
#[derive(Debug, Clone, Copy)]
pub enum LockCheck {
    /// Lockfile check is enabled.
    Enabled(LockedSource),
    /// Lockfile check is disabled.
    Disabled,
}

/// The CLI flag that requested frozen mode.
#[derive(Debug, Clone, Copy)]
pub enum FrozenFlag {
    Frozen,
    CheckExists,
}

impl FrozenFlag {
    /// Return the name of the command-line flag without its leading dashes.
    pub fn name(self) -> &'static str {
        match self {
            Self::Frozen => "frozen",
            Self::CheckExists => "check-exists",
        }
    }
}

impl std::fmt::Display for FrozenFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "--{}", self.name())
    }
}

/// The source of the frozen flag.
#[derive(Debug, Clone, Copy)]
pub enum FrozenSource {
    /// Frozen mode was requested on the CLI.
    Cli(FrozenFlag),
    /// The `UV_FROZEN` environment variable was set.
    Env,
}

impl std::fmt::Display for FrozenSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cli(flag) => flag.fmt(f),
            Self::Env => write!(f, "UV_FROZEN=1"),
        }
    }
}

/// The kinds of Python installations and downloads to list.
#[derive(Debug, Clone, Default)]
pub enum PythonListKinds {
    /// List installed versions and available downloads.
    #[default]
    Default,
    /// Only list version downloads.
    Downloads,
    /// Only list installed versions.
    Installed,
}

/// The resolved settings to use for an invocation of the uv CLI when installing dependencies.
///
/// Borrows the resolved installer options shared by commands.
#[derive(Debug, Clone)]
pub struct InstallerSettingsRef<'a> {
    pub index_locations: &'a IndexLocations,
    pub index_strategy: IndexStrategy,
    pub keyring_provider: KeyringProviderType,
    pub dependency_metadata: &'a DependencyMetadata,
    pub config_setting: &'a ConfigSettings,
    pub config_settings_package: &'a PackageConfigSettings,
    pub build_isolation: &'a BuildIsolation,
    pub extra_build_dependencies: &'a ExtraBuildDependencies,
    pub extra_build_variables: &'a ExtraBuildVariables,
    pub exclude_newer: &'a ExcludeNewer,
    pub link_mode: LinkMode,
    pub compile_bytecode: bool,
    pub reinstall: &'a Reinstall,
    pub build_options: &'a BuildOptions,
    pub sources: NoSources,
}

/// The resolved settings to use for an invocation of the uv CLI when resolving dependencies.
///
/// Constructed from the combined [`ResolverOptions`] for an invocation.
#[derive(Debug, Clone, Default)]
pub struct ResolverSettings {
    pub build_options: BuildOptions,
    pub config_setting: ConfigSettings,
    pub config_settings_package: PackageConfigSettings,
    pub dependency_metadata: DependencyMetadata,
    pub exclude_newer: ExcludeNewer,
    pub fork_strategy: ForkStrategy,
    pub index_locations: IndexLocations,
    pub index_strategy: IndexStrategy,
    pub keyring_provider: KeyringProviderType,
    pub link_mode: LinkMode,
    pub build_isolation: BuildIsolation,
    pub extra_build_dependencies: ExtraBuildDependencies,
    pub extra_build_variables: ExtraBuildVariables,
    pub prerelease: Prerelease,
    pub resolution: ResolutionMode,
    pub sources: NoSources,
    pub torch_backend: Option<TorchMode>,
    pub cuda_driver_version: Option<Version>,
    pub amd_gpu_architecture: Option<AmdGpuArchitecture>,
    pub upgrade: Upgrade,
}

/// Normalize a deprecated prerelease mode and emit its warning.
#[expect(deprecated)]
fn warn_if_deprecated_prerelease_mode(prerelease: PrereleaseMode) -> PrereleaseMode {
    if matches!(prerelease, PrereleaseMode::IfNecessaryOrExplicit) {
        warn_user_once!(
            "The `if-necessary-or-explicit` pre-release mode is deprecated and will be removed in a future release. Use `if-necessary` instead."
        );
        PrereleaseMode::IfNecessary
    } else {
        prerelease
    }
}

/// Normalize the global and per-package prerelease modes, warning about deprecated values.
pub fn resolve_prerelease(global: PrereleaseMode, mut package: PrereleasePackage) -> Prerelease {
    for mode in package.values_mut() {
        *mode = warn_if_deprecated_prerelease_mode(*mode);
    }

    Prerelease {
        global: warn_if_deprecated_prerelease_mode(global),
        package,
    }
}

impl From<ResolverOptions> for ResolverSettings {
    fn from(value: ResolverOptions) -> Self {
        Self {
            index_locations: value.indexes.into(),
            resolution: value.resolution.unwrap_or_default(),
            prerelease: resolve_prerelease(
                value.prerelease.unwrap_or_default(),
                value.prerelease_package.unwrap_or_default(),
            ),
            fork_strategy: value.fork_strategy.unwrap_or_default(),
            dependency_metadata: DependencyMetadata::from_entries(
                value.dependency_metadata.into_iter().flatten(),
            ),
            index_strategy: value.index_strategy.unwrap_or_default(),
            keyring_provider: value.keyring_provider.unwrap_or_default(),
            config_setting: value.config_settings.unwrap_or_default(),
            config_settings_package: value.config_settings_package.unwrap_or_default(),
            build_isolation: value.build_isolation.unwrap_or_default(),
            extra_build_dependencies: value.extra_build_dependencies.unwrap_or_default(),
            extra_build_variables: value.extra_build_variables.unwrap_or_default(),
            exclude_newer: ExcludeNewer::from_args(
                value.exclude_newer,
                value
                    .exclude_newer_package
                    .unwrap_or_default()
                    .into_iter()
                    .map(Into::into)
                    .collect(),
            ),
            link_mode: value.link_mode.unwrap_or_default(),
            torch_backend: value.torch_backend,
            cuda_driver_version: None,
            amd_gpu_architecture: None,
            sources: NoSources::from_args(
                value.no_sources,
                value.no_sources_package.unwrap_or_default(),
            ),
            upgrade: value.upgrade.unwrap_or_default(),
            build_options: BuildOptions::new(
                NoBinary::from_args(value.no_binary, value.no_binary_package.unwrap_or_default()),
                NoBuild::from_args(value.no_build, value.no_build_package.unwrap_or_default()),
            ),
        }
    }
}

/// The resolved settings to use for an invocation of the uv CLI with both resolver and installer
/// capabilities.
///
/// Represents the shared settings that are used across all uv commands outside the `pip` API.
/// Constructed from the combined [`ResolverInstallerOptions`] for an invocation.
#[derive(Debug, Clone, Default)]
pub struct ResolverInstallerSettings {
    pub resolver: ResolverSettings,
    pub compile_bytecode: bool,
    pub reinstall: Reinstall,
}

impl From<ResolverInstallerOptions> for ResolverInstallerSettings {
    fn from(value: ResolverInstallerOptions) -> Self {
        let index_locations = value.indexes.into();
        Self {
            resolver: ResolverSettings {
                build_options: BuildOptions::new(
                    NoBinary::from_args(
                        value.no_binary,
                        value.no_binary_package.unwrap_or_default(),
                    ),
                    NoBuild::from_args(value.no_build, value.no_build_package.unwrap_or_default()),
                ),
                config_setting: value.config_settings.unwrap_or_default(),
                config_settings_package: value.config_settings_package.unwrap_or_default(),
                dependency_metadata: DependencyMetadata::from_entries(
                    value.dependency_metadata.into_iter().flatten(),
                ),
                exclude_newer: ExcludeNewer::from_args(
                    value.exclude_newer,
                    value
                        .exclude_newer_package
                        .unwrap_or_default()
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                ),
                fork_strategy: value.fork_strategy.unwrap_or_default(),
                index_locations,
                index_strategy: value.index_strategy.unwrap_or_default(),
                keyring_provider: value.keyring_provider.unwrap_or_default(),
                link_mode: value.link_mode.unwrap_or_default(),
                build_isolation: value.build_isolation.unwrap_or_default(),
                extra_build_dependencies: value.extra_build_dependencies.unwrap_or_default(),
                extra_build_variables: value.extra_build_variables.unwrap_or_default(),
                prerelease: resolve_prerelease(
                    value.prerelease.unwrap_or_default(),
                    value.prerelease_package.unwrap_or_default(),
                ),
                resolution: value.resolution.unwrap_or_default(),
                sources: NoSources::from_args(
                    value.no_sources,
                    value.no_sources_package.unwrap_or_default(),
                ),
                torch_backend: value.torch_backend,
                cuda_driver_version: None,
                amd_gpu_architecture: None,
                upgrade: value.upgrade.unwrap_or_default(),
            },
            compile_bytecode: value.compile_bytecode.unwrap_or_default(),
            reinstall: value.reinstall.unwrap_or_default(),
        }
    }
}

impl<'a> From<&'a ResolverInstallerSettings> for InstallerSettingsRef<'a> {
    fn from(settings: &'a ResolverInstallerSettings) -> Self {
        Self {
            index_locations: &settings.resolver.index_locations,
            index_strategy: settings.resolver.index_strategy,
            keyring_provider: settings.resolver.keyring_provider,
            dependency_metadata: &settings.resolver.dependency_metadata,
            config_setting: &settings.resolver.config_setting,
            config_settings_package: &settings.resolver.config_settings_package,
            build_isolation: &settings.resolver.build_isolation,
            extra_build_dependencies: &settings.resolver.extra_build_dependencies,
            extra_build_variables: &settings.resolver.extra_build_variables,
            exclude_newer: &settings.resolver.exclude_newer,
            link_mode: settings.resolver.link_mode,
            compile_bytecode: settings.compile_bytecode,
            reinstall: &settings.reinstall,
            build_options: &settings.resolver.build_options,
            sources: settings.resolver.sources.clone(),
        }
    }
}
