//! Resolve command settings from CLI arguments, environment variables, and configuration files.

use std::env::VarError;
use std::ffi::OsString;
use std::fmt;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Result, bail};
use rustc_hash::FxHashSet;

use uv_audit::{VulnerabilityID, VulnerabilityServiceFormat};
use uv_auth::Service;
use uv_cache::{CacheArgs, Refresh};
use uv_client::{Certificates, Connectivity, MetadataRangeRequest};
use uv_configuration::{
    ActiveEnvironment, AddBoundsKind, AnnotationStyle, BuildIsolation, BuildOptions, Concurrency,
    DependencyGroups, DependencyMode, DevMode, DryRun, EditableMode, EnvFile, ExcludeDependency,
    ExcludeNewer, ExcludeNewerPackage, ExportFormat, ExtrasSpecification, ForkStrategy,
    GitLfsSetting, HashCheckingMode, IndexStrategy, InitKind, InitProjectKind, InstallOptions,
    KeyringProviderType, Modifications, NoBinary, NoBuild, NoSources, Override, PackageOverride,
    PipCompileFormat, Prerelease, ProjectBuildBackend, ProxyUrl, PythonUpgrade,
    PythonUpgradeSource, Reinstall, RequiredVersion, RequirementsInput, ResolutionMode,
    TargetTriple, ToolRunCommand, TrustedHost, TrustedPublishing, Upgrade, VersionControlSystem,
};
use uv_distribution_types::{
    ConfigSettings, DependencyMetadata, ExcludeNewerOverride, ExtraBuildVariables, Index,
    IndexLocations, IndexUrl, MinimumLibcVersion, NameRequirementSpecification,
    PackageConfigSettings, Requirement,
};
use uv_install_wheel::LinkMode;
use uv_normalize::{ExtraName, PackageName, PipGroupName};
use uv_pep440::Version;
use uv_pep508::{MarkerTree, RequirementOrigin};
use uv_preview::Preview;
use uv_pypi_types::SupportedEnvironments;
use uv_python_types::{
    Prefix, PythonArchitecture, PythonDownloads, PythonPreference, PythonVersion, Target,
};
use uv_redacted::DisplaySafeUrl;
use uv_settings::{
    Combine, EnvFlag, EnvironmentOptions, FilesystemOptions, FrozenFlag, FrozenSource,
    IndexOptions, LockCheck, LockedFlag, LockedSource, MalwareCheckSettings, Options, PipOptions,
    PreviewFeaturesOption, PreviewOption, PublishOptions, PythonInstallMirrors, PythonListKinds,
    ResolverInstallerOptions, ResolverInstallerSchema, ResolverInstallerSettings, ResolverOptions,
    ResolverSettings, resolve_prerelease,
};
use uv_static::EnvVars;
use uv_torch::{AmdGpuArchitecture, TorchMode};
use uv_warnings::warn_user_once;
use uv_workspace::pyproject::{DependencyType, ExtraBuildDependencies, OverrideDependency};

use crate::comma::CommaSeparatedRequirements;
use crate::{
    AddArgs, AuditArgs, AuditCommonArgs, AuditOutputFormat, AuthLoginArgs, AuthLogoutArgs,
    AuthTokenArgs, ColorChoice, DependencyConstraintsArgs, ExternalCommand, GlobalArgs, InitArgs,
    ListFormat, LockArgs, Maybe, MetadataArgs, PipCheckArgs, PipCompileArgs, PipFreezeArgs,
    PipInstallArgs, PipInstallFormat, PipListArgs, PipShowArgs, PipSyncArgs, PipTreeArgs,
    PipUninstallArgs, ProjectDependencyGroupsArgs, PythonFindArgs, PythonInstallArgs,
    PythonListArgs, PythonListFormat, PythonPinArgs, PythonUninstallArgs, PythonUpgradeArgs,
    RemoveArgs, RunArgs, SyncArgs, SyncFormat, ToolAuditArgs, ToolDirArgs, ToolInstallArgs,
    ToolListArgs, ToolRunArgs, ToolUninstallArgs, TreeArgs, TreeFormat, UpgradeArgs, VenvArgs,
    VersionArgs, VersionBumpSpec, VersionFormat,
};
use crate::{
    AuthorFrom, BuildArgs, BuildOptionsArgs, CheckArgs, ExcludeNewerArgs, ExportArgs, FormatArgs,
    HashCheckingArgs, PackageExcludeNewerArgs, PublishArgs, PythonDirArgs, RegistryClientArgs,
    ResolverArgs, ResolverInstallerArgs, ToolUpgradeArgs,
    options::{
        Flag, FlagSource, IntoPipOptions, check_conflicts, flag, resolve_flag, resolve_flag_pair,
        resolver_installer_options, resolver_options, upgrade_options,
    },
};

/// The default publish URL.
const PYPI_PUBLISH_URL: &str = "https://upload.pypi.org/legacy/";

/// The resolved global settings to use for any invocation of the CLI.
#[derive(Debug, Clone)]
pub struct GlobalSettings {
    pub required_version: Option<RequiredVersion>,
    pub quiet: u8,
    pub verbose: u8,
    pub color: ColorChoice,
    pub network_settings: NetworkSettings,
    pub concurrency: Concurrency,
    pub show_settings: bool,
    pub preview: Preview,
    pub python_preference: PythonPreference,
    pub python_arch: Option<PythonArchitecture>,
    pub python_downloads: PythonDownloads,
    pub no_progress: bool,
    pub installer_metadata: bool,
}

impl GlobalSettings {
    /// Resolve the [`GlobalSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: &GlobalArgs,
        workspace: Option<&FilesystemOptions>,
        environment: &EnvironmentOptions,
        custom_certificate_file: Option<&Path>,
    ) -> anyhow::Result<Self> {
        let network_settings =
            NetworkSettings::resolve(args, workspace, environment, custom_certificate_file)?;
        let python_preference = resolve_python_preference(args, workspace, environment)?;
        let color = resolve_color(args);
        Ok(Self {
            required_version: workspace
                .and_then(|workspace| workspace.globals.required_version.clone()),
            quiet: args.quiet,
            verbose: args.verbose,
            color,
            network_settings,
            concurrency: Concurrency::new(
                environment
                    .concurrency
                    .downloads
                    .combine(workspace.and_then(|workspace| workspace.globals.concurrent_downloads))
                    .map(NonZeroUsize::get)
                    .unwrap_or(Concurrency::DEFAULT_DOWNLOADS),
                environment
                    .concurrency
                    .builds
                    .combine(workspace.and_then(|workspace| workspace.globals.concurrent_builds))
                    .map(NonZeroUsize::get)
                    .unwrap_or_else(Concurrency::threads),
                environment
                    .concurrency
                    .installs
                    .combine(workspace.and_then(|workspace| workspace.globals.concurrent_installs))
                    .map(NonZeroUsize::get)
                    .unwrap_or_else(Concurrency::threads),
                environment
                    .concurrency
                    .cache_reads
                    .map(NonZeroUsize::get)
                    .unwrap_or(Concurrency::DEFAULT_CACHE_READS),
            ),
            show_settings: args.show_settings,
            preview: resolve_preview(args, workspace, environment)?,
            python_preference,
            python_arch: environment.python_arch,
            python_downloads: flag(
                args.allow_python_downloads,
                args.no_python_downloads,
                "python-downloads",
            )?
            .map(PythonDownloads::from)
            .combine(env(env::UV_PYTHON_DOWNLOADS))
            .combine(workspace.and_then(|workspace| workspace.globals.python_downloads))
            .unwrap_or_default(),
            // Disable the progress bar with `RUST_LOG` to avoid progress fragments interleaving
            // with log messages.
            no_progress: resolve_flag(args.no_progress, "no-progress", environment.no_progress)
                .is_enabled()
                || std::env::var_os(EnvVars::RUST_LOG).is_some(),
            installer_metadata: !resolve_flag(
                args.no_installer_metadata,
                "no-installer-metadata",
                environment.no_installer_metadata,
            )
            .is_enabled(),
        })
    }
}

/// Resolve the color choice from CLI arguments and environment variables.
pub fn resolve_color(args: &GlobalArgs) -> ColorChoice {
    if let Some(color_choice) = args.color {
        // If `--color` is passed explicitly, use its value.
        color_choice
    } else if args.no_color {
        // If `--no-color` is passed explicitly, disable color output.
        ColorChoice::Never
    } else if std::env::var_os(EnvVars::NO_COLOR)
        .as_ref()
        .is_some_and(|v| !v.is_empty())
    {
        // If the `NO_COLOR` is set, disable color output.
        ColorChoice::Never
    } else if std::env::var_os(EnvVars::FORCE_COLOR)
        .as_ref()
        .is_some_and(|v| !v.is_empty())
        || std::env::var_os(EnvVars::CLICOLOR_FORCE)
            .as_ref()
            .is_some_and(|v| !v.is_empty())
    {
        // If `FORCE_COLOR` or `CLICOLOR_FORCE` is set, always enable color output.
        ColorChoice::Always
    } else {
        ColorChoice::Auto
    }
}

fn resolve_python_preference(
    args: &GlobalArgs,
    workspace: Option<&FilesystemOptions>,
    environment: &EnvironmentOptions,
) -> anyhow::Result<PythonPreference> {
    // Resolve flags from CLI and environment variables.
    let (managed_python, no_managed_python) = resolve_flag_pair(
        args.managed_python,
        args.no_managed_python,
        "managed-python",
        "no-managed-python",
        Some(environment.managed_python),
        Some(environment.no_managed_python),
    );

    // Check for conflicts between managed_python and python_preference.
    if managed_python.is_enabled() && args.python_preference.is_some() {
        check_conflicts(managed_python, Flag::from_cli("python-preference"))?;
    }

    // Check for conflicts between no_managed_python and python_preference.
    if no_managed_python.is_enabled() && args.python_preference.is_some() {
        check_conflicts(no_managed_python, Flag::from_cli("python-preference"))?;
    }

    Ok(if managed_python.is_enabled() {
        PythonPreference::OnlyManaged
    } else if no_managed_python.is_enabled() {
        PythonPreference::OnlySystem
    } else {
        args.python_preference
            .combine(workspace.and_then(|workspace| workspace.globals.python_preference))
            .unwrap_or_default()
    })
}

/// Resolve the preview setting from CLI, environment, and workspace config.
pub fn resolve_preview(
    args: &GlobalArgs,
    workspace: Option<&FilesystemOptions>,
    environment: &EnvironmentOptions,
) -> anyhow::Result<Preview> {
    // Explicit `--preview` and `--no-preview` flags take priority.
    if let Some(enabled) = flag(args.preview, args.no_preview, "preview")? {
        return Ok(if enabled {
            Preview::all()
        } else {
            Preview::default()
        });
    }

    // `UV_PREVIEW=true` enables all preview features.
    if environment.preview.value == Some(true) {
        return Ok(Preview::all());
    }

    let configured = workspace.and_then(|workspace| workspace.globals.preview.as_ref());

    // Boolean enable-all configuration takes priority.
    if matches!(
        configured,
        Some(
            PreviewOption::Preview(true)
                | PreviewOption::PreviewFeatures(PreviewFeaturesOption::Toggle(true))
        )
    ) {
        return Ok(Preview::all());
    }

    // Explicit preview feature names take priority over configured feature names.
    if !args.preview_features.is_empty() {
        return Ok(Preview::from_feature_names(&args.preview_features));
    }

    // Fall back to workspace configuration.
    Ok(configured.map(PreviewOption::resolve).unwrap_or_default())
}

/// The resolved network settings to use for any invocation of the CLI.
#[derive(Debug, Clone)]
pub struct NetworkSettings {
    pub connectivity: Connectivity,
    offline: Flag,
    pub system_certs: bool,
    pub custom_certificates: Option<Certificates>,
    pub http_proxy: Option<ProxyUrl>,
    pub https_proxy: Option<ProxyUrl>,
    pub no_proxy: Option<Vec<String>>,
    pub allow_insecure_host: Vec<TrustedHost>,
    pub read_timeout: Duration,
    pub connect_timeout: Duration,
    pub retries: u32,
    pub metadata_range_request: MetadataRangeRequest,
}

impl NetworkSettings {
    #[allow(deprecated)]
    fn resolve(
        args: &GlobalArgs,
        workspace: Option<&FilesystemOptions>,
        environment: &EnvironmentOptions,
        custom_certificate_file: Option<&Path>,
    ) -> anyhow::Result<Self> {
        // Resolve offline flag from CLI, environment variable, and workspace config.
        // Precedence: CLI > Env var > Workspace config > default (false).
        let offline = match flag(args.offline, args.no_offline, "offline")? {
            Some(true) => Flag::from_cli("offline"),
            Some(false) => Flag::disabled(),
            None => {
                // CLI didn't provide a value, check environment variable.
                let env_flag = resolve_flag(false, "offline", environment.offline);
                if environment.offline.value.is_some() {
                    env_flag
                } else if workspace
                    .and_then(|workspace| workspace.globals.offline)
                    .unwrap_or(false)
                {
                    // Workspace config enabled offline mode.
                    Flag::from_config("offline")
                } else {
                    Flag::disabled()
                }
            }
        };

        let connectivity = if offline.is_enabled() {
            Connectivity::Offline
        } else {
            Connectivity::Online
        };

        if args.native_tls {
            warn_user_once!(
                "The `--native-tls` flag is deprecated and will be removed in a future release. Use `--system-certs` instead."
            );
        }
        if args.no_native_tls {
            warn_user_once!(
                "The `--no-native-tls` flag is deprecated and will be removed in a future release. Use `--no-system-certs` instead."
            );
        }
        if environment.native_tls.value.is_some() && environment.system_certs.value.is_none() {
            warn_user_once!(
                "The `UV_NATIVE_TLS` environment variable is deprecated and will be removed in a future release. Use `UV_SYSTEM_CERTS` instead."
            );
        }
        if let Some(workspace) = workspace
            && workspace.globals.native_tls.is_some()
            && workspace.globals.system_certs.is_none()
        {
            warn_user_once!(
                "The `native-tls` setting is deprecated and will be removed in a future release. Use `system-certs` instead."
            );
        }

        // Resolve whether to use system certificates.
        //
        // `--native-tls` is a legacy alias for `--system-certs` — it enables system certificates
        // but does NOT change the TLS backend. Any explicit CLI setting should take precedence
        // over environment variables and workspace configuration, regardless of which spelling is
        // used.
        let system_certs =
            if let Some(value) = flag(args.system_certs, args.no_system_certs, "system-certs")? {
                value
            } else if let Some(value) = flag(args.native_tls, args.no_native_tls, "native-tls")? {
                value
            } else if let Some(value) = environment.system_certs.value {
                value
            } else if let Some(value) = environment.native_tls.value {
                value
            } else {
                workspace
                    .and_then(|workspace| {
                        workspace
                            .globals
                            .system_certs
                            .or(workspace.globals.native_tls)
                    })
                    .unwrap_or(false)
            };

        let allow_insecure_host = args
            .allow_insecure_host
            .as_ref()
            .map(|allow_insecure_host| {
                allow_insecure_host
                    .iter()
                    .filter_map(|value| value.clone().into_option())
            })
            .into_iter()
            .flatten()
            .chain(
                workspace
                    .and_then(|workspace| workspace.globals.allow_insecure_host.clone())
                    .into_iter()
                    .flatten(),
            )
            .collect();
        let http_proxy = workspace.and_then(|workspace| workspace.globals.http_proxy.clone());
        let https_proxy = workspace.and_then(|workspace| workspace.globals.https_proxy.clone());
        let no_proxy = workspace.and_then(|workspace| workspace.globals.no_proxy.clone());

        let custom_certificates = custom_certificate_file
            .map(Certificates::from_file)
            .transpose()?
            .or_else(Certificates::from_env);

        Ok(Self {
            connectivity,
            offline,
            system_certs,
            custom_certificates,
            http_proxy,
            https_proxy,
            no_proxy,
            allow_insecure_host,
            read_timeout: environment.http_read_timeout,
            connect_timeout: environment.http_connect_timeout,
            retries: environment.http_retries,
            metadata_range_request: environment
                .require_metadata_range_requests
                .unwrap_or_default()
                .into(),
        })
    }

    /// Check if offline mode conflicts with a refresh request.
    ///
    /// This should be called when a command uses refresh functionality to ensure
    /// offline mode and refresh are not both enabled.
    pub fn check_refresh_conflict(&self, refresh: &Refresh) -> anyhow::Result<()> {
        if !matches!(refresh, Refresh::None(_)) {
            // TODO(charlie): `Refresh` isn't a `Flag`, so we create a synthetic one here
            // (which matches Clap's representation). Consider a dedicated helper for
            // conflicts with CLI-only arguments.
            check_conflicts(self.offline, Flag::from_cli("refresh"))?;
        }
        Ok(())
    }
}

/// The resolved cache settings to use for any invocation of the CLI.
#[derive(Debug, Clone)]
pub struct CacheSettings {
    pub no_cache: bool,
    pub cache_dir: Option<PathBuf>,
}

impl CacheSettings {
    /// Resolve the [`CacheSettings`] from the CLI, environment, and filesystem configuration.
    pub fn resolve(
        args: CacheArgs,
        workspace: Option<&FilesystemOptions>,
        environment: &EnvironmentOptions,
    ) -> Self {
        Self {
            no_cache: args
                .no_cache
                .then_some(true)
                .combine(environment.no_cache.value)
                .combine(workspace.and_then(|workspace| workspace.globals.no_cache))
                .unwrap_or(false),
            cache_dir: args
                .cache_dir
                .or_else(|| workspace.and_then(|workspace| workspace.globals.cache_dir.clone())),
        }
    }
}

