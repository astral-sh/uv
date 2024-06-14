use std::env::VarError;
use std::ffi::OsString;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process;
use std::str::FromStr;

use distribution_types::IndexLocations;
use install_wheel_rs::linker::LinkMode;
use pep508_rs::RequirementOrigin;
use pypi_types::Requirement;
use uv_cache::{CacheArgs, Refresh};
use uv_client::Connectivity;
use uv_configuration::{
    Concurrency, ConfigSettings, ExtrasSpecification, IndexStrategy, KeyringProviderType, NoBinary,
    NoBuild, PreviewMode, Reinstall, SetupPyStrategy, TargetTriple, Upgrade,
};
use uv_normalize::PackageName;
use uv_resolver::{AnnotationStyle, DependencyMode, ExcludeNewer, PreReleaseMode, ResolutionMode};
use uv_toolchain::{Prefix, PythonVersion, Target};
use uv_workspace::{
    Combine, InstallerOptions, PipOptions, ResolverInstallerOptions, ResolverOptions, Workspace,
};

use crate::cli::{
    AddArgs, ColorChoice, GlobalArgs, IndexArgs, InstallerArgs, LockArgs, Maybe, PipCheckArgs,
    PipCompileArgs, PipFreezeArgs, PipInstallArgs, PipListArgs, PipShowArgs, PipSyncArgs,
    PipUninstallArgs, RemoveArgs, ResolverArgs, ResolverInstallerArgs, RunArgs, SyncArgs,
    ToolRunArgs, ToolchainInstallArgs, ToolchainListArgs, VenvArgs,
};
use crate::commands::ListFormat;

/// The resolved global settings to use for any invocation of the CLI.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct GlobalSettings {
    pub(crate) quiet: bool,
    pub(crate) verbose: u8,
    pub(crate) color: ColorChoice,
    pub(crate) native_tls: bool,
    pub(crate) connectivity: Connectivity,
    pub(crate) isolated: bool,
    pub(crate) preview: PreviewMode,
}

impl GlobalSettings {
    /// Resolve the [`GlobalSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: GlobalArgs, workspace: Option<&Workspace>) -> Self {
        Self {
            quiet: args.quiet,
            verbose: args.verbose,
            color: if args.no_color
                || std::env::var_os("NO_COLOR")
                    .filter(|v| !v.is_empty())
                    .is_some()
            {
                ColorChoice::Never
            } else if std::env::var_os("FORCE_COLOR")
                .filter(|v| !v.is_empty())
                .is_some()
                || std::env::var_os("CLICOLOR_FORCE")
                    .filter(|v| !v.is_empty())
                    .is_some()
            {
                ColorChoice::Always
            } else {
                args.color
            },
            native_tls: flag(args.native_tls, args.no_native_tls)
                .combine(workspace.and_then(|workspace| workspace.options.globals.native_tls))
                .unwrap_or(false),
            connectivity: if flag(args.offline, args.no_offline)
                .combine(workspace.and_then(|workspace| workspace.options.globals.offline))
                .unwrap_or(false)
            {
                Connectivity::Offline
            } else {
                Connectivity::Online
            },
            isolated: args.isolated,
            preview: PreviewMode::from(
                flag(args.preview, args.no_preview)
                    .combine(workspace.and_then(|workspace| workspace.options.globals.preview))
                    .unwrap_or(false),
            ),
        }
    }
}

/// The resolved cache settings to use for any invocation of the CLI.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct CacheSettings {
    pub(crate) no_cache: bool,
    pub(crate) cache_dir: Option<PathBuf>,
}

impl CacheSettings {
    /// Resolve the [`CacheSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: CacheArgs, workspace: Option<&Workspace>) -> Self {
        Self {
            no_cache: args.no_cache
                || workspace
                    .and_then(|workspace| workspace.options.globals.no_cache)
                    .unwrap_or(false),
            cache_dir: args.cache_dir.or_else(|| {
                workspace.and_then(|workspace| workspace.options.globals.cache_dir.clone())
            }),
        }
    }
}

/// The resolved settings to use for a `run` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct RunSettings {
    pub(crate) extras: ExtrasSpecification,
    pub(crate) dev: bool,
    pub(crate) target: Option<String>,
    pub(crate) args: Vec<OsString>,
    pub(crate) with: Vec<String>,
    pub(crate) python: Option<String>,
    pub(crate) refresh: Refresh,
    pub(crate) upgrade: Upgrade,
    pub(crate) package: Option<PackageName>,
    pub(crate) settings: ResolverInstallerSettings,
}

impl RunSettings {
    /// Resolve the [`RunSettings`] from the CLI and workspace configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: RunArgs, workspace: Option<Workspace>) -> Self {
        let RunArgs {
            extra,
            all_extras,
            no_all_extras,
            dev,
            no_dev,
            target,
            args,
            with,
            refresh,
            no_refresh,
            refresh_package,
            upgrade,
            no_upgrade,
            upgrade_package,
            installer,
            python,
            package,
        } = args;

        Self {
            refresh: Refresh::from_args(flag(refresh, no_refresh), refresh_package),
            upgrade: Upgrade::from_args(flag(upgrade, no_upgrade), upgrade_package),
            extras: ExtrasSpecification::from_args(
                flag(all_extras, no_all_extras).unwrap_or_default(),
                extra.unwrap_or_default(),
            ),
            dev: flag(dev, no_dev).unwrap_or(true),
            target,
            args,
            with,
            python,
            package,
            settings: ResolverInstallerSettings::combine(
                ResolverInstallerOptions::from(installer),
                workspace,
            ),
        }
    }
}

