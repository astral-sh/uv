use anyhow::Result;
use assert_fs::prelude::*;
use insta::assert_snapshot;

use crate::common::{uv_snapshot, TestContext};
use uv_static::EnvVars;

// All of the tests in this file should use `tool.uv.conflicts` in some way.
//
// They are split from `lock.rs` somewhat arbitrarily. Mostly because there are
// a lot of them, and `lock.rs` was growing large enough as it is.

/// This tests a "basic" case for specifying conflicting extras.
///
/// Namely, we check that 1) without declaring them conflicting,
/// resolution fails, 2) declaring them conflicting, resolution
/// succeeds, 3) install succeeds, 4) install fails when requesting two
/// or more extras that are declared to conflict with each other.
///
/// This test was inspired by:
/// <https://github.com/astral-sh/uv/issues/8024>
#[test]
fn extra_basic() -> Result<()> {
    let context = TestContext::new("3.12");

    // First we test that resolving with two extras that have
    // conflicting dependencies fails.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.3.0"]
        extra2 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project[extra2] depends on sortedcontainers==2.4.0 and project[extra1] depends on sortedcontainers==2.3.0, we can conclude that project[extra1] and project[extra2] are incompatible.
          And because your project requires project[extra1] and project[extra2], we can conclude that your project's requirements are unsatisfiable.
    "###);

    // And now with the same extra configuration, we tell uv about
    // the conflicting extras, which forces it to resolve each in
    // their own fork.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [
              { extra = "extra1" },
              { extra = "extra2" },
            ],
        ]

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.3.0"]
        extra2 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", extra = "extra1" },
            { package = "project", extra = "extra2" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }

        [package.optional-dependencies]
        extra1 = [
            { name = "sortedcontainers", version = "2.3.0", source = { registry = "https://pypi.org/simple" } },
        ]
        extra2 = [
            { name = "sortedcontainers", version = "2.4.0", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "sortedcontainers", marker = "extra == 'extra1'", specifier = "==2.3.0" },
            { name = "sortedcontainers", marker = "extra == 'extra2'", specifier = "==2.4.0" },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.4.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e8/c4/ba2f8066cceb6f23394729afe52f3bf7adec04bf9ed2c820b39e19299111/sortedcontainers-2.4.0.tar.gz", hash = "sha256:25caa5a06cc30b6b83d11423433f65d1f9d76c4c6a0c90e3379eaa43b9bfdb88", size = 30594 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/32/46/9cb0e58b2deb7f82b84065f37f3bffeb12413f947f9388e4cac22c4621ce/sortedcontainers-2.4.0-py2.py3-none-any.whl", hash = "sha256:a163dcaede0f1c021485e957a39245190e74249897e2ae4b2aa38595db237ee0", size = 29575 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);
    // Another install, but with one of the extras enabled.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--extra=extra1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sortedcontainers==2.3.0
    "###);
    // Another install, but with the other extra enabled.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--extra=extra2"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - sortedcontainers==2.3.0
     + sortedcontainers==2.4.0
    "###);
    // And finally, installing both extras should error.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--all-extras"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Extras `extra1` and `extra2` are incompatible with the declared conflicts: {`project[extra1]`, `project[extra2]`}
    "###);
    // As should exporting them.
    uv_snapshot!(context.filters(), context.export().arg("--frozen").arg("--all-extras"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Extras `extra1` and `extra2` are incompatible with the declared conflicts: {`project[extra1]`, `project[extra2]`}
    "###);

    Ok(())
}

/// Like `lock_conflicting_extra_basic`, but defines three conflicting
/// extras instead of two.
#[test]
fn extra_basic_three_extras() -> Result<()> {
    let context = TestContext::new("3.12");

    // First we test that resolving with two extras that have
    // conflicting dependencies fails.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.2.0"]
        extra2 = ["sortedcontainers==2.3.0"]
        project3 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project[project3] depends on sortedcontainers==2.4.0 and project[extra1] depends on sortedcontainers==2.2.0, we can conclude that project[extra1] and project[project3] are incompatible.
          And because your project requires project[extra1] and project[project3], we can conclude that your project's requirements are unsatisfiable.
    "###);

    // And now with the same extra configuration, we tell uv about
    // the conflicting extras, which forces it to resolve each in
    // their own fork.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [
              { extra = "extra1" },
              { extra = "extra2" },
              { extra = "project3" },
            ],
        ]

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.2.0"]
        extra2 = ["sortedcontainers==2.3.0"]
        project3 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", extra = "extra1" },
            { package = "project", extra = "extra2" },
            { package = "project", extra = "project3" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }

        [package.optional-dependencies]
        extra1 = [
            { name = "sortedcontainers", version = "2.2.0", source = { registry = "https://pypi.org/simple" } },
        ]
        extra2 = [
            { name = "sortedcontainers", version = "2.3.0", source = { registry = "https://pypi.org/simple" } },
        ]
        project3 = [
            { name = "sortedcontainers", version = "2.4.0", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "sortedcontainers", marker = "extra == 'extra1'", specifier = "==2.2.0" },
            { name = "sortedcontainers", marker = "extra == 'extra2'", specifier = "==2.3.0" },
            { name = "sortedcontainers", marker = "extra == 'project3'", specifier = "==2.4.0" },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.2.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/83/c9/466c0f9b42a0563366bb7c39906d9c6673315f81516f55e3a23a99f52234/sortedcontainers-2.2.0.tar.gz", hash = "sha256:331f5b7acb6bdfaf0b0646f5f86c087e414c9ae9d85e2076ad2eacb17ec2f4ff", size = 30402 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0c/75/4f79725a6ad966f1985d96c5aeda0b27d00c23afa14e8566efcdee1380ad/sortedcontainers-2.2.0-py2.py3-none-any.whl", hash = "sha256:f0694fbe8d090fab0fbabbfecad04756fbbb35dc3c0f89e0f6965396fe815d25", size = 29386 },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.4.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e8/c4/ba2f8066cceb6f23394729afe52f3bf7adec04bf9ed2c820b39e19299111/sortedcontainers-2.4.0.tar.gz", hash = "sha256:25caa5a06cc30b6b83d11423433f65d1f9d76c4c6a0c90e3379eaa43b9bfdb88", size = 30594 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/32/46/9cb0e58b2deb7f82b84065f37f3bffeb12413f947f9388e4cac22c4621ce/sortedcontainers-2.4.0-py2.py3-none-any.whl", hash = "sha256:a163dcaede0f1c021485e957a39245190e74249897e2ae4b2aa38595db237ee0", size = 29575 },
        ]
        "###
        );
    });

    Ok(())
}