/// The resolved settings to use for a `init` invocation.
#[derive(Debug, Clone)]
pub struct InitSettings {
    pub path: Option<PathBuf>,
    pub name: Option<PackageName>,
    pub kind: InitKind,
    pub bare: bool,
    pub description: Option<String>,
    pub no_description: bool,
    pub vcs: Option<VersionControlSystem>,
    pub build_backend: Option<ProjectBuildBackend>,
    pub no_readme: bool,
    pub author_from: Option<AuthorFrom>,
    pub pin_python: bool,
    pub no_workspace: bool,
    pub python: Option<String>,
    pub install_mirrors: PythonInstallMirrors,
}

impl InitSettings {
    /// Resolve the [`InitSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: InitArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> Result<Self> {
        let InitArgs {
            path,
            name,
            r#virtual,
            package,
            no_package,
            bare,
            app,
            lib,
            script,
            description,
            no_description,
            vcs,
            build_backend,
            no_readme,
            author_from,
            no_pin_python,
            pin_python,
            no_workspace,
            python,
            ..
        } = args;

        let bare = resolve_flag(bare, "bare", environment.init_bare).is_enabled();

        let filesystem_install_mirrors = filesystem
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let no_description = no_description || (bare && description.is_none());

        if r#virtual && lib {
            bail!("`--virtual` and `--lib` are mutually exclusive");
        }
        if r#virtual && build_backend.is_some() {
            bail!("`--virtual` and `--build-backend` are mutually exclusive");
        }

        let package = flag(
            package || build_backend.is_some(),
            no_package || r#virtual,
            "virtual",
        )?;

        let kind = if script {
            InitKind::Script
        } else if bare {
            if package == Some(true) || lib {
                InitKind::Project(InitProjectKind::BareWithBuildSystem)
            } else {
                InitKind::Project(InitProjectKind::Bare)
            }
        } else {
            // Merge `--app` and `--lib`.
            let app_lib_kind = match (app, lib) {
                (false, false) => InitProjectKind::ApplicationWithLibrary,
                (true, false) => InitProjectKind::Application,
                (false, true) => InitProjectKind::Library,
                (true, true) => bail!("`app` and `lib` are mutually exclusive"),
            };

            // Apply overrides from `--package`/`--no-package`.
            let app_lib_kind = match (app_lib_kind, package) {
                (InitProjectKind::ApplicationWithLibrary, None | Some(true)) => {
                    InitProjectKind::ApplicationWithLibrary
                }
                (InitProjectKind::ApplicationWithLibrary, Some(false)) => {
                    InitProjectKind::Application
                }
                (InitProjectKind::Application, None | Some(true)) => {
                    InitProjectKind::ApplicationWithLibrary
                }
                (InitProjectKind::Application, Some(false)) => InitProjectKind::Application,
                (InitProjectKind::Library, None | Some(true)) => InitProjectKind::Library,
                (InitProjectKind::Library, Some(false)) => {
                    bail!("`lib` and `no_package` are mutually exclusive");
                }
                (InitProjectKind::Bare | InitProjectKind::BareWithBuildSystem, _) => {
                    unreachable!()
                }
            };
            InitKind::Project(app_lib_kind)
        };

        if script && package == Some(true) {
            warn_user_once!("`--package` is a no-op for Python scripts, which are standalone");
        }

        Ok(Self {
            path,
            name,
            kind,
            bare,
            description,
            no_description,
            vcs: vcs.or(bare.then_some(VersionControlSystem::None)),
            build_backend,
            no_readme,
            author_from,
            pin_python: flag(pin_python, no_pin_python, "pin-python")?.unwrap_or(!bare),
            no_workspace,
            python: python.and_then(Maybe::into_option),
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
        })
    }
}

impl From<LockCheck> for Flag {
    fn from(lock_check: LockCheck) -> Self {
        match lock_check {
            LockCheck::Enabled(LockedSource::Cli(flag)) => Self::from_cli(flag.name()),
            LockCheck::Enabled(LockedSource::Env) => Self::Enabled {
                source: FlagSource::Env(EnvVars::UV_LOCKED),
                name: "locked",
            },
            LockCheck::Disabled => Self::disabled(),
        }
    }
}

impl From<FrozenSource> for Flag {
    fn from(source: FrozenSource) -> Self {
        match source {
            FrozenSource::Cli(flag) => Self::from_cli(flag.name()),
            FrozenSource::Env => Self::Enabled {
                source: FlagSource::Env(EnvVars::UV_FROZEN),
                name: "frozen",
            },
        }
    }
}

/// Resolve conflicting lock flags, letting CLI arguments override environment variables.
fn resolve_lock_flags(
    locked: LockCheck,
    frozen: Option<FrozenSource>,
) -> anyhow::Result<(LockCheck, Option<FrozenSource>)> {
    match (locked, frozen) {
        (LockCheck::Enabled(LockedSource::Cli(flag)), Some(FrozenSource::Env)) => {
            warn_user_once!("Ignoring `UV_FROZEN` because `{flag}` was provided");
            Ok((locked, None))
        }
        (LockCheck::Enabled(LockedSource::Env), Some(FrozenSource::Cli(flag))) => {
            warn_user_once!("Ignoring `UV_LOCKED` because `{flag}` was provided");
            Ok((LockCheck::Disabled, frozen))
        }
        _ => {
            check_conflicts(
                Flag::from(locked),
                frozen.map_or(Flag::Disabled, Flag::from),
            )?;
            Ok((locked, frozen))
        }
    }
}

/// Resolve frozen mode and its source from CLI arguments and the environment.
fn resolve_frozen(
    enabled: bool,
    disabled: bool,
    cli_flag: FrozenFlag,
    environment: EnvFlag,
) -> Option<FrozenSource> {
    if enabled {
        Some(FrozenSource::Cli(cli_flag))
    } else if !disabled && environment.value == Some(true) {
        Some(FrozenSource::Env)
    } else {
        None
    }
}

/// Resolve a lock check and its source from CLI arguments and the environment.
fn resolve_lock_check(
    enabled: bool,
    disabled: bool,
    cli_flag: LockedFlag,
    environment: EnvFlag,
) -> LockCheck {
    if enabled {
        LockCheck::Enabled(LockedSource::Cli(cli_flag))
    } else if !disabled && environment.value == Some(true) {
        LockCheck::Enabled(LockedSource::Env)
    } else {
        LockCheck::Disabled
    }
}

/// The resolved settings to use for a `run` invocation.
#[derive(Debug, Clone)]
pub struct RunSettings {
    pub lock_check: LockCheck,
    pub frozen: Option<FrozenSource>,
    pub extras: ExtrasSpecification,
    pub groups: DependencyGroups,
    pub editable: Option<EditableMode>,
    pub modifications: Modifications,
    pub with: Vec<String>,
    pub with_editable: Vec<String>,
    pub with_requirements: Vec<RequirementsInput>,
    pub isolated: bool,
    pub show_resolution: bool,
    pub all_packages: bool,
    pub package: Option<PackageName>,
    pub no_project: bool,
    pub active: ActiveEnvironment,
    pub no_sync: bool,
    pub python: Option<String>,
    pub python_platform: Option<TargetTriple>,
    pub install_mirrors: PythonInstallMirrors,
    pub refresh: Refresh,
    pub settings: ResolverInstallerSettings,
    pub env_file: EnvFile,
    pub max_recursion_depth: u32,
    pub malware_settings: MalwareCheckSettings,
    #[cfg(unix)]
    pub run_rlimit_nofile: Option<u32>,
}

impl RunSettings {
    // Default value for UV_RUN_MAX_RECURSION_DEPTH if unset. This is large
    // enough that it's unlikely a user actually needs this recursion depth,
    // but short enough that we detect recursion quickly enough to avoid OOMing
    // or hanging for a long time.
    const DEFAULT_MAX_RECURSION_DEPTH: u32 = 100;

    /// Resolve the [`RunSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: RunArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let RunArgs {
            extra,
            all_extras,
            no_extra,
            no_all_extras,
            dependency_groups:
                ProjectDependencyGroupsArgs {
                    dev,
                    no_dev,
                    only_dev,
                    group,
                    no_group,
                    no_default_groups,
                    only_group,
                    all_groups,
                },
            module: _,
            editable,
            no_editable,
            no_editable_package,
            inexact,
            exact,
            script: _,
            gui_script: _,
            command: _,
            with,
            with_editable,
            with_requirements,
            isolated,
            active,
            no_active,
            no_sync,
            locked,
            no_locked,
            frozen,
            no_frozen,
            installer,
            build,
            refresh,
            all_packages,
            package,
            no_project,
            python,
            python_platform,
            show_resolution,
            env_file,
            no_env_file,
            max_recursion_depth,
        } = args;

        let filesystem_install_mirrors = filesystem
            .as_ref()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        // Resolve flags from CLI and environment variables.
        let locked = resolve_lock_check(locked, no_locked, LockedFlag::Locked, environment.locked);
        let frozen = resolve_frozen(frozen, no_frozen, FrozenFlag::Frozen, environment.frozen);
        let no_sync = resolve_flag(no_sync, "no-sync", environment.no_sync);

        let (locked, frozen) = resolve_lock_flags(locked, frozen)?;

        let (dev, no_dev) = resolve_flag_pair(
            dev,
            no_dev,
            "dev",
            "no-dev",
            Some(environment.dev),
            Some(environment.no_dev),
        );

        let (editable, no_editable) = resolve_flag_pair(
            editable,
            no_editable,
            "editable",
            "no-editable",
            None,
            Some(environment.no_editable),
        );
        let isolated = isolated || environment.isolated.value == Some(true);
        let show_resolution = show_resolution || environment.show_resolution.value == Some(true);
        let no_env_file = no_env_file || environment.no_env_file.value == Some(true);

        let malware_settings = MalwareCheckSettings::resolve(filesystem.as_ref(), &environment);

        Ok(Self {
            lock_check: locked,
            frozen,
            extras: ExtrasSpecification::from_args(
                extra.unwrap_or_default(),
                no_extra,
                // TODO(blueraft): support no_default_extras
                false,
                // TODO(blueraft): support only_extra
                vec![],
                flag(all_extras, no_all_extras, "all-extras")?.unwrap_or_default(),
            ),
            groups: DependencyGroups::from_args(
                DevMode::from_args(dev.into(), no_dev.into(), only_dev),
                group,
                if no_group.is_empty() {
                    environment.no_group.clone().unwrap_or_default()
                } else {
                    no_group
                },
                no_default_groups,
                only_group,
                all_groups,
            ),
            editable: EditableMode::from_args(
                flag(editable.into(), no_editable.into(), "editable")?,
                no_editable_package,
            ),
            modifications: if flag(exact, inexact, "inexact")?.unwrap_or(false) {
                Modifications::Exact
            } else {
                Modifications::Sufficient
            },
            with: with
                .into_iter()
                .flat_map(CommaSeparatedRequirements::into_iter)
                .collect(),
            with_editable: with_editable
                .into_iter()
                .flat_map(CommaSeparatedRequirements::into_iter)
                .collect(),
            with_requirements: with_requirements
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            isolated,
            show_resolution,
            all_packages,
            package,
            no_project,
            no_sync: no_sync.is_enabled(),
            active: flag(active, no_active, "active")?.into(),
            python: python.and_then(Maybe::into_option),
            python_platform,
            refresh: Refresh::try_from(refresh)?,
            settings: resolve_resolver_installer_settings(
                installer,
                build,
                filesystem,
                &environment,
            )?,
            env_file: EnvFile::from_args(env_file, no_env_file),
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
            max_recursion_depth: max_recursion_depth.unwrap_or(Self::DEFAULT_MAX_RECURSION_DEPTH),
            malware_settings,
            #[cfg(unix)]
            run_rlimit_nofile: environment.run_rlimit_nofile,
        })
    }
}

/// The resolved settings to use for a `tool run` invocation.
#[derive(Debug, Clone)]
pub struct ToolRunSettings {
    pub command: Option<Vec<OsString>>,
    pub from: Option<String>,
    pub with: Vec<String>,
    pub with_requirements: Vec<RequirementsInput>,
    pub with_editable: Vec<String>,
    pub constraints: Vec<RequirementsInput>,
    pub overrides: Vec<RequirementsInput>,
    pub build_constraints: Vec<RequirementsInput>,
    pub isolated: bool,
    pub show_resolution: bool,
    pub lfs: GitLfsSetting,
    pub python: Option<String>,
    pub python_platform: Option<TargetTriple>,
    pub install_mirrors: PythonInstallMirrors,
    pub refresh: Refresh,
    pub options: ResolverInstallerOptions,
    pub settings: ResolverInstallerSettings,
    pub env_file: Vec<PathBuf>,
    pub no_env_file: bool,
}

impl ToolRunSettings {
    /// Resolve the [`ToolRunSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: ToolRunArgs,
        filesystem: Option<FilesystemOptions>,
        invocation_source: ToolRunCommand,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let ToolRunArgs {
            command,
            from,
            with,
            with_editable,
            with_requirements,
            constraints,
            overrides,
            build_constraints,
            isolated,
            env_file,
            no_env_file,
            show_resolution,
            installer,
            build,
            refresh,
            lfs,
            python,
            python_platform,
            torch_backend,
            generate_shell_completion: _,
        } = args;

        // If `--upgrade` was passed explicitly, warn.
        if installer.upgrade || !installer.upgrade_package.is_empty() {
            if with.is_empty() && with_requirements.is_empty() {
                warn_user_once!(
                    "Tools cannot be upgraded via `{invocation_source}`; use `uv tool upgrade --all` to upgrade all installed tools, or `{invocation_source} package@latest` to run the latest version of a tool."
                );
            } else {
                warn_user_once!(
                    "Tools cannot be upgraded via `{invocation_source}`; use `uv tool upgrade --all` to upgrade all installed tools, `{invocation_source} package@latest` to run the latest version of a tool, or `{invocation_source} --refresh package` to upgrade any `--with` dependencies."
                );
            }
        }

        // If `--reinstall` was passed explicitly, warn.
        if installer.reinstall.reinstall || !installer.reinstall.reinstall_package.is_empty() {
            if with.is_empty() && with_requirements.is_empty() {
                warn_user_once!(
                    "Tools cannot be reinstalled via `{invocation_source}`; use `uv tool upgrade --all --reinstall` to reinstall all installed tools, `{invocation_source} package@latest` to run the latest version of a tool, or `uv cache prune` to clear any cached tool environments."
                );
            } else {
                warn_user_once!(
                    "Tools cannot be reinstalled via `{invocation_source}`; use `uv tool upgrade --all --reinstall` to reinstall all installed tools, `{invocation_source} package@latest` to run the latest version of a tool, `{invocation_source} --refresh package` to reinstall any `--with` dependencies, or `uv cache prune` to clear any cached tool environments."
                );
            }
        }

        let filesystem_options = filesystem.map(FilesystemOptions::into_options);

        let options = resolver_installer_options_with_environment(
            resolver_installer_options(
                installer,
                build,
                filesystem_options
                    .as_ref()
                    .and_then(|options| options.top_level.index.as_deref())
                    .unwrap_or_default(),
            )?,
            &environment,
        )
        .combine(ResolverInstallerOptions::from(
            filesystem_options
                .as_ref()
                .map(|options| options.top_level.clone())
                .unwrap_or_default(),
        ));

        let filesystem_install_mirrors = filesystem_options
            .map(|options| options.install_mirrors.clone())
            .unwrap_or_default();

        let mut settings = ResolverInstallerSettings::from(options.clone());
        if torch_backend.is_some() {
            settings.resolver.torch_backend = torch_backend;
        }
        let lfs = GitLfsSetting::new(lfs.then_some(true), environment.lfs);

        // Resolve flags from CLI and environment variables.
        let isolated = isolated || environment.isolated.value == Some(true);
        let show_resolution = show_resolution || environment.show_resolution.value == Some(true);
        let no_env_file = no_env_file || environment.no_env_file.value == Some(true);

        Ok(Self {
            command: command.map(|ExternalCommand::Cmd(command)| command),
            from,
            with: with
                .into_iter()
                .flat_map(CommaSeparatedRequirements::into_iter)
                .collect(),
            with_editable: with_editable
                .into_iter()
                .flat_map(CommaSeparatedRequirements::into_iter)
                .collect(),
            with_requirements: with_requirements
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            constraints: constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            overrides: overrides
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            build_constraints: build_constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            isolated,
            show_resolution,
            lfs,
            python: python.and_then(Maybe::into_option),
            python_platform,
            refresh: Refresh::try_from(refresh)?,
            settings,
            options,
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
            env_file,
            no_env_file,
        })
    }
}

/// The resolved settings to use for a `tool install` invocation.
#[derive(Debug, Clone)]
pub struct ToolInstallSettings {
    pub package: String,
    pub from: Option<String>,
    pub with: Vec<String>,
    pub with_requirements: Vec<RequirementsInput>,
    pub with_executables_from: Vec<String>,
    pub with_editable: Vec<String>,
    pub constraints: Vec<RequirementsInput>,
    pub overrides: Vec<RequirementsInput>,
    pub excludes: Vec<RequirementsInput>,
    pub build_constraints: Vec<RequirementsInput>,
    pub lfs: GitLfsSetting,
    pub python: Option<String>,
    pub python_platform: Option<TargetTriple>,
    pub refresh: Refresh,
    pub options: ResolverInstallerOptions,
    pub settings: ResolverInstallerSettings,
    pub force: bool,
    pub editable: bool,
    pub install_mirrors: PythonInstallMirrors,
}

