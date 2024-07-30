use std::env::VarError;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process;
use std::str::FromStr;

use distribution_types::IndexLocations;
use install_wheel_rs::linker::LinkMode;
use pep508_rs::{ExtraName, RequirementOrigin};
use pypi_types::Requirement;
use uv_cache::{CacheArgs, Refresh};
use uv_cli::options::{flag, resolver_installer_options, resolver_options};
use uv_cli::{
    AddArgs, ColorChoice, Commands, ExternalCommand, GlobalArgs, InitArgs, ListFormat, LockArgs,
    Maybe, PipCheckArgs, PipCompileArgs, PipFreezeArgs, PipInstallArgs, PipListArgs, PipShowArgs,
    PipSyncArgs, PipTreeArgs, PipUninstallArgs, PythonFindArgs, PythonInstallArgs, PythonListArgs,
    PythonPinArgs, PythonUninstallArgs, RemoveArgs, RunArgs, SyncArgs, ToolDirArgs,
    ToolInstallArgs, ToolListArgs, ToolRunArgs, ToolUninstallArgs, TreeArgs, VenvArgs,
};
use uv_client::Connectivity;
use uv_configuration::{
    BuildOptions, Concurrency, ConfigSettings, ExtrasSpecification, HashCheckingMode,
    IndexStrategy, KeyringProviderType, NoBinary, NoBuild, PreviewMode, Reinstall, SetupPyStrategy,
    TargetTriple, Upgrade,
};
use uv_normalize::PackageName;
use uv_python::{Prefix, PythonFetch, PythonPreference, PythonVersion, Target};
use uv_requirements::RequirementsSource;
use uv_resolver::{AnnotationStyle, DependencyMode, ExcludeNewer, PreReleaseMode, ResolutionMode};
use uv_settings::{
    Combine, FilesystemOptions, Options, PipOptions, ResolverInstallerOptions, ResolverOptions,
};
use uv_workspace::pyproject::DependencyType;

use crate::commands::pip::operations::Modifications;

/// The resolved global settings to use for any invocation of the CLI.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct GlobalSettings {
    pub(crate) quiet: bool,
    pub(crate) verbose: u8,
    pub(crate) color: ColorChoice,
    pub(crate) native_tls: bool,
    pub(crate) connectivity: Connectivity,
    pub(crate) show_settings: bool,
    pub(crate) preview: PreviewMode,
    pub(crate) python_preference: PythonPreference,
    pub(crate) python_fetch: PythonFetch,
    pub(crate) no_progress: bool,
}

impl GlobalSettings {
    /// Resolve the [`GlobalSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(
        command: &Commands,
        args: &GlobalArgs,
        workspace: Option<&FilesystemOptions>,
    ) -> Self {
        let preview = PreviewMode::from(
            flag(args.preview, args.no_preview)
                .combine(workspace.and_then(|workspace| workspace.globals.preview))
                .unwrap_or(false),
        );

        // Always use preview mode python preferences during preview commands
        // TODO(zanieb): There should be a cleaner way to do this, we should probably resolve
        // force preview to true for these commands but it would break our experimental warning
        // right now
        let default_python_preference = if matches!(
            command,
            Commands::Project(_) | Commands::Python(_) | Commands::Tool(_)
        ) {
            PythonPreference::default_from(PreviewMode::Enabled)
        } else {
            PythonPreference::default_from(preview)
        };

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
                .combine(workspace.and_then(|workspace| workspace.globals.native_tls))
                .unwrap_or(false),
            connectivity: if flag(args.offline, args.no_offline)
                .combine(workspace.and_then(|workspace| workspace.globals.offline))
                .unwrap_or(false)
            {
                Connectivity::Offline
            } else {
                Connectivity::Online
            },
            show_settings: args.show_settings,
            preview,
            python_preference: args
                .python_preference
                .combine(workspace.and_then(|workspace| workspace.globals.python_preference))
                .unwrap_or(default_python_preference),
            python_fetch: args
                .python_fetch
                .combine(workspace.and_then(|workspace| workspace.globals.python_fetch))
                .unwrap_or_default(),
            no_progress: args.no_progress,
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
    /// Resolve the [`CacheSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: CacheArgs, workspace: Option<&FilesystemOptions>) -> Self {
        Self {
            no_cache: args.no_cache
                || workspace
                    .and_then(|workspace| workspace.globals.no_cache)
                    .unwrap_or(false),
            cache_dir: args
                .cache_dir
                .or_else(|| workspace.and_then(|workspace| workspace.globals.cache_dir.clone())),
        }
    }
}

/// The resolved settings to use for a `init` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct InitSettings {
    pub(crate) path: Option<String>,
    pub(crate) name: Option<PackageName>,
    pub(crate) r#virtual: bool,
    pub(crate) no_readme: bool,
    pub(crate) no_workspace: bool,
    pub(crate) python: Option<String>,
}