/// The resolved settings to use for a `tool run` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct ToolRunSettings {
    pub(crate) target: String,
    pub(crate) args: Vec<OsString>,
    pub(crate) from: Option<String>,
    pub(crate) with: Vec<String>,
    pub(crate) python: Option<String>,
    pub(crate) settings: ResolverInstallerSettings,
}

impl ToolRunSettings {
    /// Resolve the [`ToolRunSettings`] from the CLI and workspace configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: ToolRunArgs, workspace: Option<Workspace>) -> Self {
        let ToolRunArgs {
            target,
            args,
            from,
            with,
            installer,
            python,
        } = args;

        Self {
            target,
            args,
            from,
            with,
            python,
            settings: ResolverInstallerSettings::combine(
                ResolverInstallerOptions::from(installer),
                workspace,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) enum ToolchainListKinds {
    #[default]
    Default,
    Installed,
}

/// The resolved settings to use for a `tool run` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct ToolchainListSettings {
    pub(crate) kinds: ToolchainListKinds,
    pub(crate) all_platforms: bool,
    pub(crate) all_versions: bool,
}

impl ToolchainListSettings {
    /// Resolve the [`ToolchainListSettings`] from the CLI and workspace configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: ToolchainListArgs, _workspace: Option<Workspace>) -> Self {
        let ToolchainListArgs {
            all_versions,
            all_platforms,
            only_installed,
        } = args;

        let kinds = if only_installed {
            ToolchainListKinds::Installed
        } else {
            ToolchainListKinds::default()
        };

        Self {
            kinds,
            all_platforms,
            all_versions,
        }
    }
}

/// The resolved settings to use for a `toolchain install` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct ToolchainInstallSettings {
    pub(crate) target: Option<String>,
    pub(crate) force: bool,
}

impl ToolchainInstallSettings {
    /// Resolve the [`ToolchainInstallSettings`] from the CLI and workspace configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: ToolchainInstallArgs, _workspace: Option<Workspace>) -> Self {
        let ToolchainInstallArgs { target, force } = args;

        Self { target, force }
    }
}

/// The resolved settings to use for a `sync` invocation.
#[allow(clippy::struct_excessive_bools, dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct SyncSettings {
    pub(crate) refresh: Refresh,
    pub(crate) extras: ExtrasSpecification,
    pub(crate) dev: bool,
    pub(crate) python: Option<String>,
    pub(crate) settings: InstallerSettings,
}

impl SyncSettings {
    /// Resolve the [`SyncSettings`] from the CLI and workspace configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: SyncArgs, workspace: Option<Workspace>) -> Self {
        let SyncArgs {
            extra,
            all_extras,
            no_all_extras,
            dev,
            no_dev,
            refresh,
            no_refresh,
            refresh_package,
            installer,
            python,
        } = args;

        Self {
            refresh: Refresh::from_args(flag(refresh, no_refresh), refresh_package),
            extras: ExtrasSpecification::from_args(
                flag(all_extras, no_all_extras).unwrap_or_default(),
                extra.unwrap_or_default(),
            ),
            dev: flag(dev, no_dev).unwrap_or(true),
            python,
            settings: InstallerSettings::combine(InstallerOptions::from(installer), workspace),
        }
    }
}

/// The resolved settings to use for a `lock` invocation.
#[allow(clippy::struct_excessive_bools, dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct LockSettings {
    pub(crate) refresh: Refresh,
    pub(crate) upgrade: Upgrade,
    pub(crate) python: Option<String>,
    pub(crate) settings: ResolverSettings,
}

impl LockSettings {
    /// Resolve the [`LockSettings`] from the CLI and workspace configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: LockArgs, workspace: Option<Workspace>) -> Self {
        let LockArgs {
            refresh,
            no_refresh,
            refresh_package,
            upgrade,
            no_upgrade,
            upgrade_package,
            resolver,
            python,
        } = args;

        Self {
            refresh: Refresh::from_args(flag(refresh, no_refresh), refresh_package),
            upgrade: Upgrade::from_args(flag(upgrade, no_upgrade), upgrade_package),
            python,
            settings: ResolverSettings::combine(ResolverOptions::from(resolver), workspace),
        }
    }
}

/// The resolved settings to use for a `add` invocation.
#[allow(clippy::struct_excessive_bools, dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct AddSettings {
    pub(crate) requirements: Vec<String>,
    pub(crate) python: Option<String>,
}

impl AddSettings {
    /// Resolve the [`AddSettings`] from the CLI and workspace configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: AddArgs, _workspace: Option<Workspace>) -> Self {
        let AddArgs {
            requirements,
            python,
        } = args;

        Self {
            requirements,
            python,
        }
    }
}

/// The resolved settings to use for a `remove` invocation.
#[allow(clippy::struct_excessive_bools, dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct RemoveSettings {
    pub(crate) requirements: Vec<PackageName>,
    pub(crate) python: Option<String>,
}

impl RemoveSettings {
    /// Resolve the [`RemoveSettings`] from the CLI and workspace configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: RemoveArgs, _workspace: Option<Workspace>) -> Self {
        let RemoveArgs {
            requirements,
            python,
        } = args;

        Self {
            requirements,
            python,
        }
    }
}