impl ToolInstallSettings {
    /// Resolve the [`ToolInstallSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: ToolInstallArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let ToolInstallArgs {
            package,
            editable,
            from,
            with,
            with_editable,
            with_requirements,
            with_executables_from,
            constraints:
                DependencyConstraintsArgs {
                    constraints,
                    overrides,
                    excludes,
                    build_constraints,
                },
            lfs,
            installer,
            force,
            build,
            refresh,
            python,
            python_platform,
            torch_backend,
        } = args;

        let filesystem_options = filesystem.map(FilesystemOptions::into_options);

        let options = resolver_installer_options_with_environment(
            resolver_installer_options(
                installer,
                build,
                filesystem_options
                    .as_ref()
                    .and_then(|options| options.top_level.index.as_deref())
                    .unwrap_or_default(),
            )?,
            &environment,
        )
        .combine(ResolverInstallerOptions::from(
            filesystem_options
                .as_ref()
                .map(|options| options.top_level.clone())
                .unwrap_or_default(),
        ));

        let filesystem_install_mirrors = filesystem_options
            .map(|options| options.install_mirrors.clone())
            .unwrap_or_default();

        let mut settings = ResolverInstallerSettings::from(options.clone());
        if torch_backend.is_some() {
            settings.resolver.torch_backend = torch_backend;
        }
        let lfs = GitLfsSetting::new(lfs.then_some(true), environment.lfs);

        Ok(Self {
            package,
            from,
            with: with
                .into_iter()
                .flat_map(CommaSeparatedRequirements::into_iter)
                .collect(),
            with_editable: with_editable
                .into_iter()
                .flat_map(CommaSeparatedRequirements::into_iter)
                .collect(),
            with_requirements: with_requirements
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            with_executables_from: with_executables_from
                .into_iter()
                .flat_map(CommaSeparatedRequirements::into_iter)
                .collect(),
            constraints: constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            overrides: overrides
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            excludes: excludes
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            build_constraints: build_constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            lfs,
            python: python.and_then(Maybe::into_option),
            python_platform,
            force,
            editable,
            refresh: Refresh::try_from(refresh)?,
            options,
            settings,
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
        })
    }
}

/// The resolved settings to use for a `tool upgrade` invocation.
#[derive(Debug, Clone)]
pub struct ToolUpgradeSettings {
    pub names: Vec<String>,
    pub python: Option<String>,
    pub python_platform: Option<TargetTriple>,
    pub install_mirrors: PythonInstallMirrors,
    pub args: ResolverInstallerOptions,
    pub filesystem: ResolverInstallerOptions,
}
impl ToolUpgradeSettings {
    /// Resolve the [`ToolUpgradeSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: ToolUpgradeArgs,
        filesystem: Option<FilesystemOptions>,
        environment: &EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let ToolUpgradeArgs {
            name,
            python,
            python_platform,
            upgrade,
            upgrade_package,
            upgrade_group,
            index_args,
            all,
            reinstall,
            registry_client,
            version_selection,
            config_setting,
            config_setting_package: config_settings_package,
            build_isolation,
            exclude_newer,
            link_mode,
            compile_bytecode,
            sources,
            build,
        } = args;

        if upgrade {
            warn_user_once!("`--upgrade` is enabled by default on `uv tool upgrade`");
        }
        if !upgrade_package.is_empty() {
            warn_user_once!("`--upgrade-package` is enabled by default on `uv tool upgrade`");
        }

        // Enable `--upgrade` by default.
        let installer = ResolverInstallerArgs {
            index_args,
            upgrade: upgrade_package.is_empty(),
            no_upgrade: false,
            upgrade_package,
            upgrade_group,
            reinstall,
            registry_client,
            version_selection,
            config_setting,
            config_settings_package,
            build_isolation,
            exclude_newer,
            link_mode,
            compile_bytecode,
            sources,
        };

        let args = resolver_installer_options_with_environment(
            resolver_installer_options(installer, build, configured_indexes(filesystem.as_ref()))?,
            environment,
        );
        let filesystem = filesystem.map(FilesystemOptions::into_options);
        let filesystem_install_mirrors = filesystem
            .as_ref()
            .map(|options| options.install_mirrors.clone())
            .unwrap_or_default();
        let top_level = ResolverInstallerOptions::from(
            filesystem
                .map(|options| options.top_level)
                .unwrap_or_default(),
        );

        Ok(Self {
            names: if all { vec![] } else { name },
            python: python.and_then(Maybe::into_option),
            python_platform,
            args,
            filesystem: top_level,
            install_mirrors: environment
                .install_mirrors
                .clone()
                .combine(filesystem_install_mirrors),
        })
    }
}

/// The resolved settings to use for a `tool list` invocation.
#[derive(Debug, Clone)]
pub struct ToolListSettings {
    pub show_paths: bool,
    pub show_version_specifiers: bool,
    pub show_with: bool,
    pub show_extras: bool,
    pub show_python: bool,
    pub outdated: bool,
    pub args: ResolverInstallerOptions,
    pub filesystem: ResolverInstallerOptions,
}

impl ToolListSettings {
    /// Resolve the [`ToolListSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: ToolListArgs,
        filesystem: Option<FilesystemOptions>,
    ) -> anyhow::Result<Self> {
        let ToolListArgs {
            show_paths,
            show_version_specifiers,
            show_with,
            show_extras,
            show_python,
            outdated,
            no_outdated,
            exclude_newer:
                PackageExcludeNewerArgs {
                    exclude_newer: ExcludeNewerArgs { exclude_newer },
                    exclude_newer_package,
                },
            python_preference: _,
            no_python_downloads: _,
        } = args;

        let top_level = filesystem
            .map(FilesystemOptions::into_options)
            .map(|options| options.top_level)
            .unwrap_or_default();
        let filesystem = ResolverInstallerOptions {
            indexes: IndexOptions {
                index: top_level.index,
                ..IndexOptions::default()
            },
            exclude_newer: top_level.exclude_newer,
            exclude_newer_package: top_level.exclude_newer_package,
            ..ResolverInstallerOptions::default()
        };

        Ok(Self {
            show_paths,
            show_version_specifiers,
            show_with,
            show_extras,
            show_python,
            outdated: flag(outdated, no_outdated, "outdated")?.unwrap_or(false),
            args: ResolverInstallerOptions {
                exclude_newer,
                exclude_newer_package: exclude_newer_package.map(ExcludeNewerPackage::from_iter),
                ..ResolverInstallerOptions::default()
            },
            filesystem,
        })
    }
}

/// The resolved settings to use for a `tool audit` invocation.
#[derive(Debug, Clone)]
pub struct ToolAuditSettings {
    pub names: Vec<PackageName>,
    pub output_format: AuditOutputFormat,
    pub service_format: VulnerabilityServiceFormat,
    pub service_url: Option<DisplaySafeUrl>,
    pub ignore: Vec<VulnerabilityID>,
    pub ignore_until_fixed: Vec<VulnerabilityID>,
    pub filesystem: ResolverInstallerOptions,
}

impl ToolAuditSettings {
    /// Resolve the [`ToolAuditSettings`] from the CLI and user-level configuration.
    pub fn resolve(args: ToolAuditArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let ToolAuditArgs {
            name,
            all,
            audit:
                AuditCommonArgs {
                    offline: _,
                    output_format,
                    ignore,
                    ignore_until_fixed,
                    service_format,
                    service_url,
                },
        } = args;

        let audit = filesystem
            .as_ref()
            .and_then(|options| options.audit.clone())
            .unwrap_or_default();
        let filesystem = filesystem
            .map(FilesystemOptions::into_options)
            .map(|options| ResolverInstallerOptions::from(options.top_level))
            .unwrap_or_default();

        let ignore = ignore
            .into_iter()
            .chain(audit.ignore.unwrap_or_default())
            .map(VulnerabilityID::new)
            .collect();
        let ignore_until_fixed = ignore_until_fixed
            .into_iter()
            .chain(audit.ignore_until_fixed.unwrap_or_default())
            .map(VulnerabilityID::new)
            .collect();

        Self {
            names: if all { vec![] } else { name },
            output_format,
            service_format,
            service_url,
            ignore,
            ignore_until_fixed,
            filesystem,
        }
    }
}

/// The resolved settings to use for a `tool uninstall` invocation.
#[derive(Debug, Clone)]
pub struct ToolUninstallSettings {
    pub name: Vec<PackageName>,
}

impl ToolUninstallSettings {
    /// Resolve the [`ToolUninstallSettings`] from the CLI and filesystem configuration.
    pub fn resolve(args: ToolUninstallArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let ToolUninstallArgs { name, all } = args;

        Self {
            name: if all { vec![] } else { name },
        }
    }
}

/// The resolved settings to use for a `tool dir` invocation.
#[derive(Debug, Clone)]
pub struct ToolDirSettings {
    pub bin: bool,
}

impl ToolDirSettings {
    /// Resolve the [`ToolDirSettings`] from the CLI and filesystem configuration.
    #[expect(clippy::needless_pass_by_value)]
    pub fn resolve(args: ToolDirArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let ToolDirArgs { bin } = args;

        Self { bin }
    }
}

/// The resolved settings to use for a `tool run` invocation.
#[derive(Debug, Clone)]
pub struct PythonListSettings {
    pub request: Option<String>,
    pub kinds: PythonListKinds,
    pub all_platforms: bool,
    pub all_arches: bool,
    pub all_versions: bool,
    pub show_urls: bool,
    pub output_format: PythonListFormat,
    pub install_mirrors: PythonInstallMirrors,
}

impl PythonListSettings {
    /// Resolve the [`PythonListSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: PythonListArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> Self {
        let PythonListArgs {
            request,
            all_versions,
            all_platforms,
            all_arches,
            only_installed,
            only_downloads,
            show_urls,
            output_format,
            python_downloads_json_url: python_downloads_json_url_arg,
        } = args;

        let filesystem_install_mirrors = filesystem
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let install_mirrors = PythonInstallMirrors {
            python_downloads_json_url: python_downloads_json_url_arg,
            ..Default::default()
        }
        .combine(environment.install_mirrors)
        .combine(filesystem_install_mirrors);

        let kinds = if only_installed {
            PythonListKinds::Installed
        } else if only_downloads {
            PythonListKinds::Downloads
        } else {
            PythonListKinds::default()
        };

        Self {
            request,
            kinds,
            all_platforms,
            all_arches,
            all_versions,
            show_urls,
            output_format,
            install_mirrors,
        }
    }
}

/// The resolved settings to use for a `python dir` invocation.
#[derive(Debug, Clone)]
pub struct PythonDirSettings {
    pub bin: bool,
}

impl PythonDirSettings {
    /// Resolve the [`PythonDirSettings`] from the CLI and filesystem configuration.
    #[expect(clippy::needless_pass_by_value)]
    pub fn resolve(args: PythonDirArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let PythonDirArgs { bin } = args;

        Self { bin }
    }
}

/// The resolved settings to use for a `python install` invocation.
#[derive(Debug, Clone)]
pub struct PythonInstallSettings {
    pub install_dir: Option<PathBuf>,
    pub targets: Vec<String>,
    pub reinstall: bool,
    pub force: bool,
    pub upgrade: PythonUpgrade,
    pub bin: Option<bool>,
    pub registry: Option<bool>,
    pub install_mirrors: PythonInstallMirrors,
    pub default: bool,
    pub compile_bytecode: bool,
}

impl PythonInstallSettings {
    /// Resolve the [`PythonInstallSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: PythonInstallArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let filesystem_install_mirrors = filesystem
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let install_mirrors = args
            .install_mirrors()
            .combine(environment.install_mirrors)
            .combine(filesystem_install_mirrors);

        let PythonInstallArgs {
            install_dir,
            targets,
            reinstall,
            bin,
            no_bin,
            registry,
            no_registry,
            force,
            upgrade,
            mirror: _,
            pypy_mirror: _,
            graalpy_mirror: _,
            pyodide_mirror: _,
            python_downloads_json_url: _,
            default,
            compile_bytecode,
        } = args;

        Ok(Self {
            install_dir,
            targets,
            reinstall,
            force,
            upgrade: if upgrade {
                PythonUpgrade::Enabled(PythonUpgradeSource::Install)
            } else {
                PythonUpgrade::Disabled
            },
            bin: flag(bin, no_bin, "bin")?.or(environment.python_install_bin),
            registry: match flag(registry, no_registry, "registry")? {
                Some(registry) => Some(registry),
                None => environment.python_install_registry.or(
                    if environment.python_no_registry.value == Some(true) {
                        Some(false)
                    } else {
                        None
                    },
                ),
            },
            install_mirrors,
            default,
            compile_bytecode: flag(
                compile_bytecode.compile_bytecode,
                compile_bytecode.no_compile_bytecode,
                "compile-bytecode",
            )?
            .unwrap_or_default(),
        })
    }
}

/// The resolved settings to use for a `python upgrade` invocation.
#[expect(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct PythonUpgradeSettings {
    pub install_dir: Option<PathBuf>,
    pub targets: Vec<String>,
    pub force: bool,
    pub registry: Option<bool>,
    pub install_mirrors: PythonInstallMirrors,
    pub reinstall: bool,
    pub default: bool,
    pub bin: Option<bool>,
    pub compile_bytecode: bool,
}

impl PythonUpgradeSettings {
    /// Resolve the [`PythonUpgradeSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: PythonUpgradeArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let filesystem_install_mirrors = filesystem
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let install_mirrors = args
            .install_mirrors()
            .combine(environment.install_mirrors)
            .combine(filesystem_install_mirrors);

        let force = false;
        let default = false;
        let bin = None;
        let registry = environment.python_install_registry.or(
            if environment.python_no_registry.value == Some(true) {
                Some(false)
            } else {
                None
            },
        );

        let PythonUpgradeArgs {
            install_dir,
            targets,
            mirror: _,
            pypy_mirror: _,
            graalpy_mirror: _,
            pyodide_mirror: _,
            reinstall,
            python_downloads_json_url: _,
            compile_bytecode,
        } = args;

        Ok(Self {
            install_dir,
            targets,
            force,
            registry,
            install_mirrors,
            reinstall,
            default,
            bin,
            compile_bytecode: flag(
                compile_bytecode.compile_bytecode,
                compile_bytecode.no_compile_bytecode,
                "compile-bytecode",
            )?
            .unwrap_or_default(),
        })
    }
}

/// The resolved settings to use for a `python uninstall` invocation.
#[derive(Debug, Clone)]
pub struct PythonUninstallSettings {
    pub install_dir: Option<PathBuf>,
    pub targets: Vec<String>,
    pub all: bool,
}

impl PythonUninstallSettings {
    /// Resolve the [`PythonUninstallSettings`] from the CLI and filesystem configuration.
    pub fn resolve(args: PythonUninstallArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let PythonUninstallArgs {
            install_dir,
            targets,
            all,
        } = args;

        Self {
            install_dir,
            targets,
            all,
        }
    }
}

/// The resolved settings to use for a `python find` invocation.
#[derive(Debug, Clone)]
pub struct PythonFindSettings {
    pub request: Option<String>,
    pub show_version: bool,
    pub resolve_links: bool,
    pub no_project: bool,
    pub system: bool,
    pub python_downloads_json_url: Option<String>,
}

impl PythonFindSettings {
    /// Resolve the [`PythonFindSettings`] from the CLI and workspace configuration.
    pub fn resolve(
        args: PythonFindArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let PythonFindArgs {
            request,
            show_version,
            resolve_links,
            no_project,
            system,
            no_system,
            script: _,
            python_downloads_json_url,
        } = args;

        let filesystem_install_mirrors = filesystem
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let install_mirrors = PythonInstallMirrors {
            python_downloads_json_url,
            ..Default::default()
        }
        .combine(environment.install_mirrors)
        .combine(filesystem_install_mirrors);

        let PythonInstallMirrors {
            python_install_mirror: _,
            pypy_install_mirror: _,
            graalpy_install_mirror: _,
            pyodide_install_mirror: _,
            python_downloads_json_url,
        } = install_mirrors;

        Ok(Self {
            request,
            show_version,
            resolve_links,
            no_project,
            system: flag(system, no_system, "system")?.unwrap_or_default(),
            python_downloads_json_url,
        })
    }
}

/// The resolved settings to use for a `python pin` invocation.
#[derive(Debug, Clone)]
pub struct PythonPinSettings {
    pub request: Option<String>,
    pub resolved: bool,
    pub no_project: bool,
    pub global: bool,
    pub rm: bool,
    pub install_mirrors: PythonInstallMirrors,
}

impl PythonPinSettings {
    /// Resolve the [`PythonPinSettings`] from the CLI and workspace configuration.
    pub fn resolve(
        args: PythonPinArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let PythonPinArgs {
            request,
            no_resolved,
            resolved,
            no_project,
            global,
            rm,
            python_downloads_json_url,
        } = args;

        let filesystem_install_mirrors = filesystem
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let install_mirrors = PythonInstallMirrors {
            python_downloads_json_url,
            ..Default::default()
        }
        .combine(environment.install_mirrors)
        .combine(filesystem_install_mirrors);

        Ok(Self {
            request,
            resolved: flag(resolved, no_resolved, "resolved")?.unwrap_or(false),
            no_project,
            global,
            rm,
            install_mirrors,
        })
    }
}

