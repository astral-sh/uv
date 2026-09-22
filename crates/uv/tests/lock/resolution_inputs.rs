use anyhow::{Result, anyhow};
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;
use url::Url;
use uv_test::packse::{PackseServer, scenario::Scenario};
use uv_test::uv_snapshot;

/// Consultations retain rules that remove dependencies and omit unrelated declarations of every kind.
#[test]
fn prune_unused_inputs() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject = context.temp_dir.child("pyproject.toml");
    let project = indoc! {r#"
        [project]
        name = "project"
        version = "1.0"
        requires-python = ">=3.12"
        dependencies = ["excluded", "overridden", "shadowed"]

        [tool.uv]
        preview-features = ["resolution-inputs"]
        constraint-dependencies = [
            "unused>=1",
            { package = { name = "absent" }, dependencies = ["unused>=2"] },
        ]
        override-dependencies = [
            "overridden; python_version < '0'",
            "shadowed==1",
            { package = { name = "project" }, dependencies = ["shadowed; python_version < '0'"] },
            "unused==1",
            { package = { name = "absent" }, dependencies = ["unused==2"] },
        ]
        exclude-dependencies = [
            "excluded", "unused",
            { package = { name = "absent" }, dependencies = ["unused"] },
        ]
        dependency-metadata = [{ name = "unused", version = "1" }]

        [tool.uv.exclude-newer-package]
        unused = "2020-01-01T00:00:00Z"
        excluded = "2020-01-01T00:00:00Z"
        overridden = "2020-01-01T00:00:00Z"
        shadowed = "2020-01-01T00:00:00Z"
    "#};
    pyproject.write_str(project)?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        overrides = [
            { package = { name = "project" }, dependencies = [{ name = "shadowed", marker = "python_full_version < '0'" }] },
            { name = "overridden", marker = "python_full_version < '0'" },
        ]
        excludes = ["excluded"]

        [manifest.resolution-inputs]
        constraints = [
            "excluded",
            "overridden",
            "project",
            "shadowed",
        ]
        overrides = [
            "excluded",
            "overridden",
            "project",
        ]
        exclusions = [
            "excluded",
            "overridden",
            "project",
            "shadowed",
        ]
        scoped-constraints = ["project"]
        scoped-overrides = ["project"]
        scoped-exclusions = ["project"]
        dependency-metadata = [{ name = "project" }]

        [[package]]
        name = "project"
        version = "1.0"
        source = { virtual = "." }

        [package.metadata]
        requires-dist = [
            { name = "excluded" },
            { name = "overridden" },
            { name = "shadowed" },
        ]
        "#);
    });

    // Unrelated entries, shadowed global overrides, and unconsulted cutoffs can change independently.
    pyproject.write_str(
        &project
            .replace("unused", "other")
            .replace("absent", "other-parent")
            .replace("shadowed==1", "shadowed==2")
            .replace("2020-01-01", "2021-01-01"),
    )?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache").arg("--no-preview"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    // Removing a rule that filtered out a dependency must invalidate the lock.
    pyproject.write_str(&project.replace("\"excluded\", \"unused\"", "\"unused\""))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because excluded was not found in the cache and your project depends on excluded, we can conclude that your project's requirements are unsatisfiable.

    hint: Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.
    ");
    Ok(())
}

/// Global URL overrides can affect source selection even when a scoped override supplies the requirement.
#[test]
fn source_policy() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let child = context.temp_dir.child("child");
    child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "1.0"
    "#})?;
    let other = context.temp_dir.child("other");
    other.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "2.0"
    "#})?;
    let child_url = Url::from_file_path(child.path())
        .map_err(|()| anyhow!("child path is not a valid file URL"))?;
    let other_url = Url::from_file_path(other.path())
        .map_err(|()| anyhow!("other path is not a valid file URL"))?;
    let project = formatdoc! {r#"
        [project]
        name = "project"
        version = "1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        preview-features = ["resolution-inputs"]
        override-dependencies = [
            "child @ {child_url}",
            {{ package = {{ name = "project" }}, dependencies = ["child>=1"] }},
        ]
        exclude-newer-package = {{ child = "2020-01-01T00:00:00Z" }}
    "#};
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&project)?;
    uv_snapshot!(context.filters(), context.tree().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stdout -----
    project v1.0
    └── child v1.0

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // A local source does not consult the package's registry upload cutoff.
    pyproject.write_str(&project.replace("2020-01-01", "2021-01-01"))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    pyproject.write_str(&project.replace(child_url.as_str(), other_url.as_str()))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    Ok(())
}

