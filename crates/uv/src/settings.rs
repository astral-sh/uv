use std::env::VarError;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process;
use std::str::FromStr;

use url::Url;

use uv_cache::{CacheArgs, Refresh};
use uv_cli::comma::CommaSeparatedRequirements;
use uv_cli::{
    options::{flag, resolver_installer_options, resolver_options},
    AuthorFrom, BuildArgs, ExportArgs, PublishArgs, PythonDirArgs, ResolverInstallerArgs,
    ToolUpgradeArgs,
};
use uv_cli::{
    AddArgs, ColorChoice, ExternalCommand, GlobalArgs, InitArgs, ListFormat, LockArgs, Maybe,
    PipCheckArgs, PipCompileArgs, PipFreezeArgs, PipInstallArgs, PipListArgs, PipShowArgs,
    PipSyncArgs, PipTreeArgs, PipUninstallArgs, PythonFindArgs, PythonInstallArgs, PythonListArgs,
    PythonListFormat, PythonPinArgs, PythonUninstallArgs, RemoveArgs, RunArgs, SyncArgs,
    ToolDirArgs, ToolInstallArgs, ToolListArgs, ToolRunArgs, ToolUninstallArgs, TreeArgs, VenvArgs,
};
use uv_client::Connectivity;
use uv_configuration::{
    BuildOptions, Concurrency, ConfigSettings, DependencyGroups, DryRun, EditableMode,
    ExportFormat, ExtrasSpecification, HashCheckingMode, IndexStrategy, InstallOptions,
    KeyringProviderType, NoBinary, NoBuild, PreviewMode, ProjectBuildBackend, Reinstall,
    RequiredVersion, SourceStrategy, TargetTriple, TrustedHost, TrustedPublishing, Upgrade,
    VersionControlSystem,
};
use uv_distribution_types::{DependencyMetadata, Index, IndexLocations, IndexUrl};
use uv_install_wheel::LinkMode;
use uv_normalize::{PackageName, PipGroupName};
use uv_pep508::{ExtraName, MarkerTree, RequirementOrigin};
use uv_pypi_types::{Requirement, SupportedEnvironments};
use uv_python::{Prefix, PythonDownloads, PythonPreference, PythonVersion, Target};
use uv_resolver::{
    AnnotationStyle, DependencyMode, ExcludeNewer, ForkStrategy, PrereleaseMode, ResolutionMode,
};
use uv_settings::{
    Combine, FilesystemOptions, Options, PipOptions, PublishOptions, PythonInstallMirrors,
    ResolverInstallerOptions, ResolverOptions,
};
use uv_static::EnvVars;
use uv_warnings::warn_user_once;
use uv_workspace::pyproject::DependencyType;

use crate::commands::ToolRunCommand;
use crate::commands::{pip::operations::Modifications, InitKind, InitProjectKind};

/// The default publish URL.
const PYPI_PUBLISH_URL: &str = "https://upload.pypi.org/legacy/";

/// The resolved global settings to use for any invocation of the CLI.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct GlobalSettings {
    pub(crate) required_version: Option<RequiredVersion>,
    pub(crate) quiet: bool,
    pub(crate) verbose: u8,
    pub(crate) color: ColorChoice,
    pub(crate) network_settings: NetworkSettings,
    pub(crate) concurrency: Concurrency,
    pub(crate) show_settings: bool,
    pub(crate) preview: PreviewMode,
    pub(crate) python_preference: PythonPreference,
    pub(crate) python_downloads: PythonDownloads,
    pub(crate) no_progress: bool,
    pub(crate) installer_metadata: bool,
}

impl GlobalSettings {
    /// Resolve the [`GlobalSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: &GlobalArgs, workspace: Option<&FilesystemOptions>) -> Self {
        let network_settings = NetworkSettings::resolve(args, workspace);
        Self {
            required_version: workspace
                .and_then(|workspace| workspace.globals.required_version.clone()),
            quiet: args.quiet,
            verbose: args.verbose,
            color: if let Some(color_choice) = args.color {
                // If `--color` is passed explicitly, use its value.
                color_choice
            } else if std::env::var_os(EnvVars::NO_COLOR)
                .filter(|v| !v.is_empty())
                .is_some()
            {
                // If the `NO_COLOR` is set, disable color output.
                ColorChoice::Never
            } else if std::env::var_os(EnvVars::FORCE_COLOR)
                .filter(|v| !v.is_empty())
                .is_some()
                || std::env::var_os(EnvVars::CLICOLOR_FORCE)
                    .filter(|v| !v.is_empty())
                    .is_some()
            {
                // If `FORCE_COLOR` or `CLICOLOR_FORCE` is set, always enable color output.
                ColorChoice::Always
            } else {
                ColorChoice::Auto
            },
            network_settings,
            concurrency: Concurrency {
                downloads: env(env::CONCURRENT_DOWNLOADS)
                    .combine(workspace.and_then(|workspace| workspace.globals.concurrent_downloads))
                    .map(NonZeroUsize::get)
                    .unwrap_or(Concurrency::DEFAULT_DOWNLOADS),
                builds: env(env::CONCURRENT_BUILDS)
                    .combine(workspace.and_then(|workspace| workspace.globals.concurrent_builds))
                    .map(NonZeroUsize::get)
                    .unwrap_or_else(Concurrency::threads),
                installs: env(env::CONCURRENT_INSTALLS)
                    .combine(workspace.and_then(|workspace| workspace.globals.concurrent_installs))
                    .map(NonZeroUsize::get)
                    .unwrap_or_else(Concurrency::threads),
            },
            show_settings: args.show_settings,
            preview: PreviewMode::from(
                flag(args.preview, args.no_preview)
                    .combine(workspace.and_then(|workspace| workspace.globals.preview))
                    .unwrap_or(false),
            ),
            python_preference: args
                .python_preference
                .combine(workspace.and_then(|workspace| workspace.globals.python_preference))
                .unwrap_or_default(),
            python_downloads: flag(args.allow_python_downloads, args.no_python_downloads)
                .map(PythonDownloads::from)
                .combine(env(env::UV_PYTHON_DOWNLOADS))
                .combine(workspace.and_then(|workspace| workspace.globals.python_downloads))
                .unwrap_or_default(),
            // Disable the progress bar with `RUST_LOG` to avoid progress fragments interleaving
            // with log messages.
            no_progress: args.no_progress || std::env::var_os(EnvVars::RUST_LOG).is_some(),
            installer_metadata: !args.no_installer_metadata,
        }
    }
}

/// The resolved network settings to use for any invocation of the CLI.
#[derive(Debug, Clone)]
pub(crate) struct NetworkSettings {
    pub(crate) connectivity: Connectivity,
    pub(crate) native_tls: bool,
    pub(crate) allow_insecure_host: Vec<TrustedHost>,
}