/// This tests that extras don't conflict with one another when they are in
/// distinct groups of extras.
#[test]
fn extra_multiple_not_conflicting1() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [
              { extra = "extra1" },
              { extra = "extra2" },
            ],
            [
              { extra = "project3" },
              { extra = "project4" },
            ],
        ]

        [project.optional-dependencies]
        extra1 = []
        extra2 = []
        project3 = []
        project4 = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);
    // extra1/extra2 conflict!
    uv_snapshot!(
        context.filters(),
        context.sync().arg("--frozen").arg("--extra=extra1").arg("--extra=extra2"),
        @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Extras `extra1` and `extra2` are incompatible with the declared conflicts: {`project[extra1]`, `project[extra2]`}
    "###);
    // project3/project4 conflict!
    uv_snapshot!(
        context.filters(),
        context.sync().arg("--frozen").arg("--extra=project3").arg("--extra=project4"),
        @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Extras `project3` and `project4` are incompatible with the declared conflicts: {`project[project3]`, `project[project4]`}
    "###);
    // ... but extra1/project3 does not.
    uv_snapshot!(
        context.filters(),
        context.sync().arg("--frozen").arg("--extra=extra1").arg("--extra=project3"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###);
    // ... and neither does extra2/project3.
    uv_snapshot!(
        context.filters(),
        context.sync().arg("--frozen").arg("--extra=extra2").arg("--extra=project3"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###);
    // And similarly, with project 4.
    uv_snapshot!(
        context.filters(),
        context.sync().arg("--frozen").arg("--extra=extra1").arg("--extra=project4"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###);
    // ... and neither does extra2/project3.
    uv_snapshot!(
        context.filters(),
        context.sync().arg("--frozen").arg("--extra=extra2").arg("--extra=project4"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###);

    Ok(())
}

/// This tests that if the user has conflicting extras, but puts them in two
/// distinct groups of extras, then resolution still fails. (Because the only
/// way to resolve them in different forks is to define the extras as directly
/// conflicting.)
#[test]
fn extra_multiple_not_conflicting2() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.3.0"]
        extra2 = ["sortedcontainers==2.4.0"]
        project3 = ["sortedcontainers==2.3.0"]
        project4 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Fails, as expected.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project[project4] depends on sortedcontainers==2.4.0 and project[extra1] depends on sortedcontainers==2.3.0, we can conclude that project[extra1] and project[project4] are incompatible.
          And because your project requires project[extra1] and project[project4], we can conclude that your project's requirements are unsatisfiable.
    "###);

    // If we define extra1/extra2 as conflicting and project3/project4
    // as conflicting, that still isn't enough! That's because extra1
    // conflicts with project4 and extra2 conflicts with project3.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [
              { extra = "extra1" },
              { extra = "extra2" },
            ],
            [
              { extra = "project3" },
              { extra = "project4" },
            ],
        ]

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.3.0"]
        extra2 = ["sortedcontainers==2.4.0"]
        project3 = ["sortedcontainers==2.3.0"]
        project4 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project[project3] depends on sortedcontainers==2.3.0 and project[extra2] depends on sortedcontainers==2.4.0, we can conclude that project[extra2] and project[project3] are incompatible.
          And because your project requires project[extra2] and project[project3], we can conclude that your project's requirements are unsatisfiable.
    "###);

    // One could try to declare all pairs of conflicting extras as
    // conflicting, but this doesn't quite work either. For example,
    // the first group of conflicting extra, extra1/extra2,
    // specifically allows project4 to be co-mingled with extra1 (and
    // similarly, project3 with extra2), which are conflicting.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [
              { extra = "extra1" },
              { extra = "extra2" },
            ],
            [
              { extra = "project3" },
              { extra = "project4" },
            ],
            [
              { extra = "extra1" },
              { extra = "project4" },
            ],
            [
              { extra = "extra2" },
              { extra = "project3" },
            ],
        ]

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.3.0"]
        extra2 = ["sortedcontainers==2.4.0"]
        project3 = ["sortedcontainers==2.3.0"]
        project4 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // We can also fix this by just putting them all in one big
    // group, even though extra1/project3 don't conflict and
    // extra2/project4 don't conflict.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [
              { extra = "extra1" },
              { extra = "extra2" },
              { extra = "project3" },
              { extra = "project4" },
            ],
        ]

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.3.0"]
        extra2 = ["sortedcontainers==2.4.0"]
        project3 = ["sortedcontainers==2.3.0"]
        project4 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    Ok(())
}

/// This tests that we handle two independent sets of conflicting
/// extras correctly.
#[test]
fn extra_multiple_independent() -> Result<()> {
    let context = TestContext::new("3.12");

    // If we don't declare any conflicting extras, then resolution
    // will of course fail.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.3.0"]
        extra2 = ["sortedcontainers==2.4.0"]
        project3 = ["anyio==4.1.0"]
        project4 = ["anyio==4.2.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project[project4] depends on anyio==4.2.0 and project[project3] depends on anyio==4.1.0, we can conclude that project[project3] and project[project4] are incompatible.
          And because your project requires project[project3] and project[project4], we can conclude that your project's requirements are unsatisfiable.
    "###);

    // OK, responding to the error, we declare our anyio extras
    // as conflicting. But now we should see sortedcontainers as
    // conflicting.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [
              { extra = "project3" },
              { extra = "project4" },
            ],
        ]

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.3.0"]
        extra2 = ["sortedcontainers==2.4.0"]
        project3 = ["anyio==4.1.0"]
        project4 = ["anyio==4.2.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project[extra2] depends on sortedcontainers==2.4.0 and project[extra1] depends on sortedcontainers==2.3.0, we can conclude that project[extra1] and project[extra2] are incompatible.
          And because your project requires project[extra1] and project[extra2], we can conclude that your project's requirements are unsatisfiable.
    "###);

    // Once we declare ALL our conflicting extras, resolution succeeds.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [
              { extra = "extra1" },
              { extra = "extra2" },
            ],
            [
              { extra = "project3" },
              { extra = "project4" },
            ],
        ]

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.3.0"]
        extra2 = ["sortedcontainers==2.4.0"]
        project3 = ["anyio==4.1.0"]
        project4 = ["anyio==4.2.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", extra = "extra1" },
            { package = "project", extra = "extra2" },
        ], [
            { package = "project", extra = "project3" },
            { package = "project", extra = "project4" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.1.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/6e/57/075e07fb01ae2b740289ec9daec670f60c06f62d04b23a68077fd5d73fab/anyio-4.1.0.tar.gz", hash = "sha256:5a0bec7085176715be77df87fc66d6c9d70626bd752fcc85f57cdbee5b3760da", size = 155773 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/85/4f/d010eca6914703d8e6be222165d02c3e708ed909cdb2b7af3743667f302e/anyio-4.1.0-py3-none-any.whl", hash = "sha256:56a415fbc462291813a94528a779597226619c8e78af7de0507333f700011e5f", size = 83924 },
        ]

        [[package]]
        name = "anyio"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz", hash = "sha256:e1875bb4b4e2de1669f4bc7869b6d3f54231cdced71605e6e64c9be77e3be50f", size = 158770 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bf/cd/d6d9bb1dadf73e7af02d18225cbd2c93f8552e13130484f1c8dcfece292b/anyio-4.2.0-py3-none-any.whl", hash = "sha256:745843b39e829e108e518c489b31dc757de7d2131d53fac32bd8df268227bfee", size = 85481 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }

        [package.optional-dependencies]
        extra1 = [
            { name = "sortedcontainers", version = "2.3.0", source = { registry = "https://pypi.org/simple" } },
        ]
        extra2 = [
            { name = "sortedcontainers", version = "2.4.0", source = { registry = "https://pypi.org/simple" } },
        ]
        project3 = [
            { name = "anyio", version = "4.1.0", source = { registry = "https://pypi.org/simple" } },
        ]
        project4 = [
            { name = "anyio", version = "4.2.0", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", marker = "extra == 'project3'", specifier = "==4.1.0" },
            { name = "anyio", marker = "extra == 'project4'", specifier = "==4.2.0" },
            { name = "sortedcontainers", marker = "extra == 'extra1'", specifier = "==2.3.0" },
            { name = "sortedcontainers", marker = "extra == 'extra2'", specifier = "==2.4.0" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.4.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e8/c4/ba2f8066cceb6f23394729afe52f3bf7adec04bf9ed2c820b39e19299111/sortedcontainers-2.4.0.tar.gz", hash = "sha256:25caa5a06cc30b6b83d11423433f65d1f9d76c4c6a0c90e3379eaa43b9bfdb88", size = 30594 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/32/46/9cb0e58b2deb7f82b84065f37f3bffeb12413f947f9388e4cac22c4621ce/sortedcontainers-2.4.0-py2.py3-none-any.whl", hash = "sha256:a163dcaede0f1c021485e957a39245190e74249897e2ae4b2aa38595db237ee0", size = 29575 },
        ]
        "###
        );
    });

    Ok(())
}