/// The resolved settings to use for a `pip compile` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PipCompileSettings {
    pub(crate) src_file: Vec<PathBuf>,
    pub(crate) constraint: Vec<PathBuf>,
    pub(crate) r#override: Vec<PathBuf>,
    pub(crate) refresh: Refresh,
    pub(crate) upgrade: Upgrade,
    pub(crate) overrides_from_workspace: Vec<Requirement>,
    pub(crate) settings: PipSettings,
}

impl PipCompileSettings {
    /// Resolve the [`PipCompileSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: PipCompileArgs, workspace: Option<Workspace>) -> Self {
        let PipCompileArgs {
            src_file,
            constraint,
            r#override,
            extra,
            all_extras,
            no_all_extras,
            no_deps,
            deps,
            output_file,
            no_strip_extras,
            strip_extras,
            no_annotate,
            annotate,
            no_header,
            header,
            annotation_style,
            custom_compile_command,
            refresh,
            no_refresh,
            refresh_package,
            resolver,
            python,
            system,
            no_system,
            upgrade,
            no_upgrade,
            upgrade_package,
            generate_hashes,
            no_generate_hashes,
            legacy_setup_py,
            no_legacy_setup_py,
            no_build_isolation,
            build_isolation,
            no_build,
            build,
            no_binary,
            only_binary,
            python_version,
            python_platform,
            no_emit_package,
            emit_index_url,
            no_emit_index_url,
            emit_find_links,
            no_emit_find_links,
            emit_marker_expression,
            no_emit_marker_expression,
            emit_index_annotation,
            no_emit_index_annotation,
            compat_args: _,
        } = args;

        let overrides_from_workspace = if let Some(workspace) = &workspace {
            workspace
                .options
                .override_dependencies
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

        Self {
            src_file,
            constraint: constraint
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            r#override,
            refresh: Refresh::from_args(flag(refresh, no_refresh), refresh_package),
            upgrade: Upgrade::from_args(flag(upgrade, no_upgrade), upgrade_package),
            overrides_from_workspace,
            settings: PipSettings::combine(
                PipOptions {
                    python,
                    system: flag(system, no_system),
                    no_build: flag(no_build, build),
                    no_binary,
                    only_binary,
                    no_build_isolation: flag(no_build_isolation, build_isolation),
                    extra,
                    all_extras: flag(all_extras, no_all_extras),
                    no_deps: flag(no_deps, deps),
                    output_file,
                    no_strip_extras: flag(no_strip_extras, strip_extras),
                    no_annotate: flag(no_annotate, annotate),
                    no_header: flag(no_header, header),
                    custom_compile_command,
                    generate_hashes: flag(generate_hashes, no_generate_hashes),
                    legacy_setup_py: flag(legacy_setup_py, no_legacy_setup_py),
                    python_version,
                    python_platform,
                    no_emit_package,
                    emit_index_url: flag(emit_index_url, no_emit_index_url),
                    emit_find_links: flag(emit_find_links, no_emit_find_links),
                    emit_marker_expression: flag(emit_marker_expression, no_emit_marker_expression),
                    emit_index_annotation: flag(emit_index_annotation, no_emit_index_annotation),
                    annotation_style,
                    concurrent_builds: env(env::CONCURRENT_BUILDS),
                    concurrent_downloads: env(env::CONCURRENT_DOWNLOADS),
                    concurrent_installs: env(env::CONCURRENT_INSTALLS),
                    ..PipOptions::from(resolver)
                },
                workspace,
            ),
        }
    }
}

/// The resolved settings to use for a `pip sync` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PipSyncSettings {
    pub(crate) src_file: Vec<PathBuf>,
    pub(crate) constraint: Vec<PathBuf>,
    pub(crate) reinstall: Reinstall,
    pub(crate) refresh: Refresh,
    pub(crate) dry_run: bool,
    pub(crate) settings: PipSettings,
}

impl PipSyncSettings {
    /// Resolve the [`PipSyncSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: PipSyncArgs, workspace: Option<Workspace>) -> Self {
        let PipSyncArgs {
            src_file,
            constraint,
            installer,
            exclude_newer,
            reinstall,
            no_reinstall,
            reinstall_package,
            refresh,
            no_refresh,
            refresh_package,
            require_hashes,
            no_require_hashes,
            python,
            system,
            no_system,
            break_system_packages,
            no_break_system_packages,
            target,
            prefix,
            legacy_setup_py,
            no_legacy_setup_py,
            no_build_isolation,
            build_isolation,
            no_build,
            build,
            no_binary,
            only_binary,
            python_version,
            python_platform,
            strict,
            no_strict,
            dry_run,
            compat_args: _,
        } = args;

        Self {
            src_file,
            constraint: constraint
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            reinstall: Reinstall::from_args(flag(reinstall, no_reinstall), reinstall_package),
            refresh: Refresh::from_args(flag(refresh, no_refresh), refresh_package),
            dry_run,
            settings: PipSettings::combine(
                PipOptions {
                    python,
                    system: flag(system, no_system),
                    break_system_packages: flag(break_system_packages, no_break_system_packages),
                    exclude_newer,
                    target,
                    prefix,
                    no_build: flag(no_build, build),
                    no_binary,
                    only_binary,
                    no_build_isolation: flag(no_build_isolation, build_isolation),
                    strict: flag(strict, no_strict),
                    legacy_setup_py: flag(legacy_setup_py, no_legacy_setup_py),
                    python_version,
                    python_platform,
                    require_hashes: flag(require_hashes, no_require_hashes),
                    concurrent_builds: env(env::CONCURRENT_BUILDS),
                    concurrent_downloads: env(env::CONCURRENT_DOWNLOADS),
                    concurrent_installs: env(env::CONCURRENT_INSTALLS),
                    ..PipOptions::from(installer)
                },
                workspace,
            ),
        }
    }
}

