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

/// Read from a `uv.toml` file in the current directory.
#[test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
fn resolve_uv_toml() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

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
    let default = capture_uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
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
                indexes: [
                    Index {
                        name: None,
                        url: Pypi(
                            VerbatimUrl {
                                url: DisplaySafeUrl {
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
                                expanded: false,
                            },
                        ),
                        explicit: false,
                        default: true,
                        origin: Some(
                            Project,
                        ),
                        format: Simple,
                        publish_url: None,
                        authenticate: Auto,
                        ignore_error_codes: None,
                        cache_control: None,
                        exclude_newer: None,
                    },
                ],
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
            resolution: LowestDirect,
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
            generate_hashes: true,
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
    "#
    );

    // Resolution should use the highest version, and generate hashes.
    let highest = diff_uv_snapshot!(
        context.filters(),
        &default,
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
            .arg("--resolution=highest"),
        @"
    --- old
    +++ new
    @@ -156,7 +156,7 @@
             allow_empty_requirements: false,
             strict: false,
             dependency_mode: Transitive,
    -        resolution: LowestDirect,
    +        resolution: Highest,
             prerelease: IfNecessaryOrExplicit,
             fork_strategy: RequiresPython,
             dependency_metadata: DependencyMetadata(
    "
    );

    // Resolution should use the highest version, and omit hashes.
    diff_uv_snapshot!(
        context.filters(),
        &highest,
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
            .arg("--resolution=highest")
            .arg("--no-generate-hashes"),
        @"
    --- old
    +++ new
    @@ -168,7 +168,7 @@
             no_annotate: false,
             no_header: false,
             custom_compile_command: None,
    -        generate_hashes: true,
    +        generate_hashes: false,
             config_setting: ConfigSettings(
                 {},
             ),
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
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
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
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
    "#
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

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
    "#
    );

    // Providing an additional index URL on the command-line should be merged with the
    // configuration file.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .arg("--extra-index-url")
        .arg("https://test.pypi.org/simple"), @r#"
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

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in"), @r#"
    "#
    );

    // But the command-line should take precedence over both.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .arg("--resolution=lowest-direct"), @r#"
    "#
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
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .env(EnvVars::XDG_CONFIG_HOME, xdg.path()), @r#"
    "#
    );

    // Add a local configuration to generate hashes.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r"
        [pip]
        generate-hashes = true
    "})?;

    // Resolution should use the lowest direct version and generate hashes.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .env(EnvVars::XDG_CONFIG_HOME, xdg.path()), @r#"
    "#
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

    // Resolution should use the highest version.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .env(EnvVars::XDG_CONFIG_HOME, xdg.path()), @r#"
    "#
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
    "#
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
    unknown field `project`, expected one of `required-version`, `system-certs`, `native-tls`, `offline`, `no-cache`, `cache-dir`, `preview`, `python-preference`, `python-downloads`, `concurrent-downloads`, `concurrent-builds`, `concurrent-installs`, `index`, `index-url`, `extra-index-url`, `no-index`, `find-links`, `index-strategy`, `keyring-provider`, `http-proxy`, `https-proxy`, `no-proxy`, `allow-insecure-host`, `resolution`, `prerelease`, `fork-strategy`, `dependency-metadata`, `config-settings`, `config-settings-package`, `no-build-isolation`, `no-build-isolation-package`, `extra-build-dependencies`, `extra-build-variables`, `exclude-newer`, `exclude-newer-package`, `link-mode`, `compile-bytecode`, `no-sources`, `no-sources-package`, `upgrade`, `upgrade-package`, `reinstall`, `reinstall-package`, `no-build`, `no-build-package`, `no-binary`, `no-binary-package`, `torch-backend`, `python-install-mirror`, `pypy-install-mirror`, `python-downloads-json-url`, `publish-url`, `trusted-publishing`, `check-url`, `add-bounds`, `audit`, `pip`, `cache-keys`, `override-dependencies`, `exclude-dependencies`, `constraint-dependencies`, `build-constraint-dependencies`, `environments`, `required-environments`, `conflicts`, `workspace`, `sources`, `managed`, `package`, `default-groups`, `dependency-groups`, `dev-dependencies`, `build-backend`
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

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("requirements.in")
        .arg("--show-settings")
        .arg("--index-url")
        .arg("https://cli.pypi.org/simple"), @r#"
    "#
    );

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("requirements.in")
        .arg("--show-settings")
        .arg("--default-index")
        .arg("https://cli.pypi.org/simple"), @r#"
    "#
    );

    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        index-url = "https://file.pypi.org/simple"
    "#})?;

    // Prefer the `--default-index` from the CLI, and treat it as the default.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("requirements.in")
        .arg("--show-settings")
        .arg("--default-index")
        .arg("https://cli.pypi.org/simple"), @r#"
    "#
    );

    // Prefer the `--index` from the CLI, but treat the index from the file as the default.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("requirements.in")
        .arg("--show-settings")
        .arg("--index")
        .arg("https://cli.pypi.org/simple"), @r#"
    "#
    );

    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r#"
        [[index]]
        url = "https://file.pypi.org/simple"
        default = true
    "#})?;

    // Prefer the `--index-url` from the CLI, and treat it as the default.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("requirements.in")
        .arg("--show-settings")
        .arg("--index-url")
        .arg("https://cli.pypi.org/simple"), @r#"
    "#
    );

    // Prefer the `--extra-index-url` from the CLI, but not as the default.
    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.pip_compile())
        .arg("requirements.in")
        .arg("--show-settings")
        .arg("--extra-index-url")
        .arg("https://cli.pypi.org/simple"), @r#"
    "#
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

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    let default = capture_uv_snapshot!(context.filters(), add_shared_args(context.pip_install())
        .arg("-r")
        .arg("requirements.in")
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
    "#
    );

    diff_uv_snapshot!(
        context.filters(),
        &default,
        add_shared_args(context.pip_install())
            .arg("-r")
            .arg("requirements.in")
            .arg("--no-verify-hashes")
            .arg("--show-settings"),
        @"
    --- old
    +++ new
    @@ -154,9 +154,7 @@
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
    "
    );

    diff_uv_snapshot!(
        context.filters(),
        &default,
        add_shared_args(context.pip_install())
            .arg("-r")
            .arg("requirements.in")
            .arg("--require-hashes")
            .arg("--show-settings"),
        @"
    --- old
    +++ new
    @@ -155,7 +155,7 @@
             compile_bytecode: false,
             sources: None,
             hash_checking: Some(
    -            Verify,
    +            Require,
             ),
             upgrade: Upgrade {
                 strategy: None,
    "
    );

    diff_uv_snapshot!(
        context.filters(),
        &default,
        add_shared_args(context.pip_install())
            .arg("-r")
            .arg("requirements.in")
            .arg("--no-require-hashes")
            .arg("--show-settings"),
        @"
    --- old
    +++ new
    @@ -154,9 +154,7 @@
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
    "
    );

    diff_uv_snapshot!(
        context.filters(),
        &default,
        add_shared_args(context.pip_install())
            .arg("-r")
            .arg("requirements.in")
            .env(EnvVars::UV_NO_VERIFY_HASHES, "1")
            .arg("--show-settings"),
        @"
    --- old
    +++ new
    @@ -154,9 +154,7 @@
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
    "
    );

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

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.version()).arg("--show-settings").arg("--preview"), @r#"
    "#
    );

    diff_uv_snapshot!(
        context.filters(),
        &baseline,
        add_shared_args(context.version()).arg("--show-settings").arg("--preview").arg("--no-preview"),
        @""
    );

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.version()).arg("--show-settings").arg("--preview").arg("--preview-features").arg("python-install-default"), @r#"
    "#
    );

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.version()).arg("--show-settings").arg("--preview-features").arg("python-install-default,python-upgrade"), @r#"
    "#
    );

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.version()).arg("--show-settings").arg("--preview-features").arg("python-install-default").arg("--preview-feature").arg("python-upgrade"), @r#"
    "#
    );

    diff_uv_snapshot!(
        context.filters(),
        &baseline,
        add_shared_args(context.version()).arg("--show-settings")
        .arg("--preview-features").arg("python-install-default").arg("--preview-feature").arg("python-upgrade")
        .arg("--no-preview"),
        @""
    );
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
    "
    );

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.version())
        .arg("--show-settings")
        .arg("--no-system-certs")
        .env(EnvVars::UV_NATIVE_TLS, "1"), @"
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
    "
    );

    config.write_str(indoc::indoc! {r"
        system-certs = false
        native-tls = true
    "})?;

    diff_uv_snapshot!(context.filters(), &baseline, add_shared_args(context.version())
        .arg("--show-settings"), @"
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

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio>3.0.0")?;

    // `--no-upgrade` overrides `--upgrade-package`.
    // TODO(charlie): This should mark `sniffio` for upgrade, but it doesn't.
    let no_upgrade = capture_uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--no-upgrade")
        .arg("--upgrade-package")
        .arg("sniffio")
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
                strategy: Some(
                    {
                        PackageName(
                            "sniffio",
                        ),
                    },
                    {},
                ),
                constraints: {},
            },
            reinstall: None,
        },
    }

    ----- stderr -----
    "#
    );

    // Write a `uv.toml` file to the directory.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r"
        [pip]
        upgrade = false
    "})?;

    // Despite `upgrade = false` in the configuration file, we should mark `idna` for upgrade.
    diff_uv_snapshot!(
        context.filters(),
        &no_upgrade,
        add_shared_args(context.pip_compile())
            .arg("--upgrade-package")
            .arg("idna")
            .arg("--show-settings")
            .arg("requirements.in"),
        @r#"
    --- old
    +++ new
    @@ -160,14 +160,7 @@
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
    "#
    );

    // Write a `uv.toml` file to the directory.
    let config = context.temp_dir.child("uv.toml");
    config.write_str(indoc::indoc! {r"
        [pip]
        upgrade = true
    "})?;

    // Despite `--upgrade-package idna` in the command line, we should upgrade all packages.
    diff_uv_snapshot!(
        context.filters(),
        &no_upgrade,
        add_shared_args(context.pip_compile())
            .arg("--upgrade-package")
            .arg("idna")
            .arg("--show-settings")
            .arg("requirements.in"),
        @r#"
    --- old
    +++ new
    @@ -160,14 +160,7 @@
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
    "#
    );

    // Write a `uv.toml` file to the directory.
    config.write_str(indoc::indoc! {r#"
        [pip]
        upgrade-package = ["idna"]
    "#})?;

    // Despite `upgrade-package = ["idna"]` in the configuration file, we should disable upgrades.
    diff_uv_snapshot!(
        context.filters(),
        &no_upgrade,
        add_shared_args(context.pip_compile())
            .arg("--no-upgrade")
            .arg("--show-settings")
            .arg("requirements.in"),
        @r#"
    --- old
    +++ new
    @@ -163,7 +163,7 @@
                 strategy: Some(
                     {
                         PackageName(
    -                        "sniffio",
    +                        "idna",
                         ),
                     },
                     {},
    "#
    );

    // Despite `upgrade-package = ["idna"]` in the configuration file, we should enable all upgrades.
    diff_uv_snapshot!(
        context.filters(),
        &no_upgrade,
        add_shared_args(context.pip_compile())
            .arg("--upgrade")
            .arg("--show-settings")
            .arg("requirements.in"),
        @r#"
    --- old
    +++ new
    @@ -163,7 +163,7 @@
                 strategy: Some(
                     {
                         PackageName(
    -                        "sniffio",
    +                        "idna",
                         ),
                     },
                     {},
    "#
    );

    // Mark both `sniffio` and `idna` for upgrade.
    diff_uv_snapshot!(
        context.filters(),
        &no_upgrade,
        add_shared_args(context.pip_compile())
            .arg("--upgrade-package")
            .arg("sniffio")
            .arg("--show-settings")
            .arg("requirements.in"),
        @r#"
    --- old
    +++ new
    @@ -165,6 +165,9 @@
                         PackageName(
                             "sniffio",
                         ),
    +                    PackageName(
    +                        "idna",
    +                    ),
                     },
                     {},
                 ),
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

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc::indoc! {r#"
        [project]
        name = "foo"
        version = "0.0.0"
        dependencies = ["anyio>3.0.0"]
    "#})?;

    // `--no-upgrade` overrides `--upgrade-package`.
    // TODO(charlie): This should mark `sniffio` for upgrade, but it doesn't.
    let no_upgrade = capture_uv_snapshot!(context.filters(), add_shared_args(context.lock())
        .arg("--no-upgrade")
        .arg("--upgrade-package")
        .arg("sniffio")
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
            upgrade: Upgrade {
                strategy: Some(
                    {
                        PackageName(
                            "sniffio",
                        ),
                    },
                    {},
                ),
                constraints: {},
            },
        },
    }

    ----- stderr -----
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
    diff_uv_snapshot!(
        context.filters(),
        &no_upgrade,
        add_shared_args(context.lock())
            .arg("--upgrade-package")
            .arg("idna")
            .arg("--show-settings"),
        @r#"
    --- old
    +++ new
    @@ -98,14 +98,7 @@
             sources: None,
             torch_backend: None,
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
    diff_uv_snapshot!(
        context.filters(),
        &no_upgrade,
        add_shared_args(context.lock())
            .arg("--upgrade-package")
            .arg("idna")
            .arg("--show-settings"),
        @r#"
    --- old
    +++ new
    @@ -98,14 +98,7 @@
             sources: None,
             torch_backend: None,
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
    diff_uv_snapshot!(
        context.filters(),
        &no_upgrade,
        add_shared_args(context.lock())
            .arg("--no-upgrade")
            .arg("--show-settings"),
        @r#"
    --- old
    +++ new
    @@ -101,7 +101,7 @@
                 strategy: Some(
                     {
                         PackageName(
    -                        "sniffio",
    +                        "idna",
                         ),
                     },
                     {},
    "#
    );

    // Despite `upgrade-package = ["idna"]` in the configuration file, we should enable all upgrades.
    diff_uv_snapshot!(
        context.filters(),
        &no_upgrade,
        add_shared_args(context.lock())
            .arg("--upgrade")
            .arg("--show-settings"),
        @r#"
    --- old
    +++ new
    @@ -101,7 +101,7 @@
                 strategy: Some(
                     {
                         PackageName(
    -                        "sniffio",
    +                        "idna",
                         ),
                     },
                     {},
    "#
    );

    // Mark both `sniffio` and `idna` for upgrade.
    diff_uv_snapshot!(
        context.filters(),
        &no_upgrade,
        add_shared_args(context.lock())
            .arg("--upgrade-package")
            .arg("sniffio")
            .arg("--show-settings"),
        @r#"
    --- old
    +++ new
    @@ -103,6 +103,9 @@
                         PackageName(
                             "sniffio",
                         ),
    +                    PackageName(
    +                        "idna",
    +                    ),
                     },
                     {},
                 ),
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

    // Write a `uv.toml` file to disable build isolation.
    let uv_toml = context.temp_dir.child("uv.toml");
    uv_toml.write_str(indoc::indoc! {r"
        no-build-isolation = true
    "})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("numpy")?;

    let shared = capture_uv_snapshot!(context.filters(), add_shared_args(context.pip_compile())
        .arg("--show-settings")
        .arg("requirements.in")
        .arg("--no-build-isolation-package").arg("numpy"), @r#"
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
            build_isolation: Shared,
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

    // Now enable build isolation for all packages except `numpy`.
    uv_toml.write_str(indoc::indoc! {r"
        no-build-isolation = false
    "})?;

    diff_uv_snapshot!(
        context.filters(),
        &shared,
        add_shared_args(context.pip_compile())
            .arg("--show-settings")
            .arg("requirements.in")
            .arg("--no-build-isolation-package").arg("numpy"),
        @r#"
    --- old
    +++ new
    @@ -104,7 +104,13 @@
             index_strategy: FirstIndex,
             keyring_provider: Disabled,
             torch_backend: None,
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
    "#);

    Ok(())
}