#[test]
fn extra_config_change_ignore_lockfile() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [
              { extra = "extra1" },
              { extra = "extra2" },
            ],
        ]

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.3.0"]
        extra2 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", extra = "extra1" },
            { package = "project", extra = "extra2" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }

        [package.optional-dependencies]
        extra1 = [
            { name = "sortedcontainers", version = "2.3.0", source = { registry = "https://pypi.org/simple" } },
        ]
        extra2 = [
            { name = "sortedcontainers", version = "2.4.0", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "sortedcontainers", marker = "extra == 'extra1'", specifier = "==2.3.0" },
            { name = "sortedcontainers", marker = "extra == 'extra2'", specifier = "==2.4.0" },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.4.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e8/c4/ba2f8066cceb6f23394729afe52f3bf7adec04bf9ed2c820b39e19299111/sortedcontainers-2.4.0.tar.gz", hash = "sha256:25caa5a06cc30b6b83d11423433f65d1f9d76c4c6a0c90e3379eaa43b9bfdb88", size = 30594 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/32/46/9cb0e58b2deb7f82b84065f37f3bffeb12413f947f9388e4cac22c4621ce/sortedcontainers-2.4.0-py2.py3-none-any.whl", hash = "sha256:a163dcaede0f1c021485e957a39245190e74249897e2ae4b2aa38595db237ee0", size = 29575 },
        ]
        "###
        );
    });

    // Re-run with `--locked` to check it's okay.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Now get rid of the conflicting group config, and check that `--locked`
    // fails.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.3.0"]
        extra2 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    // Re-run with `--locked`, which should now fail because of
    // the conflicting group config removal.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project[extra2] depends on sortedcontainers==2.4.0 and project[extra1] depends on sortedcontainers==2.3.0, we can conclude that project[extra1] and project[extra2] are incompatible.
          And because your project requires project[extra1] and project[extra2], we can conclude that your project's requirements are unsatisfiable.
    "###);

    Ok(())
}

