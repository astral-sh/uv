use uv_cache::Refresh;
use uv_configuration::ConfigSettings;
use uv_resolver::PrereleaseMode;
use uv_settings::{PipOptions, ResolverInstallerOptions, ResolverOptions};

use crate::{
    BuildOptionsArgs, IndexArgs, InstallerArgs, Maybe, RefreshArgs, ResolverArgs,
    ResolverInstallerArgs,
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
            allow_insecure_host,
            resolution,
            prerelease,
            pre,
            config_setting,
            no_build_isolation,
            no_build_isolation_package,
            build_isolation,
            exclude_newer,
            link_mode,
            no_sources,
        } = args;

        Self {
            upgrade: flag(upgrade, no_upgrade),
            upgrade_package: Some(upgrade_package),
            index_strategy,
            keyring_provider,
            allow_insecure_host: allow_insecure_host.map(|allow_insecure_host| {
                allow_insecure_host
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect()
            }),
            resolution,
            prerelease: if pre {
                Some(PrereleaseMode::Allow)
            } else {
                prerelease
            },
            config_settings: config_setting
                .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
            no_build_isolation: flag(no_build_isolation, build_isolation),
            no_build_isolation_package: Some(no_build_isolation_package),
            exclude_newer,
            link_mode,
            no_sources: if no_sources { Some(true) } else { None },
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
            allow_insecure_host,
            config_setting,
            no_build_isolation,
            build_isolation,
            exclude_newer,
            link_mode,
            compile_bytecode,
            no_compile_bytecode,
            no_sources,
        } = args;

        Self {
            reinstall: flag(reinstall, no_reinstall),
            reinstall_package: Some(reinstall_package),
            index_strategy,
            keyring_provider,
            allow_insecure_host: allow_insecure_host.map(|allow_insecure_host| {
                allow_insecure_host
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect()
            }),
            config_settings: config_setting
                .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
            no_build_isolation: flag(no_build_isolation, build_isolation),
            exclude_newer,
            link_mode,
            compile_bytecode: flag(compile_bytecode, no_compile_bytecode),
            no_sources: if no_sources { Some(true) } else { None },
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
            allow_insecure_host,
            resolution,
            prerelease,
            pre,
            config_setting,
            no_build_isolation,
            no_build_isolation_package,
            build_isolation,
            exclude_newer,
            link_mode,
            compile_bytecode,
            no_compile_bytecode,
            no_sources,
        } = args;

        Self {
            upgrade: flag(upgrade, no_upgrade),
            upgrade_package: Some(upgrade_package),
            reinstall: flag(reinstall, no_reinstall),
            reinstall_package: Some(reinstall_package),
            index_strategy,
            keyring_provider,
            allow_insecure_host: allow_insecure_host.map(|allow_insecure_host| {
                allow_insecure_host
                    .into_iter()
                    .filter_map(Maybe::into_option)
                    .collect()
            }),
            resolution,
            prerelease: if pre {
                Some(PrereleaseMode::Allow)
            } else {
                prerelease
            },
            config_settings: config_setting
                .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
            no_build_isolation: flag(no_build_isolation, build_isolation),
            no_build_isolation_package: Some(no_build_isolation_package),
            exclude_newer,
            link_mode,
            compile_bytecode: flag(compile_bytecode, no_compile_bytecode),
            no_sources: if no_sources { Some(true) } else { None },
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

/// Construct the [`ResolverOptions`] from the [`ResolverArgs`] and [`BuildOptionsArgs`].
pub fn resolver_options(
    resolver_args: ResolverArgs,
    build_args: BuildOptionsArgs,
) -> ResolverOptions {
    let ResolverArgs {
        index_args,
        upgrade,
        no_upgrade,
        upgrade_package,
        index_strategy,
        keyring_provider,
        allow_insecure_host,
        resolution,
        prerelease,
        pre,
        config_setting,
        no_build_isolation,
        no_build_isolation_package,
        build_isolation,
        exclude_newer,
        link_mode,
        no_sources,
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
        allow_insecure_host: allow_insecure_host.map(|allow_insecure_host| {
            allow_insecure_host
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect()
        }),
        resolution,
        prerelease: if pre {
            Some(PrereleaseMode::Allow)
        } else {
            prerelease
        },
        config_settings: config_setting
            .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
        no_build_isolation: flag(no_build_isolation, build_isolation),
        no_build_isolation_package: Some(no_build_isolation_package),
        exclude_newer,
        link_mode,
        no_build: flag(no_build, build),
        no_build_package: Some(no_build_package),
        no_binary: flag(no_binary, binary),
        no_binary_package: Some(no_binary_package),
        no_sources: if no_sources { Some(true) } else { None },
    }
}

/// Construct the [`ResolverInstallerOptions`] from the [`ResolverInstallerArgs`] and [`BuildOptionsArgs`].
pub fn resolver_installer_options(
    resolver_installer_args: ResolverInstallerArgs,
    build_args: BuildOptionsArgs,
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
        allow_insecure_host,
        resolution,
        prerelease,
        pre,
        config_setting,
        no_build_isolation,
        no_build_isolation_package,
        build_isolation,
        exclude_newer,
        link_mode,
        compile_bytecode,
        no_compile_bytecode,
        no_sources,
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
        upgrade_package: if upgrade_package.is_empty() {
            None
        } else {
            Some(upgrade_package)
        },
        reinstall: flag(reinstall, no_reinstall),
        reinstall_package: if reinstall_package.is_empty() {
            None
        } else {
            Some(reinstall_package)
        },
        index_strategy,
        keyring_provider,
        allow_insecure_host: allow_insecure_host.map(|allow_insecure_host| {
            allow_insecure_host
                .into_iter()
                .filter_map(Maybe::into_option)
                .collect()
        }),
        resolution,
        prerelease: if pre {
            Some(PrereleaseMode::Allow)
        } else {
            prerelease
        },
        config_settings: config_setting
            .map(|config_settings| config_settings.into_iter().collect::<ConfigSettings>()),
        no_build_isolation: flag(no_build_isolation, build_isolation),
        no_build_isolation_package: if no_build_isolation_package.is_empty() {
            None
        } else {
            Some(no_build_isolation_package)
        },
        exclude_newer,
        link_mode,
        compile_bytecode: flag(compile_bytecode, no_compile_bytecode),
        no_build: flag(no_build, build),
        no_build_package: if no_build_package.is_empty() {
            None
        } else {
            Some(no_build_package)
        },
        no_binary: flag(no_binary, binary),
        no_binary_package: if no_binary_package.is_empty() {
            None
        } else {
            Some(no_binary_package)
        },
        no_sources: if no_sources { Some(true) } else { None },
    }
}