/// The resolved settings to use for a `pip install` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PipInstallSettings {
    pub(crate) package: Vec<String>,
    pub(crate) requirement: Vec<PathBuf>,
    pub(crate) editable: Vec<String>,
    pub(crate) constraint: Vec<PathBuf>,
    pub(crate) r#override: Vec<PathBuf>,
    pub(crate) upgrade: Upgrade,
    pub(crate) reinstall: Reinstall,
    pub(crate) refresh: Refresh,
    pub(crate) dry_run: bool,
    pub(crate) overrides_from_workspace: Vec<Requirement>,
    pub(crate) settings: PipSettings,
}

impl PipInstallSettings {
    /// Resolve the [`PipInstallSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: PipInstallArgs, workspace: Option<Workspace>) -> Self {
        let PipInstallArgs {
            package,
            requirement,
            editable,
            constraint,
            r#override,
            extra,
            all_extras,
            no_all_extras,
            upgrade,
            no_upgrade,
            upgrade_package,
            reinstall,
            no_reinstall,
            reinstall_package,
            refresh,
            no_refresh,
            refresh_package,
            no_deps,
            deps,
            require_hashes,
            no_require_hashes,
            installer,
            python,
            system,
            no_system,
            break_system_packages,
            no_break_system_packages,
            target,
            prefix,
            legacy_setup_py,
            no_legacy_setup_py,
            no_build_isolation,
            build_isolation,
            no_build,
            build,
            no_binary,
            only_binary,
            python_version,
            python_platform,
            strict,
            no_strict,
            dry_run,
            compat_args: _,
        } = args;

        let overrides_from_workspace = if let Some(workspace) = &workspace {
            workspace
                .options
                .override_dependencies
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

        Self {
            package,
            requirement,
            editable,
            constraint: constraint
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            r#override,
            upgrade: Upgrade::from_args(flag(upgrade, no_upgrade), upgrade_package),
            reinstall: Reinstall::from_args(flag(reinstall, no_reinstall), reinstall_package),
            refresh: Refresh::from_args(flag(refresh, no_refresh), refresh_package),
            dry_run,
            overrides_from_workspace,
            settings: PipSettings::combine(
                PipOptions {
                    python,
                    system: flag(system, no_system),
                    break_system_packages: flag(break_system_packages, no_break_system_packages),
                    target,
                    prefix,
                    no_build: flag(no_build, build),
                    no_binary,
                    only_binary,
                    no_build_isolation: flag(no_build_isolation, build_isolation),
                    strict: flag(strict, no_strict),
                    extra,
                    all_extras: flag(all_extras, no_all_extras),
                    no_deps: flag(no_deps, deps),
                    legacy_setup_py: flag(legacy_setup_py, no_legacy_setup_py),
                    python_version,
                    python_platform,
                    require_hashes: flag(require_hashes, no_require_hashes),
                    concurrent_builds: env(env::CONCURRENT_BUILDS),
                    concurrent_downloads: env(env::CONCURRENT_DOWNLOADS),
                    concurrent_installs: env(env::CONCURRENT_INSTALLS),
                    ..PipOptions::from(installer)
                },
                workspace,
            ),
        }
    }
}

/// The resolved settings to use for a `pip uninstall` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PipUninstallSettings {
    pub(crate) package: Vec<String>,
    pub(crate) requirement: Vec<PathBuf>,
    pub(crate) settings: PipSettings,
}

impl PipUninstallSettings {
    /// Resolve the [`PipUninstallSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: PipUninstallArgs, workspace: Option<Workspace>) -> Self {
        let PipUninstallArgs {
            package,
            requirement,
            python,
            keyring_provider,
            system,
            no_system,
            break_system_packages,
            no_break_system_packages,
            target,
            prefix,
        } = args;

        Self {
            package,
            requirement,
            settings: PipSettings::combine(
                PipOptions {
                    python,
                    system: flag(system, no_system),
                    break_system_packages: flag(break_system_packages, no_break_system_packages),
                    target,
                    prefix,
                    keyring_provider,
                    ..PipOptions::default()
                },
                workspace,
            ),
        }
    }
}

/// The resolved settings to use for a `pip freeze` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PipFreezeSettings {
    pub(crate) exclude_editable: bool,
    pub(crate) settings: PipSettings,
}

impl PipFreezeSettings {
    /// Resolve the [`PipFreezeSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: PipFreezeArgs, workspace: Option<Workspace>) -> Self {
        let PipFreezeArgs {
            exclude_editable,
            strict,
            no_strict,
            python,
            system,
            no_system,
        } = args;

        Self {
            exclude_editable,
            settings: PipSettings::combine(
                PipOptions {
                    python,
                    system: flag(system, no_system),
                    strict: flag(strict, no_strict),
                    ..PipOptions::default()
                },
                workspace,
            ),
        }
    }
}

/// The resolved settings to use for a `pip list` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PipListSettings {
    pub(crate) editable: bool,
    pub(crate) exclude_editable: bool,
    pub(crate) exclude: Vec<PackageName>,
    pub(crate) format: ListFormat,
    pub(crate) settings: PipSettings,
}

