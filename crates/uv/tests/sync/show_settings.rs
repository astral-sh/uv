use std::process::Command;

use assert_fs::prelude::*;
use uv_static::EnvVars;

use uv_test::{capture_uv_snapshot, diff_uv_snapshot, uv_snapshot};

/// Add shared arguments to a command.
///
/// In particular, remove any user-defined environment variables and set any machine-specific
/// environment variables to static values.
fn add_shared_args(mut command: Command) -> Command {
    command
        .env(EnvVars::UV_LINK_MODE, "clone")
        .env(EnvVars::UV_CONCURRENT_DOWNLOADS, "50")
        .env(EnvVars::UV_CONCURRENT_BUILDS, "16")
        .env(EnvVars::UV_CONCURRENT_INSTALLS, "8")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS);

    if cfg!(unix) {
        // Avoid locale issues in tests
        command.env(EnvVars::LC_ALL, "C");
    }
    command
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn pip_compile_baseline() {
    let context = uv_test::test_context!("3.12");

    capture_uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        required_version: None,
        quiet: 0,
        verbose: 0,
        color: Auto,
        network_settings: NetworkSettings {
            connectivity: Online,
            offline: Disabled,
            system_certs: false,
            http_proxy: None,
            https_proxy: None,
            no_proxy: None,
            allow_insecure_host: [],
            read_timeout: [TIME],
            connect_timeout: [TIME],
            retries: 3,
        },
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        show_settings: true,
        preview: Preview {
            flags: [],
        },
        python_preference: Managed,
        python_downloads: Automatic,
        no_progress: false,
        installer_metadata: true,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        format: None,
        src_file: [
            "requirements.in",
        ],
        constraints: [],
        overrides: [],
        excludes: [],
        build_constraints: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        excludes_from_workspace: [],
        build_constraints_from_workspace: [],
        environments: SupportedEnvironments(
            [],
        ),
        required_environments: SupportedEnvironments(
            [],
        ),
        refresh: None(
            Timestamp(
                SystemTime {
                    tv_sec: [TIME],
                    tv_nsec: [TIME],
                },
            ),
        ),
        settings: PipSettings {
            index_locations: IndexLocations {
                indexes: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            install_mirrors: PythonInstallMirrors {
                python_install_mirror: None,
                pypy_install_mirror: None,
                python_downloads_json_url: None,
            },
            system: false,
            extras: ExtrasSpecification(
                ExtrasSpecificationInner {
                    include: Some(
                        [],
                    ),
                    exclude: [],
                    only_extras: false,
                    history: ExtrasSpecificationHistory {
                        extra: [],
                        only_extra: [],
                        no_extra: [],
                        all_extras: false,
                        no_default_extras: false,
                        defaults: List(
                            [],
                        ),
                    },
                },
            ),
            groups: [],
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            torch_backend: None,
            cuda_driver_version: None,
            amd_gpu_architecture: None,
            build_isolation: Isolate,
            extra_build_dependencies: ExtraBuildDependencies(
                {},
            ),
            extra_build_variables: ExtraBuildVariables(
                {},
            ),
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: Highest,
            prerelease: IfNecessaryOrExplicit,
            fork_strategy: RequiresPython,
            dependency_metadata: DependencyMetadata(
                {},
            ),
            output_file: None,
            no_strip_extras: false,
            no_strip_markers: false,
            no_annotate: false,
            no_header: false,
            custom_compile_command: None,
            generate_hashes: false,
            config_setting: ConfigSettings(
                {},
            ),
            config_settings_package: PackageConfigSettings(
                {},
            ),
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: ExcludeNewer {
                global: None,
                package: ExcludeNewerPackage(
                    {},
                ),
            },
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: None,
            hash_checking: Some(
                Verify,
            ),
            upgrade: Upgrade {
                strategy: None,
                constraints: {},
            },
            reinstall: None,
        },
    }

    ----- stderr -----
    "#);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn pip_install_baseline() {
    let context = uv_test::test_context!("3.12");

    capture_uv_snapshot!(context.filters(), add_shared_args(context.pip_install())
        .arg("--show-settings")
        .arg("-r")
        .arg("requirements.in"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        required_version: None,
        quiet: 0,
        verbose: 0,
        color: Auto,
        network_settings: NetworkSettings {
            connectivity: Online,
            offline: Disabled,
            system_certs: false,
            http_proxy: None,
            https_proxy: None,
            no_proxy: None,
            allow_insecure_host: [],
            read_timeout: [TIME],
            connect_timeout: [TIME],
            retries: 3,
        },
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        show_settings: true,
        preview: Preview {
            flags: [],
        },
        python_preference: Managed,
        python_downloads: Automatic,
        no_progress: false,
        installer_metadata: true,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipInstallSettings {
        package: [],
        requirements: [
            "requirements.in",
        ],
        editables: [],
        editable: None,
        constraints: [],
        overrides: [],
        excludes: [],
        build_constraints: [],
        dry_run: Disabled,
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        excludes_from_workspace: [],
        build_constraints_from_workspace: [],
        modifications: Sufficient,
        refresh: None(
            Timestamp(
                SystemTime {
                    tv_sec: [TIME],
                    tv_nsec: [TIME],
                },
            ),
        ),
        settings: PipSettings {
            index_locations: IndexLocations {
                indexes: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            install_mirrors: PythonInstallMirrors {
                python_install_mirror: None,
                pypy_install_mirror: None,
                python_downloads_json_url: None,
            },
            system: false,
            extras: ExtrasSpecification(
                ExtrasSpecificationInner {
                    include: Some(
                        [],
                    ),
                    exclude: [],
                    only_extras: false,
                    history: ExtrasSpecificationHistory {
                        extra: [],
                        only_extra: [],
                        no_extra: [],
                        all_extras: false,
                        no_default_extras: false,
                        defaults: List(
                            [],
                        ),
                    },
                },
            ),
            groups: [],
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            torch_backend: None,
            cuda_driver_version: None,
            amd_gpu_architecture: None,
            build_isolation: Isolate,
            extra_build_dependencies: ExtraBuildDependencies(
                {},
            ),
            extra_build_variables: ExtraBuildVariables(
                {},
            ),
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: Highest,
            prerelease: IfNecessaryOrExplicit,
            fork_strategy: RequiresPython,
            dependency_metadata: DependencyMetadata(
                {},
            ),
            output_file: None,
            no_strip_extras: false,
            no_strip_markers: false,
            no_annotate: false,
            no_header: false,
            custom_compile_command: None,
            generate_hashes: false,
            config_setting: ConfigSettings(
                {},
            ),
            config_settings_package: PackageConfigSettings(
                {},
            ),
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: ExcludeNewer {
                global: None,
                package: ExcludeNewerPackage(
                    {},
                ),
            },
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: None,
            hash_checking: Some(
                Verify,
            ),
            upgrade: Upgrade {
                strategy: None,
                constraints: {},
            },
            reinstall: None,
        },
    }

    ----- stderr -----
    "#);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn lock_baseline() {
    let context = uv_test::test_context!("3.12");

    capture_uv_snapshot!(context.filters(), add_shared_args(context.lock())
        .arg("--show-settings"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        required_version: None,
        quiet: 0,
        verbose: 0,
        color: Auto,
        network_settings: NetworkSettings {
            connectivity: Online,
            offline: Disabled,
            system_certs: false,
            http_proxy: None,
            https_proxy: None,
            no_proxy: None,
            allow_insecure_host: [],
            read_timeout: [TIME],
            connect_timeout: [TIME],
            retries: 3,
        },
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        show_settings: true,
        preview: Preview {
            flags: [],
        },
        python_preference: Managed,
        python_downloads: Automatic,
        no_progress: false,
        installer_metadata: true,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    LockSettings {
        lock_check: Disabled,
        frozen: None,
        dry_run: Disabled,
        script: None,
        python: None,
        install_mirrors: PythonInstallMirrors {
            python_install_mirror: None,
            pypy_install_mirror: None,
            python_downloads_json_url: None,
        },
        refresh: None(
            Timestamp(
                SystemTime {
                    tv_sec: [TIME],
                    tv_nsec: [TIME],
                },
            ),
        ),
        settings: ResolverSettings {
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            config_setting: ConfigSettings(
                {},
            ),
            config_settings_package: PackageConfigSettings(
                {},
            ),
            dependency_metadata: DependencyMetadata(
                {},
            ),
            exclude_newer: ExcludeNewer {
                global: None,
                package: ExcludeNewerPackage(
                    {},
                ),
            },
            fork_strategy: RequiresPython,
            index_locations: IndexLocations {
                indexes: [],
                flat_index: [],
                no_index: false,
            },
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            link_mode: Clone,
            build_isolation: Isolate,
            extra_build_dependencies: ExtraBuildDependencies(
                {},
            ),
            extra_build_variables: ExtraBuildVariables(
                {},
            ),
            prerelease: IfNecessaryOrExplicit,
            resolution: Highest,
            sources: None,
            torch_backend: None,
            cuda_driver_version: None,
            amd_gpu_architecture: None,
            upgrade: Upgrade {
                strategy: None,
                constraints: {},
            },
        },
    }

    ----- stderr -----
    "#);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn version_baseline() {
    let context = uv_test::test_context!("3.12");

    capture_uv_snapshot!(context.filters(), add_shared_args(context.version())
        .arg("--show-settings"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        required_version: None,
        quiet: 0,
        verbose: 0,
        color: Auto,
        network_settings: NetworkSettings {
            connectivity: Online,
            offline: Disabled,
            system_certs: false,
            http_proxy: None,
            https_proxy: None,
            no_proxy: None,
            allow_insecure_host: [],
            read_timeout: [TIME],
            connect_timeout: [TIME],
            retries: 3,
        },
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        show_settings: true,
        preview: Preview {
            flags: [],
        },
        python_preference: Managed,
        python_downloads: Automatic,
        no_progress: false,
        installer_metadata: true,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    VersionSettings {
        value: None,
        bump: [],
        short: false,
        output_format: Text,
        dry_run: false,
        lock_check: Disabled,
        frozen: None,
        active: None,
        no_sync: false,
        package: None,
        python: None,
        install_mirrors: PythonInstallMirrors {
            python_install_mirror: None,
            pypy_install_mirror: None,
            python_downloads_json_url: None,
        },
        refresh: None(
            Timestamp(
                SystemTime {
                    tv_sec: [TIME],
                    tv_nsec: [TIME],
                },
            ),
        ),
        settings: ResolverInstallerSettings {
            resolver: ResolverSettings {
                build_options: BuildOptions {
                    no_binary: None,
                    no_build: None,
                },
                config_setting: ConfigSettings(
                    {},
                ),
                config_settings_package: PackageConfigSettings(
                    {},
                ),
                dependency_metadata: DependencyMetadata(
                    {},
                ),
                exclude_newer: ExcludeNewer {
                    global: None,
                    package: ExcludeNewerPackage(
                        {},
                    ),
                },
                fork_strategy: RequiresPython,
                index_locations: IndexLocations {
                    indexes: [],
                    flat_index: [],
                    no_index: false,
                },
                index_strategy: FirstIndex,
                keyring_provider: Disabled,
                link_mode: Clone,
                build_isolation: Isolate,
                extra_build_dependencies: ExtraBuildDependencies(
                    {},
                ),
                extra_build_variables: ExtraBuildVariables(
                    {},
                ),
                prerelease: IfNecessaryOrExplicit,
                resolution: Highest,
                sources: None,
                torch_backend: None,
                cuda_driver_version: None,
                amd_gpu_architecture: None,
                upgrade: Upgrade {
                    strategy: None,
                    constraints: {},
                },
            },
            compile_bytecode: false,
            reinstall: None,
        },
        malware_settings: MalwareCheckSettings {
            enabled: false,
            malware_check_url: None,
        },
    }

    ----- stderr -----
    "#);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn tool_install_baseline() {
    let context = uv_test::test_context!("3.12");

    capture_uv_snapshot!(context.filters(), add_shared_args(context.tool_install())
        .arg("--show-settings")
        .arg("anyio"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        required_version: None,
        quiet: 0,
        verbose: 0,
        color: Auto,
        network_settings: NetworkSettings {
            connectivity: Online,
            offline: Disabled,
            system_certs: false,
            http_proxy: None,
            https_proxy: None,
            no_proxy: None,
            allow_insecure_host: [],
            read_timeout: [TIME],
            connect_timeout: [TIME],
            retries: 3,
        },
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        show_settings: true,
        preview: Preview {
            flags: [],
        },
        python_preference: Managed,
        python_downloads: Automatic,
        no_progress: false,
        installer_metadata: true,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    ToolInstallSettings {
        package: "anyio",
        from: None,
        with: [],
        with_requirements: [],
        with_executables_from: [],
        with_editable: [],
        constraints: [],
        overrides: [],
        excludes: [],
        build_constraints: [],
        lfs: Disabled,
        python: None,
        python_platform: None,
        refresh: None(
            Timestamp(
                SystemTime {
                    tv_sec: [TIME],
                    tv_nsec: [TIME],
                },
            ),
        ),
        options: ResolverInstallerOptions {
            index: None,
            index_url: None,
            extra_index_url: None,
            no_index: None,
            find_links: None,
            index_strategy: None,
            keyring_provider: None,
            resolution: None,
            prerelease: None,
            fork_strategy: None,
            dependency_metadata: None,
            config_settings: None,
            config_settings_package: None,
            build_isolation: None,
            extra_build_dependencies: None,
            extra_build_variables: None,
            exclude_newer: None,
            exclude_newer_package: None,
            link_mode: Some(
                Clone,
            ),
            torch_backend: None,
            compile_bytecode: None,
            no_sources: None,
            no_sources_package: None,
            upgrade: None,
            reinstall: None,
            no_build: None,
            no_build_package: None,
            no_binary: None,
            no_binary_package: None,
        },
        settings: ResolverInstallerSettings {
            resolver: ResolverSettings {
                build_options: BuildOptions {
                    no_binary: None,
                    no_build: None,
                },
                config_setting: ConfigSettings(
                    {},
                ),
                config_settings_package: PackageConfigSettings(
                    {},
                ),
                dependency_metadata: DependencyMetadata(
                    {},
                ),
                exclude_newer: ExcludeNewer {
                    global: None,
                    package: ExcludeNewerPackage(
                        {},
                    ),
                },
                fork_strategy: RequiresPython,
                index_locations: IndexLocations {
                    indexes: [],
                    flat_index: [],
                    no_index: false,
                },
                index_strategy: FirstIndex,
                keyring_provider: Disabled,
                link_mode: Clone,
                build_isolation: Isolate,
                extra_build_dependencies: ExtraBuildDependencies(
                    {},
                ),
                extra_build_variables: ExtraBuildVariables(
                    {},
                ),
                prerelease: IfNecessaryOrExplicit,
                resolution: Highest,
                sources: None,
                torch_backend: None,
                cuda_driver_version: None,
                amd_gpu_architecture: None,
                upgrade: Upgrade {
                    strategy: None,
                    constraints: {},
                },
            },
            compile_bytecode: false,
            reinstall: None,
        },
        force: false,
        editable: false,
        install_mirrors: PythonInstallMirrors {
            python_install_mirror: None,
            pypy_install_mirror: None,
            python_downloads_json_url: None,
        },
    }

    ----- stderr -----
    "#);
}

/// Read from a `uv.toml` file in the current directory.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_uv_toml() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
    );

    // Write a `uv.toml` file to the directory.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        [pip]
        resolution = "lowest-direct"
        generate-hashes = true
        index-url = "https://pypi.org/simple"
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    // Resolution should use the lowest direct version, and generate hashes.
    let configured = diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
    ...
         ),
         settings: PipSettings {
             index_locations: IndexLocations {
    -            indexes: [],
    +            indexes: [
    +                Index {
    +                    name: None,
    +                    url: Pypi(
    +                        VerbatimUrl {
    +                            url: DisplaySafeUrl {
    +                                scheme: "https",
    +                                cannot_be_a_base: false,
    +                                username: "",
    +                                password: None,
    +                                host: Some(
    +                                    Domain(
    +                                        "pypi.org",
    +                                    ),
    +                                ),
    +                                port: None,
    +                                path: "/simple",
    +                                query: None,
    +                                fragment: None,
    +                            },
    +                            given: Some(
    +                                "https://pypi.org/simple",
    +                            ),
    +                            expanded: false,
    +                        },
    +                    ),
    +                    explicit: false,
    +                    default: true,
    +                    origin: Some(
    +                        Project,
    +                    ),
    +                    format: Simple,
    +                    publish_url: None,
    +                    authenticate: Auto,
    +                    ignore_error_codes: None,
    +                    cache_control: None,
    +                    exclude_newer: None,
    +                },
    +            ],
                 flat_index: [],
                 no_index: false,
             },
    ...
             allow_empty_requirements: false,
             strict: false,
             dependency_mode: Transitive,
    -        resolution: Highest,
    +        resolution: LowestDirect,
             prerelease: IfNecessaryOrExplicit,
             fork_strategy: RequiresPython,
             dependency_metadata: DependencyMetadata(
    ...
             no_annotate: false,
             no_header: false,
             custom_compile_command: None,
    -        generate_hashes: false,
    +        generate_hashes: true,
             config_setting: ConfigSettings(
                 {},
             ),
    ...
    "#
    );

    // Resolution should use the highest version, and generate hashes.
    // Compare against output of the same command without `--resolution=highest`.
    let highest_resolution = diff_uv_snapshot!(context.filters(), &configured, add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
            .arg("--resolution=highest"), @"
    ...
             allow_empty_requirements: false,
             strict: false,
             dependency_mode: Transitive,
    -        resolution: LowestDirect,
    +        resolution: Highest,
             prerelease: IfNecessaryOrExplicit,
             fork_strategy: RequiresPython,
             dependency_metadata: DependencyMetadata(
    ...
    "
    );

    // Resolution should use the highest version, and omit hashes.
    // Compare against output of the same command without `--no-generate-hashes`.
    diff_uv_snapshot!(context.filters(), &highest_resolution, add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
            .arg("--resolution=highest")
            .arg("--no-generate-hashes"), @"
    ...
             no_annotate: false,
             no_header: false,
             custom_compile_command: None,
    -        generate_hashes: true,
    +        generate_hashes: false,
             config_setting: ConfigSettings(
                 {},
             ),
    ...
    "
    );

    Ok(())
}

/// Read from a `pyproject.toml` file in the current directory.
///
/// We prefer `uv.toml` when both are present, but respect `pyproject.toml` otherwise.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_pyproject_toml() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
    );

    // Write a `uv.toml` file to the directory.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        [pip]
        resolution = "lowest-direct"
        generate-hashes = true
        index-url = "https://pypi.org/simple"
    "#})?;

    // Write a `pyproject.toml` file to the directory.
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    // Resolution should use the lowest direct version, and generate hashes.
    let uv_toml = diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
    ...
         ),
         settings: PipSettings {
             index_locations: IndexLocations {
    -            indexes: [],
    +            indexes: [
    +                Index {
    +                    name: None,
    +                    url: Pypi(
    +                        VerbatimUrl {
    +                            url: DisplaySafeUrl {
    +                                scheme: "https",
    +                                cannot_be_a_base: false,
    +                                username: "",
    +                                password: None,
    +                                host: Some(
    +                                    Domain(
    +                                        "pypi.org",
    +                                    ),
    +                                ),
    +                                port: None,
    +                                path: "/simple",
    +                                query: None,
    +                                fragment: None,
    +                            },
    +                            given: Some(
    +                                "https://pypi.org/simple",
    +                            ),
    +                            expanded: false,
    +                        },
    +                    ),
    +                    explicit: false,
    +                    default: true,
    +                    origin: Some(
    +                        Project,
    +                    ),
    +                    format: Simple,
    +                    publish_url: None,
    +                    authenticate: Auto,
    +                    ignore_error_codes: None,
    +                    cache_control: None,
    +                    exclude_newer: None,
    +                },
    +            ],
                 flat_index: [],
                 no_index: false,
             },
    ...
             allow_empty_requirements: false,
             strict: false,
             dependency_mode: Transitive,
    -        resolution: Highest,
    +        resolution: LowestDirect,
             prerelease: IfNecessaryOrExplicit,
             fork_strategy: RequiresPython,
             dependency_metadata: DependencyMetadata(
    ...
             no_annotate: false,
             no_header: false,
             custom_compile_command: None,
    -        generate_hashes: false,
    +        generate_hashes: true,
             config_setting: ConfigSettings(
                 {},
             ),
    ...
    "#
    );

    // Remove the `uv.toml` file.
    fs_err::remove_file(config.path())?;

    // Resolution should use the highest version, and omit hashes.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @"");

    // Add configuration to the `pyproject.toml` file.
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"

        [tool.uv.pip]
        python-platform = "x86_64-unknown-linux-gnu"
        resolution = "lowest-direct"
        generate-hashes = true
        index-url = "https://pypi.org/simple"
    "#})?;

    // Resolution should use the lowest direct version, and generate hashes.
    // Compare against output of the same command before replacing `uv.toml` with `pyproject.toml`.
    diff_uv_snapshot!(context.filters(), &uv_toml, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @"
    ...
                         ),
                         explicit: false,
                         default: true,
    -                    origin: Some(
    -                        Project,
    -                    ),
    +                    origin: None,
                         format: Simple,
                         publish_url: None,
                         authenticate: Auto,
    ...
                 {},
             ),
             python_version: None,
    -        python_platform: None,
    +        python_platform: Some(
    +            X8664UnknownLinuxGnu,
    +        ),
             universal: false,
             exclude_newer: ExcludeNewer {
                 global: None,
    ...
    "
    );

    Ok(())
}

/// Merge index URLs across configuration.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_index_url() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
    );

    // Write a `pyproject.toml` file to the directory.
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"

        [tool.uv.pip]
        index-url = "https://test.pypi.org/simple"
        extra-index-url = ["https://pypi.org/simple"]
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    let configured = diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
    ...
         ),
         settings: PipSettings {
             index_locations: IndexLocations {
    -            indexes: [],
    +            indexes: [
    +                Index {
    +                    name: None,
    +                    url: Pypi(
    +                        VerbatimUrl {
    +                            url: DisplaySafeUrl {
    +                                scheme: "https",
    +                                cannot_be_a_base: false,
    +                                username: "",
    +                                password: None,
    +                                host: Some(
    +                                    Domain(
    +                                        "pypi.org",
    +                                    ),
    +                                ),
    +                                port: None,
    +                                path: "/simple",
    +                                query: None,
    +                                fragment: None,
    +                            },
    +                            given: Some(
    +                                "https://pypi.org/simple",
    +                            ),
    +                            expanded: false,
    +                        },
    +                    ),
    +                    explicit: false,
    +                    default: false,
    +                    origin: None,
    +                    format: Simple,
    +                    publish_url: None,
    +                    authenticate: Auto,
    +                    ignore_error_codes: None,
    +                    cache_control: None,
    +                    exclude_newer: None,
    +                },
    +                Index {
    +                    name: None,
    +                    url: Url(
    +                        VerbatimUrl {
    +                            url: DisplaySafeUrl {
    +                                scheme: "https",
    +                                cannot_be_a_base: false,
    +                                username: "",
    +                                password: None,
    +                                host: Some(
    +                                    Domain(
    +                                        "test.pypi.org",
    +                                    ),
    +                                ),
    +                                port: None,
    +                                path: "/simple",
    +                                query: None,
    +                                fragment: None,
    +                            },
    +                            given: Some(
    +                                "https://test.pypi.org/simple",
    +                            ),
    +                            expanded: false,
    +                        },
    +                    ),
    +                    explicit: false,
    +                    default: true,
    +                    origin: None,
    +                    format: Simple,
    +                    publish_url: None,
    +                    authenticate: Auto,
    +                    ignore_error_codes: None,
    +                    cache_control: None,
    +                    exclude_newer: None,
    +                },
    +            ],
                 flat_index: [],
                 no_index: false,
             },
    ...
    "#
    );

    // Providing an additional index URL on the command-line should be merged with the
    // configuration file.
    // Compare against output of the same command without `--extra-index-url`.
    diff_uv_snapshot!(context.filters(), &configured, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .arg("--extra-index-url")
        .arg("https://test.pypi.org/simple"), @r#"
    ...
                 indexes: [
                     Index {
                         name: None,
    +                    url: Url(
    +                        VerbatimUrl {
    +                            url: DisplaySafeUrl {
    +                                scheme: "https",
    +                                cannot_be_a_base: false,
    +                                username: "",
    +                                password: None,
    +                                host: Some(
    +                                    Domain(
    +                                        "test.pypi.org",
    +                                    ),
    +                                ),
    +                                port: None,
    +                                path: "/simple",
    +                                query: None,
    +                                fragment: None,
    +                            },
    +                            given: Some(
    +                                "https://test.pypi.org/simple",
    +                            ),
    +                            expanded: false,
    +                        },
    +                    ),
    +                    explicit: false,
    +                    default: false,
    +                    origin: Some(
    +                        Cli,
    +                    ),
    +                    format: Simple,
    +                    publish_url: None,
    +                    authenticate: Auto,
    +                    ignore_error_codes: None,
    +                    cache_control: None,
    +                    exclude_newer: None,
    +                },
    +                Index {
    +                    name: None,
                         url: Pypi(
                             VerbatimUrl {
                                 url: DisplaySafeUrl {
    ...
    "#
    );

    Ok(())
}

/// Allow `--find-links` in configuration files.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_find_links() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
    );

    // Write a `pyproject.toml` file to the directory.
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"

        [tool.uv.pip]
        no-index = true
        find-links = ["https://download.pytorch.org/whl/torch_stable.html"]
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("tqdm")?;

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
    ...
         settings: PipSettings {
             index_locations: IndexLocations {
                 indexes: [],
    -            flat_index: [],
    -            no_index: false,
    +            flat_index: [
    +                Index {
    +                    name: None,
    +                    url: Url(
    +                        VerbatimUrl {
    +                            url: DisplaySafeUrl {
    +                                scheme: "https",
    +                                cannot_be_a_base: false,
    +                                username: "",
    +                                password: None,
    +                                host: Some(
    +                                    Domain(
    +                                        "download.pytorch.org",
    +                                    ),
    +                                ),
    +                                port: None,
    +                                path: "/whl/torch_stable.html",
    +                                query: None,
    +                                fragment: None,
    +                            },
    +                            given: Some(
    +                                "https://download.pytorch.org/whl/torch_stable.html",
    +                            ),
    +                            expanded: false,
    +                        },
    +                    ),
    +                    explicit: false,
    +                    default: false,
    +                    origin: None,
    +                    format: Flat,
    +                    publish_url: None,
    +                    authenticate: Auto,
    +                    ignore_error_codes: None,
    +                    cache_control: None,
    +                    exclude_newer: None,
    +                },
    +            ],
    +            no_index: true,
             },
             python: None,
             install_mirrors: PythonInstallMirrors {
    ...
    "#
    );

    Ok(())
}

/// Merge configuration between the top-level `tool.uv` and the more specific `tool.uv.pip`.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_top_level() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
    );

    // Write out to the top-level (`tool.uv`, rather than `tool.uv.pip`).
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"

        [tool.uv]
        resolution = "lowest-direct"
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @"
    ...
             allow_empty_requirements: false,
             strict: false,
             dependency_mode: Transitive,
    -        resolution: Highest,
    +        resolution: LowestDirect,
             prerelease: IfNecessaryOrExplicit,
             fork_strategy: RequiresPython,
             dependency_metadata: DependencyMetadata(
    ...
    "
    );

    // Write out to both the top-level (`tool.uv`) and the pip section (`tool.uv.pip`). The
    // `tool.uv.pip` section should take precedence when combining.
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"

        [tool.uv]
        resolution = "lowest-direct"
        extra-index-url = ["https://test.pypi.org/simple"]

        [tool.uv.pip]
        resolution = "highest"
        extra-index-url = ["https://download.pytorch.org/whl"]
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    let combined = diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
    ...
         ),
         settings: PipSettings {
             index_locations: IndexLocations {
    -            indexes: [],
    +            indexes: [
    +                Index {
    +                    name: None,
    +                    url: Url(
    +                        VerbatimUrl {
    +                            url: DisplaySafeUrl {
    +                                scheme: "https",
    +                                cannot_be_a_base: false,
    +                                username: "",
    +                                password: None,
    +                                host: Some(
    +                                    Domain(
    +                                        "download.pytorch.org",
    +                                    ),
    +                                ),
    +                                port: None,
    +                                path: "/whl",
    +                                query: None,
    +                                fragment: None,
    +                            },
    +                            given: Some(
    +                                "https://download.pytorch.org/whl",
    +                            ),
    +                            expanded: false,
    +                        },
    +                    ),
    +                    explicit: false,
    +                    default: false,
    +                    origin: None,
    +                    format: Simple,
    +                    publish_url: None,
    +                    authenticate: Auto,
    +                    ignore_error_codes: None,
    +                    cache_control: None,
    +                    exclude_newer: None,
    +                },
    +                Index {
    +                    name: None,
    +                    url: Url(
    +                        VerbatimUrl {
    +                            url: DisplaySafeUrl {
    +                                scheme: "https",
    +                                cannot_be_a_base: false,
    +                                username: "",
    +                                password: None,
    +                                host: Some(
    +                                    Domain(
    +                                        "test.pypi.org",
    +                                    ),
    +                                ),
    +                                port: None,
    +                                path: "/simple",
    +                                query: None,
    +                                fragment: None,
    +                            },
    +                            given: Some(
    +                                "https://test.pypi.org/simple",
    +                            ),
    +                            expanded: false,
    +                        },
    +                    ),
    +                    explicit: false,
    +                    default: false,
    +                    origin: None,
    +                    format: Simple,
    +                    publish_url: None,
    +                    authenticate: Auto,
    +                    ignore_error_codes: None,
    +                    cache_control: None,
    +                    exclude_newer: None,
    +                },
    +            ],
                 flat_index: [],
                 no_index: false,
             },
    ...
    "#
    );

    // But the command-line should take precedence over both.
    // Compare against output of the same command without `--resolution=lowest-direct`.
    diff_uv_snapshot!(context.filters(), &combined, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .arg("--resolution=lowest-direct"), @"
    ...
             allow_empty_requirements: false,
             strict: false,
             dependency_mode: Transitive,
    -        resolution: Highest,
    +        resolution: LowestDirect,
             prerelease: IfNecessaryOrExplicit,
             fork_strategy: RequiresPython,
             dependency_metadata: DependencyMetadata(
    ...
    "
    );

    Ok(())
}

/// Verify that user configuration is respected.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_user_configuration() -> anyhow::Result<()> {
    let xdg = assert_fs::TempDir::new().expect("Failed to create temp dir");
    let uv = xdg.child("uv");
    let config = uv.child("uv.toml");
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
            .env(EnvVars::XDG_CONFIG_HOME, xdg.path())
    );

    config.write_str(indoc::indoc! {r#"
        [pip]
        resolution = "lowest-direct"
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    // Resolution should use the lowest direct version.
    let user_configuration = diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .env(EnvVars::XDG_CONFIG_HOME, xdg.path()), @"
    ...
             allow_empty_requirements: false,
             strict: false,
             dependency_mode: Transitive,
    -        resolution: Highest,
    +        resolution: LowestDirect,
             prerelease: IfNecessaryOrExplicit,
             fork_strategy: RequiresPython,
             dependency_metadata: DependencyMetadata(
    ...
    "
    );

    // Add a local configuration to generate hashes.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r"
        [pip]
        generate-hashes = true
    "})?;

    // Resolution should use the lowest direct version and generate hashes.
    // Compare against output of the same command without the local `generate-hashes = true` setting.
    diff_uv_snapshot!(context.filters(), &user_configuration, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .env(EnvVars::XDG_CONFIG_HOME, xdg.path()), @"
    ...
             no_annotate: false,
             no_header: false,
             custom_compile_command: None,
    -        generate_hashes: false,
    +        generate_hashes: true,
             config_setting: ConfigSettings(
                 {},
             ),
    ...
    "
    );

    // Add a local configuration to override the user configuration.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        [pip]
        resolution = "highest"
    "#})?;

    // Resolution should use the highest version.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .env(EnvVars::XDG_CONFIG_HOME, xdg.path()), @"");

    // However, the user-level `tool.uv.pip` settings override the project-level `tool.uv` settings.
    // This is awkward, but we merge the user configuration into the workspace configuration, so
    // the resulting configuration has both `tool.uv.pip.resolution` (from the user configuration)
    // and `tool.uv.resolution` (from the workspace settings), so we choose the former.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        resolution = "highest"
    "#})?;

    // Resolution should use the lowest direct version.
    // Compare against output of the same command without the local top-level
    // `resolution = "highest"` setting.
    diff_uv_snapshot!(context.filters(), &user_configuration, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .env(EnvVars::XDG_CONFIG_HOME, xdg.path()), @""
    );

    Ok(())
}

/// Verify that system configuration can be disabled with `UV_NO_SYSTEM_CONFIG`.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_system_configuration_can_be_disabled() -> anyhow::Result<()> {
    let xdg = assert_fs::TempDir::new().expect("Failed to create temp dir");
    let uv = xdg.child("uv");
    let config = uv.child("uv.toml");
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
            .env(EnvVars::XDG_CONFIG_DIRS, xdg.path())
    );

    config.write_str(indoc::indoc! {r#"
        [pip]
        resolution = "lowest-direct"
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .env(EnvVars::XDG_CONFIG_DIRS, xdg.path())
        .env_remove(EnvVars::UV_NO_SYSTEM_CONFIG), @"
    ...
             allow_empty_requirements: false,
             strict: false,
             dependency_mode: Transitive,
    -        resolution: Highest,
    +        resolution: LowestDirect,
             prerelease: IfNecessaryOrExplicit,
             fork_strategy: RequiresPython,
             dependency_metadata: DependencyMetadata(
    ...
    ");

    diff_uv_snapshot!(
        context.filters(),
        &baseline,
        add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .env(EnvVars::XDG_CONFIG_DIRS, xdg.path())
        .env(EnvVars::UV_NO_SYSTEM_CONFIG, "1"),
        @""
    );

    Ok(())
}

/// When running a user-level command (like `uv tool install`), we should read user configuration,
/// but ignore project-local configuration.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_tool() -> anyhow::Result<()> {
    // Create a temporary directory to store the user configuration.
    let xdg = assert_fs::TempDir::new().expect("Failed to create temp dir");
    let uv = xdg.child("uv");
    let config = uv.child("uv.toml");
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.tool_install())
            .arg("--show-settings")
            .arg("requirements.in")
            .env(EnvVars::XDG_CONFIG_HOME, xdg.path())
    );

    config.write_str(indoc::indoc! {r#"
        resolution = "lowest-direct"
    "#})?;

    // Add a local configuration to disable build isolation.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r"
        no-build-isolation = true
    "})?;

    // If we're running a user-level command, like `uv tool install`, we should use lowest direct,
    // but retain build isolation (since we ignore the local configuration).
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.tool_install())
        .arg("--show-settings")
        .arg("requirements.in")
        .env(EnvVars::XDG_CONFIG_HOME, xdg.path()), @"
    ...
             find_links: None,
             index_strategy: None,
             keyring_provider: None,
    -        resolution: None,
    +        resolution: Some(
    +            LowestDirect,
    +        ),
             prerelease: None,
             fork_strategy: None,
             dependency_metadata: None,
    ...
                     {},
                 ),
                 prerelease: IfNecessaryOrExplicit,
    -            resolution: Highest,
    +            resolution: LowestDirect,
                 sources: None,
                 torch_backend: None,
                 cuda_driver_version: None,
    ...
    "
    );

    Ok(())
}

/// Read from a `pyproject.toml` file in the current directory. In this case, the `pyproject.toml`
/// file uses the Poetry schema.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_poetry_toml() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
    );

    // Write a `uv.toml` file to the directory.
    let config = context.temp_dir.child("pyproject.toml");
    config.write_str(indoc::indoc! {r#"
        [tool.poetry]
        name = "project"
        version = "0.1.0"

        [tool.poetry.dependencies]
        python = "^3.10"
        rich = "^13.7.1"

        [build-system]
        requires = ["poetry-core"]
        build-backend = "poetry.core.masonry.api"

        [tool.uv.pip]
        resolution = "lowest-direct"
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    // Resolution should use the lowest direct version, and generate hashes.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @"
    ...
             allow_empty_requirements: false,
             strict: false,
             dependency_mode: Transitive,
    -        resolution: Highest,
    +        resolution: LowestDirect,
             prerelease: IfNecessaryOrExplicit,
             fork_strategy: RequiresPython,
             dependency_metadata: DependencyMetadata(
    ...
    "
    );

    Ok(())
}

/// Read from both a `uv.toml` and `pyproject.toml` file in the current directory.
///
/// Some fields in `[tool.uv]` are masked by `uv.toml` being defined, and should be warned about.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_both() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
    );

    // Write a `uv.toml` file to the directory.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        [pip]
        resolution = "lowest-direct"
        generate-hashes = true
        index-url = "https://pypi.org/simple"
    "#})?;

    // Write a `pyproject.toml` file to the directory
    let config = context.temp_dir.child("pyproject.toml");
    config.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"

        [tool.uv]
        offline = true
        dev-dependencies = ["pytest"]

        [tool.uv.pip]
        resolution = "highest"
        extra-index-url = ["https://test.pypi.org/simple"]
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    // Resolution should succeed, but warn that the `pip` section in `pyproject.toml` is ignored.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
    ...
         ),
         settings: PipSettings {
             index_locations: IndexLocations {
    -            indexes: [],
    +            indexes: [
    +                Index {
    +                    name: None,
    +                    url: Pypi(
    +                        VerbatimUrl {
    +                            url: DisplaySafeUrl {
    +                                scheme: "https",
    +                                cannot_be_a_base: false,
    +                                username: "",
    +                                password: None,
    +                                host: Some(
    +                                    Domain(
    +                                        "pypi.org",
    +                                    ),
    +                                ),
    +                                port: None,
    +                                path: "/simple",
    +                                query: None,
    +                                fragment: None,
    +                            },
    +                            given: Some(
    +                                "https://pypi.org/simple",
    +                            ),
    +                            expanded: false,
    +                        },
    +                    ),
    +                    explicit: false,
    +                    default: true,
    +                    origin: Some(
    +                        Project,
    +                    ),
    +                    format: Simple,
    +                    publish_url: None,
    +                    authenticate: Auto,
    +                    ignore_error_codes: None,
    +                    cache_control: None,
    +                    exclude_newer: None,
    +                },
    +            ],
                 flat_index: [],
                 no_index: false,
             },
    ...
             allow_empty_requirements: false,
             strict: false,
             dependency_mode: Transitive,
    -        resolution: Highest,
    +        resolution: LowestDirect,
             prerelease: IfNecessaryOrExplicit,
             fork_strategy: RequiresPython,
             dependency_metadata: DependencyMetadata(
    ...
             no_annotate: false,
             no_header: false,
             custom_compile_command: None,
    -        generate_hashes: false,
    +        generate_hashes: true,
             config_setting: ConfigSettings(
                 {},
             ),
    ...
     }

     ----- stderr -----
    +warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    +warning: Found both a `uv.toml` file and a `[tool.uv]` section in an adjacent `pyproject.toml`. The following fields from `[tool.uv]` will be ignored in favor of the `uv.toml` file:
    +- offline
    +- pip
    ...
    "#
    );

    Ok(())
}