/// This tests that we report an error when a requirement unconditionally
/// enables a conflicting extra.
#[test]
fn extra_unconditional() -> Result<()> {
    let context = TestContext::new("3.12");

    let root_pyproject_toml = context.temp_dir.child("pyproject.toml");
    root_pyproject_toml.write_str(
        r#"
        [project]
        name = "dummy"
        version = "0.1.0"
        requires-python = "==3.12.*"
        dependencies = [
          "proxy1[extra1,extra2]"
        ]

        [tool.uv.workspace]
        members = ["proxy1"]

        [tool.uv.sources]
        proxy1 = { workspace = true }

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    let proxy1_pyproject_toml = context.temp_dir.child("proxy1").child("pyproject.toml");
    proxy1_pyproject_toml.write_str(
        r#"
        [project]
        name = "proxy1"
        version = "0.1.0"
        requires-python = "==3.12.*"
        dependencies = []

        [project.optional-dependencies]
        extra1 = ["anyio==4.1.0"]
        extra2 = ["anyio==4.2.0"]

        [tool.uv]
        conflicts = [
          [
            { extra = "extra1" },
            { extra = "extra2" },
          ],
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Found conflicting extra `extra1` unconditionally enabled in `proxy1[extra1,extra2] @ file://[TEMP_DIR]/proxy1`
    "###);

    // An error should occur even when only one conflicting extra is enabled.
    root_pyproject_toml.write_str(
        r#"
        [project]
        name = "dummy"
        version = "0.1.0"
        requires-python = "==3.12.*"
        dependencies = [
          "proxy1[extra1]"
        ]

        [tool.uv.workspace]
        members = ["proxy1"]

        [tool.uv.sources]
        proxy1 = { workspace = true }

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Found conflicting extra `extra1` unconditionally enabled in `proxy1[extra1] @ file://[TEMP_DIR]/proxy1`
    "###);

    // And same thing for the other extra.
    root_pyproject_toml.write_str(
        r#"
        [project]
        name = "dummy"
        version = "0.1.0"
        requires-python = "==3.12.*"
        dependencies = [
          "proxy1[extra2]"
        ]

        [tool.uv.workspace]
        members = ["proxy1"]

        [tool.uv.sources]
        proxy1 = { workspace = true }

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        "#,
    )?;
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Found conflicting extra `extra2` unconditionally enabled in `proxy1[extra2] @ file://[TEMP_DIR]/proxy1`
    "###);

    Ok(())
}

/// This tests how we deal with mutually conflicting extras that span multiple
/// packages in a workspace.
#[test]
fn extra_nested_across_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

    let root_pyproject_toml = context.temp_dir.child("pyproject.toml");
    root_pyproject_toml.write_str(
        r#"
        [project]
        name = "dummy"
        version = "0.1.0"
        requires-python = "==3.12.*"

        [project.optional-dependencies]
        extra1 = [
          "proxy1[extra1]",
        ]
        extra2 = [
          "proxy1[extra2]"
        ]

        [tool.uv.sources]
        proxy1 = { path = "./proxy1" }
        dummysub =  { workspace = true }

        [tool.uv.workspace]
        members = ["dummysub"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv]
        conflicts = [
          [
            { extra = "extra1" },
            { extra = "extra2" },
          ],
        ]
        "#,
    )?;

    let sub_pyproject_toml = context.temp_dir.child("dummysub").child("pyproject.toml");
    sub_pyproject_toml.write_str(
        r#"
        [project]
        name = "dummysub"
        version = "0.1.0"
        requires-python = "==3.12.*"

        [project.optional-dependencies]
        extra1 = [
          "proxy1[extra1]",
        ]
        extra2 = [
          "proxy1[extra2]"
        ]

        [tool.uv.sources]
        proxy1 = { path = "../proxy1" }

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv]
        conflicts = [
          [
            { extra = "extra1" },
            { extra = "extra2" },
          ],
        ]
        "#,
    )?;

    let proxy1_pyproject_toml = context.temp_dir.child("proxy1").child("pyproject.toml");
    proxy1_pyproject_toml.write_str(
        r#"
        [project]
        name = "proxy1"
        version = "0.1.0"
        requires-python = "==3.12.*"
        dependencies = []

        [project.optional-dependencies]
        extra1 = ["anyio==4.1.0"]
        extra2 = ["anyio==4.2.0"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    // In the scheme above, we declare that `dummy[extra1]` conflicts
    // with `dummy[extra2]`, and that `dummysub[extra1]` conflicts
    // with `dummysub[extra2]`. But we don't account for the fact that
    // `dummy[extra1]` conflicts with `dummysub[extra2]` and that
    // `dummy[extra2]` conflicts with `dummysub[extra1]`. So we end
    // up with a resolution failure.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because dummy[extra2] depends on proxy1[extra2] and only proxy1[extra2]==0.1.0 is available, we can conclude that dummy[extra2] depends on proxy1[extra2]==0.1.0. (1)

          Because proxy1[extra2]==0.1.0 depends on anyio==4.2.0 and proxy1[extra1]==0.1.0 depends on anyio==4.1.0, we can conclude that proxy1[extra1]==0.1.0 and proxy1[extra2]==0.1.0 are incompatible.
          And because we know from (1) that dummy[extra2] depends on proxy1[extra2]==0.1.0, we can conclude that dummy[extra2] and proxy1[extra1]==0.1.0 are incompatible.
          And because only proxy1[extra1]==0.1.0 is available and dummysub[extra1] depends on proxy1[extra1], we can conclude that dummysub[extra1] and dummy[extra2] are incompatible.
          And because your workspace requires dummy[extra2] and dummysub[extra1], we can conclude that your workspace's requirements are unsatisfiable.
    "###);

    // Now let's write out the full set of conflicts, taking
    // advantage of the optional `package` key.
    root_pyproject_toml.write_str(
        r#"
        [project]
        name = "dummy"
        version = "0.1.0"
        requires-python = "==3.12.*"

        [project.optional-dependencies]
        extra1 = [
          "proxy1[extra1]",
        ]
        extra2 = [
          "proxy1[extra2]"
        ]

        [tool.uv.sources]
        proxy1 = { path = "./proxy1" }
        dummysub =  { workspace = true }

        [tool.uv.workspace]
        members = ["dummysub"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv]
        conflicts = [
          [
            { extra = "extra1" },
            { extra = "extra2" },
          ],
          [
            { package = "dummysub", extra = "extra1" },
            { package = "dummysub", extra = "extra2" },
          ],
          [
            { extra = "extra1" },
            { package = "dummysub", extra = "extra2" },
          ],
          [
            { package = "dummysub", extra = "extra1" },
            { extra = "extra2" },
          ],
        ]
        "#,
    )?;
    // And we can remove the conflicts from `dummysub` since
    // there specified in `dummy`.
    sub_pyproject_toml.write_str(
        r#"
        [project]
        name = "dummysub"
        version = "0.1.0"
        requires-python = "==3.12.*"

        [project.optional-dependencies]
        extra1 = [
          "proxy1[extra1]",
        ]
        extra2 = [
          "proxy1[extra2]"
        ]

        [tool.uv.sources]
        proxy1 = { path = "../proxy1" }

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;
    // And now things should work.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    Ok(())
}

/// This tests a "basic" case for specifying conflicting groups.
#[test]
fn group_basic() -> Result<()> {
    let context = TestContext::new("3.12");

    // First we test that resolving with two groups that have
    // conflicting dependencies fails.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        description = "Add your description here"
        requires-python = ">=3.12"

        [dependency-groups]
        group1 = ["sortedcontainers==2.3.0"]
        group2 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project:group2 depends on sortedcontainers==2.4.0 and project:group1 depends on sortedcontainers==2.3.0, we can conclude that project:group1 and project:group2 are incompatible.
          And because your project requires project:group1 and project:group2, we can conclude that your project's requirements are unsatisfiable.
    "###);

    // And now with the same group configuration, we tell uv about
    // the conflicting groups, which forces it to resolve each in
    // their own fork.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        description = "Add your description here"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [
              { group = "group1" },
              { group = "group2" },
            ],
        ]

        [dependency-groups]
        group1 = ["sortedcontainers==2.3.0"]
        group2 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", group = "group1" },
            { package = "project", group = "group2" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }

        [package.dev-dependencies]
        group1 = [
            { name = "sortedcontainers", version = "2.3.0", source = { registry = "https://pypi.org/simple" } },
        ]
        group2 = [
            { name = "sortedcontainers", version = "2.4.0", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]

        [package.metadata.requires-dev]
        group1 = [{ name = "sortedcontainers", specifier = "==2.3.0" }]
        group2 = [{ name = "sortedcontainers", specifier = "==2.4.0" }]

        [[package]]
        name = "sortedcontainers"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.4.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e8/c4/ba2f8066cceb6f23394729afe52f3bf7adec04bf9ed2c820b39e19299111/sortedcontainers-2.4.0.tar.gz", hash = "sha256:25caa5a06cc30b6b83d11423433f65d1f9d76c4c6a0c90e3379eaa43b9bfdb88", size = 30594 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/32/46/9cb0e58b2deb7f82b84065f37f3bffeb12413f947f9388e4cac22c4621ce/sortedcontainers-2.4.0-py2.py3-none-any.whl", hash = "sha256:a163dcaede0f1c021485e957a39245190e74249897e2ae4b2aa38595db237ee0", size = 29575 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);
    // Another install, but with one of the groups enabled.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group=group1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sortedcontainers==2.3.0
    "###);
    // Another install, but with the other group enabled.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group=group2"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - sortedcontainers==2.3.0
     + sortedcontainers==2.4.0
    "###);
    // And finally, installing both groups should error.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group=group1").arg("--group=group2"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Groups `group1` and `group2` are incompatible with the declared conflicts: {`project:group1`, `project:group2`}
    "###);

    Ok(())
}

/// This tests a case of specifying conflicting groups, where some of the conflicts are enabled by
/// default.
#[test]
fn group_default() -> Result<()> {
    let context = TestContext::new("3.12");

    // Tell uv about the conflicting groups, which forces it to resolve each in
    // their own fork.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        description = "Add your description here"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [
              { group = "group1" },
              { group = "group2" },
            ],
        ]
        default-groups = ["group1"]

        [dependency-groups]
        group1 = ["sortedcontainers==2.3.0"]
        group2 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", group = "group1" },
            { package = "project", group = "group2" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }

        [package.dev-dependencies]
        group1 = [
            { name = "sortedcontainers", version = "2.3.0", source = { registry = "https://pypi.org/simple" } },
        ]
        group2 = [
            { name = "sortedcontainers", version = "2.4.0", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]

        [package.metadata.requires-dev]
        group1 = [{ name = "sortedcontainers", specifier = "==2.3.0" }]
        group2 = [{ name = "sortedcontainers", specifier = "==2.4.0" }]

        [[package]]
        name = "sortedcontainers"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.4.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e8/c4/ba2f8066cceb6f23394729afe52f3bf7adec04bf9ed2c820b39e19299111/sortedcontainers-2.4.0.tar.gz", hash = "sha256:25caa5a06cc30b6b83d11423433f65d1f9d76c4c6a0c90e3379eaa43b9bfdb88", size = 30594 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/32/46/9cb0e58b2deb7f82b84065f37f3bffeb12413f947f9388e4cac22c4621ce/sortedcontainers-2.4.0-py2.py3-none-any.whl", hash = "sha256:a163dcaede0f1c021485e957a39245190e74249897e2ae4b2aa38595db237ee0", size = 29575 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Install from the lockfile, which should include the `extra1` group by default.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sortedcontainers==2.3.0
    "###);

    // Another install, but with one of the groups enabled.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group=group1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 2 packages in [TIME]
    "###);

    // Another install, but with the other group enabled. This should error, since `group1` is
    // enabled by default.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group=group2"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Groups `group1` (enabled by default) and `group2` are incompatible with the declared conflicts: {`project:group1`, `project:group2`}
    "###);

    // If the group is explicitly requested, we should still fail, but shouldn't mark it as
    // "enabled by default".
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group=group1").arg("--group=group2"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Groups `group1` and `group2` are incompatible with the declared conflicts: {`project:group1`, `project:group2`}
    "###);

    // If we install via `--all-groups`, we should also avoid marking the group as "enabled by
    // default".
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--all-groups"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Groups `group1` and `group2` are incompatible with the declared conflicts: {`project:group1`, `project:group2`}
    "###);

    // Disabling the default group should succeed.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--no-group=group1").arg("--group=group2"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - sortedcontainers==2.3.0
     + sortedcontainers==2.4.0
    "###);

    Ok(())
}

/// This tests a case where we declare an extra and a group as conflicting.
#[test]
fn mixed() -> Result<()> {
    let context = TestContext::new("3.12");

    // First we test that resolving with a conflicting extra
    // and group fails.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        description = "Add your description here"
        requires-python = ">=3.12"

        [dependency-groups]
        group1 = ["sortedcontainers==2.3.0"]

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project[extra1] depends on sortedcontainers==2.4.0 and project:group1 depends on sortedcontainers==2.3.0, we can conclude that project:group1 and project[extra1] are incompatible.
          And because your project requires project[extra1] and project:group1, we can conclude that your project's requirements are unsatisfiable.
    "###);

    // And now with the same extra/group configuration, we tell uv
    // about the conflicting groups, which forces it to resolve each in
    // their own fork.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        description = "Add your description here"
        requires-python = ">=3.12"

        [tool.uv]
        conflicts = [
            [
              { group = "group1" },
              { extra = "extra1" },
            ],
        ]

        [dependency-groups]
        group1 = ["sortedcontainers==2.3.0"]

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.4.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", group = "group1" },
            { package = "project", extra = "extra1" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }

        [package.optional-dependencies]
        extra1 = [
            { name = "sortedcontainers", version = "2.4.0", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.dev-dependencies]
        group1 = [
            { name = "sortedcontainers", version = "2.3.0", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [{ name = "sortedcontainers", marker = "extra == 'extra1'", specifier = "==2.4.0" }]

        [package.metadata.requires-dev]
        group1 = [{ name = "sortedcontainers", specifier = "==2.3.0" }]

        [[package]]
        name = "sortedcontainers"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.4.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e8/c4/ba2f8066cceb6f23394729afe52f3bf7adec04bf9ed2c820b39e19299111/sortedcontainers-2.4.0.tar.gz", hash = "sha256:25caa5a06cc30b6b83d11423433f65d1f9d76c4c6a0c90e3379eaa43b9bfdb88", size = 30594 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/32/46/9cb0e58b2deb7f82b84065f37f3bffeb12413f947f9388e4cac22c4621ce/sortedcontainers-2.4.0-py2.py3-none-any.whl", hash = "sha256:a163dcaede0f1c021485e957a39245190e74249897e2ae4b2aa38595db237ee0", size = 29575 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);
    // Another install, but with the group enabled.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group=group1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sortedcontainers==2.3.0
    "###);
    // Another install, but with the extra enabled.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--extra=extra1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - sortedcontainers==2.3.0
     + sortedcontainers==2.4.0
    "###);
    // And finally, installing both the group and the extra should fail.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group=group1").arg("--extra=extra1"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Group `group1` and extra `extra1` are incompatible with the declared conflicts: {`project:group1`, `project[extra1]`}
    "###);

    Ok(())
}

#[test]
fn multiple_sources_index_disjoint_extras() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        cu118 = ["jinja2==3.1.2"]
        cu124 = ["jinja2==3.1.3"]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]
        conflicts = [
            [
                { extra = "cu118" },
                { extra = "cu124" },
            ],
        ]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", extra = "cu118" },
            { index = "torch-cu124", extra = "cu124" },
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://download.pytorch.org/whl/cu124"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", extra = "cu118" },
            { package = "project", extra = "cu124" },
        ]]

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://download.pytorch.org/whl/cu118" }
        resolution-markers = [
        ]
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.2-py3-none-any.whl", hash = "sha256:6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://download.pytorch.org/whl/cu124" }
        resolution-markers = [
        ]
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215 },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069 },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452 },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462 },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869 },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906 },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296 },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038 },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572 },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        cu118 = [
            { name = "jinja2", version = "3.1.2", source = { registry = "https://download.pytorch.org/whl/cu118" } },
        ]
        cu124 = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://download.pytorch.org/whl/cu124" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "jinja2", marker = "extra == 'cu118'", specifier = "==3.1.2", index = "https://download.pytorch.org/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://download.pytorch.org/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn multiple_sources_index_disjoint_groups() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        cu118 = ["jinja2==3.1.2"]
        cu124 = ["jinja2==3.1.3"]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]
        conflicts = [
            [
                { group = "cu118" },
                { group = "cu124" },
            ],
        ]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", group = "cu118" },
            { index = "torch-cu124", group = "cu124" },
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://download.pytorch.org/whl/cu124"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", group = "cu118" },
            { package = "project", group = "cu124" },
        ]]

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://download.pytorch.org/whl/cu118" }
        resolution-markers = [
        ]
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.2-py3-none-any.whl", hash = "sha256:6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://download.pytorch.org/whl/cu124" }
        resolution-markers = [
        ]
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215 },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069 },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452 },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462 },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869 },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906 },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296 },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038 },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572 },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.dev-dependencies]
        cu118 = [
            { name = "jinja2", version = "3.1.2", source = { registry = "https://download.pytorch.org/whl/cu118" } },
        ]
        cu124 = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://download.pytorch.org/whl/cu124" } },
        ]

        [package.metadata]

        [package.metadata.requires-dev]
        cu118 = [{ name = "jinja2", specifier = "==3.1.2", index = "https://download.pytorch.org/whl/cu118", conflict = { package = "project", group = "cu118" } }]
        cu124 = [{ name = "jinja2", specifier = "==3.1.3", index = "https://download.pytorch.org/whl/cu124", conflict = { package = "project", group = "cu124" } }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn multiple_sources_index_disjoint_extras_with_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        cu118 = ["jinja2[i18n]==3.1.2"]
        cu124 = ["jinja2[i18n]==3.1.3"]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]
        conflicts = [
            [
                { extra = "cu118" },
                { extra = "cu124" },
            ],
        ]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", extra = "cu118" },
            { index = "torch-cu124", extra = "cu124" },
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://download.pytorch.org/whl/cu124"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", extra = "cu118" },
            { package = "project", extra = "cu124" },
        ]]

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "babel"
        version = "2.16.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/2a/74/f1bc80f23eeba13393b7222b11d95ca3af2c1e28edca18af487137eefed9/babel-2.16.0.tar.gz", hash = "sha256:d1f3554ca26605fe173f3de0c65f750f5a42f924499bf134de6423582298e316", size = 9348104 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ed/20/bc79bc575ba2e2a7f70e8a1155618bb1301eaa5132a8271373a6903f73f8/babel-2.16.0-py3-none-any.whl", hash = "sha256:368b5b98b37c06b7daf6696391c3240c938b37767d4584413e8438c5c435fa8b", size = 9587599 },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://download.pytorch.org/whl/cu118" }
        resolution-markers = [
        ]
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.2-py3-none-any.whl", hash = "sha256:6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61" },
        ]

        [package.optional-dependencies]
        i18n = [
            { name = "babel" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://download.pytorch.org/whl/cu124" }
        resolution-markers = [
        ]
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa" },
        ]

        [package.optional-dependencies]
        i18n = [
            { name = "babel" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215 },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069 },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452 },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462 },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869 },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906 },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296 },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038 },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572 },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        cu118 = [
            { name = "jinja2", version = "3.1.2", source = { registry = "https://download.pytorch.org/whl/cu118" }, extra = ["i18n"] },
        ]
        cu124 = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://download.pytorch.org/whl/cu124" }, extra = ["i18n"] },
        ]

        [package.metadata]
        requires-dist = [
            { name = "jinja2", extras = ["i18n"], marker = "extra == 'cu118'", specifier = "==3.1.2", index = "https://download.pytorch.org/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", extras = ["i18n"], marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://download.pytorch.org/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn multiple_sources_index_disjoint_extras_with_marker() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        cu118 = ["jinja2==3.1.2"]
        cu124 = ["jinja2==3.1.3"]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]
        conflicts = [
            [
                { extra = "cu118" },
                { extra = "cu124" },
            ],
        ]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", extra = "cu118", marker = "sys_platform == 'darwin'" },
            { index = "torch-cu124", extra = "cu124" },
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://download.pytorch.org/whl/cu124"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform != 'darwin'",
        ]
        conflicts = [[
            { package = "project", extra = "cu118" },
            { package = "project", extra = "cu124" },
        ]]

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://download.pytorch.org/whl/cu118" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "markupsafe", marker = "sys_platform == 'darwin'" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.2-py3-none-any.whl", hash = "sha256:6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform != 'darwin'",
        ]
        dependencies = [
            { name = "markupsafe", marker = "sys_platform != 'darwin'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/7a/ff/75c28576a1d900e87eb6335b063fab47a8ef3c8b4d88524c4bf78f670cce/Jinja2-3.1.2.tar.gz", hash = "sha256:31351a702a408a9e7595a8fc6150fc3f43bb6bf7e319770cbc0db9df9437e852", size = 268239 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bc/c3/f068337a370801f372f2f8f6bad74a5c140f6fda3d9de154052708dd3c65/Jinja2-3.1.2-py3-none-any.whl", hash = "sha256:6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61", size = 133101 },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://download.pytorch.org/whl/cu124" }
        resolution-markers = [
        ]
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215 },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069 },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452 },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462 },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869 },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906 },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296 },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038 },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572 },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        cu118 = [
            { name = "jinja2", version = "3.1.2", source = { registry = "https://download.pytorch.org/whl/cu118" }, marker = "sys_platform == 'darwin'" },
            { name = "jinja2", version = "3.1.2", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform != 'darwin'" },
        ]
        cu124 = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://download.pytorch.org/whl/cu124" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "jinja2", marker = "sys_platform == 'darwin' and extra == 'cu118'", specifier = "==3.1.2", index = "https://download.pytorch.org/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", marker = "sys_platform != 'darwin' and extra == 'cu118'", specifier = "==3.1.2" },
            { name = "jinja2", marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://download.pytorch.org/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    Ok(())
}

/// Tests that forks excluding both conflicting extras are handled correctly.
///
/// This previously failed where running `uv sync` wouldn't install anything,
/// despite `sniffio` being an unconditional dependency.
#[test]
fn non_optional_dependency_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = [
          "sniffio>=1",
        ]

        [project.optional-dependencies]
        x1 = ["idna==3.5"]
        x2 = ["idna==3.6"]

        [tool.uv]
        conflicts = [
          [
            {package = "project", extra = "x1"},
            {package = "project", extra = "x2"},
          ],
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Like `non_optional_dependency_extra`, but for groups.
///
/// This test never regressed, but we added it here to ensure it doesn't.
#[test]
fn non_optional_dependency_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = [
          "sniffio>=1",
        ]

        [dependency-groups]
        g1 = ["idna==3.5"]
        g2 = ["idna==3.6"]

        [tool.uv]
        conflicts = [
          [
            {package = "project", group = "g1"},
            {package = "project", group = "g2"},
          ],
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// This tests a case where there are three extras, with two conflicting (`foo`
/// and `bar`), and the third (`baz`) containing a dependent (`anyio`) of
/// both dependencies (`idna==3.5` and `idna==3.6`) listed in the conflicting
/// extras.
///
/// This is a regression test for a more minimal case than was reported[1].
/// Specifically, this would produce an ambiguous lock file where both
/// `idna==3.5` and `idna==3.6` could be installed in some circumstances.
///
/// [1]: <https://github.com/astral-sh/uv/issues/9289>
#[test]
fn shared_optional_dependency_extra1() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        foo = [
          "idna==3.5",
        ]
        bar = [
          "idna==3.6",
        ]
        baz = [
          "anyio",
        ]

        [tool.uv]
        conflicts = [
          [
            { extra = "foo" },
            { extra = "bar" },
          ],
        ]
        "#,
    )?;

    // This shouldn't install two versions of `idna`, only one, `idna==3.5`.
    uv_snapshot!(context.filters(), context.sync().arg("--extra=baz").arg("--extra=foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.5
     + idna==3.6
     + sniffio==1.3.1
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", extra = "foo" },
            { package = "project", extra = "bar" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.5"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        bar = [
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
        ]
        baz = [
            { name = "anyio" },
        ]
        foo = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", marker = "extra == 'baz'" },
            { name = "idna", marker = "extra == 'bar'", specifier = "==3.6" },
            { name = "idna", marker = "extra == 'foo'", specifier = "==3.5" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    Ok(())
}

/// Another variation on `shared_optional_dependency_extra1`, but with
/// a slightly different outcome. In this case, when one of the extras
/// is enabled, the `sniffio` dependency was not installed.
///
/// Regression test for: <https://github.com/astral-sh/uv/issues/9289>
/// Regression test for: <https://github.com/astral-sh/uv/issues/9622>
/// Regression test for: <https://github.com/astral-sh/uv/issues/9640>
#[test]
fn shared_optional_dependency_extra2() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11,<3.12"
        dependencies = []

        [project.optional-dependencies]
        foo = [
          "idna==3.5",
          "anyio",
        ]
        bar = [
          "idna==3.6",
          "anyio",
        ]

        [tool.uv]
        conflicts = [
          [
            { extra = "foo" },
            { extra = "bar" },
          ],
        ]
        "#,
    )?;

    // This shouldn't install two versions of `idna`, only one, `idna==3.5`.
    uv_snapshot!(context.filters(), context.sync().arg("--extra=bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.10
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 5 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = "==3.11.*"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", extra = "foo" },
            { package = "project", extra = "bar" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.5"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        bar = [
            { name = "anyio" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
        ]
        foo = [
            { name = "anyio" },
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", marker = "extra == 'bar'" },
            { name = "anyio", marker = "extra == 'foo'" },
            { name = "idna", marker = "extra == 'bar'", specifier = "==3.6" },
            { name = "idna", marker = "extra == 'foo'", specifier = "==3.5" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    Ok(())
}

/// Like `shared_optional_dependency_extra1`, but puts the dependent
/// in the list of production dependencies instead of as an optional
/// dependency.
///
/// Regression test for: <https://github.com/astral-sh/uv/issues/9289>
#[test]
fn shared_dependency_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [project.optional-dependencies]
        foo = [
          "idna==3.5",
        ]
        bar = [
          "idna==3.6",
        ]

        [tool.uv]
        conflicts = [
          [
            { extra = "foo" },
            { extra = "bar" },
          ],
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", extra = "foo" },
            { package = "project", extra = "bar" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.5"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.optional-dependencies]
        bar = [
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
        ]
        foo = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio" },
            { name = "idna", marker = "extra == 'bar'", specifier = "==3.6" },
            { name = "idna", marker = "extra == 'foo'", specifier = "==3.5" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // This shouldn't install two versions of `idna`, only one, `idna==3.5`.
    // So this should remove `idna==3.6` installed above.
    uv_snapshot!(context.filters(), context.sync().arg("--extra=foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 2 packages in [TIME]
     + idna==3.5
     ~ idna==3.6
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--extra=bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     ~ idna==3.5
     ~ idna==3.6
    "###);

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     ~ idna==3.5
     ~ idna==3.6
    "###);

    Ok(())
}

/// This test ensures that when `--extra=foo` is given in the CLI, it is
/// appropriately namespaced to the correct package. That is, it doesn't
/// erroneously enable _every_ extra named `foo`, but only the top-level extra
/// named `foo`.
///
/// This isn't a regression test from `main`, but is a regression test for a
/// bug found in ongoing work. (Where I wasn't properly namespacing extras with
/// their corresponding package names.)
///
/// Ref <https://github.com/astral-sh/uv/issues/9289>
#[test]
fn extras_are_namespaced() -> Result<()> {
    let context = TestContext::new("3.12");

    let root_pyproject_toml = context.temp_dir.child("pyproject.toml");
    root_pyproject_toml.write_str(
        r#"
[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.11,<3.12"
dependencies = [
  "proxy1",
  "anyio>=4",
]

[tool.uv.workspace]
members = ["proxy1"]

[project.optional-dependencies]
x1 = ["idna==3.6"]

[tool.uv.sources]
proxy1 = { workspace = true }

[tool.uv]
conflicts = [
  [
    {package = "project", extra = "x1"},
    {package = "proxy1", extra = "x2"},
    {package = "proxy1", extra = "x3"},
  ],
]
        "#,
    )?;

    let proxy1_pyproject_toml = context.temp_dir.child("proxy1").child("pyproject.toml");
    proxy1_pyproject_toml.write_str(
        r#"
        [project]
        name = "proxy1"
        version = "0.1.0"
        requires-python = ">=3.11,<3.12"
        dependencies = []

        [project.optional-dependencies]
        x2 = ["idna==3.4"]
        x3 = ["idna==3.5"]
        "#,
    )?;

    // I believe there are multiple valid solutions here, but the main
    // thing is that `x2` should _not_ activate the `idna==3.4` dependency
    // in `proxy1`. The `--extra=x2` should be a no-op, since there is no
    // `x2` extra in the top level `pyproject.toml`.
    uv_snapshot!(context.filters(), context.sync().arg("--extra=x2"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.11
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 7 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.5
     + sniffio==1.3.1
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = "==3.11.*"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", extra = "x1" },
            { package = "proxy1", extra = "x2" },
            { package = "proxy1", extra = "x3" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "project",
            "proxy1",
        ]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna", version = "3.4", source = { registry = "https://pypi.org/simple" } },
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.4"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8b/e1/43beb3d38dba6cb420cefa297822eac205a277ab43e5ba5d5c46faf96438/idna-3.4.tar.gz", hash = "sha256:814f528e8dead7d329833b91c5faa87d60bf71824cd12a7530b5526063d02cb4", size = 183077 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fc/34/3030de6f1370931b9dbb4dad48f6ab1015ab1d32447850b9fc94e60097be/idna-3.4-py3-none-any.whl", hash = "sha256:90b77e79eaa3eba6de819a0c442c0b4ceefc341a7a2ab77d7562bf49f425c5c2", size = 61538 },
        ]

        [[package]]
        name = "idna"
        version = "3.5"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "anyio" },
            { name = "proxy1" },
        ]

        [package.optional-dependencies]
        x1 = [
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", specifier = ">=4" },
            { name = "idna", marker = "extra == 'x1'", specifier = "==3.6" },
            { name = "proxy1", virtual = "proxy1" },
        ]

        [[package]]
        name = "proxy1"
        version = "0.1.0"
        source = { virtual = "proxy1" }

        [package.optional-dependencies]
        x2 = [
            { name = "idna", version = "3.4", source = { registry = "https://pypi.org/simple" } },
        ]
        x3 = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "idna", marker = "extra == 'x2'", specifier = "==3.4" },
            { name = "idna", marker = "extra == 'x3'", specifier = "==3.5" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    Ok(())
}

/// This tests a case I stumbled on while working on [1] where conflict markers
/// were written when they didn't need to be.
///
/// That is, the conflict markers written here were correct, but redundant with
/// the fact that `cu118` and `cu124` could not be enabled simultaneously.
/// In other words, this is a regression test for the conflict marker
/// simplification I did as part of fixing [1].
///
/// [1]: <https://github.com/astral-sh/uv/issues/9289>
#[test]
fn jinja_no_conflict_markers1() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        cu118 = ["jinja2[i18n]==3.1.2"]
        cu124 = ["jinja2[i18n]==3.1.3"]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]
        conflicts = [
            [
                { extra = "cu118" },
                { extra = "cu124" },
            ],
        ]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", extra = "cu118" },
            { index = "torch-cu124", extra = "cu124" },
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://download.pytorch.org/whl/cu124"
        explicit = true
        "#,
    )?;

    let mut cmd = context.sync();
    // I guess --exclude-newer doesn't work with the torch indices?
    // That's because the Torch indices are missing the upload date
    // metadata. We pin our versions anyway, so this should be fine.
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    uv_snapshot!(context.filters(), cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Audited in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "project", extra = "cu118" },
            { package = "project", extra = "cu124" },
        ]]

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "babel"
        version = "2.16.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/2a/74/f1bc80f23eeba13393b7222b11d95ca3af2c1e28edca18af487137eefed9/babel-2.16.0.tar.gz", hash = "sha256:d1f3554ca26605fe173f3de0c65f750f5a42f924499bf134de6423582298e316", size = 9348104 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ed/20/bc79bc575ba2e2a7f70e8a1155618bb1301eaa5132a8271373a6903f73f8/babel-2.16.0-py3-none-any.whl", hash = "sha256:368b5b98b37c06b7daf6696391c3240c938b37767d4584413e8438c5c435fa8b", size = 9587599 },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://download.pytorch.org/whl/cu118" }
        resolution-markers = [
        ]
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.2-py3-none-any.whl", hash = "sha256:6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61" },
        ]

        [package.optional-dependencies]
        i18n = [
            { name = "babel" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://download.pytorch.org/whl/cu124" }
        resolution-markers = [
        ]
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa" },
        ]

        [package.optional-dependencies]
        i18n = [
            { name = "babel" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215 },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069 },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452 },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462 },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869 },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906 },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296 },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038 },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572 },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        cu118 = [
            { name = "jinja2", version = "3.1.2", source = { registry = "https://download.pytorch.org/whl/cu118" }, extra = ["i18n"] },
        ]
        cu124 = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://download.pytorch.org/whl/cu124" }, extra = ["i18n"] },
        ]

        [package.metadata]
        requires-dist = [
            { name = "jinja2", extras = ["i18n"], marker = "extra == 'cu118'", specifier = "==3.1.2", index = "https://download.pytorch.org/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", extras = ["i18n"], marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://download.pytorch.org/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        "###
        );
    });

    Ok(())
}