impl NetworkSettings {
    pub(crate) fn resolve(args: &GlobalArgs, workspace: Option<&FilesystemOptions>) -> Self {
        let connectivity = if flag(args.offline, args.no_offline)
            .combine(workspace.and_then(|workspace| workspace.globals.offline))
            .unwrap_or(false)
        {
            Connectivity::Offline
        } else {
            Connectivity::Online
        };
        let native_tls = flag(args.native_tls, args.no_native_tls)
            .combine(workspace.and_then(|workspace| workspace.globals.native_tls))
            .unwrap_or(false);
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
        Self {
            connectivity,
            native_tls,
            allow_insecure_host,
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
    pub(crate) path: Option<PathBuf>,
    pub(crate) name: Option<PackageName>,
    pub(crate) package: bool,
    pub(crate) kind: InitKind,
    pub(crate) bare: bool,
    pub(crate) description: Option<String>,
    pub(crate) no_description: bool,
    pub(crate) vcs: Option<VersionControlSystem>,
    pub(crate) build_backend: Option<ProjectBuildBackend>,
    pub(crate) no_readme: bool,
    pub(crate) author_from: Option<AuthorFrom>,
    pub(crate) pin_python: bool,
    pub(crate) no_workspace: bool,
    pub(crate) python: Option<String>,
    pub(crate) install_mirrors: PythonInstallMirrors,
}

impl InitSettings {
    /// Resolve the [`InitSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: InitArgs, filesystem: Option<FilesystemOptions>) -> Self {
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

        let kind = match (app, lib, script) {
            (true, false, false) => InitKind::Project(InitProjectKind::Application),
            (false, true, false) => InitKind::Project(InitProjectKind::Library),
            (false, false, true) => InitKind::Script,
            (false, false, false) => InitKind::default(),
            (_, _, _) => unreachable!("`app`, `lib`, and `script` are mutually exclusive"),
        };

        let package = flag(package || build_backend.is_some(), no_package || r#virtual)
            .unwrap_or(kind.packaged_by_default());

        let install_mirrors = filesystem
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let no_description = no_description || (bare && description.is_none());

        Self {
            path,
            name,
            package,
            kind,
            bare,
            description,
            no_description,
            vcs: vcs.or(bare.then_some(VersionControlSystem::None)),
            build_backend,
            no_readme: no_readme || bare,
            author_from,
            pin_python: flag(pin_python, no_pin_python).unwrap_or(!bare),
            no_workspace,
            python: python.and_then(Maybe::into_option),
            install_mirrors,
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
    pub(crate) dev: DependencyGroups,
    pub(crate) editable: EditableMode,
    pub(crate) modifications: Modifications,
    pub(crate) with: Vec<String>,
    pub(crate) with_editable: Vec<String>,
    pub(crate) with_requirements: Vec<PathBuf>,
    pub(crate) isolated: bool,
    pub(crate) show_resolution: bool,
    pub(crate) all_packages: bool,
    pub(crate) package: Option<PackageName>,
    pub(crate) no_project: bool,
    pub(crate) active: Option<bool>,
    pub(crate) no_sync: bool,
    pub(crate) python: Option<String>,
    pub(crate) install_mirrors: PythonInstallMirrors,
    pub(crate) refresh: Refresh,
    pub(crate) settings: ResolverInstallerSettings,
    pub(crate) env_file: Vec<PathBuf>,
    pub(crate) no_env_file: bool,
    pub(crate) max_recursion_depth: u32,
}

impl RunSettings {
    // Default value for UV_RUN_MAX_RECURSION_DEPTH if unset. This is large
    // enough that it's unlikely a user actually needs this recursion depth,
    // but short enough that we detect recursion quickly enough to avoid OOMing
    // or hanging for a long time.
    const DEFAULT_MAX_RECURSION_DEPTH: u32 = 100;

    /// Resolve the [`RunSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: RunArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let RunArgs {
            extra,
            all_extras,
            no_extra,
            no_all_extras,
            dev,
            no_dev,
            group,
            no_group,
            no_default_groups,
            only_group,
            all_groups,
            module: _,
            only_dev,
            no_editable,
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
            frozen,
            installer,
            build,
            refresh,
            all_packages,
            package,
            no_project,
            python,
            show_resolution,
            env_file,
            no_env_file,
            max_recursion_depth,
        } = args;

        let install_mirrors = filesystem
            .clone()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        Self {
            locked,
            frozen,
            extras: ExtrasSpecification::from_args(
                flag(all_extras, no_all_extras).unwrap_or_default(),
                no_extra,
                extra.unwrap_or_default(),
            ),
            dev: DependencyGroups::from_args(
                dev,
                no_dev,
                only_dev,
                group,
                no_group,
                no_default_groups,
                only_group,
                all_groups,
            ),
            editable: EditableMode::from_args(no_editable),
            modifications: if flag(exact, inexact).unwrap_or(false) {
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
            no_sync,
            active: flag(active, no_active),
            python: python.and_then(Maybe::into_option),
            refresh: Refresh::from(refresh),
            settings: ResolverInstallerSettings::combine(
                resolver_installer_options(installer, build),
                filesystem,
            ),
            env_file,
            no_env_file,
            install_mirrors,
            max_recursion_depth: max_recursion_depth.unwrap_or(Self::DEFAULT_MAX_RECURSION_DEPTH),
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
    pub(crate) with_editable: Vec<String>,
    pub(crate) constraints: Vec<PathBuf>,
    pub(crate) overrides: Vec<PathBuf>,
    pub(crate) isolated: bool,
    pub(crate) show_resolution: bool,
    pub(crate) python: Option<String>,
    pub(crate) install_mirrors: PythonInstallMirrors,
    pub(crate) refresh: Refresh,
    pub(crate) options: ResolverInstallerOptions,
    pub(crate) settings: ResolverInstallerSettings,
}

impl ToolRunSettings {
    /// Resolve the [`ToolRunSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(
        args: ToolRunArgs,
        filesystem: Option<FilesystemOptions>,
        invocation_source: ToolRunCommand,
    ) -> Self {
        let ToolRunArgs {
            command,
            from,
            with,
            with_editable,
            with_requirements,
            constraints,
            overrides,
            isolated,
            show_resolution,
            installer,
            build,
            refresh,
            python,
            generate_shell_completion: _,
        } = args;

        // If `--upgrade` was passed explicitly, warn.
        if installer.upgrade || !installer.upgrade_package.is_empty() {
            if with.is_empty() && with_requirements.is_empty() {
                warn_user_once!("Tools cannot be upgraded via `{invocation_source}`; use `uv tool upgrade --all` to upgrade all installed tools, or `{invocation_source} package@latest` to run the latest version of a tool.");
            } else {
                warn_user_once!("Tools cannot be upgraded via `{invocation_source}`; use `uv tool upgrade --all` to upgrade all installed tools, `{invocation_source} package@latest` to run the latest version of a tool, or `{invocation_source} --refresh package` to upgrade any `--with` dependencies.");
            }
        }

        // If `--reinstall` was passed explicitly, warn.
        if installer.reinstall || !installer.reinstall_package.is_empty() {
            if with.is_empty() && with_requirements.is_empty() {
                warn_user_once!("Tools cannot be reinstalled via `{invocation_source}`; use `uv tool upgrade --all --reinstall` to reinstall all installed tools, or `{invocation_source} package@latest` to run the latest version of a tool.");
            } else {
                warn_user_once!("Tools cannot be reinstalled via `{invocation_source}`; use `uv tool upgrade --all --reinstall` to reinstall all installed tools, `{invocation_source} package@latest` to run the latest version of a tool, or `{invocation_source} --refresh package` to reinstall any `--with` dependencies.");
            }
        }

        let options = resolver_installer_options(installer, build).combine(
            filesystem
                .clone()
                .map(FilesystemOptions::into_options)
                .map(|options| options.top_level)
                .unwrap_or_default(),
        );

        let install_mirrors = filesystem
            .map(FilesystemOptions::into_options)
            .map(|options| options.install_mirrors)
            .unwrap_or_default();

        let settings = ResolverInstallerSettings::from(options.clone());

        Self {
            command,
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
            isolated,
            show_resolution,
            python: python.and_then(Maybe::into_option),
            refresh: Refresh::from(refresh),
            settings,
            options,
            install_mirrors,
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
    pub(crate) with_editable: Vec<String>,
    pub(crate) constraints: Vec<PathBuf>,
    pub(crate) overrides: Vec<PathBuf>,
    pub(crate) python: Option<String>,
    pub(crate) refresh: Refresh,
    pub(crate) options: ResolverInstallerOptions,
    pub(crate) settings: ResolverInstallerSettings,
    pub(crate) force: bool,
    pub(crate) editable: bool,
    pub(crate) install_mirrors: PythonInstallMirrors,
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
            with_editable,
            with_requirements,
            constraints,
            overrides,
            installer,
            force,
            build,
            refresh,
            python,
        } = args;

        let options = resolver_installer_options(installer, build).combine(
            filesystem
                .clone()
                .map(FilesystemOptions::into_options)
                .map(|options| options.top_level)
                .unwrap_or_default(),
        );

        let install_mirrors = filesystem
            .map(FilesystemOptions::into_options)
            .map(|options| options.install_mirrors)
            .unwrap_or_default();

        let settings = ResolverInstallerSettings::from(options.clone());

        Self {
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
            constraints: constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            overrides: overrides
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            python: python.and_then(Maybe::into_option),
            force,
            editable,
            refresh: Refresh::from(refresh),
            options,
            settings,
            install_mirrors,
        }
    }
}

/// The resolved settings to use for a `tool upgrade` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct ToolUpgradeSettings {
    pub(crate) names: Vec<String>,
    pub(crate) python: Option<String>,
    pub(crate) install_mirrors: PythonInstallMirrors,
    pub(crate) args: ResolverInstallerOptions,
    pub(crate) filesystem: ResolverInstallerOptions,
}
impl ToolUpgradeSettings {
    /// Resolve the [`ToolUpgradeSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: ToolUpgradeArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let ToolUpgradeArgs {
            name,
            python,
            upgrade,
            upgrade_package,
            index_args,
            all,
            reinstall,
            no_reinstall,
            reinstall_package,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease,
            pre,
            fork_strategy,
            config_setting,
            no_build_isolation,
            no_build_isolation_package,
            build_isolation,
            exclude_newer,
            link_mode,
            compile_bytecode,
            no_compile_bytecode,
            no_sources,
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
            reinstall,
            no_reinstall,
            reinstall_package,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease,
            pre,
            fork_strategy,
            config_setting,
            no_build_isolation,
            no_build_isolation_package,
            build_isolation,
            exclude_newer,
            link_mode,
            compile_bytecode,
            no_compile_bytecode,
            no_sources,
        };

        let args = resolver_installer_options(installer, build);
        let filesystem = filesystem.map(FilesystemOptions::into_options);
        let install_mirrors = filesystem
            .clone()
            .map(|options| options.install_mirrors)
            .unwrap_or_default();
        let top_level = filesystem
            .map(|options| options.top_level)
            .unwrap_or_default();

        Self {
            names: if all { vec![] } else { name },
            python: python.and_then(Maybe::into_option),
            args,
            filesystem: top_level,
            install_mirrors,
        }
    }
}

/// The resolved settings to use for a `tool list` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct ToolListSettings {
    pub(crate) show_paths: bool,
    pub(crate) show_version_specifiers: bool,
}

