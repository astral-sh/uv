use std::fmt;

use anstream::{eprint, eprintln};

use tracing::debug;
use uv_cache::Refresh;
use uv_configuration::{BuildIsolation, Reinstall, Upgrade};
use uv_distribution_types::{
    ConfigSettings, Index, IndexArg, IndexArgStrategy, PackageConfigSettings, Requirement,
};
use uv_preview::{Preview, PreviewFeature};
use uv_resolver::{ExcludeNewer, ExcludeNewerPackage, PrereleaseMode};
use uv_settings::{
    Combine, EnvFlag, FilesystemOptions, PipOptions, ResolverInstallerOptions, ResolverOptions,
};
use uv_warnings::{
    owo_colors::{AnsiColors, OwoColorize},
    warn_user_once,
};

use crate::{
    BuildOptionsArgs, FetchArgs, IndexArgs, InstallerArgs, Maybe, RefreshArgs, ResolverArgs,
    ResolverInstallerArgs,
};

/// Given a boolean flag pair (like `--upgrade` and `--no-upgrade`), resolve the value of the flag.
pub fn flag(yes: bool, no: bool, name: &str) -> Option<bool> {
    match (yes, no) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        (false, false) => None,
        (..) => {
            eprintln!(
                "{}{} `{}` and `{}` cannot be used together. \
                Boolean flags on different levels are currently not supported \
                (https://github.com/clap-rs/clap/issues/6049)",
                "error".bold().red(),
                ":".bold(),
                format!("--{name}").green(),
                format!("--no-{name}").green(),
            );
            // No error forwarding since should eventually be solved on the clap side.
            #[expect(clippy::exit)]
            {
                std::process::exit(2);
            }
        }
    }
}

/// The source of a boolean flag value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagSource {
    /// The flag was set via command-line argument.
    Cli,
    /// The flag was set via environment variable.
    Env(&'static str),
    /// The flag was set via workspace/project configuration.
    Config,
}

impl fmt::Display for FlagSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cli => write!(f, "command-line argument"),
            Self::Env(name) => write!(f, "environment variable `{name}`"),
            Self::Config => write!(f, "workspace configuration"),
        }
    }
}

/// A boolean flag value with its source.
#[derive(Debug, Clone, Copy)]
pub enum Flag {
    /// The flag is not set.
    Disabled,
    /// The flag is enabled with a known source.
    Enabled {
        source: FlagSource,
        /// The CLI flag name (e.g., "locked" for `--locked`).
        name: &'static str,
    },
}

impl Flag {
    /// Create a flag that is explicitly disabled.
    pub const fn disabled() -> Self {
        Self::Disabled
    }

    /// Create an enabled flag from a CLI argument.
    pub const fn from_cli(name: &'static str) -> Self {
        Self::Enabled {
            source: FlagSource::Cli,
            name,
        }
    }

    /// Create an enabled flag from workspace/project configuration.
    pub const fn from_config(name: &'static str) -> Self {
        Self::Enabled {
            source: FlagSource::Config,
            name,
        }
    }

    /// Returns `true` if the flag is set.
    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled { .. })
    }

    /// Returns the source of the flag, if it is set.
    pub fn source(self) -> Option<FlagSource> {
        match self {
            Self::Disabled => None,
            Self::Enabled { source, .. } => Some(source),
        }
    }

    /// Returns the CLI flag name, if the flag is enabled.
    pub fn name(self) -> Option<&'static str> {
        match self {
            Self::Disabled => None,
            Self::Enabled { name, .. } => Some(name),
        }
    }
}

impl From<Flag> for bool {
    fn from(flag: Flag) -> Self {
        flag.is_enabled()
    }
}

/// Resolve a boolean flag from CLI arguments and an environment variable.
///
/// The CLI argument takes precedence over the environment variable. Returns a [`Flag`] with the
/// resolved value and source.
pub fn resolve_flag(cli_flag: bool, name: &'static str, env_flag: EnvFlag) -> Flag {
    if cli_flag {
        Flag::Enabled {
            source: FlagSource::Cli,
            name,
        }
    } else if env_flag.value == Some(true) {
        Flag::Enabled {
            source: FlagSource::Env(env_flag.env_var),
            name,
        }
    } else {
        Flag::Disabled
    }
}