/// Like `jinja_no_conflict_markers1`, but includes a PEP 508 marker
/// to spice things up. As with `jinja_no_conflict_markers1`, we
/// shouldn't see any conflict markers in the lock file here.
#[test]
fn jinja_no_conflict_markers2() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        cu118 = ["jinja2==3.1.2"]
        cu124 = ["jinja2==3.1.3"]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]
        conflicts = [
            [
                { extra = "cu118" },
                { extra = "cu124" },
            ],
        ]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", extra = "cu118", marker = "sys_platform == 'darwin'" },
            { index = "torch-cu124", extra = "cu124" },
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://download.pytorch.org/whl/cu124"
        explicit = true
        "#,
    )?;

    let mut cmd = context.sync();
    // I guess --exclude-newer doesn't work with the torch indices?
    // That's because the Torch indices are missing the upload date
    // metadata. We pin our versions anyway, so this should be fine.
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    uv_snapshot!(context.filters(), cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Audited in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform != 'darwin'",
        ]
        conflicts = [[
            { package = "project", extra = "cu118" },
            { package = "project", extra = "cu124" },
        ]]

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://download.pytorch.org/whl/cu118" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "markupsafe", marker = "sys_platform == 'darwin'" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.2-py3-none-any.whl", hash = "sha256:6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform != 'darwin'",
        ]
        dependencies = [
            { name = "markupsafe", marker = "sys_platform != 'darwin'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/7a/ff/75c28576a1d900e87eb6335b063fab47a8ef3c8b4d88524c4bf78f670cce/Jinja2-3.1.2.tar.gz", hash = "sha256:31351a702a408a9e7595a8fc6150fc3f43bb6bf7e319770cbc0db9df9437e852", size = 268239 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bc/c3/f068337a370801f372f2f8f6bad74a5c140f6fda3d9de154052708dd3c65/Jinja2-3.1.2-py3-none-any.whl", hash = "sha256:6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61", size = 133101 },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://download.pytorch.org/whl/cu124" }
        resolution-markers = [
        ]
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215 },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069 },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452 },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462 },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869 },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906 },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296 },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038 },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572 },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        cu118 = [
            { name = "jinja2", version = "3.1.2", source = { registry = "https://download.pytorch.org/whl/cu118" }, marker = "sys_platform == 'darwin'" },
            { name = "jinja2", version = "3.1.2", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform != 'darwin'" },
        ]
        cu124 = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://download.pytorch.org/whl/cu124" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "jinja2", marker = "sys_platform == 'darwin' and extra == 'cu118'", specifier = "==3.1.2", index = "https://download.pytorch.org/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", marker = "sys_platform != 'darwin' and extra == 'cu118'", specifier = "==3.1.2" },
            { name = "jinja2", marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://download.pytorch.org/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        "###
        );
    });

    Ok(())
}