impl ToolListSettings {
    /// Resolve the [`ToolListSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: ToolListArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let ToolListArgs {
            show_paths,
            show_version_specifiers,
            python_preference: _,
            no_python_downloads: _,
        } = args;

        Self {
            show_paths,
            show_version_specifiers,
        }
    }
}

/// The resolved settings to use for a `tool uninstall` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct ToolUninstallSettings {
    pub(crate) name: Vec<PackageName>,
}

impl ToolUninstallSettings {
    /// Resolve the [`ToolUninstallSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: ToolUninstallArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let ToolUninstallArgs { name, all } = args;

        Self {
            name: if all { vec![] } else { name },
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
    /// Only list version downloads.
    Downloads,
    /// Only list installed versions.
    Installed,
}

/// The resolved settings to use for a `tool run` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PythonListSettings {
    pub(crate) kinds: PythonListKinds,
    pub(crate) all_platforms: bool,
    pub(crate) all_arches: bool,
    pub(crate) all_versions: bool,
    pub(crate) show_urls: bool,
    pub(crate) output_format: PythonListFormat,
}

impl PythonListSettings {
    /// Resolve the [`PythonListSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: PythonListArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let PythonListArgs {
            all_versions,
            all_platforms,
            all_arches,
            only_installed,
            only_downloads,
            show_urls,
            output_format,
        } = args;

        let kinds = if only_installed {
            PythonListKinds::Installed
        } else if only_downloads {
            PythonListKinds::Downloads
        } else {
            PythonListKinds::default()
        };

        Self {
            kinds,
            all_platforms,
            all_arches,
            all_versions,
            show_urls,
            output_format,
        }
    }
}

/// The resolved settings to use for a `python dir` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PythonDirSettings {
    pub(crate) bin: bool,
}

impl PythonDirSettings {
    /// Resolve the [`PythonDirSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: PythonDirArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let PythonDirArgs { bin } = args;

        Self { bin }
    }
}

/// The resolved settings to use for a `python install` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PythonInstallSettings {
    pub(crate) install_dir: Option<PathBuf>,
    pub(crate) targets: Vec<String>,
    pub(crate) reinstall: bool,
    pub(crate) force: bool,
    pub(crate) python_install_mirror: Option<String>,
    pub(crate) pypy_install_mirror: Option<String>,
    pub(crate) default: bool,
}

impl PythonInstallSettings {
    /// Resolve the [`PythonInstallSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: PythonInstallArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let options = filesystem.map(FilesystemOptions::into_options);
        let (python_mirror, pypy_mirror) = match options {
            Some(options) => (
                options.install_mirrors.python_install_mirror,
                options.install_mirrors.pypy_install_mirror,
            ),
            None => (None, None),
        };
        let python_mirror = args.mirror.or(python_mirror);
        let pypy_mirror = args.pypy_mirror.or(pypy_mirror);

        let PythonInstallArgs {
            install_dir,
            targets,
            reinstall,
            force,
            mirror: _,
            pypy_mirror: _,
            default,
        } = args;

        Self {
            install_dir,
            targets,
            reinstall,
            force,
            python_install_mirror: python_mirror,
            pypy_install_mirror: pypy_mirror,
            default,
        }
    }
}

/// The resolved settings to use for a `python uninstall` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PythonUninstallSettings {
    pub(crate) install_dir: Option<PathBuf>,
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
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PythonFindSettings {
    pub(crate) request: Option<String>,
    pub(crate) no_project: bool,
    pub(crate) system: bool,
}

impl PythonFindSettings {
    /// Resolve the [`PythonFindSettings`] from the CLI and workspace configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: PythonFindArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let PythonFindArgs {
            request,
            no_project,
            system,
            no_system,
        } = args;

        Self {
            request,
            no_project,
            system: flag(system, no_system).unwrap_or_default(),
        }
    }
}

/// The resolved settings to use for a `python pin` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PythonPinSettings {
    pub(crate) request: Option<String>,
    pub(crate) resolved: bool,
    pub(crate) no_project: bool,
    pub(crate) global: bool,
}

impl PythonPinSettings {
    /// Resolve the [`PythonPinSettings`] from the CLI and workspace configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: PythonPinArgs, _filesystem: Option<FilesystemOptions>) -> Self {
        let PythonPinArgs {
            request,
            no_resolved,
            resolved,
            no_project,
            global,
        } = args;