impl PipListSettings {
    /// Resolve the [`PipListSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: PipListArgs, workspace: Option<Workspace>) -> Self {
        let PipListArgs {
            editable,
            exclude_editable,
            exclude,
            format,
            strict,
            no_strict,
            python,
            system,
            no_system,
            compat_args: _,
        } = args;

        Self {
            editable,
            exclude_editable,
            exclude,
            format,
            settings: PipSettings::combine(
                PipOptions {
                    python,
                    system: flag(system, no_system),
                    strict: flag(strict, no_strict),
                    ..PipOptions::default()
                },
                workspace,
            ),
        }
    }
}

/// The resolved settings to use for a `pip show` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PipShowSettings {
    pub(crate) package: Vec<PackageName>,
    pub(crate) settings: PipSettings,
}

impl PipShowSettings {
    /// Resolve the [`PipShowSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: PipShowArgs, workspace: Option<Workspace>) -> Self {
        let PipShowArgs {
            package,
            strict,
            no_strict,
            python,
            system,
            no_system,
        } = args;

        Self {
            package,
            settings: PipSettings::combine(
                PipOptions {
                    python,
                    system: flag(system, no_system),
                    strict: flag(strict, no_strict),
                    ..PipOptions::default()
                },
                workspace,
            ),
        }
    }
}

/// The resolved settings to use for a `pip check` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PipCheckSettings {
    pub(crate) settings: PipSettings,
}

impl PipCheckSettings {
    /// Resolve the [`PipCheckSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: PipCheckArgs, workspace: Option<Workspace>) -> Self {
        let PipCheckArgs {
            python,
            system,
            no_system,
        } = args;

        Self {
            settings: PipSettings::combine(
                PipOptions {
                    python,
                    system: flag(system, no_system),
                    ..PipOptions::default()
                },
                workspace,
            ),
        }
    }
}

/// The resolved settings to use for a `pip check` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct VenvSettings {
    pub(crate) seed: bool,
    pub(crate) allow_existing: bool,
    pub(crate) name: PathBuf,
    pub(crate) prompt: Option<String>,
    pub(crate) system_site_packages: bool,
    pub(crate) settings: PipSettings,
}

impl VenvSettings {
    /// Resolve the [`VenvSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: VenvArgs, workspace: Option<Workspace>) -> Self {
        let VenvArgs {
            python,
            system,
            no_system,
            seed,
            allow_existing,
            name,
            prompt,
            system_site_packages,
            index_args,
            index_strategy,
            keyring_provider,
            exclude_newer,
            link_mode,
            compat_args: _,
        } = args;

        Self {
            seed,
            allow_existing,
            name,
            prompt,
            system_site_packages,
            settings: PipSettings::combine(
                PipOptions {
                    python,
                    system: flag(system, no_system),
                    index_strategy,
                    keyring_provider,
                    exclude_newer,
                    link_mode,
                    ..PipOptions::from(index_args)
                },
                workspace,
            ),
        }
    }
}

/// The resolved settings to use for an invocation of the `uv` CLI when installing dependencies.
///
/// Combines the `[tool.uv]` persistent configuration with the command-line arguments
/// ([`InstallerArgs`], represented as [`InstallerOptions`]).
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
pub(crate) struct InstallerSettings {
    pub(crate) index_locations: IndexLocations,
    pub(crate) index_strategy: IndexStrategy,
    pub(crate) keyring_provider: KeyringProviderType,
    pub(crate) config_setting: ConfigSettings,
    pub(crate) link_mode: LinkMode,
    pub(crate) compile_bytecode: bool,
}

impl InstallerSettings {
    /// Resolve the [`InstallerSettings`] from the CLI and workspace configuration.
    pub(crate) fn combine(args: InstallerOptions, workspace: Option<Workspace>) -> Self {
        let ResolverInstallerOptions {
            index_url,
            extra_index_url,
            no_index,
            find_links,
            index_strategy,
            keyring_provider,
            resolution: _,
            prerelease: _,
            config_settings,
            exclude_newer: _,
            link_mode,
            compile_bytecode,
        } = workspace
            .map(|workspace| workspace.options.top_level)
            .unwrap_or_default();

        Self {
            index_locations: IndexLocations::new(
                args.index_url.combine(index_url),
                args.extra_index_url
                    .combine(extra_index_url)
                    .unwrap_or_default(),
                args.find_links.combine(find_links).unwrap_or_default(),
                args.no_index.combine(no_index).unwrap_or_default(),
            ),
            index_strategy: args
                .index_strategy
                .combine(index_strategy)
                .unwrap_or_default(),
            keyring_provider: args
                .keyring_provider
                .combine(keyring_provider)
                .unwrap_or_default(),
            config_setting: args
                .config_settings
                .combine(config_settings)
                .unwrap_or_default(),
            link_mode: args.link_mode.combine(link_mode).unwrap_or_default(),
            compile_bytecode: args
                .compile_bytecode
                .combine(compile_bytecode)
                .unwrap_or_default(),
        }
    }
}

/// The resolved settings to use for an invocation of the `uv` CLI when resolving dependencies.
///
/// Combines the `[tool.uv]` persistent configuration with the command-line arguments
/// ([`ResolverArgs`], represented as [`ResolverOptions`]).
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
pub(crate) struct ResolverSettings {
    pub(crate) index_locations: IndexLocations,
    pub(crate) index_strategy: IndexStrategy,
    pub(crate) keyring_provider: KeyringProviderType,
    pub(crate) resolution: ResolutionMode,
    pub(crate) prerelease: PreReleaseMode,
    pub(crate) config_setting: ConfigSettings,
    pub(crate) exclude_newer: Option<ExcludeNewer>,
    pub(crate) link_mode: LinkMode,
}