/// The resolved settings to use for a `sync` invocation.
#[derive(Debug, Clone)]
pub struct SyncSettings {
    pub lock_check: LockCheck,
    pub frozen: Option<FrozenSource>,
    pub dry_run: DryRun,
    pub script: Option<PathBuf>,
    pub active: ActiveEnvironment,
    pub extras: ExtrasSpecification,
    pub groups: DependencyGroups,
    pub editable: Option<EditableMode>,
    pub install_options: InstallOptions,
    pub modifications: Modifications,
    pub all_packages: bool,
    pub package: Vec<PackageName>,
    pub python: Option<String>,
    pub python_platform: Option<TargetTriple>,
    pub install_mirrors: PythonInstallMirrors,
    pub refresh: Refresh,
    pub settings: ResolverInstallerSettings,
    pub output_format: SyncFormat,
    pub malware_settings: MalwareCheckSettings,
}

impl SyncSettings {
    /// Resolve the [`SyncSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: SyncArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let SyncArgs {
            extra,
            all_extras,
            no_extra,
            no_all_extras,
            dependency_groups:
                ProjectDependencyGroupsArgs {
                    dev,
                    no_dev,
                    only_dev,
                    group,
                    no_group,
                    no_default_groups,
                    only_group,
                    all_groups,
                },
            editable,
            no_editable,
            no_editable_package,
            inexact,
            exact,
            no_install_project,
            only_install_project,
            no_install_workspace,
            only_install_workspace,
            no_install_local,
            only_install_local,
            no_install_package,
            only_install_package,
            locked,
            no_locked,
            frozen,
            no_frozen,
            active,
            no_active,
            dry_run,
            installer,
            build,
            refresh,
            all_packages,
            package,
            script,
            python,
            python_platform,
            check,
            no_check,
            output_format,
        } = args;
        let filesystem_install_mirrors = filesystem
            .as_ref()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let malware_settings = MalwareCheckSettings::resolve(filesystem.as_ref(), &environment);
        let settings =
            resolve_resolver_installer_settings(installer, build, filesystem, &environment)?;

        let check = flag(check, no_check, "check")?.unwrap_or_default();
        let dry_run = if check {
            DryRun::Check
        } else {
            DryRun::from_args(dry_run)
        };

        // Resolve flags from CLI and environment variables.
        let locked = resolve_lock_check(locked, no_locked, LockedFlag::Locked, environment.locked);
        let frozen = resolve_frozen(frozen, no_frozen, FrozenFlag::Frozen, environment.frozen);

        let (locked, frozen) = resolve_lock_flags(locked, frozen)?;

        let (dev, no_dev) = resolve_flag_pair(
            dev,
            no_dev,
            "dev",
            "no-dev",
            Some(environment.dev),
            Some(environment.no_dev),
        );
        let (editable, no_editable) = resolve_flag_pair(
            editable,
            no_editable,
            "editable",
            "no-editable",
            None,
            Some(environment.no_editable),
        );

        let (no_install_project, only_install_project) = resolve_flag_pair(
            no_install_project,
            only_install_project,
            "no-install-project",
            "only-install-project",
            Some(environment.no_install_project),
            Some(environment.only_install_project),
        );
        let (no_install_workspace, only_install_workspace) = resolve_flag_pair(
            no_install_workspace,
            only_install_workspace,
            "no-install-workspace",
            "only-install-workspace",
            Some(environment.no_install_workspace),
            Some(environment.only_install_workspace),
        );
        let (no_install_local, only_install_local) = resolve_flag_pair(
            no_install_local,
            only_install_local,
            "no-install-local",
            "only-install-local",
            Some(environment.no_install_local),
            Some(environment.only_install_local),
        );
        check_conflicts(no_install_project, only_install_project)?;
        check_conflicts(no_install_workspace, only_install_workspace)?;
        check_conflicts(no_install_local, only_install_local)?;
        if script.is_some() {
            let script = Flag::from_cli("script");
            check_conflicts(no_install_project, script)?;
            check_conflicts(no_install_workspace, script)?;
            check_conflicts(no_install_local, script)?;
        }
        let no_install_project = no_install_project.is_enabled();
        let only_install_project = only_install_project.is_enabled();
        let no_install_workspace = no_install_workspace.is_enabled();
        let only_install_workspace = only_install_workspace.is_enabled();
        let no_install_local = no_install_local.is_enabled();
        let only_install_local = only_install_local.is_enabled();

        Ok(Self {
            output_format,
            lock_check: locked,
            frozen,
            dry_run,
            script,
            active: flag(active, no_active, "active")?.into(),
            extras: ExtrasSpecification::from_args(
                extra.unwrap_or_default(),
                no_extra,
                // TODO(blueraft): support no_default_extras
                false,
                // TODO(blueraft): support only_extra
                vec![],
                flag(all_extras, no_all_extras, "all-extras")?.unwrap_or_default(),
            ),
            groups: DependencyGroups::from_args(
                DevMode::from_args(dev.into(), no_dev.into(), only_dev),
                group,
                if no_group.is_empty() {
                    environment.no_group.clone().unwrap_or_default()
                } else {
                    no_group
                },
                no_default_groups,
                only_group,
                all_groups,
            ),
            editable: EditableMode::from_args(
                flag(editable.into(), no_editable.into(), "editable")?,
                no_editable_package,
            ),
            install_options: InstallOptions::new(
                no_install_project,
                only_install_project,
                no_install_workspace,
                only_install_workspace,
                no_install_local,
                only_install_local,
                no_install_package,
                only_install_package,
            ),
            modifications: if flag(exact, inexact, "inexact")?.unwrap_or(true) {
                Modifications::Exact
            } else {
                Modifications::Sufficient
            },
            all_packages,
            package,
            python: python.and_then(Maybe::into_option),
            python_platform,
            refresh: Refresh::try_from(refresh)?,
            settings,
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
            malware_settings,
        })
    }
}

/// The resolved settings to use for a `lock` invocation.
#[derive(Debug, Clone)]
pub struct LockSettings {
    pub lock_check: LockCheck,
    pub frozen: Option<FrozenSource>,
    pub dry_run: DryRun,
    pub script: Option<PathBuf>,
    pub python: Option<String>,
    pub install_mirrors: PythonInstallMirrors,
    pub refresh: Refresh,
    pub settings: ResolverSettings,
}

impl LockSettings {
    /// Resolve the [`LockSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: LockArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let LockArgs {
            check,
            locked,
            no_locked,
            check_exists,
            frozen,
            no_frozen,
            dry_run,
            script,
            resolver,
            build,
            refresh,
            python,
        } = args;

        let filesystem_install_mirrors = filesystem
            .as_ref()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        // Resolve flags from CLI and environment variables.
        let locked = resolve_lock_check(
            locked || check,
            no_locked,
            if check {
                LockedFlag::Check
            } else {
                LockedFlag::Locked
            },
            environment.locked,
        );
        let frozen = resolve_frozen(
            frozen || check_exists,
            no_frozen,
            if check_exists {
                FrozenFlag::CheckExists
            } else {
                FrozenFlag::Frozen
            },
            environment.frozen,
        );

        let (locked, frozen) = resolve_lock_flags(locked, frozen)?;

        Ok(Self {
            lock_check: locked,
            frozen,
            dry_run: DryRun::from_args(dry_run),
            script,
            python: python.and_then(Maybe::into_option),
            refresh: Refresh::try_from(refresh)?,
            settings: resolve_resolver_settings(resolver, build, filesystem, &environment)?,
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
        })
    }
}

/// The resolved settings to use for an `upgrade` invocation.
#[derive(Debug, Clone)]
pub struct UpgradeSettings {
    pub packages: Vec<PackageName>,
    pub exclude: Vec<PackageName>,
    pub install_mirrors: PythonInstallMirrors,
    pub settings: ResolverSettings,
}

impl UpgradeSettings {
    /// Resolve the [`UpgradeSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: UpgradeArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let filesystem_install_mirrors = filesystem
            .as_ref()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();
        let (packages, exclude, options) =
            upgrade_options(args, configured_indexes(filesystem.as_ref()))?;
        let mut settings = combine_resolver_settings(options, filesystem, &environment);
        settings.upgrade = if packages.is_empty() {
            Upgrade::default()
        } else {
            Upgrade::from_packages(packages.clone())
        };

        Ok(Self {
            packages,
            exclude,
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
            settings,
        })
    }
}

/// The resolved settings to use for a `lock` invocation.
#[derive(Debug, Clone)]
pub struct MetadataSettings {
    #[expect(dead_code)]
    script: Option<PathBuf>,
    pub lock_check: LockCheck,
    pub frozen: Option<FrozenSource>,
    pub sync: Option<Modifications>,
    pub active: ActiveEnvironment,
    pub python: Option<String>,
    pub install_mirrors: PythonInstallMirrors,
    pub refresh: Refresh,
    pub settings: ResolverSettings,
    pub malware_settings: MalwareCheckSettings,
}

impl MetadataSettings {
    /// Resolve the [`LockSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: Box<MetadataArgs>,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let MetadataArgs {
            script,
            locked,
            no_locked,
            frozen,
            no_frozen,
            resolver,
            build,
            refresh,
            sync,
            exact,
            active,
            python,
        } = *args;

        let filesystem_install_mirrors = filesystem
            .as_ref()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        // Resolve flags from CLI and environment variables.
        let locked = resolve_lock_check(locked, no_locked, LockedFlag::Locked, environment.locked);
        let frozen = resolve_frozen(frozen, no_frozen, FrozenFlag::Frozen, environment.frozen);

        let (locked, frozen) = resolve_lock_flags(locked, frozen)?;

        let malware_settings = MalwareCheckSettings::resolve(filesystem.as_ref(), &environment);

        Ok(Self {
            script,
            lock_check: locked,
            frozen,
            sync: sync.then_some(if exact {
                Modifications::Exact
            } else {
                Modifications::Sufficient
            }),
            active: Some(active).into(),
            python: python.and_then(Maybe::into_option),
            refresh: Refresh::try_from(refresh)?,
            settings: resolve_resolver_settings(resolver, build, filesystem, &environment)?,
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
            malware_settings,
        })
    }
}

/// The resolved settings to use for a `add` invocation.
#[expect(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct AddSettings {
    pub lock_check: LockCheck,
    pub frozen: Option<FrozenSource>,
    pub active: ActiveEnvironment,
    pub no_sync: bool,
    pub packages: Vec<String>,
    pub requirements: Vec<RequirementsInput>,
    pub constraints: Vec<RequirementsInput>,
    pub marker: Option<MarkerTree>,
    pub dependency_type: DependencyType,
    pub editable: Option<EditableMode>,
    pub extras: Vec<ExtraName>,
    pub raw: bool,
    pub bounds: Option<AddBoundsKind>,
    pub rev: Option<String>,
    pub tag: Option<String>,
    pub branch: Option<String>,
    pub lfs: GitLfsSetting,
    pub package: Option<PackageName>,
    pub script: Option<PathBuf>,
    pub python: Option<String>,
    pub workspace: Option<bool>,
    pub no_install_project: bool,
    pub only_install_project: bool,
    pub no_install_workspace: bool,
    pub only_install_workspace: bool,
    pub no_install_local: bool,
    pub only_install_local: bool,
    pub no_install_package: Vec<PackageName>,
    pub only_install_package: Vec<PackageName>,
    pub install_mirrors: PythonInstallMirrors,
    pub refresh: Refresh,
    pub indexes: Vec<Index>,
    pub settings: ResolverInstallerSettings,
    pub malware_settings: MalwareCheckSettings,
}

impl AddSettings {
    /// Resolve the [`AddSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: AddArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let AddArgs {
            packages,
            requirements,
            constraints,
            marker,
            dev,
            optional,
            group,
            editable,
            no_editable,
            no_editable_package,
            extra,
            raw,
            bounds,
            rev,
            tag,
            branch,
            lfs,
            no_sync,
            locked,
            no_locked,
            frozen,
            no_frozen,
            active,
            no_active,
            installer,
            build,
            refresh,
            package,
            script,
            python,
            workspace,
            no_workspace,
            no_install_project,
            only_install_project,
            no_install_workspace,
            only_install_workspace,
            no_install_local,
            only_install_local,
            no_install_package,
            only_install_package,
        } = args;

        // Resolve flags from CLI and environment variables.
        let dev = dev || environment.dev.value == Some(true);
        let (editable, no_editable) = resolve_flag_pair(
            editable,
            no_editable,
            "editable",
            "no-editable",
            None,
            Some(environment.no_editable),
        );

        let (no_install_project, only_install_project) = resolve_flag_pair(
            no_install_project,
            only_install_project,
            "no-install-project",
            "only-install-project",
            Some(environment.no_install_project),
            Some(environment.only_install_project),
        );
        let (no_install_workspace, only_install_workspace) = resolve_flag_pair(
            no_install_workspace,
            only_install_workspace,
            "no-install-workspace",
            "only-install-workspace",
            Some(environment.no_install_workspace),
            Some(environment.only_install_workspace),
        );
        let (no_install_local, only_install_local) = resolve_flag_pair(
            no_install_local,
            only_install_local,
            "no-install-local",
            "only-install-local",
            Some(environment.no_install_local),
            Some(environment.only_install_local),
        );
        check_conflicts(no_install_project, only_install_project)?;
        check_conflicts(no_install_workspace, only_install_workspace)?;
        check_conflicts(no_install_local, only_install_local)?;

        let dependency_type = if let Some(extra) = optional {
            DependencyType::Optional(extra)
        } else if let Some(group) = group {
            DependencyType::Group(group)
        } else if dev {
            DependencyType::Dev
        } else {
            DependencyType::Production
        };

        // If the user passed an `--index-url` or `--extra-index-url`, warn.
        if installer
            .index_args
            .index_url
            .as_ref()
            .is_some_and(Maybe::is_some)
        {
            if script.is_some() {
                warn_user_once!(
                    "Indexes specified via `--index-url` will not be persisted to the script; use `--default-index` instead."
                );
            } else {
                warn_user_once!(
                    "Indexes specified via `--index-url` will not be persisted to the `pyproject.toml` file; use `--default-index` instead."
                );
            }
        }

        if installer
            .index_args
            .extra_index_url
            .as_ref()
            .is_some_and(|extra_index_url| extra_index_url.iter().any(Maybe::is_some))
        {
            if script.is_some() {
                warn_user_once!(
                    "Indexes specified via `--extra-index-url` will not be persisted to the script; use `--index` instead."
                );
            } else {
                warn_user_once!(
                    "Indexes specified via `--extra-index-url` will not be persisted to the `pyproject.toml` file; use `--index` instead."
                );
            }
        }

        let filesystem_install_mirrors = filesystem
            .as_ref()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let bounds = bounds.or(filesystem.as_ref().and_then(|fs| fs.add.add_bounds));
        let lfs = GitLfsSetting::new(lfs.then_some(true), environment.lfs);

        // Resolve flags from CLI and environment variables.
        let locked = resolve_lock_check(locked, no_locked, LockedFlag::Locked, environment.locked);
        let frozen = resolve_frozen(frozen, no_frozen, FrozenFlag::Frozen, environment.frozen);
        let no_sync = resolve_flag(no_sync, "no-sync", environment.no_sync);

        let (locked, frozen) = resolve_lock_flags(locked, frozen)?;

        // Check for conflicts between no_sync and frozen.
        check_conflicts(no_sync, frozen.map_or(Flag::Disabled, Flag::from))?;

        let no_install_package_flag = if no_install_package.is_empty() {
            Flag::disabled()
        } else {
            Flag::from_cli("no-install-package")
        };
        let only_install_package_flag = if only_install_package.is_empty() {
            Flag::disabled()
        } else {
            Flag::from_cli("only-install-package")
        };

        for install_flag in [
            no_install_project,
            no_install_workspace,
            no_install_local,
            only_install_project,
            only_install_workspace,
            only_install_local,
            no_install_package_flag,
            only_install_package_flag,
        ] {
            check_conflicts(install_flag, frozen.map_or(Flag::Disabled, Flag::from))?;
            check_conflicts(install_flag, no_sync)?;
        }

        let no_install_project = no_install_project.is_enabled();
        let only_install_project = only_install_project.is_enabled();
        let no_install_workspace = no_install_workspace.is_enabled();
        let only_install_workspace = only_install_workspace.is_enabled();
        let no_install_local = no_install_local.is_enabled();
        let only_install_local = only_install_local.is_enabled();

        let malware_settings = MalwareCheckSettings::resolve(filesystem.as_ref(), &environment);
        let active = flag(active, no_active, "active")?.into();
        let workspace = flag(workspace, no_workspace, "workspace")?;
        let editable = EditableMode::from_args(
            flag(editable.into(), no_editable.into(), "editable")?,
            no_editable_package,
        );
        let refresh = Refresh::try_from(refresh)?;
        let options =
            resolver_installer_options(installer, build, configured_indexes(filesystem.as_ref()))?;
        let indexes = options.indexes.index.clone().unwrap_or_default();

        Ok(Self {
            lock_check: locked,
            frozen,
            active,
            no_sync: no_sync.is_enabled(),
            packages,
            requirements,
            constraints: constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            marker,
            dependency_type,
            raw,
            bounds,
            rev,
            tag,
            branch,
            lfs,
            package,
            script,
            python: python.and_then(Maybe::into_option),
            workspace,
            no_install_project,
            only_install_project,
            no_install_workspace,
            only_install_workspace,
            no_install_local,
            only_install_local,
            no_install_package,
            only_install_package,
            editable,
            extras: extra.unwrap_or_default(),
            refresh,
            indexes,
            settings: combine_resolver_installer_settings(options, filesystem, &environment),
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
            malware_settings,
        })
    }
}