        Self {
            request,
            resolved: flag(resolved, no_resolved).unwrap_or(false),
            no_project,
            global,
        }
    }
}

/// The resolved settings to use for a `sync` invocation.
#[allow(clippy::struct_excessive_bools, dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct SyncSettings {
    pub(crate) locked: bool,
    pub(crate) frozen: bool,
    pub(crate) dry_run: DryRun,
    pub(crate) script: Option<PathBuf>,
    pub(crate) active: Option<bool>,
    pub(crate) extras: ExtrasSpecification,
    pub(crate) dev: DependencyGroups,
    pub(crate) editable: EditableMode,
    pub(crate) install_options: InstallOptions,
    pub(crate) modifications: Modifications,
    pub(crate) all_packages: bool,
    pub(crate) package: Option<PackageName>,
    pub(crate) python: Option<String>,
    pub(crate) install_mirrors: PythonInstallMirrors,
    pub(crate) refresh: Refresh,
    pub(crate) settings: ResolverInstallerSettings,
}

impl SyncSettings {
    /// Resolve the [`SyncSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: SyncArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let SyncArgs {
            extra,
            all_extras,
            no_extra,
            no_all_extras,
            dev,
            no_dev,
            only_dev,
            group,
            no_group,
            no_default_groups,
            only_group,
            all_groups,
            no_editable,
            inexact,
            exact,
            no_install_project,
            no_install_workspace,
            no_install_package,
            locked,
            frozen,
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
        } = args;
        let install_mirrors = filesystem
            .clone()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let settings = ResolverInstallerSettings::combine(
            resolver_installer_options(installer, build),
            filesystem,
        );

        Self {
            locked,
            frozen,
            dry_run: DryRun::from_args(dry_run),
            script,
            active: flag(active, no_active),
            extras: ExtrasSpecification::from_args(
                flag(all_extras, no_all_extras).unwrap_or_default(),
                no_extra,
                extra.unwrap_or_default(),
            ),
            dev: DependencyGroups::from_args(
                dev,
                no_dev,
                only_dev,
                group,
                no_group,
                no_default_groups,
                only_group,
                all_groups,
            ),
            editable: EditableMode::from_args(no_editable),
            install_options: InstallOptions::new(
                no_install_project,
                no_install_workspace,
                no_install_package,
            ),
            modifications: if flag(exact, inexact).unwrap_or(true) {
                Modifications::Exact
            } else {
                Modifications::Sufficient
            },
            all_packages,
            package,
            python: python.and_then(Maybe::into_option),
            refresh: Refresh::from(refresh),
            settings,
            install_mirrors,
        }
    }
}

/// The resolved settings to use for a `lock` invocation.
#[allow(clippy::struct_excessive_bools, dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct LockSettings {
    pub(crate) locked: bool,
    pub(crate) frozen: bool,
    pub(crate) dry_run: DryRun,
    pub(crate) script: Option<PathBuf>,
    pub(crate) python: Option<String>,
    pub(crate) install_mirrors: PythonInstallMirrors,
    pub(crate) refresh: Refresh,
    pub(crate) settings: ResolverSettings,
}

impl LockSettings {
    /// Resolve the [`LockSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: LockArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let LockArgs {
            check,
            check_exists,
            dry_run,
            script,
            resolver,
            build,
            refresh,
            python,
        } = args;

        let install_mirrors = filesystem
            .clone()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        Self {
            locked: check,
            frozen: check_exists,
            dry_run: DryRun::from_args(dry_run),
            script,
            python: python.and_then(Maybe::into_option),
            refresh: Refresh::from(refresh),
            settings: ResolverSettings::combine(resolver_options(resolver, build), filesystem),
            install_mirrors,
        }
    }
}

/// The resolved settings to use for a `add` invocation.
#[allow(clippy::struct_excessive_bools, dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct AddSettings {
    pub(crate) locked: bool,
    pub(crate) frozen: bool,
    pub(crate) active: Option<bool>,
    pub(crate) no_sync: bool,
    pub(crate) packages: Vec<String>,
    pub(crate) requirements: Vec<PathBuf>,
    pub(crate) constraints: Vec<PathBuf>,
    pub(crate) marker: Option<MarkerTree>,
    pub(crate) dependency_type: DependencyType,
    pub(crate) editable: Option<bool>,
    pub(crate) extras: Vec<ExtraName>,
    pub(crate) raw_sources: bool,
    pub(crate) rev: Option<String>,
    pub(crate) tag: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) package: Option<PackageName>,
    pub(crate) script: Option<PathBuf>,
    pub(crate) python: Option<String>,
    pub(crate) install_mirrors: PythonInstallMirrors,
    pub(crate) refresh: Refresh,
    pub(crate) indexes: Vec<Index>,
    pub(crate) settings: ResolverInstallerSettings,
}

impl AddSettings {
    /// Resolve the [`AddSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: AddArgs, filesystem: Option<FilesystemOptions>) -> Self {
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
            extra,
            raw_sources,
            rev,
            tag,
            branch,
            no_sync,
            locked,
            frozen,
            active,
            no_active,
            installer,
            build,
            refresh,
            package,
            script,
            python,
        } = args;

        let dependency_type = if let Some(extra) = optional {
            DependencyType::Optional(extra)
        } else if let Some(group) = group {
            DependencyType::Group(group)
        } else if dev {
            DependencyType::Dev
        } else {
            DependencyType::Production
        };

        // Track the `--index` and `--default-index` arguments from the command-line.
        let indexes = installer
            .index_args
            .default_index
            .clone()
            .and_then(Maybe::into_option)
            .into_iter()
            .chain(
                installer
                    .index_args
                    .index
                    .clone()
                    .into_iter()
                    .flatten()
                    .filter_map(Maybe::into_option),
            )
            .collect::<Vec<_>>();

        // If the user passed an `--index-url` or `--extra-index-url`, warn.
        if installer
            .index_args
            .index_url
            .as_ref()
            .is_some_and(Maybe::is_some)
        {
            if script.is_some() {
                warn_user_once!("Indexes specified via `--index-url` will not be persisted to the script; use `--default-index` instead.");
            } else {
                warn_user_once!("Indexes specified via `--index-url` will not be persisted to the `pyproject.toml` file; use `--default-index` instead.");
            }
        }

        if installer
            .index_args
            .extra_index_url
            .as_ref()
            .is_some_and(|extra_index_url| extra_index_url.iter().any(Maybe::is_some))
        {
            if script.is_some() {
                warn_user_once!("Indexes specified via `--extra-index-url` will not be persisted to the script; use `--index` instead.");
            } else {
                warn_user_once!("Indexes specified via `--extra-index-url` will not be persisted to the `pyproject.toml` file; use `--index` instead.");
            }
        }

        let install_mirrors = filesystem
            .clone()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        Self {
            locked,
            frozen,
            active: flag(active, no_active),
            no_sync,
            packages,
            requirements,
            constraints: constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            marker,
            dependency_type,
            raw_sources,
            rev,
            tag,
            branch,
            package,
            script,
            python: python.and_then(Maybe::into_option),
            editable: flag(editable, no_editable),
            extras: extra.unwrap_or_default(),
            refresh: Refresh::from(refresh),
            indexes,
            settings: ResolverInstallerSettings::combine(
                resolver_installer_options(installer, build),
                filesystem,
            ),
            install_mirrors,
        }
    }
}