/// Read from both a `uv.toml` and `pyproject.toml` file in the current directory.
///
/// But the fields `[tool.uv]` defines aren't allowed in `uv.toml` so there's no warning.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_both_special_fields() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
    );

    // Write a `uv.toml` file to the directory.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        [pip]
        resolution = "lowest-direct"
        generate-hashes = true
        index-url = "https://pypi.org/simple"
    "#})?;

    // Write a `pyproject.toml` file to the directory
    let config = context.temp_dir.child("pyproject.toml");
    config.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"

        [dependency-groups]
        mygroup = ["iniconfig"]

        [tool.uv]
        dev-dependencies = ["pytest"]

        [tool.uv.dependency-groups]
        mygroup = {requires-python = ">=3.12"}
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    // Resolution should succeed, but warn that the `pip` section in `pyproject.toml` is ignored.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
    ...
         ),
         settings: PipSettings {
             index_locations: IndexLocations {
    -            indexes: [],
    +            indexes: [
    +                Index {
    +                    name: None,
    +                    url: Pypi(
    +                        VerbatimUrl {
    +                            url: DisplaySafeUrl {
    +                                scheme: "https",
    +                                cannot_be_a_base: false,
    +                                username: "",
    +                                password: None,
    +                                host: Some(
    +                                    Domain(
    +                                        "pypi.org",
    +                                    ),
    +                                ),
    +                                port: None,
    +                                path: "/simple",
    +                                query: None,
    +                                fragment: None,
    +                            },
    +                            given: Some(
    +                                "https://pypi.org/simple",
    +                            ),
    +                            expanded: false,
    +                        },
    +                    ),
    +                    explicit: false,
    +                    default: true,
    +                    origin: Some(
    +                        Project,
    +                    ),
    +                    format: Simple,
    +                    publish_url: None,
    +                    authenticate: Auto,
    +                    ignore_error_codes: None,
    +                    cache_control: None,
    +                    exclude_newer: None,
    +                },
    +            ],
                 flat_index: [],
                 no_index: false,
             },
    ...
             allow_empty_requirements: false,
             strict: false,
             dependency_mode: Transitive,
    -        resolution: Highest,
    +        resolution: LowestDirect,
             prerelease: IfNecessaryOrExplicit,
             fork_strategy: RequiresPython,
             dependency_metadata: DependencyMetadata(
    ...
             no_annotate: false,
             no_header: false,
             custom_compile_command: None,
    -        generate_hashes: false,
    +        generate_hashes: true,
             config_setting: ConfigSettings(
                 {},
             ),
    ...
     }

     ----- stderr -----
    +warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    ...
    "#
    );

    Ok(())
}