/// The resolved settings to use for a `remove` invocation.
#[derive(Debug, Clone)]
pub struct RemoveSettings {
    pub lock_check: LockCheck,
    pub frozen: Option<FrozenSource>,
    pub active: ActiveEnvironment,
    pub no_sync: bool,
    pub packages: Vec<PackageName>,
    pub dependency_type: DependencyType,
    pub package: Option<PackageName>,
    pub script: Option<PathBuf>,
    pub python: Option<String>,
    pub install_mirrors: PythonInstallMirrors,
    pub refresh: Refresh,
    pub settings: ResolverInstallerSettings,
    pub malware_settings: MalwareCheckSettings,
}

impl RemoveSettings {
    /// Resolve the [`RemoveSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: RemoveArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let RemoveArgs {
            dev,
            optional,
            packages,
            group,
            no_sync,
            locked,
            no_locked,
            frozen,
            no_frozen,
            active,
            no_active,
            installer,
            build,
            refresh,
            package,
            script,
            python,
        } = args;

        // Resolve flags from CLI and environment variables.
        let dev = dev || environment.dev.value == Some(true);

        let dependency_type = if let Some(extra) = optional {
            DependencyType::Optional(extra)
        } else if let Some(group) = group {
            DependencyType::Group(group)
        } else if dev {
            DependencyType::Dev
        } else {
            DependencyType::Production
        };

        let filesystem_install_mirrors = filesystem
            .as_ref()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let packages = packages
            .into_iter()
            .map(|requirement| requirement.name)
            .collect();

        // Resolve flags from CLI and environment variables.
        let locked = resolve_lock_check(locked, no_locked, LockedFlag::Locked, environment.locked);
        let frozen = resolve_frozen(frozen, no_frozen, FrozenFlag::Frozen, environment.frozen);
        let no_sync = resolve_flag(no_sync, "no-sync", environment.no_sync);

        let (locked, frozen) = resolve_lock_flags(locked, frozen)?;

        // Check for conflicts between no_sync and frozen.
        check_conflicts(no_sync, frozen.map_or(Flag::Disabled, Flag::from))?;

        let malware_settings = MalwareCheckSettings::resolve(filesystem.as_ref(), &environment);

        Ok(Self {
            lock_check: locked,
            frozen,
            active: flag(active, no_active, "active")?.into(),
            no_sync: no_sync.is_enabled(),
            packages,
            dependency_type,
            package,
            script,
            python: python.and_then(Maybe::into_option),
            refresh: Refresh::try_from(refresh)?,
            settings: resolve_resolver_installer_settings(
                installer,
                build,
                filesystem,
                &environment,
            )?,
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
            malware_settings,
        })
    }
}

/// The resolved settings to use for a `version` invocation.
#[derive(Debug, Clone)]
pub struct VersionSettings {
    pub value: Option<String>,
    pub bump: Vec<VersionBumpSpec>,
    pub short: bool,
    pub output_format: VersionFormat,
    pub dry_run: bool,
    pub lock_check: LockCheck,
    pub frozen: Option<FrozenSource>,
    pub active: ActiveEnvironment,
    pub no_sync: bool,
    pub package: Option<PackageName>,
    pub python: Option<String>,
    pub install_mirrors: PythonInstallMirrors,
    pub refresh: Refresh,
    pub settings: ResolverInstallerSettings,
    pub malware_settings: MalwareCheckSettings,
}

impl VersionSettings {
    /// Resolve the [`RemoveSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: VersionArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let VersionArgs {
            value,
            bump,
            short,
            output_format,
            dry_run,
            no_sync,
            locked,
            no_locked,
            frozen,
            no_frozen,
            active,
            no_active,
            installer,
            build,
            refresh,
            package,
            python,
        } = args;

        let filesystem_install_mirrors = filesystem
            .as_ref()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        // Resolve flags from CLI and environment variables.
        let locked = resolve_lock_check(locked, no_locked, LockedFlag::Locked, environment.locked);
        let frozen = resolve_frozen(frozen, no_frozen, FrozenFlag::Frozen, environment.frozen);
        let no_sync = resolve_flag(no_sync, "no-sync", environment.no_sync);

        let (locked, frozen) = resolve_lock_flags(locked, frozen)?;

        // Check for conflicts between no_sync and frozen.
        check_conflicts(no_sync, frozen.map_or(Flag::Disabled, Flag::from))?;

        let malware_settings = MalwareCheckSettings::resolve(filesystem.as_ref(), &environment);

        Ok(Self {
            value,
            bump,
            short,
            output_format,
            dry_run,
            lock_check: locked,
            frozen,
            active: flag(active, no_active, "active")?.into(),
            no_sync: no_sync.is_enabled(),
            package,
            python: python.and_then(Maybe::into_option),
            refresh: Refresh::try_from(refresh)?,
            settings: resolve_resolver_installer_settings(
                installer,
                build,
                filesystem,
                &environment,
            )?,
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
            malware_settings,
        })
    }
}

/// The resolved settings to use for a `tree` invocation.
#[derive(Debug, Clone)]
pub struct TreeSettings {
    pub groups: DependencyGroups,
    pub lock_check: LockCheck,
    pub frozen: Option<FrozenSource>,
    pub universal: bool,
    pub format: TreeFormat,
    pub depth: u8,
    pub prune: Vec<PackageName>,
    pub package: Vec<PackageName>,
    pub no_dedupe: bool,
    pub invert: bool,
    pub outdated: bool,
    pub show_sizes: bool,
    pub script: Option<PathBuf>,
    pub python_version: Option<PythonVersion>,
    pub python_platform: Option<TargetTriple>,
    pub python: Option<String>,
    pub install_mirrors: PythonInstallMirrors,
    pub resolver: ResolverSettings,
}

impl TreeSettings {
    /// Resolve the [`TreeSettings`] from the CLI and workspace configuration.
    pub fn resolve(
        args: TreeArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let TreeArgs {
            tree,
            universal,
            format,
            dependency_groups:
                ProjectDependencyGroupsArgs {
                    dev,
                    no_dev,
                    only_dev,
                    group,
                    no_group,
                    no_default_groups,
                    only_group,
                    all_groups,
                },
            locked,
            no_locked,
            frozen,
            no_frozen,
            build,
            resolver,
            script,
            python_version,
            python_platform,
            python,
        } = args;

        let filesystem_install_mirrors = filesystem
            .as_ref()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        // Resolve flags from CLI and environment variables.
        let locked = resolve_lock_check(locked, no_locked, LockedFlag::Locked, environment.locked);
        let frozen = resolve_frozen(frozen, no_frozen, FrozenFlag::Frozen, environment.frozen);

        let (locked, frozen) = resolve_lock_flags(locked, frozen)?;

        let (dev, no_dev) = resolve_flag_pair(
            dev,
            no_dev,
            "dev",
            "no-dev",
            Some(environment.dev),
            Some(environment.no_dev),
        );

        Ok(Self {
            groups: DependencyGroups::from_args(
                DevMode::from_args(dev.into(), no_dev.into(), only_dev),
                group,
                if no_group.is_empty() {
                    environment.no_group.clone().unwrap_or_default()
                } else {
                    no_group
                },
                no_default_groups,
                only_group,
                all_groups,
            ),
            lock_check: locked,
            frozen,
            universal,
            format,
            depth: tree.depth,
            prune: tree.prune,
            package: tree.package,
            no_dedupe: tree.no_dedupe,
            invert: tree.invert,
            outdated: tree.outdated,
            show_sizes: tree.show_sizes,
            script,
            python_version,
            python_platform,
            python: python.and_then(Maybe::into_option),
            resolver: resolve_resolver_settings(resolver, build, filesystem, &environment)?,
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
        })
    }
}

/// The resolved settings to use for an `export` invocation.
#[expect(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct ExportSettings {
    pub format: Option<ExportFormat>,
    pub all_packages: bool,
    pub package: Vec<PackageName>,
    pub prune: Vec<PackageName>,
    pub extras: ExtrasSpecification,
    pub groups: DependencyGroups,
    pub editable: Option<EditableMode>,
    pub hashes: bool,
    pub install_options: InstallOptions,
    pub batch: Option<PathBuf>,
    pub output_file: Option<PathBuf>,
    pub lock_check: LockCheck,
    pub frozen: Option<FrozenSource>,
    pub include_annotations: bool,
    pub include_header: bool,
    pub include_index_url: bool,
    pub include_find_links: bool,
    pub script: Option<PathBuf>,
    pub python: Option<String>,
    pub install_mirrors: PythonInstallMirrors,
    pub refresh: Refresh,
    pub settings: ResolverSettings,
}

impl ExportSettings {
    /// Resolve the [`ExportSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: ExportArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let ExportArgs {
            format,
            all_packages,
            package,
            prune,
            extra,
            all_extras,
            no_extra,
            no_all_extras,
            dependency_groups:
                ProjectDependencyGroupsArgs {
                    dev,
                    no_dev,
                    only_dev,
                    group,
                    no_group,
                    no_default_groups,
                    only_group,
                    all_groups,
                },
            annotate,
            no_annotate,
            header,
            no_header,
            emit_index_url,
            no_emit_index_url,
            emit_find_links,
            no_emit_find_links,
            editable,
            no_editable,
            no_editable_package,
            hashes,
            no_hashes,
            batch,
            output_file,
            no_emit_project,
            only_emit_project,
            no_emit_workspace,
            only_emit_workspace,
            no_emit_local,
            only_emit_local,
            no_emit_package,
            only_emit_package,
            locked,
            no_locked,
            frozen: frozen_cli,
            no_frozen,
            resolver,
            build,
            refresh,
            script,
            python,
        } = args;
        let filesystem_install_mirrors = filesystem
            .as_ref()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        // Resolve flags from CLI and environment variables.
        let locked = resolve_lock_check(locked, no_locked, LockedFlag::Locked, environment.locked);
        let frozen = resolve_frozen(
            frozen_cli,
            no_frozen,
            FrozenFlag::Frozen,
            environment.frozen,
        );

        let (locked, frozen) = resolve_lock_flags(locked, frozen)?;

        let (dev, no_dev) = resolve_flag_pair(
            dev,
            no_dev,
            "dev",
            "no-dev",
            Some(environment.dev),
            Some(environment.no_dev),
        );
        let (editable, no_editable) = resolve_flag_pair(
            editable,
            no_editable,
            "editable",
            "no-editable",
            None,
            Some(environment.no_editable),
        );

        Ok(Self {
            format,
            all_packages,
            package,
            prune,
            extras: ExtrasSpecification::from_args(
                extra.unwrap_or_default(),
                no_extra,
                // TODO(blueraft): support no_default_extras
                false,
                // TODO(blueraft): support only_extra
                vec![],
                flag(all_extras, no_all_extras, "all-extras")?.unwrap_or_default(),
            ),
            groups: DependencyGroups::from_args(
                DevMode::from_args(dev.into(), no_dev.into(), only_dev),
                group,
                if no_group.is_empty() {
                    environment.no_group.clone().unwrap_or_default()
                } else {
                    no_group
                },
                no_default_groups,
                only_group,
                all_groups,
            ),
            editable: EditableMode::from_args(
                flag(editable.into(), no_editable.into(), "editable")?,
                no_editable_package,
            ),
            hashes: flag(hashes, no_hashes, "hashes")?.unwrap_or(true),
            install_options: InstallOptions::new(
                no_emit_project,
                only_emit_project,
                no_emit_workspace,
                only_emit_workspace,
                no_emit_local,
                only_emit_local,
                no_emit_package,
                only_emit_package,
            ),
            batch,
            output_file,
            lock_check: locked,
            frozen,
            include_annotations: flag(annotate, no_annotate, "annotate")?.unwrap_or(true),
            include_header: flag(header, no_header, "header")?.unwrap_or(true),
            include_index_url: flag(emit_index_url, no_emit_index_url, "emit-index-url")?
                .unwrap_or(false),
            include_find_links: flag(emit_find_links, no_emit_find_links, "emit-find-links")?
                .unwrap_or(false),
            script,
            python: python.and_then(Maybe::into_option),
            refresh: Refresh::try_from(refresh)?,
            settings: resolve_resolver_settings(resolver, build, filesystem, &environment)?,
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
        })
    }
}

/// The resolved settings to use for a `format` invocation.
#[derive(Debug, Clone)]
pub struct FormatSettings {
    pub ruff_path: Option<PathBuf>,
    pub check: bool,
    pub diff: bool,
    pub extra_args: Vec<String>,
    pub version: Option<String>,
    pub exclude_newer: Option<jiff::Timestamp>,
    pub no_project: bool,
    pub show_version: bool,
}

impl FormatSettings {
    /// Resolve the [`FormatSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: FormatArgs,
        _filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> Self {
        let FormatArgs {
            check,
            diff,
            extra_args,
            version,
            exclude_newer,
            no_project,
            show_version,
        } = args;

        Self {
            ruff_path: environment.ruff_path,
            check,
            diff,
            extra_args,
            version,
            exclude_newer: exclude_newer
                .and_then(ExcludeNewerOverride::into_value)
                .map(|value| value.timestamp()),
            no_project,
            show_version,
        }
    }
}

/// The resolved settings to use for a `check` invocation.
#[derive(Debug, Clone)]
pub struct CheckSettings {
    pub ty_path: Option<PathBuf>,
    #[expect(dead_code)]
    script: Option<PathBuf>,
    pub fix: bool,
    pub all_packages: bool,
    pub package: Vec<PackageName>,
    pub extras: ExtrasSpecification,
    pub groups: DependencyGroups,
    pub lock_check: LockCheck,
    pub frozen: Option<FrozenSource>,
    pub no_sync: bool,
    pub no_install_project: bool,
    pub isolated: bool,
    pub python: Option<String>,
    pub install_mirrors: PythonInstallMirrors,
    pub refresh: Refresh,
    pub settings: ResolverInstallerSettings,
    pub ty_version: Option<String>,
    pub show_version: bool,
    pub show_command: bool,
    pub no_project: bool,
    pub malware_settings: MalwareCheckSettings,
}

impl CheckSettings {
    /// Resolve the [`CheckSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: CheckArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let CheckArgs {
            fix,
            all_packages,
            package,
            script,
            extra,
            all_extras,
            no_extra,
            no_all_extras,
            dependency_groups:
                ProjectDependencyGroupsArgs {
                    dev,
                    no_dev,
                    only_dev,
                    group,
                    no_group,
                    no_default_groups,
                    only_group,
                    all_groups,
                },
            locked,
            no_locked,
            frozen,
            no_frozen,
            no_sync,
            no_install_project,
            isolated,
            python,
            ty_version,
            show_version,
            show_command,
            no_project,
            installer,
            build,
            refresh,
        } = args;

        let filesystem_install_mirrors = filesystem
            .as_ref()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let locked = resolve_lock_check(locked, no_locked, LockedFlag::Locked, environment.locked);
        let frozen = resolve_frozen(frozen, no_frozen, FrozenFlag::Frozen, environment.frozen);
        let no_sync = resolve_flag(no_sync, "no-sync", environment.no_sync);
        let no_install_project = resolve_flag(
            no_install_project,
            "no-install-project",
            environment.no_install_project,
        );
        let isolated = resolve_flag(isolated, "isolated", environment.isolated).is_enabled();
        let (locked, frozen) = resolve_lock_flags(locked, frozen)?;
        check_conflicts(no_install_project, no_sync)?;
        if script.is_some() {
            check_conflicts(no_install_project, Flag::from_cli("script"))?;
        }
        if no_project {
            check_conflicts(no_install_project, Flag::from_cli("no-project"))?;
        }

        let (dev, no_dev) = resolve_flag_pair(
            dev,
            no_dev,
            "dev",
            "no-dev",
            Some(environment.dev),
            Some(environment.no_dev),
        );
        let malware_settings = MalwareCheckSettings::resolve(filesystem.as_ref(), &environment);
        let settings =
            resolve_resolver_installer_settings(installer, build, filesystem, &environment)?;
        Ok(Self {
            ty_path: environment.ty_path,
            script,
            fix,
            all_packages,
            package,
            extras: ExtrasSpecification::from_args(
                extra.unwrap_or_default(),
                no_extra,
                false,
                vec![],
                flag(all_extras, no_all_extras, "all-extras")?.unwrap_or_default(),
            ),
            groups: DependencyGroups::from_args(
                DevMode::from_args(dev.into(), no_dev.into(), only_dev),
                group,
                if no_group.is_empty() {
                    environment.no_group.clone().unwrap_or_default()
                } else {
                    no_group
                },
                no_default_groups,
                only_group,
                all_groups,
            ),
            lock_check: locked,
            frozen,
            no_sync: no_sync.is_enabled(),
            no_install_project: no_install_project.is_enabled(),
            isolated,
            python: python.and_then(Maybe::into_option),
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
            refresh: Refresh::try_from(refresh)?,
            settings,
            ty_version,
            show_version,
            show_command,
            no_project,
            malware_settings,
        })
    }
}