/// Misses detect newly added global and scoped settings, including scopes for dependency-free parents.
#[test]
fn newly_matching_inputs() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject = context.temp_dir.child("pyproject.toml");
    let project = indoc! {r#"
        [project]
        name = "project"
        version = "1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [tool.uv]
        preview-features = ["resolution-inputs"]

        [tool.uv.sources]
        child = { path = "child" }
    "#};
    context
        .temp_dir
        .child("child/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "child"
        version = "1.0"
        dependencies = []
    "#})?;
    pyproject.write_str(project)?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]

        [manifest.resolution-inputs]
        constraints = [
            "child",
            "project",
        ]
        overrides = [
            "child",
            "project",
        ]
        exclusions = [
            "child",
            "project",
        ]
        scoped-constraints = [
            "child",
            "project",
        ]
        scoped-overrides = [
            "child",
            "project",
        ]
        scoped-exclusions = ["project"]
        dependency-metadata = [
            { name = "child" },
            { name = "project" },
        ]

        [[package]]
        name = "child"
        version = "1.0"
        source = { directory = "child" }

        [[package]]
        name = "project"
        version = "1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "child" },
        ]

        [package.metadata]
        requires-dist = [{ name = "child", directory = "child" }]
        "#);
    });

    // All of these add a declaration to a previously unsuccessful lookup.
    for setting in [
        "constraint-dependencies = [\"child>=1\"]",
        "override-dependencies = [\"child==1.0\"]",
        "constraint-dependencies = [{ package = { name = \"child\" }, dependencies = [] }]",
        "override-dependencies = [{ package = { name = \"child\" }, dependencies = [] }]",
        "exclude-dependencies = [{ package = { name = \"project\" }, dependencies = [] }]",
        "dependency-metadata = [{ name = \"child\", version = \"1.0\" }]",
    ] {
        pyproject.write_str(&project.replace("[tool.uv]\n", &format!("[tool.uv]\n{setting}\n")))?;
        insta::allow_duplicates! {
            uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
            exit_code: 1 (failure)
            ----- stderr -----
            Resolved 2 packages in [TIME]
            error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

            hint: To update the lockfile, run `uv lock`.
            ");
        }
    }

    // Looking up overrides for a dependency-free parent does not consult its exclusions.
    pyproject.write_str(&project.replace(
        "[tool.uv]\n",
        "[tool.uv]\nexclude-dependencies = [{ package = { name = \"child\" }, dependencies = [] }]\n",
    ))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // A new exclusion removes a previously resolved package.
    pyproject.write_str(&project.replace(
        "[tool.uv]\n",
        "[tool.uv]\nexclude-dependencies = [\"child\"]\n",
    ))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    Ok(())
}

/// Complete scopes retain empty exact entries that shadow versionless overrides and exclusions.
#[test]
fn empty_scopes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject = context.temp_dir.child("pyproject.toml");
    let project = indoc! {r#"
        [project]
        name = "project"
        version = "1.0"
        requires-python = ">=3.12"
        dependencies = ["missing; python_version < '0'"]

        [tool.uv]
        preview-features = ["resolution-inputs"]
        override-dependencies = [
            { package = { name = "project" }, dependencies = ["missing"] },
            { package = { name = "project", version = "1.0" }, dependencies = [] },
        ]
        exclude-dependencies = [
            { package = { name = "project" }, dependencies = ["missing"] },
            { package = { name = "project", version = "1.0" }, dependencies = [] },
        ]
    "#};
    pyproject.write_str(project)?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        overrides = [
            { package = { name = "project" }, dependencies = [{ name = "missing" }] },
            { package = { name = "project", version = "1.0" }, dependencies = [] },
        ]
        excludes = [
            { package = { name = "project" }, dependencies = ["missing"] },
            { package = { name = "project", version = "1.0" }, dependencies = [] },
        ]

        [manifest.resolution-inputs]
        constraints = [
            "missing",
            "project",
        ]
        overrides = [
            "missing",
            "project",
        ]
        exclusions = [
            "missing",
            "project",
        ]
        scoped-constraints = ["project"]
        scoped-overrides = ["project"]
        scoped-exclusions = ["project"]
        dependency-metadata = [{ name = "project" }]

        [[package]]
        name = "project"
        version = "1.0"
        source = { virtual = "." }

        [package.metadata]
        requires-dist = [{ name = "missing", marker = "python_full_version < '0'" }]
        "#);
    });
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    // An added dependency in the previously empty override is immediately relevant.
    pyproject.write_str(&project.replacen(
        "version = \"1.0\" }, dependencies = []",
        "version = \"1.0\" }, dependencies = [\"missing\"]",
        1,
    ))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because missing was not found in the cache and your project depends on missing, we can conclude that your project's requirements are unsatisfiable.

    hint: Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.
    ");
    Ok(())
}

