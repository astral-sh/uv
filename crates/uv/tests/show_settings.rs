#![cfg(all(feature = "python", feature = "pypi"))]

use std::process::Command;

use assert_fs::prelude::*;

use common::{uv_snapshot, TestContext};

mod common;

/// Create a `pip compile` command, overwriting defaults for any settings that vary based on machine
/// and operating system.
fn add_shared_args(mut command: Command) -> Command {
    command
        .env("UV_LINK_MODE", "clone")
        .env("UV_CONCURRENT_DOWNLOADS", "50")
        .env("UV_CONCURRENT_BUILDS", "16")
        .env("UV_CONCURRENT_INSTALLS", "8");
    command
}

/// Read from a `uv.toml` file in the current directory.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_uv_toml() -> anyhow::Result<()> {
    let context = TestContext::new("3.12");

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
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: Some(
                    Pypi(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://pypi.org/simple",
                            ),
                        },
                    ),
                ),
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: LowestDirect,
            prerelease: IfNecessaryOrExplicit,
            output_file: None,
            no_strip_extras: false,
            no_strip_markers: false,
            no_annotate: false,
            no_header: false,
            custom_compile_command: None,
            generate_hashes: true,
            config_setting: ConfigSettings(
                {},
            ),
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
    );

    // Resolution should use the highest version, and generate hashes.
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .arg("--resolution=highest"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: Some(
                    Pypi(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://pypi.org/simple",
                            ),
                        },
                    ),
                ),
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: Highest,
            prerelease: IfNecessaryOrExplicit,
            output_file: None,
            no_strip_extras: false,
            no_strip_markers: false,
            no_annotate: false,
            no_header: false,
            custom_compile_command: None,
            generate_hashes: true,
            config_setting: ConfigSettings(
                {},
            ),
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
    );

    // Resolution should use the highest version, and omit hashes.
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .arg("--resolution=highest")
        .arg("--no-generate-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: Some(
                    Pypi(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://pypi.org/simple",
                            ),
                        },
                    ),
                ),
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: Highest,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
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
    let context = TestContext::new("3.12");

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
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: Some(
                    Pypi(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://pypi.org/simple",
                            ),
                        },
                    ),
                ),
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: LowestDirect,
            prerelease: IfNecessaryOrExplicit,
            output_file: None,
            no_strip_extras: false,
            no_strip_markers: false,
            no_annotate: false,
            no_header: false,
            custom_compile_command: None,
            generate_hashes: true,
            config_setting: ConfigSettings(
                {},
            ),
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
    );

    // Remove the `uv.toml` file.
    fs_err::remove_file(config.path())?;

    // Resolution should use the highest version, and omit hashes.
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: None,
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: Highest,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
    );

    // Add configuration to the `pyproject.toml` file.
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "example"
        version = "0.0.0"

        [tool.uv.pip]
        resolution = "lowest-direct"
        generate-hashes = true
        index-url = "https://pypi.org/simple"
    "#})?;

    // Resolution should use the lowest direct version, and generate hashes.
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: Some(
                    Pypi(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://pypi.org/simple",
                            ),
                        },
                    ),
                ),
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: LowestDirect,
            prerelease: IfNecessaryOrExplicit,
            output_file: None,
            no_strip_extras: false,
            no_strip_markers: false,
            no_annotate: false,
            no_header: false,
            custom_compile_command: None,
            generate_hashes: true,
            config_setting: ConfigSettings(
                {},
            ),
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
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
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: Some(
                    Url(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "test.pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://test.pypi.org/simple",
                            ),
                        },
                    ),
                ),
                extra_index: [
                    Pypi(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://pypi.org/simple",
                            ),
                        },
                    ),
                ],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: Highest,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
    );

    // Providing an additional index URL on the command-line should be merged with the
    // configuration file.
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .arg("--extra-index-url")
        .arg("https://test.pypi.org/simple"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: Some(
                    Url(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "test.pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://test.pypi.org/simple",
                            ),
                        },
                    ),
                ),
                extra_index: [
                    Url(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "test.pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://test.pypi.org/simple",
                            ),
                        },
                    ),
                    Pypi(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://pypi.org/simple",
                            ),
                        },
                    ),
                ],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: Highest,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
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
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: None,
                extra_index: [],
                flat_index: [
                    Url(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "download.pytorch.org",
                                    ),
                                ),
                                port: None,
                                path: "/whl/torch_stable.html",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://download.pytorch.org/whl/torch_stable.html",
                            ),
                        },
                    ),
                ],
                no_index: true,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: Highest,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
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
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: None,
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: LowestDirect,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
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

    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: None,
                extra_index: [
                    Url(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "download.pytorch.org",
                                    ),
                                ),
                                port: None,
                                path: "/whl",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://download.pytorch.org/whl",
                            ),
                        },
                    ),
                    Url(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "test.pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://test.pypi.org/simple",
                            ),
                        },
                    ),
                ],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: Highest,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
    );

    // But the command-line should take precedence over both.
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .arg("--resolution=lowest-direct"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: None,
                extra_index: [
                    Url(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "download.pytorch.org",
                                    ),
                                ),
                                port: None,
                                path: "/whl",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://download.pytorch.org/whl",
                            ),
                        },
                    ),
                    Url(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "test.pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://test.pypi.org/simple",
                            ),
                        },
                    ),
                ],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: LowestDirect,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
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
    // Create a temporary directory to store the user configuration.
    let xdg = assert_fs::TempDir::new().expect("Failed to create temp dir");
    let uv = xdg.child("uv");
    let config = uv.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        [pip]
        resolution = "lowest-direct"
    "#})?;

    let context = TestContext::new("3.12");

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    // Resolution should use the lowest direct version.
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .env("XDG_CONFIG_HOME", xdg.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: None,
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: LowestDirect,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
    );

    // Add a local configuration to generate hashes.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r"
        [pip]
        generate-hashes = true
    "})?;

    // Resolution should use the lowest direct version and generate hashes.
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .env("XDG_CONFIG_HOME", xdg.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: None,
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: LowestDirect,
            prerelease: IfNecessaryOrExplicit,
            output_file: None,
            no_strip_extras: false,
            no_strip_markers: false,
            no_annotate: false,
            no_header: false,
            custom_compile_command: None,
            generate_hashes: true,
            config_setting: ConfigSettings(
                {},
            ),
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
    );

    // Add a local configuration to override the user configuration.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        [pip]
        resolution = "highest"
    "#})?;

    // Resolution should use the highest version.
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .env("XDG_CONFIG_HOME", xdg.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: None,
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: Highest,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
    );

    // However, the user-level `tool.uv.pip` settings override the project-level `tool.uv` settings.
    // This is awkward, but we merge the user configuration into the workspace configuration, so
    // the resulting configuration has both `tool.uv.pip.resolution` (from the user configuration)
    // and `tool.uv.resolution` (from the workspace settings), so we choose the former.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        resolution = "highest"
    "#})?;

    // Resolution should use the highest version.
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .env("XDG_CONFIG_HOME", xdg.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: None,
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: LowestDirect,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
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
    config.write_str(indoc::indoc! {r#"
        resolution = "lowest-direct"
    "#})?;

    let context = TestContext::new("3.12");

    // Add a local configuration to disable build isolation.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r"
        no-build-isolation = true
    "})?;

    // If we're running a user-level command, like `uv tool install`, we should use lowest direct,
    // but retain build isolation (since we ignore the local configuration).
    uv_snapshot!(context.filters(), add_shared_args(context.tool_install())
        .arg("--show-settings")
        .arg("requirements.in")
        .env("XDG_CONFIG_HOME", xdg.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    ToolInstallSettings {
        package: "requirements.in",
        from: None,
        with: [],
        with_requirements: [],
        python: None,
        refresh: None(
            Timestamp(
                SystemTime {
                    tv_sec: [TIME],
                    tv_nsec: [TIME],
                },
            ),
        ),
        options: ResolverInstallerOptions {
            index_url: None,
            extra_index_url: None,
            no_index: None,
            find_links: None,
            index_strategy: None,
            keyring_provider: None,
            allow_insecure_host: None,
            resolution: Some(
                LowestDirect,
            ),
            prerelease: None,
            config_settings: None,
            no_build_isolation: None,
            no_build_isolation_package: None,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            link_mode: Some(
                Clone,
            ),
            compile_bytecode: None,
            no_sources: None,
            upgrade: None,
            upgrade_package: None,
            reinstall: None,
            reinstall_package: None,
            no_build: None,
            no_build_package: None,
            no_binary: None,
            no_binary_package: None,
        },
        settings: ResolverInstallerSettings {
            index_locations: IndexLocations {
                index: None,
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            resolution: LowestDirect,
            prerelease: IfNecessaryOrExplicit,
            config_setting: ConfigSettings(
                {},
            ),
            no_build_isolation: false,
            no_build_isolation_package: [],
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            upgrade: None,
            reinstall: None,
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
        },
        force: false,
        editable: false,
    }

    ----- stderr -----
    "###
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
    let context = TestContext::new("3.12");

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
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: None,
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: LowestDirect,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
    );

    Ok(())
}

/// Read from both a `uv.toml` and `pyproject.toml` file in the current directory.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_both() -> anyhow::Result<()> {
    let context = TestContext::new("3.12");

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

        [tool.uv.pip]
        resolution = "highest"
        extra-index-url = ["https://test.pypi.org/simple"]
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    // Resolution should succeed, but warn that the `pip` section in `pyproject.toml` is ignored.
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: Some(
                    Pypi(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://pypi.org/simple",
                            ),
                        },
                    ),
                ),
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: LowestDirect,
            prerelease: IfNecessaryOrExplicit,
            output_file: None,
            no_strip_extras: false,
            no_strip_markers: false,
            no_annotate: false,
            no_header: false,
            custom_compile_command: None,
            generate_hashes: true,
            config_setting: ConfigSettings(
                {},
            ),
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
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
    let context = TestContext::new("3.12");

    // Write a `uv.toml` to a temporary location. (Use the cache directory for convenience, since
    // it's already obfuscated in the fixtures.)
    let config_dir = &context.cache_dir;
    let config = config_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        [pip]
        resolution = "lowest-direct"
        generate-hashes = true
        index-url = "https://pypi.org/simple"
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("--config-file")
        .arg(config.path())
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: Some(
                    Pypi(
                        VerbatimUrl {
                            url: Url {
                                scheme: "https",
                                cannot_be_a_base: false,
                                username: "",
                                password: None,
                                host: Some(
                                    Domain(
                                        "pypi.org",
                                    ),
                                ),
                                port: None,
                                path: "/simple",
                                query: None,
                                fragment: None,
                            },
                            given: Some(
                                "https://pypi.org/simple",
                            ),
                        },
                    ),
                ),
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: LowestDirect,
            prerelease: IfNecessaryOrExplicit,
            output_file: None,
            no_strip_extras: false,
            no_strip_markers: false,
            no_annotate: false,
            no_header: false,
            custom_compile_command: None,
            generate_hashes: true,
            config_setting: ConfigSettings(
                {},
            ),
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
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
        .arg("requirements.in"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `[CACHE_DIR]/uv.toml`
      Caused by: TOML parse error at line 1, column 1
      |
    1 | [project]
      | ^
    unknown field `project`

    "###
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
        .arg("requirements.in"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The `--config-file` argument expects to receive a `uv.toml` file, not a `pyproject.toml`. If you're trying to run a command from another project, use the `--directory` argument instead.
    error: Failed to parse: `[CACHE_DIR]/pyproject.toml`
      Caused by: TOML parse error at line 9, column 3
      |
    9 | ""
      |   ^
    expected `.`, `=`

    "###
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
    let context = TestContext::new("3.12");

    // Set `lowest-direct` in a `uv.toml`.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        [pip]
        resolution = "lowest-direct"
    "#})?;

    let child = context.temp_dir.child("child");
    fs_err::create_dir(&child)?;

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
    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .current_dir(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: None,
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: LowestDirect,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
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

    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .current_dir(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: None,
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: Highest,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
    );

    Ok(())
}

/// Deserialize an insecure host.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn allow_insecure_host() -> anyhow::Result<()> {
    let context = TestContext::new("3.12");

    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        allow-insecure-host = ["google.com", { host = "example.com" }]
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    GlobalSettings {
        quiet: false,
        verbose: 0,
        color: Auto,
        native_tls: false,
        concurrency: Concurrency {
            downloads: 50,
            builds: 16,
            installs: 8,
        },
        connectivity: Online,
        show_settings: true,
        preview: Disabled,
        python_preference: OnlySystem,
        python_downloads: Automatic,
        no_progress: false,
    }
    CacheSettings {
        no_cache: false,
        cache_dir: Some(
            "[CACHE_DIR]/",
        ),
    }
    PipCompileSettings {
        src_file: [
            "requirements.in",
        ],
        constraint: [],
        override: [],
        build_constraint: [],
        constraints_from_workspace: [],
        overrides_from_workspace: [],
        environments: SupportedEnvironments(
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
                index: None,
                extra_index: [],
                flat_index: [],
                no_index: false,
            },
            python: None,
            system: false,
            extras: None,
            break_system_packages: false,
            target: None,
            prefix: None,
            index_strategy: FirstIndex,
            keyring_provider: Disabled,
            allow_insecure_host: [
                TrustedHost {
                    scheme: None,
                    host: "google.com",
                    port: None,
                },
                TrustedHost {
                    scheme: None,
                    host: "example.com",
                    port: None,
                },
            ],
            no_build_isolation: false,
            no_build_isolation_package: [],
            build_options: BuildOptions {
                no_binary: None,
                no_build: None,
            },
            allow_empty_requirements: false,
            strict: false,
            dependency_mode: Transitive,
            resolution: Highest,
            prerelease: IfNecessaryOrExplicit,
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
            python_version: None,
            python_platform: None,
            universal: false,
            exclude_newer: Some(
                ExcludeNewer(
                    2024-03-25T00:00:00Z,
                ),
            ),
            no_emit_package: [],
            emit_index_url: false,
            emit_find_links: false,
            emit_build_options: false,
            emit_marker_expression: false,
            emit_index_annotation: false,
            annotation_style: Split,
            link_mode: Clone,
            compile_bytecode: false,
            sources: Enabled,
            hash_checking: None,
            upgrade: None,
            reinstall: None,
        },
    }

    ----- stderr -----
    "###
    );

    Ok(())
}
