use std::fmt;

use anyhow::bail;

use uv_cache::Refresh;
use uv_configuration::{BuildIsolation, Reinstall, Upgrade};
use uv_distribution_types::{ConfigSettings, Index, PackageConfigSettings, Requirement};
use uv_resolver::{ExcludeNewerPackage, PrereleaseMode};
use uv_settings::{Combine, EnvFlag, PipOptions, ResolverInstallerOptions, ResolverOptions};
use uv_warnings::owo_colors::OwoColorize;

use crate::{
    BuildOptionsArgs, FetchArgs, IndexArgs, InstallerArgs, Maybe, RefreshArgs, ResolverArgs,
    ResolverInstallerArgs,
};

/// Given a boolean flag pair (like `--upgrade` and `--no-upgrade`), resolve the value of the flag.
pub fn flag(yes: bool, no: bool, name: &str) -> anyhow::Result<Option<bool>> {
    match (yes, no) {
        (true, false) => Ok(Some(true)),
        (false, true) => Ok(Some(false)),
        (false, false) => Ok(None),
        (..) => {
            bail!(
                "`{}` and `{}` cannot be used together. \
                Boolean flags on different levels are currently not supported \
                (https://github.com/clap-rs/clap/issues/6049)",
                format!("--{name}").green(),
                format!("--no-{name}").green(),
            );
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

/// Resolve a pair of mutually exclusive boolean flags from the CLI and environment variables.
///
/// If either flag is set on the command line, both environment variables are ignored so the CLI
/// retains precedence over the full pair.
pub fn resolve_flag_pair(
    cli_flag: bool,
    cli_no_flag: bool,
    name: &'static str,
    no_name: &'static str,
    env_flag: Option<EnvFlag>,
    env_no_flag: Option<EnvFlag>,
) -> (Flag, Flag) {
    if cli_flag || cli_no_flag {
        (
            if cli_flag {
                Flag::from_cli(name)
            } else {
                Flag::disabled()
            },
            if cli_no_flag {
                Flag::from_cli(no_name)
            } else {
                Flag::disabled()
            },
        )
    } else {
        (
            env_flag.map_or_else(Flag::disabled, |env_flag| {
                resolve_flag(false, name, env_flag)
            }),
            env_no_flag.map_or_else(Flag::disabled, |env_no_flag| {
                resolve_flag(false, no_name, env_no_flag)
            }),
        )
    }
}

/// Check if two flags conflict and return an error if they do.
///
/// This function checks if both flags are enabled (truthy) and reports an error if so, including
/// the source of each flag (CLI or environment variable) in the error message.
pub fn check_conflicts(flag_a: Flag, flag_b: Flag) -> anyhow::Result<()> {
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
        bail!(
            "the argument {} cannot be used with {}",
            display_a.green(),
            display_b.green()
        );
    }
    Ok(())
}

impl TryFrom<RefreshArgs> for Refresh {
    type Error = anyhow::Error;

    fn try_from(value: RefreshArgs) -> anyhow::Result<Self> {
        let RefreshArgs {
            refresh,
            no_refresh,
            refresh_package,
        } = value;

        Ok(Self::from_args(
            flag(refresh, no_refresh, "no-refresh")?,
            refresh_package,
        ))
    }
}

/// Extract the `--index` and `--default-index` values from [`IndexArgs`].
pub fn indexes_from_args(
    default_index: Option<&Maybe<Index>>,
    index: Option<&[Vec<Maybe<Index>>]>,
) -> Option<Vec<Index>> {
    let default_index = default_index
        .cloned()
        .and_then(Maybe::into_option)
        .map(|default_index| vec![default_index]);
    let index = index.map(|index| {
        index
            .iter()
            .flatten()
            .cloned()
            .filter_map(Maybe::into_option)
            .collect()
    });

    default_index.combine(index)
}

impl TryFrom<ResolverArgs> for PipOptions {
    type Error = anyhow::Error;

    fn try_from(args: ResolverArgs) -> anyhow::Result<Self> {
        let ResolverArgs {
            index_args,
            upgrade,
            no_upgrade,
            upgrade_package,
            upgrade_group,
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

        if !upgrade_group.is_empty() {
            bail!(
                "`{}` is not supported in `uv pip` commands",
                "--upgrade-group".green()
            );
        }

        Ok(Self {
            upgrade: flag(upgrade, no_upgrade, "no-upgrade")?,
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
            no_build_isolation: flag(no_build_isolation, build_isolation, "build-isolation")?,
            no_build_isolation_package: Some(no_build_isolation_package),
            exclude_newer,
            exclude_newer_package: exclude_newer_package.map(ExcludeNewerPackage::from_iter),
            link_mode,
            no_sources: if no_sources { Some(true) } else { None },
            no_sources_package: if no_sources_package.is_empty() {
                None
            } else {
                Some(no_sources_package)
            },
            ..Self::from(index_args)
        })
    }
}

impl TryFrom<InstallerArgs> for PipOptions {
    type Error = anyhow::Error;

    fn try_from(args: InstallerArgs) -> anyhow::Result<Self> {
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

        Ok(Self {
            reinstall: flag(reinstall, no_reinstall, "reinstall")?,
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
            no_build_isolation: flag(no_build_isolation, build_isolation, "build-isolation")?,
            exclude_newer,
            exclude_newer_package: exclude_newer_package.map(ExcludeNewerPackage::from_iter),
            link_mode,
            compile_bytecode: flag(compile_bytecode, no_compile_bytecode, "compile-bytecode")?,
            no_sources: if no_sources { Some(true) } else { None },
            no_sources_package: if no_sources_package.is_empty() {
                None
            } else {
                Some(no_sources_package)
            },
            ..Self::from(index_args)
        })
    }
}

impl TryFrom<ResolverInstallerArgs> for PipOptions {
    type Error = anyhow::Error;

    fn try_from(args: ResolverInstallerArgs) -> anyhow::Result<Self> {
        let ResolverInstallerArgs {
            index_args,
            upgrade,
            no_upgrade,
            upgrade_package,
            upgrade_group,
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

        if !upgrade_group.is_empty() {
            bail!(
                "`{}` is not supported in `uv pip` commands",
                "--upgrade-group".green()
            );
        }

        Ok(Self {
            upgrade: flag(upgrade, no_upgrade, "upgrade")?,
            upgrade_package: Some(upgrade_package),
            reinstall: flag(reinstall, no_reinstall, "reinstall")?,
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
            no_build_isolation: flag(no_build_isolation, build_isolation, "build-isolation")?,
            no_build_isolation_package: Some(no_build_isolation_package),
            exclude_newer,
            exclude_newer_package: exclude_newer_package.map(ExcludeNewerPackage::from_iter),
            link_mode,
            compile_bytecode: flag(compile_bytecode, no_compile_bytecode, "compile-bytecode")?,
            no_sources: if no_sources { Some(true) } else { None },
            no_sources_package: if no_sources_package.is_empty() {
                None
            } else {
                Some(no_sources_package)
            },
            ..Self::from(index_args)
        })
    }
}

impl From<FetchArgs> for PipOptions {
    fn from(args: FetchArgs) -> Self {
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
            ..Self::from(index_args)
        }
    }
}

impl From<IndexArgs> for PipOptions {
    fn from(args: IndexArgs) -> Self {
        let IndexArgs {
            default_index,
            index,
            index_url,
            extra_index_url,
            no_index,
            find_links,
        } = args;

        Self {
            index: indexes_from_args(default_index.as_ref(), index.as_deref()),
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
) -> anyhow::Result<ResolverOptions> {
    let ResolverArgs {
        index_args,
        upgrade,
        no_upgrade,
        upgrade_package,
        upgrade_group,
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

    Ok(ResolverOptions {
        index: indexes_from_args(
            index_args.default_index.as_ref(),
            index_args.index.as_deref(),
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
            flag(upgrade, no_upgrade, "no-upgrade")?,
            upgrade_package.into_iter().map(Requirement::from).collect(),
            upgrade_group,
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
            flag(no_build_isolation, build_isolation, "build-isolation")?,
            no_build_isolation_package,
        ),
        extra_build_dependencies: None,
        extra_build_variables: None,
        exclude_newer,
        exclude_newer_package: exclude_newer_package.map(ExcludeNewerPackage::from_iter),
        link_mode,
        torch_backend: None,
        no_build: flag(no_build, build, "build")?,
        no_build_package: if no_build_package.is_empty() {
            None
        } else {
            Some(no_build_package)
        },
        no_binary: flag(no_binary, binary, "binary")?,
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
    })
}

/// Construct the [`ResolverInstallerOptions`] from the [`ResolverInstallerArgs`] and [`BuildOptionsArgs`].
pub fn resolver_installer_options(
    resolver_installer_args: ResolverInstallerArgs,
    build_args: BuildOptionsArgs,
) -> anyhow::Result<ResolverInstallerOptions> {
    let index = indexes_from_args(
        resolver_installer_args.index_args.default_index.as_ref(),
        resolver_installer_args.index_args.index.as_deref(),
    );
    resolver_installer_options_with_indexes(resolver_installer_args, build_args, index)
}

/// Construct the [`ResolverInstallerOptions`] with a precomputed list of indexes.
pub fn resolver_installer_options_with_indexes(
    resolver_installer_args: ResolverInstallerArgs,
    build_args: BuildOptionsArgs,
    index: Option<Vec<Index>>,
) -> anyhow::Result<ResolverInstallerOptions> {
    let ResolverInstallerArgs {
        index_args,
        upgrade,
        no_upgrade,
        upgrade_package,
        upgrade_group,
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

    Ok(ResolverInstallerOptions {
        index,
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
            flag(upgrade, no_upgrade, "upgrade")?,
            upgrade_package.into_iter().map(Requirement::from).collect(),
            upgrade_group,
        ),
        reinstall: Reinstall::from_args(
            flag(reinstall, no_reinstall, "reinstall")?,
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
            flag(no_build_isolation, build_isolation, "build-isolation")?,
            no_build_isolation_package,
        ),
        extra_build_dependencies: None,
        extra_build_variables: None,
        exclude_newer,
        exclude_newer_package: exclude_newer_package.map(ExcludeNewerPackage::from_iter),
        link_mode,
        compile_bytecode: flag(compile_bytecode, no_compile_bytecode, "compile-bytecode")?,
        no_build: flag(no_build, build, "build")?,
        no_build_package: if no_build_package.is_empty() {
            None
        } else {
            Some(no_build_package)
        },
        no_binary: flag(no_binary, binary, "binary")?,
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
    })
}