/// The resolved settings to use for a `remove` invocation.
#[allow(clippy::struct_excessive_bools, dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct RemoveSettings {
    pub(crate) locked: bool,
    pub(crate) frozen: bool,
    pub(crate) active: Option<bool>,
    pub(crate) no_sync: bool,
    pub(crate) packages: Vec<PackageName>,
    pub(crate) dependency_type: DependencyType,
    pub(crate) package: Option<PackageName>,
    pub(crate) script: Option<PathBuf>,
    pub(crate) python: Option<String>,
    pub(crate) install_mirrors: PythonInstallMirrors,
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
            packages,
            group,
            no_sync,
            locked,
            frozen,
            active,
            no_active,
            installer,
            build,
            refresh,
            package,
            script,
            python,
        } = args;

        let dependency_type = if let Some(extra) = optional {
            DependencyType::Optional(extra)
        } else if let Some(group) = group {
            DependencyType::Group(group)
        } else if dev {
            DependencyType::Dev
        } else {
            DependencyType::Production
        };

        let install_mirrors = filesystem
            .clone()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        let packages = packages
            .into_iter()
            .map(|requirement| requirement.name)
            .collect();

        Self {
            locked,
            frozen,
            active: flag(active, no_active),
            no_sync,
            packages,
            dependency_type,
            package,
            script,
            python: python.and_then(Maybe::into_option),
            refresh: Refresh::from(refresh),
            settings: ResolverInstallerSettings::combine(
                resolver_installer_options(installer, build),
                filesystem,
            ),
            install_mirrors,
        }
    }
}

/// The resolved settings to use for a `tree` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct TreeSettings {
    pub(crate) dev: DependencyGroups,
    pub(crate) locked: bool,
    pub(crate) frozen: bool,
    pub(crate) universal: bool,
    pub(crate) depth: u8,
    pub(crate) prune: Vec<PackageName>,
    pub(crate) package: Vec<PackageName>,
    pub(crate) no_dedupe: bool,
    pub(crate) invert: bool,
    pub(crate) outdated: bool,
    #[allow(dead_code)]
    pub(crate) script: Option<PathBuf>,
    pub(crate) python_version: Option<PythonVersion>,
    pub(crate) python_platform: Option<TargetTriple>,
    pub(crate) python: Option<String>,
    pub(crate) install_mirrors: PythonInstallMirrors,
    pub(crate) resolver: ResolverSettings,
}

impl TreeSettings {
    /// Resolve the [`TreeSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: TreeArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let TreeArgs {
            tree,
            universal,
            dev,
            only_dev,
            no_dev,
            group,
            no_group,
            no_default_groups,
            only_group,
            all_groups,
            locked,
            frozen,
            build,
            resolver,
            script,
            python_version,
            python_platform,
            python,
        } = args;
        let install_mirrors = filesystem
            .clone()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        Self {
            dev: DependencyGroups::from_args(
                dev,
                no_dev,
                only_dev,
                group,
                no_group,
                no_default_groups,
                only_group,
                all_groups,
            ),
            locked,
            frozen,
            universal,
            depth: tree.depth,
            prune: tree.prune,
            package: tree.package,
            no_dedupe: tree.no_dedupe,
            invert: tree.invert,
            outdated: tree.outdated,
            script,
            python_version,
            python_platform,
            python: python.and_then(Maybe::into_option),
            resolver: ResolverSettings::combine(resolver_options(resolver, build), filesystem),
            install_mirrors,
        }
    }
}

/// The resolved settings to use for an `export` invocation.
#[allow(clippy::struct_excessive_bools, dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct ExportSettings {
    pub(crate) format: ExportFormat,
    pub(crate) all_packages: bool,
    pub(crate) package: Option<PackageName>,
    pub(crate) prune: Vec<PackageName>,
    pub(crate) extras: ExtrasSpecification,
    pub(crate) dev: DependencyGroups,
    pub(crate) editable: EditableMode,
    pub(crate) hashes: bool,
    pub(crate) install_options: InstallOptions,
    pub(crate) output_file: Option<PathBuf>,
    pub(crate) locked: bool,
    pub(crate) frozen: bool,
    pub(crate) include_header: bool,
    pub(crate) script: Option<PathBuf>,
    pub(crate) python: Option<String>,
    pub(crate) install_mirrors: PythonInstallMirrors,
    pub(crate) refresh: Refresh,
    pub(crate) settings: ResolverSettings,
}

impl ExportSettings {
    /// Resolve the [`ExportSettings`] from the CLI and filesystem configuration.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn resolve(args: ExportArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let ExportArgs {
            format,
            all_packages,
            package,
            prune,
            extra,
            all_extras,
            no_extra,
            no_all_extras,
            dev,
            no_dev,
            only_dev,
            group,
            no_group,
            no_default_groups,
            only_group,
            all_groups,
            header,
            no_header,
            no_editable,
            hashes,
            no_hashes,
            output_file,
            no_emit_project,
            no_emit_workspace,
            no_emit_package,
            locked,
            frozen,
            resolver,
            build,
            refresh,
            script,
            python,
        } = args;
        let install_mirrors = filesystem
            .clone()
            .map(|fs| fs.install_mirrors.clone())
            .unwrap_or_default();

        Self {
            format,
            all_packages,
            package,
            prune,
            extras: ExtrasSpecification::from_args(
                flag(all_extras, no_all_extras).unwrap_or_default(),
                no_extra,
                extra.unwrap_or_default(),
            ),
            dev: DependencyGroups::from_args(
                dev,
                no_dev,
                only_dev,
                group,
                no_group,
                no_default_groups,
                only_group,
                all_groups,
            ),
            editable: EditableMode::from_args(no_editable),
            hashes: flag(hashes, no_hashes).unwrap_or(true),
            install_options: InstallOptions::new(
                no_emit_project,
                no_emit_workspace,
                no_emit_package,
            ),
            output_file,
            locked,
            frozen,
            include_header: flag(header, no_header).unwrap_or(true),
            script,
            python: python.and_then(Maybe::into_option),
            refresh: Refresh::from(refresh),
            settings: ResolverSettings::combine(resolver_options(resolver, build), filesystem),
            install_mirrors,
        }
    }
}

/// The resolved settings to use for a `pip compile` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PipCompileSettings {
    pub(crate) src_file: Vec<PathBuf>,
    pub(crate) constraints: Vec<PathBuf>,
    pub(crate) overrides: Vec<PathBuf>,
    pub(crate) build_constraints: Vec<PathBuf>,
    pub(crate) constraints_from_workspace: Vec<Requirement>,
    pub(crate) overrides_from_workspace: Vec<Requirement>,
    pub(crate) build_constraints_from_workspace: Vec<Requirement>,
    pub(crate) environments: SupportedEnvironments,
    pub(crate) refresh: Refresh,
    pub(crate) settings: PipSettings,
}

impl PipCompileSettings {
    /// Resolve the [`PipCompileSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: PipCompileArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let PipCompileArgs {
            src_file,
            constraints,
            overrides,
            extra,
            all_extras,
            no_all_extras,
            build_constraints,
            refresh,
            no_deps,
            deps,
            group,
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