impl InitSettings {
    /// Resolve the [`InitSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: InitArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let InitArgs {
            path,
            name,
            r#virtual,
            no_readme,
            no_workspace,
            python,
        } = args;

        Self {
            path,
            name,
            r#virtual,
            no_readme,
            no_workspace,
            python,
        }
    }
}

/// The resolved settings to use for a `run` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct RunSettings {
    pub(crate) locked: bool,
    pub(crate) frozen: bool,
    pub(crate) extras: ExtrasSpecification,
    pub(crate) dev: bool,
    pub(crate) command: ExternalCommand,
    pub(crate) with: Vec<String>,
    pub(crate) with_requirements: Vec<PathBuf>,
    pub(crate) isolated: bool,
    pub(crate) show_resolution: bool,
    pub(crate) package: Option<PackageName>,
    pub(crate) no_project: bool,
    pub(crate) python: Option<String>,
    pub(crate) refresh: Refresh,
    pub(crate) settings: ResolverInstallerSettings,
}

impl RunSettings {
    /// Resolve the [`RunSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: RunArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let RunArgs {
            extra,
            all_extras,
            no_all_extras,
            dev,
            no_dev,
            command,
            with,
            with_requirements,
            isolated,
            locked,
            frozen,
            installer,
            build,
            refresh,
            package,
            no_project,
            python,
            show_resolution,
        } = args;

        Self {
            locked,
            frozen,
            extras: ExtrasSpecification::from_args(
                flag(all_extras, no_all_extras).unwrap_or_default(),
                extra.unwrap_or_default(),
            ),
            dev: flag(dev, no_dev).unwrap_or(true),
            command,
            with,
            with_requirements: with_requirements
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            isolated,
            show_resolution,
            package,
            no_project,
            python,
            refresh: Refresh::from(refresh),
            settings: ResolverInstallerSettings::combine(
                resolver_installer_options(installer, build),
                filesystem,
            ),
        }
    }
}

/// The resolved settings to use for a `tool run` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct ToolRunSettings {
    pub(crate) command: Option<ExternalCommand>,
    pub(crate) from: Option<String>,
    pub(crate) with: Vec<String>,
    pub(crate) with_requirements: Vec<PathBuf>,
    pub(crate) isolated: bool,
    pub(crate) show_resolution: bool,
    pub(crate) python: Option<String>,
    pub(crate) refresh: Refresh,
    pub(crate) settings: ResolverInstallerSettings,
}

impl ToolRunSettings {
    /// Resolve the [`ToolRunSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: ToolRunArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let ToolRunArgs {
            command,
            from,
            with,
            with_requirements,
            isolated,
            show_resolution,
            installer,
            build,
            refresh,
            python,
        } = args;

        Self {
            command,
            from,
            with,
            with_requirements: with_requirements
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            isolated,
            show_resolution,
            python,
            refresh: Refresh::from(refresh),
            settings: ResolverInstallerSettings::combine(
                resolver_installer_options(installer, build),
                filesystem,
            ),
        }
    }
}

/// The resolved settings to use for a `tool install` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct ToolInstallSettings {
    pub(crate) package: String,
    pub(crate) from: Option<String>,
    pub(crate) with: Vec<String>,
    pub(crate) with_requirements: Vec<PathBuf>,
    pub(crate) python: Option<String>,
    pub(crate) refresh: Refresh,
    pub(crate) settings: ResolverInstallerSettings,
    pub(crate) force: bool,
    pub(crate) editable: bool,
}

impl ToolInstallSettings {
    /// Resolve the [`ToolInstallSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: ToolInstallArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let ToolInstallArgs {
            package,
            editable,
            from,
            with,
            with_requirements,
            installer,
            force,
            build,
            refresh,
            python,
        } = args;

        Self {
            package,
            from,
            with,
            with_requirements: with_requirements
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            python,
            force,
            editable,
            refresh: Refresh::from(refresh),
            settings: ResolverInstallerSettings::combine(
                resolver_installer_options(installer, build),
                filesystem,
            ),
        }
    }
}

/// The resolved settings to use for a `tool list` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct ToolListSettings {
    pub(crate) show_paths: bool,
}

impl ToolListSettings {
    /// Resolve the [`ToolListSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: ToolListArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let ToolListArgs { show_paths } = args;

        Self { show_paths }
    }
}

/// The resolved settings to use for a `tool uninstall` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct ToolUninstallSettings {
    pub(crate) name: Option<PackageName>,
}

impl ToolUninstallSettings {
    /// Resolve the [`ToolUninstallSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: ToolUninstallArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let ToolUninstallArgs { name, all } = args;

        Self {
            name: name.filter(|_| !all),
        }
    }
}

/// The resolved settings to use for a `tool dir` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct ToolDirSettings {
    pub(crate) bin: bool,
}