/// This tests a somewhat pathological case where a package has an extra whose
/// name corresponds to uv's conflicting extra encoding of another extra. That
/// is, an extra `foo` and an extra `extra-3-pkg-foo`.
///
/// In theory these could collide and cause problems. But in practice, we don't
/// involve the `extra == "foo"` marker in the same places, I believe, as we do
/// `extra == "extra-3-pkg-foo"`.
///
/// Ref: <https://github.com/astral-sh/uv/pull/9370#discussion_r1876083284>
#[test]
fn collision_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "pkg"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [project.optional-dependencies]
        foo = ["idna==3.5"]
        bar = ["idna==3.6"]
        extra-3-pkg-foo = ["sortedcontainers>=2"]

        [tool.uv]
        conflicts = [
          [
            { extra = "foo" },
            { extra = "bar" },
          ],
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock,
            @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
        ]
        conflicts = [[
            { package = "pkg", extra = "foo" },
            { package = "pkg", extra = "bar" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-foo'" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-bar' or extra != 'extra-3-pkg-foo'" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.5"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "pkg"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.optional-dependencies]
        bar = [
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
        ]
        extra-3-pkg-foo = [
            { name = "sortedcontainers" },
        ]
        foo = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio" },
            { name = "idna", marker = "extra == 'bar'", specifier = "==3.6" },
            { name = "idna", marker = "extra == 'foo'", specifier = "==3.5" },
            { name = "sortedcontainers", marker = "extra == 'extra-3-pkg-foo'", specifier = ">=2" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.4.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e8/c4/ba2f8066cceb6f23394729afe52f3bf7adec04bf9ed2c820b39e19299111/sortedcontainers-2.4.0.tar.gz", hash = "sha256:25caa5a06cc30b6b83d11423433f65d1f9d76c4c6a0c90e3379eaa43b9bfdb88", size = 30594 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/32/46/9cb0e58b2deb7f82b84065f37f3bffeb12413f947f9388e4cac22c4621ce/sortedcontainers-2.4.0-py2.py3-none-any.whl", hash = "sha256:a163dcaede0f1c021485e957a39245190e74249897e2ae4b2aa38595db237ee0", size = 29575 },
        ]
        "###
        );
    });

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    // The extra `extra-3-pkg-foo` is meant to collide with the encoded
    // extra name generated by the extra `foo`. When `foo` is enabled,
    // we expect to see `idna==3.5`, but when `extra-3-pkg-foo` is enabled,
    // we don't. Instead, we should just see `anyio` and `sortedcontainers`
    // installed.
    uv_snapshot!(
        context.filters(),
        context.sync().arg("--frozen").arg("--extra=extra-3-pkg-foo"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sortedcontainers==2.4.0
    "###
    );

    // Verify that activating `foo` does result in `idna==3.5`.
    uv_snapshot!(
        context.filters(),
        context.sync().arg("--frozen").arg("--extra=foo"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 1 package in [TIME]
     - idna==3.6
     + idna==3.5
     - sortedcontainers==2.4.0
    "###
    );

    // And that activating both is fine and dandy. We get `idna==3.5`
    // and `sortedcontainers`.
    uv_snapshot!(
        context.filters(),
        context.sync().arg("--frozen").arg("--extra=extra-3-pkg-foo").arg("--extra=foo"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 1 package in [TIME]
     + sortedcontainers==2.4.0
    "###
    );

    Ok(())
}