        let build_constraints_from_workspace = if let Some(configuration) = &filesystem {
            configuration
                .build_constraint_dependencies
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

        let environments = if let Some(configuration) = &filesystem {
            configuration.environments.clone().unwrap_or_default()
        } else {
            SupportedEnvironments::default()
        };

        Self {
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
            constraints_from_workspace,
            overrides_from_workspace,
            build_constraints_from_workspace,
            environments,
            refresh: Refresh::from(refresh),
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system),
                    no_build: flag(no_build, build),
                    no_binary,
                    only_binary,
                    extra,
                    all_extras: flag(all_extras, no_all_extras),
                    no_deps: flag(no_deps, deps),
                    group: Some(group),
                    output_file,
                    no_strip_extras: flag(no_strip_extras, strip_extras),
                    no_strip_markers: flag(no_strip_markers, strip_markers),
                    no_annotate: flag(no_annotate, annotate),
                    no_header: flag(no_header, header),
                    custom_compile_command,
                    generate_hashes: flag(generate_hashes, no_generate_hashes),
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
    pub(crate) constraints: Vec<PathBuf>,
    pub(crate) build_constraints: Vec<PathBuf>,
    pub(crate) dry_run: DryRun,
    pub(crate) refresh: Refresh,
    pub(crate) settings: PipSettings,
}

impl PipSyncSettings {
    /// Resolve the [`PipSyncSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: Box<PipSyncArgs>, filesystem: Option<FilesystemOptions>) -> Self {
        let PipSyncArgs {
            src_file,
            constraints,
            build_constraints,
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
            constraints: constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            build_constraints: build_constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            dry_run: DryRun::from_args(dry_run),
            refresh: Refresh::from(refresh),
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
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
                    python_version,
                    python_platform,
                    strict: flag(strict, no_strict),
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
    pub(crate) requirements: Vec<PathBuf>,
    pub(crate) editables: Vec<String>,
    pub(crate) constraints: Vec<PathBuf>,
    pub(crate) overrides: Vec<PathBuf>,
    pub(crate) build_constraints: Vec<PathBuf>,
    pub(crate) dry_run: DryRun,
    pub(crate) constraints_from_workspace: Vec<Requirement>,
    pub(crate) overrides_from_workspace: Vec<Requirement>,
    pub(crate) build_constraints_from_workspace: Vec<Requirement>,
    pub(crate) modifications: Modifications,
    pub(crate) refresh: Refresh,
    pub(crate) settings: PipSettings,
}

impl PipInstallSettings {
    /// Resolve the [`PipInstallSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: PipInstallArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let PipInstallArgs {
            package,
            requirements,
            editable,
            constraints,
            overrides,
            build_constraints,
            extra,
            all_extras,
            no_all_extras,
            installer,
            refresh,
            no_deps,
            deps,
            group,
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

        let build_constraints_from_workspace = if let Some(configuration) = &filesystem {
            configuration
                .build_constraint_dependencies
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
            build_constraints: build_constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            dry_run: DryRun::from_args(dry_run),
            constraints_from_workspace,
            overrides_from_workspace,
            build_constraints_from_workspace,
            modifications: if flag(exact, inexact).unwrap_or(false) {
                Modifications::Exact
            } else {
                Modifications::Sufficient
            },
            refresh: Refresh::from(refresh),
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system),
                    break_system_packages: flag(break_system_packages, no_break_system_packages),
                    target,
                    prefix,
                    no_build: flag(no_build, build),
                    no_binary,
                    only_binary,
                    strict: flag(strict, no_strict),
                    extra,
                    all_extras: flag(all_extras, no_all_extras),
                    group: Some(group),
                    no_deps: flag(no_deps, deps),
                    python_version,
                    python_platform,
                    require_hashes: flag(require_hashes, no_require_hashes),
                    verify_hashes: flag(verify_hashes, no_verify_hashes),
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
    pub(crate) requirements: Vec<PathBuf>,
    pub(crate) dry_run: DryRun,
    pub(crate) settings: PipSettings,
}

impl PipUninstallSettings {
    /// Resolve the [`PipUninstallSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: PipUninstallArgs, filesystem: Option<FilesystemOptions>) -> Self {
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

        Self {
            package,
            requirements,
            dry_run: DryRun::from_args(dry_run),
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
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
    pub(crate) paths: Option<Vec<PathBuf>>,
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
            paths,
            system,
            no_system,
            compat_args: _,
        } = args;

        Self {
            exclude_editable,
            paths,
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
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
    pub(crate) outdated: bool,
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
            outdated,
            no_outdated,
            strict,
            no_strict,
            fetch,
            python,
            system,
            no_system,
            compat_args: _,
        } = args;

        Self {
            editable: flag(editable, exclude_editable),
            exclude,
            format,
            outdated: flag(outdated, no_outdated).unwrap_or(false),
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system),
                    strict: flag(strict, no_strict),
                    ..PipOptions::from(fetch)
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
    pub(crate) files: bool,
    pub(crate) settings: PipSettings,
}

impl PipShowSettings {
    /// Resolve the [`PipShowSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: PipShowArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let PipShowArgs {
            package,
            strict,
            no_strict,
            files,
            python,
            system,
            no_system,
            compat_args: _,
        } = args;

        Self {
            package,
            files,
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
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
    pub(crate) show_version_specifiers: bool,
    pub(crate) depth: u8,
    pub(crate) prune: Vec<PackageName>,
    pub(crate) package: Vec<PackageName>,
    pub(crate) no_dedupe: bool,
    pub(crate) invert: bool,
    pub(crate) outdated: bool,
    pub(crate) settings: PipSettings,
}

impl PipTreeSettings {
    /// Resolve the [`PipTreeSettings`] from the CLI and workspace configuration.
    pub(crate) fn resolve(args: PipTreeArgs, filesystem: Option<FilesystemOptions>) -> Self {
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

        Self {
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
                    system: flag(system, no_system),
                    strict: flag(strict, no_strict),
                    ..PipOptions::from(fetch)
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
                    python: python.and_then(Maybe::into_option),
                    system: flag(system, no_system),
                    ..PipOptions::default()
                },
                filesystem,
            ),
        }
    }
}

/// The resolved settings to use for a `build` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct BuildSettings {
    pub(crate) src: Option<PathBuf>,
    pub(crate) package: Option<PackageName>,
    pub(crate) all_packages: bool,
    pub(crate) out_dir: Option<PathBuf>,
    pub(crate) sdist: bool,
    pub(crate) wheel: bool,
    pub(crate) list: bool,
    pub(crate) build_logs: bool,
    pub(crate) force_pep517: bool,
    pub(crate) build_constraints: Vec<PathBuf>,
    pub(crate) hash_checking: Option<HashCheckingMode>,
    pub(crate) python: Option<String>,
    pub(crate) install_mirrors: PythonInstallMirrors,
    pub(crate) refresh: Refresh,
    pub(crate) settings: ResolverSettings,
}

impl BuildSettings {
    /// Resolve the [`BuildSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: BuildArgs, filesystem: Option<FilesystemOptions>) -> Self {
        let BuildArgs {
            src,
            out_dir,
            package,
            all_packages,
            sdist,
            wheel,
            list,
            force_pep517,
            build_constraints,
            require_hashes,
            no_require_hashes,
            verify_hashes,
            no_verify_hashes,
            build_logs,
            no_build_logs,
            python,
            build,
            refresh,
            resolver,
        } = args;

        let install_mirrors = match &filesystem {
            Some(fs) => fs.install_mirrors.clone(),
            None => PythonInstallMirrors::default(),
        };

        Self {
            src,
            package,
            all_packages,
            out_dir,
            sdist,
            wheel,
            list,
            build_logs: flag(build_logs, no_build_logs).unwrap_or(true),
            build_constraints: build_constraints
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect(),
            force_pep517,
            hash_checking: HashCheckingMode::from_args(
                flag(require_hashes, no_require_hashes),
                flag(verify_hashes, no_verify_hashes),
            ),
            python: python.and_then(Maybe::into_option),
            refresh: Refresh::from(refresh),
            settings: ResolverSettings::combine(resolver_options(resolver, build), filesystem),
            install_mirrors,
        }
    }
}