impl ResolverSettings {
    /// Resolve the [`ResolverSettings`] from the CLI and workspace configuration.
    pub(crate) fn combine(args: ResolverOptions, workspace: Option<Workspace>) -> Self {
        let ResolverInstallerOptions {
            index_url,
            extra_index_url,
            no_index,
            find_links,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease,
            config_settings,
            exclude_newer,
            link_mode,
            compile_bytecode: _,
        } = workspace
            .map(|workspace| workspace.options.top_level)
            .unwrap_or_default();

        Self {
            index_locations: IndexLocations::new(
                args.index_url.combine(index_url),
                args.extra_index_url
                    .combine(extra_index_url)
                    .unwrap_or_default(),
                args.find_links.combine(find_links).unwrap_or_default(),
                args.no_index.combine(no_index).unwrap_or_default(),
            ),
            resolution: args.resolution.combine(resolution).unwrap_or_default(),
            prerelease: args.prerelease.combine(prerelease).unwrap_or_default(),
            index_strategy: args
                .index_strategy
                .combine(index_strategy)
                .unwrap_or_default(),
            keyring_provider: args
                .keyring_provider
                .combine(keyring_provider)
                .unwrap_or_default(),
            config_setting: args
                .config_settings
                .combine(config_settings)
                .unwrap_or_default(),
            exclude_newer: args.exclude_newer.combine(exclude_newer),
            link_mode: args.link_mode.combine(link_mode).unwrap_or_default(),
        }
    }
}

/// The resolved settings to use for an invocation of the `uv` CLI with both resolver and installer
/// capabilities.
///
/// Represents the shared settings that are used across all `uv` commands outside the `pip` API.
/// Analogous to the settings contained in the `[tool.uv]` table, combined with [`ResolverInstallerArgs`].
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
pub(crate) struct ResolverInstallerSettings {
    pub(crate) index_locations: IndexLocations,
    pub(crate) index_strategy: IndexStrategy,
    pub(crate) keyring_provider: KeyringProviderType,
    pub(crate) resolution: ResolutionMode,
    pub(crate) prerelease: PreReleaseMode,
    pub(crate) config_setting: ConfigSettings,
    pub(crate) exclude_newer: Option<ExcludeNewer>,
    pub(crate) link_mode: LinkMode,
    pub(crate) compile_bytecode: bool,
}

impl ResolverInstallerSettings {
    /// Resolve the [`ResolverInstallerSettings`] from the CLI and workspace configuration.
    pub(crate) fn combine(args: ResolverInstallerOptions, workspace: Option<Workspace>) -> Self {
        let ResolverInstallerOptions {
            index_url,
            extra_index_url,
            no_index,
            find_links,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease,
            config_settings,
            exclude_newer,
            link_mode,
            compile_bytecode,
        } = workspace
            .map(|workspace| workspace.options.top_level)
            .unwrap_or_default();

        Self {
            index_locations: IndexLocations::new(
                args.index_url.combine(index_url),
                args.extra_index_url
                    .combine(extra_index_url)
                    .unwrap_or_default(),
                args.find_links.combine(find_links).unwrap_or_default(),
                args.no_index.combine(no_index).unwrap_or_default(),
            ),
            resolution: args.resolution.combine(resolution).unwrap_or_default(),
            prerelease: args.prerelease.combine(prerelease).unwrap_or_default(),
            index_strategy: args
                .index_strategy
                .combine(index_strategy)
                .unwrap_or_default(),
            keyring_provider: args
                .keyring_provider
                .combine(keyring_provider)
                .unwrap_or_default(),
            config_setting: args
                .config_settings
                .combine(config_settings)
                .unwrap_or_default(),
            exclude_newer: args.exclude_newer.combine(exclude_newer),
            link_mode: args.link_mode.combine(link_mode).unwrap_or_default(),
            compile_bytecode: args
                .compile_bytecode
                .combine(compile_bytecode)
                .unwrap_or_default(),
        }
    }
}

/// The resolved settings to use for an invocation of the `pip` CLI.
///
/// Represents the shared settings that are used across all `pip` commands. Analogous to the
/// settings contained in the `[tool.uv.pip]` table.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PipSettings {
    pub(crate) index_locations: IndexLocations,
    pub(crate) python: Option<String>,
    pub(crate) system: bool,
    pub(crate) extras: ExtrasSpecification,
    pub(crate) break_system_packages: bool,
    pub(crate) target: Option<Target>,
    pub(crate) prefix: Option<Prefix>,
    pub(crate) index_strategy: IndexStrategy,
    pub(crate) keyring_provider: KeyringProviderType,
    pub(crate) no_binary: NoBinary,
    pub(crate) no_build: NoBuild,
    pub(crate) no_build_isolation: bool,
    pub(crate) strict: bool,
    pub(crate) dependency_mode: DependencyMode,
    pub(crate) resolution: ResolutionMode,
    pub(crate) prerelease: PreReleaseMode,
    pub(crate) output_file: Option<PathBuf>,
    pub(crate) no_strip_extras: bool,
    pub(crate) no_annotate: bool,
    pub(crate) no_header: bool,
    pub(crate) custom_compile_command: Option<String>,
    pub(crate) generate_hashes: bool,
    pub(crate) setup_py: SetupPyStrategy,
    pub(crate) config_setting: ConfigSettings,
    pub(crate) python_version: Option<PythonVersion>,
    pub(crate) python_platform: Option<TargetTriple>,
    pub(crate) exclude_newer: Option<ExcludeNewer>,
    pub(crate) no_emit_package: Vec<PackageName>,
    pub(crate) emit_index_url: bool,
    pub(crate) emit_find_links: bool,
    pub(crate) emit_marker_expression: bool,
    pub(crate) emit_index_annotation: bool,
    pub(crate) annotation_style: AnnotationStyle,
    pub(crate) link_mode: LinkMode,
    pub(crate) compile_bytecode: bool,
    pub(crate) require_hashes: bool,
    pub(crate) concurrency: Concurrency,
}