/// Check if two flags conflict and exit with an error if they do.
///
/// This function checks if both flags are enabled (truthy) and reports an error if so, including
/// the source of each flag (CLI or environment variable) in the error message.
pub fn check_conflicts(flag_a: Flag, flag_b: Flag) {
    if let (
        Flag::Enabled {
            source: source_a,
            name: name_a,
        },
        Flag::Enabled {
            source: source_b,
            name: name_b,
        },
    ) = (flag_a, flag_b)
    {
        let display_a = match source_a {
            FlagSource::Cli => format!("`--{name_a}`"),
            FlagSource::Env(env) => format!("`{env}` (environment variable)"),
            FlagSource::Config => format!("`{name_a}` (workspace configuration)"),
        };
        let display_b = match source_b {
            FlagSource::Cli => format!("`--{name_b}`"),
            FlagSource::Env(env) => format!("`{env}` (environment variable)"),
            FlagSource::Config => format!("`{name_b}` (workspace configuration)"),
        };
        eprintln!(
            "{}{} the argument {} cannot be used with {}",
            "error".bold().red(),
            ":".bold(),
            display_a.green(),
            display_b.green(),
        );
        #[expect(clippy::exit)]
        {
            std::process::exit(2);
        }
    }
}

/// Resolve CLI index arguments from `--index` and `--default-index` into a
/// single list of indexes.
///
/// Indexes passed by name are resolved from the filesystem configuration
/// prioritizing indexes from the workspace member, then the workspace, and then
/// the configuration.
///
/// A warning is emitted for any names which resolve to an explicit index.
///
/// If `permit_single_explicit_index` is true, the warning is only emitted when
/// more than one index is provided (this is intended for the `uv add` path
/// where a single explicit index can be used to pin a package to a source).
pub fn resolve_and_combine_indexes(
    default_index: Option<Maybe<Index>>,
    index: Option<Vec<Vec<Maybe<IndexArg>>>>,
    filesystem: Option<&FilesystemOptions>,
    package_indexes: Vec<Index>,
    preview: Preview,
    permit_single_explicit_index: bool,
) -> Option<Vec<Index>> {
    let filesystem_indexes: Vec<Index> = package_indexes
        .into_iter()
        .chain(
            filesystem
                .map(|filesystem| filesystem.top_level.indexes())
                .into_iter()
                .flatten(),
        )
        .collect();

    let strategy = if preview.is_enabled(PreviewFeature::IndexAssumeName) {
        IndexArgStrategy::IgnoreDirectory
    } else {
        IndexArgStrategy::PreferDirectory
    };
    let resolve = |index_arg: IndexArg| -> Index {
        match index_arg.try_resolve(&filesystem_indexes, strategy) {
            Ok(index) => {
                debug!("Resolved index by name: {:?}", index);
                index
            }
            Err(error) => {
                let mut error_chain = String::new();
                // Writing to a string can't fail with errors (panics on allocation failure)
                uv_warnings::write_error_chain(&error, &mut error_chain, "error", AnsiColors::Red)
                    .unwrap();
                eprint!("{}", error_chain);
                #[expect(clippy::exit)]
                std::process::exit(2);
            }
        }
    };

    let default_index = default_index
        .and_then(Maybe::into_option)
        .map(|default_index| vec![default_index]);

    let index = index.map(|index| {
        index
            .into_iter()
            .flat_map(|v| v.clone())
            .filter_map(Maybe::into_option)
            .map(resolve)
            .collect()
    });

    let combined = default_index.combine(index);

    if let Some(ref indexes) = combined {
        if !permit_single_explicit_index || indexes.len() > 1 {
            if let Some(index) = indexes.iter().find(|index| index.explicit) {
                let name = index
                    .name
                    .as_ref()
                    .map_or_else(|| index.url.to_string(), ToString::to_string);
                warn_user_once!(
                    "Explicit index `{name}` will be ignored. Explicit indexes are only used when specified in `[tool.uv.sources]`."
                );
            }
        }
    }

    combined
}

impl From<RefreshArgs> for Refresh {
    fn from(value: RefreshArgs) -> Self {
        let RefreshArgs {
            refresh,
            no_refresh,
            refresh_package,
        } = value;

        Self::from_args(flag(refresh, no_refresh, "no-refresh"), refresh_package)
    }
}