/// The resolved settings to use for a `venv` invocation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct VenvSettings {
    pub(crate) seed: bool,
    pub(crate) allow_existing: bool,
    pub(crate) path: Option<PathBuf>,
    pub(crate) prompt: Option<String>,
    pub(crate) system_site_packages: bool,
    pub(crate) relocatable: bool,
    pub(crate) no_project: bool,
    pub(crate) refresh: Refresh,
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
            path,
            prompt,
            system_site_packages,
            relocatable,
            index_args,
            index_strategy,
            keyring_provider,
            exclude_newer,
            no_project,
            link_mode,
            refresh,
            compat_args: _,
        } = args;

        Self {
            seed,
            allow_existing,
            path,
            prompt,
            system_site_packages,
            no_project,
            relocatable,
            refresh: Refresh::from(refresh),
            settings: PipSettings::combine(
                PipOptions {
                    python: python.and_then(Maybe::into_option),
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
    pub(crate) dependency_metadata: &'a DependencyMetadata,
    pub(crate) config_setting: &'a ConfigSettings,
    pub(crate) no_build_isolation: bool,
    pub(crate) no_build_isolation_package: &'a [PackageName],
    pub(crate) exclude_newer: Option<ExcludeNewer>,
    pub(crate) link_mode: LinkMode,
    pub(crate) compile_bytecode: bool,
    pub(crate) reinstall: &'a Reinstall,
    pub(crate) build_options: &'a BuildOptions,
    pub(crate) sources: SourceStrategy,
}

/// The resolved settings to use for an invocation of the uv CLI when resolving dependencies.
///
/// Combines the `[tool.uv]` persistent configuration with the command-line arguments
/// ([`ResolverArgs`], represented as [`ResolverOptions`]).
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
pub(crate) struct ResolverSettings {
    pub(crate) build_options: BuildOptions,
    pub(crate) config_setting: ConfigSettings,
    pub(crate) dependency_metadata: DependencyMetadata,
    pub(crate) exclude_newer: Option<ExcludeNewer>,
    pub(crate) fork_strategy: ForkStrategy,
    pub(crate) index_locations: IndexLocations,
    pub(crate) index_strategy: IndexStrategy,
    pub(crate) keyring_provider: KeyringProviderType,
    pub(crate) link_mode: LinkMode,
    pub(crate) no_build_isolation: bool,
    pub(crate) no_build_isolation_package: Vec<PackageName>,
    pub(crate) prerelease: PrereleaseMode,
    pub(crate) resolution: ResolutionMode,
    pub(crate) sources: SourceStrategy,
    pub(crate) upgrade: Upgrade,
}

impl ResolverSettings {
    /// Resolve the [`ResolverSettings`] from the CLI and filesystem configuration.
    pub(crate) fn combine(args: ResolverOptions, filesystem: Option<FilesystemOptions>) -> Self {
        let options = args.combine(ResolverOptions::from(
            filesystem
                .map(FilesystemOptions::into_options)
                .map(|options| options.top_level)
                .unwrap_or_default(),
        ));

        Self::from(options)
    }
}

impl From<ResolverOptions> for ResolverSettings {
    fn from(value: ResolverOptions) -> Self {
        let index_locations = IndexLocations::new(
            value
                .index
                .into_iter()
                .flatten()
                .chain(value.extra_index_url.into_iter().flatten().map(Index::from))
                .chain(value.index_url.into_iter().map(Index::from))
                .collect(),
            value
                .find_links
                .into_iter()
                .flatten()
                .map(Index::from)
                .collect(),
            value.no_index.unwrap_or_default(),
        );
        Self {
            index_locations,
            resolution: value.resolution.unwrap_or_default(),
            prerelease: value.prerelease.unwrap_or_default(),
            fork_strategy: value.fork_strategy.unwrap_or_default(),
            dependency_metadata: DependencyMetadata::from_entries(
                value.dependency_metadata.into_iter().flatten(),
            ),
            index_strategy: value.index_strategy.unwrap_or_default(),
            keyring_provider: value.keyring_provider.unwrap_or_default(),
            config_setting: value.config_settings.unwrap_or_default(),
            no_build_isolation: value.no_build_isolation.unwrap_or_default(),
            no_build_isolation_package: value.no_build_isolation_package.unwrap_or_default(),
            exclude_newer: value.exclude_newer,
            link_mode: value.link_mode.unwrap_or_default(),
            sources: SourceStrategy::from_args(value.no_sources.unwrap_or_default()),
            upgrade: Upgrade::from_args(
                value.upgrade,
                value
                    .upgrade_package
                    .into_iter()
                    .flatten()
                    .map(Requirement::from)
                    .collect(),
            ),
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
/// Analogous to the settings contained in the `[tool.uv]` table, combined with [`ResolverInstallerArgs`].
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
pub(crate) struct ResolverInstallerSettings {
    pub(crate) resolver: ResolverSettings,
    pub(crate) compile_bytecode: bool,
    pub(crate) reinstall: Reinstall,
}

impl ResolverInstallerSettings {
    /// Reconcile the [`ResolverInstallerSettings`] from the CLI and filesystem configuration.
    pub(crate) fn combine(
        args: ResolverInstallerOptions,
        filesystem: Option<FilesystemOptions>,
    ) -> Self {
        let options = args.combine(
            filesystem
                .map(FilesystemOptions::into_options)
                .map(|options| options.top_level)
                .unwrap_or_default(),
        );

        Self::from(options)
    }
}

impl From<ResolverInstallerOptions> for ResolverInstallerSettings {
    fn from(value: ResolverInstallerOptions) -> Self {
        let index_locations = IndexLocations::new(
            value
                .index
                .into_iter()
                .flatten()
                .chain(value.extra_index_url.into_iter().flatten().map(Index::from))
                .chain(value.index_url.into_iter().map(Index::from))
                .collect(),
            value
                .find_links
                .into_iter()
                .flatten()
                .map(Index::from)
                .collect(),
            value.no_index.unwrap_or_default(),
        );
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
                dependency_metadata: DependencyMetadata::from_entries(
                    value.dependency_metadata.into_iter().flatten(),
                ),
                exclude_newer: value.exclude_newer,
                fork_strategy: value.fork_strategy.unwrap_or_default(),
                index_locations,
                index_strategy: value.index_strategy.unwrap_or_default(),
                keyring_provider: value.keyring_provider.unwrap_or_default(),
                link_mode: value.link_mode.unwrap_or_default(),
                no_build_isolation: value.no_build_isolation.unwrap_or_default(),
                no_build_isolation_package: value.no_build_isolation_package.unwrap_or_default(),
                prerelease: value.prerelease.unwrap_or_default(),
                resolution: value.resolution.unwrap_or_default(),
                sources: SourceStrategy::from_args(value.no_sources.unwrap_or_default()),
                upgrade: Upgrade::from_args(
                    value.upgrade,
                    value
                        .upgrade_package
                        .into_iter()
                        .flatten()
                        .map(Requirement::from)
                        .collect(),
                ),
            },
            compile_bytecode: value.compile_bytecode.unwrap_or_default(),
            reinstall: Reinstall::from_args(
                value.reinstall,
                value.reinstall_package.unwrap_or_default(),
            ),
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
    pub(crate) install_mirrors: PythonInstallMirrors,
    pub(crate) system: bool,
    pub(crate) extras: ExtrasSpecification,
    pub(crate) groups: Vec<PipGroupName>,
    pub(crate) break_system_packages: bool,
    pub(crate) target: Option<Target>,
    pub(crate) prefix: Option<Prefix>,
    pub(crate) index_strategy: IndexStrategy,
    pub(crate) keyring_provider: KeyringProviderType,
    pub(crate) no_build_isolation: bool,
    pub(crate) no_build_isolation_package: Vec<PackageName>,
    pub(crate) build_options: BuildOptions,
    pub(crate) allow_empty_requirements: bool,
    pub(crate) strict: bool,
    pub(crate) dependency_mode: DependencyMode,
    pub(crate) resolution: ResolutionMode,
    pub(crate) prerelease: PrereleaseMode,
    pub(crate) fork_strategy: ForkStrategy,
    pub(crate) dependency_metadata: DependencyMetadata,
    pub(crate) output_file: Option<PathBuf>,
    pub(crate) no_strip_extras: bool,
    pub(crate) no_strip_markers: bool,
    pub(crate) no_annotate: bool,
    pub(crate) no_header: bool,
    pub(crate) custom_compile_command: Option<String>,
    pub(crate) generate_hashes: bool,
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
    pub(crate) sources: SourceStrategy,
    pub(crate) hash_checking: Option<HashCheckingMode>,
    pub(crate) upgrade: Upgrade,
    pub(crate) reinstall: Reinstall,
}

impl PipSettings {
    /// Resolve the [`PipSettings`] from the CLI and filesystem configuration.
    pub(crate) fn combine(args: PipOptions, filesystem: Option<FilesystemOptions>) -> Self {
        let Options {
            top_level,
            pip,
            install_mirrors,
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
            keyring_provider,
            no_build,
            no_binary,
            only_binary,
            no_build_isolation,
            no_build_isolation_package,
            strict,
            extra,
            all_extras,
            no_extra,
            group,
            no_deps,
            allow_empty_requirements,
            resolution,
            prerelease,
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
            upgrade,
            upgrade_package,
            reinstall,
            reinstall_package,
        } = pip.unwrap_or_default();

        let ResolverInstallerOptions {
            index: top_level_index,
            index_url: top_level_index_url,
            extra_index_url: top_level_extra_index_url,
            no_index: top_level_no_index,
            find_links: top_level_find_links,
            index_strategy: top_level_index_strategy,
            keyring_provider: top_level_keyring_provider,
            resolution: top_level_resolution,
            prerelease: top_level_prerelease,
            fork_strategy: top_level_fork_strategy,
            dependency_metadata: top_level_dependency_metadata,
            config_settings: top_level_config_settings,
            no_build_isolation: top_level_no_build_isolation,
            no_build_isolation_package: top_level_no_build_isolation_package,
            exclude_newer: top_level_exclude_newer,
            link_mode: top_level_link_mode,
            compile_bytecode: top_level_compile_bytecode,
            no_sources: top_level_no_sources,
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
        let index = index.combine(top_level_index);
        let no_index = no_index.combine(top_level_no_index);
        let index_url = index_url.combine(top_level_index_url);
        let extra_index_url = extra_index_url.combine(top_level_extra_index_url);
        let find_links = find_links.combine(top_level_find_links);
        let index_strategy = index_strategy.combine(top_level_index_strategy);
        let keyring_provider = keyring_provider.combine(top_level_keyring_provider);
        let resolution = resolution.combine(top_level_resolution);
        let prerelease = prerelease.combine(top_level_prerelease);
        let fork_strategy = fork_strategy.combine(top_level_fork_strategy);
        let dependency_metadata = dependency_metadata.combine(top_level_dependency_metadata);
        let config_settings = config_settings.combine(top_level_config_settings);
        let no_build_isolation = no_build_isolation.combine(top_level_no_build_isolation);
        let no_build_isolation_package =
            no_build_isolation_package.combine(top_level_no_build_isolation_package);
        let exclude_newer = exclude_newer.combine(top_level_exclude_newer);
        let link_mode = link_mode.combine(top_level_link_mode);
        let compile_bytecode = compile_bytecode.combine(top_level_compile_bytecode);
        let no_sources = no_sources.combine(top_level_no_sources);
        let upgrade = upgrade.combine(top_level_upgrade);
        let upgrade_package = upgrade_package.combine(top_level_upgrade_package);
        let reinstall = reinstall.combine(top_level_reinstall);
        let reinstall_package = reinstall_package.combine(top_level_reinstall_package);

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
                args.all_extras.combine(all_extras).unwrap_or_default(),
                args.no_extra.combine(no_extra).unwrap_or_default(),
                args.extra.combine(extra).unwrap_or_default(),
            ),
            groups: args.group.combine(group).unwrap_or_default(),
            dependency_mode: if args.no_deps.combine(no_deps).unwrap_or_default() {
                DependencyMode::Direct
            } else {
                DependencyMode::Transitive
            },
            resolution: args.resolution.combine(resolution).unwrap_or_default(),
            prerelease: args.prerelease.combine(prerelease).unwrap_or_default(),
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
            no_build_isolation: args
                .no_build_isolation
                .combine(no_build_isolation)
                .unwrap_or_default(),
            no_build_isolation_package: args
                .no_build_isolation_package
                .combine(no_build_isolation_package)
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
            sources: SourceStrategy::from_args(
                args.no_sources.combine(no_sources).unwrap_or_default(),
            ),
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
            install_mirrors,
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
            no_build_isolation: settings.resolver.no_build_isolation,
            no_build_isolation_package: &settings.resolver.no_build_isolation_package,
            exclude_newer: settings.resolver.exclude_newer,
            link_mode: settings.resolver.link_mode,
            compile_bytecode: settings.compile_bytecode,
            reinstall: &settings.reinstall,
            build_options: &settings.resolver.build_options,
            sources: settings.resolver.sources,
        }
    }
}

/// The resolved settings to use for an invocation of the `uv publish` CLI.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub(crate) struct PublishSettings {
    // CLI only, see [`PublishArgs`] for docs.
    pub(crate) files: Vec<String>,
    pub(crate) username: Option<String>,
    pub(crate) password: Option<String>,
    pub(crate) index: Option<String>,

    // Both CLI and configuration.
    pub(crate) publish_url: Url,
    pub(crate) trusted_publishing: TrustedPublishing,
    pub(crate) keyring_provider: KeyringProviderType,
    pub(crate) check_url: Option<IndexUrl>,

    // Configuration only
    pub(crate) index_locations: IndexLocations,
}

impl PublishSettings {
    /// Resolve the [`PublishSettings`] from the CLI and filesystem configuration.
    pub(crate) fn resolve(args: PublishArgs, filesystem: Option<FilesystemOptions>) -> Self {
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
        let ResolverInstallerOptions {
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
            publish_url: args
                .publish_url
                .combine(publish_url)
                .unwrap_or_else(|| Url::parse(PYPI_PUBLISH_URL).unwrap()),
            trusted_publishing: trusted_publishing
                .combine(args.trusted_publishing)
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

// Environment variables that are not exposed as CLI arguments.
mod env {
    use uv_static::EnvVars;

    pub(super) const CONCURRENT_DOWNLOADS: (&str, &str) =
        (EnvVars::UV_CONCURRENT_DOWNLOADS, "a non-zero integer");

    pub(super) const CONCURRENT_BUILDS: (&str, &str) =
        (EnvVars::UV_CONCURRENT_BUILDS, "a non-zero integer");

    pub(super) const CONCURRENT_INSTALLS: (&str, &str) =
        (EnvVars::UV_CONCURRENT_INSTALLS, "a non-zero integer");

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
#[allow(clippy::exit, clippy::print_stderr)]
fn parse_failure(name: &str, expected: &str) -> ! {
    eprintln!("error: invalid value for {name}, expected {expected}");
    process::exit(1)
}