/// Without a trace, older locks continue to compare every configured declaration.
#[test]
fn legacy_locks() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject = context.temp_dir.child("pyproject.toml");
    let project = indoc! {r#"
        [project]
        name = "project"
        version = "1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        constraint-dependencies = ["unused>=1"]
    "#};
    pyproject.write_str(project)?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline").arg("--no-preview"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    pyproject.write_str(&project.replace("unused>=1", "unused>=2"))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--preview-features=resolution-inputs"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    Ok(())
}

/// Resolving build dependencies does not add them to the runtime configuration trace.
#[cfg(feature = "test-pypi")]
#[test]
fn ignores_build_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [tool.uv]
        preview-features = ["resolution-inputs"]
        dependency-metadata = [{ name = "iniconfig", version = "2.0.0" }]
        constraint-dependencies = ["iniconfig==2.0.0"]
        override-dependencies = ["iniconfig==2.0.0"]

        [tool.uv.sources]
        child = { path = "child" }
    "#})?;
    context
        .temp_dir
        .child("child/pyproject.toml")
        .write_str(indoc! {r#"
        [build-system]
        requires = ["iniconfig==2.0.0"]
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    context
        .temp_dir
        .child("child/backend.py")
        .write_str(indoc! {r#"
        from pathlib import Path
        import iniconfig

        def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
            dist_info = Path(metadata_directory) / "child-1.0.dist-info"
            dist_info.mkdir()
            (dist_info / "METADATA").write_text(
                "Metadata-Version: 2.2\nName: child\nVersion: 1.0\n"
            )
            return dist_info.name
    "#})?;
    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]

        [manifest.resolution-inputs]
        constraints = [
            "child",
            "project",
        ]
        overrides = [
            "child",
            "project",
        ]
        exclusions = [
            "child",
            "project",
        ]
        scoped-constraints = [
            "child",
            "project",
        ]
        scoped-overrides = [
            "child",
            "project",
        ]
        scoped-exclusions = ["project"]
        dependency-metadata = [
            { name = "child" },
            { name = "project" },
        ]

        [[package]]
        name = "child"
        version = "1.0"
        source = { directory = "child" }

        [[package]]
        name = "project"
        version = "1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "child" },
        ]

        [package.metadata]
        requires-dist = [{ name = "child", directory = "child" }]
        "#);
    });
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    Ok(())
}

/// Exact metadata declarations shadow fallbacks; selection changes invalidate traced locks.
#[cfg(feature = "test-pypi")]
#[test]
fn metadata_precedence() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    let project = indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        preview-features = ["resolution-inputs"]

        [[tool.uv.dependency-metadata]]
        name = "anyio"
        version = "3.7.0"

        [[tool.uv.dependency-metadata]]
        name = "anyio"
        version = "3.6.0"

        [[tool.uv.dependency-metadata]]
        name = "anyio"

        [[tool.uv.dependency-metadata]]
        name = "idna"
        version = "3.6"

        [[tool.uv.dependency-metadata]]
        name = "sniffio"
    "#};
    pyproject_toml.write_str(project)?;

    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]

        [manifest.resolution-inputs]
        constraints = ["project"]
        overrides = ["project"]
        exclusions = ["project"]
        scoped-constraints = ["project"]
        scoped-overrides = ["project"]
        dependency-metadata = [{ name = "project" }]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        "#);
    });

    // Locks that omit unused metadata can be validated without preview enabled.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-preview"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    // Adding a dependency invalidates the lock and makes its metadata relevant.
    let project = project.replace("dependencies = []", "dependencies = [\"anyio==3.7.0\"]");
    pyproject_toml.write_str(&project)?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Added anyio v3.7.0
    ");
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]

        [manifest.resolution-inputs]
        constraints = [
            "anyio",
            "project",
        ]
        overrides = [
            "anyio",
            "project",
        ]
        exclusions = [
            "anyio",
            "project",
        ]
        scoped-constraints = ["project"]
        scoped-overrides = [
            "anyio",
            "project",
        ]
        scoped-exclusions = ["project"]
        candidate-policy = ["anyio"]
        exclude-newer = ["anyio"]
        dependency-metadata = [
            { name = "anyio", version = "3.7.0" },
            { name = "project" },
        ]

        [[manifest.dependency-metadata]]
        name = "anyio"
        version = "3.7.0"

        [[package]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737, upload-time = "2023-05-27T11:12:46.688Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873, upload-time = "2023-05-27T11:12:44.474Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = "==3.7.0" }]
        "#);
    });

    // A pruned lock remains valid with unused versioned and versionless declarations configured.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Changing metadata for an unrelated package does not invalidate the lock.
    pyproject_toml.write_str(&project.replace("version = \"3.6\"", "version = \"3.5\""))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Other versions and the shadowed versionless fallback may change independently.
    pyproject_toml.write_str(&project.replace("version = \"3.6.0\"", "version = \"3.5.0\""))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    pyproject_toml.write_str(&project.replace(
        "name = \"anyio\"\n\n",
        "name = \"anyio\"\nrequires-dist = [\"unused\"]\n\n",
    ))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Removing the exact entry activates the fallback and invalidates the lock.
    let fallback = project.replace(
        "[[tool.uv.dependency-metadata]]\nname = \"anyio\"\nversion = \"3.7.0\"\n\n",
        "",
    );
    pyproject_toml.write_str(&fallback)?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Adding an exact entry that shadows the recorded fallback also invalidates the lock.
    pyproject_toml.write_str(&project)?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Changes to relevant metadata still invalidate the lock.
    pyproject_toml.write_str(&project.replace(
        "version = \"3.7.0\"",
        "version = \"3.7.0\"\nrequires-dist = [\"iniconfig\"]",
    ))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 3 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    Ok(())
}