impl ToolDirSettings {
    /// Resolve the [`ToolDirSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: ToolDirArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let ToolDirArgs { bin } = args;

        Self { bin }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) enum PythonListKinds {
    #[default]
    Default,
    Installed,
}

/// The resolved settings to use for a `tool run` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PythonListSettings {
    pub(crate) kinds: PythonListKinds,
    pub(crate) all_platforms: bool,
    pub(crate) all_versions: bool,
}

impl PythonListSettings {
    /// Resolve the [`PythonListSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: PythonListArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let PythonListArgs {
            all_versions,
            all_platforms,
            only_installed,
        } = args;

        let kinds = if only_installed {
            PythonListKinds::Installed
        } else {
            PythonListKinds::default()
        };

        Self {
            kinds,
            all_platforms,
            all_versions,
        }
    }
}

/// The resolved settings to use for a `python install` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PythonInstallSettings {
    pub(crate) targets: Vec<String>,
    pub(crate) reinstall: bool,
}

impl PythonInstallSettings {
    /// Resolve the [`PythonInstallSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: PythonInstallArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let PythonInstallArgs { targets, reinstall } = args;

        Self { targets, reinstall }
    }
}

/// The resolved settings to use for a `python uninstall` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PythonUninstallSettings {
    pub(crate) targets: Vec<String>,
    pub(crate) all: bool,
}

impl PythonUninstallSettings {
    /// Resolve the [`PythonUninstallSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(
        args: PythonUninstallArgs,
        _filesystem: Option<FilesystemOptions>,
    ) -> Self {
        let PythonUninstallArgs { targets, all } = args;

        Self { targets, all }
    }
}

/// The resolved settings to use for a `python find` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PythonFindSettings {
    pub(crate) request: Option<String>,
}

impl PythonFindSettings {
    /// Resolve the [`PythonFindSettings`] from the CLI and workspace configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: PythonFindArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let PythonFindArgs { request } = args;

        Self { request }
    }
}

/// The resolved settings to use for a `python pin` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PythonPinSettings {
    pub(crate) request: Option<String>,
    pub(crate) resolved: bool,
    pub(crate) no_workspace: bool,
}

impl PythonPinSettings {
    /// Resolve the [`PythonPinSettings`] from the CLI and workspace configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: PythonPinArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let PythonPinArgs {
            request,
            no_resolved,
            resolved,
            no_workspace,
        } = args;

        Self {
            request,
            resolved: flag(resolved, no_resolved).unwrap_or(false),
            no_workspace,
        }
    }
}
/// The resolved settings to use for a `sync` invocation.
#[allow(clippy::struct_excessive_bools, dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct SyncSettings {
    pub(crate) locked: bool,
    pub(crate) frozen: bool,
    pub(crate) extras: ExtrasSpecification,
    pub(crate) dev: bool,
    pub(crate) modifications: Modifications,
    pub(crate) python: Option<String>,
    pub(crate) refresh: Refresh,
    pub(crate) settings: ResolverInstallerSettings,
}

impl SyncSettings {
    /// Resolve the [`SyncSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: SyncArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let SyncArgs {
            locked,
            frozen,
            extra,
            all_extras,
            no_all_extras,
            dev,
            no_dev,
            no_clean,
            installer,
            build,
            refresh,
            python,
        } = args;

        let modifications = if no_clean {
            Modifications::Sufficient
        } else {
            Modifications::Exact
        };

        Self {
            locked,
            frozen,
            extras: ExtrasSpecification::from_args(
                flag(all_extras, no_all_extras).unwrap_or_default(),
                extra.unwrap_or_default(),
            ),
            dev: flag(dev, no_dev).unwrap_or(true),
            modifications,
            python,
            refresh: Refresh::from(refresh),
            settings: ResolverInstallerSettings::combine(
                resolver_installer_options(installer, build),
                filesystem,
            ),
        }
    }
}

/// The resolved settings to use for a `lock` invocation.
#[allow(clippy::struct_excessive_bools, dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct LockSettings {
    pub(crate) locked: bool,
    pub(crate) frozen: bool,
    pub(crate) python: Option<String>,
    pub(crate) refresh: Refresh,
    pub(crate) settings: ResolverSettings,
}

impl LockSettings {
    /// Resolve the [`LockSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: LockArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let LockArgs {
            locked,
            frozen,
            resolver,
            build,
            refresh,
            python,
        } = args;

        Self {
            locked,
            frozen,
            python,
            refresh: Refresh::from(refresh),
            settings: ResolverSettings::combine(resolver_options(resolver, build), filesystem),
        }
    }
}