/// The resolved settings to use for an `audit` invocation.
#[derive(Debug, Clone)]
pub struct AuditSettings {
    pub extras: ExtrasSpecification,
    pub groups: DependencyGroups,
    pub lock_check: LockCheck,
    pub frozen: Option<FrozenSource>,
    pub python_version: Option<PythonVersion>,
    pub python_platform: Option<TargetTriple>,
    pub install_mirrors: PythonInstallMirrors,
    pub settings: ResolverSettings,
    pub output_format: AuditOutputFormat,
    pub service_format: VulnerabilityServiceFormat,
    pub service_url: Option<DisplaySafeUrl>,
    pub ignore: Vec<VulnerabilityID>,
    pub ignore_until_fixed: Vec<VulnerabilityID>,
}

impl AuditSettings {
    /// Resolve the [`AuditSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: AuditArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let AuditArgs {
            no_extra,
            no_dev,
            no_group,
            no_default_groups,
            only_group,
            only_dev,
            script: _,
            python_version,
            python_platform,
            locked,
            no_locked,
            frozen,
            no_frozen,
            audit:
                AuditCommonArgs {
                    offline: _,
                    output_format,
                    ignore,
                    ignore_until_fixed,
                    service_format,
                    service_url,
                },
            build,
            resolver,
        } = args;

        let filesystem_install_mirrors = filesystem
            .as_ref()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let filesystem_audit = filesystem
            .as_ref()
            .and_then(|fs| fs.audit.clone())
            .unwrap_or_default();

        let no_dev = no_dev || environment.no_dev.value == Some(true);

        // Resolve flags from CLI and environment variables.
        let locked = resolve_lock_check(locked, no_locked, LockedFlag::Locked, environment.locked);
        let frozen = resolve_frozen(frozen, no_frozen, FrozenFlag::Frozen, environment.frozen);

        let (locked, frozen) = resolve_lock_flags(locked, frozen)?;

        // Audit includes all groups by default, regardless of `tool.uv.default-groups`.
        // `--no-default-groups` disables that implicit selection.
        let all_groups = only_group.is_empty() && !only_dev && !no_default_groups;

        Ok(Self {
            extras: ExtrasSpecification::from_args(
                vec![],
                no_extra,
                // TODO(ww): support no_default_extras?
                false,
                // TODO(ww): support only_extra?
                vec![],
                true,
            ),
            groups: DependencyGroups::from_args(
                DevMode::from_args(all_groups, no_dev, only_dev),
                vec![],
                if no_group.is_empty() {
                    environment.no_group.clone().unwrap_or_default()
                } else {
                    no_group
                },
                no_default_groups,
                only_group,
                all_groups,
            ),
            lock_check: locked,
            frozen,
            python_version,
            python_platform,
            settings: resolve_resolver_settings(resolver, build, filesystem, &environment)?,
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
            output_format,
            service_format,
            service_url,
            ignore: {
                let config_ignore = filesystem_audit.ignore.unwrap_or_default();
                let mut merged = ignore;
                merged.extend(config_ignore);
                merged.into_iter().map(VulnerabilityID::new).collect()
            },
            ignore_until_fixed: {
                let config_ignore_until_fixed =
                    filesystem_audit.ignore_until_fixed.unwrap_or_default();
                let mut merged = ignore_until_fixed;
                merged.extend(config_ignore_until_fixed);
                merged.into_iter().map(VulnerabilityID::new).collect()
            },
        })
    }
}

fn workspace_overrides(filesystem: Option<&FilesystemOptions>) -> Vec<Override<Requirement>> {
    let mut overrides = Vec::new();
    for dependency in filesystem
        .and_then(|configuration| configuration.override_dependencies.as_ref())
        .into_iter()
        .flatten()
    {
        match dependency {
            OverrideDependency::Requirement(requirement) => {
                overrides.push(Override::Requirement(Requirement::from(
                    requirement
                        .clone()
                        .with_origin(RequirementOrigin::Workspace),
                )));
            }
            OverrideDependency::Package(package) => {
                overrides.push(Override::Package(PackageOverride {
                    package: package.package.clone(),
                    dependencies: package
                        .dependencies
                        .iter()
                        .cloned()
                        .map(|requirement| {
                            Requirement::from(requirement.with_origin(RequirementOrigin::Workspace))
                        })
                        .collect(),
                }));
            }
        }
    }
    overrides
}

/// The resolved settings to use for a `pip compile` invocation.
#[derive(Debug, Clone)]
pub struct PipCompileSettings {
    pub format: Option<PipCompileFormat>,
    pub src_file: Vec<RequirementsInput>,
    pub constraints: Vec<RequirementsInput>,
    pub overrides: Vec<RequirementsInput>,
    pub excludes: Vec<RequirementsInput>,
    pub build_constraints: Vec<RequirementsInput>,
    pub constraints_from_workspace: Vec<Requirement>,
    pub overrides_from_workspace: Vec<Override<Requirement>>,
    pub excludes_from_workspace: Vec<ExcludeDependency>,
    pub build_constraints_from_workspace: Vec<NameRequirementSpecification>,
    pub environments: SupportedEnvironments,
    pub required_environments: SupportedEnvironments,
    pub minimum_libc_version: Option<MinimumLibcVersion>,
    pub refresh: Refresh,
    pub settings: PipSettings,
}

impl PipCompileSettings {
    /// Resolve the [`PipCompileSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: PipCompileArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let PipCompileArgs {
            src_file,
            constraints:
                DependencyConstraintsArgs {
                    constraints,
                    overrides,
                    excludes,
                    build_constraints,
                },
            extra,
            all_extras,
            no_all_extras,
            refresh,
            no_deps,
            deps,
            group,
            output_file,
            format,
            no_strip_extras,
            strip_extras,
            no_strip_markers,
            strip_markers,
            no_annotate,
            annotate,
            no_header,
            header,
            annotation_style,
            custom_compile_command,
            resolver,
            python,
            system,
            no_system,
            generate_hashes,
            no_generate_hashes,
            no_build,
            build,
            no_binary,
            only_binary,
            python_version,
            python_platform,
            universal,
            no_universal,
            no_emit_package,
            emit_index_url,
            no_emit_index_url,
            emit_find_links,
            no_emit_find_links,
            emit_build_options,
            no_emit_build_options,
            emit_marker_expression,
            no_emit_marker_expression,
            emit_index_annotation,
            no_emit_index_annotation,
            torch_backend,
            compat_args: _,
        } = args;

        let constraints_from_workspace = if let Some(configuration) = &filesystem {
            configuration
                .constraint_dependencies
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|requirement| {
                    Requirement::from(requirement.with_origin(RequirementOrigin::Workspace))
                })
                .collect()
        } else {
            Vec::new()
        };

        let overrides_from_workspace = workspace_overrides(filesystem.as_ref());

        let excludes_from_workspace = if let Some(configuration) = &filesystem {
            configuration
                .exclude_dependencies
                .clone()
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let build_constraints_from_workspace = if let Some(configuration) = &filesystem {
            configuration
                .build_constraint_dependencies
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|requirement| {
                    let (requirement, hashes) = requirement.into_parts();
                    NameRequirementSpecification {
                        requirement: Requirement::from(
                            requirement.with_origin(RequirementOrigin::Workspace),
                        ),
                        hashes,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        let environments = if let Some(configuration) = &filesystem {
            configuration.environments.clone().unwrap_or_default()
        } else {
            SupportedEnvironments::default()
        };

        let required_environments = if let Some(configuration) = &filesystem {
            configuration
                .required_environments
                .clone()
                .unwrap_or_default()
        } else {
            SupportedEnvironments::default()
        };

        let minimum_libc_version = filesystem
            .as_ref()
            .and_then(|configuration| configuration.minimum_libc_version);

        Ok(Self {
            format,
            src_file,
            constraints: constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            build_constraints: build_constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            overrides: overrides
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            excludes: excludes
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            constraints_from_workspace,
            overrides_from_workspace,
            excludes_from_workspace,
            build_constraints_from_workspace,
            environments,
            required_environments,
            minimum_libc_version,
            refresh: Refresh::try_from(refresh)?,
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system, "system")?,
                    no_build: flag(no_build, build, "build")?,
                    no_binary,
                    only_binary,
                    extra,
                    all_extras: flag(all_extras, no_all_extras, "all-extras")?,
                    no_deps: flag(no_deps, deps, "deps")?,
                    group: Some(group),
                    output_file,
                    no_strip_extras: flag(no_strip_extras, strip_extras, "strip-extras")?,
                    no_strip_markers: flag(no_strip_markers, strip_markers, "strip-markers")?,
                    no_annotate: flag(no_annotate, annotate, "annotate")?,
                    no_header: flag(no_header, header, "header")?,
                    custom_compile_command,
                    generate_hashes: flag(generate_hashes, no_generate_hashes, "generate-hashes")?,
                    python_version,
                    python_platform,
                    universal: flag(universal, no_universal, "universal")?,
                    no_emit_package,
                    emit_index_url: flag(emit_index_url, no_emit_index_url, "emit-index-url")?,
                    emit_find_links: flag(emit_find_links, no_emit_find_links, "emit-find-links")?,
                    emit_build_options: flag(
                        emit_build_options,
                        no_emit_build_options,
                        "emit-build-options",
                    )?,
                    emit_marker_expression: flag(
                        emit_marker_expression,
                        no_emit_marker_expression,
                        "emit-marker-expression",
                    )?,
                    emit_index_annotation: flag(
                        emit_index_annotation,
                        no_emit_index_annotation,
                        "emit-index-annotation",
                    )?,
                    annotation_style,
                    torch_backend,
                    ..resolver.into_pip_options(configured_indexes(filesystem.as_ref()))?
                },
                filesystem,
                environment,
            ),
        })
    }
}

/// The resolved settings to use for a `pip sync` invocation.
#[derive(Debug, Clone)]
pub struct PipSyncSettings {
    pub src_file: Vec<RequirementsInput>,
    pub constraints: Vec<RequirementsInput>,
    pub build_constraints: Vec<RequirementsInput>,
    pub dry_run: DryRun,
    pub output_format: PipInstallFormat,
    pub refresh: Refresh,
    pub settings: PipSettings,
}

impl PipSyncSettings {
    /// Resolve the [`PipSyncSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: Box<PipSyncArgs>,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let PipSyncArgs {
            src_file,
            constraints,
            build_constraints,
            extra,
            all_extras,
            no_all_extras,
            group,
            installer,
            refresh,
            hash_checking:
                HashCheckingArgs {
                    require_hashes,
                    no_require_hashes,
                    verify_hashes,
                    no_verify_hashes,
                },
            python,
            system,
            no_system,
            break_system_packages,
            no_break_system_packages,
            target,
            prefix,
            allow_empty_requirements,
            no_allow_empty_requirements,
            no_build,
            build,
            no_binary,
            only_binary,
            python_version,
            python_platform,
            strict,
            no_strict,
            dry_run,
            output_format,
            torch_backend,
            compat_args: _,
            check,
        } = *args;

        Ok(Self {
            src_file,
            constraints: constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            build_constraints: build_constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            dry_run: if check {
                DryRun::Check
            } else {
                DryRun::from_args(dry_run)
            },
            output_format,
            refresh: Refresh::try_from(refresh)?,
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system, "system")?,
                    break_system_packages: flag(
                        break_system_packages,
                        no_break_system_packages,
                        "break-system-packages",
                    )?,
                    target,
                    prefix,
                    require_hashes: flag(require_hashes, no_require_hashes, "require-hashes")?,
                    verify_hashes: flag(verify_hashes, no_verify_hashes, "verify-hashes")?,
                    no_build: flag(no_build, build, "build")?,
                    no_binary,
                    only_binary,
                    allow_empty_requirements: flag(
                        allow_empty_requirements,
                        no_allow_empty_requirements,
                        "allow-empty-requirements",
                    )?,
                    python_version,
                    python_platform,
                    strict: flag(strict, no_strict, "strict")?,
                    extra,
                    all_extras: flag(all_extras, no_all_extras, "all-extras")?,
                    group: Some(group),
                    torch_backend,
                    ..installer.into_pip_options(configured_indexes(filesystem.as_ref()))?
                },
                filesystem,
                environment,
            ),
        })
    }
}

/// The resolved settings to use for a `pip install` invocation.
#[derive(Debug, Clone)]
pub struct PipInstallSettings {
    pub package: Vec<String>,
    pub requirements: Vec<RequirementsInput>,
    pub editables: Vec<String>,
    pub editable: Option<EditableMode>,
    pub constraints: Vec<RequirementsInput>,
    pub overrides: Vec<RequirementsInput>,
    pub excludes: Vec<RequirementsInput>,
    pub build_constraints: Vec<RequirementsInput>,
    pub dry_run: DryRun,
    pub output_format: PipInstallFormat,
    pub constraints_from_workspace: Vec<Requirement>,
    pub overrides_from_workspace: Vec<Override<Requirement>>,
    pub excludes_from_workspace: Vec<ExcludeDependency>,
    pub build_constraints_from_workspace: Vec<NameRequirementSpecification>,
    pub modifications: Modifications,
    pub refresh: Refresh,
    pub settings: PipSettings,
}

impl PipInstallSettings {
    /// Resolve the [`PipInstallSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: PipInstallArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let PipInstallArgs {
            package,
            requirements,
            editable,
            no_editable,
            no_editable_package,
            constraints:
                DependencyConstraintsArgs {
                    constraints,
                    overrides,
                    excludes,
                    build_constraints,
                },
            extra,
            all_extras,
            no_all_extras,
            installer,
            refresh,
            no_deps,
            deps,
            group,
            hash_checking:
                HashCheckingArgs {
                    require_hashes,
                    no_require_hashes,
                    verify_hashes,
                    no_verify_hashes,
                },
            python,
            system,
            no_system,
            break_system_packages,
            no_break_system_packages,
            target,
            prefix,
            no_build,
            build,
            no_binary,
            only_binary,
            python_version,
            python_platform,
            inexact,
            exact,
            strict,
            no_strict,
            dry_run,
            output_format,
            torch_backend,
            compat_args: _,
            check,
        } = args;

        let constraints_from_workspace = if let Some(configuration) = &filesystem {
            configuration
                .constraint_dependencies
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|requirement| {
                    Requirement::from(requirement.with_origin(RequirementOrigin::Workspace))
                })
                .collect()
        } else {
            Vec::new()
        };

        let overrides_from_workspace = workspace_overrides(filesystem.as_ref());

        let excludes_from_workspace = if let Some(configuration) = &filesystem {
            configuration
                .exclude_dependencies
                .clone()
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let build_constraints_from_workspace = if let Some(configuration) = &filesystem {
            configuration
                .build_constraint_dependencies
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|requirement| {
                    let (requirement, hashes) = requirement.into_parts();
                    NameRequirementSpecification {
                        requirement: Requirement::from(
                            requirement.with_origin(RequirementOrigin::Workspace),
                        ),
                        hashes,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        Ok(Self {
            package,
            requirements,
            editables: editable,
            constraints: constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            overrides: overrides
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            excludes: excludes
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            build_constraints: build_constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            dry_run: if check {
                DryRun::Check
            } else {
                DryRun::from_args(dry_run)
            },
            output_format,
            constraints_from_workspace,
            overrides_from_workspace,
            excludes_from_workspace,
            build_constraints_from_workspace,
            modifications: if flag(exact, inexact, "inexact")?.unwrap_or(false) {
                Modifications::Exact
            } else {
                Modifications::Sufficient
            },
            editable: EditableMode::from_args(
                if no_editable || environment.no_editable.value == Some(true) {
                    Some(false)
                } else {
                    None
                },
                no_editable_package,
            ),
            refresh: Refresh::try_from(refresh)?,
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system, "system")?,
                    break_system_packages: flag(
                        break_system_packages,
                        no_break_system_packages,
                        "break-system-packages",
                    )?,
                    target,
                    prefix,
                    no_build: flag(no_build, build, "build")?,
                    no_binary,
                    only_binary,
                    strict: flag(strict, no_strict, "strict")?,
                    extra,
                    all_extras: flag(all_extras, no_all_extras, "all-extras")?,
                    group: Some(group),
                    no_deps: flag(no_deps, deps, "deps")?,
                    python_version,
                    python_platform,
                    require_hashes: flag(require_hashes, no_require_hashes, "require-hashes")?,
                    verify_hashes: flag(verify_hashes, no_verify_hashes, "verify-hashes")?,
                    torch_backend,
                    ..installer.into_pip_options(configured_indexes(filesystem.as_ref()))?
                },
                filesystem,
                environment,
            ),
        })
    }
}

/// The resolved settings to use for a `pip uninstall` invocation.
#[derive(Debug, Clone)]
pub struct PipUninstallSettings {
    pub package: Vec<String>,
    pub requirements: Vec<RequirementsInput>,
    pub dry_run: DryRun,
    pub settings: PipSettings,
}

impl PipUninstallSettings {
    /// Resolve the [`PipUninstallSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: PipUninstallArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let PipUninstallArgs {
            package,
            requirements,
            python,
            keyring_provider,
            system,
            no_system,
            break_system_packages,
            no_break_system_packages,
            target,
            prefix,
            dry_run,
            compat_args: _,
        } = args;

        Ok(Self {
            package,
            requirements,
            dry_run: DryRun::from_args(dry_run),
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system, "system")?,
                    break_system_packages: flag(
                        break_system_packages,
                        no_break_system_packages,
                        "break-system-packages",
                    )?,
                    target,
                    prefix,
                    keyring_provider,
                    ..PipOptions::default()
                },
                filesystem,
                environment,
            ),
        })
    }
}

