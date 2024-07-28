use uv_cache::Refresh;
use uv_configuration::ConfigSettings;
use uv_resolver::PreReleaseMode;
use uv_settings::{PipOptions, ResolverInstallerOptions, ResolverOptions};

use crate::{
    BuildArgs, IndexArgs, InstallerArgs, Maybe, RefreshArgs, ResolverArgs, ResolverInstallerArgs,
};

/// Given a boolean flag pair (like `--upgrade` and `--no-upgrade`), resolve the value of the flag.
pub fn flag(yes: bool, no: bool) -> Option<bool> {
    match (yes, no) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        (false, false) => None,
        (..) => unreachable!("Clap should make this impossible"),
    }
}

impl From<RefreshArgs> for Refresh {
    fn from(value: RefreshArgs) -> Self {
        let RefreshArgs {
            refresh,
            no_refresh,
            refresh_package,
        } = value;

        Self::from_args(flag(refresh, no_refresh), refresh_package)
    }
}

impl From<ResolverArgs> for PipOptions {
    fn from(args: ResolverArgs) -> Self {
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
            config_setting,
            exclude_newer,
            link_mode,
        } = args;

        Self {
            upgrade: flag(upgrade, no_upgrade),
            upgrade_package: Some(upgrade_package),
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
            ..PipOptions::from(index_args)
        }
    }
}

impl From<InstallerArgs> for PipOptions {
    fn from(args: InstallerArgs) -> Self {
        let InstallerArgs {
            index_args,
            reinstall,
            no_reinstall,
            reinstall_package,
            index_strategy,
            keyring_provider,
            config_setting,
            exclude_newer,
            link_mode,
            compile_bytecode,
            no_compile_bytecode,
        } = args;

        Self {
            reinstall: flag(reinstall, no_reinstall),
            reinstall_package: Some(reinstall_package),
            index_strategy,
            keyring_provider,
            config_settings: config_setting
                .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
            exclude_newer,
            link_mode,
            compile_bytecode: flag(compile_bytecode, no_compile_bytecode),
            ..PipOptions::from(index_args)
        }
    }
}

impl From<ResolverInstallerArgs> for PipOptions {
    fn from(args: ResolverInstallerArgs) -> Self {
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
            config_setting,
            exclude_newer,
            link_mode,
            compile_bytecode,
            no_compile_bytecode,
        } = args;

        Self {
            upgrade: flag(upgrade, no_upgrade),
            upgrade_package: Some(upgrade_package),
            reinstall: flag(reinstall, no_reinstall),
            reinstall_package: Some(reinstall_package),
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
            ..PipOptions::from(index_args)
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
            no_index: if no_index { Some(true) } else { None },
            find_links,
            ..PipOptions::default()
        }
    }
}

/// Construct the [`ResolverOptions`] from the [`ResolverArgs`] and [`BuildArgs`].
pub fn resolver_options(resolver_args: ResolverArgs, build_args: BuildArgs) -> ResolverOptions {
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
        config_setting,
        exclude_newer,
        link_mode,
    } = resolver_args;

    let BuildArgs {
        no_build,
        build,
        no_build_package,
        no_binary,
        binary,
        no_binary_package,
    } = build_args;

    ResolverOptions {
        index_url: index_args.index_url.and_then(Maybe::into_option),
        extra_index_url: index_args.extra_index_url.map(|extra_index_urls| {
            extra_index_urls
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect()
        }),
        no_index: if index_args.no_index {
            Some(true)
        } else {
            None
        },
        find_links: index_args.find_links,
        upgrade: flag(upgrade, no_upgrade),
        upgrade_package: Some(upgrade_package),
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
        no_build: flag(no_build, build),
        no_build_package: Some(no_build_package),
        no_binary: flag(no_binary, binary),
        no_binary_package: Some(no_binary_package),
    }
}

/// Construct the [`ResolverInstallerOptions`] from the [`ResolverInstallerArgs`] and [`BuildArgs`].
pub fn resolver_installer_options(
    resolver_installer_args: ResolverInstallerArgs,
    build_args: BuildArgs,
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
        config_setting,
        exclude_newer,
        link_mode,
        compile_bytecode,
        no_compile_bytecode,
    } = resolver_installer_args;

    let BuildArgs {
        no_build,
        build,
        no_build_package,
        no_binary,
        binary,
        no_binary_package,
    } = build_args;

    ResolverInstallerOptions {
        index_url: index_args.index_url.and_then(Maybe::into_option),
        extra_index_url: index_args.extra_index_url.map(|extra_index_urls| {
            extra_index_urls
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect()
        }),
        no_index: if index_args.no_index {
            Some(true)
        } else {
            None
        },
        find_links: index_args.find_links,
        upgrade: flag(upgrade, no_upgrade),
        upgrade_package: Some(upgrade_package),
        reinstall: flag(reinstall, no_reinstall),
        reinstall_package: Some(reinstall_package),
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
        no_build: flag(no_build, build),
        no_build_package: Some(no_build_package),
        no_binary: flag(no_binary, binary),
        no_binary_package: Some(no_binary_package),
    }
}