/// The resolved settings to use for a `add` invocation.
#[allow(clippy::struct_excessive_bools, dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct AddSettings {
    pub(crate) locked: bool,
    pub(crate) frozen: bool,
    pub(crate) requirements: Vec<RequirementsSource>,
    pub(crate) dependency_type: DependencyType,
    pub(crate) editable: Option<bool>,
    pub(crate) extras: Vec<ExtraName>,
    pub(crate) raw_sources: bool,
    pub(crate) rev: Option<String>,
    pub(crate) tag: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) package: Option<PackageName>,
    pub(crate) python: Option<String>,
    pub(crate) refresh: Refresh,
    pub(crate) settings: ResolverInstallerSettings,
}

impl AddSettings {
    /// Resolve the [`AddSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: AddArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let AddArgs {
            requirements,
            dev,
            optional,
            editable,
            no_editable,
            extra,
            raw_sources,
            rev,
            tag,
            branch,
            locked,
            frozen,
            installer,
            build,
            refresh,
            package,
            python,
        } = args;

        let requirements = requirements
            .into_iter()
            .map(RequirementsSource::Package)
            .collect::<Vec<_>>();

        let dependency_type = if let Some(group) = optional {
            DependencyType::Optional(group)
        } else if dev {
            DependencyType::Dev
        } else {
            DependencyType::Production
        };

        Self {
            locked,
            frozen,
            requirements,
            dependency_type,
            raw_sources,
            rev,
            tag,
            branch,
            package,
            python,
            editable: flag(editable, no_editable),
            extras: extra.unwrap_or_default(),
            refresh: Refresh::from(refresh),
            settings: ResolverInstallerSettings::combine(
                resolver_installer_options(installer, build),
                filesystem,
            ),
        }
    }
}

/// The resolved settings to use for a `remove` invocation.
#[allow(clippy::struct_excessive_bools, dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct RemoveSettings {
    pub(crate) locked: bool,
    pub(crate) frozen: bool,
    pub(crate) requirements: Vec<PackageName>,
    pub(crate) dependency_type: DependencyType,
    pub(crate) package: Option<PackageName>,
    pub(crate) python: Option<String>,
    pub(crate) refresh: Refresh,
    pub(crate) settings: ResolverInstallerSettings,
}

impl RemoveSettings {
    /// Resolve the [`RemoveSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: RemoveArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let RemoveArgs {
            dev,
            optional,
            requirements,
            locked,
            frozen,
            installer,
            build,
            refresh,
            package,
            python,
        } = args;

        let dependency_type = if let Some(group) = optional {
            DependencyType::Optional(group)
        } else if dev {
            DependencyType::Dev
        } else {
            DependencyType::Production
        };

        Self {
            locked,
            frozen,
            requirements,
            dependency_type,
            package,
            python,
            refresh: Refresh::from(refresh),
            settings: ResolverInstallerSettings::combine(
                resolver_installer_options(installer, build),
                filesystem,
            ),
        }
    }
}

/// The resolved settings to use for a `tree` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct TreeSettings {
    pub(crate) locked: bool,
    pub(crate) frozen: bool,
    pub(crate) depth: u8,
    pub(crate) prune: Vec<PackageName>,
    pub(crate) package: Vec<PackageName>,
    pub(crate) no_dedupe: bool,
    pub(crate) invert: bool,
    pub(crate) show_version_specifiers: bool,
    pub(crate) python: Option<String>,
    pub(crate) resolver: ResolverSettings,
}

impl TreeSettings {
    /// Resolve the [`TreeSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: TreeArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let TreeArgs {
            tree,
            locked,
            frozen,
            build,
            resolver,
            python,
        } = args;

        Self {
            locked,
            frozen,
            depth: tree.depth,
            prune: tree.prune,
            package: tree.package,
            no_dedupe: tree.no_dedupe,
            invert: tree.invert,
            show_version_specifiers: tree.show_version_specifiers,
            python,
            resolver: ResolverSettings::combine(resolver_options(resolver, build), filesystem),
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
    pub(crate) constraints_from_workspace: Vec<Requirement>,
    pub(crate) overrides_from_workspace: Vec<Requirement>,
    pub(crate) refresh: Refresh,
    pub(crate) settings: PipSettings,
}

impl PipCompileSettings {
    /// Resolve the [`PipCompileSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: PipCompileArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let PipCompileArgs {
            src_file,
            constraint,
            r#override,
            extra,
            all_extras,
            no_all_extras,
            refresh,
            no_deps,
            deps,
            output_file,
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