/// The resolved settings to use for a `pip freeze` invocation.
#[derive(Debug, Clone)]
pub struct PipFreezeSettings {
    pub exclude_editable: bool,
    pub exclude: FxHashSet<PackageName>,
    pub paths: Option<Vec<PathBuf>>,
    pub settings: PipSettings,
}

impl PipFreezeSettings {
    /// Resolve the [`PipFreezeSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: PipFreezeArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let PipFreezeArgs {
            exclude_editable,
            exclude,
            strict,
            no_strict,
            python,
            paths,
            system,
            no_system,
            target,
            prefix,
            compat_args: _,
        } = args;

        Ok(Self {
            exclude_editable,
            exclude: exclude.into_iter().collect(),
            paths,
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system, "system")?,
                    strict: flag(strict, no_strict, "strict")?,
                    target,
                    prefix,
                    ..PipOptions::default()
                },
                filesystem,
                environment,
            ),
        })
    }
}

/// The resolved settings to use for a `pip list` invocation.
#[derive(Debug, Clone)]
pub struct PipListSettings {
    pub editable: Option<bool>,
    pub exclude: FxHashSet<PackageName>,
    pub format: ListFormat,
    pub outdated: bool,
    pub settings: PipSettings,
}

impl PipListSettings {
    /// Resolve the [`PipListSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: PipListArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let PipListArgs {
            editable,
            exclude_editable,
            exclude,
            format,
            outdated,
            no_outdated,
            strict,
            no_strict,
            fetch,
            python,
            system,
            no_system,
            target,
            prefix,
            compat_args: _,
        } = args;

        Ok(Self {
            editable: flag(editable, exclude_editable, "exclude-editable")?,
            exclude: exclude.into_iter().collect(),
            format,
            outdated: flag(outdated, no_outdated, "outdated")?.unwrap_or(false),
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system, "system")?,
                    strict: flag(strict, no_strict, "strict")?,
                    target,
                    prefix,
                    ..fetch.into_pip_options(configured_indexes(filesystem.as_ref()))?
                },
                filesystem,
                environment,
            ),
        })
    }
}

/// The resolved settings to use for a `pip show` invocation.
#[derive(Debug, Clone)]
pub struct PipShowSettings {
    pub package: Vec<PackageName>,
    pub files: bool,
    pub settings: PipSettings,
}

impl PipShowSettings {
    /// Resolve the [`PipShowSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: PipShowArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let PipShowArgs {
            package,
            strict,
            no_strict,
            files,
            python,
            system,
            no_system,
            target,
            prefix,
            compat_args: _,
        } = args;

        Ok(Self {
            package,
            files,
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system, "system")?,
                    strict: flag(strict, no_strict, "strict")?,
                    target,
                    prefix,
                    ..PipOptions::default()
                },
                filesystem,
                environment,
            ),
        })
    }
}

/// The resolved settings to use for a `pip tree` invocation.
#[derive(Debug, Clone)]
pub struct PipTreeSettings {
    pub show_version_specifiers: bool,
    pub depth: u8,
    pub prune: Vec<PackageName>,
    pub package: Vec<PackageName>,
    pub no_dedupe: bool,
    pub invert: bool,
    pub outdated: bool,
    pub settings: PipSettings,
}

impl PipTreeSettings {
    /// Resolve the [`PipTreeSettings`] from the CLI and workspace configuration.
    pub fn resolve(
        args: PipTreeArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let PipTreeArgs {
            show_version_specifiers,
            tree,
            strict,
            no_strict,
            fetch,
            python,
            system,
            no_system,
            compat_args: _,
        } = args;

        Ok(Self {
            show_version_specifiers,
            depth: tree.depth,
            prune: tree.prune,
            no_dedupe: tree.no_dedupe,
            invert: tree.invert,
            package: tree.package,
            outdated: tree.outdated,
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system, "system")?,
                    strict: flag(strict, no_strict, "strict")?,
                    ..fetch.into_pip_options(configured_indexes(filesystem.as_ref()))?
                },
                filesystem,
                environment,
            ),
        })
    }
}

/// The resolved settings to use for a `pip check` invocation.
#[derive(Debug, Clone)]
pub struct PipCheckSettings {
    pub settings: PipSettings,
}

impl PipCheckSettings {
    /// Resolve the [`PipCheckSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: PipCheckArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let PipCheckArgs {
            python,
            system,
            no_system,
            python_version,
            python_platform,
        } = args;

        Ok(Self {
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system, "system")?,
                    python_version,
                    python_platform,
                    ..PipOptions::default()
                },
                filesystem,
                environment,
            ),
        })
    }
}

/// The resolved settings to use for a `build` invocation.
#[derive(Debug, Clone)]
pub struct BuildSettings {
    pub skip_dependency_check: bool,
    pub src: Option<PathBuf>,
    pub package: Option<PackageName>,
    pub all_packages: bool,
    pub out_dir: Option<PathBuf>,
    pub sdist: bool,
    pub wheel: bool,
    pub list: bool,
    pub build_logs: bool,
    pub gitignore: bool,
    pub force_pep517: bool,
    pub clear: bool,
    pub build_constraints: Vec<RequirementsInput>,
    pub build_constraints_from_workspace: Vec<NameRequirementSpecification>,
    pub hash_checking: Option<HashCheckingMode>,
    pub python: Option<String>,
    pub install_mirrors: PythonInstallMirrors,
    pub refresh: Refresh,
    pub settings: ResolverSettings,
}

impl BuildSettings {
    /// Resolve the [`BuildSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: BuildArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let BuildArgs {
            skip_dependency_check,
            src,
            out_dir,
            package,
            all_packages,
            sdist,
            wheel,
            list,
            force_pep517,
            clear,
            build_constraints,
            hash_checking:
                HashCheckingArgs {
                    require_hashes,
                    no_require_hashes,
                    verify_hashes,
                    no_verify_hashes,
                },
            build_logs,
            no_build_logs,
            create_gitignore,
            no_create_gitignore,
            python,
            build,
            refresh,
            resolver,
        } = args;
        let filesystem_install_mirrors = match &filesystem {
            Some(fs) => fs.install_mirrors.clone(),
            None => PythonInstallMirrors::default(),
        };
        let build_constraints_from_workspace = if let Some(configuration) = &filesystem {
            configuration
                .build_constraint_dependencies
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|requirement| {
                    let (requirement, hashes) = requirement.into_parts();
                    NameRequirementSpecification {
                        requirement: Requirement::from(
                            requirement.with_origin(RequirementOrigin::Workspace),
                        ),
                        hashes,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        Ok(Self {
            skip_dependency_check,
            src,
            package,
            all_packages,
            out_dir,
            sdist,
            wheel,
            list,
            build_logs: flag(build_logs, no_build_logs, "build-logs")?.unwrap_or(true),
            force_pep517,
            clear,
            gitignore: flag(create_gitignore, no_create_gitignore, "create-gitignore")?
                .unwrap_or(true),
            build_constraints: build_constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            build_constraints_from_workspace,
            hash_checking: HashCheckingMode::from_args(
                flag(require_hashes, no_require_hashes, "require-hashes")?,
                flag(verify_hashes, no_verify_hashes, "verify-hashes")?,
            ),
            python: python.and_then(Maybe::into_option),
            refresh: Refresh::try_from(refresh)?,
            settings: resolve_resolver_settings(resolver, build, filesystem, &environment)?,
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
        })
    }
}

/// The resolved settings to use for a `venv` invocation.
#[derive(Debug, Clone)]
pub struct VenvSettings {
    pub seed: bool,
    pub allow_existing: bool,
    pub clear: bool,
    pub force: bool,
    pub no_clear: bool,
    pub path: Option<PathBuf>,
    pub prompt: Option<String>,
    pub system_site_packages: bool,
    pub relocatable: bool,
    pub no_relocatable: bool,
    pub no_project: bool,
    pub refresh: Refresh,
    pub settings: PipSettings,
}

impl VenvSettings {
    /// Resolve the [`VenvSettings`] from the CLI and filesystem configuration.
    pub fn resolve(
        args: VenvArgs,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> anyhow::Result<Self> {
        let VenvArgs {
            python,
            system,
            no_system,
            seed,
            allow_existing,
            clear,
            force,
            no_clear,
            path,
            prompt,
            system_site_packages,
            relocatable,
            no_relocatable,
            index_args,
            registry_client:
                RegistryClientArgs {
                    index_strategy,
                    keyring_provider,
                },
            exclude_newer:
                PackageExcludeNewerArgs {
                    exclude_newer: ExcludeNewerArgs { exclude_newer },
                    exclude_newer_package,
                },
            no_project,
            link_mode,
            refresh,
            compat_args: _,
        } = args;

        // Resolve flags from CLI and environment variables.
        let seed = seed || environment.venv_seed.value == Some(true);
        let (clear, no_clear) = resolve_flag_pair(
            clear,
            no_clear,
            "clear",
            "no-clear",
            Some(environment.venv_clear),
            None,
        );
        let (relocatable, no_relocatable) = resolve_flag_pair(
            relocatable,
            no_relocatable,
            "relocatable",
            "no-relocatable",
            Some(environment.venv_relocatable),
            None,
        );

        Ok(Self {
            seed,
            allow_existing,
            clear: clear.into(),
            force,
            no_clear: no_clear.into(),
            path,
            prompt,
            system_site_packages,
            no_project,
            relocatable: relocatable.into(),
            no_relocatable: no_relocatable.into(),
            refresh: Refresh::try_from(refresh)?,
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system, "system")?,
                    index_strategy,
                    keyring_provider,
                    exclude_newer,
                    exclude_newer_package: exclude_newer_package
                        .map(ExcludeNewerPackage::from_iter),
                    link_mode,
                    ..index_args.into_pip_options(configured_indexes(filesystem.as_ref()))?
                },
                filesystem,
                environment,
            ),
        })
    }
}

/// Return the indexes from the effective filesystem configuration.
fn configured_indexes(filesystem: Option<&FilesystemOptions>) -> &[Index] {
    filesystem
        .and_then(|options| options.top_level.index.as_deref())
        .unwrap_or_default()
}

/// Resolve the [`ResolverSettings`] from the CLI, environment, and filesystem configuration.
fn resolve_resolver_settings(
    args: ResolverArgs,
    build: BuildOptionsArgs,
    filesystem: Option<FilesystemOptions>,
    environment: &EnvironmentOptions,
) -> Result<ResolverSettings> {
    let args = resolver_options(args, build, configured_indexes(filesystem.as_ref()))?;

    Ok(combine_resolver_settings(args, filesystem, environment))
}

/// Resolve the [`ResolverSettings`] from the CLI and filesystem configuration.
fn combine_resolver_settings(
    mut args: ResolverOptions,
    filesystem: Option<FilesystemOptions>,
    environment: &EnvironmentOptions,
) -> ResolverSettings {
    args.no_binary_package = args
        .no_binary_package
        .or(environment.no_binary_package.clone());
    args.no_build_package = args
        .no_build_package
        .or(environment.no_build_package.clone());
    args.no_sources_package = args
        .no_sources_package
        .or(environment.no_sources_package.clone());

    // The problem is that for `upgrade`... we want to combine the two `Upgrade` structs,
    // not the individual fields.
    let options = args.combine(ResolverOptions::from(
        filesystem
            .map(FilesystemOptions::into_options)
            .map(|options| options.top_level)
            .unwrap_or_default(),
    ));

    ResolverSettings {
        cuda_driver_version: environment.cuda_driver_version.clone(),
        amd_gpu_architecture: environment.amd_gpu_architecture,
        ..ResolverSettings::from(options)
    }
}

/// Resolve the [`ResolverInstallerSettings`] from CLI, environment, and filesystem options.
fn resolve_resolver_installer_settings(
    args: ResolverInstallerArgs,
    build: BuildOptionsArgs,
    filesystem: Option<FilesystemOptions>,
    environment: &EnvironmentOptions,
) -> Result<ResolverInstallerSettings> {
    let args = resolver_installer_options(args, build, configured_indexes(filesystem.as_ref()))?;

    Ok(combine_resolver_installer_settings(
        args,
        filesystem,
        environment,
    ))
}

/// Reconcile the [`ResolverInstallerSettings`] from the CLI and filesystem configuration.
fn combine_resolver_installer_settings(
    args: ResolverInstallerOptions,
    filesystem: Option<FilesystemOptions>,
    environment: &EnvironmentOptions,
) -> ResolverInstallerSettings {
    let options = resolver_installer_options_with_environment(args, environment).combine(
        ResolverInstallerOptions::from(
            filesystem
                .map(FilesystemOptions::into_options)
                .map(|options| options.top_level)
                .unwrap_or_default(),
        ),
    );

    let base = ResolverInstallerSettings::from(options);
    ResolverInstallerSettings {
        resolver: ResolverSettings {
            cuda_driver_version: environment.cuda_driver_version.clone(),
            amd_gpu_architecture: environment.amd_gpu_architecture,
            ..base.resolver
        },
        ..base
    }
}

fn resolver_installer_options_with_environment(
    mut options: ResolverInstallerOptions,
    environment: &EnvironmentOptions,
) -> ResolverInstallerOptions {
    options.no_binary_package = options
        .no_binary_package
        .or(environment.no_binary_package.clone());
    options.no_build_package = options
        .no_build_package
        .or(environment.no_build_package.clone());
    options.no_sources_package = options
        .no_sources_package
        .or(environment.no_sources_package.clone());
    options
}

/// The resolved settings to use for an invocation of the `pip` CLI.
///
/// Represents the shared settings that are used across all `pip` commands. Analogous to the
/// settings contained in the `[tool.uv.pip]` table.
#[derive(Debug, Clone)]
pub struct PipSettings {
    pub index_locations: IndexLocations,
    pub python: Option<String>,
    pub install_mirrors: PythonInstallMirrors,
    pub system: bool,
    pub extras: ExtrasSpecification,
    pub groups: Vec<PipGroupName>,
    pub break_system_packages: bool,
    pub target: Option<Target>,
    pub prefix: Option<Prefix>,
    pub index_strategy: IndexStrategy,
    pub keyring_provider: KeyringProviderType,
    pub torch_backend: Option<TorchMode>,
    pub cuda_driver_version: Option<Version>,
    pub amd_gpu_architecture: Option<AmdGpuArchitecture>,
    pub build_isolation: BuildIsolation,
    pub extra_build_dependencies: ExtraBuildDependencies,
    pub extra_build_variables: ExtraBuildVariables,
    pub build_options: BuildOptions,
    pub allow_empty_requirements: bool,
    pub strict: bool,
    pub dependency_mode: DependencyMode,
    pub resolution: ResolutionMode,
    pub prerelease: Prerelease,
    pub fork_strategy: ForkStrategy,
    pub dependency_metadata: DependencyMetadata,
    pub output_file: Option<PathBuf>,
    pub no_strip_extras: bool,
    pub no_strip_markers: bool,
    pub no_annotate: bool,
    pub no_header: bool,
    pub custom_compile_command: Option<String>,
    pub generate_hashes: bool,
    pub config_setting: ConfigSettings,
    pub config_settings_package: PackageConfigSettings,
    pub python_version: Option<PythonVersion>,
    pub python_platform: Option<TargetTriple>,
    pub universal: bool,
    pub exclude_newer: ExcludeNewer,
    pub no_emit_package: Vec<PackageName>,
    pub emit_index_url: bool,
    pub emit_find_links: bool,
    pub emit_build_options: bool,
    pub emit_marker_expression: bool,
    pub emit_index_annotation: bool,
    pub annotation_style: AnnotationStyle,
    pub link_mode: LinkMode,
    pub compile_bytecode: bool,
    pub sources: NoSources,
    pub hash_checking: Option<HashCheckingMode>,
    pub upgrade: Upgrade,
    pub reinstall: Reinstall,
}