/// Like [`From`] trait for conversions specifically to [`PipOptions`] from
/// `*Args` types which contain [`uv_distribution_types::IndexArg`] elements and
/// therefore need the filesystem options.
pub trait Resolve<A>: Sized {
    fn resolve(
        args: A,
        filesystem: Option<&FilesystemOptions>,
        package_indexes: Vec<Index>,
        preview: Preview,
    ) -> Self;
}

impl Resolve<ResolverArgs> for PipOptions {
    fn resolve(
        args: ResolverArgs,
        filesystem: Option<&FilesystemOptions>,
        package_indexes: Vec<Index>,
        preview: Preview,
    ) -> Self {
        let ResolverArgs {
            index_args,
            upgrade,
            no_upgrade,
            upgrade_package,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease,
            pre,
            fork_strategy,
            config_setting,
            config_settings_package,
            no_build_isolation,
            no_build_isolation_package,
            build_isolation,
            exclude_newer,
            link_mode,
            no_sources,
            no_sources_package,
            exclude_newer_package,
        } = args;

        Self {
            upgrade: flag(upgrade, no_upgrade, "no-upgrade"),
            upgrade_package: Some(upgrade_package),
            index_strategy,
            keyring_provider,
            resolution,
            fork_strategy,
            prerelease: if pre {
                Some(PrereleaseMode::Allow)
            } else {
                prerelease
            },
            config_settings: config_setting
                .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
            config_settings_package: config_settings_package.map(|config_settings| {
                config_settings
                    .into_iter()
                    .collect::<PackageConfigSettings>()
            }),
            no_build_isolation: flag(no_build_isolation, build_isolation, "build-isolation"),
            no_build_isolation_package: Some(no_build_isolation_package),
            exclude_newer,
            exclude_newer_package: exclude_newer_package.map(ExcludeNewerPackage::from_iter),
            link_mode,
            no_sources: if no_sources { Some(true) } else { None },
            no_sources_package: Some(no_sources_package),
            ..Self::resolve(index_args, filesystem, package_indexes, preview)
        }
    }
}

impl Resolve<InstallerArgs> for PipOptions {
    fn resolve(
        args: InstallerArgs,
        filesystem: Option<&FilesystemOptions>,
        package_indexes: Vec<Index>,
        preview: Preview,
    ) -> Self {
        let InstallerArgs {
            index_args,
            reinstall,
            no_reinstall,
            reinstall_package,
            index_strategy,
            keyring_provider,
            config_setting,
            config_settings_package,
            no_build_isolation,
            build_isolation,
            exclude_newer,
            link_mode,
            compile_bytecode,
            no_compile_bytecode,
            no_sources,
            no_sources_package,
            exclude_newer_package,
        } = args;

        Self {
            reinstall: flag(reinstall, no_reinstall, "reinstall"),
            reinstall_package: Some(reinstall_package),
            index_strategy,
            keyring_provider,
            config_settings: config_setting
                .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
            config_settings_package: config_settings_package.map(|config_settings| {
                config_settings
                    .into_iter()
                    .collect::<PackageConfigSettings>()
            }),
            no_build_isolation: flag(no_build_isolation, build_isolation, "build-isolation"),
            exclude_newer,
            exclude_newer_package: exclude_newer_package.map(ExcludeNewerPackage::from_iter),
            link_mode,
            compile_bytecode: flag(compile_bytecode, no_compile_bytecode, "compile-bytecode"),
            no_sources: if no_sources { Some(true) } else { None },
            no_sources_package: Some(no_sources_package),
            ..Self::resolve(index_args, filesystem, package_indexes, preview)
        }
    }
}