        let overrides_from_workspace = if let Some(configuration) = &filesystem {
            configuration
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
            r#override: r#override
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            constraints_from_workspace,
            overrides_from_workspace,
            refresh: Refresh::from(refresh),
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
                    no_strip_markers: flag(no_strip_markers, strip_markers),
                    no_annotate: flag(no_annotate, annotate),
                    no_header: flag(no_header, header),
                    custom_compile_command,
                    generate_hashes: flag(generate_hashes, no_generate_hashes),
                    legacy_setup_py: flag(legacy_setup_py, no_legacy_setup_py),
                    python_version,
                    python_platform,
                    universal: flag(universal, no_universal),
                    no_emit_package,
                    emit_index_url: flag(emit_index_url, no_emit_index_url),
                    emit_find_links: flag(emit_find_links, no_emit_find_links),
                    emit_build_options: flag(emit_build_options, no_emit_build_options),
                    emit_marker_expression: flag(emit_marker_expression, no_emit_marker_expression),
                    emit_index_annotation: flag(emit_index_annotation, no_emit_index_annotation),
                    annotation_style,
                    concurrent_builds: env(env::CONCURRENT_BUILDS),
                    concurrent_downloads: env(env::CONCURRENT_DOWNLOADS),
                    concurrent_installs: env(env::CONCURRENT_INSTALLS),
                    ..PipOptions::from(resolver)
                },
                filesystem,
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
    pub(crate) dry_run: bool,
    pub(crate) refresh: Refresh,
    pub(crate) settings: PipSettings,
}

impl PipSyncSettings {
    /// Resolve the [`PipSyncSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: Box<PipSyncArgs>, filesystem: Option<FilesystemOptions>) -> Self {
        let PipSyncArgs {
            src_file,
            constraint,
            installer,
            refresh,
            require_hashes,
            no_require_hashes,
            verify_hashes,
            no_verify_hashes,
            python,
            system,
            no_system,
            break_system_packages,
            no_break_system_packages,
            target,
            prefix,
            allow_empty_requirements,
            no_allow_empty_requirements,
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
        } = *args;

        Self {
            src_file,
            constraint: constraint
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            dry_run,
            refresh: Refresh::from(refresh),
            settings: PipSettings::combine(
                PipOptions {
                    python,
                    system: flag(system, no_system),
                    break_system_packages: flag(break_system_packages, no_break_system_packages),
                    target,
                    prefix,
                    require_hashes: flag(require_hashes, no_require_hashes),
                    verify_hashes: flag(verify_hashes, no_verify_hashes),
                    no_build: flag(no_build, build),
                    no_binary,
                    only_binary,
                    allow_empty_requirements: flag(
                        allow_empty_requirements,
                        no_allow_empty_requirements,
                    ),
                    legacy_setup_py: flag(legacy_setup_py, no_legacy_setup_py),
                    no_build_isolation: flag(no_build_isolation, build_isolation),
                    python_version,
                    python_platform,
                    strict: flag(strict, no_strict),
                    concurrent_builds: env(env::CONCURRENT_BUILDS),
                    concurrent_downloads: env(env::CONCURRENT_DOWNLOADS),
                    concurrent_installs: env(env::CONCURRENT_INSTALLS),
                    ..PipOptions::from(installer)
                },
                filesystem,
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
    pub(crate) dry_run: bool,
    pub(crate) constraints_from_workspace: Vec<Requirement>,
    pub(crate) overrides_from_workspace: Vec<Requirement>,
    pub(crate) refresh: Refresh,
    pub(crate) settings: PipSettings,
}

impl PipInstallSettings {
    /// Resolve the [`PipInstallSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: PipInstallArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let PipInstallArgs {
            package,
            requirement,
            editable,
            constraint,
            r#override,
            extra,
            all_extras,
            no_all_extras,
            refresh,
            no_deps,
            deps,
            require_hashes,
            no_require_hashes,
            installer,
            verify_hashes,
            no_verify_hashes,
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

        let overrides_from_workspace = if let Some(configuration) = &filesystem {
            configuration
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
            r#override: r#override
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            dry_run,
            constraints_from_workspace,
            overrides_from_workspace,
            refresh: Refresh::from(refresh),
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
                    verify_hashes: flag(verify_hashes, no_verify_hashes),
                    concurrent_builds: env(env::CONCURRENT_BUILDS),
                    concurrent_downloads: env(env::CONCURRENT_DOWNLOADS),
                    concurrent_installs: env(env::CONCURRENT_INSTALLS),
                    ..PipOptions::from(installer)
                },
                filesystem,
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
    /// Resolve the [`PipUninstallSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: PipUninstallArgs, filesystem: Option<FilesystemOptions>) -> Self {
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
            compat_args: _,
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
                filesystem,
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
    /// Resolve the [`PipFreezeSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: PipFreezeArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let PipFreezeArgs {
            exclude_editable,
            strict,
            no_strict,
            python,
            system,
            no_system,
            compat_args: _,
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
                filesystem,
            ),
        }
    }
}

/// The resolved settings to use for a `pip list` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PipListSettings {
    pub(crate) editable: Option<bool>,
    pub(crate) exclude: Vec<PackageName>,
    pub(crate) format: ListFormat,
    pub(crate) settings: PipSettings,
}