/// Scoped declarations can affect candidate policy even when their parent is absent.
#[cfg(feature = "test-pypi")]
#[test]
fn candidate_policy() -> Result<()> {
    for kind in ["override-dependencies", "constraint-dependencies"] {
        let context = uv_test::test_context!("3.12").with_exclude_newer("2026-01-01T00:00:00Z");
        let pyproject_toml = context.temp_dir.child("pyproject.toml");
        let project = indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12,<3.13"
        dependencies = ["numpy>=2.3"]

        [tool.uv]
        preview-features = ["resolution-inputs"]
        override-dependencies = [
            { package = { name = "absent" }, dependencies = ["numpy==2.4.0rc1"] },
        ]
        exclude-dependencies = [
            { package = { name = "absent" }, dependencies = ["numpy"] },
        ]
    "#};
        let project = project.replace("override-dependencies", kind);
        pyproject_toml.write_str(&project)?;
        insta::allow_duplicates! {
            uv_snapshot!(context.filters(), context.tree(), @"
            exit_code: 0 (success)
            ----- stdout -----
            project v0.1.0
            └── numpy v2.3.5

            ----- stderr -----
            Resolved 2 packages in [TIME]
            ");
        }
        insta::allow_duplicates! {
            uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
            exit_code: 0 (success)
            ----- stderr -----
            Resolved 2 packages in [TIME]
            ");
        }

        // Removing the exclusion allows the scoped declaration to opt NumPy into prereleases.
        pyproject_toml
            .write_str(&project.replace("dependencies = [\"numpy\"]", "dependencies = []"))?;
        insta::allow_duplicates! {
            uv_snapshot!(context.filters(), context.lock().arg("--locked"), @"
            exit_code: 1 (failure)
            ----- stderr -----
            Resolved 2 packages in [TIME]
            error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

            hint: To update the lockfile, run `uv lock`.
            ");
        }

        // Retain a previously relevant scope during validation even if its new dependency is absent.
        pyproject_toml.write_str(&project.replace("numpy==2.4.0rc1", "unrelated==1.0"))?;
        insta::allow_duplicates! {
            uv_snapshot!(context.filters(), context.lock().arg("--locked"), @"
            exit_code: 1 (failure)
            ----- stderr -----
            Resolved 2 packages in [TIME]
            error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

            hint: To update the lockfile, run `uv lock`.
            ");
        }
    }
    Ok(())
}

/// Inputs consulted before backtracking still matter even when their packages are absent from the lock.
#[test]
fn backtracking() -> Result<()> {
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "resolution-inputs-backtracking"

        [root]
        [expected]
        satisfiable = true

        [packages.a.versions."1.0.0"]
        sdist = false
        [packages.a.versions."2.0.0"]
        requires = ["discarded==1.0.0"]
        sdist = false
        [packages.discarded.versions."1.0.0"]
        sdist = false
        [packages.leaf.versions."1.0.0"]
        sdist = false
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let context = uv_test::test_context!("3.12").with_filters(
        server
            .files()
            .map(|(filename, hash)| (hash.to_owned(), format!("[SHA256:{filename}]"))),
    );
    let pyproject = context.temp_dir.child("pyproject.toml");
    let project = indoc! {r#"
        [project]
        name = "project"
        version = "1.0"
        requires-python = ">=3.12"
        dependencies = ["a"]

        [tool.uv]
        preview-features = ["resolution-inputs"]
        constraint-dependencies = [
            "leaf>=2",
            { package = { name = "discarded" }, dependencies = ["leaf>=1"] },
        ]
        override-dependencies = [
            { package = { name = "discarded" }, dependencies = ["leaf==1.0.0"] },
        ]
        exclude-dependencies = [
            { package = { name = "discarded" }, dependencies = ["unrelated"] },
        ]
        exclude-newer-package = { discarded = "2025-01-01T00:00:00Z" }
        dependency-metadata = [{ name = "discarded", version = "1.0.0", requires-dist = ["leaf"] }]
    "#};
    pyproject.write_str(project)?;
    uv_snapshot!(context.filters(), context.tree().arg("--index-url").arg(server.index_url()), @"
    exit_code: 0 (success)
    ----- stdout -----
    project v1.0
    └── a v1.0.0

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [options.exclude-newer-package]
        discarded = "2025-01-01T00:00:00Z"

        [manifest]
        constraints = [
            { package = { name = "discarded" }, dependencies = [{ name = "leaf", specifier = ">=1" }] },
            { name = "leaf", specifier = ">=2" },
        ]
        overrides = [{ package = { name = "discarded" }, dependencies = [{ name = "leaf", specifier = "==1.0.0" }] }]
        excludes = [{ package = { name = "discarded" }, dependencies = ["unrelated"] }]

        [manifest.resolution-inputs]
        constraints = [
            "a",
            "discarded",
            "leaf",
            "project",
        ]
        overrides = [
            "a",
            "discarded",
            "leaf",
            "project",
        ]
        exclusions = [
            "a",
            "discarded",
            "leaf",
            "project",
        ]
        scoped-constraints = [
            "a",
            "discarded",
            "project",
        ]
        scoped-overrides = [
            "a",
            "discarded",
            "project",
        ]
        scoped-exclusions = [
            "a",
            "discarded",
            "project",
        ]
        candidate-policy = [
            "a",
            "discarded",
        ]
        exclude-newer = [
            "a",
            "discarded",
            "leaf",
        ]
        dependency-metadata = [
            { name = "a", version = "1.0.0" },
            { name = "a", version = "2.0.0" },
            { name = "discarded", version = "1.0.0" },
            { name = "project" },
        ]

        [[manifest.dependency-metadata]]
        name = "discarded"
        version = "1.0.0"
        requires-dist = ["leaf"]

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:[SHA256:a-1.0.0-py3-none-any.whl]", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
        ]

        [package.metadata]
        requires-dist = [{ name = "a" }]
        "#);
    });
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--index-url").arg(server.index_url()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Every mutation changes a consulted input, including a previously unsuccessful metadata lookup.
    for changed in [
        project.replace("leaf>=2", "leaf>=1"),
        project.replace(
            "dependency-metadata = [",
            "dependency-metadata = [{ name = \"a\", version = \"1.0.0\" }, ",
        ),
    ] {
        pyproject.write_str(&changed)?;
        insta::allow_duplicates! {
            uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--index-url").arg(server.index_url()), @"
            exit_code: 1 (failure)
            ----- stderr -----
            Resolved 2 packages in [TIME]
            error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

            hint: To update the lockfile, run `uv lock`.
            ");
        }
    }
    // Upload cutoffs also retain consultations for packages absent from the final graph.
    pyproject.write_str(&project.replace("2025-01-01", "2024-03-25"))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--index-url").arg(server.index_url()), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolving despite existing lockfile due to change of exclude newer timestamp from `2025-01-01T00:00:00Z` to `2024-03-25T00:00:00Z` for package `discarded`
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    // Relaxing the constraint makes the previously discarded branch viable on an upgrade.
    pyproject.write_str(&project.replace("leaf>=2", "leaf>=1"))?;
    uv_snapshot!(context.filters(), context.tree().arg("--upgrade").arg("--index-url").arg(server.index_url()), @"
    exit_code: 0 (success)
    ----- stdout -----
    project v1.0
    └── a v2.0.0
        └── discarded v1.0.0
            └── leaf v1.0.0

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");
    Ok(())
}

#[test]
fn metadata_unknown_version() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("child/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "child"
        version = "1.0"
    "#})?;
    let pyproject = context.temp_dir.child("pyproject.toml");
    let project = indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [tool.uv]
        preview-features = ["resolution-inputs"]

        [tool.uv.sources]
        child = { path = "child" }

        [[tool.uv.dependency-metadata]]
        name = "child"
        version = "1.0"

        [[tool.uv.dependency-metadata]]
        name = "child"
        version = "2.0"

        [[tool.uv.dependency-metadata]]
        name = "child"
    "#};
    pyproject.write_str(project)?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]

        [manifest.resolution-inputs]
        constraints = [
            "child",
            "project",
        ]
        overrides = [
            "child",
            "project",
        ]
        exclusions = [
            "child",
            "project",
        ]
        scoped-constraints = [
            "child",
            "project",
        ]
        scoped-overrides = [
            "child",
            "project",
        ]
        scoped-exclusions = ["project"]
        dependency-metadata = [
            { name = "child" },
            { name = "project" },
        ]

        [[manifest.dependency-metadata]]
        name = "child"

        [[manifest.dependency-metadata]]
        name = "child"
        version = "1.0"

        [[manifest.dependency-metadata]]
        name = "child"
        version = "2.0"

        [[package]]
        name = "child"
        version = "1.0"
        source = { directory = "child" }

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "child" },
        ]

        [package.metadata]
        requires-dist = [{ name = "child", directory = "child" }]
        "#);
    });
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Complete declarations participate in validation, even for an unmatched version.
    pyproject.write_str(&project.replace("version = \"2.0\"", "version = \"3.0\""))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    Ok(())
}