impl Resolve<ResolverInstallerArgs> for PipOptions {
    fn resolve(
        args: ResolverInstallerArgs,
        filesystem: Option<&FilesystemOptions>,
        package_indexes: Vec<Index>,
        preview: Preview,
    ) -> Self {
        let ResolverInstallerArgs {
            index_args,
            upgrade,
            no_upgrade,
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
            config_settings_package,
            no_build_isolation,
            no_build_isolation_package,
            build_isolation,
            exclude_newer,
            link_mode,
            compile_bytecode,
            no_compile_bytecode,
            no_sources,
            no_sources_package,
            exclude_newer_package,
        } = args;

        Self {
            upgrade: flag(upgrade, no_upgrade, "upgrade"),
            upgrade_package: Some(upgrade_package),
            reinstall: flag(reinstall, no_reinstall, "reinstall"),
            reinstall_package: Some(reinstall_package),
            index_strategy,
            keyring_provider,
            resolution,
            prerelease: if pre {
                Some(PrereleaseMode::Allow)
            } else {
                prerelease
            },
            fork_strategy,
            config_settings: config_setting
                .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
            config_settings_package: config_settings_package.map(|config_settings| {
                config_settings
                    .into_iter()
                    .collect::<PackageConfigSettings>()
            }),
            no_build_isolation: flag(no_build_isolation, build_isolation, "build-isolation"),
            no_build_isolation_package: Some(no_build_isolation_package),
            exclude_newer,
            exclude_newer_package: exclude_newer_package.map(ExcludeNewerPackage::from_iter),
            link_mode,
            compile_bytecode: flag(compile_bytecode, no_compile_bytecode, "compile-bytecode"),
            no_sources: if no_sources { Some(true) } else { None },
            no_sources_package: Some(no_sources_package),
            ..Self::resolve(index_args, filesystem, package_indexes, preview)
        }
    }
}

impl Resolve<FetchArgs> for PipOptions {
    fn resolve(
        args: FetchArgs,
        filesystem: Option<&FilesystemOptions>,
        package_indexes: Vec<Index>,
        preview: Preview,
    ) -> Self {
        let FetchArgs {
            index_args,
            index_strategy,
            keyring_provider,
            exclude_newer,
        } = args;

        Self {
            index_strategy,
            keyring_provider,
            exclude_newer,
            ..Self::resolve(index_args, filesystem, package_indexes, preview)
        }
    }
}

impl Resolve<IndexArgs> for PipOptions {
    fn resolve(
        args: IndexArgs,
        filesystem: Option<&FilesystemOptions>,
        package_indexes: Vec<Index>,
        preview: Preview,
    ) -> Self {
        let IndexArgs {
            default_index,
            index,
            index_url,
            extra_index_url,
            no_index,
            find_links,
        } = args;

        Self {
            index: resolve_and_combine_indexes(
                default_index,
                index,
                filesystem,
                package_indexes,
                preview,
                false,
            ),
            index_url: index_url.and_then(Maybe::into_option),
            extra_index_url: extra_index_url.map(|extra_index_urls| {
                extra_index_urls
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect()
            }),
            no_index: if no_index { Some(true) } else { None },
            find_links: find_links.map(|find_links| {
                find_links
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect()
            }),
            ..Self::default()
        }
    }
}

/// Construct the [`ResolverOptions`] from the [`ResolverArgs`] and [`BuildOptionsArgs`].
pub fn resolver_options(
    resolver_args: ResolverArgs,
    build_args: BuildOptionsArgs,
    filesystem: Option<&FilesystemOptions>,
    package_indexes: Vec<Index>,
    preview: Preview,
) -> ResolverOptions {
    let ResolverArgs {
        index_args,
        upgrade,
        no_upgrade,
        upgrade_package,
        index_strategy,
        keyring_provider,
        resolution,
        prerelease,
        pre,
        fork_strategy,
        config_setting,
        config_settings_package,
        no_build_isolation,
        no_build_isolation_package,
        build_isolation,
        exclude_newer,
        link_mode,
        no_sources,
        no_sources_package,
        exclude_newer_package,
    } = resolver_args;

    let BuildOptionsArgs {
        no_build,
        build,
        no_build_package,
        no_binary,
        binary,
        no_binary_package,
    } = build_args;

    ResolverOptions {
        index: resolve_and_combine_indexes(
            index_args.default_index,
            index_args.index,
            filesystem,
            package_indexes,
            preview,
            false,
        ),
        index_url: index_args.index_url.and_then(Maybe::into_option),
        extra_index_url: index_args.extra_index_url.map(|extra_index_url| {
            extra_index_url
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect()
        }),
        no_index: if index_args.no_index {
            Some(true)
        } else {
            None
        },
        find_links: index_args.find_links.map(|find_links| {
            find_links
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect()
        }),
        upgrade: Upgrade::from_args(
            flag(upgrade, no_upgrade, "no-upgrade"),
            upgrade_package.into_iter().map(Requirement::from).collect(),
        ),
        index_strategy,
        keyring_provider,
        resolution,
        prerelease: if pre {
            Some(PrereleaseMode::Allow)
        } else {
            prerelease
        },
        fork_strategy,
        dependency_metadata: None,
        config_settings: config_setting
            .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
        config_settings_package: config_settings_package.map(|config_settings| {
            config_settings
                .into_iter()
                .collect::<PackageConfigSettings>()
        }),
        build_isolation: BuildIsolation::from_args(
            flag(no_build_isolation, build_isolation, "build-isolation"),
            no_build_isolation_package,
        ),
        extra_build_dependencies: None,
        extra_build_variables: None,
        exclude_newer: ExcludeNewer::from_args(
            exclude_newer,
            exclude_newer_package.unwrap_or_default(),
        ),
        link_mode,
        torch_backend: None,
        no_build: flag(no_build, build, "build"),
        no_build_package: Some(no_build_package),
        no_binary: flag(no_binary, binary, "binary"),
        no_binary_package: Some(no_binary_package),
        no_sources: if no_sources { Some(true) } else { None },
        no_sources_package: Some(no_sources_package),
    }
}