impl PipListSettings {
    /// Resolve the [`PipListSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: PipListArgs, filesystem: Option<FilesystemOptions>) -> Self {
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
            editable: flag(editable, exclude_editable),
            exclude,
            format,
            settings: PipSettings::combine(
                PipOptions {
                    python,
                    system: flag(system, no_system),
                    strict: flag(strict, no_strict),
                    ..PipOptions::default()
                },
                filesystem,
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
    /// Resolve the [`PipShowSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: PipShowArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let PipShowArgs {
            package,
            strict,
            no_strict,
            python,
            system,
            no_system,
            compat_args: _,
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
                filesystem,
            ),
        }
    }
}

/// The resolved settings to use for a `pip tree` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PipTreeSettings {
    pub(crate) depth: u8,
    pub(crate) prune: Vec<PackageName>,
    pub(crate) package: Vec<PackageName>,
    pub(crate) no_dedupe: bool,
    pub(crate) invert: bool,
    pub(crate) show_version_specifiers: bool,
    // CLI-only settings.
    pub(crate) shared: PipSettings,
}

impl PipTreeSettings {
    /// Resolve the [`PipTreeSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: PipTreeArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let PipTreeArgs {
            tree,
            strict,
            no_strict,
            python,
            system,
            no_system,
            compat_args: _,
        } = args;

        Self {
            depth: tree.depth,
            prune: tree.prune,
            no_dedupe: tree.no_dedupe,
            invert: tree.invert,
            show_version_specifiers: tree.show_version_specifiers,
            package: tree.package,
            // Shared settings.
            shared: PipSettings::combine(
                PipOptions {
                    python,
                    system: flag(system, no_system),
                    strict: flag(strict, no_strict),
                    ..PipOptions::default()
                },
                filesystem,
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
    /// Resolve the [`PipCheckSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: PipCheckArgs, filesystem: Option<FilesystemOptions>) -> Self {
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
                filesystem,
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
    pub(crate) relocatable: bool,
    pub(crate) settings: PipSettings,
}

impl VenvSettings {
    /// Resolve the [`VenvSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: VenvArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let VenvArgs {
            python,
            system,
            no_system,
            seed,
            allow_existing,
            name,
            prompt,
            system_site_packages,
            relocatable,
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
            relocatable,
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
                filesystem,
            ),
        }
    }
}

/// The resolved settings to use for an invocation of the uv CLI when installing dependencies.
///
/// Combines the `[tool.uv]` persistent configuration with the command-line arguments
/// ([`InstallerArgs`], represented as [`InstallerOptions`]).
#[derive(Debug, Clone)]
pub(crate) struct InstallerSettingsRef<'a> {
    pub(crate) index_locations: &'a IndexLocations,
    pub(crate) index_strategy: IndexStrategy,
    pub(crate) keyring_provider: KeyringProviderType,
    pub(crate) config_setting: &'a ConfigSettings,
    pub(crate) exclude_newer: Option<ExcludeNewer>,
    pub(crate) link_mode: LinkMode,
    pub(crate) compile_bytecode: bool,
    pub(crate) reinstall: &'a Reinstall,
    pub(crate) build_options: &'a BuildOptions,
}

/// The resolved settings to use for an invocation of the uv CLI when resolving dependencies.
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
    pub(crate) upgrade: Upgrade,
    pub(crate) build_options: BuildOptions,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolverSettingsRef<'a> {
    pub(crate) index_locations: &'a IndexLocations,
    pub(crate) index_strategy: IndexStrategy,
    pub(crate) keyring_provider: KeyringProviderType,
    pub(crate) resolution: ResolutionMode,
    pub(crate) prerelease: PreReleaseMode,
    pub(crate) config_setting: &'a ConfigSettings,
    pub(crate) exclude_newer: Option<ExcludeNewer>,
    pub(crate) link_mode: LinkMode,
    pub(crate) upgrade: &'a Upgrade,
    pub(crate) build_options: &'a BuildOptions,
}

impl ResolverSettings {
    /// Resolve the [`ResolverSettings`] from the CLI and filesystem configuration.
    pub(crate) fn combine(args: ResolverOptions, filesystem: Option<FilesystemOptions>) -> Self {
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
            upgrade,
            upgrade_package,
            reinstall: _,
            reinstall_package: _,
            no_build,
            no_build_package,
            no_binary,
            no_binary_package,
        } = filesystem
            .map(FilesystemOptions::into_options)
            .map(|options| options.top_level)
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
            upgrade: Upgrade::from_args(
                args.upgrade.combine(upgrade),
                args.upgrade_package
                    .combine(upgrade_package)
                    .into_iter()
                    .flatten()
                    .map(Requirement::from)
                    .collect(),
            ),
            build_options: BuildOptions::new(
                NoBinary::from_args(
                    args.no_binary.combine(no_binary),
                    args.no_binary_package
                        .combine(no_binary_package)
                        .unwrap_or_default(),
                ),
                NoBuild::from_args(
                    args.no_build.combine(no_build),
                    args.no_build_package
                        .combine(no_build_package)
                        .unwrap_or_default(),
                ),
            ),
        }
    }

    pub(crate) fn as_ref(&self) -> ResolverSettingsRef {
        ResolverSettingsRef {
            index_locations: &self.index_locations,
            index_strategy: self.index_strategy,
            keyring_provider: self.keyring_provider,
            resolution: self.resolution,
            prerelease: self.prerelease,
            config_setting: &self.config_setting,
            exclude_newer: self.exclude_newer,
            link_mode: self.link_mode,
            upgrade: &self.upgrade,
            build_options: &self.build_options,
        }
    }
}