impl PipSettings {
    /// Resolve the [`PipSettings`] from the CLI and filesystem configuration.
    fn combine(
        args: PipOptions,
        filesystem: Option<FilesystemOptions>,
        environment: EnvironmentOptions,
    ) -> Self {
        let Options {
            top_level,
            pip,
            install_mirrors: filesystem_install_mirrors,
            ..
        } = filesystem
            .map(FilesystemOptions::into_options)
            .unwrap_or_default();

        let PipOptions {
            python,
            system,
            break_system_packages,
            target,
            prefix,
            index,
            index_url,
            extra_index_url,
            no_index,
            find_links,
            index_strategy,
            torch_backend,
            keyring_provider,
            no_build,
            no_binary,
            only_binary,
            no_build_isolation,
            no_build_isolation_package,
            extra_build_dependencies,
            extra_build_variables,
            strict,
            extra,
            all_extras,
            no_extra,
            group,
            no_deps,
            allow_empty_requirements,
            resolution,
            prerelease,
            prerelease_package: _,
            fork_strategy,
            dependency_metadata,
            output_file,
            no_strip_extras,
            no_strip_markers,
            no_annotate,
            no_header,
            custom_compile_command,
            generate_hashes,
            config_settings,
            config_settings_package,
            python_version,
            python_platform,
            universal,
            exclude_newer,
            no_emit_package,
            emit_index_url,
            emit_find_links,
            emit_build_options,
            emit_marker_expression,
            emit_index_annotation,
            annotation_style,
            link_mode,
            compile_bytecode,
            require_hashes,
            verify_hashes,
            no_sources,
            no_sources_package,
            upgrade,
            upgrade_package,
            reinstall,
            reinstall_package,
            exclude_newer_package,
        } = pip.unwrap_or_default();

        let ResolverInstallerSchema {
            index: top_level_index,
            index_url: top_level_index_url,
            extra_index_url: top_level_extra_index_url,
            no_index: top_level_no_index,
            find_links: top_level_find_links,
            index_strategy: top_level_index_strategy,
            keyring_provider: top_level_keyring_provider,
            resolution: top_level_resolution,
            prerelease: top_level_prerelease,
            prerelease_package: top_level_prerelease_package,
            fork_strategy: top_level_fork_strategy,
            dependency_metadata: top_level_dependency_metadata,
            config_settings: top_level_config_settings,
            config_settings_package: top_level_config_settings_package,
            no_build_isolation: top_level_no_build_isolation,
            no_build_isolation_package: top_level_no_build_isolation_package,
            extra_build_dependencies: top_level_extra_build_dependencies,
            extra_build_variables: top_level_extra_build_variables,
            exclude_newer: top_level_exclude_newer,
            link_mode: top_level_link_mode,
            compile_bytecode: top_level_compile_bytecode,
            no_sources: top_level_no_sources,
            no_sources_package: top_level_no_sources_package,
            upgrade: top_level_upgrade,
            upgrade_package: top_level_upgrade_package,
            reinstall: top_level_reinstall,
            reinstall_package: top_level_reinstall_package,
            no_build: top_level_no_build,
            no_build_package: top_level_no_build_package,
            no_binary: top_level_no_binary,
            no_binary_package: top_level_no_binary_package,
            exclude_newer_package: top_level_exclude_newer_package,
            torch_backend: top_level_torch_backend,
        } = top_level;

        // Merge the top-level options (`tool.uv`) with the pip-specific options (`tool.uv.pip`),
        // preferring the latter.
        //
        // For example, prefer `tool.uv.pip.index-url` over `tool.uv.index-url`.
        let index = index.combine(top_level_index);
        let no_index = no_index.combine(top_level_no_index);
        let index_url = index_url.combine(top_level_index_url);
        let extra_index_url = extra_index_url.combine(top_level_extra_index_url);
        let find_links = find_links.combine(top_level_find_links);
        let index_strategy = index_strategy.combine(top_level_index_strategy);
        let keyring_provider = keyring_provider.combine(top_level_keyring_provider);
        let resolution = resolution.combine(top_level_resolution);
        let prerelease = prerelease.combine(top_level_prerelease);
        let prerelease_package = args
            .prerelease_package
            .combine(top_level_prerelease_package)
            .unwrap_or_default();
        let fork_strategy = fork_strategy.combine(top_level_fork_strategy);
        let dependency_metadata = dependency_metadata.combine(top_level_dependency_metadata);
        let config_settings = config_settings.combine(top_level_config_settings);
        let config_settings_package =
            config_settings_package.combine(top_level_config_settings_package);
        let no_build_isolation = no_build_isolation.combine(top_level_no_build_isolation);
        let no_build_isolation_package =
            no_build_isolation_package.combine(top_level_no_build_isolation_package);
        let extra_build_dependencies =
            extra_build_dependencies.combine(top_level_extra_build_dependencies);
        let extra_build_variables = extra_build_variables.combine(top_level_extra_build_variables);
        let exclude_newer = args
            .exclude_newer
            .combine(exclude_newer)
            .combine(top_level_exclude_newer);
        let exclude_newer_package = args
            .exclude_newer_package
            .combine(exclude_newer_package)
            .combine(top_level_exclude_newer_package)
            .unwrap_or_default();
        let link_mode = link_mode.combine(top_level_link_mode);
        let compile_bytecode = compile_bytecode.combine(top_level_compile_bytecode);
        let no_sources = no_sources.combine(top_level_no_sources);
        let no_sources_package = no_sources_package.combine(top_level_no_sources_package);
        let upgrade = upgrade.combine(top_level_upgrade);
        let upgrade_package = upgrade_package.combine(top_level_upgrade_package);
        let reinstall = reinstall.combine(top_level_reinstall);
        let reinstall_package = reinstall_package.combine(top_level_reinstall_package);
        let torch_backend = torch_backend.combine(top_level_torch_backend);
        let args_no_sources_package = args
            .no_sources_package
            .or(environment.no_sources_package.clone());

        Self {
            index_locations: IndexLocations::new(
                args.index
                    .into_iter()
                    .flatten()
                    .chain(args.extra_index_url.into_iter().flatten().map(Index::from))
                    .chain(args.index_url.into_iter().map(Index::from))
                    .chain(index.into_iter().flatten())
                    .chain(extra_index_url.into_iter().flatten().map(Index::from))
                    .chain(index_url.into_iter().map(Index::from))
                    .collect(),
                args.find_links
                    .combine(find_links)
                    .into_iter()
                    .flatten()
                    .map(Index::from)
                    .collect(),
                args.no_index.combine(no_index).unwrap_or_default(),
            ),
            extras: ExtrasSpecification::from_args(
                args.extra.combine(extra).unwrap_or_default(),
                args.no_extra.combine(no_extra).unwrap_or_default(),
                // TODO(blueraft): support no_default_extras
                false,
                // TODO(blueraft): support only_extra
                vec![],
                args.all_extras.combine(all_extras).unwrap_or_default(),
            ),

            groups: args.group.combine(group).unwrap_or_default(),
            dependency_mode: if args.no_deps.combine(no_deps).unwrap_or_default() {
                DependencyMode::Direct
            } else {
                DependencyMode::Transitive
            },
            resolution: args.resolution.combine(resolution).unwrap_or_default(),
            prerelease: resolve_prerelease(
                args.prerelease.combine(prerelease).unwrap_or_default(),
                prerelease_package,
            ),
            fork_strategy: args
                .fork_strategy
                .combine(fork_strategy)
                .unwrap_or_default(),
            dependency_metadata: DependencyMetadata::from_entries(
                args.dependency_metadata
                    .combine(dependency_metadata)
                    .unwrap_or_default(),
            ),
            output_file: args.output_file.combine(output_file),
            no_strip_extras: args
                .no_strip_extras
                .combine(no_strip_extras)
                .unwrap_or_default(),
            no_strip_markers: args
                .no_strip_markers
                .combine(no_strip_markers)
                .unwrap_or_default(),
            no_annotate: args.no_annotate.combine(no_annotate).unwrap_or_default(),
            no_header: args.no_header.combine(no_header).unwrap_or_default(),
            custom_compile_command: args.custom_compile_command.combine(custom_compile_command),
            annotation_style: args
                .annotation_style
                .combine(annotation_style)
                .unwrap_or_default(),
            index_strategy: args
                .index_strategy
                .combine(index_strategy)
                .unwrap_or_default(),
            keyring_provider: args
                .keyring_provider
                .combine(keyring_provider)
                .unwrap_or_default(),
            generate_hashes: args
                .generate_hashes
                .combine(generate_hashes)
                .unwrap_or_default(),
            allow_empty_requirements: args
                .allow_empty_requirements
                .combine(allow_empty_requirements)
                .unwrap_or_default(),
            build_isolation: BuildIsolation::from_args(
                args.no_build_isolation,
                args.no_build_isolation_package.unwrap_or_default(),
            )
            .combine(BuildIsolation::from_args(
                no_build_isolation,
                no_build_isolation_package.unwrap_or_default(),
            ))
            .unwrap_or_default(),
            extra_build_dependencies: args
                .extra_build_dependencies
                .combine(extra_build_dependencies)
                .unwrap_or_default(),
            extra_build_variables: args
                .extra_build_variables
                .combine(extra_build_variables)
                .unwrap_or_default(),
            config_setting: args
                .config_settings
                .combine(config_settings)
                .unwrap_or_default(),
            config_settings_package: args
                .config_settings_package
                .combine(config_settings_package)
                .unwrap_or_default(),
            torch_backend: args.torch_backend.combine(torch_backend),
            cuda_driver_version: environment.cuda_driver_version.clone(),
            amd_gpu_architecture: environment.amd_gpu_architecture,
            python_version: args.python_version.combine(python_version),
            python_platform: args.python_platform.combine(python_platform),
            universal: args.universal.combine(universal).unwrap_or_default(),
            exclude_newer: ExcludeNewer::from_args(
                exclude_newer,
                exclude_newer_package.into_iter().map(Into::into).collect(),
            ),
            no_emit_package: args
                .no_emit_package
                .combine(no_emit_package)
                .unwrap_or_default(),
            emit_index_url: args
                .emit_index_url
                .combine(emit_index_url)
                .unwrap_or_default(),
            emit_find_links: args
                .emit_find_links
                .combine(emit_find_links)
                .unwrap_or_default(),
            emit_build_options: args
                .emit_build_options
                .combine(emit_build_options)
                .unwrap_or_default(),
            emit_marker_expression: args
                .emit_marker_expression
                .combine(emit_marker_expression)
                .unwrap_or_default(),
            emit_index_annotation: args
                .emit_index_annotation
                .combine(emit_index_annotation)
                .unwrap_or_default(),
            link_mode: args.link_mode.combine(link_mode).unwrap_or_default(),
            hash_checking: HashCheckingMode::from_args(
                args.require_hashes.combine(require_hashes),
                args.verify_hashes.combine(verify_hashes),
            ),
            python: args.python.combine(python),
            system: args.system.combine(system).unwrap_or_default(),
            break_system_packages: args
                .break_system_packages
                .combine(break_system_packages)
                .unwrap_or_default(),
            target: args.target.combine(target).map(Target::from),
            prefix: args.prefix.combine(prefix).map(Prefix::from),
            compile_bytecode: args
                .compile_bytecode
                .combine(compile_bytecode)
                .unwrap_or_default(),
            sources: NoSources::from_args(
                args.no_sources.combine(no_sources),
                args_no_sources_package
                    .combine(no_sources_package)
                    .unwrap_or_default(),
            ),
            strict: args.strict.combine(strict).unwrap_or_default(),
            upgrade: Upgrade::from_args(
                args.upgrade,
                args.upgrade_package
                    .into_iter()
                    .flatten()
                    .map(Requirement::from)
                    .collect(),
                Vec::new(),
            )
            .combine(Upgrade::from_args(
                upgrade,
                upgrade_package
                    .into_iter()
                    .flatten()
                    .map(Requirement::from)
                    .collect(),
                Vec::new(),
            ))
            .unwrap_or_default(),
            reinstall: Reinstall::from_args(
                args.reinstall,
                args.reinstall_package.unwrap_or_default(),
            )
            .combine(Reinstall::from_args(
                reinstall,
                reinstall_package.unwrap_or_default(),
            ))
            .unwrap_or_default(),
            build_options: BuildOptions::new(
                NoBinary::from_pip_args(args.no_binary.combine(no_binary).unwrap_or_default())
                    .combine(NoBinary::from_args(
                        top_level_no_binary,
                        top_level_no_binary_package.unwrap_or_default(),
                    )),
                NoBuild::from_pip_args(
                    args.only_binary.combine(only_binary).unwrap_or_default(),
                    args.no_build.combine(no_build).unwrap_or_default(),
                )
                .combine(NoBuild::from_args(
                    top_level_no_build,
                    top_level_no_build_package.unwrap_or_default(),
                )),
            ),
            install_mirrors: environment
                .install_mirrors
                .combine(filesystem_install_mirrors),
        }
    }
}

/// The resolved settings to use for an invocation of the `uv publish` CLI.
#[derive(Clone)]
pub struct PublishSettings {
    // CLI only, see [`PublishArgs`] for docs.
    pub files: Vec<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub index: Option<String>,
    pub dry_run: bool,
    pub no_attestations: bool,

    // Both CLI and configuration.
    pub publish_url: DisplaySafeUrl,
    pub trusted_publishing: TrustedPublishing,
    pub keyring_provider: KeyringProviderType,
    pub check_url: Option<IndexUrl>,

    // Configuration only
    pub index_locations: IndexLocations,
}

impl fmt::Debug for PublishSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PublishSettings")
            .field("files", &self.files)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "****"))
            .field("index", &self.index)
            .field("dry_run", &self.dry_run)
            .field("no_attestations", &self.no_attestations)
            .field("publish_url", &self.publish_url)
            .field("trusted_publishing", &self.trusted_publishing)
            .field("keyring_provider", &self.keyring_provider)
            .field("check_url", &self.check_url)
            .field("index_locations", &self.index_locations)
            .finish()
    }
}

impl PublishSettings {
    /// Resolve the [`PublishSettings`] from the CLI and filesystem configuration.
    pub fn resolve(args: PublishArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let Options {
            publish, top_level, ..
        } = filesystem
            .map(FilesystemOptions::into_options)
            .unwrap_or_default();

        let PublishOptions {
            publish_url,
            trusted_publishing,
            check_url,
        } = publish;
        let ResolverInstallerSchema {
            keyring_provider,
            index,
            extra_index_url,
            index_url,
            ..
        } = top_level;

        // Tokens are encoded in the same way as username/password
        let (username, password) = if let Some(token) = args.token {
            (Some("__token__".to_string()), Some(token))
        } else {
            (args.username, args.password)
        };

        Self {
            files: args.files,
            username,
            password,
            dry_run: args.dry_run,
            no_attestations: args.no_attestations,
            publish_url: args
                .publish_url
                .combine(publish_url)
                .unwrap_or_else(|| DisplaySafeUrl::parse(PYPI_PUBLISH_URL).unwrap()),
            trusted_publishing: args
                .trusted_publishing
                .combine(trusted_publishing)
                .unwrap_or_default(),
            keyring_provider: args
                .keyring_provider
                .combine(keyring_provider)
                .unwrap_or_default(),
            check_url: args.check_url.combine(check_url),
            index: args.index,
            index_locations: IndexLocations::new(
                index
                    .into_iter()
                    .flatten()
                    .chain(extra_index_url.into_iter().flatten().map(Index::from))
                    .chain(index_url.into_iter().map(Index::from))
                    .collect(),
                Vec::new(),
                false,
            ),
        }
    }
}

/// The resolved settings to use for an invocation of the `uv auth logout` CLI.
#[derive(Debug, Clone)]
pub struct AuthLogoutSettings {
    pub service: Service,
    pub username: Option<String>,
}

impl AuthLogoutSettings {
    /// Resolve the [`AuthLogoutSettings`] from the CLI and filesystem configuration.
    pub fn resolve(args: AuthLogoutArgs) -> Self {
        Self {
            service: args.service,
            username: args.username,
        }
    }
}

/// The resolved settings to use for an invocation of the `uv auth token` CLI.
#[derive(Debug, Clone)]
pub struct AuthTokenSettings {
    pub service: Service,
    pub username: Option<String>,
}

impl AuthTokenSettings {
    /// Resolve the [`AuthTokenSettings`] from the CLI and filesystem configuration.
    pub fn resolve(args: AuthTokenArgs) -> Self {
        Self {
            service: args.service,
            username: args.username,
        }
    }
}

/// The resolved settings to use for an invocation of the `uv auth set` CLI.
#[derive(Debug, Clone)]
pub struct AuthLoginSettings {
    pub service: Service,
    pub username: Option<String>,
    pub password: Option<String>,
    pub token: Option<String>,
}

impl AuthLoginSettings {
    /// Resolve the [`AuthLoginSettings`] from the CLI and filesystem configuration.
    pub fn resolve(args: AuthLoginArgs) -> Self {
        Self {
            service: args.service,
            username: args.username,
            password: args.password,
            token: args.token,
        }
    }
}

// Environment variables that are not exposed as CLI arguments.
mod env {
    use uv_static::EnvVars;
    pub(super) const UV_PYTHON_DOWNLOADS: (&str, &str) = (
        EnvVars::UV_PYTHON_DOWNLOADS,
        "one of 'auto', 'true', 'manual', 'never', or 'false'",
    );
}

/// Attempt to load and parse an environment variable with the given name.
///
/// Exits the program and prints an error message containing the expected type if
/// parsing values.
fn env<T>((name, expected): (&str, &str)) -> Option<T>
where
    T: FromStr,
{
    let val = match std::env::var(name) {
        Ok(val) => val,
        Err(VarError::NotPresent) => return None,
        Err(VarError::NotUnicode(_)) => parse_failure(name, expected),
    };
    Some(
        val.parse()
            .unwrap_or_else(|_| parse_failure(name, expected)),
    )
}

/// Prints a parse error and exits the process.
#[expect(clippy::exit, clippy::print_stderr)]
fn parse_failure(name: &str, expected: &str) -> ! {
    eprintln!("error: invalid value for {name}, expected {expected}");
    process::exit(1)
}

#[cfg(test)]
mod tests {
    use crate::{IndexArgs, RegistryClientArgs};

    use super::*;

    #[test]
    fn upgrade_settings_target_only_requested_package() -> anyhow::Result<()> {
        let package = PackageName::from_str("anyio")?;
        let settings = UpgradeSettings::resolve(
            UpgradeArgs {
                packages: vec![package.clone()],
                exclude: Vec::new(),
                index_args: IndexArgs {
                    index: None,
                    default_index: None,
                    index_url: None,
                    extra_index_url: None,
                    find_links: None,
                    no_index: false,
                },
                registry_client: RegistryClientArgs {
                    index_strategy: None,
                    keyring_provider: None,
                },
            },
            None,
            EnvironmentOptions::new()?,
        )?;
        let expected = FxHashSet::from_iter([package]);

        assert!(!settings.settings.upgrade.is_all());
        assert_eq!(settings.settings.upgrade.packages(), Some(&expected));
        Ok(())
    }
}