impl PipSettings {
    /// Resolve the [`PipSettings`] from the CLI and workspace configuration.
    pub(crate) fn combine(args: PipOptions, workspace: Option<Workspace>) -> Self {
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
            concurrent_builds,
            concurrent_downloads,
            concurrent_installs,
        } = workspace
            .map(|workspace| workspace.options.pip())
            .unwrap_or_default();

        Self {
            index_locations: IndexLocations::new(
                args.index_url.combine(index_url),
                args.extra_index_url
                    .combine(extra_index_url)
                    .unwrap_or_default(),
                args.find_links.combine(find_links).unwrap_or_default(),
                args.no_index.combine(no_index).unwrap_or_default(),
            ),
            extras: ExtrasSpecification::from_args(
                args.all_extras.combine(all_extras).unwrap_or_default(),
                args.extra.combine(extra).unwrap_or_default(),
            ),
            dependency_mode: if args.no_deps.combine(no_deps).unwrap_or_default() {
                DependencyMode::Direct
            } else {
                DependencyMode::Transitive
            },
            resolution: args.resolution.combine(resolution).unwrap_or_default(),
            prerelease: args.prerelease.combine(prerelease).unwrap_or_default(),
            output_file: args.output_file.combine(output_file),
            no_strip_extras: args
                .no_strip_extras
                .combine(no_strip_extras)
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
            setup_py: if args
                .legacy_setup_py
                .combine(legacy_setup_py)
                .unwrap_or_default()
            {
                SetupPyStrategy::Setuptools
            } else {
                SetupPyStrategy::Pep517
            },
            no_build_isolation: args
                .no_build_isolation
                .combine(no_build_isolation)
                .unwrap_or_default(),
            no_build: NoBuild::from_args(
                args.only_binary.combine(only_binary).unwrap_or_default(),
                args.no_build.combine(no_build).unwrap_or_default(),
            ),
            config_setting: args
                .config_settings
                .combine(config_settings)
                .unwrap_or_default(),
            python_version: args.python_version.combine(python_version),
            python_platform: args.python_platform.combine(python_platform),
            exclude_newer: args.exclude_newer.combine(exclude_newer),
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
            emit_marker_expression: args
                .emit_marker_expression
                .combine(emit_marker_expression)
                .unwrap_or_default(),
            emit_index_annotation: args
                .emit_index_annotation
                .combine(emit_index_annotation)
                .unwrap_or_default(),
            link_mode: args.link_mode.combine(link_mode).unwrap_or_default(),
            require_hashes: args
                .require_hashes
                .combine(require_hashes)
                .unwrap_or_default(),
            python: args.python.combine(python),
            system: args.system.combine(system).unwrap_or_default(),
            break_system_packages: args
                .break_system_packages
                .combine(break_system_packages)
                .unwrap_or_default(),
            target: args.target.combine(target).map(Target::from),
            prefix: args.prefix.combine(prefix).map(Prefix::from),
            no_binary: NoBinary::from_args(args.no_binary.combine(no_binary).unwrap_or_default()),
            compile_bytecode: args
                .compile_bytecode
                .combine(compile_bytecode)
                .unwrap_or_default(),
            strict: args.strict.combine(strict).unwrap_or_default(),
            concurrency: Concurrency {
                downloads: args
                    .concurrent_downloads
                    .combine(concurrent_downloads)
                    .map(NonZeroUsize::get)
                    .unwrap_or(Concurrency::DEFAULT_DOWNLOADS),
                builds: args
                    .concurrent_builds
                    .combine(concurrent_builds)
                    .map(NonZeroUsize::get)
                    .unwrap_or_else(Concurrency::threads),
                installs: args
                    .concurrent_installs
                    .combine(concurrent_installs)
                    .map(NonZeroUsize::get)
                    .unwrap_or_else(Concurrency::threads),
            },
        }
    }
}

// Environment variables that are not exposed as CLI arguments.
mod env {
    pub(super) const CONCURRENT_DOWNLOADS: (&str, &str) =
        ("UV_CONCURRENT_DOWNLOADS", "a non-zero integer");

    pub(super) const CONCURRENT_BUILDS: (&str, &str) =
        ("UV_CONCURRENT_BUILDS", "a non-zero integer");

    pub(super) const CONCURRENT_INSTALLS: (&str, &str) =
        ("UV_CONCURRENT_INSTALLS", "a non-zero integer");
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
#[allow(clippy::exit, clippy::print_stderr)]
fn parse_failure(name: &str, expected: &str) -> ! {
    eprintln!("error: invalid value for {name}, expected {expected}");
    process::exit(1)
}

/// Given a boolean flag pair (like `--upgrade` and `--no-upgrade`), resolve the value of the flag.
fn flag(yes: bool, no: bool) -> Option<bool> {
    match (yes, no) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        (false, false) => None,
        (..) => unreachable!("Clap should make this impossible"),
    }
}