/// The resolved settings to use for an invocation of the uv CLI with both resolver and installer
/// capabilities.
///
/// Represents the shared settings that are used across all uv commands outside the `pip` API.
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
    pub(crate) upgrade: Upgrade,
    pub(crate) reinstall: Reinstall,
    pub(crate) build_options: BuildOptions,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolverInstallerSettingsRef<'a> {
    pub(crate) index_locations: &'a IndexLocations,
    pub(crate) index_strategy: IndexStrategy,
    pub(crate) keyring_provider: KeyringProviderType,
    pub(crate) resolution: ResolutionMode,
    pub(crate) prerelease: PreReleaseMode,
    pub(crate) config_setting: &'a ConfigSettings,
    pub(crate) exclude_newer: Option<ExcludeNewer>,
    pub(crate) link_mode: LinkMode,
    pub(crate) compile_bytecode: bool,
    pub(crate) upgrade: &'a Upgrade,
    pub(crate) reinstall: &'a Reinstall,
    pub(crate) build_options: &'a BuildOptions,
}

impl ResolverInstallerSettings {
    /// Resolve the [`ResolverInstallerSettings`] from the CLI and filesystem configuration.
    pub(crate) fn combine(
        args: ResolverInstallerOptions,
        filesystem: Option<FilesystemOptions>,
    ) -> Self {
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
            upgrade,
            upgrade_package,
            reinstall,
            reinstall_package,
            no_build,
            no_build_package,
            no_binary,
            no_binary_package,
        } = filesystem
            .map(FilesystemOptions::into_options)
            .map(|options| options.top_level)
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
            upgrade: Upgrade::from_args(
                args.upgrade.combine(upgrade),
                args.upgrade_package
                    .combine(upgrade_package)
                    .into_iter()
                    .flatten()
                    .map(Requirement::from)
                    .collect(),
            ),
            reinstall: Reinstall::from_args(
                args.reinstall.combine(reinstall),
                args.reinstall_package
                    .combine(reinstall_package)
                    .unwrap_or_default(),
            ),
            build_options: BuildOptions::new(
                NoBinary::from_args(
                    args.no_binary.combine(no_binary),
                    args.no_binary_package
                        .combine(no_binary_package)
                        .unwrap_or_default(),
                ),
                NoBuild::from_args(
                    args.no_build.combine(no_build),
                    args.no_build_package
                        .combine(no_build_package)
                        .unwrap_or_default(),
                ),
            ),
        }
    }

    pub(crate) fn as_ref(&self) -> ResolverInstallerSettingsRef {
        ResolverInstallerSettingsRef {
            index_locations: &self.index_locations,
            index_strategy: self.index_strategy,
            keyring_provider: self.keyring_provider,
            resolution: self.resolution,
            prerelease: self.prerelease,
            config_setting: &self.config_setting,
            exclude_newer: self.exclude_newer,
            link_mode: self.link_mode,
            compile_bytecode: self.compile_bytecode,
            upgrade: &self.upgrade,
            reinstall: &self.reinstall,
            build_options: &self.build_options,
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
    pub(crate) no_build_isolation: bool,
    pub(crate) build_options: BuildOptions,
    pub(crate) allow_empty_requirements: bool,
    pub(crate) strict: bool,
    pub(crate) dependency_mode: DependencyMode,
    pub(crate) resolution: ResolutionMode,
    pub(crate) prerelease: PreReleaseMode,
    pub(crate) output_file: Option<PathBuf>,
    pub(crate) no_strip_extras: bool,
    pub(crate) no_strip_markers: bool,
    pub(crate) no_annotate: bool,
    pub(crate) no_header: bool,
    pub(crate) custom_compile_command: Option<String>,
    pub(crate) generate_hashes: bool,
    pub(crate) setup_py: SetupPyStrategy,
    pub(crate) config_setting: ConfigSettings,
    pub(crate) python_version: Option<PythonVersion>,
    pub(crate) python_platform: Option<TargetTriple>,
    pub(crate) universal: bool,
    pub(crate) exclude_newer: Option<ExcludeNewer>,
    pub(crate) no_emit_package: Vec<PackageName>,
    pub(crate) emit_index_url: bool,
    pub(crate) emit_find_links: bool,
    pub(crate) emit_build_options: bool,
    pub(crate) emit_marker_expression: bool,
    pub(crate) emit_index_annotation: bool,
    pub(crate) annotation_style: AnnotationStyle,
    pub(crate) link_mode: LinkMode,
    pub(crate) compile_bytecode: bool,
    pub(crate) hash_checking: Option<HashCheckingMode>,
    pub(crate) upgrade: Upgrade,
    pub(crate) reinstall: Reinstall,
    pub(crate) concurrency: Concurrency,
}

impl PipSettings {
    /// Resolve the [`PipSettings`] from the CLI and filesystem configuration.
    pub(crate) fn combine(args: PipOptions, filesystem: Option<FilesystemOptions>) -> Self {
        let Options { top_level, pip, .. } = filesystem
            .map(FilesystemOptions::into_options)
            .unwrap_or_default();

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
            allow_empty_requirements,
            resolution,
            prerelease,
            output_file,
            no_strip_extras,
            no_strip_markers,
            no_annotate,
            no_header,
            custom_compile_command,
            generate_hashes,
            legacy_setup_py,
            config_settings,
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
            upgrade,
            upgrade_package,
            reinstall,
            reinstall_package,
            concurrent_builds,
            concurrent_downloads,
            concurrent_installs,
        } = pip.unwrap_or_default();

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
            no_build: top_level_no_build,
            no_build_package: top_level_no_build_package,
            no_binary: top_level_no_binary,
            no_binary_package: top_level_no_binary_package,
        } = top_level;

        // Merge the top-level options (`tool.uv`) with the pip-specific options (`tool.uv.pip`),
        // preferring the latter.
        //
        // For example, prefer `tool.uv.pip.index-url` over `tool.uv.index-url`.
        let index_url = index_url.combine(top_level_index_url);
        let extra_index_url = extra_index_url.combine(top_level_extra_index_url);
        let no_index = no_index.combine(top_level_no_index);
        let find_links = find_links.combine(top_level_find_links);
        let index_strategy = index_strategy.combine(top_level_index_strategy);
        let keyring_provider = keyring_provider.combine(top_level_keyring_provider);
        let resolution = resolution.combine(top_level_resolution);
        let prerelease = prerelease.combine(top_level_prerelease);
        let config_settings = config_settings.combine(top_level_config_settings);
        let exclude_newer = exclude_newer.combine(top_level_exclude_newer);
        let link_mode = link_mode.combine(top_level_link_mode);
        let compile_bytecode = compile_bytecode.combine(top_level_compile_bytecode);
        let upgrade = upgrade.combine(top_level_upgrade);
        let upgrade_package = upgrade_package.combine(top_level_upgrade_package);
        let reinstall = reinstall.combine(top_level_reinstall);
        let reinstall_package = reinstall_package.combine(top_level_reinstall_package);

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
            config_setting: args
                .config_settings
                .combine(config_settings)
                .unwrap_or_default(),
            python_version: args.python_version.combine(python_version),
            python_platform: args.python_platform.combine(python_platform),
            universal: args.universal.combine(universal).unwrap_or_default(),
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
                args.require_hashes
                    .combine(require_hashes)
                    .unwrap_or_default(),
                args.verify_hashes
                    .combine(verify_hashes)
                    .unwrap_or_default(),
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
            strict: args.strict.combine(strict).unwrap_or_default(),
            upgrade: Upgrade::from_args(
                args.upgrade.combine(upgrade),
                args.upgrade_package
                    .combine(upgrade_package)
                    .into_iter()
                    .flatten()
                    .map(Requirement::from)
                    .collect(),
            ),
            reinstall: Reinstall::from_args(
                args.reinstall.combine(reinstall),
                args.reinstall_package
                    .combine(reinstall_package)
                    .unwrap_or_default(),
            ),
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
        }
    }
}

impl<'a> From<ResolverInstallerSettingsRef<'a>> for ResolverSettingsRef<'a> {
    fn from(settings: ResolverInstallerSettingsRef<'a>) -> Self {
        Self {
            index_locations: settings.index_locations,
            index_strategy: settings.index_strategy,
            keyring_provider: settings.keyring_provider,
            resolution: settings.resolution,
            prerelease: settings.prerelease,
            config_setting: settings.config_setting,
            exclude_newer: settings.exclude_newer,
            link_mode: settings.link_mode,
            upgrade: settings.upgrade,
            build_options: settings.build_options,
        }
    }
}

impl<'a> From<ResolverInstallerSettingsRef<'a>> for InstallerSettingsRef<'a> {
    fn from(settings: ResolverInstallerSettingsRef<'a>) -> Self {
        Self {
            index_locations: settings.index_locations,
            index_strategy: settings.index_strategy,
            keyring_provider: settings.keyring_provider,
            config_setting: settings.config_setting,
            exclude_newer: settings.exclude_newer,
            link_mode: settings.link_mode,
            compile_bytecode: settings.compile_bytecode,
            reinstall: settings.reinstall,
            build_options: settings.build_options,
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