/// Construct the [`ResolverInstallerOptions`] from the [`ResolverInstallerArgs`] and [`BuildOptionsArgs`].
pub fn resolver_installer_options(
    resolver_installer_args: ResolverInstallerArgs,
    build_args: BuildOptionsArgs,
    filesystem: Option<&FilesystemOptions>,
    package_indexes: Vec<Index>,
    preview: Preview,
) -> ResolverInstallerOptions {
    let ResolverInstallerArgs {
        index_args,
        upgrade,
        no_upgrade,
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
        config_settings_package,
        no_build_isolation,
        no_build_isolation_package,
        build_isolation,
        exclude_newer,
        exclude_newer_package,
        link_mode,
        compile_bytecode,
        no_compile_bytecode,
        no_sources,
        no_sources_package,
    } = resolver_installer_args;

    let BuildOptionsArgs {
        no_build,
        build,
        no_build_package,
        no_binary,
        binary,
        no_binary_package,
    } = build_args;

    ResolverInstallerOptions {
        index: resolve_and_combine_indexes(
            index_args.default_index,
            index_args.index,
            filesystem,
            package_indexes,
            preview,
            false,
        ),
        index_url: index_args.index_url.and_then(Maybe::into_option),
        extra_index_url: index_args.extra_index_url.map(|extra_index_url| {
            extra_index_url
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect()
        }),
        no_index: if index_args.no_index {
            Some(true)
        } else {
            None
        },
        find_links: index_args.find_links.map(|find_links| {
            find_links
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect()
        }),
        upgrade: Upgrade::from_args(
            flag(upgrade, no_upgrade, "upgrade"),
            upgrade_package.into_iter().map(Requirement::from).collect(),
        ),
        reinstall: Reinstall::from_args(
            flag(reinstall, no_reinstall, "reinstall"),
            reinstall_package,
        ),
        index_strategy,
        keyring_provider,
        resolution,
        prerelease: if pre {
            Some(PrereleaseMode::Allow)
        } else {
            prerelease
        },
        fork_strategy,
        dependency_metadata: None,
        config_settings: config_setting
            .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
        config_settings_package: config_settings_package.map(|config_settings| {
            config_settings
                .into_iter()
                .collect::<PackageConfigSettings>()
        }),
        build_isolation: BuildIsolation::from_args(
            flag(no_build_isolation, build_isolation, "build-isolation"),
            no_build_isolation_package,
        ),
        extra_build_dependencies: None,
        extra_build_variables: None,
        exclude_newer,
        exclude_newer_package: exclude_newer_package.map(ExcludeNewerPackage::from_iter),
        link_mode,
        compile_bytecode: flag(compile_bytecode, no_compile_bytecode, "compile-bytecode"),
        no_build: flag(no_build, build, "build"),
        no_build_package: if no_build_package.is_empty() {
            None
        } else {
            Some(no_build_package)
        },
        no_binary: flag(no_binary, binary, "binary"),
        no_binary_package: if no_binary_package.is_empty() {
            None
        } else {
            Some(no_binary_package)
        },
        no_sources: if no_sources { Some(true) } else { None },
        no_sources_package: if no_sources_package.is_empty() {
            None
        } else {
            Some(no_sources_package)
        },
        torch_backend: None,
    }
}