impl From<ResolverArgs> for PipOptions {
    fn from(args: ResolverArgs) -> Self {
        let ResolverArgs {
            index_args,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease,
            pre,
            config_setting,
            exclude_newer,
            link_mode,
        } = args;

        Self {
            index_url: index_args.index_url.and_then(Maybe::into_option),
            extra_index_url: index_args.extra_index_url.map(|extra_index_urls| {
                extra_index_urls
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect()
            }),
            no_index: Some(index_args.no_index),
            find_links: index_args.find_links,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease: if pre {
                Some(PreReleaseMode::Allow)
            } else {
                prerelease
            },
            config_settings: config_setting
                .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
            exclude_newer,
            link_mode,
            ..PipOptions::default()
        }
    }
}

impl From<InstallerArgs> for PipOptions {
    fn from(args: InstallerArgs) -> Self {
        let InstallerArgs {
            index_args,
            index_strategy,
            keyring_provider,
            config_setting,
            link_mode,
            compile_bytecode,
            no_compile_bytecode,
        } = args;

        Self {
            index_url: index_args.index_url.and_then(Maybe::into_option),
            extra_index_url: index_args.extra_index_url.map(|extra_index_urls| {
                extra_index_urls
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect()
            }),
            no_index: Some(index_args.no_index),
            find_links: index_args.find_links,
            index_strategy,
            keyring_provider,
            config_settings: config_setting
                .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
            link_mode,
            compile_bytecode: flag(compile_bytecode, no_compile_bytecode),
            ..PipOptions::default()
        }
    }
}

impl From<ResolverInstallerArgs> for PipOptions {
    fn from(args: ResolverInstallerArgs) -> Self {
        let ResolverInstallerArgs {
            index_args,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease,
            pre,
            config_setting,
            exclude_newer,
            link_mode,
            compile_bytecode,
            no_compile_bytecode,
        } = args;

        Self {
            index_url: index_args.index_url.and_then(Maybe::into_option),
            extra_index_url: index_args.extra_index_url.map(|extra_index_urls| {
                extra_index_urls
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect()
            }),
            no_index: Some(index_args.no_index),
            find_links: index_args.find_links,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease: if pre {
                Some(PreReleaseMode::Allow)
            } else {
                prerelease
            },
            config_settings: config_setting
                .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
            exclude_newer,
            link_mode,
            compile_bytecode: flag(compile_bytecode, no_compile_bytecode),
            ..PipOptions::default()
        }
    }
}

impl From<InstallerArgs> for InstallerOptions {
    fn from(args: InstallerArgs) -> Self {
        let InstallerArgs {
            index_args,
            index_strategy,
            keyring_provider,
            config_setting,
            link_mode,
            compile_bytecode,
            no_compile_bytecode,
        } = args;

        Self {
            index_url: index_args.index_url.and_then(Maybe::into_option),
            extra_index_url: index_args.extra_index_url.map(|extra_index_urls| {
                extra_index_urls
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect()
            }),
            no_index: Some(index_args.no_index),
            find_links: index_args.find_links,
            index_strategy,
            keyring_provider,
            config_settings: config_setting
                .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
            link_mode,
            compile_bytecode: flag(compile_bytecode, no_compile_bytecode),
        }
    }
}

impl From<ResolverArgs> for ResolverOptions {
    fn from(args: ResolverArgs) -> Self {
        let ResolverArgs {
            index_args,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease,
            pre,
            config_setting,
            exclude_newer,
            link_mode,
        } = args;

        Self {
            index_url: index_args.index_url.and_then(Maybe::into_option),
            extra_index_url: index_args.extra_index_url.map(|extra_index_urls| {
                extra_index_urls
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect()
            }),
            no_index: Some(index_args.no_index),
            find_links: index_args.find_links,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease: if pre {
                Some(PreReleaseMode::Allow)
            } else {
                prerelease
            },
            config_settings: config_setting
                .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
            exclude_newer,
            link_mode,
        }
    }
}

impl From<ResolverInstallerArgs> for ResolverInstallerOptions {
    fn from(args: ResolverInstallerArgs) -> Self {
        let ResolverInstallerArgs {
            index_args,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease,
            pre,
            config_setting,
            exclude_newer,
            link_mode,
            compile_bytecode,
            no_compile_bytecode,
        } = args;

        Self {
            index_url: index_args.index_url.and_then(Maybe::into_option),
            extra_index_url: index_args.extra_index_url.map(|extra_index_urls| {
                extra_index_urls
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect()
            }),
            no_index: Some(index_args.no_index),
            find_links: index_args.find_links,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease: if pre {
                Some(PreReleaseMode::Allow)
            } else {
                prerelease
            },
            config_settings: config_setting
                .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
            exclude_newer,
            link_mode,
            compile_bytecode: flag(compile_bytecode, no_compile_bytecode),
        }
    }
}

impl From<IndexArgs> for PipOptions {
    fn from(args: IndexArgs) -> Self {
        let IndexArgs {
            index_url,
            extra_index_url,
            no_index,
            find_links,
        } = args;

        Self {
            index_url: index_url.and_then(Maybe::into_option),
            extra_index_url: extra_index_url.map(|extra_index_urls| {
                extra_index_urls
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect()
            }),
            no_index: Some(no_index),
            find_links,
            ..PipOptions::default()
        }
    }
}