/// Read preview settings from both a `uv.toml` and `pyproject.toml` file in the current directory.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_both_preview() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let uv_config = context.temp_dir.child("uv.toml");
    let pyproject = context.temp_dir.child("pyproject.toml");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.version()).arg("--show-settings")
    );

    pyproject.write_str(indoc::indoc! {r"
        [tool.uv]
        preview = true
    "})?;
    uv_config.write_str(r#"preview-features = ["pylock"]"#)?;

    // The warning should name the ignored `preview` setting.
    // Compare against the baseline.
    let preview_masked = diff_uv_snapshot!(
        context.filters(),
        &baseline,
        add_shared_args(context.version()).arg("--show-settings"),
        @"
    ...
         },
         show_settings: true,
         preview: Preview {
    -        flags: [],
    +        flags: [
    +            Pylock,
    +        ],
         },
         python_preference: Managed,
         python_downloads: Automatic,
    ...
     }

     ----- stderr -----
    +warning: Found both a `uv.toml` file and a `[tool.uv]` section in an adjacent `pyproject.toml`. The following fields from `[tool.uv]` will be ignored in favor of the `uv.toml` file:
    +- preview
    ...
    "
    );

    pyproject.write_str(indoc::indoc! {r#"
        [tool.uv]
        preview-features = ["unknown-preview-feature"]
    "#})?;
    uv_config.write_str("preview = false")?;

    // The warning should name the ignored `preview-features` setting without warning about its
    // unknown feature name.
    // Compare against the inverse spelling combination.
    diff_uv_snapshot!(
        context.filters(),
        &preview_masked,
        add_shared_args(context.version()).arg("--show-settings"),
        @"
    ...
         },
         show_settings: true,
         preview: Preview {
    -        flags: [
    -            Pylock,
    -        ],
    +        flags: [],
         },
         python_preference: Managed,
         python_downloads: Automatic,
    ...

     ----- stderr -----
     warning: Found both a `uv.toml` file and a `[tool.uv]` section in an adjacent `pyproject.toml`. The following fields from `[tool.uv]` will be ignored in favor of the `uv.toml` file:
    -- preview
    +- preview-features
    ...
    "
    );

    Ok(())
}

/// Tests that errors when parsing `conflicts` are reported.
#[test]
fn invalid_conflicts() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject = context.temp_dir.child("pyproject.toml");

    // Write in `pyproject.toml` schema and test the singleton case.
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [{extra = "dev"}],
        ]
    "#})?;

    // The file should be rejected for violating the schema.
    uv_snapshot!(context.filters(), add_shared_args(context.lock()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 7, column 13
      |
    7 | conflicts = [
      |             ^
    Each set of conflicts must have at least two entries, but found only one
    "
    );

    // Now test the empty case.
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [[]]
    "#})?;

    // The file should be rejected for violating the schema.
    uv_snapshot!(context.filters(), add_shared_args(context.lock()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 7, column 13
      |
    7 | conflicts = [[]]
      |             ^^^^
    Each set of conflicts must have at least two entries, but found none
    "
    );

    // Now test the duplicate case.
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [{extra = "dev"}, {extra = "dev"}],
        ]
    "#})?;

    // The file should be rejected for violating the schema.
    uv_snapshot!(context.filters(), add_shared_args(context.lock()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 7, column 13
      |
    7 | conflicts = [
      |             ^
    Each set of conflicts must have at least two entries, but found only one
    "
    );

    // Now test entries that duplicate after applying the default package.
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [{extra = "dev"}, {package = "example", extra = "dev"}],
        ]
    "#})?;

    // The file should be rejected for violating the schema.
    uv_snapshot!(context.filters(), add_shared_args(context.lock()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Each set of conflicts must have at least two entries, but found only one
    "
    );

    Ok(())
}

/// Tests that valid `conflicts` are parsed okay.
#[test]
fn valid_conflicts() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");
    let xdg = assert_fs::TempDir::new().expect("Failed to create temp dir");
    let pyproject = context.temp_dir.child("pyproject.toml");

    // Write in `pyproject.toml` schema.
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [{extra = "x1"}, {extra = "x2"}],
        ]
    "#})?;
    uv_snapshot!(context.filters(), add_shared_args(context.lock())
        .env(EnvVars::XDG_CONFIG_HOME, xdg.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "
    );

    Ok(())
}

/// Read from a `--config-file` command line argument.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_config_file() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    // Write a `uv.toml` to a temporary location. (Use the cache directory for convenience, since
    // it's already obfuscated in the fixtures.)
    let config_dir = &context.cache_dir;
    let config = config_dir.child("uv.toml");
    config.write_str("")?;

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("--config-file")
            .arg(config.path())
            .arg("requirements.in")
    );

    config.write_str(indoc::indoc! {r#"
        [pip]
        resolution = "lowest-direct"
        generate-hashes = true
        index-url = "https://pypi.org/simple"
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("--config-file")
        .arg(config.path())
        .arg("requirements.in"), @r#"
    ...
         ),
         settings: PipSettings {
             index_locations: IndexLocations {
    -            indexes: [],
    +            indexes: [
    +                Index {
    +                    name: None,
    +                    url: Pypi(
    +                        VerbatimUrl {
    +                            url: DisplaySafeUrl {
    +                                scheme: "https",
    +                                cannot_be_a_base: false,
    +                                username: "",
    +                                password: None,
    +                                host: Some(
    +                                    Domain(
    +                                        "pypi.org",
    +                                    ),
    +                                ),
    +                                port: None,
    +                                path: "/simple",
    +                                query: None,
    +                                fragment: None,
    +                            },
    +                            given: Some(
    +                                "https://pypi.org/simple",
    +                            ),
    +                            expanded: false,
    +                        },
    +                    ),
    +                    explicit: false,
    +                    default: true,
    +                    origin: None,
    +                    format: Simple,
    +                    publish_url: None,
    +                    authenticate: Auto,
    +                    ignore_error_codes: None,
    +                    cache_control: None,
    +                    exclude_newer: None,
    +                },
    +            ],
                 flat_index: [],
                 no_index: false,
             },
    ...
             allow_empty_requirements: false,
             strict: false,
             dependency_mode: Transitive,
    -        resolution: Highest,
    +        resolution: LowestDirect,
             prerelease: IfNecessaryOrExplicit,
             fork_strategy: RequiresPython,
             dependency_metadata: DependencyMetadata(
    ...
             no_annotate: false,
             no_header: false,
             custom_compile_command: None,
    -        generate_hashes: false,
    +        generate_hashes: true,
             config_setting: ConfigSettings(
                 {},
             ),
    ...
    "#
    );

    // Write in `pyproject.toml` schema.
    config.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"

        [tool.uv.pip]
        resolution = "lowest-direct"
        generate-hashes = true
        index-url = "https://pypi.org/simple"
    "#})?;

    // The file should be rejected for violating the schema.
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("--config-file")
        .arg(config.path())
        .arg("requirements.in"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `[CACHE_DIR]/uv.toml`
      Caused by: TOML parse error at line 1, column 2
      |
    1 | [project]
      |  ^^^^^^^
    unknown field `project`, expected one of `required-version`, `system-certs`, `native-tls`, `offline`, `no-cache`, `cache-dir`, `preview`, `preview-features`, `python-preference`, `python-downloads`, `concurrent-downloads`, `concurrent-builds`, `concurrent-installs`, `index`, `index-url`, `extra-index-url`, `no-index`, `find-links`, `index-strategy`, `keyring-provider`, `http-proxy`, `https-proxy`, `no-proxy`, `allow-insecure-host`, `resolution`, `prerelease`, `fork-strategy`, `dependency-metadata`, `config-settings`, `config-settings-package`, `no-build-isolation`, `no-build-isolation-package`, `extra-build-dependencies`, `extra-build-variables`, `exclude-newer`, `exclude-newer-package`, `link-mode`, `compile-bytecode`, `no-sources`, `no-sources-package`, `upgrade`, `upgrade-package`, `reinstall`, `reinstall-package`, `no-build`, `no-build-package`, `no-binary`, `no-binary-package`, `torch-backend`, `python-install-mirror`, `pypy-install-mirror`, `python-downloads-json-url`, `publish-url`, `trusted-publishing`, `check-url`, `add-bounds`, `audit`, `pip`, `cache-keys`, `override-dependencies`, `exclude-dependencies`, `constraint-dependencies`, `build-constraint-dependencies`, `environments`, `required-environments`, `conflicts`, `workspace`, `sources`, `managed`, `package`, `default-groups`, `dependency-groups`, `dev-dependencies`, `build-backend`
    "
    );

    // Write an _actual_ `pyproject.toml`.
    let config = config_dir.child("pyproject.toml");
    config.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"

        [tool.uv.pip]
        resolution = "lowest-direct"
        generate-hashes = true
        index-url = "https://pypi.org/simple"
        """#
    })?;

    // The file should be rejected for violating the schema, with a custom warning.
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("--config-file")
        .arg(config.path())
        .arg("requirements.in"), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The `--config-file` argument expects to receive a `uv.toml` file, not a `pyproject.toml`. If you're trying to run a command from another project, use the `--project` argument instead.
    error: Failed to parse: `[CACHE_DIR]/pyproject.toml`
      Caused by: TOML parse error at line 9, column 3
      |
    9 | ""
      |   ^
    key with no value, expected `=`
    "#
    );

    Ok(())
}

/// Ignore empty `pyproject.toml` files when discovering configuration.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_skip_empty() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let child = context.temp_dir.child("child");
    fs_err::create_dir(&child)?;

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
            .current_dir(&child)
    );

    // Set `lowest-direct` in a `uv.toml`.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        [pip]
        resolution = "lowest-direct"
    "#})?;

    // Create an empty in a `pyproject.toml`.
    let pyproject = child.child("pyproject.toml");
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "child"
        dependencies = [
          "httpx",
        ]
    "#})?;

    // Resolution in `child` should use lowest-direct, skipping the `pyproject.toml`, which lacks a
    // `tool.uv`.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .current_dir(&child), @"
    ...
             allow_empty_requirements: false,
             strict: false,
             dependency_mode: Transitive,
    -        resolution: Highest,
    +        resolution: LowestDirect,
             prerelease: IfNecessaryOrExplicit,
             fork_strategy: RequiresPython,
             dependency_metadata: DependencyMetadata(
    ...
    "
    );

    // Adding a `tool.uv` section should cause us to ignore the `uv.toml`.
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "child"
        dependencies = [
          "httpx",
        ]

        [tool.uv]
    "#})?;

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .current_dir(&child), @"");

    Ok(())
}

/// Deserialize an insecure host.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn allow_insecure_host() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
    );

    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        allow-insecure-host = ["google.com", { host = "example.com" }]
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
    ...
             http_proxy: None,
             https_proxy: None,
             no_proxy: None,
    -        allow_insecure_host: [],
    +        allow_insecure_host: [
    +            Host {
    +                scheme: None,
    +                host: "google.com",
    +                port: None,
    +            },
    +            Host {
    +                scheme: None,
    +                host: "example.com",
    +                port: None,
    +            },
    +        ],
             read_timeout: [TIME],
             connect_timeout: [TIME],
             retries: 3,
    ...
    "#
    );

    Ok(())
}

/// Prioritize indexes defined across multiple configuration sources.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn index_priority() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
    );

    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        [[index]]
        url = "https://file.pypi.org/simple"
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    let index_url = diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("requirements.in")
        .arg("--show-settings")
        .arg("--index-url")
        .arg("https://cli.pypi.org/simple"), @r#"
    ...
         ),
         settings: PipSettings {
             index_locations: IndexLocations {
    -            indexes: [],
    +            indexes: [
    +                Index {
    +                    name: None,
    +                    url: Url(
    +                        VerbatimUrl {
    +                            url: DisplaySafeUrl {
    +                                scheme: "https",
    +                                cannot_be_a_base: false,
    +                                username: "",
    +                                password: None,
    +                                host: Some(
    +                                    Domain(
    +                                        "cli.pypi.org",
    +                                    ),
    +                                ),
    +                                port: None,
    +                                path: "/simple",
    +                                query: None,
    +                                fragment: None,
    +                            },
    +                            given: Some(
    +                                "https://cli.pypi.org/simple",
    +                            ),
    +                            expanded: false,
    +                        },
    +                    ),
    +                    explicit: false,
    +                    default: true,
    +                    origin: Some(
    +                        Cli,
    +                    ),
    +                    format: Simple,
    +                    publish_url: None,
    +                    authenticate: Auto,
    +                    ignore_error_codes: None,
    +                    cache_control: None,
    +                    exclude_newer: None,
    +                },
    +                Index {
    +                    name: None,
    +                    url: Url(
    +                        VerbatimUrl {
    +                            url: DisplaySafeUrl {
    +                                scheme: "https",
    +                                cannot_be_a_base: false,
    +                                username: "",
    +                                password: None,
    +                                host: Some(
    +                                    Domain(
    +                                        "file.pypi.org",
    +                                    ),
    +                                ),
    +                                port: None,
    +                                path: "/simple",
    +                                query: None,
    +                                fragment: None,
    +                            },
    +                            given: Some(
    +                                "https://file.pypi.org/simple",
    +                            ),
    +                            expanded: false,
    +                        },
    +                    ),
    +                    explicit: false,
    +                    default: false,
    +                    origin: Some(
    +                        Project,
    +                    ),
    +                    format: Simple,
    +                    publish_url: None,
    +                    authenticate: Auto,
    +                    ignore_error_codes: None,
    +                    cache_control: None,
    +                    exclude_newer: None,
    +                },
    +            ],
                 flat_index: [],
                 no_index: false,
             },
    ...
    "#
    );

    // Compare against output of the same command with `--index-url` instead of `--default-index`.
    diff_uv_snapshot!(context.filters(), &index_url, add_shared_args(context.pip_compile())
        .arg("requirements.in")
        .arg("--show-settings")
        .arg("--default-index")
        .arg("https://cli.pypi.org/simple"), @""
    );

    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        index-url = "https://file.pypi.org/simple"
    "#})?;

    // Prefer the `--default-index` from the CLI, and treat it as the default.
    // Compare against output with `[[index]]` in the config and `--index-url` on the CLI.
    let default_index = diff_uv_snapshot!(context.filters(), &index_url, add_shared_args(context.pip_compile())
        .arg("requirements.in")
        .arg("--show-settings")
        .arg("--default-index")
        .arg("https://cli.pypi.org/simple"), @"
    ...
                             },
                         ),
                         explicit: false,
    -                    default: false,
    +                    default: true,
                         origin: Some(
                             Project,
                         ),
    ...
    "
    );

    // Prefer the `--index` from the CLI, but treat the index from the file as the default.
    // Compare against output of the same command with `--default-index` instead of `--index`.
    diff_uv_snapshot!(context.filters(), &default_index, add_shared_args(context.pip_compile())
        .arg("requirements.in")
        .arg("--show-settings")
        .arg("--index")
        .arg("https://cli.pypi.org/simple"), @"
    ...
                             },
                         ),
                         explicit: false,
    -                    default: true,
    +                    default: false,
                         origin: Some(
                             Cli,
                         ),
    ...
    "
    );

    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        [[index]]
        url = "https://file.pypi.org/simple"
        default = true
    "#})?;

    // Prefer the `--index-url` from the CLI, and treat it as the default.
    // Compare against output with legacy `index-url` in the config and `--default-index` on the CLI.
    let index_url = diff_uv_snapshot!(context.filters(), &default_index, add_shared_args(context.pip_compile())
        .arg("requirements.in")
        .arg("--show-settings")
        .arg("--index-url")
        .arg("https://cli.pypi.org/simple"), @""
    );

    // Prefer the `--extra-index-url` from the CLI, but not as the default.
    // Compare against output of the same command with `--index-url` instead of `--extra-index-url`.
    diff_uv_snapshot!(context.filters(), &index_url, add_shared_args(context.pip_compile())
        .arg("requirements.in")
        .arg("--show-settings")
        .arg("--extra-index-url")
        .arg("https://cli.pypi.org/simple"), @"
    ...
                             },
                         ),
                         explicit: false,
    -                    default: true,
    +                    default: false,
                         origin: Some(
                             Cli,
                         ),
    ...
    "
    );

    Ok(())
}

/// Verify hashes by default.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn verify_hashes() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_install())
            .arg("--show-settings")
            .arg("-r")
            .arg("requirements.in")
    );

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    let default = diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_install())
        .arg("-r")
        .arg("requirements.in")
        .arg("--show-settings"), @"");

    // Compare against output of the same command without `--no-verify-hashes`.
    diff_uv_snapshot!(context.filters(), &default, add_shared_args(context.pip_install())
            .arg("-r")
            .arg("requirements.in")
            .arg("--no-verify-hashes")
            .arg("--show-settings"), @"
    ...
             link_mode: Clone,
             compile_bytecode: false,
             sources: None,
    -        hash_checking: Some(
    -            Verify,
    -        ),
    +        hash_checking: None,
             upgrade: Upgrade {
                 strategy: None,
                 constraints: {},
    ...
    "
    );

    // Compare against output of the same command without `--require-hashes`.
    diff_uv_snapshot!(context.filters(), &default, add_shared_args(context.pip_install())
            .arg("-r")
            .arg("requirements.in")
            .arg("--require-hashes")
            .arg("--show-settings"), @"
    ...
             compile_bytecode: false,
             sources: None,
             hash_checking: Some(
    -            Verify,
    +            Require,
             ),
             upgrade: Upgrade {
                 strategy: None,
    ...
    "
    );

    // Compare against output of the same command without `--no-require-hashes`.
    diff_uv_snapshot!(context.filters(), &default, add_shared_args(context.pip_install())
            .arg("-r")
            .arg("requirements.in")
            .arg("--no-require-hashes")
            .arg("--show-settings"), @"
    ...
             link_mode: Clone,
             compile_bytecode: false,
             sources: None,
    -        hash_checking: Some(
    -            Verify,
    -        ),
    +        hash_checking: None,
             upgrade: Upgrade {
                 strategy: None,
                 constraints: {},
    ...
    "
    );

    // Compare against output of the same command without `UV_NO_VERIFY_HASHES=1`.
    diff_uv_snapshot!(context.filters(), &default, add_shared_args(context.pip_install())
            .arg("-r")
            .arg("requirements.in")
            .env(EnvVars::UV_NO_VERIFY_HASHES, "1")
            .arg("--show-settings"), @"
    ...
             link_mode: Clone,
             compile_bytecode: false,
             sources: None,
    -        hash_checking: Some(
    -            Verify,
    -        ),
    +        hash_checking: None,
             upgrade: Upgrade {
                 strategy: None,
                 constraints: {},
    ...
    "
    );

    // Compare against output without `--verify-hashes` and `--no-require-hashes`.
    diff_uv_snapshot!(
        context.filters(),
        &default,
        add_shared_args(context.pip_install())
            .arg("-r")
            .arg("requirements.in")
            .arg("--verify-hashes")
            .arg("--no-require-hashes")
            .arg("--show-settings"),
        @""
    );

    Ok(())
}

/// Test preview feature flagging.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn preview_features() {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.version()).arg("--show-settings")
    );

    let preview = diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.version()).arg("--show-settings").arg("--preview"), @"
    ...
         },
         show_settings: true,
         preview: Preview {
    -        flags: [],
    +        flags: [
    +            PythonInstallDefault,
    +            PythonUpgrade,
    +            JsonOutput,
    +            Pylock,
    +            AddBounds,
    +            PackageConflicts,
    +            ExtraBuildDependencies,
    +            DetectModuleConflicts,
    +            Format,
    +            NativeAuth,
    +            S3Endpoint,
    +            CacheSize,
    +            InitProjectFlag,
    +            WorkspaceMetadata,
    +            WorkspaceDir,
    +            WorkspaceList,
    +            SbomExport,
    +            AuthHelper,
    +            DirectPublish,
    +            TargetWorkspaceDiscovery,
    +            MetadataJson,
    +            GcsEndpoint,
    +            AdjustUlimit,
    +            SpecialCondaEnvNames,
    +            RelocatableEnvsDefault,
    +            PublishRequireNormalized,
    +            Audit,
    +            ProjectDirectoryMustExist,
    +            IndexExcludeNewer,
    +            AzureEndpoint,
    +            TomlBackwardsCompatibility,
    +            MalwareCheck,
    +            VenvSafeClear,
    +            Check,
    +            PackagedInit,
    +            CentralizedProjectEnvs,
    +        ],
         },
         python_preference: Managed,
         python_downloads: Automatic,
    ...
    "
    );

    diff_uv_snapshot!(
        context.filters(),
        &baseline,
        add_shared_args(context.version()).arg("--show-settings").arg("--preview").arg("--no-preview"),
        @""
    );

    // Compare against output of `--preview` alone.
    diff_uv_snapshot!(context.filters(), &preview, add_shared_args(context.version()).arg("--show-settings").arg("--preview").arg("--preview-features").arg("python-install-default"), @""
    );

    let preview_features = diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.version()).arg("--show-settings").arg("--preview-features").arg("python-install-default,python-upgrade"), @"
    ...
         },
         show_settings: true,
         preview: Preview {
    -        flags: [],
    +        flags: [
    +            PythonInstallDefault,
    +            PythonUpgrade,
    +        ],
         },
         python_preference: Managed,
         python_downloads: Automatic,
    ...
    "
    );

    diff_uv_snapshot!(
        context.filters(),
        &preview_features,
        add_shared_args(context.version())
            .arg("--show-settings")
            .arg("--preview-features")
            .arg("python-install-default,unknown-preview-feature,python-upgrade"),
        @"
    ...
     }

     ----- stderr -----
    +warning: Unknown preview feature: `unknown-preview-feature`
    ...
    "
    );

    diff_uv_snapshot!(
        context.filters(),
        &preview_features,
        add_shared_args(context.version())
            .arg("--show-settings")
            .env(
                EnvVars::UV_PREVIEW_FEATURES,
                "python-install-default,unknown-preview-feature,python-upgrade",
            ),
        @"
    ...
     }

     ----- stderr -----
    +warning: Unknown preview feature: `unknown-preview-feature`
    ...
    "
    );

    // Compare against output with both features passed to one `--preview-features` option.
    diff_uv_snapshot!(context.filters(), &preview_features, add_shared_args(context.version()).arg("--show-settings").arg("--preview-features").arg("python-install-default").arg("--preview-feature").arg("python-upgrade"), @""
    );

    diff_uv_snapshot!(
        context.filters(),
        &baseline,
        add_shared_args(context.version()).arg("--show-settings")
        .arg("--preview-features").arg("python-install-default").arg("--preview-feature").arg("python-upgrade")
        .arg("--no-preview"),
        @""
    );

    uv_snapshot!(
        context.filters(),
        add_shared_args(context.version())
            .arg("--show-settings")
            .arg("--preview-features")
            .arg("python-install-default,,python-upgrade"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '' for '--preview-features <PREVIEW_FEATURES>': preview feature name cannot be empty

    For more information, try '--help'.
    "
    );

    uv_snapshot!(
        context.filters(),
        add_shared_args(context.version())
            .arg("--show-settings")
            .env(
                EnvVars::UV_PREVIEW_FEATURES,
                "python-install-default,,python-upgrade",
            ),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '' for '--preview-features <PREVIEW_FEATURES>': preview feature name cannot be empty

    For more information, try '--help'.
    "
    );
}

/// Test preview precedence across configuration, environment, and CLI settings.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn preview_precedence() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let show_settings = || {
        let mut cmd = context.version();
        cmd.arg("--show-settings");
        add_shared_args(cmd)
    };

    let disabled = capture_uv_snapshot!(context.filters(), show_settings());

    let enabled = capture_uv_snapshot!(context.filters(), show_settings().arg("--preview"));

    let user_config_dir = context.user_config_dir.child("uv");
    user_config_dir.create_dir_all()?;
    let user_config = user_config_dir.child("uv.toml");
    user_config.write_str("preview = true")?;

    // `preview = true` in user `uv.toml` should be the same as passing `--preview`.
    diff_uv_snapshot!(
        context.filters(),
        &enabled,
        show_settings(),
        @""
    );

    let project_config = context.temp_dir.child("pyproject.toml");
    project_config.write_str(indoc::indoc! {r"
        [tool.uv]
        preview = false
    "})?;

    // `preview = false` in `pyproject.toml` should mask user `uv.toml`.
    diff_uv_snapshot!(
        context.filters(),
        &disabled,
        show_settings(),
        @""
    );

    // `--preview-features` should mask `preview = false` in a config file.
    diff_uv_snapshot!(
        context.filters(),
        &disabled,
        show_settings().arg("--preview-features").arg("pylock"),
        @"
    ...
         },
         show_settings: true,
         preview: Preview {
    -        flags: [],
    +        flags: [
    +            Pylock,
    +        ],
         },
         python_preference: Managed,
         python_downloads: Automatic,
    ...
    "
    );

    user_config.write_str("preview = false")?;

    project_config.write_str(indoc::indoc! {r"
        [tool.uv]
        preview = true
    "})?;

    // `preview = true` in a config file should mask `--preview-features`.
    diff_uv_snapshot!(
        context.filters(),
        &enabled,
        show_settings().arg("--preview-features").arg("pylock"),
        @""
    );

    // `UV_PREVIEW=false` should fall through to configuration.
    // Compare against `--preview`.
    diff_uv_snapshot!(
        context.filters(),
        &enabled,
        show_settings().env(EnvVars::UV_PREVIEW, "0"),
        @""
    );

    project_config.write_str("")?;
    user_config.write_str("")?;

    // `UV_PREVIEW=true` should override any explicit CLI feature list.
    diff_uv_snapshot!(
        context.filters(),
        &enabled,
        show_settings()
            .arg("--preview-features")
            .arg("pylock")
            .env(EnvVars::UV_PREVIEW, "1"),
        @""
    );

    // `--no-preview` should override `UV_PREVIEW=true`.
    diff_uv_snapshot!(
        context.filters(),
        &disabled,
        show_settings()
            .arg("--no-preview")
            .env(EnvVars::UV_PREVIEW, "1"),
        @""
    );

    user_config.write_str("preview = true")?;
    project_config.write_str(indoc::indoc! {r#"
        [tool.uv]
        preview-features = ["format-command"]
    "#})?;

    // The project setting atomically overrides the differently-spelled user setting.
    // Compare against the disabled baseline.
    let project = diff_uv_snapshot!(
        context.filters(),
        &disabled,
        show_settings(),
        @"
    ...
         },
         show_settings: true,
         preview: Preview {
    -        flags: [],
    +        flags: [
    +            Format,
    +        ],
         },
         python_preference: Managed,
         python_downloads: Automatic,
    ...
    "
    );

    user_config.write_str(r#"preview-features = ["unknown-preview-feature"]"#)?;

    // An unknown lower-priority setting is masked without warning.
    // Compare against the recognized project setting.
    diff_uv_snapshot!(
        context.filters(),
        &project,
        show_settings(),
        @""
    );

    user_config.write_str("preview = true")?;
    project_config.write_str(indoc::indoc! {r"
        [tool.uv]
        preview-features = false
    "})?;

    // An explicit disabled project setting atomically overrides the enabled user setting.
    // Compare against the disabled baseline.
    diff_uv_snapshot!(
        context.filters(),
        &disabled,
        show_settings(),
        @""
    );

    project_config.write_str(indoc::indoc! {r#"
        [tool.uv]
        preview-features = ["format-command"]
    "#})?;

    // An explicit CLI feature list should override the project feature list.
    // Compare against the project feature list.
    let cli_features = diff_uv_snapshot!(
        context.filters(),
        &project,
        show_settings().arg("--preview-features").arg("pylock"),
        @"
    ...
         show_settings: true,
         preview: Preview {
             flags: [
    -            Format,
    +            Pylock,
             ],
         },
         python_preference: Managed,
    ...
    "
    );

    project_config.write_str(indoc::indoc! {r"
        [tool.uv]
        preview-features = false
    "})?;

    // An explicit CLI feature list should override the disabled project setting.
    // Compare against the explicit CLI feature list.
    diff_uv_snapshot!(
        context.filters(),
        &cli_features,
        show_settings().arg("--preview-features").arg("pylock"),
        @""
    );

    project_config.write_str(indoc::indoc! {r#"
        [tool.uv]
        preview-features = ["unknown-preview-feature"]
    "#})?;

    // An unknown-only project setting atomically overrides the enabled user setting.
    // Compare against the disabled baseline.
    diff_uv_snapshot!(
        context.filters(),
        &disabled,
        show_settings(),
        @"
    ...
     }

     ----- stderr -----
    +warning: Unknown preview feature: `unknown-preview-feature`
    ...
    "
    );

    // An explicit CLI setting masks the unknown project setting without warning.
    // Compare against the explicit CLI feature list.
    diff_uv_snapshot!(
        context.filters(),
        &cli_features,
        show_settings().arg("--preview-features").arg("pylock"),
        @""
    );

    // `--preview` masks the unknown project setting without warning.
    // Compare against `--preview`.
    diff_uv_snapshot!(
        context.filters(),
        &enabled,
        show_settings().arg("--preview"),
        @""
    );

    project_config.write_str(indoc::indoc! {r"
        [tool.uv]
        preview-features = true
    "})?;

    // Both enable-all spellings have the same precedence.
    // Compare against `--preview`.
    diff_uv_snapshot!(
        context.filters(),
        &enabled,
        show_settings().arg("--preview-features").arg("pylock"),
        @""
    );

    Ok(())
}

/// Test `preview` and `preview-features` parsing in `uv.toml`.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn preview_features_uv_toml() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let config = context.temp_dir.child("uv.toml");

    let enabled = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.version())
            .arg("--show-settings")
            .arg("--preview")
    );

    let disabled = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.version()).arg("--show-settings")
    );

    config.write_str(r#"preview-features = ["format-command"]"#)?;

    // A feature list should enable only the named preview features.
    // Compare against the disabled baseline.
    diff_uv_snapshot!(
        context.filters(),
        &disabled,
        add_shared_args(context.version()).arg("--show-settings"),
        @"
    ...
         },
         show_settings: true,
         preview: Preview {
    -        flags: [],
    +        flags: [
    +            Format,
    +        ],
         },
         python_preference: Managed,
         python_downloads: Automatic,
    ...
    "
    );

    config.write_str("preview-features = true")?;

    // `preview-features = true` should enable all preview features.
    // Compare against `--preview`.
    diff_uv_snapshot!(
        context.filters(),
        &enabled,
        add_shared_args(context.version()).arg("--show-settings"),
        @""
    );

    config.write_str("preview-features = false")?;

    // `preview-features = false` should be equivalent to `preview = false`.
    // Compare against the disabled baseline.
    diff_uv_snapshot!(
        context.filters(),
        &disabled,
        add_shared_args(context.version()).arg("--show-settings"),
        @""
    );

    config.write_str("preview-features = []")?;

    // An empty feature list should not enable any preview features.
    // Compare against the disabled baseline.
    diff_uv_snapshot!(
        context.filters(),
        &disabled,
        add_shared_args(context.version()).arg("--show-settings"),
        @""
    );

    config.write_str(indoc::indoc! {r#"
        preview = true
        preview-features = ["format-command"]
    "#})?;

    // The two settings should be rejected when used together.
    uv_snapshot!(context.filters(), add_shared_args(context.version()).arg("--show-settings"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `uv.toml`
      Caused by: cannot specify both `preview` and `preview-features`
    ");

    config.write_str(r#"preview-features = ["unknown-preview-feature"]"#)?;

    // Unknown names should warn without enabling any recognized preview features.
    // Compare against the disabled baseline.
    diff_uv_snapshot!(
        context.filters(),
        &disabled,
        add_shared_args(context.version()).arg("--show-settings"),
        @"
    ...
     }

     ----- stderr -----
    +warning: Unknown preview feature: `unknown-preview-feature`
    ...
    "
    );

    config.write_str(r#"preview-features = ["  "]"#)?;

    // Empty preview feature names should be rejected.
    uv_snapshot!(context.filters(), add_shared_args(context.version()).arg("--show-settings"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `uv.toml`
      Caused by: TOML parse error at line 1, column 20
      |
    1 | preview-features = [\"  \"]
      |                    ^^^^^^
    preview feature name cannot be empty
    ");

    config.write_str("preview-features = 123")?;

    // Invalid preview feature types should be rejected.
    uv_snapshot!(context.filters(), add_shared_args(context.version()).arg("--show-settings"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `uv.toml`
      Caused by: TOML parse error at line 1, column 20
      |
    1 | preview-features = 123
      |                    ^^^
    invalid type: integer `123`, expected a boolean or a list of preview feature names
    ");

    Ok(())
}

/// Test preview setting discovery and diagnostics in `pyproject.toml`.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn preview_features_pyproject_toml() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject = context.temp_dir.child("pyproject.toml");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.version()).arg("--show-settings")
    );

    pyproject.write_str(indoc::indoc! {r#"
        [tool.uv]
        preview-features = ["format-command"]
    "#})?;

    // A feature list should enable only the named preview features.
    // Compare against the baseline.
    diff_uv_snapshot!(
        context.filters(),
        &baseline,
        add_shared_args(context.version()).arg("--show-settings"),
        @"
    ...
         },
         show_settings: true,
         preview: Preview {
    -        flags: [],
    +        flags: [
    +            Format,
    +        ],
         },
         python_preference: Managed,
         python_downloads: Automatic,
    ...
    "
    );

    pyproject.write_str(indoc::indoc! {r#"
        [tool.uv]
        preview = true
        preview-features = ["format-command"]
    "#})?;

    // Conflicting preview settings should be ignored during settings discovery with a warning.
    // Compare against the baseline.
    diff_uv_snapshot!(
        context.filters(),
        &baseline,
        add_shared_args(context.version()).arg("--show-settings"),
        @"
    ...
     }

     ----- stderr -----
    +warning: Failed to parse `pyproject.toml` during settings discovery:
    +  TOML parse error at line 1, column 1
    +    |
    +  1 | [tool.uv]
    +    | ^^^^^^^^^
    +  cannot specify both `preview` and `preview-features`
    +
    ...
    "
    );

    pyproject.write_str(indoc::indoc! {r"
        [tool.uv]
        preview-features = 123
    "})?;

    // Invalid preview feature types should be ignored during settings discovery with a warning.
    // Compare against the baseline.
    diff_uv_snapshot!(
        context.filters(),
        &baseline,
        add_shared_args(context.version()).arg("--show-settings"),
        @"
    ...
     }

     ----- stderr -----
    +warning: Failed to parse `pyproject.toml` during settings discovery:
    +  TOML parse error at line 2, column 20
    +    |
    +  2 | preview-features = 123
    +    |                    ^^^
    +  invalid type: integer `123`, expected a boolean or a list of preview feature names
    +
    ...
    "
    );

    Ok(())
}

/// Test PEP 723-specific preview parsing, precedence, and diagnostics.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn run_pep723_script_preview_features() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let show_settings = || {
        let mut cmd = context.run();
        cmd.arg("--show-settings").arg("main.py");
        cmd
    };

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc::indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = []
        # ///

        print("hello")
       "#
    })?;

    let baseline = capture_uv_snapshot!(context.filters(), show_settings());

    let enabled = capture_uv_snapshot!(
        context.filters(),
        context
            .run()
            .arg("--show-settings")
            .arg("--preview")
            .arg("main.py")
    );

    test_script.write_str(indoc::indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = []
        #
        # [tool.uv]
        # preview-features = ["format-command"]
        # ///

        print("hello")
       "#
    })?;

    let preview_features = diff_uv_snapshot!(
        context.filters(),
        &baseline,
        show_settings(),
        @"
    ...
         },
         show_settings: true,
         preview: Preview {
    -        flags: [],
    +        flags: [
    +            Format,
    +        ],
         },
         python_preference: Managed,
         python_downloads: Never,
    ...
    "
    );

    test_script.write_str(indoc::indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = []
        #
        # [tool.uv]
        # preview-features = ["format-command", "unknown-preview-feature"]
        # ///

        print("hello")
       "#
    })?;

    // Unknown names should warn without changing the recognized feature set.
    // Compare against the recognized feature list to isolate the warning.
    diff_uv_snapshot!(
        context.filters(),
        &preview_features,
        show_settings(),
        @"
    ...
     }

     ----- stderr -----
    +warning: Unknown preview feature: `unknown-preview-feature`
    ...
    "
    );

    test_script.write_str(indoc::indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = []
        #
        # [tool.uv]
        # preview = true
        # ///

        print("hello")
       "#
    })?;

    // The legacy `preview = true` setting should enable all preview features.
    // Compare against `--preview`.
    diff_uv_snapshot!(
        context.filters(),
        &enabled,
        show_settings(),
        @""
    );

    test_script.write_str(indoc::indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = []
        #
        # [tool.uv]
        # preview-features = [123]
        # ///

        print("hello")
       "#
    })?;

    // The diagnostic should reject a non-string preview feature name.
    uv_snapshot!(context.filters(), show_settings(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: TOML parse error at line 4, column 1
      |
    4 | [tool.uv]
      | ^^^^^^^^^
    invalid type: integer `123`, expected a string
    ");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc::indoc! {r"
            [tool.uv]
            preview = true
        "})?;
    test_script.write_str(indoc::indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = []
        #
        # [tool.uv]
        # preview-features = false
        # ///

        print("hello")
       "#
    })?;

    // The script setting should atomically override the project setting.
    assert_eq!(
        baseline,
        capture_uv_snapshot!(context.filters(), show_settings())
    );

    test_script.write_str(indoc::indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = []
        #
        # [tool.uv]
        # preview = true
        # preview-features = ["format-command"]
        # ///

        print("hello")
       "#
    })?;

    // The two settings should be rejected when used together.
    uv_snapshot!(context.filters(), show_settings(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: TOML parse error at line 4, column 1
      |
    4 | [tool.uv]
      | ^^^^^^^^^
    cannot specify both `preview` and `preview-features`
    ");

    Ok(())
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn system_certs_cli_aliases_override_env() {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.version()).arg("--show-settings")
    );

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.version())
        .arg("--show-settings")
        .arg("--no-native-tls")
        .env(EnvVars::UV_SYSTEM_CERTS, "1"), @"
    ...
     }

     ----- stderr -----
    +warning: The `--no-native-tls` flag is deprecated and will be removed in a future release. Use `--no-system-certs` instead.
    ...
    "
    );

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.version())
        .arg("--show-settings")
        .arg("--no-system-certs")
        .env(EnvVars::UV_NATIVE_TLS, "1"), @"
    ...
     }

     ----- stderr -----
    +warning: The `UV_NATIVE_TLS` environment variable is deprecated and will be removed in a future release. Use `UV_SYSTEM_CERTS` instead.
    ...
    "
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn system_certs_config_aliases() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.version()).arg("--show-settings")
    );

    let config = context.temp_dir.child("uv.toml");
    config.write_str("system-certs = true\n")?;

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.version())
        .arg("--show-settings"), @"
    ...
         network_settings: NetworkSettings {
             connectivity: Online,
             offline: Disabled,
    -        system_certs: false,
    +        system_certs: true,
             http_proxy: None,
             https_proxy: None,
             no_proxy: None,
    ...
    "
    );

    config.write_str(indoc::indoc! {r"
        system-certs = false
        native-tls = true
    "})?;

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.version())
        .arg("--show-settings"), @"
    ...
     }

     ----- stderr -----
    +warning: The `native-tls` setting is deprecated and will be removed in a future release. Use `system-certs` instead.
    ...
    "
    );

    Ok(())
}

/// Track the interactions between `upgrade` and `upgrade-package` across the `uv pip` CLI and a
/// configuration file.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn upgrade_pip_cli_config_interaction() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
    );

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    // `--no-upgrade` overrides `--upgrade-package`.
    // TODO(charlie): This should mark `sniffio` for upgrade, but it doesn't.
    let no_upgrade = diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--no-upgrade")
        .arg("--upgrade-package")
        .arg("sniffio")
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
    ...
                 Verify,
             ),
             upgrade: Upgrade {
    -            strategy: None,
    +            strategy: Some(
    +                {
    +                    PackageName(
    +                        "sniffio",
    +                    ),
    +                },
    +                {},
    +            ),
                 constraints: {},
             },
             reinstall: None,
    ...
    "#
    );

    // Write a `uv.toml` file to the directory.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r"
        [pip]
        upgrade = false
    "})?;

    // Despite `upgrade = false` in the configuration file, we should mark `idna` for upgrade.
    // Compare against output before adding `upgrade = false`, with `--no-upgrade --upgrade-package sniffio`.
    diff_uv_snapshot!(
        context.filters(),
        &no_upgrade,
        add_shared_args(context.pip_compile())
            .arg("--upgrade-package")
            .arg("idna")
            .arg("--show-settings")
            .arg("requirements.in"),
        @r#"
    ...
                 Verify,
             ),
             upgrade: Upgrade {
    -            strategy: Some(
    -                {
    -                    PackageName(
    -                        "sniffio",
    -                    ),
    -                },
    -                {},
    -            ),
    +            strategy: None,
                 constraints: {},
             },
             reinstall: None,
    ...
    "#
    );

    // Write a `uv.toml` file to the directory.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r"
        [pip]
        upgrade = true
    "})?;

    // Despite `--upgrade-package idna` in the command line, we should upgrade all packages.
    // Compare against output before adding `upgrade = true`, with `--no-upgrade --upgrade-package sniffio`.
    diff_uv_snapshot!(context.filters(), &no_upgrade, add_shared_args(context.pip_compile())
            .arg("--upgrade-package")
            .arg("idna")
            .arg("--show-settings")
            .arg("requirements.in"), @r#"
    ...
                 Verify,
             ),
             upgrade: Upgrade {
    -            strategy: Some(
    -                {
    -                    PackageName(
    -                        "sniffio",
    -                    ),
    -                },
    -                {},
    -            ),
    +            strategy: All,
                 constraints: {},
             },
             reinstall: None,
    ...
    "#
    );

    // Write a `uv.toml` file to the directory.
    config.write_str(indoc::indoc! {r#"
        [pip]
        upgrade-package = ["idna"]
    "#})?;

    // Despite `upgrade-package = ["idna"]` in the configuration file, we should disable upgrades.
    // Compare against output before adding `upgrade-package = ["idna"]`, with `--upgrade-package sniffio`.
    diff_uv_snapshot!(context.filters(), &no_upgrade, add_shared_args(context.pip_compile())
            .arg("--no-upgrade")
            .arg("--show-settings")
            .arg("requirements.in"), @r#"
    ...
                 strategy: Some(
                     {
                         PackageName(
    -                        "sniffio",
    +                        "idna",
                         ),
                     },
                     {},
    ...
    "#
    );

    // Despite `upgrade-package = ["idna"]` in the configuration file, we should enable all upgrades.
    // Compare against output before adding `upgrade-package = ["idna"]`, with `--no-upgrade --upgrade-package sniffio`.
    diff_uv_snapshot!(context.filters(), &no_upgrade, add_shared_args(context.pip_compile())
            .arg("--upgrade")
            .arg("--show-settings")
            .arg("requirements.in"), @r#"
    ...
                 strategy: Some(
                     {
                         PackageName(
    -                        "sniffio",
    +                        "idna",
                         ),
                     },
                     {},
    ...
    "#
    );

    // Mark both `sniffio` and `idna` for upgrade.
    // Compare against output before adding `upgrade-package = ["idna"]`, with `--no-upgrade`.
    diff_uv_snapshot!(context.filters(), &no_upgrade, add_shared_args(context.pip_compile())
            .arg("--upgrade-package")
            .arg("sniffio")
            .arg("--show-settings")
            .arg("requirements.in"), @r#"
    ...
                         PackageName(
                             "sniffio",
                         ),
    +                    PackageName(
    +                        "idna",
    +                    ),
                     },
                     {},
                 ),
    ...
    "#
    );

    Ok(())
}

/// Track the interactions between `upgrade` and `upgrade-package` across the project CLI and a
/// configuration file.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn upgrade_project_cli_config_interaction() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.lock()).arg("--show-settings")
    );

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc::indoc! {r#"
        [project]
        name = "foo"
        version = "0.0.0"
        dependencies = ["anyio>3.0.0"]
    "#})?;

    // `--no-upgrade` overrides `--upgrade-package`.
    // TODO(charlie): This should mark `sniffio` for upgrade, but it doesn't.
    let no_upgrade = diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.lock())
        .arg("--no-upgrade")
        .arg("--upgrade-package")
        .arg("sniffio")
        .arg("--show-settings"), @r#"
    ...
             cuda_driver_version: None,
             amd_gpu_architecture: None,
             upgrade: Upgrade {
    -            strategy: None,
    +            strategy: Some(
    +                {
    +                    PackageName(
    +                        "sniffio",
    +                    ),
    +                },
    +                {},
    +            ),
                 constraints: {},
             },
         },
    ...
    "#
    );

    // Add `upgrade = false` to the configuration file.
    pyproject_toml.write_str(indoc::indoc! {r#"
        [project]
        name = "foo"
        version = "0.0.0"
        dependencies = ["anyio>3.0.0"]

        [tool.uv]
        upgrade = false
    "#})?;

    // Despite `upgrade = false` in the configuration file, we should mark `idna` for upgrade.
    // Compare against output before adding `upgrade = false`, with `--no-upgrade --upgrade-package sniffio`.
    diff_uv_snapshot!(
        context.filters(),
        &no_upgrade,
        add_shared_args(context.lock())
            .arg("--upgrade-package")
            .arg("idna")
            .arg("--show-settings"),
        @r#"
    ...
             cuda_driver_version: None,
             amd_gpu_architecture: None,
             upgrade: Upgrade {
    -            strategy: Some(
    -                {
    -                    PackageName(
    -                        "sniffio",
    -                    ),
    -                },
    -                {},
    -            ),
    +            strategy: None,
                 constraints: {},
             },
         },
    ...
    "#
    );

    // Add `upgrade = true` to the configuration file.
    pyproject_toml.write_str(indoc::indoc! {r#"
        [project]
        name = "foo"
        version = "0.0.0"
        dependencies = ["anyio>3.0.0"]

        [tool.uv]
        upgrade = true
    "#})?;

    // Despite `--upgrade-package idna` on the CLI, we should upgrade all packages.
    // Compare against output before adding `upgrade = true`, with `--no-upgrade --upgrade-package sniffio`.
    diff_uv_snapshot!(context.filters(), &no_upgrade, add_shared_args(context.lock())
            .arg("--upgrade-package")
            .arg("idna")
            .arg("--show-settings"), @r#"
    ...
             cuda_driver_version: None,
             amd_gpu_architecture: None,
             upgrade: Upgrade {
    -            strategy: Some(
    -                {
    -                    PackageName(
    -                        "sniffio",
    -                    ),
    -                },
    -                {},
    -            ),
    +            strategy: All,
                 constraints: {},
             },
         },
    ...
    "#
    );

    pyproject_toml.write_str(indoc::indoc! {r#"
        [project]
        name = "foo"
        version = "0.0.0"
        dependencies = ["anyio>3.0.0"]

        [tool.uv]
        upgrade-package = ["idna"]
    "#})?;

    // Despite `upgrade-package = ["idna"]` in the configuration file, we should disable upgrades.
    // Compare against output before adding `upgrade-package = ["idna"]`, with `--upgrade-package sniffio`.
    diff_uv_snapshot!(context.filters(), &no_upgrade, add_shared_args(context.lock())
            .arg("--no-upgrade")
            .arg("--show-settings"), @r#"
    ...
                 strategy: Some(
                     {
                         PackageName(
    -                        "sniffio",
    +                        "idna",
                         ),
                     },
                     {},
    ...
    "#
    );

    // Despite `upgrade-package = ["idna"]` in the configuration file, we should enable all upgrades.
    // Compare against output before adding `upgrade-package = ["idna"]`, with `--no-upgrade --upgrade-package sniffio`.
    diff_uv_snapshot!(context.filters(), &no_upgrade, add_shared_args(context.lock())
            .arg("--upgrade")
            .arg("--show-settings"), @r#"
    ...
                 strategy: Some(
                     {
                         PackageName(
    -                        "sniffio",
    +                        "idna",
                         ),
                     },
                     {},
    ...
    "#
    );

    // Mark both `sniffio` and `idna` for upgrade.
    // Compare against output before adding `upgrade-package = ["idna"]`, with `--no-upgrade`.
    diff_uv_snapshot!(context.filters(), &no_upgrade, add_shared_args(context.lock())
            .arg("--upgrade-package")
            .arg("sniffio")
            .arg("--show-settings"), @r#"
    ...
                         PackageName(
                             "sniffio",
                         ),
    +                    PackageName(
    +                        "idna",
    +                    ),
                     },
                     {},
                 ),
    ...
    "#
    );

    Ok(())
}

/// Test that setting `build-isolation = true` in pyproject.toml followed by
/// `--no-build-isolation-package numpy` on the CLI disables build isolation for `numpy`.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn build_isolation_override() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let baseline = capture_uv_snapshot!(
        context.filters(),
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
            .arg("--no-build-isolation-package")
            .arg("numpy")
    );

    // Write a `uv.toml` file to disable build isolation.
    let uv_toml = context.temp_dir.child("uv.toml");
    uv_toml.write_str(indoc::indoc! {r"
        no-build-isolation = true
    "})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("numpy")?;

    let shared = diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .arg("--no-build-isolation-package").arg("numpy"), @r#"
    ...
             torch_backend: None,
             cuda_driver_version: None,
             amd_gpu_architecture: None,
    -        build_isolation: SharedPackage(
    -            [
    -                PackageName(
    -                    "numpy",
    -                ),
    -            ],
    -        ),
    +        build_isolation: Shared,
             extra_build_dependencies: ExtraBuildDependencies(
                 {},
             ),
    ...
    "#);

    // Now enable build isolation for all packages except `numpy`.
    uv_toml.write_str(indoc::indoc! {r"
        no-build-isolation = false
    "})?;

    // Compare against output before changing `no-build-isolation` from `true` to `false`.
    diff_uv_snapshot!(context.filters(), &shared, add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
            .arg("--no-build-isolation-package").arg("numpy"), @r#"
    ...
             torch_backend: None,
             cuda_driver_version: None,
             amd_gpu_architecture: None,
    -        build_isolation: Shared,
    +        build_isolation: SharedPackage(
    +            [
    +                PackageName(
    +                    "numpy",
    +                ),
    +            ],
    +        ),
             extra_build_dependencies: ExtraBuildDependencies(
                 {},
             ),
    ...
    "#);

    Ok(())
}