#[cfg(feature = "test-pypi")]
#[test]
fn metadata_forks() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0; sys_platform == 'win32'",
            "anyio==3.6.0; sys_platform != 'win32'",
        ]

        [tool.uv]
        preview-features = ["resolution-inputs"]

        [[tool.uv.dependency-metadata]]
        name = "anyio"
        version = "3.7.0"

        [[tool.uv.dependency-metadata]]
        name = "anyio"
        version = "3.5.0"

        [[tool.uv.dependency-metadata]]
        name = "anyio"
    "#})?;
    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'win32'",
            "sys_platform != 'win32'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]

        [manifest.resolution-inputs]
        constraints = [
            "anyio",
            "project",
        ]
        overrides = [
            "anyio",
            "project",
        ]
        exclusions = [
            "anyio",
            "project",
        ]
        scoped-constraints = ["project"]
        scoped-overrides = [
            "anyio",
            "project",
        ]
        scoped-exclusions = ["project"]
        candidate-policy = ["anyio"]
        exclude-newer = ["anyio"]
        dependency-metadata = [
            { name = "anyio", version = "3.6.0" },
            { name = "anyio", version = "3.7.0" },
            { name = "project" },
        ]

        [[manifest.dependency-metadata]]
        name = "anyio"

        [[manifest.dependency-metadata]]
        name = "anyio"
        version = "3.7.0"

        [[package]]
        name = "anyio"
        version = "3.6.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform != 'win32'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/2b/e3/f23b7986619c7de90f63b1ac898074ddd9641e70e1677eec3d9f40969aa3/anyio-3.6.0.tar.gz", hash = "sha256:056bd22787f4bc59cb6d9a6873929618c2233d64903eed64177070fd072bd9ca", size = 140145, upload-time = "2022-05-13T09:54:55.302Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f4/17/86c924b1371353f785e1515b830e118fcd46880ae8d7a7b11116e5f81d6f/anyio-3.6.0-py3-none-any.whl", hash = "sha256:5bd42d66c9c382e657c9acba60f9459d747c192513e76dcae8b1c611e55174ef", size = 80603, upload-time = "2022-05-13T09:54:53.623Z" },
        ]

        [[package]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform == 'win32'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737, upload-time = "2023-05-27T11:12:46.688Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873, upload-time = "2023-05-27T11:12:44.474Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "anyio", version = "3.6.0", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform != 'win32'" },
            { name = "anyio", version = "3.7.0", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform == 'win32'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", marker = "sys_platform != 'win32'", specifier = "==3.6.0" },
            { name = "anyio", marker = "sys_platform == 'win32'", specifier = "==3.7.0" },
        ]
        "#);
    });
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-preview"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");
    Ok(())
}
