use anyhow::Result;
use assert_fs::prelude::*;
use insta::assert_snapshot;

use crate::common::{uv_snapshot, TestContext};

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
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", extra = "extra1" },
            { package = "project", extra = "extra2" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

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
        provides-extras = ["extra1", "extra2"]

        [[package]]
        name = "sortedcontainers"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
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
    Audited in [TIME]
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project[extra2] depends on sortedcontainers==2.3.0 and project[extra1] depends on sortedcontainers==2.2.0, we can conclude that project[extra1] and project[extra2] are incompatible.
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
              { extra = "project3" },
            ],
        ]

        [project.optional-dependencies]
        extra1 = ["sortedcontainers==2.2.0"]
        extra2 = ["sortedcontainers==2.3.0"]
        project3 = ["sortedcontainers==2.4.0"]
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
        revision = 1
        requires-python = ">=3.12"
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
        source = { virtual = "." }

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
        provides-extras = ["extra1", "extra2", "project3"]

        [[package]]
        name = "sortedcontainers"
        version = "2.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/83/c9/466c0f9b42a0563366bb7c39906d9c6673315f81516f55e3a23a99f52234/sortedcontainers-2.2.0.tar.gz", hash = "sha256:331f5b7acb6bdfaf0b0646f5f86c087e414c9ae9d85e2076ad2eacb17ec2f4ff", size = 30402 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0c/75/4f79725a6ad966f1985d96c5aeda0b27d00c23afa14e8566efcdee1380ad/sortedcontainers-2.2.0-py2.py3-none-any.whl", hash = "sha256:f0694fbe8d090fab0fbabbfecad04756fbbb35dc3c0f89e0f6965396fe815d25", size = 29386 },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
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
    Audited in [TIME]
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
    Audited in [TIME]
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
    Audited in [TIME]
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
    Audited in [TIME]
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
    Audited in [TIME]
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
        "#,
    )?;

    // Fails, as expected.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project[extra2] depends on sortedcontainers==2.4.0 and project[extra1] depends on sortedcontainers==2.3.0, we can conclude that project[extra1] and project[extra2] are incompatible.
          And because your project requires project[extra1] and project[extra2], we can conclude that your project's requirements are unsatisfiable.
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
        revision = 1
        requires-python = ">=3.12"
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
        source = { virtual = "." }

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
        provides-extras = ["extra1", "extra2", "project3", "project4"]

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
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
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
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", extra = "extra1" },
            { package = "project", extra = "extra2" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

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
        provides-extras = ["extra1", "extra2"]

        [[package]]
        name = "sortedcontainers"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
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
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);
    // This should error since we're enabling two conflicting extras.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Found conflicting extras `proxy1[extra1]` and `proxy1[extra2]` enabled simultaneously
    "###);

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
        "#,
    )?;
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);
    // This is fine because we are only enabling one
    // extra, and thus, there is no conflict.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.1.0
     + idna==3.6
     + sniffio==1.3.1
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
        "#,
    )?;
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);
    // This is fine because we are only enabling one
    // extra, and thus, there is no conflict.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - anyio==4.1.0
     + anyio==4.2.0
    "###);

    Ok(())
}

/// A regression tests for unconditionally installing an extra that is
/// marked as conflicting. Previously, the `proxy1[extra1]` dependency
/// would be completely ignored here.
#[test]
fn extra_unconditional_non_conflicting() -> Result<()> {
    let context = TestContext::new("3.12");

    let root_pyproject_toml = context.temp_dir.child("pyproject.toml");
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
        extra2 = []
        extra3 = []

        [tool.uv]
        conflicts = [
          [
            { extra = "extra1" },
            { extra = "extra3" },
          ],
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    // This *should* install `anyio==4.1.0`, but when this
    // test was initially written, it didn't. This was because
    // `uv sync` wasn't correctly propagating extras in a way
    // that would satisfy the conflict markers that got added
    // to the `proxy1[extra1]` dependency.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.1.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    Ok(())
}

#[test]
fn extra_unconditional_in_optional() -> Result<()> {
    let context = TestContext::new("3.12");

    let root_pyproject_toml = context.temp_dir.child("pyproject.toml");
    root_pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.10.0"
        dependencies = []

        [tool.uv.workspace]
        members = ["proxy1"]

        [tool.uv.sources]
        proxy1 = { workspace = true }

        [project.optional-dependencies]
        x1 = ["proxy1[nested-x1]"]
        x2 = ["proxy1[nested-x2]"]
        "#,
    )?;

    let proxy1_pyproject_toml = context.temp_dir.child("proxy1").child("pyproject.toml");
    proxy1_pyproject_toml.write_str(
        r#"
        [project]
        name = "proxy1"
        version = "0.1.0"
        requires-python = ">=3.10.0"
        dependencies = []

        [project.optional-dependencies]
        nested-x1 = ["sortedcontainers==2.3.0"]
        nested-x2 = ["sortedcontainers==2.4.0"]

        [tool.uv]
        conflicts = [
          [
            { extra = "nested-x1" },
            { extra = "nested-x2" },
          ],
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // This shouldn't install anything.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited in [TIME]
    "###);

    // This should install `sortedcontainers==2.3.0`.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--extra=x1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sortedcontainers==2.3.0
    "###);

    // This should install `sortedcontainers==2.4.0`.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--extra=x2"), @r###"
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

    // This should error!
    uv_snapshot!(
        context.filters(),
        context.sync().arg("--frozen").arg("--extra=x1").arg("--extra=x2"),
        @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Found conflicting extras `proxy1[nested-x1]` and `proxy1[nested-x2]` enabled simultaneously
    "###);

    Ok(())
}

#[test]
fn extra_unconditional_non_local_conflict() -> Result<()> {
    let context = TestContext::new("3.12");

    let root_pyproject_toml = context.temp_dir.child("pyproject.toml");
    root_pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.10.0"
        dependencies = ["a", "b"]

        [tool.uv.workspace]
        members = ["a", "b", "c"]

        [tool.uv.sources]
        a = { workspace = true }
        b = { workspace = true }
        c = { workspace = true }
        "#,
    )?;

    let a_pyproject_toml = context.temp_dir.child("a").child("pyproject.toml");
    a_pyproject_toml.write_str(
        r#"
        [project]
        name = "a"
        version = "0.1.0"
        requires-python = ">=3.10.0"
        dependencies = ["c[x1]"]

        [tool.uv.sources]
        c = { workspace = true }
        "#,
    )?;

    let b_pyproject_toml = context.temp_dir.child("b").child("pyproject.toml");
    b_pyproject_toml.write_str(
        r#"
        [project]
        name = "b"
        version = "0.1.0"
        requires-python = ">=3.10.0"
        dependencies = ["c[x2]"]

        [tool.uv.sources]
        c = { workspace = true }
        "#,
    )?;

    let c_pyproject_toml = context.temp_dir.child("c").child("pyproject.toml");
    c_pyproject_toml.write_str(
        r#"
        [project]
        name = "c"
        version = "0.1.0"
        requires-python = ">=3.10.0"
        dependencies = []

        [project.optional-dependencies]
        x1 = ["sortedcontainers==2.3.0"]
        x2 = ["sortedcontainers==2.4.0"]

        [tool.uv]
        conflicts = [
          [
            { extra = "x1" },
            { extra = "x2" },
          ],
        ]
        "#,
    )?;

    // Regrettably, this produces a lock file, and it is one
    // that can never be installed! Namely, because two different
    // conflicting extras are enabled unconditionally in all
    // configurations.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    // This should fail. If it doesn't and we generated a lock
    // file above, then this will likely result in the installation
    // of two different versions of the same package.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Found conflicting extras `c[x1]` and `c[x2]` enabled simultaneously
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

          Because proxy1[extra1]==0.1.0 depends on anyio==4.1.0 and proxy1[extra2]==0.1.0 depends on anyio==4.2.0, we can conclude that proxy1[extra1]==0.1.0 and proxy1[extra2]==0.1.0 are incompatible.
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
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", group = "group1" },
            { package = "project", group = "group2" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

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
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
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
    Audited in [TIME]
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
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", group = "group1" },
            { package = "project", group = "group2" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

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
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
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
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sortedcontainers==2.3.0
    "###);

    // Another install, but with one of the groups enabled.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group=group1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project:group1 depends on sortedcontainers==2.3.0 and project[extra1] depends on sortedcontainers==2.4.0, we can conclude that project:group1 and project[extra1] are incompatible.
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
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", group = "group1" },
            { package = "project", extra = "extra1" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

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
        provides-extras = ["extra1"]

        [package.metadata.requires-dev]
        group1 = [{ name = "sortedcontainers", specifier = "==2.3.0" }]

        [[package]]
        name = "sortedcontainers"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/14/10/6a9481890bae97da9edd6e737c9c3dec6aea3fc2fa53b0934037b35c89ea/sortedcontainers-2.3.0.tar.gz", hash = "sha256:59cc937650cf60d677c16775597c89a960658a09cf7c1a668f86e1e4464b10a1", size = 30509 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/4d/a7046ae1a1a4cc4e9bbed194c387086f06b25038be596543d026946330c9/sortedcontainers-2.3.0-py2.py3-none-any.whl", hash = "sha256:37257a32add0a3ee490bb170b599e93095eed89a55da91fa9f48753ea12fd73f", size = 29479 },
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
    Audited in [TIME]
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
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-30T00:00Z");

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
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu124"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
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
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", extra = "cu118" },
            { package = "project", extra = "cu124" },
        ]]

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu118" }
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.2-py3-none-any.whl", hash = "sha256:6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu124" }
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
            { name = "jinja2", version = "3.1.2", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu118" } },
        ]
        cu124 = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu124" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "jinja2", marker = "extra == 'cu118'", specifier = "==3.1.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        provides-extras = ["cu118", "cu124"]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
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
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-30T00:00Z");

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
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu124"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
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
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", group = "cu118" },
            { package = "project", group = "cu124" },
        ]]

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu118" }
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.2-py3-none-any.whl", hash = "sha256:6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu124" }
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
            { name = "jinja2", version = "3.1.2", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu118" } },
        ]
        cu124 = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu124" } },
        ]

        [package.metadata]

        [package.metadata.requires-dev]
        cu118 = [{ name = "jinja2", specifier = "==3.1.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu118", conflict = { package = "project", group = "cu118" } }]
        cu124 = [{ name = "jinja2", specifier = "==3.1.3", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu124", conflict = { package = "project", group = "cu124" } }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
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
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-30T00:00Z");

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
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu124"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
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
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", extra = "cu118" },
            { package = "project", extra = "cu124" },
        ]]

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

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
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu118" }
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
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu124" }
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
            { name = "jinja2", version = "3.1.2", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu118" }, extra = ["i18n"] },
        ]
        cu124 = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu124" }, extra = ["i18n"] },
        ]

        [package.metadata]
        requires-dist = [
            { name = "jinja2", extras = ["i18n"], marker = "extra == 'cu118'", specifier = "==3.1.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", extras = ["i18n"], marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        provides-extras = ["cu118", "cu124"]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
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
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-30T00:00Z");

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
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu124"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
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
        revision = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "extra != 'extra-7-project-cu118' and extra == 'extra-7-project-cu124'",
            "sys_platform == 'darwin' and extra == 'extra-7-project-cu118' and extra != 'extra-7-project-cu124'",
            "sys_platform != 'darwin' and extra == 'extra-7-project-cu118' and extra != 'extra-7-project-cu124'",
            "extra != 'extra-7-project-cu118' and extra != 'extra-7-project-cu124'",
        ]
        conflicts = [[
            { package = "project", extra = "cu118" },
            { package = "project", extra = "cu124" },
        ]]

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu118" }
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
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu124" }
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
            { name = "jinja2", version = "3.1.2", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu118" }, marker = "sys_platform == 'darwin'" },
            { name = "jinja2", version = "3.1.2", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform != 'darwin'" },
        ]
        cu124 = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu124" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "jinja2", marker = "sys_platform == 'darwin' and extra == 'cu118'", specifier = "==3.1.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", marker = "sys_platform != 'darwin' and extra == 'cu118'", specifier = "==3.1.2" },
            { name = "jinja2", marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        provides-extras = ["cu118", "cu124"]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
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

/// Like `non_optional_dependency_extra`, but for extras and groups mixed
/// together.
///
/// This test never regressed, but we added it here to ensure it doesn't.
#[test]
fn non_optional_dependency_mixed() -> Result<()> {
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

        [dependency-groups]
        x2 = ["idna==3.6"]

        [tool.uv]
        conflicts = [
          [
            {package = "project", extra = "x1"},
            {package = "project", group = "x2"},
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
        revision = 1
        requires-python = ">=3.12"
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
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-7-project-foo'" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-7-project-bar' or extra != 'extra-7-project-foo'" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
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
        provides-extras = ["foo", "bar", "baz"]

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

/// This is like `shared_optional_dependency_extra1`, but for groups.
///
/// Ref <https://github.com/astral-sh/uv/issues/9289>
#[test]
fn shared_optional_dependency_group1() -> Result<()> {
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
            { group = "foo" },
            { group = "bar" },
          ],
        ]
        "#,
    )?;

    // This shouldn't install two versions of `idna`, only one, `idna==3.5`.
    uv_snapshot!(context.filters(), context.sync().arg("--group=baz").arg("--group=foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
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
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", group = "foo" },
            { package = "project", group = "bar" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'group-7-project-foo'" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'group-7-project-bar' or extra != 'group-7-project-foo'" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
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
        source = { virtual = "." }

        [package.dev-dependencies]
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

        [package.metadata.requires-dev]
        bar = [{ name = "idna", specifier = "==3.6" }]
        baz = [{ name = "anyio" }]
        foo = [{ name = "idna", specifier = "==3.5" }]

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

/// This is like `shared_optional_dependency_extra1`, but for extras/groups.
///
/// Ref <https://github.com/astral-sh/uv/issues/9289>
#[test]
fn shared_optional_dependency_mixed1() -> Result<()> {
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

        [dependency-groups]
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
            { group = "bar" },
          ],
        ]
        "#,
    )?;

    // This shouldn't install two versions of `idna`, only one, `idna==3.5`.
    uv_snapshot!(context.filters(), context.sync().arg("--group=baz").arg("--extra=foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
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
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", extra = "foo" },
            { package = "project", group = "bar" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-7-project-foo'" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'group-7-project-bar' or extra != 'extra-7-project-foo'" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
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
        source = { virtual = "." }

        [package.optional-dependencies]
        foo = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.dev-dependencies]
        bar = [
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
        ]
        baz = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "idna", marker = "extra == 'foo'", specifier = "==3.5" }]
        provides-extras = ["foo"]

        [package.metadata.requires-dev]
        bar = [{ name = "idna", specifier = "==3.6" }]
        baz = [{ name = "anyio" }]

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
    let context = TestContext::new("3.11");

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
        revision = 1
        requires-python = "==3.11.*"
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
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-7-project-foo'" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-7-project-bar'" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
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
        provides-extras = ["foo", "bar"]

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

/// Like `shared_optional_dependency_extra2`, but for groups.
///
/// Regression test for: <https://github.com/astral-sh/uv/issues/9289>
/// Regression test for: <https://github.com/astral-sh/uv/issues/9622>
/// Regression test for: <https://github.com/astral-sh/uv/issues/9640>
#[test]
fn shared_optional_dependency_group2() -> Result<()> {
    let context = TestContext::new("3.11");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11,<3.12"
        dependencies = []

        [dependency-groups]
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
            { group = "foo" },
            { group = "bar" },
          ],
        ]
        "#,
    )?;

    // This shouldn't install two versions of `idna`, only one, `idna==3.5`.
    uv_snapshot!(context.filters(), context.sync().arg("--group=bar"), @r###"
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
        revision = 1
        requires-python = "==3.11.*"
        conflicts = [[
            { package = "project", group = "foo" },
            { package = "project", group = "bar" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'group-7-project-foo'" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'group-7-project-bar'" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
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
        source = { virtual = "." }

        [package.dev-dependencies]
        bar = [
            { name = "anyio" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
        ]
        foo = [
            { name = "anyio" },
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]

        [package.metadata.requires-dev]
        bar = [
            { name = "anyio" },
            { name = "idna", specifier = "==3.6" },
        ]
        foo = [
            { name = "anyio" },
            { name = "idna", specifier = "==3.5" },
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

/// Like `shared_optional_dependency_extra2`, but for extras/groups.
///
/// Regression test for: <https://github.com/astral-sh/uv/issues/9289>
/// Regression test for: <https://github.com/astral-sh/uv/issues/9622>
/// Regression test for: <https://github.com/astral-sh/uv/issues/9640>
#[test]
fn shared_optional_dependency_mixed2() -> Result<()> {
    let context = TestContext::new("3.11");

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

        [dependency-groups]
        bar = [
          "idna==3.6",
          "anyio",
        ]

        [tool.uv]
        conflicts = [
          [
            { extra = "foo" },
            { group = "bar" },
          ],
        ]
        "#,
    )?;

    // This shouldn't install two versions of `idna`, only one, `idna==3.5`.
    uv_snapshot!(context.filters(), context.sync().arg("--group=bar"), @r###"
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
        revision = 1
        requires-python = "==3.11.*"
        conflicts = [[
            { package = "project", extra = "foo" },
            { package = "project", group = "bar" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-7-project-foo'" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'group-7-project-bar'" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
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
        source = { virtual = "." }

        [package.optional-dependencies]
        foo = [
            { name = "anyio" },
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.dev-dependencies]
        bar = [
            { name = "anyio" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", marker = "extra == 'foo'" },
            { name = "idna", marker = "extra == 'foo'", specifier = "==3.5" },
        ]
        provides-extras = ["foo"]

        [package.metadata.requires-dev]
        bar = [
            { name = "anyio" },
            { name = "idna", specifier = "==3.6" },
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
        revision = 1
        requires-python = ">=3.12"
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
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-7-project-foo'" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-7-project-bar' or extra != 'extra-7-project-foo'" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
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
        provides-extras = ["foo", "bar"]

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
    Installed 1 package in [TIME]
     - idna==3.6
     + idna==3.5
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--extra=bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - idna==3.5
     + idna==3.6
    "###);

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Audited 3 packages in [TIME]
    "###);

    Ok(())
}

/// Like `shared_dependency_extra`, but for groups.
///
/// Regression test for: <https://github.com/astral-sh/uv/issues/9289>
#[test]
fn shared_dependency_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [dependency-groups]
        foo = [
          "idna==3.5",
        ]
        bar = [
          "idna==3.6",
        ]

        [tool.uv]
        conflicts = [
          [
            { group = "foo" },
            { group = "bar" },
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
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", group = "foo" },
            { package = "project", group = "bar" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'group-7-project-foo'" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'group-7-project-bar' or extra != 'group-7-project-foo'" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
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
        source = { virtual = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.dev-dependencies]
        bar = [
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
        ]
        foo = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio" }]

        [package.metadata.requires-dev]
        bar = [{ name = "idna", specifier = "==3.6" }]
        foo = [{ name = "idna", specifier = "==3.5" }]

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
    uv_snapshot!(context.filters(), context.sync().arg("--group=foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - idna==3.6
     + idna==3.5
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--group=bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - idna==3.5
     + idna==3.6
    "###);

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Audited 3 packages in [TIME]
    "###);

    Ok(())
}

/// Like `shared_dependency_extra`, but for extras/groups.
///
/// Regression test for: <https://github.com/astral-sh/uv/issues/9289>
#[test]
fn shared_dependency_mixed() -> Result<()> {
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

        [dependency-groups]
        bar = [
          "idna==3.6",
        ]

        [tool.uv]
        conflicts = [
          [
            { extra = "foo" },
            { group = "bar" },
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
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", extra = "foo" },
            { package = "project", group = "bar" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-7-project-foo'" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'group-7-project-bar' or extra != 'extra-7-project-foo'" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
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
        source = { virtual = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.optional-dependencies]
        foo = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.dev-dependencies]
        bar = [
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio" },
            { name = "idna", marker = "extra == 'foo'", specifier = "==3.5" },
        ]
        provides-extras = ["foo"]

        [package.metadata.requires-dev]
        bar = [{ name = "idna", specifier = "==3.6" }]

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
    Installed 1 package in [TIME]
     - idna==3.6
     + idna==3.5
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--group=bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - idna==3.5
     + idna==3.6
    "###);

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Audited 3 packages in [TIME]
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
    let context = TestContext::new("3.11");

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

    // Error out, as x2 extra is only on the child.
    uv_snapshot!(context.filters(), context.sync().arg("--extra=x2"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    error: Extra `x2` is not defined in the project's `optional-dependencies` table
    "###);

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
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
        revision = 1
        requires-python = "==3.11.*"
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
            { name = "idna", version = "3.4", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-6-proxy1-x2' or (extra == 'extra-6-proxy1-x3' and extra == 'extra-7-project-x1')" },
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-6-proxy1-x3' or (extra == 'extra-6-proxy1-x2' and extra == 'extra-7-project-x1')" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-7-project-x1' or (extra == 'extra-6-proxy1-x2' and extra == 'extra-6-proxy1-x3') or (extra != 'extra-6-proxy1-x2' and extra != 'extra-6-proxy1-x3')" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/8b/e1/43beb3d38dba6cb420cefa297822eac205a277ab43e5ba5d5c46faf96438/idna-3.4.tar.gz", hash = "sha256:814f528e8dead7d329833b91c5faa87d60bf71824cd12a7530b5526063d02cb4", size = 183077 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fc/34/3030de6f1370931b9dbb4dad48f6ab1015ab1d32447850b9fc94e60097be/idna-3.4-py3-none-any.whl", hash = "sha256:90b77e79eaa3eba6de819a0c442c0b4ceefc341a7a2ab77d7562bf49f425c5c2", size = 61538 },
        ]

        [[package]]
        name = "idna"
        version = "3.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
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
        provides-extras = ["x1"]

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
        provides-extras = ["x2", "x3"]

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
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-30T00:00Z");

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
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu124"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
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
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", extra = "cu118" },
            { package = "project", extra = "cu124" },
        ]]

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

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
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu118" }
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
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu124" }
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
            { name = "jinja2", version = "3.1.2", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu118" }, extra = ["i18n"] },
        ]
        cu124 = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu124" }, extra = ["i18n"] },
        ]

        [package.metadata]
        requires-dist = [
            { name = "jinja2", extras = ["i18n"], marker = "extra == 'cu118'", specifier = "==3.1.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", extras = ["i18n"], marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        provides-extras = ["cu118", "cu124"]
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
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-30T00:00Z");

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
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu124"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
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
        revision = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "extra != 'extra-7-project-cu118' and extra == 'extra-7-project-cu124'",
            "sys_platform == 'darwin' and extra == 'extra-7-project-cu118' and extra != 'extra-7-project-cu124'",
            "sys_platform != 'darwin' and extra == 'extra-7-project-cu118' and extra != 'extra-7-project-cu124'",
            "extra != 'extra-7-project-cu118' and extra != 'extra-7-project-cu124'",
        ]
        conflicts = [[
            { package = "project", extra = "cu118" },
            { package = "project", extra = "cu124" },
        ]]

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu118" }
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
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu124" }
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
            { name = "jinja2", version = "3.1.2", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu118" }, marker = "sys_platform == 'darwin'" },
            { name = "jinja2", version = "3.1.2", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform != 'darwin'" },
        ]
        cu124 = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu124" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "jinja2", marker = "sys_platform == 'darwin' and extra == 'cu118'", specifier = "==3.1.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", marker = "sys_platform != 'darwin' and extra == 'cu118'", specifier = "==3.1.2" },
            { name = "jinja2", marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        provides-extras = ["cu118", "cu124"]
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
        revision = 1
        requires-python = ">=3.12"
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
        sdist = { url = "https://files.pythonhosted.org/packages/9b/c4/db3e4b22ebc18ee797dae8e14b5db68e5826ae6337334c276f1cb4ff84fb/idna-3.5.tar.gz", hash = "sha256:27009fe2735bf8723353582d48575b23c533cc2c2de7b5a68908d91b5eb18d08", size = 64640 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ea/65/9c7a31be86861d43da3d4f8661f677b38120320540773a04979ad6fa9ecd/idna-3.5-py3-none-any.whl", hash = "sha256:79b8f0ac92d2351be5f6122356c9a592c96d81c9a79e4b488bf2a6a15f88057a", size = 61566 },
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
        provides-extras = ["foo", "bar", "extra-3-pkg-foo"]

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

/// This tests that uv's graph traversal to determine which extras are always
/// enabled is working properly. In particular, this catches a regression
/// Konsti found on the initial work adding conflict markers where not all
/// extras/groups would be propagated through the graph traversal.
///
/// In this case, there are a number of conflict markers that are removed
/// entirely from the lock file of this particular case.
///
/// Ref: <https://github.com/astral-sh/uv/pull/9370#discussion_r1875958904>
#[test]
fn extra_inferences() -> Result<()> {
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
        dependencies = ["quickpath-airflow-operator==1.0.2"]

        [project.optional-dependencies]
        x1 = ["apache-airflow==2.5.0"]
        x2 = ["apache-airflow==2.6.0"]

        [tool.uv]
        conflicts = [
          [
            { extra = "x1" },
            { extra = "x2" },
          ],
        ]

        # These are to permit skipping some builds
        # that speed up this test.
        [[tool.uv.dependency-metadata]]
        name = "python-nvd3"
        version = "0.16.0"
        requires-dist = ["python-slugify>=1.2.5", "Jinja2>=2.8"]

        [[tool.uv.dependency-metadata]]
        name = "unicodecsv"
        version = "0.14.1"
        requires-dist = []
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 137 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock,
            @r###"
        version = 1
        revision = 1
        requires-python = ">=3.12"
        conflicts = [[
            { package = "pkg", extra = "x1" },
            { package = "pkg", extra = "x2" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]

        [[manifest.dependency-metadata]]
        name = "python-nvd3"
        version = "0.16.0"
        requires-dist = ["python-slugify>=1.2.5", "jinja2>=2.8"]

        [[manifest.dependency-metadata]]
        name = "unicodecsv"
        version = "0.14.1"

        [[package]]
        name = "a2wsgi"
        version = "1.10.4"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/94/1c/07d91da0c8618ecc146a811ab01985ca95ad07221483625dc00a024ea5cb/a2wsgi-1.10.4.tar.gz", hash = "sha256:50e81ac55aa609fa2c666e42bacc25c424c8884ce6072f1a7e902114b7ee5d63", size = 18186 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c4/a6/73b02f52206f7bc3600a702726bd75b0cda229a23c4a7ea6189bbd9ae528/a2wsgi-1.10.4-py3-none-any.whl", hash = "sha256:f17da93bf5952e0b0938c87f261c52b7305ddfab1ff3c70dd10b4b76db3851d3", size = 16812 },
        ]

        [[package]]
        name = "aiohttp"
        version = "3.9.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "aiosignal" },
            { name = "attrs" },
            { name = "frozenlist" },
            { name = "multidict" },
            { name = "yarl" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/18/93/1f005bbe044471a0444a82cdd7356f5120b9cf94fe2c50c0cdbf28f1258b/aiohttp-3.9.3.tar.gz", hash = "sha256:90842933e5d1ff760fae6caca4b2b3edba53ba8f4b71e95dacf2818a2aca06f7", size = 7499669 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/02/fe/b15ae84c4641ff829154d7a6646c4ba4612208ab28229c90bf0844e59e18/aiohttp-3.9.3-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:38a19bc3b686ad55804ae931012f78f7a534cce165d089a2059f658f6c91fa60", size = 592525 },
            { url = "https://files.pythonhosted.org/packages/5f/75/b3f077038cb3a8d83cd4d128e23d432bd40b6efd79e6f4361551f3c92e5e/aiohttp-3.9.3-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:770d015888c2a598b377bd2f663adfd947d78c0124cfe7b959e1ef39f5b13869", size = 393715 },
            { url = "https://files.pythonhosted.org/packages/98/e4/6e56f3d2a9404192ed46ad8edf7c676aafeb8f342ca134d69fed920a59f3/aiohttp-3.9.3-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:ee43080e75fc92bf36219926c8e6de497f9b247301bbf88c5c7593d931426679", size = 389731 },
            { url = "https://files.pythonhosted.org/packages/e9/18/64c65a8ead659bae24a47a8197195be4340f26260e4363bd4924346b9343/aiohttp-3.9.3-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:52df73f14ed99cee84865b95a3d9e044f226320a87af208f068ecc33e0c35b96", size = 1319968 },
            { url = "https://files.pythonhosted.org/packages/6f/82/58ceac3a641202957466a532e9f92f439c6a71b74a4ffcc1919e270703d2/aiohttp-3.9.3-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:dc9b311743a78043b26ffaeeb9715dc360335e5517832f5a8e339f8a43581e4d", size = 1360141 },
            { url = "https://files.pythonhosted.org/packages/ef/d1/6aea10c955896329402950407823625ab3a549b99e9c1e97fc61e5622b8a/aiohttp-3.9.3-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:b955ed993491f1a5da7f92e98d5dad3c1e14dc175f74517c4e610b1f2456fb11", size = 1401903 },
            { url = "https://files.pythonhosted.org/packages/fd/4f/5c6041fca616a1cafa4914f630d6898085afe4683be5387a4054da55f52a/aiohttp-3.9.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:504b6981675ace64c28bf4a05a508af5cde526e36492c98916127f5a02354d53", size = 1315339 },
            { url = "https://files.pythonhosted.org/packages/e2/11/4bd14dee3b507dbe20413e972c10accb79de8390ddac5154ef076c1ca31a/aiohttp-3.9.3-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:a6fe5571784af92b6bc2fda8d1925cccdf24642d49546d3144948a6a1ed58ca5", size = 1267309 },
            { url = "https://files.pythonhosted.org/packages/0e/91/fdd26fc726d7ece6bf735a8613893e14dea5de8cc90757de4a412fe89355/aiohttp-3.9.3-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:ba39e9c8627edc56544c8628cc180d88605df3892beeb2b94c9bc857774848ca", size = 1321904 },
            { url = "https://files.pythonhosted.org/packages/03/20/0a43a00edd6a401369ceb38bfe07a67823337dd26102e760d3230e0dedcf/aiohttp-3.9.3-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:e5e46b578c0e9db71d04c4b506a2121c0cb371dd89af17a0586ff6769d4c58c1", size = 1268236 },
            { url = "https://files.pythonhosted.org/packages/72/09/1f36849c36b7929dd09e013c637808fcaf908a0aa543388c2903dbb68bba/aiohttp-3.9.3-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:938a9653e1e0c592053f815f7028e41a3062e902095e5a7dc84617c87267ebd5", size = 1348120 },
            { url = "https://files.pythonhosted.org/packages/64/df/5cddb631867dbc85c058efcb16cbccb72f8bf66c0f6dca38dee346f4699a/aiohttp-3.9.3-cp312-cp312-musllinux_1_1_s390x.whl", hash = "sha256:c3452ea726c76e92f3b9fae4b34a151981a9ec0a4847a627c43d71a15ac32aa6", size = 1395925 },
            { url = "https://files.pythonhosted.org/packages/78/4c/579dcd801e1d98a8cb9144005452c65bcdaf5cce0aff1d6363385a8062b3/aiohttp-3.9.3-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:ff30218887e62209942f91ac1be902cc80cddb86bf00fbc6783b7a43b2bea26f", size = 1316774 },
            { url = "https://files.pythonhosted.org/packages/2d/8c/8e0f346927177d2c25570bbea9975d3a99556753ee53ab55386149bbb1e3/aiohttp-3.9.3-cp312-cp312-win32.whl", hash = "sha256:38f307b41e0bea3294a9a2a87833191e4bcf89bb0365e83a8be3a58b31fb7f38", size = 342182 },
            { url = "https://files.pythonhosted.org/packages/c2/bf/db1fc240d89cde43fd7bb11c1c3f9156dd184881a527ad8b0f9e8f4d434a/aiohttp-3.9.3-cp312-cp312-win_amd64.whl", hash = "sha256:b791a3143681a520c0a17e26ae7465f1b6f99461a28019d1a2f425236e6eedb5", size = 363433 },
        ]

        [[package]]
        name = "aiosignal"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "frozenlist" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ae/67/0952ed97a9793b4958e5736f6d2b346b414a2cd63e82d05940032f45b32f/aiosignal-1.3.1.tar.gz", hash = "sha256:54cd96e15e1649b75d6c87526a6ff0b6c1b0dd3459f43d9ca11d48c339b68cfc", size = 19422 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/76/ac/a7305707cb852b7e16ff80eaf5692309bde30e2b1100a1fcacdc8f731d97/aiosignal-1.3.1-py3-none-any.whl", hash = "sha256:f8376fb07dd1e86a584e4fcdec80b36b7f81aac666ebc724e2c090300dd83b17", size = 7617 },
        ]

        [[package]]
        name = "alembic"
        version = "1.13.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "mako" },
            { name = "sqlalchemy" },
            { name = "typing-extensions" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/7b/24/ddce068e2ac9b5581bd58602edb2a1be1b0752e1ff2963c696ecdbe0470d/alembic-1.13.1.tar.gz", hash = "sha256:4932c8558bf68f2ee92b9bbcb8218671c627064d5b08939437af6d77dc05e595", size = 1213288 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7f/50/9fb3a5c80df6eb6516693270621676980acd6d5a9a7efdbfa273f8d616c7/alembic-1.13.1-py3-none-any.whl", hash = "sha256:2edcc97bed0bd3272611ce3a98d98279e9c209e7186e43e75bbb1b2bdfdbcc43", size = 233424 },
        ]

        [[package]]
        name = "annotated-types"
        version = "0.6.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/67/fe/8c7b275824c6d2cd17c93ee85d0ee81c090285b6d52f4876ccc47cf9c3c4/annotated_types-0.6.0.tar.gz", hash = "sha256:563339e807e53ffd9c267e99fc6d9ea23eb8443c08f112651963e24e22f84a5d", size = 14670 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/28/78/d31230046e58c207284c6b2c4e8d96e6d3cb4e52354721b944d3e1ee4aa5/annotated_types-0.6.0-py3-none-any.whl", hash = "sha256:0641064de18ba7a25dee8f96403ebc39113d0cb953a01429249d5c7564666a43", size = 12360 },
        ]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "apache-airflow"
        version = "2.5.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "alembic", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "apache-airflow-providers-common-sql", version = "1.8.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x1'" },
            { name = "apache-airflow-providers-ftp", version = "3.6.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x1'" },
            { name = "apache-airflow-providers-http", version = "4.7.0", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x1'" },
            { name = "apache-airflow-providers-imap", version = "3.4.0", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x1'" },
            { name = "apache-airflow-providers-sqlite", version = "3.5.0", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x1'" },
            { name = "argcomplete", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "attrs", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "blinker", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "cattrs", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "colorlog", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "configupdater", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "connexion", extra = ["flask", "swagger-ui"], marker = "extra == 'extra-3-pkg-x1'" },
            { name = "cron-descriptor", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "croniter", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "cryptography", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "deprecated", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "dill", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "flask", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "flask-appbuilder", version = "4.1.4", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x1'" },
            { name = "flask-caching", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "flask-login", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "flask-session", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "flask-wtf", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "graphviz", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "gunicorn", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "httpx", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "itsdangerous", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "jinja2", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "jsonschema", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "lazy-object-proxy", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "linkify-it-py", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "lockfile", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "markdown", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "markdown-it-py", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "markupsafe", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "marshmallow-oneofschema", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "mdit-py-plugins", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "packaging", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "pathspec", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "pendulum", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "pluggy", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "psutil", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "pygments", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "pyjwt", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "python-daemon", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "python-dateutil", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "python-nvd3", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "python-slugify", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "rich", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "setproctitle", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "sqlalchemy", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "sqlalchemy-jsonfield", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "tabulate", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "tenacity", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "termcolor", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "typing-extensions", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "unicodecsv", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "werkzeug", marker = "extra == 'extra-3-pkg-x1'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/eb/7d/fef0976adf269614870aa0aea79782601040f2b46267bbf03cf5314f2a67/apache-airflow-2.5.0.tar.gz", hash = "sha256:cd6c6c2d7dc2a0af9d509442d2ea4aaa150d55a8d950769f1635c497c396a3c9", size = 6080151 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/94/3a/3d050e3decaf7c1064e6e0e5e06f5f9be6c7476b438f8d25e458421e2e13/apache_airflow-2.5.0-py3-none-any.whl", hash = "sha256:fb2c2768e3a233e60b110431b907045256a224cffaf01fa475ccef3db8d6edd3", size = 6627918 },
        ]

        [[package]]
        name = "apache-airflow"
        version = "2.6.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "alembic" },
            { name = "apache-airflow-providers-common-sql", version = "1.11.1", source = { registry = "https://pypi.org/simple" } },
            { name = "apache-airflow-providers-ftp", version = "3.7.0", source = { registry = "https://pypi.org/simple" } },
            { name = "apache-airflow-providers-http", version = "4.10.0", source = { registry = "https://pypi.org/simple" } },
            { name = "apache-airflow-providers-imap", version = "3.5.0", source = { registry = "https://pypi.org/simple" } },
            { name = "apache-airflow-providers-sqlite", version = "3.7.1", source = { registry = "https://pypi.org/simple" } },
            { name = "argcomplete" },
            { name = "asgiref" },
            { name = "attrs" },
            { name = "blinker" },
            { name = "cattrs" },
            { name = "colorlog" },
            { name = "configupdater" },
            { name = "connexion", extra = ["flask"] },
            { name = "cron-descriptor" },
            { name = "croniter" },
            { name = "cryptography" },
            { name = "deprecated" },
            { name = "dill" },
            { name = "flask" },
            { name = "flask-appbuilder", version = "4.3.0", source = { registry = "https://pypi.org/simple" } },
            { name = "flask-caching" },
            { name = "flask-login" },
            { name = "flask-session" },
            { name = "flask-wtf" },
            { name = "graphviz" },
            { name = "gunicorn" },
            { name = "httpx" },
            { name = "itsdangerous" },
            { name = "jinja2" },
            { name = "jsonschema" },
            { name = "lazy-object-proxy" },
            { name = "linkify-it-py" },
            { name = "lockfile" },
            { name = "markdown" },
            { name = "markdown-it-py" },
            { name = "markupsafe" },
            { name = "marshmallow-oneofschema" },
            { name = "mdit-py-plugins" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "pendulum" },
            { name = "pluggy" },
            { name = "psutil" },
            { name = "pydantic" },
            { name = "pygments" },
            { name = "pyjwt" },
            { name = "python-daemon" },
            { name = "python-dateutil" },
            { name = "python-nvd3" },
            { name = "python-slugify" },
            { name = "rfc3339-validator" },
            { name = "rich" },
            { name = "rich-argparse" },
            { name = "setproctitle" },
            { name = "sqlalchemy" },
            { name = "sqlalchemy-jsonfield" },
            { name = "tabulate" },
            { name = "tenacity" },
            { name = "termcolor" },
            { name = "typing-extensions" },
            { name = "unicodecsv" },
            { name = "werkzeug" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/2c/db/d59ec4b76f00f3521d5a719b771971003ca5ba69e5dcbb11b1d84dcaa049/apache-airflow-2.6.0.tar.gz", hash = "sha256:78cc7957546fd74eee09234ad39fe7a6c913a114d66077fc85410a2cedec51ea", size = 11468599 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/1b/c3/4d4a4050ddc8f9be1ce54c87ec0655cae462eab767474380c9bd19654372/apache_airflow-2.6.0-py3-none-any.whl", hash = "sha256:a7375b1b738f129462e70eb5a1bb95f604862fcb6437047ee159e5b885739402", size = 12085556 },
        ]

        [[package]]
        name = "apache-airflow-providers-common-sql"
        version = "1.8.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "apache-airflow", version = "2.5.0", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x1'" },
            { name = "sqlparse", marker = "extra == 'extra-3-pkg-x1'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/97/4c/7360977c53952ed7b5ab4e13f1c433c6bbf00ab13e3221dc72e57ef668b5/apache_airflow_providers_common_sql-1.8.1.tar.gz", hash = "sha256:1cd2fdfc7ce7a8a7475943672bdf1cdf424a0a355e47d0a2fb56046fec473050", size = 29503 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d9/57/2475919a62d6c4715b3e1f381a8e239dc592b8e1ce4ca7038218a52a5413/apache_airflow_providers_common_sql-1.8.1-py3-none-any.whl", hash = "sha256:16a76a8fad3c9ecd51ac0d386790be4ceacfd7c5cd7768e3f51ea90c89f9d29a", size = 39574 },
        ]

        [[package]]
        name = "apache-airflow-providers-common-sql"
        version = "1.11.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "apache-airflow", version = "2.6.0", source = { registry = "https://pypi.org/simple" } },
            { name = "more-itertools" },
            { name = "sqlparse" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/44/4e/c5d0d8aaa794fe2e1ac13925b3424c69cb14f0096ba8096e70f47a8ae9d3/apache_airflow_providers_common_sql-1.11.1.tar.gz", hash = "sha256:5b8c4d267677758dcb28b1a8976f10f30e50a4b568abda93a667ad6ff938060b", size = 33439 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/91/60/a5db60c6272f390375e10d5cb38c2f053049a64fce43d396efee8328525a/apache_airflow_providers_common_sql-1.11.1-py3-none-any.whl", hash = "sha256:eb8531b9eedcfdcdd972f12a0bffadc289ade9c3587a5bb5bd146f3240d5eb02", size = 45257 },
        ]

        [[package]]
        name = "apache-airflow-providers-ftp"
        version = "3.6.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "apache-airflow", version = "2.5.0", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x1'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/5f/db/8ba81e6f25b726ee3bc971bcd20c344d8ce0a4dea3d0524096115daf92e5/apache-airflow-providers-ftp-3.6.1.tar.gz", hash = "sha256:a3c8659d1455f6e8a0a3f6a960b15cb6d69e7572f8826037a4002c397d26ac12", size = 16778 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/db/ae/8b2dd4de000b8483766838e06c2ee0005c10177620e0e97b5b3e9061b5ef/apache_airflow_providers_ftp-3.6.1-py3-none-any.whl", hash = "sha256:17442f0dfb0fec6995b7193c31cff3eb0cdc8f99004158980fd447bd39ddb331", size = 18901 },
        ]

        [[package]]
        name = "apache-airflow-providers-ftp"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "apache-airflow", version = "2.6.0", source = { registry = "https://pypi.org/simple" } },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/04/be/15ed8d2752ceb9c1af2598e701cf0f1a3574a6740d547edadd3e5f8ad1a0/apache_airflow_providers_ftp-3.7.0.tar.gz", hash = "sha256:cc6e7602fb9b92255b83343673923339c8fe7d22c8c738ba372ce38a70cce1cc", size = 13020 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/93/09/bdaf595d71e0f2292346a55199ae522bd39eddd32b45b2ad875528e80e9e/apache_airflow_providers_ftp-3.7.0-py3-none-any.whl", hash = "sha256:817936fa2f9f5d7f00f50bfe0dfdf83a3ffbcd863cb92b862a2ab5a40da1b8c4", size = 19219 },
        ]

        [[package]]
        name = "apache-airflow-providers-http"
        version = "4.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "aiohttp", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "apache-airflow", version = "2.5.0", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x1'" },
            { name = "asgiref", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "requests", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "requests-toolbelt", marker = "extra == 'extra-3-pkg-x1'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/3d/e7/b6ed57dedf075d8bb9dc82382b364a5055083f2758df3f0e5f25c4ae3085/apache-airflow-providers-http-4.7.0.tar.gz", hash = "sha256:ec2fbaeb997ebbc597e086f5cb6e7c92d7955854a1b54f4fc6549b7efb9b7daa", size = 20792 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/81/12/14de3bc493a512a916c15950cf0502a6243c4d4c1dd4c6d4fb905bdfb58a/apache_airflow_providers_http-4.7.0-py3-none-any.whl", hash = "sha256:839a456d705a35f2801f687b3daf0545632bde2e44142c44bc76ce36fd0b5678", size = 25521 },
        ]

        [[package]]
        name = "apache-airflow-providers-http"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "aiohttp" },
            { name = "apache-airflow", version = "2.6.0", source = { registry = "https://pypi.org/simple" } },
            { name = "asgiref" },
            { name = "requests" },
            { name = "requests-toolbelt" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/bc/f9/c0cf92fbcb28fbc925607ac9cfacc2018de27cba2dcc1fc24f557c127cc6/apache_airflow_providers_http-4.10.0.tar.gz", hash = "sha256:f8280633053f25d7d11ef33f26aa7225fb081400a0dd97c6af9e0a9f59b26566", size = 18195 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/51/53/3238ae94666f90ca2ca4c40edd803cd3fd45cabb8f05ace73656003a748d/apache_airflow_providers_http-4.10.0-py3-none-any.whl", hash = "sha256:3c9af9a64954973a3bc31d33adf145b9b4273e5194603bee86c4fad6c51d872f", size = 27231 },
        ]

        [[package]]
        name = "apache-airflow-providers-imap"
        version = "3.4.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "apache-airflow", version = "2.5.0", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x1'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/f6/d1/524c13c4c98040d2139305530a0c4acb773dc37bb578c41ec135ca438bae/apache-airflow-providers-imap-3.4.0.tar.gz", hash = "sha256:347f78efa0e1353f90be78244fad0136d7c0cb1af25b9a3ba296d4e31fb53f6c", size = 15806 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/81/f1/fd276f800b5790ec046b0fdeadc758055e4ba58474e6f1c9877ddeb08f75/apache_airflow_providers_imap-3.4.0-py3-none-any.whl", hash = "sha256:4972793b0dbb25d5372fe1bf174329958075985c0beca013459ec89fcbbca620", size = 17047 },
        ]

        [[package]]
        name = "apache-airflow-providers-imap"
        version = "3.5.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "apache-airflow", version = "2.6.0", source = { registry = "https://pypi.org/simple" } },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9f/cf/c138f68f83b874824f4c225a2985acf8b5e2f1d6f521d2b94ac615774eee/apache_airflow_providers_imap-3.5.0.tar.gz", hash = "sha256:d50929801cb9b7abcea33799e8b3cd64b756bd42530a44281febaffcdb30998f", size = 12231 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/40/b3/7100ae60ab07679aa7a7f471f4747e2360f3d36c1e4b1fd79a81352d14cb/apache_airflow_providers_imap-3.5.0-py3-none-any.whl", hash = "sha256:f5f26c662cff27a532c6a7c3ed0a8bc9c7fcae14634424e82e85150e631a2adb", size = 17289 },
        ]

        [[package]]
        name = "apache-airflow-providers-sqlite"
        version = "3.5.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "apache-airflow", version = "2.5.0", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x1'" },
            { name = "apache-airflow-providers-common-sql", version = "1.8.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x1'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/62/4a/ba727087ef486e16e028ba169c198007a1dcbde569179bfbec6b57602054/apache-airflow-providers-sqlite-3.5.0.tar.gz", hash = "sha256:6f47c25d7fb026fa8b8b4dc8edfee1491998255048255625716fceaae2c67024", size = 12946 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e1/30/95f0c4a6b3ba7f1a2a6a9f0f3f6e0cb0de812851ffd1765981eec3d305b6/apache_airflow_providers_sqlite-3.5.0-py3-none-any.whl", hash = "sha256:7baba67c9ddf75b31d8197e22ca491c2e1cf9e4c6306c1788c150b5457c3a182", size = 13687 },
        ]

        [[package]]
        name = "apache-airflow-providers-sqlite"
        version = "3.7.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "apache-airflow", version = "2.6.0", source = { registry = "https://pypi.org/simple" } },
            { name = "apache-airflow-providers-common-sql", version = "1.11.1", source = { registry = "https://pypi.org/simple" } },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/77/32/f8c8edc5e071d2d9d92d3711cea0089a3fd2e22d619deb5174a55228bc3e/apache_airflow_providers_sqlite-3.7.1.tar.gz", hash = "sha256:0dd8967ac1f3fb518cef278611a2f8bbae19421e215e0bf4ede01542d4af86cf", size = 9454 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/5c/19/dc768e8294358794350c605ec307c1a6f14e12ef037b52c132b24d415eca/apache_airflow_providers_sqlite-3.7.1-py3-none-any.whl", hash = "sha256:49af9ce1012e2d52ef89dc5d5832ff9bc245330635aa4f011590dc19113a380f", size = 13977 },
        ]

        [[package]]
        name = "apispec"
        version = "3.3.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/1f/41/55fcfb7f5e8ac178a32875f55dfd0f5568c3cd1022724d34d0434e32b47b/apispec-3.3.2.tar.gz", hash = "sha256:d23ebd5b71e541e031b02a19db10b5e6d5ef8452c552833e3e1afc836b40b1ad", size = 67794 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/35/a2/80a82b22296c942a5298bb760e8e21c86ace3342de840ff3df8938af4272/apispec-3.3.2-py2.py3-none-any.whl", hash = "sha256:a1df9ec6b2cd0edf45039ef025abd7f0660808fa2edf737d3ba1cf5ef1a4625b", size = 27146 },
        ]

        [package.optional-dependencies]
        yaml = [
            { name = "pyyaml", marker = "extra == 'extra-3-pkg-x1'" },
        ]

        [[package]]
        name = "apispec"
        version = "5.2.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/39/bb/2910f46ecba16334c19e4f02906b1fdb0e69f9c3fd9a21afcf86c45ba89e/apispec-5.2.2.tar.gz", hash = "sha256:6ea6542e1ebffe9fd95ba01ef3f51351eac6c200a974562c7473059b9cd20aa7", size = 75729 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7b/bf/8ab9b532c9a22e9cc4920ed7436fde5f207807346564b95d9782f1e2aa5e/apispec-5.2.2-py3-none-any.whl", hash = "sha256:f5f0d6b452c3e4a0e0922dce8815fac89dc4dbc758acef21fb9e01584d6602a5", size = 29618 },
        ]

        [package.optional-dependencies]
        yaml = [
            { name = "pyyaml" },
        ]

        [[package]]
        name = "argcomplete"
        version = "3.2.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/3c/c0/031c507227ce3b715274c1cd1f3f9baf7a0f7cec075e22c7c8b5d4e468a9/argcomplete-3.2.3.tar.gz", hash = "sha256:bf7900329262e481be5a15f56f19736b376df6f82ed27576fa893652c5de6c23", size = 81130 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/88/8c/61021c45428ad2ef6131c6068d14f7f0968767e972e427cd87bd25c9ea7b/argcomplete-3.2.3-py3-none-any.whl", hash = "sha256:c12355e0494c76a2a7b73e3a59b09024ca0ba1e279fb9ed6c1b82d5b74b6a70c", size = 42580 },
        ]

        [[package]]
        name = "asgiref"
        version = "3.8.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/29/38/b3395cc9ad1b56d2ddac9970bc8f4141312dbaec28bc7c218b0dfafd0f42/asgiref-3.8.1.tar.gz", hash = "sha256:c343bd80a0bec947a9860adb4c432ffa7db769836c64238fc34bdc3fec84d590", size = 35186 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/39/e3/893e8757be2612e6c266d9bb58ad2e3651524b5b40cf56761e985a28b13e/asgiref-3.8.1-py3-none-any.whl", hash = "sha256:3e1e3ecc849832fe52ccf2cb6686b7a55f82bb1d6aee72a58826471390335e47", size = 23828 },
        ]

        [[package]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[package]]
        name = "babel"
        version = "2.14.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e2/80/cfbe44a9085d112e983282ee7ca4c00429bc4d1ce86ee5f4e60259ddff7f/Babel-2.14.0.tar.gz", hash = "sha256:6919867db036398ba21eb5c7a0f6b28ab8cbc3ae7a73a44ebe34ae74a4e7d363", size = 10795622 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0d/35/4196b21041e29a42dc4f05866d0c94fa26c9da88ce12c38c2265e42c82fb/Babel-2.14.0-py3-none-any.whl", hash = "sha256:efb1a25b7118e67ce3a259bed20545c29cb68be8ad2c784c83689981b7a57287", size = 11034798 },
        ]

        [[package]]
        name = "blinker"
        version = "1.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a1/13/6df5fc090ff4e5d246baf1f45fe9e5623aa8565757dfa5bd243f6a545f9e/blinker-1.7.0.tar.gz", hash = "sha256:e6820ff6fa4e4d1d8e2747c2283749c3f547e4fee112b98555cdcdae32996182", size = 28134 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fa/2a/7f3714cbc6356a0efec525ce7a0613d581072ed6eb53eb7b9754f33db807/blinker-1.7.0-py3-none-any.whl", hash = "sha256:c3f865d4d54db7abc53758a01601cf343fe55b84c1de4e3fa910e420b438d5b9", size = 13068 },
        ]

        [[package]]
        name = "cachelib"
        version = "0.9.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/79/f2/1c76df4d295789bbc836eea50b813d64f86e640c29fe8f0a3686e9c4e3e9/cachelib-0.9.0.tar.gz", hash = "sha256:38222cc7c1b79a23606de5c2607f4925779e37cdcea1c2ad21b8bae94b5425a5", size = 21007 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/93/70/58e525451478055b0fd2859b22226888a6985d404fe65e014fc4893d3b75/cachelib-0.9.0-py3-none-any.whl", hash = "sha256:811ceeb1209d2fe51cd2b62810bd1eccf70feba5c52641532498be5c675493b3", size = 15716 },
        ]

        [[package]]
        name = "cattrs"
        version = "23.2.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "attrs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/1e/57/c6ccd22658c4bcb3beb3f1c262e1f170cf136e913b122763d0ddd328d284/cattrs-23.2.3.tar.gz", hash = "sha256:a934090d95abaa9e911dac357e3a8699e0b4b14f8529bcc7d2b1ad9d51672b9f", size = 610215 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b3/0d/cd4a4071c7f38385dc5ba91286723b4d1090b87815db48216212c6c6c30e/cattrs-23.2.3-py3-none-any.whl", hash = "sha256:0341994d94971052e9ee70662542699a3162ea1e0c62f7ce1b4a57f563685108", size = 57474 },
        ]

        [[package]]
        name = "certifi"
        version = "2024.2.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/71/da/e94e26401b62acd6d91df2b52954aceb7f561743aa5ccc32152886c76c96/certifi-2024.2.2.tar.gz", hash = "sha256:0569859f95fc761b18b45ef421b1290a0f65f147e92a1e5eb3e635f9a5e4e66f", size = 164886 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ba/06/a07f096c664aeb9f01624f858c3add0a4e913d6c96257acb4fce61e7de14/certifi-2024.2.2-py3-none-any.whl", hash = "sha256:dc383c07b76109f368f6106eee2b593b04a011ea4d55f652c6ca24a754d1cdd1", size = 163774 },
        ]

        [[package]]
        name = "cffi"
        version = "1.16.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "pycparser" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/68/ce/95b0bae7968c65473e1298efb042e10cafc7bafc14d9e4f154008241c91d/cffi-1.16.0.tar.gz", hash = "sha256:bcb3ef43e58665bbda2fb198698fcae6776483e0c4a631aa5647806c25e02cc0", size = 512873 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/22/04/1d10d5baf3faaae9b35f6c49bcf25c1be81ea68cc7ee6923206d02be85b0/cffi-1.16.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:fa3a0128b152627161ce47201262d3140edb5a5c3da88d73a1b790a959126956", size = 183322 },
            { url = "https://files.pythonhosted.org/packages/b4/f6/b28d2bfb5fca9e8f9afc9d05eae245bed9f6ba5c2897fefee7a9abeaf091/cffi-1.16.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:68e7c44931cc171c54ccb702482e9fc723192e88d25a0e133edd7aff8fcd1f6e", size = 177173 },
            { url = "https://files.pythonhosted.org/packages/9b/1a/575200306a3dfd9102ce573e7158d459a1bd7e44637e4f22a999c4fd64b1/cffi-1.16.0-cp312-cp312-manylinux_2_12_i686.manylinux2010_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:abd808f9c129ba2beda4cfc53bde801e5bcf9d6e0f22f095e45327c038bfe68e", size = 453846 },
            { url = "https://files.pythonhosted.org/packages/e4/c7/c09cc6fd1828ea950e60d44e0ef5ed0b7e3396fbfb856e49ca7d629b1408/cffi-1.16.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:88e2b3c14bdb32e440be531ade29d3c50a1a59cd4e51b1dd8b0865c54ea5d2e2", size = 477041 },
            { url = "https://files.pythonhosted.org/packages/b4/5f/c6e7e8d80fbf727909e4b1b5b9352082fc1604a14991b1d536bfaee5a36c/cffi-1.16.0-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:fcc8eb6d5902bb1cf6dc4f187ee3ea80a1eba0a89aba40a5cb20a5087d961357", size = 483787 },
            { url = "https://files.pythonhosted.org/packages/a3/81/5f5d61338951afa82ce4f0f777518708893b9420a8b309cc037fbf114e63/cffi-1.16.0-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:b7be2d771cdba2942e13215c4e340bfd76398e9227ad10402a8767ab1865d2e6", size = 469137 },
            { url = "https://files.pythonhosted.org/packages/09/d4/8759cc3b2222c159add8ce3af0089912203a31610f4be4c36f98e320b4c6/cffi-1.16.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:e715596e683d2ce000574bae5d07bd522c781a822866c20495e52520564f0969", size = 477578 },
            { url = "https://files.pythonhosted.org/packages/4c/00/e17e2a8df0ff5aca2edd9eeebd93e095dd2515f2dd8d591d84a3233518f6/cffi-1.16.0-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:2d92b25dbf6cae33f65005baf472d2c245c050b1ce709cc4588cdcdd5495b520", size = 487099 },
            { url = "https://files.pythonhosted.org/packages/c9/6e/751437067affe7ac0944b1ad4856ec11650da77f0dd8f305fae1117ef7bb/cffi-1.16.0-cp312-cp312-win32.whl", hash = "sha256:b2ca4e77f9f47c55c194982e10f058db063937845bb2b7a86c84a6cfe0aefa8b", size = 173564 },
            { url = "https://files.pythonhosted.org/packages/e9/63/e285470a4880a4f36edabe4810057bd4b562c6ddcc165eacf9c3c7210b40/cffi-1.16.0-cp312-cp312-win_amd64.whl", hash = "sha256:68678abf380b42ce21a5f2abde8efee05c114c2fdb2e9eef2efdb0257fba1235", size = 181956 },
        ]

        [[package]]
        name = "charset-normalizer"
        version = "3.3.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/63/09/c1bc53dab74b1816a00d8d030de5bf98f724c52c1635e07681d312f20be8/charset-normalizer-3.3.2.tar.gz", hash = "sha256:f30c3cb33b24454a82faecaf01b19c18562b1e89558fb6c56de4d9118a032fd5", size = 104809 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/b2/fcedc8255ec42afee97f9e6f0145c734bbe104aac28300214593eb326f1d/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:0b2b64d2bb6d3fb9112bafa732def486049e63de9618b5843bcdd081d8144cd8", size = 192892 },
            { url = "https://files.pythonhosted.org/packages/2e/7d/2259318c202f3d17f3fe6438149b3b9e706d1070fe3fcbb28049730bb25c/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:ddbb2551d7e0102e7252db79ba445cdab71b26640817ab1e3e3648dad515003b", size = 122213 },
            { url = "https://files.pythonhosted.org/packages/3a/52/9f9d17c3b54dc238de384c4cb5a2ef0e27985b42a0e5cc8e8a31d918d48d/charset_normalizer-3.3.2-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:55086ee1064215781fff39a1af09518bc9255b50d6333f2e4c74ca09fac6a8f6", size = 119404 },
            { url = "https://files.pythonhosted.org/packages/99/b0/9c365f6d79a9f0f3c379ddb40a256a67aa69c59609608fe7feb6235896e1/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:8f4a014bc36d3c57402e2977dada34f9c12300af536839dc38c0beab8878f38a", size = 137275 },
            { url = "https://files.pythonhosted.org/packages/91/33/749df346e93d7a30cdcb90cbfdd41a06026317bfbfb62cd68307c1a3c543/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:a10af20b82360ab00827f916a6058451b723b4e65030c5a18577c8b2de5b3389", size = 147518 },
            { url = "https://files.pythonhosted.org/packages/72/1a/641d5c9f59e6af4c7b53da463d07600a695b9824e20849cb6eea8a627761/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8d756e44e94489e49571086ef83b2bb8ce311e730092d2c34ca8f7d925cb20aa", size = 140182 },
            { url = "https://files.pythonhosted.org/packages/ee/fb/14d30eb4956408ee3ae09ad34299131fb383c47df355ddb428a7331cfa1e/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:90d558489962fd4918143277a773316e56c72da56ec7aa3dc3dbbe20fdfed15b", size = 141869 },
            { url = "https://files.pythonhosted.org/packages/df/3e/a06b18788ca2eb6695c9b22325b6fde7dde0f1d1838b1792a0076f58fe9d/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:6ac7ffc7ad6d040517be39eb591cac5ff87416c2537df6ba3cba3bae290c0fed", size = 144042 },
            { url = "https://files.pythonhosted.org/packages/45/59/3d27019d3b447a88fe7e7d004a1e04be220227760264cc41b405e863891b/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:7ed9e526742851e8d5cc9e6cf41427dfc6068d4f5a3bb03659444b4cabf6bc26", size = 138275 },
            { url = "https://files.pythonhosted.org/packages/7b/ef/5eb105530b4da8ae37d506ccfa25057961b7b63d581def6f99165ea89c7e/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:8bdb58ff7ba23002a4c5808d608e4e6c687175724f54a5dade5fa8c67b604e4d", size = 144819 },
            { url = "https://files.pythonhosted.org/packages/a2/51/e5023f937d7f307c948ed3e5c29c4b7a3e42ed2ee0b8cdf8f3a706089bf0/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:6b3251890fff30ee142c44144871185dbe13b11bab478a88887a639655be1068", size = 149415 },
            { url = "https://files.pythonhosted.org/packages/24/9d/2e3ef673dfd5be0154b20363c5cdcc5606f35666544381bee15af3778239/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_s390x.whl", hash = "sha256:b4a23f61ce87adf89be746c8a8974fe1c823c891d8f86eb218bb957c924bb143", size = 141212 },
            { url = "https://files.pythonhosted.org/packages/5b/ae/ce2c12fcac59cb3860b2e2d76dc405253a4475436b1861d95fe75bdea520/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:efcb3f6676480691518c177e3b465bcddf57cea040302f9f4e6e191af91174d4", size = 142167 },
            { url = "https://files.pythonhosted.org/packages/ed/3a/a448bf035dce5da359daf9ae8a16b8a39623cc395a2ffb1620aa1bce62b0/charset_normalizer-3.3.2-cp312-cp312-win32.whl", hash = "sha256:d965bba47ddeec8cd560687584e88cf699fd28f192ceb452d1d7ee807c5597b7", size = 93041 },
            { url = "https://files.pythonhosted.org/packages/b6/7c/8debebb4f90174074b827c63242c23851bdf00a532489fba57fef3416e40/charset_normalizer-3.3.2-cp312-cp312-win_amd64.whl", hash = "sha256:96b02a3dc4381e5494fad39be677abcb5e6634bf7b4fa83a6dd3112607547001", size = 100397 },
            { url = "https://files.pythonhosted.org/packages/28/76/e6222113b83e3622caa4bb41032d0b1bf785250607392e1b778aca0b8a7d/charset_normalizer-3.3.2-py3-none-any.whl", hash = "sha256:3e4d1f6587322d2788836a99c69062fbb091331ec940e02d12d179c1d53e25fc", size = 48543 },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "colorama", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941 },
        ]

        [[package]]
        name = "colorama"
        version = "0.4.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 },
        ]

        [[package]]
        name = "colorlog"
        version = "4.8.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "colorama", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/75/32/cdfba08674d72fe7895a8ec7be8f171e8502274999cae9497e4545404873/colorlog-4.8.0.tar.gz", hash = "sha256:59b53160c60902c405cdec28d38356e09d40686659048893e026ecbd589516b1", size = 28770 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/51/62/61449c6bb74c2a3953c415b2cdb488e4f0518ac67b35e2b03a6d543035ca/colorlog-4.8.0-py2.py3-none-any.whl", hash = "sha256:3dd15cb27e8119a24c1a7b5c93f9f3b455855e0f73993b1c25921b2f646f1dcd", size = 10023 },
        ]

        [[package]]
        name = "configupdater"
        version = "3.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/2b/f4/603bd8a65e040b23d25b5843836297b0f4e430f509d8ed2ef8f072fb4127/ConfigUpdater-3.2.tar.gz", hash = "sha256:9fdac53831c1b062929bf398b649b87ca30e7f1a735f3fbf482072804106306b", size = 140603 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e7/f0/b59cb7613d9d0f866b6ff247c5953ad78363c27ff5d684a2a98899ab8220/ConfigUpdater-3.2-py2.py3-none-any.whl", hash = "sha256:0f65a041627d7693840b4dd743581db4c441c97195298a29d075f91b79539df2", size = 34688 },
        ]

        [[package]]
        name = "connexion"
        version = "3.0.6"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "asgiref" },
            { name = "httpx" },
            { name = "inflection" },
            { name = "jinja2" },
            { name = "jsonschema" },
            { name = "python-multipart" },
            { name = "pyyaml" },
            { name = "requests" },
            { name = "starlette" },
            { name = "typing-extensions" },
            { name = "werkzeug" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/cb/7e/0b62de62ed9824621f69342cf08aadd1f1ba42d2eca0673079fa231634e1/connexion-3.0.6.tar.gz", hash = "sha256:6dba2e3d3920653a16d41b373ee8b104281d078c2a3916b773b575c8e919eb1b", size = 87548 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/4a/36/96cf48c7d5735ca425876151e1f30bc6742fa411099c42ece5e42ea207e3/connexion-3.0.6-py3-none-any.whl", hash = "sha256:9172600de1fd315ca368d6005f6cd56672559f4c7a3b5b239f0b3d473a10758f", size = 112415 },
        ]

        [package.optional-dependencies]
        flask = [
            { name = "a2wsgi" },
            { name = "flask", extra = ["async"] },
        ]
        swagger-ui = [
            { name = "swagger-ui-bundle", marker = "extra == 'extra-3-pkg-x1'" },
        ]

        [[package]]
        name = "cron-descriptor"
        version = "1.4.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/28/be/bb2c93b445fdd341ec057e0920c5b2b35c910db6f0d76dca8ea123504107/cron_descriptor-1.4.3.tar.gz", hash = "sha256:7b1a00d7d25d6ae6896c0da4457e790b98cba778398a3d48e341e5e0d33f0488", size = 30289 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/18/136b0073305038d1317f3442f614a698ce686830a2810bbe7e875311e09f/cron_descriptor-1.4.3-py3-none-any.whl", hash = "sha256:a67ba21804983b1427ed7f3e1ec27ee77bf24c652b0430239c268c5ddfbf9dc0", size = 49799 },
        ]

        [[package]]
        name = "croniter"
        version = "2.0.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "python-dateutil" },
            { name = "pytz" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/7f/ef/ee405df01052969bc7b243b9ffdcebb7adf32e10d6181db4add197948c7a/croniter-2.0.3.tar.gz", hash = "sha256:28763ad39c404e159140874f08010cfd8a18f4c2a7cea1ce73e9506a4380cfc1", size = 43274 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/75/b4/a0b97aa934f623fe2342a3c681c70b46f398e91788498802d6147f8f3ff1/croniter-2.0.3-py2.py3-none-any.whl", hash = "sha256:84dc95b2eb6760144cc01eca65a6b9cc1619c93b2dc37d8a27f4319b3eb740de", size = 20009 },
        ]

        [[package]]
        name = "cryptography"
        version = "42.0.5"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "cffi", marker = "platform_python_implementation != 'PyPy'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/13/9e/a55763a32d340d7b06d045753c186b690e7d88780cafce5f88cb931536be/cryptography-42.0.5.tar.gz", hash = "sha256:6fe07eec95dfd477eb9530aef5bead34fec819b3aaf6c5bd6d20565da607bfe1", size = 671025 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/f1/fd98e6e79242d9aeaf6a5d49639a7e85f05741575af14d3f4a1d477f572e/cryptography-42.0.5-cp37-abi3-macosx_10_12_universal2.whl", hash = "sha256:a30596bae9403a342c978fb47d9b0ee277699fa53bbafad14706af51fe543d16", size = 5883181 },
            { url = "https://files.pythonhosted.org/packages/d9/f9/27dda069a9f9bfda7c75305e222d904cc2445acf5eab5c696ade57d36f1b/cryptography-42.0.5-cp37-abi3-macosx_10_12_x86_64.whl", hash = "sha256:b7ffe927ee6531c78f81aa17e684e2ff617daeba7f189f911065b2ea2d526dec", size = 3106715 },
            { url = "https://files.pythonhosted.org/packages/e2/59/61b2364f2a4d3668d933531bc30d012b9b2de1e534df4805678471287d57/cryptography-42.0.5-cp37-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:2424ff4c4ac7f6b8177b53c17ed5d8fa74ae5955656867f5a8affaca36a27abb", size = 4376731 },
            { url = "https://files.pythonhosted.org/packages/fb/0b/14509319a1b49858425553d2fb3808579cfdfe98c1d71a3f046c1b4e0108/cryptography-42.0.5-cp37-abi3-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:329906dcc7b20ff3cad13c069a78124ed8247adcac44b10bea1130e36caae0b4", size = 4568288 },
            { url = "https://files.pythonhosted.org/packages/8c/50/9185cca136596448d9cc595ae22a9bd4412ad35d812550c37c1390d54673/cryptography-42.0.5-cp37-abi3-manylinux_2_28_aarch64.whl", hash = "sha256:b03c2ae5d2f0fc05f9a2c0c997e1bc18c8229f392234e8a0194f202169ccd278", size = 4362222 },
            { url = "https://files.pythonhosted.org/packages/64/f7/d3c83c79947cc6807e6acd3b2d9a1cbd312042777bc7eec50c869913df79/cryptography-42.0.5-cp37-abi3-manylinux_2_28_x86_64.whl", hash = "sha256:f8837fe1d6ac4a8052a9a8ddab256bc006242696f03368a4009be7ee3075cdb7", size = 4578380 },
            { url = "https://files.pythonhosted.org/packages/e5/61/67e090a41c70ee526bd5121b1ccabab85c727574332d03326baaedea962d/cryptography-42.0.5-cp37-abi3-musllinux_1_1_aarch64.whl", hash = "sha256:0270572b8bd2c833c3981724b8ee9747b3ec96f699a9665470018594301439ee", size = 4475683 },
            { url = "https://files.pythonhosted.org/packages/5b/3d/c3c21e3afaf43bacccc3ebf61d1a0d47cef6e2607dbba01662f6f9d8fc40/cryptography-42.0.5-cp37-abi3-musllinux_1_1_x86_64.whl", hash = "sha256:b8cac287fafc4ad485b8a9b67d0ee80c66bf3574f655d3b97ef2e1082360faf1", size = 4651973 },
            { url = "https://files.pythonhosted.org/packages/d8/b1/127ecb373d02db85a7a7de5093d7ac7b7714b8907d631f0591e8f002998d/cryptography-42.0.5-cp37-abi3-musllinux_1_2_aarch64.whl", hash = "sha256:16a48c23a62a2f4a285699dba2e4ff2d1cff3115b9df052cdd976a18856d8e3d", size = 4448866 },
            { url = "https://files.pythonhosted.org/packages/2c/9c/821ef6144daf80360cf6093520bf07eec7c793103ed4b1bf3fa17d2b55d8/cryptography-42.0.5-cp37-abi3-musllinux_1_2_x86_64.whl", hash = "sha256:2bce03af1ce5a5567ab89bd90d11e7bbdff56b8af3acbbec1faded8f44cb06da", size = 4652546 },
            { url = "https://files.pythonhosted.org/packages/86/7f/1c6bb9ef3c4e5e2a438ab2b7ac85af52a9aa9a9a9a326b89e1b25659b598/cryptography-42.0.5-cp37-abi3-win32.whl", hash = "sha256:b6cd2203306b63e41acdf39aa93b86fb566049aeb6dc489b70e34bcd07adca74", size = 2431140 },
            { url = "https://files.pythonhosted.org/packages/36/33/ed48350d38a6a151dd3cf1850a5966b86c5752212ddaaceb44e65bf412e5/cryptography-42.0.5-cp37-abi3-win_amd64.whl", hash = "sha256:98d8dc6d012b82287f2c3d26ce1d2dd130ec200c8679b6213b3c73c08b2b7940", size = 2890092 },
            { url = "https://files.pythonhosted.org/packages/6d/4d/f7c14c7a49e35df829e04d451a57b843208be7442c8e087250c195775be1/cryptography-42.0.5-cp39-abi3-macosx_10_12_universal2.whl", hash = "sha256:5e6275c09d2badf57aea3afa80d975444f4be8d3bc58f7f80d2a484c6f9485c8", size = 5881247 },
            { url = "https://files.pythonhosted.org/packages/50/26/248cd8b6809635ed412159791c0d3869d8ec9dfdc57d428d500a14d425b7/cryptography-42.0.5-cp39-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:e4985a790f921508f36f81831817cbc03b102d643b5fcb81cd33df3fa291a1a1", size = 4376966 },
            { url = "https://files.pythonhosted.org/packages/d4/fa/057f9d7a5364c86ccb6a4bd4e5c58920dcb66532be0cc21da3f9c7617ec3/cryptography-42.0.5-cp39-abi3-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:7cde5f38e614f55e28d831754e8a3bacf9ace5d1566235e39d91b35502d6936e", size = 4567683 },
            { url = "https://files.pythonhosted.org/packages/0e/1d/62a2324882c0db89f64358dadfb95cae024ee3ba9fde3d5fd4d2f58af9f5/cryptography-42.0.5-cp39-abi3-manylinux_2_28_aarch64.whl", hash = "sha256:7367d7b2eca6513681127ebad53b2582911d1736dc2ffc19f2c3ae49997496bc", size = 4363579 },
            { url = "https://files.pythonhosted.org/packages/48/c8/c0962598c43d3cff2c9d6ac66d0c612bdfb1975be8d87b8889960cf8c81d/cryptography-42.0.5-cp39-abi3-manylinux_2_28_x86_64.whl", hash = "sha256:cd2030f6650c089aeb304cf093f3244d34745ce0cfcc39f20c6fbfe030102e2a", size = 4578653 },
            { url = "https://files.pythonhosted.org/packages/69/f6/630eb71f246208103ffee754b8375b6b334eeedb28620b3ae57be815eeeb/cryptography-42.0.5-cp39-abi3-musllinux_1_1_aarch64.whl", hash = "sha256:a2913c5375154b6ef2e91c10b5720ea6e21007412f6437504ffea2109b5a33d7", size = 4476954 },
            { url = "https://files.pythonhosted.org/packages/7d/bc/b6c691c960b5dcd54c5444e73af7f826e62af965ba59b6d7e9928b6489a2/cryptography-42.0.5-cp39-abi3-musllinux_1_1_x86_64.whl", hash = "sha256:c41fb5e6a5fe9ebcd58ca3abfeb51dffb5d83d6775405305bfa8715b76521922", size = 4650638 },
            { url = "https://files.pythonhosted.org/packages/c2/40/c7cb9d6819b90640ffc3c4028b28f46edc525feaeaa0d98ea23e843d446d/cryptography-42.0.5-cp39-abi3-musllinux_1_2_aarch64.whl", hash = "sha256:3eaafe47ec0d0ffcc9349e1708be2aaea4c6dd4978d76bf6eb0cb2c13636c6fc", size = 4450500 },
            { url = "https://files.pythonhosted.org/packages/ca/2e/9f2c49bd6a18d46c05ec098b040e7d4599c61f50ced40a39adfae3f68306/cryptography-42.0.5-cp39-abi3-musllinux_1_2_x86_64.whl", hash = "sha256:1b95b98b0d2af784078fa69f637135e3c317091b615cd0905f8b8a087e86fa30", size = 4651722 },
            { url = "https://files.pythonhosted.org/packages/93/56/2d8d8903513185743bc6f763797fcba1718093190943735aa2ce8f3f0328/cryptography-42.0.5-cp39-abi3-win32.whl", hash = "sha256:1f71c10d1e88467126f0efd484bd44bca5e14c664ec2ede64c32f20875c0d413", size = 2431150 },
            { url = "https://files.pythonhosted.org/packages/e3/14/13acd84f2a8303d9410ba2e24534a9d90c2817583636a91c4f314224768d/cryptography-42.0.5-cp39-abi3-win_amd64.whl", hash = "sha256:a011a644f6d7d03736214d38832e030d8268bcff4a41f728e6030325fea3e400", size = 2891129 },
        ]

        [[package]]
        name = "deprecated"
        version = "1.2.14"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "wrapt" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/92/14/1e41f504a246fc224d2ac264c227975427a85caf37c3979979edb9b1b232/Deprecated-1.2.14.tar.gz", hash = "sha256:e5323eb936458dccc2582dc6f9c322c852a775a27065ff2b0c4970b9d53d01b3", size = 2974416 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/20/8d/778b7d51b981a96554f29136cd59ca7880bf58094338085bcf2a979a0e6a/Deprecated-1.2.14-py2.py3-none-any.whl", hash = "sha256:6fac8b097794a90302bdbb17b9b815e732d3c4720583ff1b198499d78470466c", size = 9561 },
        ]

        [[package]]
        name = "dill"
        version = "0.3.8"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/17/4d/ac7ffa80c69ea1df30a8aa11b3578692a5118e7cd1aa157e3ef73b092d15/dill-0.3.8.tar.gz", hash = "sha256:3ebe3c479ad625c4553aca177444d89b486b1d84982eeacded644afc0cf797ca", size = 184847 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c9/7a/cef76fd8438a42f96db64ddaa85280485a9c395e7df3db8158cfec1eee34/dill-0.3.8-py3-none-any.whl", hash = "sha256:c36ca9ffb54365bdd2f8eb3eff7d2a21237f8452b57ace88b1ac615b7e815bd7", size = 116252 },
        ]

        [[package]]
        name = "dnspython"
        version = "2.6.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/37/7d/c871f55054e403fdfd6b8f65fd6d1c4e147ed100d3e9f9ba1fe695403939/dnspython-2.6.1.tar.gz", hash = "sha256:e8f0f9c23a7b7cb99ded64e6c3a6f3e701d78f50c55e002b839dea7225cff7cc", size = 332727 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/87/a1/8c5287991ddb8d3e4662f71356d9656d91ab3a36618c3dd11b280df0d255/dnspython-2.6.1-py3-none-any.whl", hash = "sha256:5ef3b9680161f6fa89daf8ad451b5f1a33b18ae8a1c6778cdf4b43f08c0a6e50", size = 307696 },
        ]

        [[package]]
        name = "docutils"
        version = "0.20.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/1f/53/a5da4f2c5739cf66290fac1431ee52aff6851c7c8ffd8264f13affd7bcdd/docutils-0.20.1.tar.gz", hash = "sha256:f08a4e276c3a1583a86dce3e34aba3fe04d02bba2dd51ed16106244e8a923e3b", size = 2058365 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/26/87/f238c0670b94533ac0353a4e2a1a771a0cc73277b88bff23d3ae35a256c1/docutils-0.20.1-py3-none-any.whl", hash = "sha256:96f387a2c5562db4476f09f13bbab2192e764cac08ebbf3a34a95d9b1e4a59d6", size = 572666 },
        ]

        [[package]]
        name = "email-validator"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "dnspython" },
            { name = "idna" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8e/db/5849bad11c5e03b41c7331370a2020bc98822dd8253b1861ec70351b8e75/email_validator-1.3.1.tar.gz", hash = "sha256:d178c5c6fa6c6824e9b04f199cf23e79ac15756786573c190d2ad13089411ad2", size = 29344 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ba/ec/adc595d04e795b04bb0028fc6b067713fdb4a7e8cec44b428f7144fc432e/email_validator-1.3.1-py2.py3-none-any.whl", hash = "sha256:49a72f5fa6ed26be1c964f0567d931d10bf3fdeeacdf97bc26ef1cd2a44e0bda", size = 22715 },
        ]

        [[package]]
        name = "flask"
        version = "2.2.5"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "itsdangerous" },
            { name = "jinja2" },
            { name = "werkzeug" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/5f/76/a4d2c4436dda4b0a12c71e075c508ea7988a1066b06a575f6afe4fecc023/Flask-2.2.5.tar.gz", hash = "sha256:edee9b0a7ff26621bd5a8c10ff484ae28737a2410d99b0bb9a6850c7fb977aa0", size = 697814 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9f/1a/8b6d48162861009d1e017a9740431c78d860809773b66cac220a11aa3310/Flask-2.2.5-py3-none-any.whl", hash = "sha256:58107ed83443e86067e41eff4631b058178191a355886f8e479e347fa1285fdf", size = 101817 },
        ]

        [package.optional-dependencies]
        async = [
            { name = "asgiref" },
        ]

        [[package]]
        name = "flask-appbuilder"
        version = "4.1.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "apispec", version = "3.3.2", source = { registry = "https://pypi.org/simple" }, extra = ["yaml"], marker = "extra == 'extra-3-pkg-x1'" },
            { name = "click", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "colorama", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "email-validator", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "flask", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "flask-babel", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "flask-jwt-extended", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "flask-login", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "flask-sqlalchemy", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "flask-wtf", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "jsonschema", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "marshmallow", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "marshmallow-enum", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "marshmallow-sqlalchemy", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "prison", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "pyjwt", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "python-dateutil", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "sqlalchemy", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "sqlalchemy-utils", marker = "extra == 'extra-3-pkg-x1'" },
            { name = "wtforms", marker = "extra == 'extra-3-pkg-x1'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/95/53/f7a191ba8a42cb4f2dd2248658c5adc16ddea5f4b4e1d832c2fb3a51cd18/Flask-AppBuilder-4.1.4.tar.gz", hash = "sha256:601f71348152886ac801835101a6e4427cebf23f82865d9c2d5964ace8a1bfec", size = 7030368 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3b/28/9acebe45a02908193b28668867b8e948e747e37cb95937ee5f0281d71e27/Flask_AppBuilder-4.1.4-py3-none-any.whl", hash = "sha256:682e18fab43ccec8f4aac696f090ae45326b0ee1f3ad9608896111ff8405a7a4", size = 1779178 },
        ]

        [[package]]
        name = "flask-appbuilder"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "apispec", version = "5.2.2", source = { registry = "https://pypi.org/simple" }, extra = ["yaml"] },
            { name = "click" },
            { name = "colorama" },
            { name = "email-validator" },
            { name = "flask" },
            { name = "flask-babel" },
            { name = "flask-jwt-extended" },
            { name = "flask-limiter" },
            { name = "flask-login" },
            { name = "flask-sqlalchemy" },
            { name = "flask-wtf" },
            { name = "jsonschema" },
            { name = "marshmallow" },
            { name = "marshmallow-enum" },
            { name = "marshmallow-sqlalchemy" },
            { name = "prison" },
            { name = "pyjwt" },
            { name = "python-dateutil" },
            { name = "sqlalchemy" },
            { name = "sqlalchemy-utils" },
            { name = "wtforms" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ee/c2/2a082c68ef6045387db1f78eab3b2f919d5cf6b7c42e6c213e9c5d99959c/Flask-AppBuilder-4.3.0.tar.gz", hash = "sha256:3a6e871d71120ffb25e1d52a862c0327019bdf0478dc4e62100e7923bca2c4f5", size = 7099756 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/86/a2/1fe19bc4baff75a3ae014f4367e77bbd08391f5fd3275f598c58b8cb61e1/Flask_AppBuilder-4.3.0-py3-none-any.whl", hash = "sha256:597975dc6ce983a18d3110e0643d0aae38656b5d36ea74be27eb89c2da686ca7", size = 2480081 },
        ]

        [[package]]
        name = "flask-babel"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "babel" },
            { name = "flask" },
            { name = "jinja2" },
            { name = "pytz" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d7/fe/655e6a5a99ceb815fe839f0698956a9d6c7d5bcc06ca1ee7c6eb6dac154b/Flask-Babel-2.0.0.tar.gz", hash = "sha256:f9faf45cdb2e1a32ea2ec14403587d4295108f35017a7821a2b1acb8cfd9257d", size = 19588 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ab/3e/02331179ffab8b79e0383606a028b6a60fb1b4419b84935edd43223406a0/Flask_Babel-2.0.0-py3-none-any.whl", hash = "sha256:e6820a052a8d344e178cdd36dd4bb8aea09b4bda3d5f9fa9f008df2c7f2f5468", size = 9345 },
        ]

        [[package]]
        name = "flask-caching"
        version = "2.1.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "cachelib" },
            { name = "flask" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/eb/70/09b3254474c0a9085f5faa2fd4da9bbd5318b370e30c4489067db1ead842/Flask-Caching-2.1.0.tar.gz", hash = "sha256:b7500c145135836a952e3de3a80881d9654e327a29c852c9265607f5c449235c", size = 67277 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/99/e279e06a1a0147763bda92359b3fa5eab8566e032407e539eb851cdfb4bc/Flask_Caching-2.1.0-py3-none-any.whl", hash = "sha256:f02645a629a8c89800d96dc8f690a574a0d49dcd66c7536badc6d362ba46b716", size = 28768 },
        ]

        [[package]]
        name = "flask-jwt-extended"
        version = "4.6.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "flask" },
            { name = "pyjwt" },
            { name = "werkzeug" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9a/ea/2f97d46df8deb419ff98c8931cd7a3a142e967ac33522a2f2204449eee52/Flask-JWT-Extended-4.6.0.tar.gz", hash = "sha256:9215d05a9413d3855764bcd67035e75819d23af2fafb6b55197eb5a3313fdfb2", size = 34263 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/be/f7/b5415a5ec78666408cd9af9e8163e2953374808d222625cff33f64adfd2a/Flask_JWT_Extended-4.6.0-py2.py3-none-any.whl", hash = "sha256:63a28fc9731bcc6c4b8815b6f954b5904caa534fc2ae9b93b1d3ef12930dca95", size = 22676 },
        ]

        [[package]]
        name = "flask-limiter"
        version = "3.5.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "flask" },
            { name = "limits" },
            { name = "ordered-set" },
            { name = "rich" },
            { name = "typing-extensions" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ad/62/24440d89655fefa01f3dd7614d95222b4c1f65e5d1dbf4744c3609300c4a/Flask-Limiter-3.5.1.tar.gz", hash = "sha256:8117e1040e5d5c31bf667d3b649fcba325f979d814a3d76a3a2331c3eab63c5e", size = 301691 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/99/a1/c10e95494cf00029501dbd40669b367e91ed1c15d68cb0eb57269a22f3a0/Flask_Limiter-3.5.1-py3-none-any.whl", hash = "sha256:d40526719994197da180caa870ba01e722ed6a70a75790021638dbf29aae82ee", size = 28508 },
        ]

        [[package]]
        name = "flask-login"
        version = "0.6.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "flask" },
            { name = "werkzeug" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c3/6e/2f4e13e373bb49e68c02c51ceadd22d172715a06716f9299d9df01b6ddb2/Flask-Login-0.6.3.tar.gz", hash = "sha256:5e23d14a607ef12806c699590b89d0f0e0d67baeec599d75947bf9c147330333", size = 48834 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/59/f5/67e9cc5c2036f58115f9fe0f00d203cf6780c3ff8ae0e705e7a9d9e8ff9e/Flask_Login-0.6.3-py3-none-any.whl", hash = "sha256:849b25b82a436bf830a054e74214074af59097171562ab10bfa999e6b78aae5d", size = 17303 },
        ]

        [[package]]
        name = "flask-session"
        version = "0.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "flask" },
            { name = "msgspec" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/bc/60/23f2a4d50ef625e783130757690ccd28fbd47c9b04fe41583b800f0c5904/flask_session-0.7.0.tar.gz", hash = "sha256:88e63df090e548373ff0f5e7e1a8333b5e8a5ceaebb2506571138c58351b6886", size = 938549 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f5/f0/fec9b98f9222ab1000be1ad8062d756c4fe11a2a69d5599fbe71fd9943f1/flask_session-0.7.0-py3-none-any.whl", hash = "sha256:b1d4a1ce560aa82c06f25b46a105a13e9cd43776987d6ae7d9fea2cbfc7b4bc4", size = 22205 },
        ]

        [[package]]
        name = "flask-sqlalchemy"
        version = "2.5.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "flask" },
            { name = "sqlalchemy" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/35/f0/39dd2d8e7e5223f78a5206d7020dc0e16718a964acfb3564d89e9798ab9b/Flask-SQLAlchemy-2.5.1.tar.gz", hash = "sha256:2bda44b43e7cacb15d4e05ff3cc1f8bc97936cc464623424102bfc2c35e95912", size = 132750 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/26/2c/9088b6bd95bca539230bbe9ad446737ed391aab9a83aff403e18dded3e75/Flask_SQLAlchemy-2.5.1-py2.py3-none-any.whl", hash = "sha256:f12c3d4cc5cc7fdcc148b9527ea05671718c3ea45d50c7e732cceb33f574b390", size = 17716 },
        ]

        [[package]]
        name = "flask-wtf"
        version = "1.2.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "flask" },
            { name = "itsdangerous" },
            { name = "wtforms" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9b/ef/b6ec35e02f479f6e76e02ede14594c9cfa5e6dcbab6ea0e82fa413993a2a/flask_wtf-1.2.1.tar.gz", hash = "sha256:8bb269eb9bb46b87e7c8233d7e7debdf1f8b74bf90cc1789988c29b37a97b695", size = 42498 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/02/2b/0f0cf68a2f052ea3dbb8b6c8c2a7e8aea5e6df7410f5e289437fefbeb461/flask_wtf-1.2.1-py3-none-any.whl", hash = "sha256:fa6793f2fb7e812e0fe9743b282118e581fb1b6c45d414b8af05e659bd653287", size = 12725 },
        ]

        [[package]]
        name = "frozenlist"
        version = "1.4.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/cf/3d/2102257e7acad73efc4a0c306ad3953f68c504c16982bbdfee3ad75d8085/frozenlist-1.4.1.tar.gz", hash = "sha256:c037a86e8513059a2613aaba4d817bb90b9d9b6b69aace3ce9c877e8c8ed402b", size = 37820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b4/db/4cf37556a735bcdb2582f2c3fa286aefde2322f92d3141e087b8aeb27177/frozenlist-1.4.1-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:1979bc0aeb89b33b588c51c54ab0161791149f2461ea7c7c946d95d5f93b56ae", size = 93937 },
            { url = "https://files.pythonhosted.org/packages/46/03/69eb64642ca8c05f30aa5931d6c55e50b43d0cd13256fdd01510a1f85221/frozenlist-1.4.1-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:cc7b01b3754ea68a62bd77ce6020afaffb44a590c2289089289363472d13aedb", size = 53656 },
            { url = "https://files.pythonhosted.org/packages/3f/ab/c543c13824a615955f57e082c8a5ee122d2d5368e80084f2834e6f4feced/frozenlist-1.4.1-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:c9c92be9fd329ac801cc420e08452b70e7aeab94ea4233a4804f0915c14eba9b", size = 51868 },
            { url = "https://files.pythonhosted.org/packages/a9/b8/438cfd92be2a124da8259b13409224d9b19ef8f5a5b2507174fc7e7ea18f/frozenlist-1.4.1-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:5c3894db91f5a489fc8fa6a9991820f368f0b3cbdb9cd8849547ccfab3392d86", size = 280652 },
            { url = "https://files.pythonhosted.org/packages/54/72/716a955521b97a25d48315c6c3653f981041ce7a17ff79f701298195bca3/frozenlist-1.4.1-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:ba60bb19387e13597fb059f32cd4d59445d7b18b69a745b8f8e5db0346f33480", size = 286739 },
            { url = "https://files.pythonhosted.org/packages/65/d8/934c08103637567084568e4d5b4219c1016c60b4d29353b1a5b3587827d6/frozenlist-1.4.1-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8aefbba5f69d42246543407ed2461db31006b0f76c4e32dfd6f42215a2c41d09", size = 289447 },
            { url = "https://files.pythonhosted.org/packages/70/bb/d3b98d83ec6ef88f9bd63d77104a305d68a146fd63a683569ea44c3085f6/frozenlist-1.4.1-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:780d3a35680ced9ce682fbcf4cb9c2bad3136eeff760ab33707b71db84664e3a", size = 265466 },
            { url = "https://files.pythonhosted.org/packages/0b/f2/b8158a0f06faefec33f4dff6345a575c18095a44e52d4f10c678c137d0e0/frozenlist-1.4.1-cp312-cp312-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:9acbb16f06fe7f52f441bb6f413ebae6c37baa6ef9edd49cdd567216da8600cd", size = 281530 },
            { url = "https://files.pythonhosted.org/packages/ea/a2/20882c251e61be653764038ece62029bfb34bd5b842724fff32a5b7a2894/frozenlist-1.4.1-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:23b701e65c7b36e4bf15546a89279bd4d8675faabc287d06bbcfac7d3c33e1e6", size = 281295 },
            { url = "https://files.pythonhosted.org/packages/4c/f9/8894c05dc927af2a09663bdf31914d4fb5501653f240a5bbaf1e88cab1d3/frozenlist-1.4.1-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:3e0153a805a98f5ada7e09826255ba99fb4f7524bb81bf6b47fb702666484ae1", size = 268054 },
            { url = "https://files.pythonhosted.org/packages/37/ff/a613e58452b60166507d731812f3be253eb1229808e59980f0405d1eafbf/frozenlist-1.4.1-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:dd9b1baec094d91bf36ec729445f7769d0d0cf6b64d04d86e45baf89e2b9059b", size = 286904 },
            { url = "https://files.pythonhosted.org/packages/cc/6e/0091d785187f4c2020d5245796d04213f2261ad097e0c1cf35c44317d517/frozenlist-1.4.1-cp312-cp312-musllinux_1_1_s390x.whl", hash = "sha256:1a4471094e146b6790f61b98616ab8e44f72661879cc63fa1049d13ef711e71e", size = 290754 },
            { url = "https://files.pythonhosted.org/packages/a5/c2/e42ad54bae8bcffee22d1e12a8ee6c7717f7d5b5019261a8c861854f4776/frozenlist-1.4.1-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:5667ed53d68d91920defdf4035d1cdaa3c3121dc0b113255124bcfada1cfa1b8", size = 282602 },
            { url = "https://files.pythonhosted.org/packages/b6/61/56bad8cb94f0357c4bc134acc30822e90e203b5cb8ff82179947de90c17f/frozenlist-1.4.1-cp312-cp312-win32.whl", hash = "sha256:beee944ae828747fd7cb216a70f120767fc9f4f00bacae8543c14a6831673f89", size = 44063 },
            { url = "https://files.pythonhosted.org/packages/3e/dc/96647994a013bc72f3d453abab18340b7f5e222b7b7291e3697ca1fcfbd5/frozenlist-1.4.1-cp312-cp312-win_amd64.whl", hash = "sha256:64536573d0a2cb6e625cf309984e2d873979709f2cf22839bf2d61790b448ad5", size = 50452 },
            { url = "https://files.pythonhosted.org/packages/83/10/466fe96dae1bff622021ee687f68e5524d6392b0a2f80d05001cd3a451ba/frozenlist-1.4.1-py3-none-any.whl", hash = "sha256:04ced3e6a46b4cfffe20f9ae482818e34eba9b5fb0ce4056e4cc9b6e212d09b7", size = 11552 },
        ]

        [[package]]
        name = "graphviz"
        version = "0.20.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/fa/83/5a40d19b8347f017e417710907f824915fba411a9befd092e52746b63e9f/graphviz-0.20.3.zip", hash = "sha256:09d6bc81e6a9fa392e7ba52135a9d49f1ed62526f96499325930e87ca1b5925d", size = 256455 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/be/d59db2d1d52697c6adc9eacaf50e8965b6345cc143f671e1ed068818d5cf/graphviz-0.20.3-py3-none-any.whl", hash = "sha256:81f848f2904515d8cd359cc611faba817598d2feaac4027b266aa3eda7b3dde5", size = 47126 },
        ]

        [[package]]
        name = "greenlet"
        version = "3.0.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/17/14/3bddb1298b9a6786539ac609ba4b7c9c0842e12aa73aaa4d8d73ec8f8185/greenlet-3.0.3.tar.gz", hash = "sha256:43374442353259554ce33599da8b692d5aa96f8976d567d4badf263371fbe491", size = 182013 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a2/2f/461615adc53ba81e99471303b15ac6b2a6daa8d2a0f7f77fd15605e16d5b/greenlet-3.0.3-cp312-cp312-macosx_11_0_universal2.whl", hash = "sha256:70fb482fdf2c707765ab5f0b6655e9cfcf3780d8d87355a063547b41177599be", size = 273085 },
            { url = "https://files.pythonhosted.org/packages/e9/55/2c3cfa3cdbb940cf7321fbcf544f0e9c74898eed43bf678abf416812d132/greenlet-3.0.3-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:d4d1ac74f5c0c0524e4a24335350edad7e5f03b9532da7ea4d3c54d527784f2e", size = 660514 },
            { url = "https://files.pythonhosted.org/packages/38/77/efb21ab402651896c74f24a172eb4d7479f9f53898bd5e56b9e20bb24ffd/greenlet-3.0.3-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:149e94a2dd82d19838fe4b2259f1b6b9957d5ba1b25640d2380bea9c5df37676", size = 674295 },
            { url = "https://files.pythonhosted.org/packages/74/3a/92f188ace0190f0066dca3636cf1b09481d0854c46e92ec5e29c7cefe5b1/greenlet-3.0.3-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:15d79dd26056573940fcb8c7413d84118086f2ec1a8acdfa854631084393efcc", size = 669395 },
            { url = "https://files.pythonhosted.org/packages/63/0f/847ed02cdfce10f0e6e3425cd054296bddb11a17ef1b34681fa01a055187/greenlet-3.0.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:881b7db1ebff4ba09aaaeae6aa491daeb226c8150fc20e836ad00041bcb11230", size = 670455 },
            { url = "https://files.pythonhosted.org/packages/bd/37/56b0da468a85e7704f3b2bc045015301bdf4be2184a44868c71f6dca6fe2/greenlet-3.0.3-cp312-cp312-manylinux_2_24_x86_64.manylinux_2_28_x86_64.whl", hash = "sha256:fcd2469d6a2cf298f198f0487e0a5b1a47a42ca0fa4dfd1b6862c999f018ebbf", size = 625692 },
            { url = "https://files.pythonhosted.org/packages/7c/68/b5f4084c0a252d7e9c0d95fc1cfc845d08622037adb74e05be3a49831186/greenlet-3.0.3-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:1f672519db1796ca0d8753f9e78ec02355e862d0998193038c7073045899f305", size = 1152597 },
            { url = "https://files.pythonhosted.org/packages/a4/fa/31e22345518adcd69d1d6ab5087a12c178aa7f3c51103f6d5d702199d243/greenlet-3.0.3-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:2516a9957eed41dd8f1ec0c604f1cdc86758b587d964668b5b196a9db5bfcde6", size = 1181043 },
            { url = "https://files.pythonhosted.org/packages/53/80/3d94d5999b4179d91bcc93745d1b0815b073d61be79dd546b840d17adb18/greenlet-3.0.3-cp312-cp312-win_amd64.whl", hash = "sha256:bba5387a6975598857d86de9eac14210a49d554a77eb8261cc68b7d082f78ce2", size = 293635 },
        ]

        [[package]]
        name = "gunicorn"
        version = "21.2.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "packaging" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/06/89/acd9879fa6a5309b4bf16a5a8855f1e58f26d38e0c18ede9b3a70996b021/gunicorn-21.2.0.tar.gz", hash = "sha256:88ec8bff1d634f98e61b9f65bc4bf3cd918a90806c6f5c48bc5603849ec81033", size = 3632557 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0e/2a/c3a878eccb100ccddf45c50b6b8db8cf3301a6adede6e31d48e8531cab13/gunicorn-21.2.0-py3-none-any.whl", hash = "sha256:3213aa5e8c24949e792bcacfc176fef362e7aac80b76c56f6b5122bf350722f0", size = 80176 },
        ]

        [[package]]
        name = "h11"
        version = "0.14.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/f5/38/3af3d3633a34a3316095b39c8e8fb4853a28a536e55d347bd8d8e9a14b03/h11-0.14.0.tar.gz", hash = "sha256:8f19fbbe99e72420ff35c00b27a34cb9937e902a8b810e2c88300c6f0a3b699d", size = 100418 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/95/04/ff642e65ad6b90db43e668d70ffb6736436c7ce41fcc549f4e9472234127/h11-0.14.0-py3-none-any.whl", hash = "sha256:e3fe4ac4b851c468cc8363d500db52c2ead036020723024a109d37346efaa761", size = 58259 },
        ]

        [[package]]
        name = "httpcore"
        version = "1.0.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "certifi" },
            { name = "h11" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/03/9d/2055e6b65592d3a485a1141761ba7047674bbe085cebac0988b30e93c9e6/httpcore-1.0.4.tar.gz", hash = "sha256:cb2839ccfcba0d2d3c1131d3c3e26dfc327326fbe7a5dc0dbfe9f6c9151bb022", size = 83096 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2c/93/13f25f2f78646bab97aee7680821e30bd85b2ff0fc45d5fdf5393b79716d/httpcore-1.0.4-py3-none-any.whl", hash = "sha256:ac418c1db41bade2ad53ae2f3834a3a0f5ae76b56cf5aa497d2d033384fc7d73", size = 77850 },
        ]

        [[package]]
        name = "httpx"
        version = "0.27.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "anyio" },
            { name = "certifi" },
            { name = "httpcore" },
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/5c/2d/3da5bdf4408b8b2800061c339f240c1802f2e82d55e50bd39c5a881f47f0/httpx-0.27.0.tar.gz", hash = "sha256:a0cb88a46f32dc874e04ee956e4c2764aba2aa228f650b06788ba6bda2962ab5", size = 126413 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/41/7b/ddacf6dcebb42466abd03f368782142baa82e08fc0c1f8eaa05b4bae87d5/httpx-0.27.0-py3-none-any.whl", hash = "sha256:71d5465162c13681bff01ad59b2cc68dd838ea1f10e51574bac27103f00c91a5", size = 75590 },
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
        name = "importlib-resources"
        version = "6.4.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/c8/9d/6ee73859d6be81c6ea7ebac89655e92740296419bd37e5c8abdb5b62fd55/importlib_resources-6.4.0.tar.gz", hash = "sha256:cdb2b453b8046ca4e3798eb1d84f3cce1446a0e8e7b5ef4efb600f19fc398145", size = 42040 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/75/06/4df55e1b7b112d183f65db9503bff189e97179b256e1ea450a3c365241e0/importlib_resources-6.4.0-py3-none-any.whl", hash = "sha256:50d10f043df931902d4194ea07ec57960f66a80449ff867bfe782b4c486ba78c", size = 38168 },
        ]

        [[package]]
        name = "inflection"
        version = "0.5.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e1/7e/691d061b7329bc8d54edbf0ec22fbfb2afe61facb681f9aaa9bff7a27d04/inflection-0.5.1.tar.gz", hash = "sha256:1a29730d366e996aaacffb2f1f1cb9593dc38e2ddd30c91250c6dde09ea9b417", size = 15091 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/59/91/aa6bde563e0085a02a435aa99b49ef75b0a4b062635e606dab23ce18d720/inflection-0.5.1-py2.py3-none-any.whl", hash = "sha256:f38b2b640938a4f35ade69ac3d053042959b62a0f1076a5bbaa1b9526605a8a2", size = 9454 },
        ]

        [[package]]
        name = "itsdangerous"
        version = "2.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7f/a1/d3fb83e7a61fa0c0d3d08ad0a94ddbeff3731c05212617dff3a94e097f08/itsdangerous-2.1.2.tar.gz", hash = "sha256:5dbbc68b317e5e42f327f9021763545dc3fc3bfe22e6deb96aaf1fc38874156a", size = 56143 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/5f/447e04e828f47465eeab35b5d408b7ebaaaee207f48b7136c5a7267a30ae/itsdangerous-2.1.2-py3-none-any.whl", hash = "sha256:2c2349112351b88699d8d4b6b075022c0808887cb7ad10069318a8b0bc88db44", size = 15749 },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b2/5e/3a21abf3cd467d7876045335e681d276ac32492febe6d98ad89562d1a7e1/Jinja2-3.1.3.tar.gz", hash = "sha256:ac8bd6544d4bb2c9792bf3a159e80bba8fda7f07e81bc3aed565432d5925ba90", size = 268261 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/30/6d/6de6be2d02603ab56e72997708809e8a5b0fbfee080735109b40a3564843/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa", size = 133236 },
        ]

        [[package]]
        name = "jsonschema"
        version = "4.21.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "attrs" },
            { name = "jsonschema-specifications" },
            { name = "referencing" },
            { name = "rpds-py" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/c5/3f6165d3df419ea7b0990b3abed4ff348946a826caf0e7c990b65ff7b9be/jsonschema-4.21.1.tar.gz", hash = "sha256:85727c00279f5fa6bedbe6238d2aa6403bedd8b4864ab11207d07df3cc1b2ee5", size = 321491 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/39/9d/b035d024c62c85f2e2d4806a59ca7b8520307f34e0932fbc8cc75fe7b2d9/jsonschema-4.21.1-py3-none-any.whl", hash = "sha256:7996507afae316306f9e2290407761157c6f78002dcf7419acb99822143d1c6f", size = 85527 },
        ]

        [[package]]
        name = "jsonschema-specifications"
        version = "2023.12.[X]"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "referencing" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/f8/b9/cc0cc592e7c195fb8a650c1d5990b10175cf13b4c97465c72ec841de9e4b/jsonschema_specifications-2023.12.[X].tar.gz", hash = "sha256:48a76787b3e70f5ed53f1160d2b81f586e4ca6d1548c5de7085d1682674764cc", size = 13983 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ee/07/44bd408781594c4d0a027666ef27fab1e441b109dc3b76b4f836f8fd04fe/jsonschema_specifications-2023.12.[X]-py3-none-any.whl", hash = "sha256:87e4fdf3a94858b8a2ba2778d9ba57d8a9cafca7c7489c46ba0d30a8bc6a9c3c", size = 18482 },
        ]

        [[package]]
        name = "lazy-object-proxy"
        version = "1.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/2c/f0/f02e2d150d581a294efded4020094a371bbab42423fe78625ac18854d89b/lazy-object-proxy-1.10.0.tar.gz", hash = "sha256:78247b6d45f43a52ef35c25b5581459e85117225408a4128a3daf8bf9648ac69", size = 43271 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d0/5d/768a7f2ccebb29604def61842fd54f6f5f75c79e366ee8748dda84de0b13/lazy_object_proxy-1.10.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:e98c8af98d5707dcdecc9ab0863c0ea6e88545d42ca7c3feffb6b4d1e370c7ba", size = 27560 },
            { url = "https://files.pythonhosted.org/packages/b3/ce/f369815549dbfa4bebed541fa4e1561d69e4f268a1f6f77da886df182dab/lazy_object_proxy-1.10.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:952c81d415b9b80ea261d2372d2a4a2332a3890c2b83e0535f263ddfe43f0d43", size = 72403 },
            { url = "https://files.pythonhosted.org/packages/44/46/3771e0a4315044aa7b67da892b2fb1f59dfcf0eaff2c8967b2a0a85d5896/lazy_object_proxy-1.10.0-cp312-cp312-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:80b39d3a151309efc8cc48675918891b865bdf742a8616a337cb0090791a0de9", size = 72401 },
            { url = "https://files.pythonhosted.org/packages/81/39/84ce4740718e1c700bd04d3457ac92b2e9ce76529911583e7a2bf4d96eb2/lazy_object_proxy-1.10.0-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:e221060b701e2aa2ea991542900dd13907a5c90fa80e199dbf5a03359019e7a3", size = 75375 },
            { url = "https://files.pythonhosted.org/packages/86/3b/d6b65da2b864822324745c0a73fe7fd86c67ccea54173682c3081d7adea8/lazy_object_proxy-1.10.0-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:92f09ff65ecff3108e56526f9e2481b8116c0b9e1425325e13245abfd79bdb1b", size = 75466 },
            { url = "https://files.pythonhosted.org/packages/f5/33/467a093bf004a70022cb410c590d937134bba2faa17bf9dc42a48f49af35/lazy_object_proxy-1.10.0-cp312-cp312-win32.whl", hash = "sha256:3ad54b9ddbe20ae9f7c1b29e52f123120772b06dbb18ec6be9101369d63a4074", size = 25914 },
            { url = "https://files.pythonhosted.org/packages/77/ce/7956dc5ac2f8b62291b798c8363c81810e22a9effe469629d297d087e350/lazy_object_proxy-1.10.0-cp312-cp312-win_amd64.whl", hash = "sha256:127a789c75151db6af398b8972178afe6bda7d6f68730c057fbbc2e96b08d282", size = 27525 },
            { url = "https://files.pythonhosted.org/packages/31/8b/94dc8d58704ab87b39faed6f2fc0090b9d90e2e2aa2bbec35c79f3d2a054/lazy_object_proxy-1.10.0-pp310.pp311.pp312.pp38.pp39-none-any.whl", hash = "sha256:80fa48bd89c8f2f456fc0765c11c23bf5af827febacd2f523ca5bc1893fcc09d", size = 16405 },
        ]

        [[package]]
        name = "limits"
        version = "3.10.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "deprecated" },
            { name = "importlib-resources" },
            { name = "packaging" },
            { name = "typing-extensions" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/7d/84/c0c909a8f886d10248e2e16fc0ab92af6245831b53fcf635489e8ed886fe/limits-3.10.1.tar.gz", hash = "sha256:1ee31d169d498da267a1b72183ae5940afc64b17b4ed4dfd977f6ea5607c2cfb", size = 69722 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/56/90/cf9d158b7329dde86674c30fcfc2b5aea68d37d0d09e208724fd70c8e43e/limits-3.10.1-py3-none-any.whl", hash = "sha256:446242f5a6f7b8c7744e286a70793264ed81bca97860f94b821347284d14fbe9", size = 45159 },
        ]

        [[package]]
        name = "linkify-it-py"
        version = "2.0.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "uc-micro-py" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/2a/ae/bb56c6828e4797ba5a4821eec7c43b8bf40f69cda4d4f5f8c8a2810ec96a/linkify-it-py-2.0.3.tar.gz", hash = "sha256:68cda27e162e9215c17d786649d1da0021a451bdc436ef9e0fa0ba5234b9b048", size = 27946 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/04/1e/b832de447dee8b582cac175871d2f6c3d5077cc56d5575cadba1fd1cccfa/linkify_it_py-2.0.3-py3-none-any.whl", hash = "sha256:6bcbc417b0ac14323382aef5c5192c0075bf8a9d6b41820a2b66371eac6b6d79", size = 19820 },
        ]

        [[package]]
        name = "lockfile"
        version = "0.12.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/17/47/72cb04a58a35ec495f96984dddb48232b551aafb95bde614605b754fe6f7/lockfile-0.12.2.tar.gz", hash = "sha256:6aed02de03cba24efabcd600b30540140634fc06cfa603822d508d5361e9f799", size = 20874 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c8/22/9460e311f340cb62d26a38c419b1381b8593b0bb6b5d1f056938b086d362/lockfile-0.12.2-py2.py3-none-any.whl", hash = "sha256:6c3cb24f344923d30b2785d5ad75182c8ea7ac1b6171b08657258ec7429d50fa", size = 13564 },
        ]

        [[package]]
        name = "mako"
        version = "1.3.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d4/1b/71434d9fa9be1ac1bc6fb5f54b9d41233be2969f16be759766208f49f072/Mako-1.3.2.tar.gz", hash = "sha256:2a0c8ad7f6274271b3bb7467dd37cf9cc6dab4bc19cb69a4ef10669402de698e", size = 390659 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2b/8d/9f11d0b9ac521febb806e7f30dc5982d0f4f5821217712c59005fbc5c1e3/Mako-1.3.2-py3-none-any.whl", hash = "sha256:32a99d70754dfce237019d17ffe4a282d2d3351b9c476e90d8a60e63f133b80c", size = 78667 },
        ]

        [[package]]
        name = "markdown"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/22/02/4785861427848cc11e452cc62bb541006a1087cf04a1de83aedd5530b948/Markdown-3.6.tar.gz", hash = "sha256:ed4f41f6daecbeeb96e576ce414c41d2d876daa9a16cb35fa8ed8c2ddfad0224", size = 354715 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fc/b3/0c0c994fe49cd661084f8d5dc06562af53818cc0abefaca35bdc894577c3/Markdown-3.6-py3-none-any.whl", hash = "sha256:48f276f4d8cfb8ce6527c8f79e2ee29708508bf4d40aa410fbc3b4ee832c850f", size = 105381 },
        ]

        [[package]]
        name = "markdown-it-py"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "mdurl" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/38/71/3b932df36c1a044d397a1f92d1cf91ee0a503d91e470cbd670aa66b07ed0/markdown-it-py-3.0.0.tar.gz", hash = "sha256:e3f60a94fa066dc52ec76661e37c851cb232d92f9886b15cb560aaada2df8feb", size = 74596 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/42/d7/1ec15b46af6af88f19b8e5ffea08fa375d433c998b8a7639e76935c14f1f/markdown_it_py-3.0.0-py3-none-any.whl", hash = "sha256:355216845c60bd96232cd8d8c40e8f9765cc86f46880e43a8fd22dc1a1a8cab1", size = 87528 },
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
        name = "marshmallow"
        version = "3.21.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "packaging" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/5b/17/1b117d1875d8287a85cc2d5e2effd3f31bd8afd9f142c7b8391b9d665f0c/marshmallow-3.21.1.tar.gz", hash = "sha256:4e65e9e0d80fc9e609574b9983cf32579f305c718afb30d7233ab818571768c3", size = 176377 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/04/37055b7013dfaaf66e3a9a51e46857cc9be151476a891b995fa70da7e139/marshmallow-3.21.1-py3-none-any.whl", hash = "sha256:f085493f79efb0644f270a9bf2892843142d80d7174bbbd2f3713f2a589dc633", size = 49362 },
        ]

        [[package]]
        name = "marshmallow-enum"
        version = "1.5.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "marshmallow" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8e/8c/ceecdce57dfd37913143087fffd15f38562a94f0d22823e3c66eac0dca31/marshmallow-enum-1.5.1.tar.gz", hash = "sha256:38e697e11f45a8e64b4a1e664000897c659b60aa57bfa18d44e226a9920b6e58", size = 4013 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c6/59/ef3a3dc499be447098d4a89399beb869f813fee1b5a57d5d79dee2c1bf51/marshmallow_enum-1.5.1-py2.py3-none-any.whl", hash = "sha256:57161ab3dbfde4f57adeb12090f39592e992b9c86d206d02f6bd03ebec60f072", size = 4186 },
        ]

        [[package]]
        name = "marshmallow-oneofschema"
        version = "3.1.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "marshmallow" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/35/75/8dd134f08375845910d134e50246fdfcab3f1d84ab3284bd09bb15f69be9/marshmallow_oneofschema-3.1.1.tar.gz", hash = "sha256:68b4a57d0281a04ac25d4eb7a4c5865a57090a0a8fd30fd6362c8e833ac6a6d9", size = 8684 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/5c/81/3ef15337c19d3e3432945aad738081a5f54c16885277c7dff300b5f85b24/marshmallow_oneofschema-3.1.1-py3-none-any.whl", hash = "sha256:ff4cb2a488785ee8edd521a765682c2c80c78b9dc48894124531bdfa1ec9303b", size = 5726 },
        ]

        [[package]]
        name = "marshmallow-sqlalchemy"
        version = "0.26.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "marshmallow" },
            { name = "sqlalchemy" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9f/5c/2446f7692fa458f8884b4be2f35723a307c464e2edb150754c86f3b3acbe/marshmallow-sqlalchemy-0.26.1.tar.gz", hash = "sha256:d8525f74de51554b5c8491effe036f60629a426229befa33ff614c8569a16a73", size = 49540 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/84/1f4d7393d04f2ae0d4098791d1901a713f45ba70ff6f3c35ff2f7fd81f7b/marshmallow_sqlalchemy-0.26.1-py2.py3-none-any.whl", hash = "sha256:ba7493eeb8669a3bf00d8f906b657feaa87a740ae9e4ecf829cfd6ddf763d276", size = 15639 },
        ]

        [[package]]
        name = "mdit-py-plugins"
        version = "0.4.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markdown-it-py" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b4/db/61960d68d5c39ff0dd48cb799a39ae4e297f6e9b96bf2f8da29d897fba0c/mdit_py_plugins-0.4.0.tar.gz", hash = "sha256:d8ab27e9aed6c38aa716819fedfde15ca275715955f8a185a8e1cf90fb1d2c1b", size = 41531 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e5/3c/fe85f19699a7b40c8f9ce8ecee7e269b9b3c94099306df6f9891bdefeedd/mdit_py_plugins-0.4.0-py3-none-any.whl", hash = "sha256:b51b3bb70691f57f974e257e367107857a93b36f322a9e6d44ca5bf28ec2def9", size = 54080 },
        ]

        [[package]]
        name = "mdurl"
        version = "0.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d6/54/cfe61301667036ec958cb99bd3efefba235e65cdeb9c84d24a8293ba1d90/mdurl-0.1.2.tar.gz", hash = "sha256:bb413d29f5eea38f31dd4754dd7377d4465116fb207585f97bf925588687c1ba", size = 8729 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b3/38/89ba8ad64ae25be8de66a6d463314cf1eb366222074cfda9ee839c56a4b4/mdurl-0.1.2-py3-none-any.whl", hash = "sha256:84008a41e51615a49fc9966191ff91509e3c40b939176e643fd50a5c2196b8f8", size = 9979 },
        ]

        [[package]]
        name = "more-itertools"
        version = "10.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/df/ad/7905a7fd46ffb61d976133a4f47799388209e73cbc8c1253593335da88b4/more-itertools-10.2.0.tar.gz", hash = "sha256:8fccb480c43d3e99a00087634c06dd02b0d50fbf088b380de5a41a015ec239e1", size = 114449 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/50/e2/8e10e465ee3987bb7c9ab69efb91d867d93959095f4807db102d07995d94/more_itertools-10.2.0-py3-none-any.whl", hash = "sha256:686b06abe565edfab151cb8fd385a05651e1fdf8f0a14191e4439283421f8684", size = 57015 },
        ]

        [[package]]
        name = "msgspec"
        version = "0.18.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/5e/fb/42b1865063fddb14dbcbb6e74e0a366ecf1ba371c4948664dde0b0e10f95/msgspec-0.18.6.tar.gz", hash = "sha256:a59fc3b4fcdb972d09138cb516dbde600c99d07c38fd9372a6ef500d2d031b4e", size = 216757 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/1d/b5/c8fbf1db814eb29eda402952374b594b2559419ba7ec6d0997a9e5687530/msgspec-0.18.6-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:d86f5071fe33e19500920333c11e2267a31942d18fed4d9de5bc2fbab267d28c", size = 202109 },
            { url = "https://files.pythonhosted.org/packages/d7/9a/235d2dbab078a0b8e6f338205dc59be0b027ce000554ee6a9c41b19339e5/msgspec-0.18.6-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:ce13981bfa06f5eb126a3a5a38b1976bddb49a36e4f46d8e6edecf33ccf11df1", size = 190281 },
            { url = "https://files.pythonhosted.org/packages/0e/f2/f864ed36a8a62c26b57c3e08d212bd8f3d12a3ca3ef64600be5452aa3c82/msgspec-0.18.6-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:e97dec6932ad5e3ee1e3c14718638ba333befc45e0661caa57033cd4cc489466", size = 210305 },
            { url = "https://files.pythonhosted.org/packages/73/16/dfef780ced7d690dd5497846ed242ef3e27e319d59d1ddaae816a4f2c15e/msgspec-0.18.6-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:ad237100393f637b297926cae1868b0d500f764ccd2f0623a380e2bcfb2809ca", size = 212510 },
            { url = "https://files.pythonhosted.org/packages/c1/90/f5b3a788c4b3d92190e3345d1afa3dd107d5f16b8194e1f61b72582ee9bd/msgspec-0.18.6-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:db1d8626748fa5d29bbd15da58b2d73af25b10aa98abf85aab8028119188ed57", size = 214844 },
            { url = "https://files.pythonhosted.org/packages/ce/0b/d4cc1b09f8dfcc6cc4cc9739c13a86e093fe70257b941ea9feb15df22996/msgspec-0.18.6-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:d70cb3d00d9f4de14d0b31d38dfe60c88ae16f3182988246a9861259c6722af6", size = 217113 },
            { url = "https://files.pythonhosted.org/packages/3f/76/30d8f152299f65c85c46a2cbeaf95ad1d18516b5ce730acdaef696d4cfe6/msgspec-0.18.6-cp312-cp312-win_amd64.whl", hash = "sha256:1003c20bfe9c6114cc16ea5db9c5466e49fae3d7f5e2e59cb70693190ad34da0", size = 187184 },
        ]

        [[package]]
        name = "multidict"
        version = "6.0.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/f9/79/722ca999a3a09a63b35aac12ec27dfa8e5bb3a38b0f857f7a1a209a88836/multidict-6.0.5.tar.gz", hash = "sha256:f7e301075edaf50500f0b341543c41194d8df3ae5caf4702f2095f3ca73dd8da", size = 59867 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/90/9c/7fda9c0defa09538c97b1f195394be82a1f53238536f70b32eb5399dfd4e/multidict-6.0.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:51d035609b86722963404f711db441cf7134f1889107fb171a970c9701f92e1e", size = 49575 },
            { url = "https://files.pythonhosted.org/packages/be/21/d6ca80dd1b9b2c5605ff7475699a8ff5dc6ea958cd71fb2ff234afc13d79/multidict-6.0.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:cbebcd5bcaf1eaf302617c114aa67569dd3f090dd0ce8ba9e35e9985b41ac35b", size = 29638 },
            { url = "https://files.pythonhosted.org/packages/9c/18/9565f32c19d186168731e859692dfbc0e98f66a1dcf9e14d69c02a78b75a/multidict-6.0.5-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:2ffc42c922dbfddb4a4c3b438eb056828719f07608af27d163191cb3e3aa6cc5", size = 29874 },
            { url = "https://files.pythonhosted.org/packages/4e/4e/3815190e73e6ef101b5681c174c541bf972a1b064e926e56eea78d06e858/multidict-6.0.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ceb3b7e6a0135e092de86110c5a74e46bda4bd4fbfeeb3a3bcec79c0f861e450", size = 129914 },
            { url = "https://files.pythonhosted.org/packages/0c/08/bb47f886457e2259aefc10044e45c8a1b62f0c27228557e17775869d0341/multidict-6.0.5-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:79660376075cfd4b2c80f295528aa6beb2058fd289f4c9252f986751a4cd0496", size = 134589 },
            { url = "https://files.pythonhosted.org/packages/d5/2f/952f79b5f0795cf4e34852fc5cf4dfda6166f63c06c798361215b69c131d/multidict-6.0.5-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:e4428b29611e989719874670fd152b6625500ad6c686d464e99f5aaeeaca175a", size = 133259 },
            { url = "https://files.pythonhosted.org/packages/24/1f/af976383b0b772dd351210af5b60ff9927e3abb2f4a103e93da19a957da0/multidict-6.0.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:d84a5c3a5f7ce6db1f999fb9438f686bc2e09d38143f2d93d8406ed2dd6b9226", size = 130779 },
            { url = "https://files.pythonhosted.org/packages/fc/b1/b0a7744be00b0f5045c7ed4e4a6b8ee6bde4672b2c620474712299df5979/multidict-6.0.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:76c0de87358b192de7ea9649beb392f107dcad9ad27276324c24c91774ca5271", size = 120125 },
            { url = "https://files.pythonhosted.org/packages/d0/bf/2a1d667acf11231cdf0b97a6cd9f30e7a5cf847037b5cf6da44884284bd0/multidict-6.0.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:79a6d2ba910adb2cbafc95dad936f8b9386e77c84c35bc0add315b856d7c3abb", size = 167095 },
            { url = "https://files.pythonhosted.org/packages/5e/e8/ad6ee74b1a2050d3bc78f566dabcc14c8bf89cbe87eecec866c011479815/multidict-6.0.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:92d16a3e275e38293623ebf639c471d3e03bb20b8ebb845237e0d3664914caef", size = 155823 },
            { url = "https://files.pythonhosted.org/packages/45/7c/06926bb91752c52abca3edbfefac1ea90d9d1bc00c84d0658c137589b920/multidict-6.0.5-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:fb616be3538599e797a2017cccca78e354c767165e8858ab5116813146041a24", size = 170233 },
            { url = "https://files.pythonhosted.org/packages/3c/29/3dd36cf6b9c5abba8b97bba84eb499a168ba59c3faec8829327b3887d123/multidict-6.0.5-cp312-cp312-musllinux_1_1_s390x.whl", hash = "sha256:14c2976aa9038c2629efa2c148022ed5eb4cb939e15ec7aace7ca932f48f9ba6", size = 169035 },
            { url = "https://files.pythonhosted.org/packages/60/47/9a0f43470c70bbf6e148311f78ef5a3d4996b0226b6d295bdd50fdcfe387/multidict-6.0.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:435a0984199d81ca178b9ae2c26ec3d49692d20ee29bc4c11a2a8d4514c67eda", size = 166229 },
            { url = "https://files.pythonhosted.org/packages/1d/23/c1b7ae7a0b8a3e08225284ef3ecbcf014b292a3ee821bc4ed2185fd4ce7d/multidict-6.0.5-cp312-cp312-win32.whl", hash = "sha256:9fe7b0653ba3d9d65cbe7698cca585bf0f8c83dbbcc710db9c90f478e175f2d5", size = 25840 },
            { url = "https://files.pythonhosted.org/packages/4a/68/66fceb758ad7a88993940dbdf3ac59911ba9dc46d7798bf6c8652f89f853/multidict-6.0.5-cp312-cp312-win_amd64.whl", hash = "sha256:01265f5e40f5a17f8241d52656ed27192be03bfa8764d88e8220141d1e4b3556", size = 27905 },
            { url = "https://files.pythonhosted.org/packages/fa/a2/17e1e23c6be0a916219c5292f509360c345b5fa6beeb50d743203c27532c/multidict-6.0.5-py3-none-any.whl", hash = "sha256:0d63c74e3d7ab26de115c49bffc92cc77ed23395303d496eae515d4204a625e7", size = 9729 },
        ]

        [[package]]
        name = "ordered-set"
        version = "4.1.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/4c/ca/bfac8bc689799bcca4157e0e0ced07e70ce125193fc2e166d2e685b7e2fe/ordered-set-4.1.0.tar.gz", hash = "sha256:694a8e44c87657c59292ede72891eb91d34131f6531463aab3009191c77364a8", size = 12826 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/33/55/af02708f230eb77084a299d7b08175cff006dea4f2721074b92cdb0296c0/ordered_set-4.1.0-py3-none-any.whl", hash = "sha256:046e1132c71fcf3330438a539928932caf51ddbc582496833e23de611de14562", size = 7634 },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 },
        ]

        [[package]]
        name = "pathspec"
        version = "0.9.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/f6/33/436c5cb94e9f8902e59d1d544eb298b83c84b9ec37b5b769c5a0ad6edb19/pathspec-0.9.0.tar.gz", hash = "sha256:e564499435a2673d586f6b2130bb5b95f04a3ba06f81b8f895b651a3c76aabb1", size = 29483 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/42/ba/a9d64c7bcbc7e3e8e5f93a52721b377e994c22d16196e2b0f1236774353a/pathspec-0.9.0-py2.py3-none-any.whl", hash = "sha256:7d15c4ddb0b5c802d161efc417ec1a2558ea2653c2e8ad9c19098201dc1c993a", size = 31165 },
        ]

        [[package]]
        name = "pendulum"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "python-dateutil" },
            { name = "time-machine", marker = "implementation_name != 'pypy'" },
            { name = "tzdata" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/fe/27c7438c6ac8b8f8bef3c6e571855602ee784b85d072efddfff0ceb1cd77/pendulum-3.0.0.tar.gz", hash = "sha256:5d034998dea404ec31fae27af6b22cff1708f830a1ed7353be4d1019bb9f584e", size = 84524 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/1e/37/17c8f0e7481a32f21b9002dd68912a8813f2c1d77b984e00af56eb9ae31b/pendulum-3.0.0-cp312-cp312-macosx_10_12_x86_64.whl", hash = "sha256:409e64e41418c49f973d43a28afe5df1df4f1dd87c41c7c90f1a63f61ae0f1f7", size = 362284 },
            { url = "https://files.pythonhosted.org/packages/12/e6/08f462f6ea87e2159f19b43ff88231d26e02bda31c10bcb29290a617ace4/pendulum-3.0.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:a38ad2121c5ec7c4c190c7334e789c3b4624798859156b138fcc4d92295835dc", size = 352964 },
            { url = "https://files.pythonhosted.org/packages/47/29/b6877f6b53b91356c2c56d19ddab17b165ca994ad1e57b32c089e79f3fb5/pendulum-3.0.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:fde4d0b2024b9785f66b7f30ed59281bd60d63d9213cda0eb0910ead777f6d37", size = 335848 },
            { url = "https://files.pythonhosted.org/packages/2b/77/62ca666f30b2558342deadda26290a575459a7b59248ea1e978b84175227/pendulum-3.0.0-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:4b2c5675769fb6d4c11238132962939b960fcb365436b6d623c5864287faa319", size = 362215 },
            { url = "https://files.pythonhosted.org/packages/e0/29/ce37593f5ea51862c60dadf4e863d604f954478b3abbcc60a14dc05e242c/pendulum-3.0.0-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8af95e03e066826f0f4c65811cbee1b3123d4a45a1c3a2b4fc23c4b0dff893b5", size = 448673 },
            { url = "https://files.pythonhosted.org/packages/72/6a/68a8c7b8f1977d89aabfd0e2becb0921e5515dfb365097e98a522334a151/pendulum-3.0.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:2165a8f33cb15e06c67070b8afc87a62b85c5a273e3aaa6bc9d15c93a4920d6f", size = 384891 },
            { url = "https://files.pythonhosted.org/packages/30/e6/edd699300f47a3c53c0d8ed26e905b9a31057c3646211e58cc540716a440/pendulum-3.0.0-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:ad5e65b874b5e56bd942546ea7ba9dd1d6a25121db1c517700f1c9de91b28518", size = 559558 },
            { url = "https://files.pythonhosted.org/packages/d4/97/95a44aa5e1763d3a966551ed0e12f56508d8dfcc60e1f0395909b6a08626/pendulum-3.0.0-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:17fe4b2c844bbf5f0ece69cfd959fa02957c61317b2161763950d88fed8e13b9", size = 558240 },
            { url = "https://files.pythonhosted.org/packages/9a/91/fcd992eb36b77ab43f2cf44307b72c01a6fbb27f55c1bb2d4af30e9a6cb7/pendulum-3.0.0-cp312-none-win_amd64.whl", hash = "sha256:78f8f4e7efe5066aca24a7a57511b9c2119f5c2b5eb81c46ff9222ce11e0a7a5", size = 293456 },
            { url = "https://files.pythonhosted.org/packages/3b/60/ba8aa296ca6d76603d58146b4a222cd99e7da33831158b8c00240a896a56/pendulum-3.0.0-cp312-none-win_arm64.whl", hash = "sha256:28f49d8d1e32aae9c284a90b6bb3873eee15ec6e1d9042edd611b22a94ac462f", size = 288054 },
        ]

        [[package]]
        name = "pkg"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "quickpath-airflow-operator" },
        ]

        [package.optional-dependencies]
        x1 = [
            { name = "apache-airflow", version = "2.5.0", source = { registry = "https://pypi.org/simple" } },
        ]
        x2 = [
            { name = "apache-airflow", version = "2.6.0", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "apache-airflow", marker = "extra == 'x1'", specifier = "==2.5.0" },
            { name = "apache-airflow", marker = "extra == 'x2'", specifier = "==2.6.0" },
            { name = "quickpath-airflow-operator", specifier = "==1.0.2" },
        ]
        provides-extras = ["x1", "x2"]

        [[package]]
        name = "pluggy"
        version = "1.4.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/54/c6/43f9d44d92aed815e781ca25ba8c174257e27253a94630d21be8725a2b59/pluggy-1.4.0.tar.gz", hash = "sha256:8c85c2876142a764e5b7548e7d9a0e0ddb46f5185161049a79b7e974454223be", size = 65812 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a5/5b/0cc789b59e8cc1bf288b38111d002d8c5917123194d45b29dcdac64723cc/pluggy-1.4.0-py3-none-any.whl", hash = "sha256:7db9f7b503d67d1c5b95f59773ebb58a8c1c288129a88665838012cfb07b8981", size = 20120 },
        ]

        [[package]]
        name = "prison"
        version = "0.2.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "six" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/50/65/4456caa4e9bbd1d4d4b5eecaea41bb2cd31efe0e7e423c7a9ad8e2be75ea/prison-0.2.1.tar.gz", hash = "sha256:e6cd724044afcb1a8a69340cad2f1e3151a5839fd3a8027fd1357571e797c599", size = 12040 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f1/bd/e55e14cd213174100be0353824f2add41e8996c6f32081888897e8ec48b5/prison-0.2.1-py2.py3-none-any.whl", hash = "sha256:f90bab63fca497aa0819a852f64fb21a4e181ed9f6114deaa5dc04001a7555c5", size = 5794 },
        ]

        [[package]]
        name = "psutil"
        version = "5.9.8"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/90/c7/6dc0a455d111f68ee43f27793971cf03fe29b6ef972042549db29eec39a2/psutil-5.9.8.tar.gz", hash = "sha256:6be126e3225486dff286a8fb9a06246a5253f4c7c53b475ea5f5ac934e64194c", size = 503247 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e7/e3/07ae864a636d70a8a6f58da27cb1179192f1140d5d1da10886ade9405797/psutil-5.9.8-cp36-abi3-macosx_10_9_x86_64.whl", hash = "sha256:aee678c8720623dc456fa20659af736241f575d79429a0e5e9cf88ae0605cc81", size = 248702 },
            { url = "https://files.pythonhosted.org/packages/b3/bd/28c5f553667116b2598b9cc55908ec435cb7f77a34f2bff3e3ca765b0f78/psutil-5.9.8-cp36-abi3-manylinux_2_12_i686.manylinux2010_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:8cb6403ce6d8e047495a701dc7c5bd788add903f8986d523e3e20b98b733e421", size = 285242 },
            { url = "https://files.pythonhosted.org/packages/c5/4f/0e22aaa246f96d6ac87fe5ebb9c5a693fbe8877f537a1022527c47ca43c5/psutil-5.9.8-cp36-abi3-manylinux_2_12_x86_64.manylinux2010_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:d06016f7f8625a1825ba3732081d77c94589dca78b7a3fc072194851e88461a4", size = 288191 },
            { url = "https://files.pythonhosted.org/packages/6e/f5/2aa3a4acdc1e5940b59d421742356f133185667dd190b166dbcfcf5d7b43/psutil-5.9.8-cp37-abi3-win32.whl", hash = "sha256:bc56c2a1b0d15aa3eaa5a60c9f3f8e3e565303b465dbf57a1b730e7a2b9844e0", size = 251252 },
            { url = "https://files.pythonhosted.org/packages/93/52/3e39d26feae7df0aa0fd510b14012c3678b36ed068f7d78b8d8784d61f0e/psutil-5.9.8-cp37-abi3-win_amd64.whl", hash = "sha256:8db4c1b57507eef143a15a6884ca10f7c73876cdf5d51e713151c1236a0e68cf", size = 255090 },
            { url = "https://files.pythonhosted.org/packages/05/33/2d74d588408caedd065c2497bdb5ef83ce6082db01289a1e1147f6639802/psutil-5.9.8-cp38-abi3-macosx_11_0_arm64.whl", hash = "sha256:d16bbddf0693323b8c6123dd804100241da461e41d6e332fb0ba6058f630f8c8", size = 249898 },
        ]

        [[package]]
        name = "pycparser"
        version = "2.21"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/5e/0b/95d387f5f4433cb0f53ff7ad859bd2c6051051cebbb564f139a999ab46de/pycparser-2.21.tar.gz", hash = "sha256:e644fdec12f7872f86c58ff790da456218b10f863970249516d60a5eaca77206", size = 170877 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/62/d5/5f610ebe421e85889f2e55e33b7f9a6795bd982198517d912eb1c76e1a53/pycparser-2.21-py2.py3-none-any.whl", hash = "sha256:8ee45429555515e1f6b185e78100aea234072576aa43ab53aefcae078162fca9", size = 118697 },
        ]

        [[package]]
        name = "pydantic"
        version = "2.6.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "annotated-types" },
            { name = "pydantic-core" },
            { name = "typing-extensions" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4b/de/38b517edac45dd022e5d139aef06f9be4762ec2e16e2b14e1634ba28886b/pydantic-2.6.4.tar.gz", hash = "sha256:b1704e0847db01817624a6b86766967f552dd9dbf3afba4004409f908dcc84e6", size = 680828 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e5/f3/8296f550276194a58c5500d55b19a27ae0a5a3a51ffef66710c58544b32d/pydantic-2.6.4-py3-none-any.whl", hash = "sha256:cc46fce86607580867bdc3361ad462bab9c222ef042d3da86f2fb333e1d916c5", size = 394911 },
        ]

        [[package]]
        name = "pydantic-core"
        version = "2.16.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "typing-extensions" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/77/3f/65dbe5231946fe02b4e6ea92bc303d2462f45d299890fd5e8bfe4d1c3d66/pydantic_core-2.16.3.tar.gz", hash = "sha256:1cac689f80a3abab2d3c0048b29eea5751114054f032a941a32de4c852c59cad", size = 368930 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/03/c8/9afd3a316123806d7bff177beba7906ab9dd267845ae42f98f051d2250a0/pydantic_core-2.16.3-cp312-cp312-macosx_10_12_x86_64.whl", hash = "sha256:0f56ae86b60ea987ae8bcd6654a887238fd53d1384f9b222ac457070b7ac4cff", size = 1900858 },
            { url = "https://files.pythonhosted.org/packages/e7/b2/b6eef8d0a914e44826785cc99cd7a1711c2eea2dfc69bc3aefc3be507234/pydantic_core-2.16.3-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:c9bd22a2a639e26171068f8ebb5400ce2c1bc7d17959f60a3b753ae13c632975", size = 1710501 },
            { url = "https://files.pythonhosted.org/packages/3c/82/b79a75a6f5b19f7f43b08671f6b818a335b5d970b9e50a39acd3f07aed32/pydantic_core-2.16.3-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:4204e773b4b408062960e65468d5346bdfe139247ee5f1ca2a378983e11388a2", size = 1858820 },
            { url = "https://files.pythonhosted.org/packages/60/7e/5bdb72aa8f1de0a0e38194dd261b5335747ef8d9bf3421fc960498442830/pydantic_core-2.16.3-cp312-cp312-manylinux_2_17_armv7l.manylinux2014_armv7l.whl", hash = "sha256:f651dd19363c632f4abe3480a7c87a9773be27cfe1341aef06e8759599454120", size = 1851491 },
            { url = "https://files.pythonhosted.org/packages/d7/d9/b3d217a092bf23b143e59a691d61598c308386293c310ff6746a0c8ed6a5/pydantic_core-2.16.3-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:aaf09e615a0bf98d406657e0008e4a8701b11481840be7d31755dc9f97c44053", size = 2046483 },
            { url = "https://files.pythonhosted.org/packages/54/c0/7ecafb2dad658078bf28e4045a29a7b2de76319ebbc8cf7ef177d17e4d9e/pydantic_core-2.16.3-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8e47755d8152c1ab5b55928ab422a76e2e7b22b5ed8e90a7d584268dd49e9c6b", size = 2937056 },
            { url = "https://files.pythonhosted.org/packages/dc/df/cd1cdd79a307c06fbea11be2cd8f361604b82f9b28c7712bd1220c44f226/pydantic_core-2.16.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:500960cb3a0543a724a81ba859da816e8cf01b0e6aaeedf2c3775d12ee49cade", size = 2156558 },
            { url = "https://files.pythonhosted.org/packages/7c/6e/3c188b11eef09d15702f3808bf6d0b2828a4268fb4be19ac7a2ef4f6a8c7/pydantic_core-2.16.3-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.whl", hash = "sha256:cf6204fe865da605285c34cf1172879d0314ff267b1c35ff59de7154f35fdc2e", size = 1926070 },
            { url = "https://files.pythonhosted.org/packages/46/28/cb10d96904bd7483a6237855e427876e72c369ec100d6c946d468257bbb8/pydantic_core-2.16.3-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d33dd21f572545649f90c38c227cc8631268ba25c460b5569abebdd0ec5974ca", size = 2034580 },
            { url = "https://files.pythonhosted.org/packages/af/9b/3eb4c9dc8712543424b1731c44d3597f56ed4be3bdfbec3f9a45111b774a/pydantic_core-2.16.3-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:49d5d58abd4b83fb8ce763be7794d09b2f50f10aa65c0f0c1696c677edeb7cbf", size = 2167261 },
            { url = "https://files.pythonhosted.org/packages/d5/a2/e320fd95c61c7420908b318a74f76f562a8434180c3e60fa880b3c2d4338/pydantic_core-2.16.3-cp312-none-win32.whl", hash = "sha256:f53aace168a2a10582e570b7736cc5bef12cae9cf21775e3eafac597e8551fbe", size = 1755601 },
            { url = "https://files.pythonhosted.org/packages/21/58/88e734fd2e5bd894e3eccd41be3169b8292e820ef82337f17ec4291c0668/pydantic_core-2.16.3-cp312-none-win_amd64.whl", hash = "sha256:0d32576b1de5a30d9a97f300cc6a3f4694c428d956adbc7e6e2f9cad279e45ed", size = 1867737 },
            { url = "https://files.pythonhosted.org/packages/42/cb/c44678e6f3b517bd89beebc2bd0afc440674b9820d008ef3d0fac482476a/pydantic_core-2.16.3-cp312-none-win_arm64.whl", hash = "sha256:ec08be75bb268473677edb83ba71e7e74b43c008e4a7b1907c6d57e940bf34b6", size = 1848305 },
        ]

        [[package]]
        name = "pygments"
        version = "2.17.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/55/59/8bccf4157baf25e4aa5a0bb7fa3ba8600907de105ebc22b0c78cfbf6f565/pygments-2.17.2.tar.gz", hash = "sha256:da46cec9fd2de5be3a8a784f434e4c4ab670b4ff54d605c4c2717e9d49c4c367", size = 4827772 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/97/9c/372fef8377a6e340b1704768d20daaded98bf13282b5327beb2e2fe2c7ef/pygments-2.17.2-py3-none-any.whl", hash = "sha256:b27c2826c47d0f3219f29554824c30c5e8945175d888647acd804ddd04af846c", size = 1179756 },
        ]

        [[package]]
        name = "pyjwt"
        version = "2.8.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/30/72/8259b2bccfe4673330cea843ab23f86858a419d8f1493f66d413a76c7e3b/PyJWT-2.8.0.tar.gz", hash = "sha256:57e28d156e3d5c10088e0c68abb90bfac3df82b40a71bd0daa20c65ccd5c23de", size = 78313 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2b/4f/e04a8067c7c96c364cef7ef73906504e2f40d690811c021e1a1901473a19/PyJWT-2.8.0-py3-none-any.whl", hash = "sha256:59127c392cc44c2da5bb3192169a91f429924e17aff6534d70fdc02ab3e04320", size = 22591 },
        ]

        [[package]]
        name = "python-daemon"
        version = "3.0.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "docutils" },
            { name = "lockfile" },
            { name = "setuptools" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/84/50/97b81327fccbb70eb99f3c95bd05a0c9d7f13fb3f4cfd975885110d1205a/python-daemon-3.0.1.tar.gz", hash = "sha256:6c57452372f7eaff40934a1c03ad1826bf5e793558e87fef49131e6464b4dae5", size = 81337 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/83/7f/feffd97af851e2a837b5ca9bfbe570002c45397734724e4abfd4c62fdd0d/python_daemon-3.0.1-py3-none-any.whl", hash = "sha256:42bb848a3260a027fa71ad47ecd959e471327cb34da5965962edd5926229f341", size = 31872 },
        ]

        [[package]]
        name = "python-dateutil"
        version = "2.9.0.post0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "six" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/66/c0/0c8b6ad9f17a802ee498c46e004a0eb49bc148f2fd230864601a86dcf6db/python-dateutil-2.9.0.post0.tar.gz", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 342432 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ec/57/56b9bcc3c9c6a792fcbaf139543cee77261f3651ca9da0c93f5c1221264b/python_dateutil-2.9.0.post0-py2.py3-none-any.whl", hash = "sha256:a8b2bc7bffae282281c8140a97d3aa9c14da0b136dfe83f850eea9a5f7470427", size = 229892 },
        ]

        [[package]]
        name = "python-multipart"
        version = "0.0.9"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/5c/0f/9c55ac6c84c0336e22a26fa84ca6c51d58d7ac3a2d78b0dfa8748826c883/python_multipart-0.0.9.tar.gz", hash = "sha256:03f54688c663f1b7977105f021043b0793151e4cb1c1a9d4a11fc13d622c4026", size = 31516 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3d/47/444768600d9e0ebc82f8e347775d24aef8f6348cf00e9fa0e81910814e6d/python_multipart-0.0.9-py3-none-any.whl", hash = "sha256:97ca7b8ea7b05f977dc3849c3ba99d51689822fab725c3703af7c866a0c2b215", size = 22299 },
        ]

        [[package]]
        name = "python-nvd3"
        version = "0.15.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "jinja2" },
            { name = "python-slugify" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/0b/aa/97165daa6e319409c5c2582e62736a7353bda3c90d90fdcb0b11e116dd2d/python-nvd3-0.15.0.tar.gz", hash = "sha256:fbd75ff47e0ef255b4aa4f3a8b10dc8b4024aa5a9a7abed5b2406bd3cb817715", size = 31954 }

        [[package]]
        name = "python-slugify"
        version = "8.0.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "text-unidecode" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/87/c7/5e1547c44e31da50a460df93af11a535ace568ef89d7a811069ead340c4a/python-slugify-8.0.4.tar.gz", hash = "sha256:59202371d1d05b54a9e7720c5e038f928f45daaffe41dd10822f3907b937c856", size = 10921 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a4/62/02da182e544a51a5c3ccf4b03ab79df279f9c60c5e82d5e8bec7ca26ac11/python_slugify-8.0.4-py2.py3-none-any.whl", hash = "sha256:276540b79961052b66b7d116620b36518847f52d5fd9e3a70164fc8c50faa6b8", size = 10051 },
        ]

        [[package]]
        name = "pytz"
        version = "2024.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/90/26/9f1f00a5d021fff16dee3de13d43e5e978f3d58928e129c3a62cf7eb9738/pytz-2024.1.tar.gz", hash = "sha256:2a29735ea9c18baf14b448846bde5a48030ed267578472d8955cd0e7443a9812", size = 316214 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9c/3d/a121f284241f08268b21359bd425f7d4825cffc5ac5cd0e1b3d82ffd2b10/pytz-2024.1-py2.py3-none-any.whl", hash = "sha256:328171f4e3623139da4983451950b28e95ac706e13f3f2630a879749e7a8b319", size = 505474 },
        ]

        [[package]]
        name = "pyyaml"
        version = "6.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/cd/e5/af35f7ea75cf72f2cd079c95ee16797de7cd71f29ea7c68ae5ce7be1eda0/PyYAML-6.0.1.tar.gz", hash = "sha256:bfdf460b1736c775f2ba9f6a92bca30bc2095067b8a9d77876d1fad6cc3b4a43", size = 125201 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bc/06/1b305bf6aa704343be85444c9d011f626c763abb40c0edc1cad13bfd7f86/PyYAML-6.0.1-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:855fb52b0dc35af121542a76b9a84f8d1cd886ea97c84703eaa6d88e37a2ad28", size = 178692 },
            { url = "https://files.pythonhosted.org/packages/84/02/404de95ced348b73dd84f70e15a41843d817ff8c1744516bf78358f2ffd2/PyYAML-6.0.1-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:40df9b996c2b73138957fe23a16a4f0ba614f4c0efce1e9406a184b6d07fa3a9", size = 165622 },
            { url = "https://files.pythonhosted.org/packages/c7/4c/4a2908632fc980da6d918b9de9c1d9d7d7e70b2672b1ad5166ed27841ef7/PyYAML-6.0.1-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:a08c6f0fe150303c1c6b71ebcd7213c2858041a7e01975da3a99aed1e7a378ef", size = 696937 },
            { url = "https://files.pythonhosted.org/packages/b4/33/720548182ffa8344418126017aa1d4ab4aeec9a2275f04ce3f3573d8ace8/PyYAML-6.0.1-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:6c22bec3fbe2524cde73d7ada88f6566758a8f7227bfbf93a408a9d86bcc12a0", size = 724969 },
            { url = "https://files.pythonhosted.org/packages/4f/78/77b40157b6cb5f2d3d31a3d9b2efd1ba3505371f76730d267e8b32cf4b7f/PyYAML-6.0.1-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:8d4e9c88387b0f5c7d5f281e55304de64cf7f9c0021a3525bd3b1c542da3b0e4", size = 712604 },
            { url = "https://files.pythonhosted.org/packages/2e/97/3e0e089ee85e840f4b15bfa00e4e63d84a3691ababbfea92d6f820ea6f21/PyYAML-6.0.1-cp312-cp312-win32.whl", hash = "sha256:d483d2cdf104e7c9fa60c544d92981f12ad66a457afae824d146093b8c294c54", size = 126098 },
            { url = "https://files.pythonhosted.org/packages/2b/9f/fbade56564ad486809c27b322d0f7e6a89c01f6b4fe208402e90d4443a99/PyYAML-6.0.1-cp312-cp312-win_amd64.whl", hash = "sha256:0d3304d8c0adc42be59c5f8a4d9e3d7379e6955ad754aa9d6ab7a398b59dd1df", size = 138675 },
        ]

        [[package]]
        name = "quickpath-airflow-operator"
        version = "1.0.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "apache-airflow", version = "2.5.0", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x1'" },
            { name = "apache-airflow", version = "2.6.0", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-3-pkg-x2' or extra != 'extra-3-pkg-x1'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d0/df/a0954234d18c9903a6c1a8991a6702230ceb9a1e1b7221fa02b57a4a7de4/quickpath-airflow-operator-1.0.2.tar.gz", hash = "sha256:c678424ee7f2802aacb281336ad78b2315f73f32307be99fdc3a3a8b1724a86b", size = 4696 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/54/6f/5a61a1cd52804e492de2dae368cde66b08438aceb81fcf11e152fa855eec/quickpath_airflow_operator-1.0.2-py3-none-any.whl", hash = "sha256:09967f8afd28c01c46856fbe8141e64dd42b14865c3b9a537861cfe0f441675e", size = 6388 },
        ]

        [[package]]
        name = "referencing"
        version = "0.34.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "attrs" },
            { name = "rpds-py" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/59/d7/48b862b8133da2e0ed091195028f0d45c4d0be0f7f23dbe046a767282f37/referencing-0.34.0.tar.gz", hash = "sha256:5773bd84ef41799a5a8ca72dc34590c041eb01bf9aa02632b4a973fb0181a844", size = 62624 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/42/8e/ae1de7b12223986e949bdb886c004de7c304b6fa94de5b87c926c1099656/referencing-0.34.0-py3-none-any.whl", hash = "sha256:d53ae300ceddd3169f1ffa9caf2cb7b769e92657e4fafb23d34b93679116dfd4", size = 26661 },
        ]

        [[package]]
        name = "requests"
        version = "2.31.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "certifi" },
            { name = "charset-normalizer" },
            { name = "idna" },
            { name = "urllib3" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6414b7a155ccd8f7f9d4380d62fd5b955065c3/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7ffb0d1cf7af05c3d2791656dbd6ad1e1", size = 110794 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/70/8e/0e2d847013cb52cd35b38c009bb167a1a26b2ce6cd6965bf26b47bc0bf44/requests-2.31.0-py3-none-any.whl", hash = "sha256:58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f", size = 62574 },
        ]

        [[package]]
        name = "requests-toolbelt"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "requests" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/f3/61/d7545dafb7ac2230c70d38d31cbfe4cc64f7144dc41f6e4e4b78ecd9f5bb/requests-toolbelt-1.0.0.tar.gz", hash = "sha256:7681a0a3d047012b5bdc0ee37d7f8f07ebe76ab08caeccfc3921ce23c88d5bc6", size = 206888 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3f/51/d4db610ef29373b879047326cbf6fa98b6c1969d6f6dc423279de2b1be2c/requests_toolbelt-1.0.0-py2.py3-none-any.whl", hash = "sha256:cccfdd665f0a24fcf4726e690f65639d272bb0637b9b92dfd91a5568ccf6bd06", size = 54481 },
        ]

        [[package]]
        name = "rfc3339-validator"
        version = "0.1.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "six" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/28/ea/a9387748e2d111c3c2b275ba970b735e04e15cdb1eb30693b6b5708c4dbd/rfc3339_validator-0.1.4.tar.gz", hash = "sha256:138a2abdf93304ad60530167e51d2dfb9549521a836871b88d7f4695d0022f6b", size = 5513 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7b/44/4e421b96b67b2daff264473f7465db72fbdf36a07e05494f50300cc7b0c6/rfc3339_validator-0.1.4-py2.py3-none-any.whl", hash = "sha256:24f6ec1eda14ef823da9e36ec7113124b39c04d50a4d3d3a3c2859577e7791fa", size = 3490 },
        ]

        [[package]]
        name = "rich"
        version = "13.7.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markdown-it-py" },
            { name = "pygments" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b3/01/c954e134dc440ab5f96952fe52b4fdc64225530320a910473c1fe270d9aa/rich-13.7.1.tar.gz", hash = "sha256:9be308cb1fe2f1f57d67ce99e95af38a1e2bc71ad9813b0e247cf7ffbcc3a432", size = 221248 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/87/67/a37f6214d0e9fe57f6ae54b2956d550ca8365857f42a1ce0392bb21d9410/rich-13.7.1-py3-none-any.whl", hash = "sha256:4edbae314f59eb482f54e9e30bf00d33350aaa94f4bfcd4e9e3110e64d0d7222", size = 240681 },
        ]

        [[package]]
        name = "rich-argparse"
        version = "1.4.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "rich" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/7b/04/4b0b9b06662a4559041a88c2d31e93ecbc8aca1c45fee10a0c1a000b7274/rich_argparse-1.4.0.tar.gz", hash = "sha256:c275f34ea3afe36aec6342c2a2298893104b5650528941fb53c21067276dba19", size = 17112 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/f6/19c1208dd565f13ff6062db31006b6ddf4c68c1946d5ee7bc0fec0592f1d/rich_argparse-1.4.0-py3-none-any.whl", hash = "sha256:68b263d3628d07b1d27cfe6ad896da2f5a5583ee2ba226aeeb24459840023b38", size = 19948 },
        ]

        [[package]]
        name = "rpds-py"
        version = "0.18.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/55/ba/ce7b9f0fc5323f20ffdf85f682e51bee8dc03e9b54503939ebb63d1d0d5e/rpds_py-0.18.0.tar.gz", hash = "sha256:42821446ee7a76f5d9f71f9e33a4fb2ffd724bb3e7f93386150b61a43115788d", size = 25313 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/09/b6/45690f5d3f8c551bb462e063a2f336d72c8884ed26aa19beb53a374d3854/rpds_py-0.18.0-cp312-cp312-macosx_10_12_x86_64.whl", hash = "sha256:7f2facbd386dd60cbbf1a794181e6aa0bd429bd78bfdf775436020172e2a23f0", size = 338820 },
            { url = "https://files.pythonhosted.org/packages/7a/58/9bfc53b266df92f0515e72fd16e4890dc6b56fc3bfc216b3a2a729c866b5/rpds_py-0.18.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:1d9a5be316c15ffb2b3c405c4ff14448c36b4435be062a7f578ccd8b01f0c4d8", size = 332988 },
            { url = "https://files.pythonhosted.org/packages/5a/57/2fcfd462cc53876ac4acef69dbf4fb941da971440049ca72051da54ea60d/rpds_py-0.18.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:cd5bf1af8efe569654bbef5a3e0a56eca45f87cfcffab31dd8dde70da5982475", size = 1106717 },
            { url = "https://files.pythonhosted.org/packages/a3/7e/37298d351e0b0ee6136a0663a0836c7dc22acbf4554835244aa40d9e5d43/rpds_py-0.18.0-cp312-cp312-manylinux_2_17_armv7l.manylinux2014_armv7l.whl", hash = "sha256:5417558f6887e9b6b65b4527232553c139b57ec42c64570569b155262ac0754f", size = 1120259 },
            { url = "https://files.pythonhosted.org/packages/c9/26/285661286e0c3fe398082de9b3009cd25198f776484269f61d29f60ecbfb/rpds_py-0.18.0-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:56a737287efecafc16f6d067c2ea0117abadcd078d58721f967952db329a3e5c", size = 1219992 },
            { url = "https://files.pythonhosted.org/packages/9a/8b/d446775cffcb0c07ea7183cc85e0ffd02bb25c68ce5bb248bf03ee5a2192/rpds_py-0.18.0-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8f03bccbd8586e9dd37219bce4d4e0d3ab492e6b3b533e973fa08a112cb2ffc9", size = 1276119 },
            { url = "https://files.pythonhosted.org/packages/c3/96/2211a1ca4b4e259e222169074ec0fa41f0ee18665dfc68988a139dc7e6e8/rpds_py-0.18.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:4457a94da0d5c53dc4b3e4de1158bdab077db23c53232f37a3cb7afdb053a4e3", size = 1115487 },
            { url = "https://files.pythonhosted.org/packages/29/71/59074d37725cee2140cb9c3404fbfa70b2dcf037f2dcce3b7a4db3967f18/rpds_py-0.18.0-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.whl", hash = "sha256:0ab39c1ba9023914297dd88ec3b3b3c3f33671baeb6acf82ad7ce883f6e8e157", size = 1141402 },
            { url = "https://files.pythonhosted.org/packages/f9/d9/355890b2273f3cbfb7666dfac80c6ac59ad8f97a7d6d4f24c444bed504ea/rpds_py-0.18.0-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:9d54553c1136b50fd12cc17e5b11ad07374c316df307e4cfd6441bea5fb68496", size = 1357339 },
            { url = "https://files.pythonhosted.org/packages/de/67/330d6f74a9ab37cf1597d5f7fb40437346b00dce15dc14c31aeb96762c56/rpds_py-0.18.0-cp312-cp312-musllinux_1_2_i686.whl", hash = "sha256:0af039631b6de0397ab2ba16eaf2872e9f8fca391b44d3d8cac317860a700a3f", size = 1381314 },
            { url = "https://files.pythonhosted.org/packages/41/78/6be52bb734db3774c6093848774b4dd4d5866bc32bb208f2d335a6c9861b/rpds_py-0.18.0-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:84ffab12db93b5f6bad84c712c92060a2d321b35c3c9960b43d08d0f639d60d7", size = 1364328 },
            { url = "https://files.pythonhosted.org/packages/55/96/3e9646719bc6a719951f32bb03069caaa873536ad6429b21b3a4059d2008/rpds_py-0.18.0-cp312-none-win32.whl", hash = "sha256:685537e07897f173abcf67258bee3c05c374fa6fff89d4c7e42fb391b0605e98", size = 195011 },
            { url = "https://files.pythonhosted.org/packages/14/8c/e69f5636f4ab6ee0855aef3b16e6c97f8b636e9e04fa5a4bcc75126acb13/rpds_py-0.18.0-cp312-none-win_amd64.whl", hash = "sha256:e003b002ec72c8d5a3e3da2989c7d6065b47d9eaa70cd8808b5384fbb970f4ec", size = 206344 },
        ]

        [[package]]
        name = "setproctitle"
        version = "1.3.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ff/e1/b16b16a1aa12174349d15b73fd4b87e641a8ae3fb1163e80938dbbf6ae98/setproctitle-1.3.3.tar.gz", hash = "sha256:c913e151e7ea01567837ff037a23ca8740192880198b7fbb90b16d181607caae", size = 27253 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/32/22/9672612b194e4ac5d9fb67922ad9d30232b4b66129b0381ab5efeb6ae88f/setproctitle-1.3.3-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:d4460795a8a7a391e3567b902ec5bdf6c60a47d791c3b1d27080fc203d11c9dc", size = 16917 },
            { url = "https://files.pythonhosted.org/packages/49/e5/562ff00f2f3f4253ff8fa6886e0432b8eae8cde82530ac19843d8ed2c485/setproctitle-1.3.3-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:bdfd7254745bb737ca1384dee57e6523651892f0ea2a7344490e9caefcc35e64", size = 11264 },
            { url = "https://files.pythonhosted.org/packages/8f/1f/f97ea7bf71c873590a63d62ba20bf7294439d1c28603e5c63e3616c2131a/setproctitle-1.3.3-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:477d3da48e216d7fc04bddab67b0dcde633e19f484a146fd2a34bb0e9dbb4a1e", size = 31907 },
            { url = "https://files.pythonhosted.org/packages/66/fb/2d90806b9a2ed97c140baade3d1d2d41d3b51458300a2d999268be24d21d/setproctitle-1.3.3-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:ab2900d111e93aff5df9fddc64cf51ca4ef2c9f98702ce26524f1acc5a786ae7", size = 33333 },
            { url = "https://files.pythonhosted.org/packages/38/39/e7ce791f5635f3a16bd21d6b79bd9280c4c4aed8ab936b4b21334acf05a7/setproctitle-1.3.3-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:088b9efc62d5aa5d6edf6cba1cf0c81f4488b5ce1c0342a8b67ae39d64001120", size = 30573 },
            { url = "https://files.pythonhosted.org/packages/20/22/fd76bbde4194d4e31d5b31a02f80c8e7e54a99d3d8ff34f3d656c6655689/setproctitle-1.3.3-cp312-cp312-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a6d50252377db62d6a0bb82cc898089916457f2db2041e1d03ce7fadd4a07381", size = 31601 },
            { url = "https://files.pythonhosted.org/packages/51/5c/a6257cc68e17abcc4d4a78cc6666aa0d3805af6d942576625c4a468a72f0/setproctitle-1.3.3-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:87e668f9561fd3a457ba189edfc9e37709261287b52293c115ae3487a24b92f6", size = 40717 },
            { url = "https://files.pythonhosted.org/packages/db/31/4f0faad7ef641be4e8dfcbc40829775f2d6a4ca1ff435a4074047fa3dad1/setproctitle-1.3.3-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:287490eb90e7a0ddd22e74c89a92cc922389daa95babc833c08cf80c84c4df0a", size = 39384 },
            { url = "https://files.pythonhosted.org/packages/22/17/8763dc4f9ddf36af5f043ceec213b0f9f45f09fd2d5061a89c699aabe8b0/setproctitle-1.3.3-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:4fe1c49486109f72d502f8be569972e27f385fe632bd8895f4730df3c87d5ac8", size = 42350 },
            { url = "https://files.pythonhosted.org/packages/7b/b2/2403cecf2e5c5b4da22f7d9df4b2149bf92d03a3422185e682e81055549c/setproctitle-1.3.3-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:4a6ba2494a6449b1f477bd3e67935c2b7b0274f2f6dcd0f7c6aceae10c6c6ba3", size = 40704 },
            { url = "https://files.pythonhosted.org/packages/5e/c1/11e80061ac06aece2a0ffcaf018cdc088aebb2fc586f68201755518532ad/setproctitle-1.3.3-cp312-cp312-win32.whl", hash = "sha256:2df2b67e4b1d7498632e18c56722851ba4db5d6a0c91aaf0fd395111e51cdcf4", size = 11057 },
            { url = "https://files.pythonhosted.org/packages/90/e8/ece468e93e99d3b2826e9649f6d03e80f071d451e20c742f201f77d1bea1/setproctitle-1.3.3-cp312-cp312-win_amd64.whl", hash = "sha256:f38d48abc121263f3b62943f84cbaede05749047e428409c2c199664feb6abc7", size = 11809 },
        ]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485 },
        ]

        [[package]]
        name = "six"
        version = "1.16.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/71/39/171f1c67cd00715f190ba0b100d606d440a28c93c7714febeca8b79af85e/six-1.16.0.tar.gz", hash = "sha256:1e61c37477a1626458e36f7b1d82aa5c9b094fa4802892072e49de9c60c4c926", size = 34041 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d9/5a/e7c31adbe875f2abbb91bd84cf2dc52d792b5a01506781dbcf25c91daf11/six-1.16.0-py2.py3-none-any.whl", hash = "sha256:8abb2f1d86890a2dfb989f9a77cfcfd3e47c2a354b01111771326f8aa26e0254", size = 11053 },
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
        name = "sqlalchemy"
        version = "1.4.52"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "greenlet", marker = "platform_machine == 'AMD64' or platform_machine == 'WIN32' or platform_machine == 'aarch64' or platform_machine == 'amd64' or platform_machine == 'ppc64le' or platform_machine == 'win32' or platform_machine == 'x86_64'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8a/a4/b5991829c34af0505e0f2b1ccf9588d1ba90f2d984ee208c90c985f1265a/SQLAlchemy-1.4.52.tar.gz", hash = "sha256:80e63bbdc5217dad3485059bdf6f65a7d43f33c8bde619df5c220edf03d87296", size = 8514200 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fc/30/7e04f16d0508d4e57edd5c8def5810bb31bc73203beacd8bf83ed18ff0f1/SQLAlchemy-1.4.52-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:49e3772eb3380ac88d35495843daf3c03f094b713e66c7d017e322144a5c6b7c", size = 1589216 },
            { url = "https://files.pythonhosted.org/packages/ce/e6/9da1e081321a514c0147a2e0b293f27ca93f0f299cbd5ba746a9422a9f07/SQLAlchemy-1.4.52-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:618827c1a1c243d2540314c6e100aee7af09a709bd005bae971686fab6723554", size = 1628827 },
            { url = "https://files.pythonhosted.org/packages/10/c1/1613a8dcd05e6dacc9505554ce6c217a1cfda0da9c7592e258856945c6b6/SQLAlchemy-1.4.52-cp312-cp312-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:de9acf369aaadb71a725b7e83a5ef40ca3de1cf4cdc93fa847df6b12d3cd924b", size = 1627867 },
            { url = "https://files.pythonhosted.org/packages/0e/a7/97e7893673165b41dacfb07476df83a2fb5c9445feea5e54ad6ed3d27cb5/SQLAlchemy-1.4.52-cp312-cp312-win32.whl", hash = "sha256:763bd97c4ebc74136ecf3526b34808c58945023a59927b416acebcd68d1fc126", size = 1589871 },
            { url = "https://files.pythonhosted.org/packages/49/62/d0e4502e27eaa10da35243d5241c3be3ed3974d607281e3b4ccc065d9853/SQLAlchemy-1.4.52-cp312-cp312-win_amd64.whl", hash = "sha256:f12aaf94f4d9679ca475975578739e12cc5b461172e04d66f7a3c39dd14ffc64", size = 1591783 },
        ]

        [[package]]
        name = "sqlalchemy-jsonfield"
        version = "1.0.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "sqlalchemy" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d0/77/88de5c9ac1a44db1abb493d9d0995681b200ad625d80a4a289c7be438d80/SQLAlchemy-JSONField-1.0.2.tar.gz", hash = "sha256:dab3abc9d75a1640e7f3d4875564a4199f665d27863da8d5a089e4eaca5e67f2", size = 15879 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fd/67/d75d119e70863e0519c8eec5fc66714d34ad1ee9e5e73bf4fc8e3d259fac/SQLAlchemy_JSONField-1.0.2-py3-none-any.whl", hash = "sha256:b2945fa1e60b07d5764a7c73b18da427948b35dd4c07c0e94939001dc2dacf77", size = 10217 },
        ]

        [[package]]
        name = "sqlalchemy-utils"
        version = "0.41.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "sqlalchemy" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/bf/abfd5474cdd89ddd36dbbde9c6efba16bfa7f5448913eba946fed14729da/SQLAlchemy-Utils-0.41.2.tar.gz", hash = "sha256:bc599c8c3b3319e53ce6c5c3c471120bd325d0071fb6f38a10e924e3d07b9990", size = 138017 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d5/f0/dc4757b83ac1ab853cf222df8535ed73973e0c203d983982ba7b8bc60508/SQLAlchemy_Utils-0.41.2-py3-none-any.whl", hash = "sha256:85cf3842da2bf060760f955f8467b87983fb2e30f1764fd0e24a48307dc8ec6e", size = 93083 },
        ]

        [[package]]
        name = "sqlparse"
        version = "0.4.4"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/65/16/10f170ec641ed852611b6c9441b23d10b5702ab5288371feab3d36de2574/sqlparse-0.4.4.tar.gz", hash = "sha256:d446183e84b8349fa3061f0fe7f06ca94ba65b426946ffebe6e3e8295332420c", size = 72383 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/98/5a/66d7c9305baa9f11857f247d4ba761402cea75db6058ff850ed7128957b7/sqlparse-0.4.4-py3-none-any.whl", hash = "sha256:5430a4fe2ac7d0f93e66f1efc6e1338a41884b7ddf2a350cedd20ccc4d9d28f3", size = 41183 },
        ]

        [[package]]
        name = "starlette"
        version = "0.37.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "anyio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/61/b5/6bceb93ff20bd7ca36e6f7c540581abb18f53130fabb30ba526e26fd819b/starlette-0.37.2.tar.gz", hash = "sha256:9af890290133b79fc3db55474ade20f6220a364a0402e0b556e7cd5e1e093823", size = 2843736 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fd/18/31fa32ed6c68ba66220204ef0be798c349d0a20c1901f9d4a794e08c76d8/starlette-0.37.2-py3-none-any.whl", hash = "sha256:6fe59f29268538e5d0d182f2791a479a0c64638e6935d1c6989e63fb2699c6ee", size = 71908 },
        ]

        [[package]]
        name = "swagger-ui-bundle"
        version = "1.1.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "jinja2", marker = "extra == 'extra-3-pkg-x1'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/01/e6/d8ae21087a42627c2a04a738c947825b78c26b18595704b94bd3227197a2/swagger_ui_bundle-1.1.0.tar.gz", hash = "sha256:20673c3431c8733d5d1615ecf79d9acf30cff75202acaf21a7d9c7f489714529", size = 2599741 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a2/66/8fb11445940bde7ca328d6aa23dd36b6056197d862f4bd6bb51c820c50e5/swagger_ui_bundle-1.1.0-py3-none-any.whl", hash = "sha256:f7526f7bb99923e10594c54247265839bec97e96b0438561ac86faf40d40dd57", size = 2626591 },
        ]

        [[package]]
        name = "tabulate"
        version = "0.9.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ec/fe/802052aecb21e3797b8f7902564ab6ea0d60ff8ca23952079064155d1ae1/tabulate-0.9.0.tar.gz", hash = "sha256:0095b12bf5966de529c0feb1fa08671671b3368eec77d7ef7ab114be2c068b3c", size = 81090 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/40/44/4a5f08c96eb108af5cb50b41f76142f0afa346dfa99d5296fe7202a11854/tabulate-0.9.0-py3-none-any.whl", hash = "sha256:024ca478df22e9340661486f85298cff5f6dcdba14f3813e8830015b9ed1948f", size = 35252 },
        ]

        [[package]]
        name = "tenacity"
        version = "8.2.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/89/3c/253e1627262373784bf9355db9d6f20d2d8831d79f91e9cca48050cddcc2/tenacity-8.2.3.tar.gz", hash = "sha256:5398ef0d78e63f40007c1fb4c0bff96e1911394d2fa8d194f77619c05ff6cc8a", size = 40651 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f4/f1/990741d5bb2487d529d20a433210ffa136a367751e454214013b441c4575/tenacity-8.2.3-py3-none-any.whl", hash = "sha256:ce510e327a630c9e1beaf17d42e6ffacc88185044ad85cf74c0a8887c6a0f88c", size = 24401 },
        ]

        [[package]]
        name = "termcolor"
        version = "2.4.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/10/56/d7d66a84f96d804155f6ff2873d065368b25a07222a6fd51c4f24ef6d764/termcolor-2.4.0.tar.gz", hash = "sha256:aab9e56047c8ac41ed798fa36d892a37aca6b3e9159f3e0c24bc64a9b3ac7b7a", size = 12664 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d9/5f/8c716e47b3a50cbd7c146f45881e11d9414def768b7cd9c5e6650ec2a80a/termcolor-2.4.0-py3-none-any.whl", hash = "sha256:9297c0df9c99445c2412e832e882a7884038a25617c60cea2ad69488d4040d63", size = 7719 },
        ]

        [[package]]
        name = "text-unidecode"
        version = "1.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ab/e2/e9a00f0ccb71718418230718b3d900e71a5d16e701a3dae079a21e9cd8f8/text-unidecode-1.3.tar.gz", hash = "sha256:bad6603bb14d279193107714b288be206cac565dfa49aa5b105294dd5c4aab93", size = 76885 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a6/a5/c0b6468d3824fe3fde30dbb5e1f687b291608f9473681bbf7dabbf5a87d7/text_unidecode-1.3-py2.py3-none-any.whl", hash = "sha256:1311f10e8b895935241623731c2ba64f4c455287888b18189350b67134a822e8", size = 78154 },
        ]

        [[package]]
        name = "time-machine"
        version = "2.14.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "python-dateutil" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/dc/c1/ca7e6e7cc4689dadf599d7432dd649b195b84262000ed5d87d52aafcb7e6/time-machine-2.14.1.tar.gz", hash = "sha256:57dc7efc1dde4331902d1bdefd34e8ee890a5c28533157e3b14a429c86b39533", size = 24965 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3f/4f/3dc3995be802f820fa608bc9eab2ece3399ddb7ba8306faf890a5cb96bb6/time_machine-2.14.1-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:30a4a18357fa6cf089eeefcb37e9549b42523aebb5933894770a8919e6c398e1", size = 20542 },
            { url = "https://files.pythonhosted.org/packages/6b/09/42850355ba563f92022c9512445b5973d93a806e601bb5ff4e9340675556/time_machine-2.14.1-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:d45bd60bea85869615b117667f10a821e3b0d3603c47bfd105b45d1f67156fc8", size = 16825 },
            { url = "https://files.pythonhosted.org/packages/42/8c/5e32b5b8bf6be7938e1f7964ba159c85ac4c11123e2e2f296f2578598a6e/time_machine-2.14.1-cp312-cp312-macosx_14_0_arm64.whl", hash = "sha256:39de6d37a14ff8882d4f1cbd50c53268b54e1cf4ef9be2bfe590d10a51ccd314", size = 17174 },
            { url = "https://files.pythonhosted.org/packages/31/7e/7083ec1cfdd8fb1246f6c743d13f791009c7d23c86918943c0098c62d8a7/time_machine-2.14.1-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:7fd7d188b4f9d358c6bd477daf93b460d9b244a4c296ddd065945f2b6193c2bd", size = 33978 },
            { url = "https://files.pythonhosted.org/packages/12/ea/1029dd2776c3cc42266919fb294f122b87d7480ceb84c226275f7f6fb2a4/time_machine-2.14.1-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:99e6f013e67c4f74a9d8f57e34173b2047f2ad48f764e44c38f3ee5344a38c01", size = 31820 },
            { url = "https://files.pythonhosted.org/packages/8f/68/f8570de2080ea01ff28a8510ede07eaec3581efb16a49e8da2c939554c2c/time_machine-2.14.1-cp312-cp312-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a927d87501da8b053a27e80f5d0e1e58fbde4b50d70df2d3853ed67e89a731cf", size = 33632 },
            { url = "https://files.pythonhosted.org/packages/7e/a1/ff47ac095674a0b20cba2e511662b04b2ffef128e2fbe894fff7e21db930/time_machine-2.14.1-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:c77a616561dd4c7c442e9eee8cbb915750496e9a5a7fca6bcb11a9860226d2d0", size = 38569 },
            { url = "https://files.pythonhosted.org/packages/c3/45/e6932c2c73e4f6ff575f56c137fe636fe6074785e7ef2dad6e767cd73eea/time_machine-2.14.1-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:e7fa70a6bdca40cc4a8386fd85bc1bae0a23ab11e49604ef853ab3ce92be127f", size = 36774 },
            { url = "https://files.pythonhosted.org/packages/b0/f9/aef680042be5adee01b7b0cf532b04e47456a31b10a206bd68659c06aad6/time_machine-2.14.1-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:d63ef00d389fa6d2c76c863af580b3e4a8f0ccc6a9aea8e64590588e37f13c00", size = 38920 },
            { url = "https://files.pythonhosted.org/packages/88/6c/c7f62a1beb11749aeae0b055106d4982bf725cecec20db0491f67f1f360c/time_machine-2.14.1-cp312-cp312-win32.whl", hash = "sha256:6706eb06487354a5e219cacea709fb3ec44dec3842c6218237d5069fa5f1ad64", size = 19066 },
            { url = "https://files.pythonhosted.org/packages/be/a8/27a4937a4126a9fa6cf3ee1c2c6c9b5471db88e14c2babbd7546ab497b02/time_machine-2.14.1-cp312-cp312-win_amd64.whl", hash = "sha256:36aa4f17adcd73a6064bf4991a29126cac93521f0690805edb91db837c4e1453", size = 19942 },
            { url = "https://files.pythonhosted.org/packages/22/5e/19db2889ac0930ffa3e128206422d56ff96c19a610cb6c81200d67e0af19/time_machine-2.14.1-cp312-cp312-win_arm64.whl", hash = "sha256:edea570f3835a036e8860bb8d6eb8d08473c59313db86e36e3b207f796fd7b14", size = 18279 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]

        [[package]]
        name = "tzdata"
        version = "2024.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/74/5b/e025d02cb3b66b7b76093404392d4b44343c69101cc85f4d180dd5784717/tzdata-2024.1.tar.gz", hash = "sha256:2674120f8d891909751c38abcdfd386ac0a5a1127954fbc332af6b5ceae07efd", size = 190559 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/65/58/f9c9e6be752e9fcb8b6a0ee9fb87e6e7a1f6bcab2cdc73f02bb7ba91ada0/tzdata-2024.1-py2.py3-none-any.whl", hash = "sha256:9068bc196136463f5245e51efda838afa15aaeca9903f49050dfa2679db4d252", size = 345370 },
        ]

        [[package]]
        name = "uc-micro-py"
        version = "1.0.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/91/7a/146a99696aee0609e3712f2b44c6274566bc368dfe8375191278045186b8/uc-micro-py-1.0.3.tar.gz", hash = "sha256:d321b92cff673ec58027c04015fcaa8bb1e005478643ff4a500882eaab88c48a", size = 6043 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/37/87/1f677586e8ac487e29672e4b17455758fce261de06a0d086167bb760361a/uc_micro_py-1.0.3-py3-none-any.whl", hash = "sha256:db1dffff340817673d7b466ec86114a9dc0e9d4d9b5ba229d9d60e5c12600cd5", size = 6229 },
        ]

        [[package]]
        name = "unicodecsv"
        version = "0.14.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/6f/a4/691ab63b17505a26096608cc309960b5a6bdf39e4ba1a793d5f9b1a53270/unicodecsv-0.14.1.tar.gz", hash = "sha256:018c08037d48649a0412063ff4eda26eaa81eff1546dbffa51fa5293276ff7fc", size = 10267 }

        [[package]]
        name = "urllib3"
        version = "2.2.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7a/50/7fd50a27caa0652cd4caf224aa87741ea41d3265ad13f010886167cfcc79/urllib3-2.2.1.tar.gz", hash = "sha256:d0570876c61ab9e520d776c38acbbb5b05a776d3f9ff98a5c8fd5162a444cf19", size = 291020 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl", hash = "sha256:450b20ec296a467077128bff42b73080516e71b56ff59a60a02bef2232c4fa9d", size = 121067 },
        ]

        [[package]]
        name = "werkzeug"
        version = "3.0.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/0d/cc/ff1904eb5eb4b455e442834dabf9427331ac0fa02853bf83db817a7dd53d/werkzeug-3.0.1.tar.gz", hash = "sha256:507e811ecea72b18a404947aded4b3390e1db8f826b494d76550ef45bb3b1dcc", size = 801436 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl", hash = "sha256:90a285dc0e42ad56b34e696398b8122ee4c681833fb35b8334a095d82c56da10", size = 226669 },
        ]

        [[package]]
        name = "wrapt"
        version = "1.16.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/95/4c/063a912e20bcef7124e0df97282a8af3ff3e4b603ce84c481d6d7346be0a/wrapt-1.16.0.tar.gz", hash = "sha256:5f370f952971e7d17c7d1ead40e49f32345a7f7a5373571ef44d800d06b1899d", size = 53972 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/17/224132494c1e23521868cdd57cd1e903f3b6a7ba6996b7b8f077ff8ac7fe/wrapt-1.16.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:5eb404d89131ec9b4f748fa5cfb5346802e5ee8836f57d516576e61f304f3b7b", size = 37614 },
            { url = "https://files.pythonhosted.org/packages/6a/d7/cfcd73e8f4858079ac59d9db1ec5a1349bc486ae8e9ba55698cc1f4a1dff/wrapt-1.16.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:9090c9e676d5236a6948330e83cb89969f433b1943a558968f659ead07cb3b36", size = 38316 },
            { url = "https://files.pythonhosted.org/packages/7e/79/5ff0a5c54bda5aec75b36453d06be4f83d5cd4932cc84b7cb2b52cee23e2/wrapt-1.16.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:94265b00870aa407bd0cbcfd536f17ecde43b94fb8d228560a1e9d3041462d73", size = 86322 },
            { url = "https://files.pythonhosted.org/packages/c4/81/e799bf5d419f422d8712108837c1d9bf6ebe3cb2a81ad94413449543a923/wrapt-1.16.0-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:f2058f813d4f2b5e3a9eb2eb3faf8f1d99b81c3e51aeda4b168406443e8ba809", size = 79055 },
            { url = "https://files.pythonhosted.org/packages/62/62/30ca2405de6a20448ee557ab2cd61ab9c5900be7cbd18a2639db595f0b98/wrapt-1.16.0-cp312-cp312-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:98b5e1f498a8ca1858a1cdbffb023bfd954da4e3fa2c0cb5853d40014557248b", size = 87291 },
            { url = "https://files.pythonhosted.org/packages/49/4e/5d2f6d7b57fc9956bf06e944eb00463551f7d52fc73ca35cfc4c2cdb7aed/wrapt-1.16.0-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:14d7dc606219cdd7405133c713f2c218d4252f2a469003f8c46bb92d5d095d81", size = 90374 },
            { url = "https://files.pythonhosted.org/packages/a6/9b/c2c21b44ff5b9bf14a83252a8b973fb84923764ff63db3e6dfc3895cf2e0/wrapt-1.16.0-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:49aac49dc4782cb04f58986e81ea0b4768e4ff197b57324dcbd7699c5dfb40b9", size = 83896 },
            { url = "https://files.pythonhosted.org/packages/14/26/93a9fa02c6f257df54d7570dfe8011995138118d11939a4ecd82cb849613/wrapt-1.16.0-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:418abb18146475c310d7a6dc71143d6f7adec5b004ac9ce08dc7a34e2babdc5c", size = 91738 },
            { url = "https://files.pythonhosted.org/packages/a2/5b/4660897233eb2c8c4de3dc7cefed114c61bacb3c28327e64150dc44ee2f6/wrapt-1.16.0-cp312-cp312-win32.whl", hash = "sha256:685f568fa5e627e93f3b52fda002c7ed2fa1800b50ce51f6ed1d572d8ab3e7fc", size = 35568 },
            { url = "https://files.pythonhosted.org/packages/5c/cc/8297f9658506b224aa4bd71906447dea6bb0ba629861a758c28f67428b91/wrapt-1.16.0-cp312-cp312-win_amd64.whl", hash = "sha256:dcdba5c86e368442528f7060039eda390cc4091bfd1dca41e8046af7c910dda8", size = 37653 },
            { url = "https://files.pythonhosted.org/packages/ff/21/abdedb4cdf6ff41ebf01a74087740a709e2edb146490e4d9beea054b0b7a/wrapt-1.16.0-py3-none-any.whl", hash = "sha256:6906c4100a8fcbf2fa735f6059214bb13b97f75b1a61777fcf6432121ef12ef1", size = 23362 },
        ]

        [[package]]
        name = "wtforms"
        version = "3.1.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/6a/c7/96d10183c3470f1836846f7b9527d6cb0b6c2226ebca40f36fa29f23de60/wtforms-3.1.2.tar.gz", hash = "sha256:f8d76180d7239c94c6322f7990ae1216dae3659b7aa1cee94b6318bdffb474b9", size = 134705 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/18/19/c3232f35e24dccfad372e9f341c4f3a1166ae7c66e4e1351a9467c921cc1/wtforms-3.1.2-py3-none-any.whl", hash = "sha256:bf831c042829c8cdbad74c27575098d541d039b1faa74c771545ecac916f2c07", size = 145961 },
        ]

        [[package]]
        name = "yarl"
        version = "1.9.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "multidict" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e0/ad/bedcdccbcbf91363fd425a948994f3340924145c2bc8ccb296f4a1e52c28/yarl-1.9.4.tar.gz", hash = "sha256:566db86717cf8080b99b58b083b773a908ae40f06681e87e589a976faf8246bf", size = 141869 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7b/cd/a921122610dedfed94e494af18e85aae23e93274c00ca464cfc591c8f4fb/yarl-1.9.4-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:0d2454f0aef65ea81037759be5ca9947539667eecebca092733b2eb43c965a81", size = 129561 },
            { url = "https://files.pythonhosted.org/packages/7c/a0/887c93020c788f249c24eaab288c46e5fed4d2846080eaf28ed3afc36e8d/yarl-1.9.4-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:44d8ffbb9c06e5a7f529f38f53eda23e50d1ed33c6c869e01481d3fafa6b8142", size = 81595 },
            { url = "https://files.pythonhosted.org/packages/54/99/ed3c92c38f421ba6e36caf6aa91c34118771d252dce800118fa2f44d7962/yarl-1.9.4-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:aaaea1e536f98754a6e5c56091baa1b6ce2f2700cc4a00b0d49eca8dea471074", size = 79400 },
            { url = "https://files.pythonhosted.org/packages/ea/45/65801be625ef939acc8b714cf86d4a198c0646e80dc8970359d080c47204/yarl-1.9.4-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:3777ce5536d17989c91696db1d459574e9a9bd37660ea7ee4d3344579bb6f129", size = 317397 },
            { url = "https://files.pythonhosted.org/packages/06/91/9696601a8ba674c8f0c15035cc9e94ca31f541330364adcfd5a399f598bf/yarl-1.9.4-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:9fc5fc1eeb029757349ad26bbc5880557389a03fa6ada41703db5e068881e5f2", size = 327246 },
            { url = "https://files.pythonhosted.org/packages/da/3e/bf25177b3618889bf067aacf01ef54e910cd569d14e2f84f5e7bec23bb82/yarl-1.9.4-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:ea65804b5dc88dacd4a40279af0cdadcfe74b3e5b4c897aa0d81cf86927fee78", size = 327321 },
            { url = "https://files.pythonhosted.org/packages/28/1c/bdb3411467b805737dd2720b85fd082e49f59bf0cc12dc1dfcc80ab3d274/yarl-1.9.4-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:aa102d6d280a5455ad6a0f9e6d769989638718e938a6a0a2ff3f4a7ff8c62cc4", size = 322424 },
            { url = "https://files.pythonhosted.org/packages/41/e9/53bc89f039df2824a524a2aa03ee0bfb8f0585b08949e7521f5eab607085/yarl-1.9.4-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:09efe4615ada057ba2d30df871d2f668af661e971dfeedf0c159927d48bbeff0", size = 310868 },
            { url = "https://files.pythonhosted.org/packages/79/cd/a78c3b0304a4a970b5ae3993f4f5f649443bc8bfa5622f244aed44c810ed/yarl-1.9.4-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:008d3e808d03ef28542372d01057fd09168419cdc8f848efe2804f894ae03e51", size = 323452 },
            { url = "https://files.pythonhosted.org/packages/2e/5e/1c78eb05ae0efae08498fd7ab939435a29f12c7f161732e7fe327e5b8ca1/yarl-1.9.4-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:6f5cb257bc2ec58f437da2b37a8cd48f666db96d47b8a3115c29f316313654ff", size = 313554 },
            { url = "https://files.pythonhosted.org/packages/04/e0/0029563a8434472697aebb269fdd2ffc8a19e3840add1d5fa169ec7c56e3/yarl-1.9.4-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:992f18e0ea248ee03b5a6e8b3b4738850ae7dbb172cc41c966462801cbf62cf7", size = 331029 },
            { url = "https://files.pythonhosted.org/packages/de/1b/7e6b1ad42ccc0ed059066a7ae2b6fd4bce67795d109a99ccce52e9824e96/yarl-1.9.4-cp312-cp312-musllinux_1_1_s390x.whl", hash = "sha256:0e9d124c191d5b881060a9e5060627694c3bdd1fe24c5eecc8d5d7d0eb6faabc", size = 333839 },
            { url = "https://files.pythonhosted.org/packages/85/8a/c364d6e2eeb4e128a5ee9a346fc3a09aa76739c0c4e2a7305989b54f174b/yarl-1.9.4-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:3986b6f41ad22988e53d5778f91855dc0399b043fc8946d4f2e68af22ee9ff10", size = 328251 },
            { url = "https://files.pythonhosted.org/packages/ec/9d/0da94b33b9fb89041e10f95a14a55b0fef36c60b6a1d5ff85a0c2ecb1a97/yarl-1.9.4-cp312-cp312-win32.whl", hash = "sha256:4b21516d181cd77ebd06ce160ef8cc2a5e9ad35fb1c5930882baff5ac865eee7", size = 70195 },
            { url = "https://files.pythonhosted.org/packages/c5/f4/2fdc5a11503bc61818243653d836061c9ce0370e2dd9ac5917258a007675/yarl-1.9.4-cp312-cp312-win_amd64.whl", hash = "sha256:a9bd00dc3bc395a662900f33f74feb3e757429e545d831eef5bb280252631984", size = 76397 },
            { url = "https://files.pythonhosted.org/packages/4d/05/4d79198ae568a92159de0f89e710a8d19e3fa267b719a236582eee921f4a/yarl-1.9.4-py3-none-any.whl", hash = "sha256:928cecb0ef9d5a7946eb6ff58417ad2fe9375762382f1bf5c55e61645f2c43ad", size = 31638 },
        ]
        "###
        );
    });

    Ok(())
}

/// This is a regression test[1] for repeated resolution markers in the
/// lock file.
///
/// Before this test was fixed, the `resolution-markers` in the lock file
/// below looked like this:
///
/// ```text
/// resolution-markers = [
///     "sys_platform != 'linux'",
///     "sys_platform == 'linux'",
///     "sys_platform != 'linux'",
///     "sys_platform == 'linux'",
/// ]
/// ```
///
/// [1]: <https://github.com/astral-sh/uv/issues/9296>
#[test]
fn deduplicate_resolution_markers() -> Result<()> {
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
        dependencies = []

        [project.optional-dependencies]
        x1 = [
          "idna==3.5 ; sys_platform != 'linux'",
          "idna==3.6 ; sys_platform == 'linux'",
        ]
        x2 = [
          "markupsafe==2.0.0 ; sys_platform != 'linux'",
          "markupsafe==2.1.0 ; sys_platform == 'linux'",
        ]

        [tool.uv]
        conflicts = [
          [
            { extra = "x1" },
            { extra = "x2" },
          ],
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock,
            @r###"
        version = 1
        revision = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform != 'linux' and extra != 'extra-3-pkg-x1' and extra == 'extra-3-pkg-x2'",
            "sys_platform == 'linux' and extra != 'extra-3-pkg-x1' and extra == 'extra-3-pkg-x2'",
            "sys_platform != 'linux' and extra == 'extra-3-pkg-x1' and extra != 'extra-3-pkg-x2'",
            "sys_platform == 'linux' and extra == 'extra-3-pkg-x1' and extra != 'extra-3-pkg-x2'",
            "extra != 'extra-3-pkg-x1' and extra != 'extra-3-pkg-x2'",
        ]
        conflicts = [[
            { package = "pkg", extra = "x1" },
            { package = "pkg", extra = "x2" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "idna"
        version = "3.5"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform != 'linux'",
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
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/67/6a/5b3ed5c122e20c33d2562df06faf895a6b91b0a6b96a4626440ffe1d5c8e/MarkupSafe-2.0.0.tar.gz", hash = "sha256:4fae0677f712ee090721d8b17f412f1cbceefbf0dc180fe91bab3232f38b4527", size = 18466 }

        [[package]]
        name = "markupsafe"
        version = "2.1.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/62/0f/52c009332fdadd484e898dc8f2acca0663c1031b3517070fd34ad9c1b64e/MarkupSafe-2.1.0.tar.gz", hash = "sha256:80beaf63ddfbc64a0452b841d8036ca0611e049650e20afcb882f5d3c266d65f", size = 18546 }

        [[package]]
        name = "pkg"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        x1 = [
            { name = "idna", version = "3.5", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform != 'linux'" },
            { name = "idna", version = "3.6", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform == 'linux'" },
        ]
        x2 = [
            { name = "markupsafe", version = "2.0.0", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform != 'linux'" },
            { name = "markupsafe", version = "2.1.0", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "idna", marker = "sys_platform == 'linux' and extra == 'x1'", specifier = "==3.6" },
            { name = "idna", marker = "sys_platform != 'linux' and extra == 'x1'", specifier = "==3.5" },
            { name = "markupsafe", marker = "sys_platform == 'linux' and extra == 'x2'", specifier = "==2.1.0" },
            { name = "markupsafe", marker = "sys_platform != 'linux' and extra == 'x2'", specifier = "==2.0.0" },
        ]
        provides-extras = ["x1", "x2"]
        "###
        );
    });

    Ok(())
}

/// A regression test for a case where one could get multiple versions of
/// `torch` installed into the same environment.
///
/// This occurred because of a bug in conflict marker simplification, which in
/// turn lead to a buggy lock file. So this test only needs to check that the
/// lock file is as expected.
///
/// Ref: <https://github.com/astral-sh/uv/issues/11133>
#[test]
fn incorrect_extra_simplification_leads_to_multiple_torch_packages() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2025-02-07T00:00Z");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "test"
        version = "0.0.1"
        requires-python = ">=3.10"
        dependencies = [
            "mace-torch==0.3.9",
        ]

        [project.optional-dependencies]
        chgnet = [
            "chgnet == 0.4.0",
        ]

        m3gnet = [
            "matgl == 1.1.3",
            "torch == 2.2.1",
            "torchdata == 0.7.1",
        ]

        [tool.uv]
        constraint-dependencies = [
            "dgl==2.1",
            "torch<2.6",
        ]
        conflicts = [
            [
              { extra = "chgnet" },
              { extra = "m3gnet" },
            ],
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 115 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock,
            @r###"
        version = 1
        revision = 1
        requires-python = ">=3.10"
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform == 'win32' and extra != 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra != 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform == 'win32' and extra != 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra != 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform == 'win32' and extra != 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra != 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and sys_platform == 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform == 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform == 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        conflicts = [[
            { package = "test", extra = "chgnet" },
            { package = "test", extra = "m3gnet" },
        ]]

        [options]
        exclude-newer = "2025-02-07T00:00:00Z"

        [manifest]
        constraints = [
            { name = "dgl", specifier = "==2.1" },
            { name = "torch", specifier = "<2.6" },
        ]

        [[package]]
        name = "aiohappyeyeballs"
        version = "2.4.4"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7f/55/e4373e888fdacb15563ef6fa9fa8c8252476ea071e96fb46defac9f18bf2/aiohappyeyeballs-2.4.4.tar.gz", hash = "sha256:5fdd7d87889c63183afc18ce9271f9b0a7d32c2303e394468dd45d514a757745", size = 21977 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b9/74/fbb6559de3607b3300b9be3cc64e97548d55678e44623db17820dbd20002/aiohappyeyeballs-2.4.4-py3-none-any.whl", hash = "sha256:a980909d50efcd44795c4afeca523296716d50cd756ddca6af8c65b996e27de8", size = 14756 },
        ]

        [[package]]
        name = "aiohttp"
        version = "3.11.12"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "aiohappyeyeballs" },
            { name = "aiosignal" },
            { name = "async-timeout", marker = "python_full_version < '3.11'" },
            { name = "attrs" },
            { name = "frozenlist" },
            { name = "multidict" },
            { name = "propcache" },
            { name = "yarl" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/37/4b/952d49c73084fb790cb5c6ead50848c8e96b4980ad806cf4d2ad341eaa03/aiohttp-3.11.12.tar.gz", hash = "sha256:7603ca26d75b1b86160ce1bbe2787a0b706e592af5b2504e12caa88a217767b0", size = 7673175 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/65/42/3880e133590820aa7bc6d068eb7d8e0ad9fdce9b4663f92b821d3f6b5601/aiohttp-3.11.12-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:aa8a8caca81c0a3e765f19c6953416c58e2f4cc1b84829af01dd1c771bb2f91f", size = 708721 },
            { url = "https://files.pythonhosted.org/packages/d8/8c/04869803bed108b25afad75f94c651b287851843caacbec6677d8f2d572b/aiohttp-3.11.12-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:84ede78acde96ca57f6cf8ccb8a13fbaf569f6011b9a52f870c662d4dc8cd854", size = 468596 },
            { url = "https://files.pythonhosted.org/packages/4f/f4/9074011f0d1335b161c953fb32545b6667cf24465e1932b9767874995c7e/aiohttp-3.11.12-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:584096938a001378484aa4ee54e05dc79c7b9dd933e271c744a97b3b6f644957", size = 455758 },
            { url = "https://files.pythonhosted.org/packages/fd/68/06298c57ef8f534065930b805e6dbd83613f0534447922782fb9920fce28/aiohttp-3.11.12-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:392432a2dde22b86f70dd4a0e9671a349446c93965f261dbaecfaf28813e5c42", size = 1584797 },
            { url = "https://files.pythonhosted.org/packages/bd/1e/cee6b51fcb3b1c4185a7dc62b3113bc136fae07f39386c88c90b7f79f199/aiohttp-3.11.12-cp310-cp310-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:88d385b8e7f3a870146bf5ea31786ef7463e99eb59e31db56e2315535d811f55", size = 1632535 },
            { url = "https://files.pythonhosted.org/packages/71/1f/42424462b7a09da362e1711090db9f8d68a37a33f0aab51307335517c599/aiohttp-3.11.12-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:b10a47e5390c4b30a0d58ee12581003be52eedd506862ab7f97da7a66805befb", size = 1668484 },
            { url = "https://files.pythonhosted.org/packages/f6/79/0e25542bbe3c2bfd7a12c7a49c7bce73b09a836f65079e4b77bc2bafc89e/aiohttp-3.11.12-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:0b5263dcede17b6b0c41ef0c3ccce847d82a7da98709e75cf7efde3e9e3b5cae", size = 1589708 },
            { url = "https://files.pythonhosted.org/packages/d1/13/93ae26b75e23f7d3a613872e472fae836ca100dc5bde5936ebc93ada8890/aiohttp-3.11.12-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:50c5c7b8aa5443304c55c262c5693b108c35a3b61ef961f1e782dd52a2f559c7", size = 1544752 },
            { url = "https://files.pythonhosted.org/packages/cf/5e/48847fad1b014ef92ef18ea1339a3b58eb81d3bc717b94c3627f5d2a42c5/aiohttp-3.11.12-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:d1c031a7572f62f66f1257db37ddab4cb98bfaf9b9434a3b4840bf3560f5e788", size = 1529417 },
            { url = "https://files.pythonhosted.org/packages/ae/56/fbd4ea019303f4877f0e0b8c9de92e9db24338e7545570d3f275f3c74c53/aiohttp-3.11.12-cp310-cp310-musllinux_1_2_armv7l.whl", hash = "sha256:7e44eba534381dd2687be50cbd5f2daded21575242ecfdaf86bbeecbc38dae8e", size = 1557808 },
            { url = "https://files.pythonhosted.org/packages/f1/43/112189cf6b3c482ecdd6819b420eaa0c2033426f28d741bb7f19db5dd2bb/aiohttp-3.11.12-cp310-cp310-musllinux_1_2_i686.whl", hash = "sha256:145a73850926018ec1681e734cedcf2716d6a8697d90da11284043b745c286d5", size = 1536765 },
            { url = "https://files.pythonhosted.org/packages/30/12/59986547de8306e06c7b30e547ccda02d29636e152366caba2dd8627bfe1/aiohttp-3.11.12-cp310-cp310-musllinux_1_2_ppc64le.whl", hash = "sha256:2c311e2f63e42c1bf86361d11e2c4a59f25d9e7aabdbdf53dc38b885c5435cdb", size = 1607621 },
            { url = "https://files.pythonhosted.org/packages/aa/9b/af3b323b20df3318ed20d701d8242e523d59c842ca93f23134b05c9d5054/aiohttp-3.11.12-cp310-cp310-musllinux_1_2_s390x.whl", hash = "sha256:ea756b5a7bac046d202a9a3889b9a92219f885481d78cd318db85b15cc0b7bcf", size = 1628977 },
            { url = "https://files.pythonhosted.org/packages/36/62/adf5a331a7bda475cc326dde393fa2bc5849060b1b37ac3d1bee1953f2cd/aiohttp-3.11.12-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:526c900397f3bbc2db9cb360ce9c35134c908961cdd0ac25b1ae6ffcaa2507ff", size = 1564455 },
            { url = "https://files.pythonhosted.org/packages/90/c4/4a24291f22f111a854dfdb54dc94d4e0a5229ccbb7bc7f0bed972aa50410/aiohttp-3.11.12-cp310-cp310-win32.whl", hash = "sha256:b8d3bb96c147b39c02d3db086899679f31958c5d81c494ef0fc9ef5bb1359b3d", size = 416768 },
            { url = "https://files.pythonhosted.org/packages/51/69/5221c8006acb7bb10d9e8e2238fb216571bddc2e00a8d95bcfbe2f579c57/aiohttp-3.11.12-cp310-cp310-win_amd64.whl", hash = "sha256:7fe3d65279bfbee8de0fb4f8c17fc4e893eed2dba21b2f680e930cc2b09075c5", size = 442170 },
            { url = "https://files.pythonhosted.org/packages/9c/38/35311e70196b6a63cfa033a7f741f800aa8a93f57442991cbe51da2394e7/aiohttp-3.11.12-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:87a2e00bf17da098d90d4145375f1d985a81605267e7f9377ff94e55c5d769eb", size = 708797 },
            { url = "https://files.pythonhosted.org/packages/44/3e/46c656e68cbfc4f3fc7cb5d2ba4da6e91607fe83428208028156688f6201/aiohttp-3.11.12-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:b34508f1cd928ce915ed09682d11307ba4b37d0708d1f28e5774c07a7674cac9", size = 468669 },
            { url = "https://files.pythonhosted.org/packages/a0/d6/2088fb4fd1e3ac2bfb24bc172223babaa7cdbb2784d33c75ec09e66f62f8/aiohttp-3.11.12-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:936d8a4f0f7081327014742cd51d320296b56aa6d324461a13724ab05f4b2933", size = 455739 },
            { url = "https://files.pythonhosted.org/packages/e7/dc/c443a6954a56f4a58b5efbfdf23cc6f3f0235e3424faf5a0c56264d5c7bb/aiohttp-3.11.12-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:2de1378f72def7dfb5dbd73d86c19eda0ea7b0a6873910cc37d57e80f10d64e1", size = 1685858 },
            { url = "https://files.pythonhosted.org/packages/25/67/2d5b3aaade1d5d01c3b109aa76e3aa9630531252cda10aa02fb99b0b11a1/aiohttp-3.11.12-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:b9d45dbb3aaec05cf01525ee1a7ac72de46a8c425cb75c003acd29f76b1ffe94", size = 1743829 },
            { url = "https://files.pythonhosted.org/packages/90/9b/9728fe9a3e1b8521198455d027b0b4035522be18f504b24c5d38d59e7278/aiohttp-3.11.12-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:930ffa1925393381e1e0a9b82137fa7b34c92a019b521cf9f41263976666a0d6", size = 1785587 },
            { url = "https://files.pythonhosted.org/packages/ce/cf/28fbb43d4ebc1b4458374a3c7b6db3b556a90e358e9bbcfe6d9339c1e2b6/aiohttp-3.11.12-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:8340def6737118f5429a5df4e88f440746b791f8f1c4ce4ad8a595f42c980bd5", size = 1675319 },
            { url = "https://files.pythonhosted.org/packages/e5/d2/006c459c11218cabaa7bca401f965c9cc828efbdea7e1615d4644eaf23f7/aiohttp-3.11.12-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4016e383f91f2814e48ed61e6bda7d24c4d7f2402c75dd28f7e1027ae44ea204", size = 1619982 },
            { url = "https://files.pythonhosted.org/packages/9d/83/ca425891ebd37bee5d837110f7fddc4d808a7c6c126a7d1b5c3ad72fc6ba/aiohttp-3.11.12-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:3c0600bcc1adfaaac321422d615939ef300df81e165f6522ad096b73439c0f58", size = 1654176 },
            { url = "https://files.pythonhosted.org/packages/25/df/047b1ce88514a1b4915d252513640184b63624e7914e41d846668b8edbda/aiohttp-3.11.12-cp311-cp311-musllinux_1_2_armv7l.whl", hash = "sha256:0450ada317a65383b7cce9576096150fdb97396dcfe559109b403c7242faffef", size = 1660198 },
            { url = "https://files.pythonhosted.org/packages/d3/cc/6ecb8e343f0902528620b9dbd567028a936d5489bebd7dbb0dd0914f4fdb/aiohttp-3.11.12-cp311-cp311-musllinux_1_2_i686.whl", hash = "sha256:850ff6155371fd802a280f8d369d4e15d69434651b844bde566ce97ee2277420", size = 1650186 },
            { url = "https://files.pythonhosted.org/packages/f8/f8/453df6dd69256ca8c06c53fc8803c9056e2b0b16509b070f9a3b4bdefd6c/aiohttp-3.11.12-cp311-cp311-musllinux_1_2_ppc64le.whl", hash = "sha256:8fd12d0f989c6099e7b0f30dc6e0d1e05499f3337461f0b2b0dadea6c64b89df", size = 1733063 },
            { url = "https://files.pythonhosted.org/packages/55/f8/540160787ff3000391de0e5d0d1d33be4c7972f933c21991e2ea105b2d5e/aiohttp-3.11.12-cp311-cp311-musllinux_1_2_s390x.whl", hash = "sha256:76719dd521c20a58a6c256d058547b3a9595d1d885b830013366e27011ffe804", size = 1755306 },
            { url = "https://files.pythonhosted.org/packages/30/7d/49f3bfdfefd741576157f8f91caa9ff61a6f3d620ca6339268327518221b/aiohttp-3.11.12-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:97fe431f2ed646a3b56142fc81d238abcbaff08548d6912acb0b19a0cadc146b", size = 1692909 },
            { url = "https://files.pythonhosted.org/packages/40/9c/8ce00afd6f6112ce9a2309dc490fea376ae824708b94b7b5ea9cba979d1d/aiohttp-3.11.12-cp311-cp311-win32.whl", hash = "sha256:e10c440d142fa8b32cfdb194caf60ceeceb3e49807072e0dc3a8887ea80e8c16", size = 416584 },
            { url = "https://files.pythonhosted.org/packages/35/97/4d3c5f562f15830de472eb10a7a222655d750839943e0e6d915ef7e26114/aiohttp-3.11.12-cp311-cp311-win_amd64.whl", hash = "sha256:246067ba0cf5560cf42e775069c5d80a8989d14a7ded21af529a4e10e3e0f0e6", size = 442674 },
            { url = "https://files.pythonhosted.org/packages/4d/d0/94346961acb476569fca9a644cc6f9a02f97ef75961a6b8d2b35279b8d1f/aiohttp-3.11.12-cp312-cp312-macosx_10_13_universal2.whl", hash = "sha256:e392804a38353900c3fd8b7cacbea5132888f7129f8e241915e90b85f00e3250", size = 704837 },
            { url = "https://files.pythonhosted.org/packages/a9/af/05c503f1cc8f97621f199ef4b8db65fb88b8bc74a26ab2adb74789507ad3/aiohttp-3.11.12-cp312-cp312-macosx_10_13_x86_64.whl", hash = "sha256:8fa1510b96c08aaad49303ab11f8803787c99222288f310a62f493faf883ede1", size = 464218 },
            { url = "https://files.pythonhosted.org/packages/f2/48/b9949eb645b9bd699153a2ec48751b985e352ab3fed9d98c8115de305508/aiohttp-3.11.12-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:dc065a4285307607df3f3686363e7f8bdd0d8ab35f12226362a847731516e42c", size = 456166 },
            { url = "https://files.pythonhosted.org/packages/14/fb/980981807baecb6f54bdd38beb1bd271d9a3a786e19a978871584d026dcf/aiohttp-3.11.12-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:cddb31f8474695cd61fc9455c644fc1606c164b93bff2490390d90464b4655df", size = 1682528 },
            { url = "https://files.pythonhosted.org/packages/90/cb/77b1445e0a716914e6197b0698b7a3640590da6c692437920c586764d05b/aiohttp-3.11.12-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:9dec0000d2d8621d8015c293e24589d46fa218637d820894cb7356c77eca3259", size = 1737154 },
            { url = "https://files.pythonhosted.org/packages/ff/24/d6fb1f4cede9ccbe98e4def6f3ed1e1efcb658871bbf29f4863ec646bf38/aiohttp-3.11.12-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:e3552fe98e90fdf5918c04769f338a87fa4f00f3b28830ea9b78b1bdc6140e0d", size = 1793435 },
            { url = "https://files.pythonhosted.org/packages/17/e2/9f744cee0861af673dc271a3351f59ebd5415928e20080ab85be25641471/aiohttp-3.11.12-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:6dfe7f984f28a8ae94ff3a7953cd9678550dbd2a1f9bda5dd9c5ae627744c78e", size = 1692010 },
            { url = "https://files.pythonhosted.org/packages/90/c4/4a1235c1df544223eb57ba553ce03bc706bdd065e53918767f7fa1ff99e0/aiohttp-3.11.12-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:a481a574af914b6e84624412666cbfbe531a05667ca197804ecc19c97b8ab1b0", size = 1619481 },
            { url = "https://files.pythonhosted.org/packages/60/70/cf12d402a94a33abda86dd136eb749b14c8eb9fec1e16adc310e25b20033/aiohttp-3.11.12-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:1987770fb4887560363b0e1a9b75aa303e447433c41284d3af2840a2f226d6e0", size = 1641578 },
            { url = "https://files.pythonhosted.org/packages/1b/25/7211973fda1f5e833fcfd98ccb7f9ce4fbfc0074e3e70c0157a751d00db8/aiohttp-3.11.12-cp312-cp312-musllinux_1_2_armv7l.whl", hash = "sha256:a4ac6a0f0f6402854adca4e3259a623f5c82ec3f0c049374133bcb243132baf9", size = 1684463 },
            { url = "https://files.pythonhosted.org/packages/93/60/b5905b4d0693f6018b26afa9f2221fefc0dcbd3773fe2dff1a20fb5727f1/aiohttp-3.11.12-cp312-cp312-musllinux_1_2_i686.whl", hash = "sha256:c96a43822f1f9f69cc5c3706af33239489a6294be486a0447fb71380070d4d5f", size = 1646691 },
            { url = "https://files.pythonhosted.org/packages/b4/fc/ba1b14d6fdcd38df0b7c04640794b3683e949ea10937c8a58c14d697e93f/aiohttp-3.11.12-cp312-cp312-musllinux_1_2_ppc64le.whl", hash = "sha256:a5e69046f83c0d3cb8f0d5bd9b8838271b1bc898e01562a04398e160953e8eb9", size = 1702269 },
            { url = "https://files.pythonhosted.org/packages/5e/39/18c13c6f658b2ba9cc1e0c6fb2d02f98fd653ad2addcdf938193d51a9c53/aiohttp-3.11.12-cp312-cp312-musllinux_1_2_s390x.whl", hash = "sha256:68d54234c8d76d8ef74744f9f9fc6324f1508129e23da8883771cdbb5818cbef", size = 1734782 },
            { url = "https://files.pythonhosted.org/packages/9f/d2/ccc190023020e342419b265861877cd8ffb75bec37b7ddd8521dd2c6deb8/aiohttp-3.11.12-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:c9fd9dcf9c91affe71654ef77426f5cf8489305e1c66ed4816f5a21874b094b9", size = 1694740 },
            { url = "https://files.pythonhosted.org/packages/3f/54/186805bcada64ea90ea909311ffedcd74369bfc6e880d39d2473314daa36/aiohttp-3.11.12-cp312-cp312-win32.whl", hash = "sha256:0ed49efcd0dc1611378beadbd97beb5d9ca8fe48579fc04a6ed0844072261b6a", size = 411530 },
            { url = "https://files.pythonhosted.org/packages/3d/63/5eca549d34d141bcd9de50d4e59b913f3641559460c739d5e215693cb54a/aiohttp-3.11.12-cp312-cp312-win_amd64.whl", hash = "sha256:54775858c7f2f214476773ce785a19ee81d1294a6bedc5cc17225355aab74802", size = 437860 },
            { url = "https://files.pythonhosted.org/packages/c3/9b/cea185d4b543ae08ee478373e16653722c19fcda10d2d0646f300ce10791/aiohttp-3.11.12-cp313-cp313-macosx_10_13_universal2.whl", hash = "sha256:413ad794dccb19453e2b97c2375f2ca3cdf34dc50d18cc2693bd5aed7d16f4b9", size = 698148 },
            { url = "https://files.pythonhosted.org/packages/91/5c/80d47fe7749fde584d1404a68ade29bcd7e58db8fa11fa38e8d90d77e447/aiohttp-3.11.12-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:4a93d28ed4b4b39e6f46fd240896c29b686b75e39cc6992692e3922ff6982b4c", size = 460831 },
            { url = "https://files.pythonhosted.org/packages/8e/f9/de568f8a8ca6b061d157c50272620c53168d6e3eeddae78dbb0f7db981eb/aiohttp-3.11.12-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:d589264dbba3b16e8951b6f145d1e6b883094075283dafcab4cdd564a9e353a0", size = 453122 },
            { url = "https://files.pythonhosted.org/packages/8b/fd/b775970a047543bbc1d0f66725ba72acef788028fce215dc959fd15a8200/aiohttp-3.11.12-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:e5148ca8955affdfeb864aca158ecae11030e952b25b3ae15d4e2b5ba299bad2", size = 1665336 },
            { url = "https://files.pythonhosted.org/packages/82/9b/aff01d4f9716245a1b2965f02044e4474fadd2bcfe63cf249ca788541886/aiohttp-3.11.12-cp313-cp313-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:525410e0790aab036492eeea913858989c4cb070ff373ec3bc322d700bdf47c1", size = 1718111 },
            { url = "https://files.pythonhosted.org/packages/e0/a9/166fd2d8b2cc64f08104aa614fad30eee506b563154081bf88ce729bc665/aiohttp-3.11.12-cp313-cp313-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:9bd8695be2c80b665ae3f05cb584093a1e59c35ecb7d794d1edd96e8cc9201d7", size = 1775293 },
            { url = "https://files.pythonhosted.org/packages/13/c5/0d3c89bd9e36288f10dc246f42518ce8e1c333f27636ac78df091c86bb4a/aiohttp-3.11.12-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f0203433121484b32646a5f5ea93ae86f3d9559d7243f07e8c0eab5ff8e3f70e", size = 1677338 },
            { url = "https://files.pythonhosted.org/packages/72/b2/017db2833ef537be284f64ead78725984db8a39276c1a9a07c5c7526e238/aiohttp-3.11.12-cp313-cp313-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:40cd36749a1035c34ba8d8aaf221b91ca3d111532e5ccb5fa8c3703ab1b967ed", size = 1603365 },
            { url = "https://files.pythonhosted.org/packages/fc/72/b66c96a106ec7e791e29988c222141dd1219d7793ffb01e72245399e08d2/aiohttp-3.11.12-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:a7442662afebbf7b4c6d28cb7aab9e9ce3a5df055fc4116cc7228192ad6cb484", size = 1618464 },
            { url = "https://files.pythonhosted.org/packages/3f/50/e68a40f267b46a603bab569d48d57f23508801614e05b3369898c5b2910a/aiohttp-3.11.12-cp313-cp313-musllinux_1_2_armv7l.whl", hash = "sha256:8a2fb742ef378284a50766e985804bd6adb5adb5aa781100b09befdbfa757b65", size = 1657827 },
            { url = "https://files.pythonhosted.org/packages/c5/1d/aafbcdb1773d0ba7c20793ebeedfaba1f3f7462f6fc251f24983ed738aa7/aiohttp-3.11.12-cp313-cp313-musllinux_1_2_i686.whl", hash = "sha256:2cee3b117a8d13ab98b38d5b6bdcd040cfb4181068d05ce0c474ec9db5f3c5bb", size = 1616700 },
            { url = "https://files.pythonhosted.org/packages/b0/5e/6cd9724a2932f36e2a6b742436a36d64784322cfb3406ca773f903bb9a70/aiohttp-3.11.12-cp313-cp313-musllinux_1_2_ppc64le.whl", hash = "sha256:f6a19bcab7fbd8f8649d6595624856635159a6527861b9cdc3447af288a00c00", size = 1685643 },
            { url = "https://files.pythonhosted.org/packages/8b/38/ea6c91d5c767fd45a18151675a07c710ca018b30aa876a9f35b32fa59761/aiohttp-3.11.12-cp313-cp313-musllinux_1_2_s390x.whl", hash = "sha256:e4cecdb52aaa9994fbed6b81d4568427b6002f0a91c322697a4bfcc2b2363f5a", size = 1715487 },
            { url = "https://files.pythonhosted.org/packages/8e/24/e9edbcb7d1d93c02e055490348df6f955d675e85a028c33babdcaeda0853/aiohttp-3.11.12-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:30f546358dfa0953db92ba620101fefc81574f87b2346556b90b5f3ef16e55ce", size = 1672948 },
            { url = "https://files.pythonhosted.org/packages/25/be/0b1fb737268e003198f25c3a68c2135e76e4754bf399a879b27bd508a003/aiohttp-3.11.12-cp313-cp313-win32.whl", hash = "sha256:ce1bb21fc7d753b5f8a5d5a4bae99566386b15e716ebdb410154c16c91494d7f", size = 410396 },
            { url = "https://files.pythonhosted.org/packages/68/fd/677def96a75057b0a26446b62f8fbb084435b20a7d270c99539c26573bfd/aiohttp-3.11.12-cp313-cp313-win_amd64.whl", hash = "sha256:f7914ab70d2ee8ab91c13e5402122edbc77821c66d2758abb53aabe87f013287", size = 436234 },
        ]

        [[package]]
        name = "aiosignal"
        version = "1.3.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "frozenlist" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ba/b5/6d55e80f6d8a08ce22b982eafa278d823b541c925f11ee774b0b9c43473d/aiosignal-1.3.2.tar.gz", hash = "sha256:a8c255c66fafb1e499c9351d0bf32ff2d8a0321595ebac3b93713656d2436f54", size = 19424 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ec/6a/bc7e17a3e87a2985d3e8f4da4cd0f481060eb78fb08596c42be62c90a4d9/aiosignal-1.3.2-py2.py3-none-any.whl", hash = "sha256:45cde58e409a301715980c2b01d0c28bdde3770d8290b5eb2173759d9acb31a5", size = 7597 },
        ]

        [[package]]
        name = "annotated-types"
        version = "0.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/67/531ea369ba64dcff5ec9c3402f9f51bf748cec26dde048a2f973a4eea7f5/annotated_types-0.7.0.tar.gz", hash = "sha256:aff07c09a53a08bc8cfccb9c85b05f1aa9a2a6f23728d790723543408344ce89", size = 16081 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/78/b6/6307fbef88d9b5ee7421e68d78a9f162e0da4900bc5f5793f6d3d0e34fb8/annotated_types-0.7.0-py3-none-any.whl", hash = "sha256:1f02e8b43a8fbbc3f3e0d4f0f4bfc8131bcb4eebe8849b8e5c773f3a1c582a53", size = 13643 },
        ]

        [[package]]
        name = "ase"
        version = "3.24.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "matplotlib" },
            { name = "numpy" },
            { name = "scipy" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/5c/c9/9adb9bc641bd7222367886e4e6c753b4c64da4ff2d9565ab39aee1e34734/ase-3.24.0.tar.gz", hash = "sha256:9acc93d6daaf48cd27b844c56f8bf49428b9db0542faa3cc30d9d5b8e1842195", size = 2383264 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/1f/cd/b1253035a1da90e89f31947e052c558cd83df3bcaff34aa199e5e806d773/ase-3.24.0-py3-none-any.whl", hash = "sha256:974922df87ef4ec8cf1140359a55ab4c4dc55c38e26876bdd9c00968da1f463c", size = 2928893 },
        ]

        [[package]]
        name = "async-timeout"
        version = "5.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a5/ae/136395dfbfe00dfc94da3f3e136d0b13f394cba8f4841120e34226265780/async_timeout-5.0.1.tar.gz", hash = "sha256:d9321a7a3d5a6a5e187e824d2fa0793ce379a202935782d555d6e9d2735677d3", size = 9274 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fe/ba/e2081de779ca30d473f21f5b30e0e737c438205440784c7dfc81efc2b029/async_timeout-5.0.1-py3-none-any.whl", hash = "sha256:39e3809566ff85354557ec2398b55e096c8364bacac9405a7a1fa429e77fe76c", size = 6233 },
        ]

        [[package]]
        name = "attrs"
        version = "25.1.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/49/7c/fdf464bcc51d23881d110abd74b512a42b3d5d376a55a831b44c603ae17f/attrs-25.1.0.tar.gz", hash = "sha256:1c97078a80c814273a76b2a298a932eb681c87415c11dee0a6921de7f1b02c3e", size = 810562 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fc/30/d4986a882011f9df997a55e6becd864812ccfcd821d64aac8570ee39f719/attrs-25.1.0-py3-none-any.whl", hash = "sha256:c75a69e28a550a7e93789579c22aa26b0f5b83b75dc4e08fe092980051e1090a", size = 63152 },
        ]

        [[package]]
        name = "certifi"
        version = "2025.1.31"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/1c/ab/c9f1e32b7b1bf505bf26f0ef697775960db7932abeb7b516de930ba2705f/certifi-2025.1.31.tar.gz", hash = "sha256:3d5da6925056f6f18f119200434a4780a94263f10d1c21d032a6f6b2baa20651", size = 167577 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/fc/bce832fd4fd99766c04d1ee0eead6b0ec6486fb100ae5e74c1d91292b982/certifi-2025.1.31-py3-none-any.whl", hash = "sha256:ca78db4565a652026a4db2bcdf68f2fb589ea80d0be70e03929ed730746b84fe", size = 166393 },
        ]

        [[package]]
        name = "charset-normalizer"
        version = "3.4.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/b0/572805e227f01586461c80e0fd25d65a2115599cc9dad142fee4b747c357/charset_normalizer-3.4.1.tar.gz", hash = "sha256:44251f18cd68a75b56585dd00dae26183e102cd5e0f9f1466e6df5da2ed64ea3", size = 123188 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0d/58/5580c1716040bc89206c77d8f74418caf82ce519aae06450393ca73475d1/charset_normalizer-3.4.1-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:91b36a978b5ae0ee86c394f5a54d6ef44db1de0815eb43de826d41d21e4af3de", size = 198013 },
            { url = "https://files.pythonhosted.org/packages/d0/11/00341177ae71c6f5159a08168bcb98c6e6d196d372c94511f9f6c9afe0c6/charset_normalizer-3.4.1-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:7461baadb4dc00fd9e0acbe254e3d7d2112e7f92ced2adc96e54ef6501c5f176", size = 141285 },
            { url = "https://files.pythonhosted.org/packages/01/09/11d684ea5819e5a8f5100fb0b38cf8d02b514746607934134d31233e02c8/charset_normalizer-3.4.1-cp310-cp310-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:e218488cd232553829be0664c2292d3af2eeeb94b32bea483cf79ac6a694e037", size = 151449 },
            { url = "https://files.pythonhosted.org/packages/08/06/9f5a12939db324d905dc1f70591ae7d7898d030d7662f0d426e2286f68c9/charset_normalizer-3.4.1-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:80ed5e856eb7f30115aaf94e4a08114ccc8813e6ed1b5efa74f9f82e8509858f", size = 143892 },
            { url = "https://files.pythonhosted.org/packages/93/62/5e89cdfe04584cb7f4d36003ffa2936681b03ecc0754f8e969c2becb7e24/charset_normalizer-3.4.1-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:b010a7a4fd316c3c484d482922d13044979e78d1861f0e0650423144c616a46a", size = 146123 },
            { url = "https://files.pythonhosted.org/packages/a9/ac/ab729a15c516da2ab70a05f8722ecfccc3f04ed7a18e45c75bbbaa347d61/charset_normalizer-3.4.1-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4532bff1b8421fd0a320463030c7520f56a79c9024a4e88f01c537316019005a", size = 147943 },
            { url = "https://files.pythonhosted.org/packages/03/d2/3f392f23f042615689456e9a274640c1d2e5dd1d52de36ab8f7955f8f050/charset_normalizer-3.4.1-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:d973f03c0cb71c5ed99037b870f2be986c3c05e63622c017ea9816881d2dd247", size = 142063 },
            { url = "https://files.pythonhosted.org/packages/f2/e3/e20aae5e1039a2cd9b08d9205f52142329f887f8cf70da3650326670bddf/charset_normalizer-3.4.1-cp310-cp310-musllinux_1_2_i686.whl", hash = "sha256:3a3bd0dcd373514dcec91c411ddb9632c0d7d92aed7093b8c3bbb6d69ca74408", size = 150578 },
            { url = "https://files.pythonhosted.org/packages/8d/af/779ad72a4da0aed925e1139d458adc486e61076d7ecdcc09e610ea8678db/charset_normalizer-3.4.1-cp310-cp310-musllinux_1_2_ppc64le.whl", hash = "sha256:d9c3cdf5390dcd29aa8056d13e8e99526cda0305acc038b96b30352aff5ff2bb", size = 153629 },
            { url = "https://files.pythonhosted.org/packages/c2/b6/7aa450b278e7aa92cf7732140bfd8be21f5f29d5bf334ae987c945276639/charset_normalizer-3.4.1-cp310-cp310-musllinux_1_2_s390x.whl", hash = "sha256:2bdfe3ac2e1bbe5b59a1a63721eb3b95fc9b6817ae4a46debbb4e11f6232428d", size = 150778 },
            { url = "https://files.pythonhosted.org/packages/39/f4/d9f4f712d0951dcbfd42920d3db81b00dd23b6ab520419626f4023334056/charset_normalizer-3.4.1-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:eab677309cdb30d047996b36d34caeda1dc91149e4fdca0b1a039b3f79d9a807", size = 146453 },
            { url = "https://files.pythonhosted.org/packages/49/2b/999d0314e4ee0cff3cb83e6bc9aeddd397eeed693edb4facb901eb8fbb69/charset_normalizer-3.4.1-cp310-cp310-win32.whl", hash = "sha256:c0429126cf75e16c4f0ad00ee0eae4242dc652290f940152ca8c75c3a4b6ee8f", size = 95479 },
            { url = "https://files.pythonhosted.org/packages/2d/ce/3cbed41cff67e455a386fb5e5dd8906cdda2ed92fbc6297921f2e4419309/charset_normalizer-3.4.1-cp310-cp310-win_amd64.whl", hash = "sha256:9f0b8b1c6d84c8034a44893aba5e767bf9c7a211e313a9605d9c617d7083829f", size = 102790 },
            { url = "https://files.pythonhosted.org/packages/72/80/41ef5d5a7935d2d3a773e3eaebf0a9350542f2cab4eac59a7a4741fbbbbe/charset_normalizer-3.4.1-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:8bfa33f4f2672964266e940dd22a195989ba31669bd84629f05fab3ef4e2d125", size = 194995 },
            { url = "https://files.pythonhosted.org/packages/7a/28/0b9fefa7b8b080ec492110af6d88aa3dea91c464b17d53474b6e9ba5d2c5/charset_normalizer-3.4.1-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:28bf57629c75e810b6ae989f03c0828d64d6b26a5e205535585f96093e405ed1", size = 139471 },
            { url = "https://files.pythonhosted.org/packages/71/64/d24ab1a997efb06402e3fc07317e94da358e2585165930d9d59ad45fcae2/charset_normalizer-3.4.1-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:f08ff5e948271dc7e18a35641d2f11a4cd8dfd5634f55228b691e62b37125eb3", size = 149831 },
            { url = "https://files.pythonhosted.org/packages/37/ed/be39e5258e198655240db5e19e0b11379163ad7070962d6b0c87ed2c4d39/charset_normalizer-3.4.1-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:234ac59ea147c59ee4da87a0c0f098e9c8d169f4dc2a159ef720f1a61bbe27cd", size = 142335 },
            { url = "https://files.pythonhosted.org/packages/88/83/489e9504711fa05d8dde1574996408026bdbdbd938f23be67deebb5eca92/charset_normalizer-3.4.1-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:fd4ec41f914fa74ad1b8304bbc634b3de73d2a0889bd32076342a573e0779e00", size = 143862 },
            { url = "https://files.pythonhosted.org/packages/c6/c7/32da20821cf387b759ad24627a9aca289d2822de929b8a41b6241767b461/charset_normalizer-3.4.1-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:eea6ee1db730b3483adf394ea72f808b6e18cf3cb6454b4d86e04fa8c4327a12", size = 145673 },
            { url = "https://files.pythonhosted.org/packages/68/85/f4288e96039abdd5aeb5c546fa20a37b50da71b5cf01e75e87f16cd43304/charset_normalizer-3.4.1-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:c96836c97b1238e9c9e3fe90844c947d5afbf4f4c92762679acfe19927d81d77", size = 140211 },
            { url = "https://files.pythonhosted.org/packages/28/a3/a42e70d03cbdabc18997baf4f0227c73591a08041c149e710045c281f97b/charset_normalizer-3.4.1-cp311-cp311-musllinux_1_2_i686.whl", hash = "sha256:4d86f7aff21ee58f26dcf5ae81a9addbd914115cdebcbb2217e4f0ed8982e146", size = 148039 },
            { url = "https://files.pythonhosted.org/packages/85/e4/65699e8ab3014ecbe6f5c71d1a55d810fb716bbfd74f6283d5c2aa87febf/charset_normalizer-3.4.1-cp311-cp311-musllinux_1_2_ppc64le.whl", hash = "sha256:09b5e6733cbd160dcc09589227187e242a30a49ca5cefa5a7edd3f9d19ed53fd", size = 151939 },
            { url = "https://files.pythonhosted.org/packages/b1/82/8e9fe624cc5374193de6860aba3ea8070f584c8565ee77c168ec13274bd2/charset_normalizer-3.4.1-cp311-cp311-musllinux_1_2_s390x.whl", hash = "sha256:5777ee0881f9499ed0f71cc82cf873d9a0ca8af166dfa0af8ec4e675b7df48e6", size = 149075 },
            { url = "https://files.pythonhosted.org/packages/3d/7b/82865ba54c765560c8433f65e8acb9217cb839a9e32b42af4aa8e945870f/charset_normalizer-3.4.1-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:237bdbe6159cff53b4f24f397d43c6336c6b0b42affbe857970cefbb620911c8", size = 144340 },
            { url = "https://files.pythonhosted.org/packages/b5/b6/9674a4b7d4d99a0d2df9b215da766ee682718f88055751e1e5e753c82db0/charset_normalizer-3.4.1-cp311-cp311-win32.whl", hash = "sha256:8417cb1f36cc0bc7eaba8ccb0e04d55f0ee52df06df3ad55259b9a323555fc8b", size = 95205 },
            { url = "https://files.pythonhosted.org/packages/1e/ab/45b180e175de4402dcf7547e4fb617283bae54ce35c27930a6f35b6bef15/charset_normalizer-3.4.1-cp311-cp311-win_amd64.whl", hash = "sha256:d7f50a1f8c450f3925cb367d011448c39239bb3eb4117c36a6d354794de4ce76", size = 102441 },
            { url = "https://files.pythonhosted.org/packages/0a/9a/dd1e1cdceb841925b7798369a09279bd1cf183cef0f9ddf15a3a6502ee45/charset_normalizer-3.4.1-cp312-cp312-macosx_10_13_universal2.whl", hash = "sha256:73d94b58ec7fecbc7366247d3b0b10a21681004153238750bb67bd9012414545", size = 196105 },
            { url = "https://files.pythonhosted.org/packages/d3/8c/90bfabf8c4809ecb648f39794cf2a84ff2e7d2a6cf159fe68d9a26160467/charset_normalizer-3.4.1-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:dad3e487649f498dd991eeb901125411559b22e8d7ab25d3aeb1af367df5efd7", size = 140404 },
            { url = "https://files.pythonhosted.org/packages/ad/8f/e410d57c721945ea3b4f1a04b74f70ce8fa800d393d72899f0a40526401f/charset_normalizer-3.4.1-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:c30197aa96e8eed02200a83fba2657b4c3acd0f0aa4bdc9f6c1af8e8962e0757", size = 150423 },
            { url = "https://files.pythonhosted.org/packages/f0/b8/e6825e25deb691ff98cf5c9072ee0605dc2acfca98af70c2d1b1bc75190d/charset_normalizer-3.4.1-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:2369eea1ee4a7610a860d88f268eb39b95cb588acd7235e02fd5a5601773d4fa", size = 143184 },
            { url = "https://files.pythonhosted.org/packages/3e/a2/513f6cbe752421f16d969e32f3583762bfd583848b763913ddab8d9bfd4f/charset_normalizer-3.4.1-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:bc2722592d8998c870fa4e290c2eec2c1569b87fe58618e67d38b4665dfa680d", size = 145268 },
            { url = "https://files.pythonhosted.org/packages/74/94/8a5277664f27c3c438546f3eb53b33f5b19568eb7424736bdc440a88a31f/charset_normalizer-3.4.1-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ffc9202a29ab3920fa812879e95a9e78b2465fd10be7fcbd042899695d75e616", size = 147601 },
            { url = "https://files.pythonhosted.org/packages/7c/5f/6d352c51ee763623a98e31194823518e09bfa48be2a7e8383cf691bbb3d0/charset_normalizer-3.4.1-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:804a4d582ba6e5b747c625bf1255e6b1507465494a40a2130978bda7b932c90b", size = 141098 },
            { url = "https://files.pythonhosted.org/packages/78/d4/f5704cb629ba5ab16d1d3d741396aec6dc3ca2b67757c45b0599bb010478/charset_normalizer-3.4.1-cp312-cp312-musllinux_1_2_i686.whl", hash = "sha256:0f55e69f030f7163dffe9fd0752b32f070566451afe180f99dbeeb81f511ad8d", size = 149520 },
            { url = "https://files.pythonhosted.org/packages/c5/96/64120b1d02b81785f222b976c0fb79a35875457fa9bb40827678e54d1bc8/charset_normalizer-3.4.1-cp312-cp312-musllinux_1_2_ppc64le.whl", hash = "sha256:c4c3e6da02df6fa1410a7680bd3f63d4f710232d3139089536310d027950696a", size = 152852 },
            { url = "https://files.pythonhosted.org/packages/84/c9/98e3732278a99f47d487fd3468bc60b882920cef29d1fa6ca460a1fdf4e6/charset_normalizer-3.4.1-cp312-cp312-musllinux_1_2_s390x.whl", hash = "sha256:5df196eb874dae23dcfb968c83d4f8fdccb333330fe1fc278ac5ceeb101003a9", size = 150488 },
            { url = "https://files.pythonhosted.org/packages/13/0e/9c8d4cb99c98c1007cc11eda969ebfe837bbbd0acdb4736d228ccaabcd22/charset_normalizer-3.4.1-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:e358e64305fe12299a08e08978f51fc21fac060dcfcddd95453eabe5b93ed0e1", size = 146192 },
            { url = "https://files.pythonhosted.org/packages/b2/21/2b6b5b860781a0b49427309cb8670785aa543fb2178de875b87b9cc97746/charset_normalizer-3.4.1-cp312-cp312-win32.whl", hash = "sha256:9b23ca7ef998bc739bf6ffc077c2116917eabcc901f88da1b9856b210ef63f35", size = 95550 },
            { url = "https://files.pythonhosted.org/packages/21/5b/1b390b03b1d16c7e382b561c5329f83cc06623916aab983e8ab9239c7d5c/charset_normalizer-3.4.1-cp312-cp312-win_amd64.whl", hash = "sha256:6ff8a4a60c227ad87030d76e99cd1698345d4491638dfa6673027c48b3cd395f", size = 102785 },
            { url = "https://files.pythonhosted.org/packages/38/94/ce8e6f63d18049672c76d07d119304e1e2d7c6098f0841b51c666e9f44a0/charset_normalizer-3.4.1-cp313-cp313-macosx_10_13_universal2.whl", hash = "sha256:aabfa34badd18f1da5ec1bc2715cadc8dca465868a4e73a0173466b688f29dda", size = 195698 },
            { url = "https://files.pythonhosted.org/packages/24/2e/dfdd9770664aae179a96561cc6952ff08f9a8cd09a908f259a9dfa063568/charset_normalizer-3.4.1-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:22e14b5d70560b8dd51ec22863f370d1e595ac3d024cb8ad7d308b4cd95f8313", size = 140162 },
            { url = "https://files.pythonhosted.org/packages/24/4e/f646b9093cff8fc86f2d60af2de4dc17c759de9d554f130b140ea4738ca6/charset_normalizer-3.4.1-cp313-cp313-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:8436c508b408b82d87dc5f62496973a1805cd46727c34440b0d29d8a2f50a6c9", size = 150263 },
            { url = "https://files.pythonhosted.org/packages/5e/67/2937f8d548c3ef6e2f9aab0f6e21001056f692d43282b165e7c56023e6dd/charset_normalizer-3.4.1-cp313-cp313-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:2d074908e1aecee37a7635990b2c6d504cd4766c7bc9fc86d63f9c09af3fa11b", size = 142966 },
            { url = "https://files.pythonhosted.org/packages/52/ed/b7f4f07de100bdb95c1756d3a4d17b90c1a3c53715c1a476f8738058e0fa/charset_normalizer-3.4.1-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:955f8851919303c92343d2f66165294848d57e9bba6cf6e3625485a70a038d11", size = 144992 },
            { url = "https://files.pythonhosted.org/packages/96/2c/d49710a6dbcd3776265f4c923bb73ebe83933dfbaa841c5da850fe0fd20b/charset_normalizer-3.4.1-cp313-cp313-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:44ecbf16649486d4aebafeaa7ec4c9fed8b88101f4dd612dcaf65d5e815f837f", size = 147162 },
            { url = "https://files.pythonhosted.org/packages/b4/41/35ff1f9a6bd380303dea55e44c4933b4cc3c4850988927d4082ada230273/charset_normalizer-3.4.1-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:0924e81d3d5e70f8126529951dac65c1010cdf117bb75eb02dd12339b57749dd", size = 140972 },
            { url = "https://files.pythonhosted.org/packages/fb/43/c6a0b685fe6910d08ba971f62cd9c3e862a85770395ba5d9cad4fede33ab/charset_normalizer-3.4.1-cp313-cp313-musllinux_1_2_i686.whl", hash = "sha256:2967f74ad52c3b98de4c3b32e1a44e32975e008a9cd2a8cc8966d6a5218c5cb2", size = 149095 },
            { url = "https://files.pythonhosted.org/packages/4c/ff/a9a504662452e2d2878512115638966e75633519ec11f25fca3d2049a94a/charset_normalizer-3.4.1-cp313-cp313-musllinux_1_2_ppc64le.whl", hash = "sha256:c75cb2a3e389853835e84a2d8fb2b81a10645b503eca9bcb98df6b5a43eb8886", size = 152668 },
            { url = "https://files.pythonhosted.org/packages/6c/71/189996b6d9a4b932564701628af5cee6716733e9165af1d5e1b285c530ed/charset_normalizer-3.4.1-cp313-cp313-musllinux_1_2_s390x.whl", hash = "sha256:09b26ae6b1abf0d27570633b2b078a2a20419c99d66fb2823173d73f188ce601", size = 150073 },
            { url = "https://files.pythonhosted.org/packages/e4/93/946a86ce20790e11312c87c75ba68d5f6ad2208cfb52b2d6a2c32840d922/charset_normalizer-3.4.1-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:fa88b843d6e211393a37219e6a1c1df99d35e8fd90446f1118f4216e307e48cd", size = 145732 },
            { url = "https://files.pythonhosted.org/packages/cd/e5/131d2fb1b0dddafc37be4f3a2fa79aa4c037368be9423061dccadfd90091/charset_normalizer-3.4.1-cp313-cp313-win32.whl", hash = "sha256:eb8178fe3dba6450a3e024e95ac49ed3400e506fd4e9e5c32d30adda88cbd407", size = 95391 },
            { url = "https://files.pythonhosted.org/packages/27/f2/4f9a69cc7712b9b5ad8fdb87039fd89abba997ad5cbe690d1835d40405b0/charset_normalizer-3.4.1-cp313-cp313-win_amd64.whl", hash = "sha256:b1ac5992a838106edb89654e0aebfc24f5848ae2547d22c2c3f66454daa11971", size = 102702 },
            { url = "https://files.pythonhosted.org/packages/0e/f6/65ecc6878a89bb1c23a086ea335ad4bf21a588990c3f535a227b9eea9108/charset_normalizer-3.4.1-py3-none-any.whl", hash = "sha256:d98b1668f06378c6dbefec3b92299716b931cd4e6061f3c875a71ced1780ab85", size = 49767 },
        ]

        [[package]]
        name = "chgnet"
        version = "0.4.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "ase" },
            { name = "cython" },
            { name = "numpy" },
            { name = "nvidia-ml-py3" },
            { name = "pymatgen" },
            { name = "torch", version = "2.5.1", source = { registry = "https://pypi.org/simple" } },
            { name = "typing-extensions" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/f6/05/1aaaecfaf9567dc93f33cd94a6deda0cc8ca0e665484636d8522978658d6/chgnet-0.4.0.tar.gz", hash = "sha256:1683fd61dbbf2ccb6540b6967589f25b239951b94c4b616f1dbbc3bce8313785", size = 8756128 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/47/b9/03da4ead85cc9538d9d4db77f8d2e1919f2ec78966987a73a6f617120865/chgnet-0.4.0-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:e0c6f8bac1a0b3b469264dd9d2813b0ec69bff9c610955069612a612122999eb", size = 8916943 },
            { url = "https://files.pythonhosted.org/packages/5f/08/4e26d7ab59020ae5be8b7b70154e08c68205364832b7932949b432de8c79/chgnet-0.4.0-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:054d548be08ae57194641976ac64163f71f6d958494f98da274ee9576859d59c", size = 9237970 },
            { url = "https://files.pythonhosted.org/packages/b3/95/0438d9c72bccb394e33cdb9a8cff189e3753cea1b185c8ba579e7c50be4c/chgnet-0.4.0-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:cac302cfc70ce3d02ce17ebd1eed570306283ab1b1824cb98e5cdeb8917f99b2", size = 9224255 },
            { url = "https://files.pythonhosted.org/packages/01/30/378a7002c4923b2f7da8f79d83cb889ba819999457aaaffaa60c1c70f8e1/chgnet-0.4.0-cp310-cp310-musllinux_1_2_i686.whl", hash = "sha256:37a0459b50161fd7b2eab385daea48e2a8a562d85096c25d0d0ac2915c2b7f88", size = 9251421 },
            { url = "https://files.pythonhosted.org/packages/6a/d3/9db7f1f68a553f8d77753e901ae6b97c4d71b2513bc8ce89bff384e8e5a1/chgnet-0.4.0-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:37879736c55b13c88614f4c2e773d04e2d278a16348ebd9d962a6df9928ad265", size = 9258558 },
            { url = "https://files.pythonhosted.org/packages/b6/40/a6df6380a2721e3c1094a445960ae2fb12f9b64b7390742a6447600e2d0b/chgnet-0.4.0-cp310-cp310-win32.whl", hash = "sha256:8b464ee9ba49a6abb8b3789fa7549a2e67bbabc257e66dbe922723c91e05f8b9", size = 8814179 },
            { url = "https://files.pythonhosted.org/packages/42/07/d2c5212d10e9fcac6e2b3802b72ab1512fb73664dc859f42ebcfd8103d8b/chgnet-0.4.0-cp310-cp310-win_amd64.whl", hash = "sha256:c355bca4c7ba3895df048d75f75b7380a07470e394237203dc7ecdb6c7cf4bc2", size = 8824666 },
            { url = "https://files.pythonhosted.org/packages/09/29/cd527fc388ceed95cb9b785303721dfe136ea2f0d438a3d24aa26db6a47a/chgnet-0.4.0-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:95026262d4ad7a27ff996cf8bd7e53bfe98beacad8cb24e5a35d09cd7a507763", size = 8916734 },
            { url = "https://files.pythonhosted.org/packages/19/95/72eced00dc2535ad60f825469fb2bd3a3ed8671fa492eae0b0c158e14dea/chgnet-0.4.0-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:bc1b03d4d392775afc69745924317e114d6a4c5f3a7ac221f17092fe0cdaaaf7", size = 9275918 },
            { url = "https://files.pythonhosted.org/packages/41/8c/dd5ee202939bd3e088d2beb4313da21b05356ebb23a9f25ed5b85b246c0f/chgnet-0.4.0-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:aaa50d5d51ced9d9b4e3144fa441c441f3245284d1e0ec1f3577472436316c11", size = 9263959 },
            { url = "https://files.pythonhosted.org/packages/ca/68/d8d87e673a9de9182db5b5f0fbca70c8b021aa07a0077c534e185e873b39/chgnet-0.4.0-cp311-cp311-musllinux_1_2_i686.whl", hash = "sha256:4594772e9c87f0c4c75941b2b4fbe992ae11bb30e28264e6b93fd614191fd761", size = 9288676 },
            { url = "https://files.pythonhosted.org/packages/68/2c/4e616a4d9daa38c6a51e135008368bbba9651ca682c1673963e061e12e46/chgnet-0.4.0-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:b8f58d07dca2cca1ec3d2759030fcdfc04a3f5ea48a5f39d1bb4f98d9f7d32ae", size = 9300042 },
            { url = "https://files.pythonhosted.org/packages/03/1a/25a165d2796cf2dd3f6b3ded13aca3bdd978d707712fe15c1d7a7caf32ff/chgnet-0.4.0-cp311-cp311-win32.whl", hash = "sha256:dab2390fd8046910bac5e8726d921afa696fa40eb477550c990d157a6ce59e3d", size = 8813706 },
            { url = "https://files.pythonhosted.org/packages/23/6c/6bf740b42ddeaaa89dff870a72665c537051869e619cbed60aa76a2767c8/chgnet-0.4.0-cp311-cp311-win_amd64.whl", hash = "sha256:25b8230e3df68c3b1c77810588e2630f563f36eee81979a9eea110adb92ed3a6", size = 8824808 },
            { url = "https://files.pythonhosted.org/packages/ca/59/307a31cf4786a3baca7cabb311766a0a3ec15295ef32c2a724d89a0d9980/chgnet-0.4.0-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:7458b55c6444850ab1713cea42ee371a418898f5df13d51b57dd47183616d3fa", size = 8918290 },
            { url = "https://files.pythonhosted.org/packages/0d/34/c1cdb90b1bd5a884c154eb4195e23771be959f27bdcad0414a5e4e837f41/chgnet-0.4.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:8b26ca70451489c7e392b9c294b11517ae07c679fd3308548ed2a8b5ad1c9a11", size = 9265134 },
            { url = "https://files.pythonhosted.org/packages/ae/b9/e7d02f291e9e2f47988c409197bf382e9d1227f50c9d88a0ddf42117925f/chgnet-0.4.0-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:8d57b1d6c29e31bd4d840158cbc14fd73f37e8f33f91e45b35c257c48168db6e", size = 9243149 },
            { url = "https://files.pythonhosted.org/packages/44/19/c4b424b71c1662abecff0bee049186a6cfc15906b188e492a8b5d3bc49b9/chgnet-0.4.0-cp312-cp312-musllinux_1_2_i686.whl", hash = "sha256:dd0b927ddee05faac59251fb23fd58ed78e846c0f52fd2f642b59a58021ada0c", size = 9269138 },
            { url = "https://files.pythonhosted.org/packages/25/01/63594c4b8ecf68fd66604cc530d46c55f85bf89e4f8822636c0401192e8e/chgnet-0.4.0-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:235ac6fbf565bdb9c1a54c5f06625977a45797ce0d15cc4bcf13dcd52673ce1f", size = 9287335 },
            { url = "https://files.pythonhosted.org/packages/93/6a/366e4dbcd0a140635b3a889dd7517f9dff73948fbb7879bd5a3f65997e65/chgnet-0.4.0-cp312-cp312-win32.whl", hash = "sha256:5ca5e2f4432a789443585545b726e6b0df861942e6e909e3b14f29c31837d3a8", size = 8813957 },
            { url = "https://files.pythonhosted.org/packages/ea/d2/79e86ded321c762b37ac21d9ef12ccf109a11f7be5c5f6d1bf0cacbc35ba/chgnet-0.4.0-cp312-cp312-win_amd64.whl", hash = "sha256:c3e9de894d86f46cff1d66a089987d64dfc20ded59e3f811f7bb3ecf9e5e123d", size = 8824976 },
        ]

        [[package]]
        name = "colorama"
        version = "0.4.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 },
        ]

        [[package]]
        name = "configargparse"
        version = "1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/70/8a/73f1008adfad01cb923255b924b1528727b8270e67cb4ef41eabdc7d783e/ConfigArgParse-1.7.tar.gz", hash = "sha256:e7067471884de5478c58a511e529f0f9bd1c66bfef1dea90935438d6c23306d1", size = 43817 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/6f/b3/b4ac838711fd74a2b4e6f746703cf9dd2cf5462d17dac07e349234e21b97/ConfigArgParse-1.7-py3-none-any.whl", hash = "sha256:d249da6591465c6c26df64a9f73d2536e743be2f244eb3ebe61114af2f94f86b", size = 25489 },
        ]

        [[package]]
        name = "contourpy"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "numpy" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/25/c2/fc7193cc5383637ff390a712e88e4ded0452c9fbcf84abe3de5ea3df1866/contourpy-1.3.1.tar.gz", hash = "sha256:dfd97abd83335045a913e3bcc4a09c0ceadbe66580cf573fe961f4a825efa699", size = 13465753 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b2/a3/80937fe3efe0edacf67c9a20b955139a1a622730042c1ea991956f2704ad/contourpy-1.3.1-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:a045f341a77b77e1c5de31e74e966537bba9f3c4099b35bf4c2e3939dd54cdab", size = 268466 },
            { url = "https://files.pythonhosted.org/packages/82/1d/e3eaebb4aa2d7311528c048350ca8e99cdacfafd99da87bc0a5f8d81f2c2/contourpy-1.3.1-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:500360b77259914f7805af7462e41f9cb7ca92ad38e9f94d6c8641b089338124", size = 253314 },
            { url = "https://files.pythonhosted.org/packages/de/f3/d796b22d1a2b587acc8100ba8c07fb7b5e17fde265a7bb05ab967f4c935a/contourpy-1.3.1-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:b2f926efda994cdf3c8d3fdb40b9962f86edbc4457e739277b961eced3d0b4c1", size = 312003 },
            { url = "https://files.pythonhosted.org/packages/bf/f5/0e67902bc4394daee8daa39c81d4f00b50e063ee1a46cb3938cc65585d36/contourpy-1.3.1-cp310-cp310-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:adce39d67c0edf383647a3a007de0a45fd1b08dedaa5318404f1a73059c2512b", size = 351896 },
            { url = "https://files.pythonhosted.org/packages/1f/d6/e766395723f6256d45d6e67c13bb638dd1fa9dc10ef912dc7dd3dcfc19de/contourpy-1.3.1-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:abbb49fb7dac584e5abc6636b7b2a7227111c4f771005853e7d25176daaf8453", size = 320814 },
            { url = "https://files.pythonhosted.org/packages/a9/57/86c500d63b3e26e5b73a28b8291a67c5608d4aa87ebd17bd15bb33c178bc/contourpy-1.3.1-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a0cffcbede75c059f535725c1680dfb17b6ba8753f0c74b14e6a9c68c29d7ea3", size = 324969 },
            { url = "https://files.pythonhosted.org/packages/b8/62/bb146d1289d6b3450bccc4642e7f4413b92ebffd9bf2e91b0404323704a7/contourpy-1.3.1-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:ab29962927945d89d9b293eabd0d59aea28d887d4f3be6c22deaefbb938a7277", size = 1265162 },
            { url = "https://files.pythonhosted.org/packages/18/04/9f7d132ce49a212c8e767042cc80ae390f728060d2eea47058f55b9eff1c/contourpy-1.3.1-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:974d8145f8ca354498005b5b981165b74a195abfae9a8129df3e56771961d595", size = 1324328 },
            { url = "https://files.pythonhosted.org/packages/46/23/196813901be3f97c83ababdab1382e13e0edc0bb4e7b49a7bff15fcf754e/contourpy-1.3.1-cp310-cp310-win32.whl", hash = "sha256:ac4578ac281983f63b400f7fe6c101bedc10651650eef012be1ccffcbacf3697", size = 173861 },
            { url = "https://files.pythonhosted.org/packages/e0/82/c372be3fc000a3b2005061ca623a0d1ecd2eaafb10d9e883a2fc8566e951/contourpy-1.3.1-cp310-cp310-win_amd64.whl", hash = "sha256:174e758c66bbc1c8576992cec9599ce8b6672b741b5d336b5c74e35ac382b18e", size = 218566 },
            { url = "https://files.pythonhosted.org/packages/12/bb/11250d2906ee2e8b466b5f93e6b19d525f3e0254ac8b445b56e618527718/contourpy-1.3.1-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:3e8b974d8db2c5610fb4e76307e265de0edb655ae8169e8b21f41807ccbeec4b", size = 269555 },
            { url = "https://files.pythonhosted.org/packages/67/71/1e6e95aee21a500415f5d2dbf037bf4567529b6a4e986594d7026ec5ae90/contourpy-1.3.1-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:20914c8c973f41456337652a6eeca26d2148aa96dd7ac323b74516988bea89fc", size = 254549 },
            { url = "https://files.pythonhosted.org/packages/31/2c/b88986e8d79ac45efe9d8801ae341525f38e087449b6c2f2e6050468a42c/contourpy-1.3.1-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:19d40d37c1c3a4961b4619dd9d77b12124a453cc3d02bb31a07d58ef684d3d86", size = 313000 },
            { url = "https://files.pythonhosted.org/packages/c4/18/65280989b151fcf33a8352f992eff71e61b968bef7432fbfde3a364f0730/contourpy-1.3.1-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:113231fe3825ebf6f15eaa8bc1f5b0ddc19d42b733345eae0934cb291beb88b6", size = 352925 },
            { url = "https://files.pythonhosted.org/packages/f5/c7/5fd0146c93220dbfe1a2e0f98969293b86ca9bc041d6c90c0e065f4619ad/contourpy-1.3.1-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:4dbbc03a40f916a8420e420d63e96a1258d3d1b58cbdfd8d1f07b49fcbd38e85", size = 323693 },
            { url = "https://files.pythonhosted.org/packages/85/fc/7fa5d17daf77306840a4e84668a48ddff09e6bc09ba4e37e85ffc8e4faa3/contourpy-1.3.1-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:3a04ecd68acbd77fa2d39723ceca4c3197cb2969633836ced1bea14e219d077c", size = 326184 },
            { url = "https://files.pythonhosted.org/packages/ef/e7/104065c8270c7397c9571620d3ab880558957216f2b5ebb7e040f85eeb22/contourpy-1.3.1-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:c414fc1ed8ee1dbd5da626cf3710c6013d3d27456651d156711fa24f24bd1291", size = 1268031 },
            { url = "https://files.pythonhosted.org/packages/e2/4a/c788d0bdbf32c8113c2354493ed291f924d4793c4a2e85b69e737a21a658/contourpy-1.3.1-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:31c1b55c1f34f80557d3830d3dd93ba722ce7e33a0b472cba0ec3b6535684d8f", size = 1325995 },
            { url = "https://files.pythonhosted.org/packages/a6/e6/a2f351a90d955f8b0564caf1ebe4b1451a3f01f83e5e3a414055a5b8bccb/contourpy-1.3.1-cp311-cp311-win32.whl", hash = "sha256:f611e628ef06670df83fce17805c344710ca5cde01edfdc72751311da8585375", size = 174396 },
            { url = "https://files.pythonhosted.org/packages/a8/7e/cd93cab453720a5d6cb75588cc17dcdc08fc3484b9de98b885924ff61900/contourpy-1.3.1-cp311-cp311-win_amd64.whl", hash = "sha256:b2bdca22a27e35f16794cf585832e542123296b4687f9fd96822db6bae17bfc9", size = 219787 },
            { url = "https://files.pythonhosted.org/packages/37/6b/175f60227d3e7f5f1549fcb374592be311293132207e451c3d7c654c25fb/contourpy-1.3.1-cp312-cp312-macosx_10_13_x86_64.whl", hash = "sha256:0ffa84be8e0bd33410b17189f7164c3589c229ce5db85798076a3fa136d0e509", size = 271494 },
            { url = "https://files.pythonhosted.org/packages/6b/6a/7833cfae2c1e63d1d8875a50fd23371394f540ce809d7383550681a1fa64/contourpy-1.3.1-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:805617228ba7e2cbbfb6c503858e626ab528ac2a32a04a2fe88ffaf6b02c32bc", size = 255444 },
            { url = "https://files.pythonhosted.org/packages/7f/b3/7859efce66eaca5c14ba7619791b084ed02d868d76b928ff56890d2d059d/contourpy-1.3.1-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ade08d343436a94e633db932e7e8407fe7de8083967962b46bdfc1b0ced39454", size = 307628 },
            { url = "https://files.pythonhosted.org/packages/48/b2/011415f5e3f0a50b1e285a0bf78eb5d92a4df000553570f0851b6e309076/contourpy-1.3.1-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:47734d7073fb4590b4a40122b35917cd77be5722d80683b249dac1de266aac80", size = 347271 },
            { url = "https://files.pythonhosted.org/packages/84/7d/ef19b1db0f45b151ac78c65127235239a8cf21a59d1ce8507ce03e89a30b/contourpy-1.3.1-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:2ba94a401342fc0f8b948e57d977557fbf4d515f03c67682dd5c6191cb2d16ec", size = 318906 },
            { url = "https://files.pythonhosted.org/packages/ba/99/6794142b90b853a9155316c8f470d2e4821fe6f086b03e372aca848227dd/contourpy-1.3.1-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:efa874e87e4a647fd2e4f514d5e91c7d493697127beb95e77d2f7561f6905bd9", size = 323622 },
            { url = "https://files.pythonhosted.org/packages/3c/0f/37d2c84a900cd8eb54e105f4fa9aebd275e14e266736778bb5dccbf3bbbb/contourpy-1.3.1-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:1bf98051f1045b15c87868dbaea84f92408337d4f81d0e449ee41920ea121d3b", size = 1266699 },
            { url = "https://files.pythonhosted.org/packages/3a/8a/deb5e11dc7d9cc8f0f9c8b29d4f062203f3af230ba83c30a6b161a6effc9/contourpy-1.3.1-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:61332c87493b00091423e747ea78200659dc09bdf7fd69edd5e98cef5d3e9a8d", size = 1326395 },
            { url = "https://files.pythonhosted.org/packages/1a/35/7e267ae7c13aaf12322ccc493531f1e7f2eb8fba2927b9d7a05ff615df7a/contourpy-1.3.1-cp312-cp312-win32.whl", hash = "sha256:e914a8cb05ce5c809dd0fe350cfbb4e881bde5e2a38dc04e3afe1b3e58bd158e", size = 175354 },
            { url = "https://files.pythonhosted.org/packages/a1/35/c2de8823211d07e8a79ab018ef03960716c5dff6f4d5bff5af87fd682992/contourpy-1.3.1-cp312-cp312-win_amd64.whl", hash = "sha256:08d9d449a61cf53033612cb368f3a1b26cd7835d9b8cd326647efe43bca7568d", size = 220971 },
            { url = "https://files.pythonhosted.org/packages/9a/e7/de62050dce687c5e96f946a93546910bc67e483fe05324439e329ff36105/contourpy-1.3.1-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:a761d9ccfc5e2ecd1bf05534eda382aa14c3e4f9205ba5b1684ecfe400716ef2", size = 271548 },
            { url = "https://files.pythonhosted.org/packages/78/4d/c2a09ae014ae984c6bdd29c11e74d3121b25eaa117eca0bb76340efd7e1c/contourpy-1.3.1-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:523a8ee12edfa36f6d2a49407f705a6ef4c5098de4f498619787e272de93f2d5", size = 255576 },
            { url = "https://files.pythonhosted.org/packages/ab/8a/915380ee96a5638bda80cd061ccb8e666bfdccea38d5741cb69e6dbd61fc/contourpy-1.3.1-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ece6df05e2c41bd46776fbc712e0996f7c94e0d0543af1656956d150c4ca7c81", size = 306635 },
            { url = "https://files.pythonhosted.org/packages/29/5c/c83ce09375428298acd4e6582aeb68b1e0d1447f877fa993d9bf6cd3b0a0/contourpy-1.3.1-cp313-cp313-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:573abb30e0e05bf31ed067d2f82500ecfdaec15627a59d63ea2d95714790f5c2", size = 345925 },
            { url = "https://files.pythonhosted.org/packages/29/63/5b52f4a15e80c66c8078a641a3bfacd6e07106835682454647aca1afc852/contourpy-1.3.1-cp313-cp313-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:a9fa36448e6a3a1a9a2ba23c02012c43ed88905ec80163f2ffe2421c7192a5d7", size = 318000 },
            { url = "https://files.pythonhosted.org/packages/9a/e2/30ca086c692691129849198659bf0556d72a757fe2769eb9620a27169296/contourpy-1.3.1-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:3ea9924d28fc5586bf0b42d15f590b10c224117e74409dd7a0be3b62b74a501c", size = 322689 },
            { url = "https://files.pythonhosted.org/packages/6b/77/f37812ef700f1f185d348394debf33f22d531e714cf6a35d13d68a7003c7/contourpy-1.3.1-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:5b75aa69cb4d6f137b36f7eb2ace9280cfb60c55dc5f61c731fdf6f037f958a3", size = 1268413 },
            { url = "https://files.pythonhosted.org/packages/3f/6d/ce84e79cdd128542ebeb268f84abb4b093af78e7f8ec504676673d2675bc/contourpy-1.3.1-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:041b640d4ec01922083645a94bb3b2e777e6b626788f4095cf21abbe266413c1", size = 1326530 },
            { url = "https://files.pythonhosted.org/packages/72/22/8282f4eae20c73c89bee7a82a19c4e27af9b57bb602ecaa00713d5bdb54d/contourpy-1.3.1-cp313-cp313-win32.whl", hash = "sha256:36987a15e8ace5f58d4d5da9dca82d498c2bbb28dff6e5d04fbfcc35a9cb3a82", size = 175315 },
            { url = "https://files.pythonhosted.org/packages/e3/d5/28bca491f65312b438fbf076589dcde7f6f966b196d900777f5811b9c4e2/contourpy-1.3.1-cp313-cp313-win_amd64.whl", hash = "sha256:a7895f46d47671fa7ceec40f31fae721da51ad34bdca0bee83e38870b1f47ffd", size = 220987 },
            { url = "https://files.pythonhosted.org/packages/2f/24/a4b285d6adaaf9746e4700932f579f1a7b6f9681109f694cfa233ae75c4e/contourpy-1.3.1-cp313-cp313t-macosx_10_13_x86_64.whl", hash = "sha256:9ddeb796389dadcd884c7eb07bd14ef12408aaae358f0e2ae24114d797eede30", size = 285001 },
            { url = "https://files.pythonhosted.org/packages/48/1d/fb49a401b5ca4f06ccf467cd6c4f1fd65767e63c21322b29b04ec40b40b9/contourpy-1.3.1-cp313-cp313t-macosx_11_0_arm64.whl", hash = "sha256:19c1555a6801c2f084c7ddc1c6e11f02eb6a6016ca1318dd5452ba3f613a1751", size = 268553 },
            { url = "https://files.pythonhosted.org/packages/79/1e/4aef9470d13fd029087388fae750dccb49a50c012a6c8d1d634295caa644/contourpy-1.3.1-cp313-cp313t-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:841ad858cff65c2c04bf93875e384ccb82b654574a6d7f30453a04f04af71342", size = 310386 },
            { url = "https://files.pythonhosted.org/packages/b0/34/910dc706ed70153b60392b5305c708c9810d425bde12499c9184a1100888/contourpy-1.3.1-cp313-cp313t-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:4318af1c925fb9a4fb190559ef3eec206845f63e80fb603d47f2d6d67683901c", size = 349806 },
            { url = "https://files.pythonhosted.org/packages/31/3c/faee6a40d66d7f2a87f7102236bf4780c57990dd7f98e5ff29881b1b1344/contourpy-1.3.1-cp313-cp313t-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:14c102b0eab282427b662cb590f2e9340a9d91a1c297f48729431f2dcd16e14f", size = 321108 },
            { url = "https://files.pythonhosted.org/packages/17/69/390dc9b20dd4bb20585651d7316cc3054b7d4a7b4f8b710b2b698e08968d/contourpy-1.3.1-cp313-cp313t-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:05e806338bfeaa006acbdeba0ad681a10be63b26e1b17317bfac3c5d98f36cda", size = 327291 },
            { url = "https://files.pythonhosted.org/packages/ef/74/7030b67c4e941fe1e5424a3d988080e83568030ce0355f7c9fc556455b01/contourpy-1.3.1-cp313-cp313t-musllinux_1_2_aarch64.whl", hash = "sha256:4d76d5993a34ef3df5181ba3c92fabb93f1eaa5729504fb03423fcd9f3177242", size = 1263752 },
            { url = "https://files.pythonhosted.org/packages/f0/ed/92d86f183a8615f13f6b9cbfc5d4298a509d6ce433432e21da838b4b63f4/contourpy-1.3.1-cp313-cp313t-musllinux_1_2_x86_64.whl", hash = "sha256:89785bb2a1980c1bd87f0cb1517a71cde374776a5f150936b82580ae6ead44a1", size = 1318403 },
            { url = "https://files.pythonhosted.org/packages/b3/0e/c8e4950c77dcfc897c71d61e56690a0a9df39543d2164040301b5df8e67b/contourpy-1.3.1-cp313-cp313t-win32.whl", hash = "sha256:8eb96e79b9f3dcadbad2a3891672f81cdcab7f95b27f28f1c67d75f045b6b4f1", size = 185117 },
            { url = "https://files.pythonhosted.org/packages/c1/31/1ae946f11dfbd229222e6d6ad8e7bd1891d3d48bde5fbf7a0beb9491f8e3/contourpy-1.3.1-cp313-cp313t-win_amd64.whl", hash = "sha256:287ccc248c9e0d0566934e7d606201abd74761b5703d804ff3df8935f523d546", size = 236668 },
            { url = "https://files.pythonhosted.org/packages/3e/4f/e56862e64b52b55b5ddcff4090085521fc228ceb09a88390a2b103dccd1b/contourpy-1.3.1-pp310-pypy310_pp73-macosx_10_15_x86_64.whl", hash = "sha256:b457d6430833cee8e4b8e9b6f07aa1c161e5e0d52e118dc102c8f9bd7dd060d6", size = 265605 },
            { url = "https://files.pythonhosted.org/packages/b0/2e/52bfeeaa4541889f23d8eadc6386b442ee2470bd3cff9baa67deb2dd5c57/contourpy-1.3.1-pp310-pypy310_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:cb76c1a154b83991a3cbbf0dfeb26ec2833ad56f95540b442c73950af2013750", size = 315040 },
            { url = "https://files.pythonhosted.org/packages/52/94/86bfae441707205634d80392e873295652fc313dfd93c233c52c4dc07874/contourpy-1.3.1-pp310-pypy310_pp73-win_amd64.whl", hash = "sha256:44a29502ca9c7b5ba389e620d44f2fbe792b1fb5734e8b931ad307071ec58c53", size = 218221 },
        ]

        [[package]]
        name = "cuequivariance"
        version = "0.2.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "networkx" },
            { name = "numpy" },
            { name = "opt-einsum" },
            { name = "scipy" },
            { name = "sympy", version = "1.13.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
            { name = "sympy", version = "1.13.3", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/70/91/c37b2ad1c8534b183eeacb97d2096f50cd2d0a180d1830f235c06aa94bbd/cuequivariance-0.2.0.tar.gz", hash = "sha256:e75999ef436f281e3f5cded9f1353055a599fedae55eea861bdc840cffaf63eb", size = 76738 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/27/73/6b42a0c6e1e0cbdace8ca2c302a8f1bfad421e654e8fdf808aebab1e4319/cuequivariance-0.2.0-py3-none-any.whl", hash = "sha256:c19cc0f7e396f42ca9865c930b72d8a5f0f0d79d06310207e3f1c75bdfde6092", size = 109844 },
        ]

        [[package]]
        name = "cuequivariance-torch"
        version = "0.2.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "cuequivariance" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/04/95/d914c65769724e1195658dd70b1ebee7a313f92af5fc8c57f8c64745cc74/cuequivariance_torch-0.2.0.tar.gz", hash = "sha256:9730e6faca39a04f2faa886c81e68588919b748763bbc91e1eabbefcb4ef62d2", size = 38295 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/82/3b/d39715dc9be994b25226edaceb7838ce16c14155e7dc8f49dbed7dafc5f5/cuequivariance_torch-0.2.0-py3-none-any.whl", hash = "sha256:fd4ac2b503ecb5feb7848ab0968233ebbc32283b590c01b0b560cd539fa6f313", size = 45092 },
        ]

        [[package]]
        name = "cycler"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a9/95/a3dbbb5028f35eafb79008e7522a75244477d2838f38cbb722248dabc2a8/cycler-0.12.1.tar.gz", hash = "sha256:88bb128f02ba341da8ef447245a9e138fae777f6a23943da4540077d3601eb1c", size = 7615 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e7/05/c19819d5e3d95294a6f5947fb9b9629efb316b96de511b418c53d245aae6/cycler-0.12.1-py3-none-any.whl", hash = "sha256:85cef7cff222d8644161529808465972e51340599459b8ac3ccbac5a854e0d30", size = 8321 },
        ]

        [[package]]
        name = "cython"
        version = "3.0.11"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/84/4d/b720d6000f4ca77f030bd70f12550820f0766b568e43f11af7f7ad9061aa/cython-3.0.11.tar.gz", hash = "sha256:7146dd2af8682b4ca61331851e6aebce9fe5158e75300343f80c07ca80b1faff", size = 2755544 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/13/7f/ab5796a0951328d7818b771c36fe7e1a2077cffa28c917d9fa4a642728c3/Cython-3.0.11-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:44292aae17524abb4b70a25111fe7dec1a0ad718711d47e3786a211d5408fdaa", size = 3100879 },
            { url = "https://files.pythonhosted.org/packages/d8/3b/67480e609537e9fc899864847910ded481b82d033fea1b7fcf85893a2fc4/Cython-3.0.11-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:a75d45fbc20651c1b72e4111149fed3b33d270b0a4fb78328c54d965f28d55e1", size = 3461957 },
            { url = "https://files.pythonhosted.org/packages/f0/89/b1ae45689abecca777f95462781a76e67ff46b55495a481ec5a73a739994/Cython-3.0.11-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:d89a82937ce4037f092e9848a7bbcc65bc8e9fc9aef2bb74f5c15e7d21a73080", size = 3627062 },
            { url = "https://files.pythonhosted.org/packages/44/77/a651da74d5d41c6045bbe0b6990b1515bf4850cd7a8d8580333c90dfce2e/Cython-3.0.11-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:2a8ea2e7e2d3bc0d8630dafe6c4a5a89485598ff8a61885b74f8ed882597efd5", size = 3680431 },
            { url = "https://files.pythonhosted.org/packages/59/45/60e7e8db93c3eb8b2af8c64020c1fa502e355f4b762886a24d46e433f395/Cython-3.0.11-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:cee29846471ce60226b18e931d8c1c66a158db94853e3e79bc2da9bd22345008", size = 3497314 },
            { url = "https://files.pythonhosted.org/packages/f8/0b/6919025958926625319f83523ee7f45e7e7ae516b8054dcff6eb710daf32/Cython-3.0.11-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:eeb6860b0f4bfa402de8929833fe5370fa34069c7ebacb2d543cb017f21fb891", size = 3709091 },
            { url = "https://files.pythonhosted.org/packages/52/3c/c21b9b9271dfaa46fa2938de730f62fc94b9c2ec25ec400585e372f35dcd/Cython-3.0.11-cp310-cp310-win32.whl", hash = "sha256:3699391125ab344d8d25438074d1097d9ba0fb674d0320599316cfe7cf5f002a", size = 2576110 },
            { url = "https://files.pythonhosted.org/packages/f9/de/19fdd1c7a52e0534bf5f544e0346c15d71d20338dbd013117f763b94613f/Cython-3.0.11-cp310-cp310-win_amd64.whl", hash = "sha256:d02f4ebe15aac7cdacce1a628e556c1983f26d140fd2e0ac5e0a090e605a2d38", size = 2776386 },
            { url = "https://files.pythonhosted.org/packages/f8/73/e55be864199cd674cb3426a052726c205589b1ac66fb0090e7fe793b60b3/Cython-3.0.11-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:75ba1c70b6deeaffbac123856b8d35f253da13552207aa969078611c197377e4", size = 3113599 },
            { url = "https://files.pythonhosted.org/packages/09/c9/537108d0980beffff55336baaf8b34162ad0f3f33ededcb5db07069bc8ef/Cython-3.0.11-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:af91497dc098718e634d6ec8f91b182aea6bb3690f333fc9a7777bc70abe8810", size = 3441131 },
            { url = "https://files.pythonhosted.org/packages/93/03/e330b241ad8aa12bb9d98b58fb76d4eb7dcbe747479aab5c29fce937b9e7/Cython-3.0.11-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:3999fb52d3328a6a5e8c63122b0a8bd110dfcdb98dda585a3def1426b991cba7", size = 3595065 },
            { url = "https://files.pythonhosted.org/packages/4a/84/a3c40f2c0439d425daa5aa4e3a6fdbbb41341a14a6fd97f94906f528d9a4/Cython-3.0.11-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:d566a4e09b8979be8ab9f843bac0dd216c81f5e5f45661a9b25cd162ed80508c", size = 3641667 },
            { url = "https://files.pythonhosted.org/packages/6d/93/bdb61e0254ed8f1d21a14088a473584ecb1963d68dba5682158aa45c70ef/Cython-3.0.11-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:46aec30f217bdf096175a1a639203d44ac73a36fe7fa3dd06bd012e8f39eca0f", size = 3503650 },
            { url = "https://files.pythonhosted.org/packages/f8/62/0da548144c71176155ff5355c4cc40fb28b9effe22e830b55cec8072bdf2/Cython-3.0.11-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:ddd1fe25af330f4e003421636746a546474e4ccd8f239f55d2898d80983d20ed", size = 3709662 },
            { url = "https://files.pythonhosted.org/packages/56/d3/d9c9eaf3611a9fe5256266d07b6a5f9069aa84d20d9f6aa5824289513315/Cython-3.0.11-cp311-cp311-win32.whl", hash = "sha256:221de0b48bf387f209003508e602ce839a80463522fc6f583ad3c8d5c890d2c1", size = 2577870 },
            { url = "https://files.pythonhosted.org/packages/fd/10/236fcc0306f85a2db1b8bc147aea714b66a2f27bac4d9e09e5b2c5d5dcca/Cython-3.0.11-cp311-cp311-win_amd64.whl", hash = "sha256:3ff8ac1f0ecd4f505db4ab051e58e4531f5d098b6ac03b91c3b902e8d10c67b3", size = 2785053 },
            { url = "https://files.pythonhosted.org/packages/58/50/fbb23239efe2183e4eaf76689270d6f5b3bbcf9be9ad1eb97cc34349e6fc/Cython-3.0.11-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:11996c40c32abf843ba652a6d53cb15944c88d91f91fc4e6f0028f5df8a8f8a1", size = 3141274 },
            { url = "https://files.pythonhosted.org/packages/87/e5/76379edb21fd5bb9e2aaa1d305492bc35bba96dfb51f5d96867d9863b6df/Cython-3.0.11-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:63f2c892e9f9c1698ecfee78205541623eb31cd3a1b682668be7ac12de94aa8e", size = 3340904 },
            { url = "https://files.pythonhosted.org/packages/9a/ef/44af6aded89444dc45f4466ff207a05d3376c641cf1146c03fd14c55ae64/Cython-3.0.11-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:8b14c24f1dc4c4c9d997cca8d1b7fb01187a218aab932328247dcf5694a10102", size = 3514052 },
            { url = "https://files.pythonhosted.org/packages/e0/d5/ef8c7b6aa7a83c508f5c3bf0dfb9eb0a2a9be910c0b1f205f842128269c3/Cython-3.0.11-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:c8eed5c015685106db15dd103fd040948ddca9197b1dd02222711815ea782a27", size = 3573721 },
            { url = "https://files.pythonhosted.org/packages/e5/4a/58d6c208563504a35febff94904bb291b368a8b0f28a5e0593c770967caa/Cython-3.0.11-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:780f89c95b8aec1e403005b3bf2f0a2afa060b3eba168c86830f079339adad89", size = 3393594 },
            { url = "https://files.pythonhosted.org/packages/a0/92/a60a400be286dc661609da9db903680bba1423362000b689cf8ef0aec811/Cython-3.0.11-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:a690f2ff460682ea985e8d38ec541be97e0977fa0544aadc21efc116ff8d7579", size = 3601319 },
            { url = "https://files.pythonhosted.org/packages/ac/11/f02fc24d1a071b93e1d07497b0a528687b1f93bb4945c635119480fab3c0/Cython-3.0.11-cp312-cp312-win32.whl", hash = "sha256:2252b5aa57621848e310fe7fa6f7dce5f73aa452884a183d201a8bcebfa05a00", size = 2608335 },
            { url = "https://files.pythonhosted.org/packages/35/00/78ffea3a0ab176267a25ff049518b2582db7ac265bbf27944243d1a81ce2/Cython-3.0.11-cp312-cp312-win_amd64.whl", hash = "sha256:da394654c6da15c1d37f0b7ec5afd325c69a15ceafee2afba14b67a5df8a82c8", size = 2792586 },
            { url = "https://files.pythonhosted.org/packages/eb/19/1d7164b724f62b67c59aa3531a2be8ed1a0c7e4e80afcc6502d8409c4ee3/Cython-3.0.11-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:4341d6a64d47112884e0bcf31e6c075268220ee4cd02223047182d4dda94d637", size = 3134881 },
            { url = "https://files.pythonhosted.org/packages/0a/d7/8d834d7ec4b6e55db857f44e328246d40cb527917040fabf3c48d27609b3/Cython-3.0.11-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:351955559b37e6c98b48aecb178894c311be9d731b297782f2b78d111f0c9015", size = 3330582 },
            { url = "https://files.pythonhosted.org/packages/1c/ae/d520f3cd94a8926bc47275a968e51bbc669a28f27a058cdfc5c3081fbbf7/Cython-3.0.11-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:9c02361af9bfa10ff1ccf967fc75159e56b1c8093caf565739ed77a559c1f29f", size = 3503750 },
            { url = "https://files.pythonhosted.org/packages/6e/e4/45c556f4a6d40b6938368d420d3c985bbef9088b7d4a8d8c6648d50e4a94/Cython-3.0.11-cp313-cp313-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:6823aef13669a32caf18bbb036de56065c485d9f558551a9b55061acf9c4c27f", size = 3566498 },
            { url = "https://files.pythonhosted.org/packages/ce/2d/544f6aa3cab31b99ddb07e7eaaaca6a43db52fe0dc59090195c48fc0b033/Cython-3.0.11-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:6fb68cef33684f8cc97987bee6ae919eee7e18ee6a3ad7ed9516b8386ef95ae6", size = 3389063 },
            { url = "https://files.pythonhosted.org/packages/55/1a/9d871cc1514df273cd2ccfe3efe5ff1df509ce11768c02a834052709f152/Cython-3.0.11-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:790263b74432cb997740d73665f4d8d00b9cd1cecbdd981d93591ddf993d4f12", size = 3596353 },
            { url = "https://files.pythonhosted.org/packages/47/4e/4db412f595de4b2224a81ea5332ce107ce3e93bf87275c78648f2e3e37b8/Cython-3.0.11-cp313-cp313-win32.whl", hash = "sha256:e6dd395d1a704e34a9fac00b25f0036dce6654c6b898be6f872ac2bb4f2eda48", size = 2602768 },
            { url = "https://files.pythonhosted.org/packages/e7/91/8a29e1bce2f8a893a4c24874943b64e8ede14fac9990bd4a3f13a46c2720/Cython-3.0.11-cp313-cp313-win_amd64.whl", hash = "sha256:52186101d51497519e99b60d955fd5cb3bf747c67f00d742e70ab913f1e42d31", size = 2784414 },
            { url = "https://files.pythonhosted.org/packages/43/39/bdbec9142bc46605b54d674bf158a78b191c2b75be527c6dcf3e6dfe90b8/Cython-3.0.11-py2.py3-none-any.whl", hash = "sha256:0e25f6425ad4a700d7f77cd468da9161e63658837d1bc34861a9861a4ef6346d", size = 1171267 },
        ]

        [[package]]
        name = "dgl"
        version = "2.1.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "networkx" },
            { name = "numpy" },
            { name = "psutil" },
            { name = "requests" },
            { name = "scipy" },
            { name = "torchdata" },
            { name = "tqdm" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/09/2d/c0a40c0d78a8e9c0c43e7f6e4aebebb39c8bf0f80380db2c1f1ba9f3bdfa/dgl-2.1.0-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:ff74c7119cb1e87c1b06293eba8c22725594517062e74659b1a8636134b16974", size = 6889468 },
            { url = "https://files.pythonhosted.org/packages/89/19/08b77e50de7beb9c011905167ab38ecee759a796447ed7ad466484eafb53/dgl-2.1.0-cp310-cp310-macosx_11_0_x86_64.whl", hash = "sha256:1fda987e01987d655e9a2b16281e1f31759bd5088ce08785422055c71abc0298", size = 7464359 },
            { url = "https://files.pythonhosted.org/packages/ff/45/44fea6e5228602f5d6ad8b4f5a377f5cf5cb5cb6444d9e7cd99150f13fa2/dgl-2.1.0-cp310-cp310-manylinux1_x86_64.whl", hash = "sha256:336e5aed294b9f2f398be332caa0b256e6386e5a6a3a25dfe0715a161e6c4f94", size = 8548990 },
            { url = "https://files.pythonhosted.org/packages/73/72/6bc97419cf8fa1e11420c674fafa3c03e4df9d91bfd8fd802011b5741ce9/dgl-2.1.0-cp310-cp310-manylinux2014_aarch64.whl", hash = "sha256:65f5ae94c4c88433fc4524ab8c431494cfddd8100181d5de6df4ed08aa41d14d", size = 7430720 },
            { url = "https://files.pythonhosted.org/packages/bd/15/6571516b8bdd2f8db29e2739bca7afcd6cf75179b156f72f159a0986c9ed/dgl-2.1.0-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:fc7ea52fa9d42bf507ccf3b43b6d05c5ee3fce4c614033833ef08aa089b89bc6", size = 6872096 },
            { url = "https://files.pythonhosted.org/packages/7c/48/c62028815d8f1fadff4dc83bbb9bd2e5aa113835bbe14dad92b716d1fd05/dgl-2.1.0-cp311-cp311-macosx_11_0_x86_64.whl", hash = "sha256:994a7dd11d6da251438762e9fb6657ee2bd9715300aa1f11143a0d61be620ec5", size = 7331725 },
            { url = "https://files.pythonhosted.org/packages/15/0b/07c479b28b2b36cbc49f5fa78156fc77f918a4bf5927c9409947d2d3d68d/dgl-2.1.0-cp311-cp311-manylinux1_x86_64.whl", hash = "sha256:050af2b3d54f158be67ae222dbf786b2b35e5d5d6a29fbf2a81b854e99f13adf", size = 8561303 },
            { url = "https://files.pythonhosted.org/packages/0d/28/173f36cae067f17ddb45faa4ee91ca5e31efb194edba667761b53b2e1baf/dgl-2.1.0-cp311-cp311-manylinux2014_aarch64.whl", hash = "sha256:3b54764b7fae40d41eb936bbfe9f254b1b11ed4993d8c61be8e81d09d955f72a", size = 7443820 },
            { url = "https://files.pythonhosted.org/packages/22/62/6868d769988961d8bb482325c4a6ba3c0007d49ba96d11c79d8c33d016c9/dgl-2.1.0-cp312-cp312-macosx_11_0_x86_64.whl", hash = "sha256:c0abdc0bf4407f67d1df45b9acd5f8017d26ef13aebaaec973a35905cb70e4ef", size = 7331721 },
            { url = "https://files.pythonhosted.org/packages/24/5b/40e7802b5bfda663408841e3f4f5ccdb97ceefae8011f47be22b81c40275/dgl-2.1.0-cp312-cp312-macosx_12_0_arm64.whl", hash = "sha256:ffebbfbbc2278861601fa44b533e3b4a6f21bbfd742542932cb3af0c7621cffc", size = 6872090 },
            { url = "https://files.pythonhosted.org/packages/1a/78/84bc05dd88db7a15eb32798d538cb8fd5e4e4c6fa966a650844a43c7beb5/dgl-2.1.0-cp312-cp312-manylinux1_x86_64.whl", hash = "sha256:af9b5974d667d11b9b2dc153981241c248a314c1a54880d4a2af12a00470b78d", size = 8551552 },
            { url = "https://files.pythonhosted.org/packages/ae/d7/910035a1501d13b7fd7d90c5333ecc17ff7ebfa43195e2f856a0140d22a4/dgl-2.1.0-cp312-cp312-manylinux2014_aarch64.whl", hash = "sha256:0f8605418e71191f7a4b43a5a6597e655512c7a48171ac042993d77c676f3092", size = 7435020 },
        ]

        [[package]]
        name = "e3nn"
        version = "0.4.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "opt-einsum-fx" },
            { name = "scipy" },
            { name = "sympy", version = "1.13.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
            { name = "sympy", version = "1.13.3", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "torch", version = "2.2.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "torch", version = "2.5.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/aa/98/8e7102dea93106603383fda23bf96649c397a37b910e7c76086e584cd92d/e3nn-0.4.4.tar.gz", hash = "sha256:51c91a84c1fb72e7e3600000958fa8caad48f8270937090fb8d0f8bfffbb3525", size = 361661 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9a/b6/c8327065d1dd8bca28521fe65ff72b649ed17ca8a1f0c1f498006d7567b7/e3nn-0.4.4-py3-none-any.whl", hash = "sha256:87d99876abb362a6e07d555d10752ff2e20c2b7731d8928ebe5d9121fb019f84", size = 387707 },
        ]

        [[package]]
        name = "filelock"
        version = "3.17.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/dc/9c/0b15fb47b464e1b663b1acd1253a062aa5feecb07d4e597daea542ebd2b5/filelock-3.17.0.tar.gz", hash = "sha256:ee4e77401ef576ebb38cd7f13b9b28893194acc20a8e68e18730ba9c0e54660e", size = 18027 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/89/ec/00d68c4ddfedfe64159999e5f8a98fb8442729a63e2077eb9dcd89623d27/filelock-3.17.0-py3-none-any.whl", hash = "sha256:533dc2f7ba78dc2f0f531fc6c4940addf7b70a481e269a5a3b93be94ffbe8338", size = 16164 },
        ]

        [[package]]
        name = "fonttools"
        version = "4.55.8"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/f1/24/de7e40adc99be2aa5adc6321bbdf3cf58dbe751b87343da658dd3fc7d946/fonttools-4.55.8.tar.gz", hash = "sha256:54d481d456dcd59af25d4a9c56b2c4c3f20e9620b261b84144e5950f33e8df17", size = 3458915 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/54/b8/82b3444cb081798eabb8397452ddf73680e623d7fdf9c575594a2240b8a2/fonttools-4.55.8-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:d11600f5343092697d7434f3bf77a393c7ae74be206fe30e577b9a195fd53165", size = 2752288 },
            { url = "https://files.pythonhosted.org/packages/86/8f/9c5f2172e9f6dcf52bb6477bcd5a023d056114787c8184b683c34996f5a1/fonttools-4.55.8-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:c96f2506ce1a0beeaa9595f9a8b7446477eb133f40c0e41fc078744c28149f80", size = 2280718 },
            { url = "https://files.pythonhosted.org/packages/c6/a6/b7cd7b54412bb7a27e282ee54459cae24524ad0eab6f81ead2a91d435287/fonttools-4.55.8-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:9b5f05ef72e846e9f49ccdd74b9da4309901a4248434c63c1ee9321adcb51d65", size = 4562177 },
            { url = "https://files.pythonhosted.org/packages/0e/16/eff3be24cecb9336639148c40507f949c193642d8369352af480597633fb/fonttools-4.55.8-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:ba45b637da80a262b55b7657aec68da2ac54b8ae7891cd977a5dbe5fd26db429", size = 4604843 },
            { url = "https://files.pythonhosted.org/packages/b5/95/737574364439cbcc5e6d4f3e000f15432141680ca8cb5c216b619a3d1cab/fonttools-4.55.8-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:edcffaeadba9a334c1c3866e275d7dd495465e7dbd296f688901bdbd71758113", size = 4559127 },
            { url = "https://files.pythonhosted.org/packages/5f/07/ea90834742f9b3e51a05f0f15f7c817eb7aab3d6ebf4f06c4626825ccb89/fonttools-4.55.8-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:b9f9fce3c9b2196e162182ec5db8af8eb3acd0d76c2eafe9fdba5f370044e556", size = 4728575 },
            { url = "https://files.pythonhosted.org/packages/93/74/0c816d83cd2945a25aed592b0cb3c9ba32e8b259781bf41dc112204129d9/fonttools-4.55.8-cp310-cp310-win32.whl", hash = "sha256:f089e8da0990cfe2d67e81d9cf581ff372b48dc5acf2782701844211cd1f0eb3", size = 2155662 },
            { url = "https://files.pythonhosted.org/packages/78/bc/f5a24229edd8cdd7494f2099e1c62fca288dad4c8637ee62df04459db27e/fonttools-4.55.8-cp310-cp310-win_amd64.whl", hash = "sha256:01ea3901b0802fc5f9e854f5aeb5bc27770dd9dd24c28df8f74ba90f8b3f5915", size = 2200126 },
            { url = "https://files.pythonhosted.org/packages/0a/e3/834e0919b34b40a6a2895f533323231bba3b8f5ae22c19ab725b84cf84c0/fonttools-4.55.8-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:95f5a1d4432b3cea6571f5ce4f4e9b25bf36efbd61c32f4f90130a690925d6ee", size = 2753424 },
            { url = "https://files.pythonhosted.org/packages/b6/f9/9cf7fc04da85d37cfa1c287f0a25c274d6940dad259dbaa9fd796b87bd3c/fonttools-4.55.8-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:3d20f152de7625a0008ba1513f126daaaa0de3b4b9030aa72dd5c27294992260", size = 2281635 },
            { url = "https://files.pythonhosted.org/packages/35/1f/25330293a5bb6bd50825725270c587c2b25c2694020a82d2c424d2fd5469/fonttools-4.55.8-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:d5a3ff5bb95fd5a3962b2754f8435e6d930c84fc9e9921c51e802dddf40acd56", size = 4869363 },
            { url = "https://files.pythonhosted.org/packages/f2/e0/e58b10ef50830145ba94dbeb64b70773af61cfccea663d485c7fae2aab65/fonttools-4.55.8-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:b99d4fd2b6d0a00c7336c8363fccc7a11eccef4b17393af75ca6e77cf93ff413", size = 4898604 },
            { url = "https://files.pythonhosted.org/packages/e0/66/b59025011dbae1ea10dcb60f713a10e54d17cde5c8dc48db75af79dc2088/fonttools-4.55.8-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:d637e4d33e46619c79d1a6c725f74d71b574cd15fb5bbb9b6f3eba8f28363573", size = 4877804 },
            { url = "https://files.pythonhosted.org/packages/67/76/abbbae972af55d54f83fcaeb90e26aaac937c8711b5a32d7c63768c37891/fonttools-4.55.8-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:0f38bfb6b7a39c4162c3eb0820a0bdf8e3bdd125cd54e10ba242397d15e32439", size = 5045913 },
            { url = "https://files.pythonhosted.org/packages/8b/f2/5eb68b5202731b008ccfd4ad6d82af9a8abdec411609e76fdd6c43881f2c/fonttools-4.55.8-cp311-cp311-win32.whl", hash = "sha256:acfec948de41cd5e640d5c15d0200e8b8e7c5c6bb82afe1ca095cbc4af1188ee", size = 2154525 },
            { url = "https://files.pythonhosted.org/packages/42/d6/96dc2462006ffa16c8d475244e372abdc47d03a7bd38be0f29e7ae552af4/fonttools-4.55.8-cp311-cp311-win_amd64.whl", hash = "sha256:604c805b41241b4880e2dc86cf2d4754c06777371c8299799ac88d836cb18c3b", size = 2201043 },
            { url = "https://files.pythonhosted.org/packages/e9/ce/8358af1c353d890d4c6cbcc3d64242631f91a93f8384b76bc49db800f1de/fonttools-4.55.8-cp312-cp312-macosx_10_13_universal2.whl", hash = "sha256:63403ee0f2fa4e1de28e539f8c24f2bdca1d8ecb503fa9ea2d231d9f1e729809", size = 2747851 },
            { url = "https://files.pythonhosted.org/packages/1b/3d/7a906f58f80c1ed37bbdf7b3f9b6792906156cb9143b067bf54c38405134/fonttools-4.55.8-cp312-cp312-macosx_10_13_x86_64.whl", hash = "sha256:302e1003a760b222f711d5ba6d1ad7fd5f7f713eb872cd6a3eb44390bc9770af", size = 2279102 },
            { url = "https://files.pythonhosted.org/packages/0a/0a/91a923a9de012e0f751ef8e13e1a5ea10f3a1b8416ae9afd5db1ad351b20/fonttools-4.55.8-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:e72a7816ff8a759be9ca36ca46934f8ccf4383711ef597d9240306fe1878cb8d", size = 4784092 },
            { url = "https://files.pythonhosted.org/packages/e8/07/4b8a5c8a746cc8c8103c6462d057d8806bd925347ac3905055686dd40e94/fonttools-4.55.8-cp312-cp312-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:03c2b50b54e6e8b3564b232e57e8f58be217cf441cf0155745d9e44a76f9c30f", size = 4855206 },
            { url = "https://files.pythonhosted.org/packages/37/df/09bf09ff8eae1e74bf16f9df514fd60af9f3d994e3edb0339f7d0bbc59e2/fonttools-4.55.8-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:a7230f7590f9570d26ee903b6a4540274494e200fae978df0d9325b7b9144529", size = 4762599 },
            { url = "https://files.pythonhosted.org/packages/84/58/a80d97818a3bede7e4b58318302e89e749b9639c890ecbc972a6e533201f/fonttools-4.55.8-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:466a78984f0572305c3c48377f4e3f7f4e909f1209f45ef8e7041d5c8a744a56", size = 4990188 },
            { url = "https://files.pythonhosted.org/packages/a8/e3/1f1b1a70527ab9a1b9bfe1829a783a042c108ab3357af626e8e69a21f0e2/fonttools-4.55.8-cp312-cp312-win32.whl", hash = "sha256:243cbfc0b7cb1c307af40e321f8343a48d0a080bc1f9466cf2b5468f776ef108", size = 2142995 },
            { url = "https://files.pythonhosted.org/packages/61/cf/08c4954c944799458690eb0e498209fb6a2e79e20a869189f56d18e909b6/fonttools-4.55.8-cp312-cp312-win_amd64.whl", hash = "sha256:a19059aa892676822c1f05cb5a67296ecdfeb267fe7c47d4758f3e8e942c2b2a", size = 2189833 },
            { url = "https://files.pythonhosted.org/packages/87/fe/02a377477c5c95cb118ce8b7501d868e79fce310681a536bd1099bde6874/fonttools-4.55.8-cp313-cp313-macosx_10_13_universal2.whl", hash = "sha256:332883b6280b9d90d2ba7e9e81be77cf2ace696161e60cdcf40cfcd2b3ed06fa", size = 2735213 },
            { url = "https://files.pythonhosted.org/packages/58/e4/a839f867e636419d7e5ca426a470df575bf7b20cc780862d6f64caee405c/fonttools-4.55.8-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:6b8d7c149d47b47de7ec81763396c8266e5ebe2e0b14aa9c3ccf29e52260ab2f", size = 2272614 },
            { url = "https://files.pythonhosted.org/packages/31/c0/085d1fb2cff1589e038a67579660e16cdc0ea0ffe839a849879af43f6b1a/fonttools-4.55.8-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:4dfae7c94987149bdaa0388e6c937566aa398fa0eec973b17952350a069cff4e", size = 4762524 },
            { url = "https://files.pythonhosted.org/packages/b3/75/00670fa832e2986f9c6bfbd029f0a1e90a14333f0a6c02632284e9c1baa0/fonttools-4.55.8-cp313-cp313-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a0fe12f06169af2fdc642d26a8df53e40adc3beedbd6ffedb19f1c5397b63afd", size = 4834537 },
            { url = "https://files.pythonhosted.org/packages/f4/a5/0fd300cdd1f9ab09857ba016a7acb9eff2fb3695109eb44d93ee28389a41/fonttools-4.55.8-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:f971aa5f50c22dc4b63a891503624ae2c77330429b34ead32f23c2260c5618cd", size = 4742903 },
            { url = "https://files.pythonhosted.org/packages/59/e8/bb8da5e52802333e9ef23112583f9c24279f6cf720b005434f21f0e063fb/fonttools-4.55.8-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:708cb17b2590b7f6c6854999df0039ff1140dda9e6f56d67c3599ba6f968fab5", size = 4963841 },
            { url = "https://files.pythonhosted.org/packages/74/2b/e8268cfddb35d1ad964fcfe12d105ae4a7112b89fa098681dce110a97f9f/fonttools-4.55.8-cp313-cp313-win32.whl", hash = "sha256:cfe9cf30f391a0f2875247a3e5e44d8dcb61596e5cf89b360cdffec8a80e9961", size = 2141024 },
            { url = "https://files.pythonhosted.org/packages/b8/f9/3c69478a63250ad015a9ff1a75cd72d00aed0c26c188bd838ad5b67f7c83/fonttools-4.55.8-cp313-cp313-win_amd64.whl", hash = "sha256:1e10efc8ee10d6f1fe2931d41bccc90cd4b872f2ee4ff21f2231a2c293b2dbf8", size = 2186823 },
            { url = "https://files.pythonhosted.org/packages/cc/e6/efdcd5d6858b951c29d56de31a19355579d826712bf390d964a21b076ddb/fonttools-4.55.8-py3-none-any.whl", hash = "sha256:07636dae94f7fe88561f9da7a46b13d8e3f529f87fdb221b11d85f91eabceeb7", size = 1089900 },
        ]

        [[package]]
        name = "frozenlist"
        version = "1.5.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8f/ed/0f4cec13a93c02c47ec32d81d11c0c1efbadf4a471e3f3ce7cad366cbbd3/frozenlist-1.5.0.tar.gz", hash = "sha256:81d5af29e61b9c8348e876d442253723928dce6433e0e76cd925cd83f1b4b817", size = 39930 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/54/79/29d44c4af36b2b240725dce566b20f63f9b36ef267aaaa64ee7466f4f2f8/frozenlist-1.5.0-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:5b6a66c18b5b9dd261ca98dffcb826a525334b2f29e7caa54e182255c5f6a65a", size = 94451 },
            { url = "https://files.pythonhosted.org/packages/47/47/0c999aeace6ead8a44441b4f4173e2261b18219e4ad1fe9a479871ca02fc/frozenlist-1.5.0-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:d1b3eb7b05ea246510b43a7e53ed1653e55c2121019a97e60cad7efb881a97bb", size = 54301 },
            { url = "https://files.pythonhosted.org/packages/8d/60/107a38c1e54176d12e06e9d4b5d755b677d71d1219217cee063911b1384f/frozenlist-1.5.0-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:15538c0cbf0e4fa11d1e3a71f823524b0c46299aed6e10ebb4c2089abd8c3bec", size = 52213 },
            { url = "https://files.pythonhosted.org/packages/17/62/594a6829ac5679c25755362a9dc93486a8a45241394564309641425d3ff6/frozenlist-1.5.0-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:e79225373c317ff1e35f210dd5f1344ff31066ba8067c307ab60254cd3a78ad5", size = 240946 },
            { url = "https://files.pythonhosted.org/packages/7e/75/6c8419d8f92c80dd0ee3f63bdde2702ce6398b0ac8410ff459f9b6f2f9cb/frozenlist-1.5.0-cp310-cp310-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:9272fa73ca71266702c4c3e2d4a28553ea03418e591e377a03b8e3659d94fa76", size = 264608 },
            { url = "https://files.pythonhosted.org/packages/88/3e/82a6f0b84bc6fb7e0be240e52863c6d4ab6098cd62e4f5b972cd31e002e8/frozenlist-1.5.0-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:498524025a5b8ba81695761d78c8dd7382ac0b052f34e66939c42df860b8ff17", size = 261361 },
            { url = "https://files.pythonhosted.org/packages/fd/85/14e5f9ccac1b64ff2f10c927b3ffdf88772aea875882406f9ba0cec8ad84/frozenlist-1.5.0-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:92b5278ed9d50fe610185ecd23c55d8b307d75ca18e94c0e7de328089ac5dcba", size = 231649 },
            { url = "https://files.pythonhosted.org/packages/ee/59/928322800306f6529d1852323014ee9008551e9bb027cc38d276cbc0b0e7/frozenlist-1.5.0-cp310-cp310-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:7f3c8c1dacd037df16e85227bac13cca58c30da836c6f936ba1df0c05d046d8d", size = 241853 },
            { url = "https://files.pythonhosted.org/packages/7d/bd/e01fa4f146a6f6c18c5d34cab8abdc4013774a26c4ff851128cd1bd3008e/frozenlist-1.5.0-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:f2ac49a9bedb996086057b75bf93538240538c6d9b38e57c82d51f75a73409d2", size = 243652 },
            { url = "https://files.pythonhosted.org/packages/a5/bd/e4771fd18a8ec6757033f0fa903e447aecc3fbba54e3630397b61596acf0/frozenlist-1.5.0-cp310-cp310-musllinux_1_2_i686.whl", hash = "sha256:e66cc454f97053b79c2ab09c17fbe3c825ea6b4de20baf1be28919460dd7877f", size = 241734 },
            { url = "https://files.pythonhosted.org/packages/21/13/c83821fa5544af4f60c5d3a65d054af3213c26b14d3f5f48e43e5fb48556/frozenlist-1.5.0-cp310-cp310-musllinux_1_2_ppc64le.whl", hash = "sha256:5a3ba5f9a0dfed20337d3e966dc359784c9f96503674c2faf015f7fe8e96798c", size = 260959 },
            { url = "https://files.pythonhosted.org/packages/71/f3/1f91c9a9bf7ed0e8edcf52698d23f3c211d8d00291a53c9f115ceb977ab1/frozenlist-1.5.0-cp310-cp310-musllinux_1_2_s390x.whl", hash = "sha256:6321899477db90bdeb9299ac3627a6a53c7399c8cd58d25da094007402b039ab", size = 262706 },
            { url = "https://files.pythonhosted.org/packages/4c/22/4a256fdf5d9bcb3ae32622c796ee5ff9451b3a13a68cfe3f68e2c95588ce/frozenlist-1.5.0-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:76e4753701248476e6286f2ef492af900ea67d9706a0155335a40ea21bf3b2f5", size = 250401 },
            { url = "https://files.pythonhosted.org/packages/af/89/c48ebe1f7991bd2be6d5f4ed202d94960c01b3017a03d6954dd5fa9ea1e8/frozenlist-1.5.0-cp310-cp310-win32.whl", hash = "sha256:977701c081c0241d0955c9586ffdd9ce44f7a7795df39b9151cd9a6fd0ce4cfb", size = 45498 },
            { url = "https://files.pythonhosted.org/packages/28/2f/cc27d5f43e023d21fe5c19538e08894db3d7e081cbf582ad5ed366c24446/frozenlist-1.5.0-cp310-cp310-win_amd64.whl", hash = "sha256:189f03b53e64144f90990d29a27ec4f7997d91ed3d01b51fa39d2dbe77540fd4", size = 51622 },
            { url = "https://files.pythonhosted.org/packages/79/43/0bed28bf5eb1c9e4301003b74453b8e7aa85fb293b31dde352aac528dafc/frozenlist-1.5.0-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:fd74520371c3c4175142d02a976aee0b4cb4a7cc912a60586ffd8d5929979b30", size = 94987 },
            { url = "https://files.pythonhosted.org/packages/bb/bf/b74e38f09a246e8abbe1e90eb65787ed745ccab6eaa58b9c9308e052323d/frozenlist-1.5.0-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:2f3f7a0fbc219fb4455264cae4d9f01ad41ae6ee8524500f381de64ffaa077d5", size = 54584 },
            { url = "https://files.pythonhosted.org/packages/2c/31/ab01375682f14f7613a1ade30149f684c84f9b8823a4391ed950c8285656/frozenlist-1.5.0-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:f47c9c9028f55a04ac254346e92977bf0f166c483c74b4232bee19a6697e4778", size = 52499 },
            { url = "https://files.pythonhosted.org/packages/98/a8/d0ac0b9276e1404f58fec3ab6e90a4f76b778a49373ccaf6a563f100dfbc/frozenlist-1.5.0-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:0996c66760924da6e88922756d99b47512a71cfd45215f3570bf1e0b694c206a", size = 276357 },
            { url = "https://files.pythonhosted.org/packages/ad/c9/c7761084fa822f07dac38ac29f841d4587570dd211e2262544aa0b791d21/frozenlist-1.5.0-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:a2fe128eb4edeabe11896cb6af88fca5346059f6c8d807e3b910069f39157869", size = 287516 },
            { url = "https://files.pythonhosted.org/packages/a1/ff/cd7479e703c39df7bdab431798cef89dc75010d8aa0ca2514c5b9321db27/frozenlist-1.5.0-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:1a8ea951bbb6cacd492e3948b8da8c502a3f814f5d20935aae74b5df2b19cf3d", size = 283131 },
            { url = "https://files.pythonhosted.org/packages/59/a0/370941beb47d237eca4fbf27e4e91389fd68699e6f4b0ebcc95da463835b/frozenlist-1.5.0-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:de537c11e4aa01d37db0d403b57bd6f0546e71a82347a97c6a9f0dcc532b3a45", size = 261320 },
            { url = "https://files.pythonhosted.org/packages/b8/5f/c10123e8d64867bc9b4f2f510a32042a306ff5fcd7e2e09e5ae5100ee333/frozenlist-1.5.0-cp311-cp311-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:9c2623347b933fcb9095841f1cc5d4ff0b278addd743e0e966cb3d460278840d", size = 274877 },
            { url = "https://files.pythonhosted.org/packages/fa/79/38c505601ae29d4348f21706c5d89755ceded02a745016ba2f58bd5f1ea6/frozenlist-1.5.0-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:cee6798eaf8b1416ef6909b06f7dc04b60755206bddc599f52232606e18179d3", size = 269592 },
            { url = "https://files.pythonhosted.org/packages/19/e2/39f3a53191b8204ba9f0bb574b926b73dd2efba2a2b9d2d730517e8f7622/frozenlist-1.5.0-cp311-cp311-musllinux_1_2_i686.whl", hash = "sha256:f5f9da7f5dbc00a604fe74aa02ae7c98bcede8a3b8b9666f9f86fc13993bc71a", size = 265934 },
            { url = "https://files.pythonhosted.org/packages/d5/c9/3075eb7f7f3a91f1a6b00284af4de0a65a9ae47084930916f5528144c9dd/frozenlist-1.5.0-cp311-cp311-musllinux_1_2_ppc64le.whl", hash = "sha256:90646abbc7a5d5c7c19461d2e3eeb76eb0b204919e6ece342feb6032c9325ae9", size = 283859 },
            { url = "https://files.pythonhosted.org/packages/05/f5/549f44d314c29408b962fa2b0e69a1a67c59379fb143b92a0a065ffd1f0f/frozenlist-1.5.0-cp311-cp311-musllinux_1_2_s390x.whl", hash = "sha256:bdac3c7d9b705d253b2ce370fde941836a5f8b3c5c2b8fd70940a3ea3af7f4f2", size = 287560 },
            { url = "https://files.pythonhosted.org/packages/9d/f8/cb09b3c24a3eac02c4c07a9558e11e9e244fb02bf62c85ac2106d1eb0c0b/frozenlist-1.5.0-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:03d33c2ddbc1816237a67f66336616416e2bbb6beb306e5f890f2eb22b959cdf", size = 277150 },
            { url = "https://files.pythonhosted.org/packages/37/48/38c2db3f54d1501e692d6fe058f45b6ad1b358d82cd19436efab80cfc965/frozenlist-1.5.0-cp311-cp311-win32.whl", hash = "sha256:237f6b23ee0f44066219dae14c70ae38a63f0440ce6750f868ee08775073f942", size = 45244 },
            { url = "https://files.pythonhosted.org/packages/ca/8c/2ddffeb8b60a4bce3b196c32fcc30d8830d4615e7b492ec2071da801b8ad/frozenlist-1.5.0-cp311-cp311-win_amd64.whl", hash = "sha256:0cc974cc93d32c42e7b0f6cf242a6bd941c57c61b618e78b6c0a96cb72788c1d", size = 51634 },
            { url = "https://files.pythonhosted.org/packages/79/73/fa6d1a96ab7fd6e6d1c3500700963eab46813847f01ef0ccbaa726181dd5/frozenlist-1.5.0-cp312-cp312-macosx_10_13_universal2.whl", hash = "sha256:31115ba75889723431aa9a4e77d5f398f5cf976eea3bdf61749731f62d4a4a21", size = 94026 },
            { url = "https://files.pythonhosted.org/packages/ab/04/ea8bf62c8868b8eada363f20ff1b647cf2e93377a7b284d36062d21d81d1/frozenlist-1.5.0-cp312-cp312-macosx_10_13_x86_64.whl", hash = "sha256:7437601c4d89d070eac8323f121fcf25f88674627505334654fd027b091db09d", size = 54150 },
            { url = "https://files.pythonhosted.org/packages/d0/9a/8e479b482a6f2070b26bda572c5e6889bb3ba48977e81beea35b5ae13ece/frozenlist-1.5.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:7948140d9f8ece1745be806f2bfdf390127cf1a763b925c4a805c603df5e697e", size = 51927 },
            { url = "https://files.pythonhosted.org/packages/e3/12/2aad87deb08a4e7ccfb33600871bbe8f0e08cb6d8224371387f3303654d7/frozenlist-1.5.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:feeb64bc9bcc6b45c6311c9e9b99406660a9c05ca8a5b30d14a78555088b0b3a", size = 282647 },
            { url = "https://files.pythonhosted.org/packages/77/f2/07f06b05d8a427ea0060a9cef6e63405ea9e0d761846b95ef3fb3be57111/frozenlist-1.5.0-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:683173d371daad49cffb8309779e886e59c2f369430ad28fe715f66d08d4ab1a", size = 289052 },
            { url = "https://files.pythonhosted.org/packages/bd/9f/8bf45a2f1cd4aa401acd271b077989c9267ae8463e7c8b1eb0d3f561b65e/frozenlist-1.5.0-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:7d57d8f702221405a9d9b40f9da8ac2e4a1a8b5285aac6100f3393675f0a85ee", size = 291719 },
            { url = "https://files.pythonhosted.org/packages/41/d1/1f20fd05a6c42d3868709b7604c9f15538a29e4f734c694c6bcfc3d3b935/frozenlist-1.5.0-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:30c72000fbcc35b129cb09956836c7d7abf78ab5416595e4857d1cae8d6251a6", size = 267433 },
            { url = "https://files.pythonhosted.org/packages/af/f2/64b73a9bb86f5a89fb55450e97cd5c1f84a862d4ff90d9fd1a73ab0f64a5/frozenlist-1.5.0-cp312-cp312-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:000a77d6034fbad9b6bb880f7ec073027908f1b40254b5d6f26210d2dab1240e", size = 283591 },
            { url = "https://files.pythonhosted.org/packages/29/e2/ffbb1fae55a791fd6c2938dd9ea779509c977435ba3940b9f2e8dc9d5316/frozenlist-1.5.0-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:5d7f5a50342475962eb18b740f3beecc685a15b52c91f7d975257e13e029eca9", size = 273249 },
            { url = "https://files.pythonhosted.org/packages/2e/6e/008136a30798bb63618a114b9321b5971172a5abddff44a100c7edc5ad4f/frozenlist-1.5.0-cp312-cp312-musllinux_1_2_i686.whl", hash = "sha256:87f724d055eb4785d9be84e9ebf0f24e392ddfad00b3fe036e43f489fafc9039", size = 271075 },
            { url = "https://files.pythonhosted.org/packages/ae/f0/4e71e54a026b06724cec9b6c54f0b13a4e9e298cc8db0f82ec70e151f5ce/frozenlist-1.5.0-cp312-cp312-musllinux_1_2_ppc64le.whl", hash = "sha256:6e9080bb2fb195a046e5177f10d9d82b8a204c0736a97a153c2466127de87784", size = 285398 },
            { url = "https://files.pythonhosted.org/packages/4d/36/70ec246851478b1c0b59f11ef8ade9c482ff447c1363c2bd5fad45098b12/frozenlist-1.5.0-cp312-cp312-musllinux_1_2_s390x.whl", hash = "sha256:9b93d7aaa36c966fa42efcaf716e6b3900438632a626fb09c049f6a2f09fc631", size = 294445 },
            { url = "https://files.pythonhosted.org/packages/37/e0/47f87544055b3349b633a03c4d94b405956cf2437f4ab46d0928b74b7526/frozenlist-1.5.0-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:52ef692a4bc60a6dd57f507429636c2af8b6046db8b31b18dac02cbc8f507f7f", size = 280569 },
            { url = "https://files.pythonhosted.org/packages/f9/7c/490133c160fb6b84ed374c266f42800e33b50c3bbab1652764e6e1fc498a/frozenlist-1.5.0-cp312-cp312-win32.whl", hash = "sha256:29d94c256679247b33a3dc96cce0f93cbc69c23bf75ff715919332fdbb6a32b8", size = 44721 },
            { url = "https://files.pythonhosted.org/packages/b1/56/4e45136ffc6bdbfa68c29ca56ef53783ef4c2fd395f7cbf99a2624aa9aaa/frozenlist-1.5.0-cp312-cp312-win_amd64.whl", hash = "sha256:8969190d709e7c48ea386db202d708eb94bdb29207a1f269bab1196ce0dcca1f", size = 51329 },
            { url = "https://files.pythonhosted.org/packages/da/3b/915f0bca8a7ea04483622e84a9bd90033bab54bdf485479556c74fd5eaf5/frozenlist-1.5.0-cp313-cp313-macosx_10_13_universal2.whl", hash = "sha256:7a1a048f9215c90973402e26c01d1cff8a209e1f1b53f72b95c13db61b00f953", size = 91538 },
            { url = "https://files.pythonhosted.org/packages/c7/d1/a7c98aad7e44afe5306a2b068434a5830f1470675f0e715abb86eb15f15b/frozenlist-1.5.0-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:dd47a5181ce5fcb463b5d9e17ecfdb02b678cca31280639255ce9d0e5aa67af0", size = 52849 },
            { url = "https://files.pythonhosted.org/packages/3a/c8/76f23bf9ab15d5f760eb48701909645f686f9c64fbb8982674c241fbef14/frozenlist-1.5.0-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:1431d60b36d15cda188ea222033eec8e0eab488f39a272461f2e6d9e1a8e63c2", size = 50583 },
            { url = "https://files.pythonhosted.org/packages/1f/22/462a3dd093d11df623179d7754a3b3269de3b42de2808cddef50ee0f4f48/frozenlist-1.5.0-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:6482a5851f5d72767fbd0e507e80737f9c8646ae7fd303def99bfe813f76cf7f", size = 265636 },
            { url = "https://files.pythonhosted.org/packages/80/cf/e075e407fc2ae7328155a1cd7e22f932773c8073c1fc78016607d19cc3e5/frozenlist-1.5.0-cp313-cp313-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:44c49271a937625619e862baacbd037a7ef86dd1ee215afc298a417ff3270608", size = 270214 },
            { url = "https://files.pythonhosted.org/packages/a1/58/0642d061d5de779f39c50cbb00df49682832923f3d2ebfb0fedf02d05f7f/frozenlist-1.5.0-cp313-cp313-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:12f78f98c2f1c2429d42e6a485f433722b0061d5c0b0139efa64f396efb5886b", size = 273905 },
            { url = "https://files.pythonhosted.org/packages/ab/66/3fe0f5f8f2add5b4ab7aa4e199f767fd3b55da26e3ca4ce2cc36698e50c4/frozenlist-1.5.0-cp313-cp313-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ce3aa154c452d2467487765e3adc730a8c153af77ad84096bc19ce19a2400840", size = 250542 },
            { url = "https://files.pythonhosted.org/packages/f6/b8/260791bde9198c87a465224e0e2bb62c4e716f5d198fc3a1dacc4895dbd1/frozenlist-1.5.0-cp313-cp313-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:9b7dc0c4338e6b8b091e8faf0db3168a37101943e687f373dce00959583f7439", size = 267026 },
            { url = "https://files.pythonhosted.org/packages/2e/a4/3d24f88c527f08f8d44ade24eaee83b2627793fa62fa07cbb7ff7a2f7d42/frozenlist-1.5.0-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:45e0896250900b5aa25180f9aec243e84e92ac84bd4a74d9ad4138ef3f5c97de", size = 257690 },
            { url = "https://files.pythonhosted.org/packages/de/9a/d311d660420b2beeff3459b6626f2ab4fb236d07afbdac034a4371fe696e/frozenlist-1.5.0-cp313-cp313-musllinux_1_2_i686.whl", hash = "sha256:561eb1c9579d495fddb6da8959fd2a1fca2c6d060d4113f5844b433fc02f2641", size = 253893 },
            { url = "https://files.pythonhosted.org/packages/c6/23/e491aadc25b56eabd0f18c53bb19f3cdc6de30b2129ee0bc39cd387cd560/frozenlist-1.5.0-cp313-cp313-musllinux_1_2_ppc64le.whl", hash = "sha256:df6e2f325bfee1f49f81aaac97d2aa757c7646534a06f8f577ce184afe2f0a9e", size = 267006 },
            { url = "https://files.pythonhosted.org/packages/08/c4/ab918ce636a35fb974d13d666dcbe03969592aeca6c3ab3835acff01f79c/frozenlist-1.5.0-cp313-cp313-musllinux_1_2_s390x.whl", hash = "sha256:140228863501b44b809fb39ec56b5d4071f4d0aa6d216c19cbb08b8c5a7eadb9", size = 276157 },
            { url = "https://files.pythonhosted.org/packages/c0/29/3b7a0bbbbe5a34833ba26f686aabfe982924adbdcafdc294a7a129c31688/frozenlist-1.5.0-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:7707a25d6a77f5d27ea7dc7d1fc608aa0a478193823f88511ef5e6b8a48f9d03", size = 264642 },
            { url = "https://files.pythonhosted.org/packages/ab/42/0595b3dbffc2e82d7fe658c12d5a5bafcd7516c6bf2d1d1feb5387caa9c1/frozenlist-1.5.0-cp313-cp313-win32.whl", hash = "sha256:31a9ac2b38ab9b5a8933b693db4939764ad3f299fcaa931a3e605bc3460e693c", size = 44914 },
            { url = "https://files.pythonhosted.org/packages/17/c4/b7db1206a3fea44bf3b838ca61deb6f74424a8a5db1dd53ecb21da669be6/frozenlist-1.5.0-cp313-cp313-win_amd64.whl", hash = "sha256:11aabdd62b8b9c4b84081a3c246506d1cddd2dd93ff0ad53ede5defec7886b28", size = 51167 },
            { url = "https://files.pythonhosted.org/packages/c6/c8/a5be5b7550c10858fcf9b0ea054baccab474da77d37f1e828ce043a3a5d4/frozenlist-1.5.0-py3-none-any.whl", hash = "sha256:d994863bba198a4a518b467bb971c56e1db3f180a25c6cf7bb1949c267f748c3", size = 11901 },
        ]

        [[package]]
        name = "fsspec"
        version = "2025.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b5/79/68612ed99700e6413de42895aa725463e821a6b3be75c87fcce1b4af4c70/fsspec-2025.2.0.tar.gz", hash = "sha256:1c24b16eaa0a1798afa0337aa0db9b256718ab2a89c425371f5628d22c3b6afd", size = 292283 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e2/94/758680531a00d06e471ef649e4ec2ed6bf185356a7f9fbfbb7368a40bd49/fsspec-2025.2.0-py3-none-any.whl", hash = "sha256:9de2ad9ce1f85e1931858535bc882543171d197001a0a5eb2ddc04f1781ab95b", size = 184484 },
        ]

        [package.optional-dependencies]
        http = [
            { name = "aiohttp" },
        ]

        [[package]]
        name = "gitdb"
        version = "4.0.12"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "smmap" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/72/94/63b0fc47eb32792c7ba1fe1b694daec9a63620db1e313033d18140c2320a/gitdb-4.0.12.tar.gz", hash = "sha256:5ef71f855d191a3326fcfbc0d5da835f26b13fbcba60c32c21091c349ffdb571", size = 394684 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a0/61/5c78b91c3143ed5c14207f463aecfc8f9dbb5092fb2869baf37c273b2705/gitdb-4.0.12-py3-none-any.whl", hash = "sha256:67073e15955400952c6565cc3e707c554a4eea2e428946f7a4c162fab9bd9bcf", size = 62794 },
        ]

        [[package]]
        name = "gitpython"
        version = "3.1.44"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "gitdb" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c0/89/37df0b71473153574a5cdef8f242de422a0f5d26d7a9e231e6f169b4ad14/gitpython-3.1.44.tar.gz", hash = "sha256:c87e30b26253bf5418b01b0660f818967f3c503193838337fe5e573331249269", size = 214196 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/1d/9a/4114a9057db2f1462d5c8f8390ab7383925fe1ac012eaa42402ad65c2963/GitPython-3.1.44-py3-none-any.whl", hash = "sha256:9e0e10cda9bed1ee64bc9a6de50e7e38a9c9943241cd7f585f6df3ed28011110", size = 207599 },
        ]

        [[package]]
        name = "h5py"
        version = "3.12.[X]"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "numpy" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/cc/0c/5c2b0a88158682aeafb10c1c2b735df5bc31f165bfe192f2ee9f2a23b5f1/h5py-3.12.[X].tar.gz", hash = "sha256:326d70b53d31baa61f00b8aa5f95c2fcb9621a3ee8365d770c551a13dbbcbfdf", size = 411457 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/df/7d/b21045fbb004ad8bb6fb3be4e6ca903841722706f7130b9bba31ef2f88e3/h5py-3.12.[X]-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:2f0f1a382cbf494679c07b4371f90c70391dedb027d517ac94fa2c05299dacda", size = 3402133 },
            { url = "https://files.pythonhosted.org/packages/29/a7/3c2a33fba1da64a0846744726fd067a92fb8abb887875a0dd8e3bac8b45d/h5py-3.12.[X]-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:cb65f619dfbdd15e662423e8d257780f9a66677eae5b4b3fc9dca70b5fd2d2a3", size = 2866436 },
            { url = "https://files.pythonhosted.org/packages/1e/d0/4bf67c3937a2437c20844165766ddd1a1817ae6b9544c3743050d8e0f403/h5py-3.12.[X]-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:3b15d8dbd912c97541312c0e07438864d27dbca857c5ad634de68110c6beb1c2", size = 5168596 },
            { url = "https://files.pythonhosted.org/packages/85/bc/e76f4b2096e0859225f5441d1b7f5e2041fffa19fc2c16756c67078417aa/h5py-3.12.[X]-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:59685fe40d8c1fbbee088c88cd4da415a2f8bee5c270337dc5a1c4aa634e3307", size = 5341537 },
            { url = "https://files.pythonhosted.org/packages/99/bd/fb8ed45308bb97e04c02bd7aed324ba11e6a4bf9ed73967ca2a168e9cf92/h5py-3.12.[X]-cp310-cp310-win_amd64.whl", hash = "sha256:577d618d6b6dea3da07d13cc903ef9634cde5596b13e832476dd861aaf651f3e", size = 2990575 },
            { url = "https://files.pythonhosted.org/packages/33/61/c463dc5fc02fbe019566d067a9d18746cd3c664f29c9b8b3c3f9ed025365/h5py-3.12.[X]-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:ccd9006d92232727d23f784795191bfd02294a4f2ba68708825cb1da39511a93", size = 3410828 },
            { url = "https://files.pythonhosted.org/packages/95/9d/eb91a9076aa998bb2179d6b1788055ea09cdf9d6619cd967f1d3321ed056/h5py-3.12.[X]-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:ad8a76557880aed5234cfe7279805f4ab5ce16b17954606cca90d578d3e713ef", size = 2872586 },
            { url = "https://files.pythonhosted.org/packages/b0/62/e2b1f9723ff713e3bd3c16dfeceec7017eadc21ef063d8b7080c0fcdc58a/h5py-3.12.[X]-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:1473348139b885393125126258ae2d70753ef7e9cec8e7848434f385ae72069e", size = 5273038 },
            { url = "https://files.pythonhosted.org/packages/e1/89/118c3255d6ff2db33b062ec996a762d99ae50c21f54a8a6047ae8eda1b9f/h5py-3.12.[X]-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:018a4597f35092ae3fb28ee851fdc756d2b88c96336b8480e124ce1ac6fb9166", size = 5452688 },
            { url = "https://files.pythonhosted.org/packages/1d/4d/cbd3014eb78d1e449b29beba1f3293a841aa8086c6f7968c383c2c7ff076/h5py-3.12.[X]-cp311-cp311-win_amd64.whl", hash = "sha256:3fdf95092d60e8130ba6ae0ef7a9bd4ade8edbe3569c13ebbaf39baefffc5ba4", size = 3006095 },
            { url = "https://files.pythonhosted.org/packages/d4/e1/ea9bfe18a3075cdc873f0588ff26ce394726047653557876d7101bf0c74e/h5py-3.12.[X]-cp312-cp312-macosx_10_13_x86_64.whl", hash = "sha256:06a903a4e4e9e3ebbc8b548959c3c2552ca2d70dac14fcfa650d9261c66939ed", size = 3372538 },
            { url = "https://files.pythonhosted.org/packages/0d/74/1009b663387c025e8fa5f3ee3cf3cd0d99b1ad5c72eeb70e75366b1ce878/h5py-3.12.[X]-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:7b3b8f3b48717e46c6a790e3128d39c61ab595ae0a7237f06dfad6a3b51d5351", size = 2868104 },
            { url = "https://files.pythonhosted.org/packages/af/52/c604adc06280c15a29037d4aa79a24fe54d8d0b51085e81ed24b2fa995f7/h5py-3.12.[X]-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:050a4f2c9126054515169c49cb900949814987f0c7ae74c341b0c9f9b5056834", size = 5194606 },
            { url = "https://files.pythonhosted.org/packages/fa/63/eeaacff417b393491beebabb8a3dc5342950409eb6d7b39d437289abdbae/h5py-3.12.[X]-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:5c4b41d1019322a5afc5082864dfd6359f8935ecd37c11ac0029be78c5d112c9", size = 5413256 },
            { url = "https://files.pythonhosted.org/packages/86/f7/bb465dcb92ca3521a15cbe1031f6d18234dbf1fb52a6796a00bfaa846ebf/h5py-3.12.[X]-cp312-cp312-win_amd64.whl", hash = "sha256:e4d51919110a030913201422fb07987db4338eba5ec8c5a15d6fab8e03d443fc", size = 2993055 },
            { url = "https://files.pythonhosted.org/packages/23/1c/ecdd0efab52c24f2a9bf2324289828b860e8dd1e3c5ada3cf0889e14fdc1/h5py-3.12.[X]-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:513171e90ed92236fc2ca363ce7a2fc6f2827375efcbb0cc7fbdd7fe11fecafc", size = 3346239 },
            { url = "https://files.pythonhosted.org/packages/93/cd/5b6f574bf3e318bbe305bc93ba45181676550eb44ba35e006d2e98004eaa/h5py-3.12.[X]-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:59400f88343b79655a242068a9c900001a34b63e3afb040bd7cdf717e440f653", size = 2843416 },
            { url = "https://files.pythonhosted.org/packages/8a/4f/b74332f313bfbe94ba03fff784219b9db385e6139708e55b11490149f90a/h5py-3.12.[X]-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:d3e465aee0ec353949f0f46bf6c6f9790a2006af896cee7c178a8c3e5090aa32", size = 5154390 },
            { url = "https://files.pythonhosted.org/packages/1a/57/93ea9e10a6457ea8d3b867207deb29a527e966a08a84c57ffd954e32152a/h5py-3.12.[X]-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:ba51c0c5e029bb5420a343586ff79d56e7455d496d18a30309616fdbeed1068f", size = 5378244 },
            { url = "https://files.pythonhosted.org/packages/50/51/0bbf3663062b2eeee78aa51da71e065f8a0a6e3cb950cc7020b4444999e6/h5py-3.12.[X]-cp313-cp313-win_amd64.whl", hash = "sha256:52ab036c6c97055b85b2a242cb540ff9590bacfda0c03dd0cf0661b311f522f8", size = 2979760 },
        ]

        [[package]]
        name = "idna"
        version = "3.10"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/f1/70/7703c29685631f5a7590aa73f1f1d3fa9a380e654b86af429e0934a32f7d/idna-3.10.tar.gz", hash = "sha256:12f65c9b470abda6dc35cf8e63cc574b1c52b11df2c86030af0ac09b01b13ea9", size = 190490 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/76/c6/c88e154df9c4e1a2a66ccf0005a88dfb2650c1dffb6f5ce603dfbd452ce3/idna-3.10-py3-none-any.whl", hash = "sha256:946d195a0d259cbba61165e88e65941f16e9b36ea6ddb97f00452bae8b1287d3", size = 70442 },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.5"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/af/92/b3130cbbf5591acf9ade8708c365f3238046ac7cb8ccba6e81abccb0ccff/jinja2-3.1.5.tar.gz", hash = "sha256:8fefff8dc3034e27bb80d67c671eb8a9bc424c0ef4c0826edbff304cceff43bb", size = 244674 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bd/0f/2ba5fbcd631e3e88689309dbe978c5769e883e4b84ebfe7da30b43275c5a/jinja2-3.1.5-py3-none-any.whl", hash = "sha256:aba0f4dc9ed8013c424088f68a5c226f7d6097ed89b246d7749c2ec4175c6adb", size = 134596 },
        ]

        [[package]]
        name = "joblib"
        version = "1.4.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/64/33/60135848598c076ce4b231e1b1895170f45fbcaeaa2c9d5e38b04db70c35/joblib-1.4.2.tar.gz", hash = "sha256:2382c5816b2636fbd20a09e0f4e9dad4736765fdfb7dca582943b9c1366b3f0e", size = 2116621 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/91/29/df4b9b42f2be0b623cbd5e2140cafcaa2bef0759a00b7b70104dcfe2fb51/joblib-1.4.2-py3-none-any.whl", hash = "sha256:06d478d5674cbc267e7496a410ee875abd68e4340feff4490bcb7afb88060ae6", size = 301817 },
        ]

        [[package]]
        name = "kiwisolver"
        version = "1.4.8"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/82/59/7c91426a8ac292e1cdd53a63b6d9439abd573c875c3f92c146767dd33faf/kiwisolver-1.4.8.tar.gz", hash = "sha256:23d5f023bdc8c7e54eb65f03ca5d5bb25b601eac4d7f1a042888a1f45237987e", size = 97538 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/47/5f/4d8e9e852d98ecd26cdf8eaf7ed8bc33174033bba5e07001b289f07308fd/kiwisolver-1.4.8-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:88c6f252f6816a73b1f8c904f7bbe02fd67c09a69f7cb8a0eecdbf5ce78e63db", size = 124623 },
            { url = "https://files.pythonhosted.org/packages/1d/70/7f5af2a18a76fe92ea14675f8bd88ce53ee79e37900fa5f1a1d8e0b42998/kiwisolver-1.4.8-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:c72941acb7b67138f35b879bbe85be0f6c6a70cab78fe3ef6db9c024d9223e5b", size = 66720 },
            { url = "https://files.pythonhosted.org/packages/c6/13/e15f804a142353aefd089fadc8f1d985561a15358c97aca27b0979cb0785/kiwisolver-1.4.8-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:ce2cf1e5688edcb727fdf7cd1bbd0b6416758996826a8be1d958f91880d0809d", size = 65413 },
            { url = "https://files.pythonhosted.org/packages/ce/6d/67d36c4d2054e83fb875c6b59d0809d5c530de8148846b1370475eeeece9/kiwisolver-1.4.8-cp310-cp310-manylinux_2_12_i686.manylinux2010_i686.whl", hash = "sha256:c8bf637892dc6e6aad2bc6d4d69d08764166e5e3f69d469e55427b6ac001b19d", size = 1650826 },
            { url = "https://files.pythonhosted.org/packages/de/c6/7b9bb8044e150d4d1558423a1568e4f227193662a02231064e3824f37e0a/kiwisolver-1.4.8-cp310-cp310-manylinux_2_12_x86_64.manylinux2010_x86_64.whl", hash = "sha256:034d2c891f76bd3edbdb3ea11140d8510dca675443da7304205a2eaa45d8334c", size = 1628231 },
            { url = "https://files.pythonhosted.org/packages/b6/38/ad10d437563063eaaedbe2c3540a71101fc7fb07a7e71f855e93ea4de605/kiwisolver-1.4.8-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:d47b28d1dfe0793d5e96bce90835e17edf9a499b53969b03c6c47ea5985844c3", size = 1408938 },
            { url = "https://files.pythonhosted.org/packages/52/ce/c0106b3bd7f9e665c5f5bc1e07cc95b5dabd4e08e3dad42dbe2faad467e7/kiwisolver-1.4.8-cp310-cp310-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:eb158fe28ca0c29f2260cca8c43005329ad58452c36f0edf298204de32a9a3ed", size = 1422799 },
            { url = "https://files.pythonhosted.org/packages/d0/87/efb704b1d75dc9758087ba374c0f23d3254505edaedd09cf9d247f7878b9/kiwisolver-1.4.8-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:d5536185fce131780ebd809f8e623bf4030ce1b161353166c49a3c74c287897f", size = 1354362 },
            { url = "https://files.pythonhosted.org/packages/eb/b3/fd760dc214ec9a8f208b99e42e8f0130ff4b384eca8b29dd0efc62052176/kiwisolver-1.4.8-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:369b75d40abedc1da2c1f4de13f3482cb99e3237b38726710f4a793432b1c5ff", size = 2222695 },
            { url = "https://files.pythonhosted.org/packages/a2/09/a27fb36cca3fc01700687cc45dae7a6a5f8eeb5f657b9f710f788748e10d/kiwisolver-1.4.8-cp310-cp310-musllinux_1_2_i686.whl", hash = "sha256:641f2ddf9358c80faa22e22eb4c9f54bd3f0e442e038728f500e3b978d00aa7d", size = 2370802 },
            { url = "https://files.pythonhosted.org/packages/3d/c3/ba0a0346db35fe4dc1f2f2cf8b99362fbb922d7562e5f911f7ce7a7b60fa/kiwisolver-1.4.8-cp310-cp310-musllinux_1_2_ppc64le.whl", hash = "sha256:d561d2d8883e0819445cfe58d7ddd673e4015c3c57261d7bdcd3710d0d14005c", size = 2334646 },
            { url = "https://files.pythonhosted.org/packages/41/52/942cf69e562f5ed253ac67d5c92a693745f0bed3c81f49fc0cbebe4d6b00/kiwisolver-1.4.8-cp310-cp310-musllinux_1_2_s390x.whl", hash = "sha256:1732e065704b47c9afca7ffa272f845300a4eb959276bf6970dc07265e73b605", size = 2467260 },
            { url = "https://files.pythonhosted.org/packages/32/26/2d9668f30d8a494b0411d4d7d4ea1345ba12deb6a75274d58dd6ea01e951/kiwisolver-1.4.8-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:bcb1ebc3547619c3b58a39e2448af089ea2ef44b37988caf432447374941574e", size = 2288633 },
            { url = "https://files.pythonhosted.org/packages/98/99/0dd05071654aa44fe5d5e350729961e7bb535372935a45ac89a8924316e6/kiwisolver-1.4.8-cp310-cp310-win_amd64.whl", hash = "sha256:89c107041f7b27844179ea9c85d6da275aa55ecf28413e87624d033cf1f6b751", size = 71885 },
            { url = "https://files.pythonhosted.org/packages/6c/fc/822e532262a97442989335394d441cd1d0448c2e46d26d3e04efca84df22/kiwisolver-1.4.8-cp310-cp310-win_arm64.whl", hash = "sha256:b5773efa2be9eb9fcf5415ea3ab70fc785d598729fd6057bea38d539ead28271", size = 65175 },
            { url = "https://files.pythonhosted.org/packages/da/ed/c913ee28936c371418cb167b128066ffb20bbf37771eecc2c97edf8a6e4c/kiwisolver-1.4.8-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:a4d3601908c560bdf880f07d94f31d734afd1bb71e96585cace0e38ef44c6d84", size = 124635 },
            { url = "https://files.pythonhosted.org/packages/4c/45/4a7f896f7467aaf5f56ef093d1f329346f3b594e77c6a3c327b2d415f521/kiwisolver-1.4.8-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:856b269c4d28a5c0d5e6c1955ec36ebfd1651ac00e1ce0afa3e28da95293b561", size = 66717 },
            { url = "https://files.pythonhosted.org/packages/5f/b4/c12b3ac0852a3a68f94598d4c8d569f55361beef6159dce4e7b624160da2/kiwisolver-1.4.8-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:c2b9a96e0f326205af81a15718a9073328df1173a2619a68553decb7097fd5d7", size = 65413 },
            { url = "https://files.pythonhosted.org/packages/a9/98/1df4089b1ed23d83d410adfdc5947245c753bddfbe06541c4aae330e9e70/kiwisolver-1.4.8-cp311-cp311-manylinux_2_12_i686.manylinux2010_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:c5020c83e8553f770cb3b5fc13faac40f17e0b205bd237aebd21d53d733adb03", size = 1343994 },
            { url = "https://files.pythonhosted.org/packages/8d/bf/b4b169b050c8421a7c53ea1ea74e4ef9c335ee9013216c558a047f162d20/kiwisolver-1.4.8-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:dace81d28c787956bfbfbbfd72fdcef014f37d9b48830829e488fdb32b49d954", size = 1434804 },
            { url = "https://files.pythonhosted.org/packages/66/5a/e13bd341fbcf73325ea60fdc8af752addf75c5079867af2e04cc41f34434/kiwisolver-1.4.8-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:11e1022b524bd48ae56c9b4f9296bce77e15a2e42a502cceba602f804b32bb79", size = 1450690 },
            { url = "https://files.pythonhosted.org/packages/9b/4f/5955dcb376ba4a830384cc6fab7d7547bd6759fe75a09564910e9e3bb8ea/kiwisolver-1.4.8-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:3b9b4d2892fefc886f30301cdd80debd8bb01ecdf165a449eb6e78f79f0fabd6", size = 1376839 },
            { url = "https://files.pythonhosted.org/packages/3a/97/5edbed69a9d0caa2e4aa616ae7df8127e10f6586940aa683a496c2c280b9/kiwisolver-1.4.8-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:3a96c0e790ee875d65e340ab383700e2b4891677b7fcd30a699146f9384a2bb0", size = 1435109 },
            { url = "https://files.pythonhosted.org/packages/13/fc/e756382cb64e556af6c1809a1bbb22c141bbc2445049f2da06b420fe52bf/kiwisolver-1.4.8-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:23454ff084b07ac54ca8be535f4174170c1094a4cff78fbae4f73a4bcc0d4dab", size = 2245269 },
            { url = "https://files.pythonhosted.org/packages/76/15/e59e45829d7f41c776d138245cabae6515cb4eb44b418f6d4109c478b481/kiwisolver-1.4.8-cp311-cp311-musllinux_1_2_i686.whl", hash = "sha256:87b287251ad6488e95b4f0b4a79a6d04d3ea35fde6340eb38fbd1ca9cd35bbbc", size = 2393468 },
            { url = "https://files.pythonhosted.org/packages/e9/39/483558c2a913ab8384d6e4b66a932406f87c95a6080112433da5ed668559/kiwisolver-1.4.8-cp311-cp311-musllinux_1_2_ppc64le.whl", hash = "sha256:b21dbe165081142b1232a240fc6383fd32cdd877ca6cc89eab93e5f5883e1c25", size = 2355394 },
            { url = "https://files.pythonhosted.org/packages/01/aa/efad1fbca6570a161d29224f14b082960c7e08268a133fe5dc0f6906820e/kiwisolver-1.4.8-cp311-cp311-musllinux_1_2_s390x.whl", hash = "sha256:768cade2c2df13db52475bd28d3a3fac8c9eff04b0e9e2fda0f3760f20b3f7fc", size = 2490901 },
            { url = "https://files.pythonhosted.org/packages/c9/4f/15988966ba46bcd5ab9d0c8296914436720dd67fca689ae1a75b4ec1c72f/kiwisolver-1.4.8-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:d47cfb2650f0e103d4bf68b0b5804c68da97272c84bb12850d877a95c056bd67", size = 2312306 },
            { url = "https://files.pythonhosted.org/packages/2d/27/bdf1c769c83f74d98cbc34483a972f221440703054894a37d174fba8aa68/kiwisolver-1.4.8-cp311-cp311-win_amd64.whl", hash = "sha256:ed33ca2002a779a2e20eeb06aea7721b6e47f2d4b8a8ece979d8ba9e2a167e34", size = 71966 },
            { url = "https://files.pythonhosted.org/packages/4a/c9/9642ea855604aeb2968a8e145fc662edf61db7632ad2e4fb92424be6b6c0/kiwisolver-1.4.8-cp311-cp311-win_arm64.whl", hash = "sha256:16523b40aab60426ffdebe33ac374457cf62863e330a90a0383639ce14bf44b2", size = 65311 },
            { url = "https://files.pythonhosted.org/packages/fc/aa/cea685c4ab647f349c3bc92d2daf7ae34c8e8cf405a6dcd3a497f58a2ac3/kiwisolver-1.4.8-cp312-cp312-macosx_10_13_universal2.whl", hash = "sha256:d6af5e8815fd02997cb6ad9bbed0ee1e60014438ee1a5c2444c96f87b8843502", size = 124152 },
            { url = "https://files.pythonhosted.org/packages/c5/0b/8db6d2e2452d60d5ebc4ce4b204feeb16176a851fd42462f66ade6808084/kiwisolver-1.4.8-cp312-cp312-macosx_10_13_x86_64.whl", hash = "sha256:bade438f86e21d91e0cf5dd7c0ed00cda0f77c8c1616bd83f9fc157fa6760d31", size = 66555 },
            { url = "https://files.pythonhosted.org/packages/60/26/d6a0db6785dd35d3ba5bf2b2df0aedc5af089962c6eb2cbf67a15b81369e/kiwisolver-1.4.8-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:b83dc6769ddbc57613280118fb4ce3cd08899cc3369f7d0e0fab518a7cf37fdb", size = 65067 },
            { url = "https://files.pythonhosted.org/packages/c9/ed/1d97f7e3561e09757a196231edccc1bcf59d55ddccefa2afc9c615abd8e0/kiwisolver-1.4.8-cp312-cp312-manylinux_2_12_i686.manylinux2010_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:111793b232842991be367ed828076b03d96202c19221b5ebab421ce8bcad016f", size = 1378443 },
            { url = "https://files.pythonhosted.org/packages/29/61/39d30b99954e6b46f760e6289c12fede2ab96a254c443639052d1b573fbc/kiwisolver-1.4.8-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:257af1622860e51b1a9d0ce387bf5c2c4f36a90594cb9514f55b074bcc787cfc", size = 1472728 },
            { url = "https://files.pythonhosted.org/packages/0c/3e/804163b932f7603ef256e4a715e5843a9600802bb23a68b4e08c8c0ff61d/kiwisolver-1.4.8-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:69b5637c3f316cab1ec1c9a12b8c5f4750a4c4b71af9157645bf32830e39c03a", size = 1478388 },
            { url = "https://files.pythonhosted.org/packages/8a/9e/60eaa75169a154700be74f875a4d9961b11ba048bef315fbe89cb6999056/kiwisolver-1.4.8-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:782bb86f245ec18009890e7cb8d13a5ef54dcf2ebe18ed65f795e635a96a1c6a", size = 1413849 },
            { url = "https://files.pythonhosted.org/packages/bc/b3/9458adb9472e61a998c8c4d95cfdfec91c73c53a375b30b1428310f923e4/kiwisolver-1.4.8-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:cc978a80a0db3a66d25767b03688f1147a69e6237175c0f4ffffaaedf744055a", size = 1475533 },
            { url = "https://files.pythonhosted.org/packages/e4/7a/0a42d9571e35798de80aef4bb43a9b672aa7f8e58643d7bd1950398ffb0a/kiwisolver-1.4.8-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:36dbbfd34838500a31f52c9786990d00150860e46cd5041386f217101350f0d3", size = 2268898 },
            { url = "https://files.pythonhosted.org/packages/d9/07/1255dc8d80271400126ed8db35a1795b1a2c098ac3a72645075d06fe5c5d/kiwisolver-1.4.8-cp312-cp312-musllinux_1_2_i686.whl", hash = "sha256:eaa973f1e05131de5ff3569bbba7f5fd07ea0595d3870ed4a526d486fe57fa1b", size = 2425605 },
            { url = "https://files.pythonhosted.org/packages/84/df/5a3b4cf13780ef6f6942df67b138b03b7e79e9f1f08f57c49957d5867f6e/kiwisolver-1.4.8-cp312-cp312-musllinux_1_2_ppc64le.whl", hash = "sha256:a66f60f8d0c87ab7f59b6fb80e642ebb29fec354a4dfad687ca4092ae69d04f4", size = 2375801 },
            { url = "https://files.pythonhosted.org/packages/8f/10/2348d068e8b0f635c8c86892788dac7a6b5c0cb12356620ab575775aad89/kiwisolver-1.4.8-cp312-cp312-musllinux_1_2_s390x.whl", hash = "sha256:858416b7fb777a53f0c59ca08190ce24e9abbd3cffa18886a5781b8e3e26f65d", size = 2520077 },
            { url = "https://files.pythonhosted.org/packages/32/d8/014b89fee5d4dce157d814303b0fce4d31385a2af4c41fed194b173b81ac/kiwisolver-1.4.8-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:085940635c62697391baafaaeabdf3dd7a6c3643577dde337f4d66eba021b2b8", size = 2338410 },
            { url = "https://files.pythonhosted.org/packages/bd/72/dfff0cc97f2a0776e1c9eb5bef1ddfd45f46246c6533b0191887a427bca5/kiwisolver-1.4.8-cp312-cp312-win_amd64.whl", hash = "sha256:01c3d31902c7db5fb6182832713d3b4122ad9317c2c5877d0539227d96bb2e50", size = 71853 },
            { url = "https://files.pythonhosted.org/packages/dc/85/220d13d914485c0948a00f0b9eb419efaf6da81b7d72e88ce2391f7aed8d/kiwisolver-1.4.8-cp312-cp312-win_arm64.whl", hash = "sha256:a3c44cb68861de93f0c4a8175fbaa691f0aa22550c331fefef02b618a9dcb476", size = 65424 },
            { url = "https://files.pythonhosted.org/packages/79/b3/e62464a652f4f8cd9006e13d07abad844a47df1e6537f73ddfbf1bc997ec/kiwisolver-1.4.8-cp313-cp313-macosx_10_13_universal2.whl", hash = "sha256:1c8ceb754339793c24aee1c9fb2485b5b1f5bb1c2c214ff13368431e51fc9a09", size = 124156 },
            { url = "https://files.pythonhosted.org/packages/8d/2d/f13d06998b546a2ad4f48607a146e045bbe48030774de29f90bdc573df15/kiwisolver-1.4.8-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:54a62808ac74b5e55a04a408cda6156f986cefbcf0ada13572696b507cc92fa1", size = 66555 },
            { url = "https://files.pythonhosted.org/packages/59/e3/b8bd14b0a54998a9fd1e8da591c60998dc003618cb19a3f94cb233ec1511/kiwisolver-1.4.8-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:68269e60ee4929893aad82666821aaacbd455284124817af45c11e50a4b42e3c", size = 65071 },
            { url = "https://files.pythonhosted.org/packages/f0/1c/6c86f6d85ffe4d0ce04228d976f00674f1df5dc893bf2dd4f1928748f187/kiwisolver-1.4.8-cp313-cp313-manylinux_2_12_i686.manylinux2010_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:34d142fba9c464bc3bbfeff15c96eab0e7310343d6aefb62a79d51421fcc5f1b", size = 1378053 },
            { url = "https://files.pythonhosted.org/packages/4e/b9/1c6e9f6dcb103ac5cf87cb695845f5fa71379021500153566d8a8a9fc291/kiwisolver-1.4.8-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:3ddc373e0eef45b59197de815b1b28ef89ae3955e7722cc9710fb91cd77b7f47", size = 1472278 },
            { url = "https://files.pythonhosted.org/packages/ee/81/aca1eb176de671f8bda479b11acdc42c132b61a2ac861c883907dde6debb/kiwisolver-1.4.8-cp313-cp313-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:77e6f57a20b9bd4e1e2cedda4d0b986ebd0216236f0106e55c28aea3d3d69b16", size = 1478139 },
            { url = "https://files.pythonhosted.org/packages/49/f4/e081522473671c97b2687d380e9e4c26f748a86363ce5af48b4a28e48d06/kiwisolver-1.4.8-cp313-cp313-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:08e77738ed7538f036cd1170cbed942ef749137b1311fa2bbe2a7fda2f6bf3cc", size = 1413517 },
            { url = "https://files.pythonhosted.org/packages/8f/e9/6a7d025d8da8c4931522922cd706105aa32b3291d1add8c5427cdcd66e63/kiwisolver-1.4.8-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a5ce1e481a74b44dd5e92ff03ea0cb371ae7a0268318e202be06c8f04f4f1246", size = 1474952 },
            { url = "https://files.pythonhosted.org/packages/82/13/13fa685ae167bee5d94b415991c4fc7bb0a1b6ebea6e753a87044b209678/kiwisolver-1.4.8-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:fc2ace710ba7c1dfd1a3b42530b62b9ceed115f19a1656adefce7b1782a37794", size = 2269132 },
            { url = "https://files.pythonhosted.org/packages/ef/92/bb7c9395489b99a6cb41d502d3686bac692586db2045adc19e45ee64ed23/kiwisolver-1.4.8-cp313-cp313-musllinux_1_2_i686.whl", hash = "sha256:3452046c37c7692bd52b0e752b87954ef86ee2224e624ef7ce6cb21e8c41cc1b", size = 2425997 },
            { url = "https://files.pythonhosted.org/packages/ed/12/87f0e9271e2b63d35d0d8524954145837dd1a6c15b62a2d8c1ebe0f182b4/kiwisolver-1.4.8-cp313-cp313-musllinux_1_2_ppc64le.whl", hash = "sha256:7e9a60b50fe8b2ec6f448fe8d81b07e40141bfced7f896309df271a0b92f80f3", size = 2376060 },
            { url = "https://files.pythonhosted.org/packages/02/6e/c8af39288edbce8bf0fa35dee427b082758a4b71e9c91ef18fa667782138/kiwisolver-1.4.8-cp313-cp313-musllinux_1_2_s390x.whl", hash = "sha256:918139571133f366e8362fa4a297aeba86c7816b7ecf0bc79168080e2bd79957", size = 2520471 },
            { url = "https://files.pythonhosted.org/packages/13/78/df381bc7b26e535c91469f77f16adcd073beb3e2dd25042efd064af82323/kiwisolver-1.4.8-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:e063ef9f89885a1d68dd8b2e18f5ead48653176d10a0e324e3b0030e3a69adeb", size = 2338793 },
            { url = "https://files.pythonhosted.org/packages/d0/dc/c1abe38c37c071d0fc71c9a474fd0b9ede05d42f5a458d584619cfd2371a/kiwisolver-1.4.8-cp313-cp313-win_amd64.whl", hash = "sha256:a17b7c4f5b2c51bb68ed379defd608a03954a1845dfed7cc0117f1cc8a9b7fd2", size = 71855 },
            { url = "https://files.pythonhosted.org/packages/a0/b6/21529d595b126ac298fdd90b705d87d4c5693de60023e0efcb4f387ed99e/kiwisolver-1.4.8-cp313-cp313-win_arm64.whl", hash = "sha256:3cd3bc628b25f74aedc6d374d5babf0166a92ff1317f46267f12d2ed54bc1d30", size = 65430 },
            { url = "https://files.pythonhosted.org/packages/34/bd/b89380b7298e3af9b39f49334e3e2a4af0e04819789f04b43d560516c0c8/kiwisolver-1.4.8-cp313-cp313t-macosx_10_13_universal2.whl", hash = "sha256:370fd2df41660ed4e26b8c9d6bbcad668fbe2560462cba151a721d49e5b6628c", size = 126294 },
            { url = "https://files.pythonhosted.org/packages/83/41/5857dc72e5e4148eaac5aa76e0703e594e4465f8ab7ec0fc60e3a9bb8fea/kiwisolver-1.4.8-cp313-cp313t-macosx_10_13_x86_64.whl", hash = "sha256:84a2f830d42707de1d191b9490ac186bf7997a9495d4e9072210a1296345f7dc", size = 67736 },
            { url = "https://files.pythonhosted.org/packages/e1/d1/be059b8db56ac270489fb0b3297fd1e53d195ba76e9bbb30e5401fa6b759/kiwisolver-1.4.8-cp313-cp313t-macosx_11_0_arm64.whl", hash = "sha256:7a3ad337add5148cf51ce0b55642dc551c0b9d6248458a757f98796ca7348712", size = 66194 },
            { url = "https://files.pythonhosted.org/packages/e1/83/4b73975f149819eb7dcf9299ed467eba068ecb16439a98990dcb12e63fdd/kiwisolver-1.4.8-cp313-cp313t-manylinux_2_12_i686.manylinux2010_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:7506488470f41169b86d8c9aeff587293f530a23a23a49d6bc64dab66bedc71e", size = 1465942 },
            { url = "https://files.pythonhosted.org/packages/c7/2c/30a5cdde5102958e602c07466bce058b9d7cb48734aa7a4327261ac8e002/kiwisolver-1.4.8-cp313-cp313t-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:2f0121b07b356a22fb0414cec4666bbe36fd6d0d759db3d37228f496ed67c880", size = 1595341 },
            { url = "https://files.pythonhosted.org/packages/ff/9b/1e71db1c000385aa069704f5990574b8244cce854ecd83119c19e83c9586/kiwisolver-1.4.8-cp313-cp313t-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:d6d6bd87df62c27d4185de7c511c6248040afae67028a8a22012b010bc7ad062", size = 1598455 },
            { url = "https://files.pythonhosted.org/packages/85/92/c8fec52ddf06231b31cbb779af77e99b8253cd96bd135250b9498144c78b/kiwisolver-1.4.8-cp313-cp313t-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:291331973c64bb9cce50bbe871fb2e675c4331dab4f31abe89f175ad7679a4d7", size = 1522138 },
            { url = "https://files.pythonhosted.org/packages/0b/51/9eb7e2cd07a15d8bdd976f6190c0164f92ce1904e5c0c79198c4972926b7/kiwisolver-1.4.8-cp313-cp313t-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:893f5525bb92d3d735878ec00f781b2de998333659507d29ea4466208df37bed", size = 1582857 },
            { url = "https://files.pythonhosted.org/packages/0f/95/c5a00387a5405e68ba32cc64af65ce881a39b98d73cc394b24143bebc5b8/kiwisolver-1.4.8-cp313-cp313t-musllinux_1_2_aarch64.whl", hash = "sha256:b47a465040146981dc9db8647981b8cb96366fbc8d452b031e4f8fdffec3f26d", size = 2293129 },
            { url = "https://files.pythonhosted.org/packages/44/83/eeb7af7d706b8347548313fa3a3a15931f404533cc54fe01f39e830dd231/kiwisolver-1.4.8-cp313-cp313t-musllinux_1_2_i686.whl", hash = "sha256:99cea8b9dd34ff80c521aef46a1dddb0dcc0283cf18bde6d756f1e6f31772165", size = 2421538 },
            { url = "https://files.pythonhosted.org/packages/05/f9/27e94c1b3eb29e6933b6986ffc5fa1177d2cd1f0c8efc5f02c91c9ac61de/kiwisolver-1.4.8-cp313-cp313t-musllinux_1_2_ppc64le.whl", hash = "sha256:151dffc4865e5fe6dafce5480fab84f950d14566c480c08a53c663a0020504b6", size = 2390661 },
            { url = "https://files.pythonhosted.org/packages/d9/d4/3c9735faa36ac591a4afcc2980d2691000506050b7a7e80bcfe44048daa7/kiwisolver-1.4.8-cp313-cp313t-musllinux_1_2_s390x.whl", hash = "sha256:577facaa411c10421314598b50413aa1ebcf5126f704f1e5d72d7e4e9f020d90", size = 2546710 },
            { url = "https://files.pythonhosted.org/packages/4c/fa/be89a49c640930180657482a74970cdcf6f7072c8d2471e1babe17a222dc/kiwisolver-1.4.8-cp313-cp313t-musllinux_1_2_x86_64.whl", hash = "sha256:be4816dc51c8a471749d664161b434912eee82f2ea66bd7628bd14583a833e85", size = 2349213 },
            { url = "https://files.pythonhosted.org/packages/1f/f9/ae81c47a43e33b93b0a9819cac6723257f5da2a5a60daf46aa5c7226ea85/kiwisolver-1.4.8-pp310-pypy310_pp73-macosx_10_15_x86_64.whl", hash = "sha256:e7a019419b7b510f0f7c9dceff8c5eae2392037eae483a7f9162625233802b0a", size = 60403 },
            { url = "https://files.pythonhosted.org/packages/58/ca/f92b5cb6f4ce0c1ebfcfe3e2e42b96917e16f7090e45b21102941924f18f/kiwisolver-1.4.8-pp310-pypy310_pp73-macosx_11_0_arm64.whl", hash = "sha256:286b18e86682fd2217a48fc6be6b0f20c1d0ed10958d8dc53453ad58d7be0bf8", size = 58657 },
            { url = "https://files.pythonhosted.org/packages/80/28/ae0240f732f0484d3a4dc885d055653c47144bdf59b670aae0ec3c65a7c8/kiwisolver-1.4.8-pp310-pypy310_pp73-manylinux_2_12_i686.manylinux2010_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4191ee8dfd0be1c3666ccbac178c5a05d5f8d689bbe3fc92f3c4abec817f8fe0", size = 84948 },
            { url = "https://files.pythonhosted.org/packages/5d/eb/78d50346c51db22c7203c1611f9b513075f35c4e0e4877c5dde378d66043/kiwisolver-1.4.8-pp310-pypy310_pp73-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:7cd2785b9391f2873ad46088ed7599a6a71e762e1ea33e87514b1a441ed1da1c", size = 81186 },
            { url = "https://files.pythonhosted.org/packages/43/f8/7259f18c77adca88d5f64f9a522792e178b2691f3748817a8750c2d216ef/kiwisolver-1.4.8-pp310-pypy310_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c07b29089b7ba090b6f1a669f1411f27221c3662b3a1b7010e67b59bb5a6f10b", size = 80279 },
            { url = "https://files.pythonhosted.org/packages/3a/1d/50ad811d1c5dae091e4cf046beba925bcae0a610e79ae4c538f996f63ed5/kiwisolver-1.4.8-pp310-pypy310_pp73-win_amd64.whl", hash = "sha256:65ea09a5a3faadd59c2ce96dc7bf0f364986a315949dc6374f04396b0d60e09b", size = 71762 },
        ]

        [[package]]
        name = "latexcodec"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/e7/ed339caf3662976949e4fdbfdf4a6db818b8d2aa1cf2b5f73af89e936bba/latexcodec-3.0.0.tar.gz", hash = "sha256:917dc5fe242762cc19d963e6548b42d63a118028cdd3361d62397e3b638b6bc5", size = 31023 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b0/bf/ea8887e9f31a8f93ca306699d11909c6140151393a4216f0d9f85a004077/latexcodec-3.0.0-py3-none-any.whl", hash = "sha256:6f3477ad5e61a0a99bd31a6a370c34e88733a6bad9c921a3ffcfacada12f41a7", size = 18150 },
        ]

        [[package]]
        name = "lightning"
        version = "2.5.0.post0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "fsspec", extra = ["http"] },
            { name = "lightning-utilities" },
            { name = "packaging" },
            { name = "pytorch-lightning" },
            { name = "pyyaml" },
            { name = "torch", version = "2.2.1", source = { registry = "https://pypi.org/simple" } },
            { name = "torchmetrics" },
            { name = "tqdm" },
            { name = "typing-extensions" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e6/6d/d9d209d9ab3d270ee89afb8f71a3d84d48d41a1880e89d8215d26e8018b7/lightning-2.5.0.post0.tar.gz", hash = "sha256:f720fe4f6d03a7f15f9aef3112c5a0d1eafd8d27b903f4a1354b609685b2ec70", size = 628132 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d4/54/2de887e964a561776ac49c5e715ca8a56138ea7e2b95325e0a79aa46a070/lightning-2.5.0.post0-py3-none-any.whl", hash = "sha256:b08463326e6fb39cb3e4db8ff2660a80ce3372a0688c80e3370c091346ea220c", size = 815241 },
        ]

        [[package]]
        name = "lightning-utilities"
        version = "0.12.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "packaging" },
            { name = "setuptools" },
            { name = "typing-extensions" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d5/26/a449b858a6beaaf779d56775a5c675d636af11e32004e4420506a48eb7f4/lightning_utilities-0.12.0.tar.gz", hash = "sha256:95b5f22a0b69eb27ca0929c6c1d510592a70080e1733a055bf154903c0343b60", size = 29677 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/50/8d/da77ceb92ed674da93959184a2777d08ccbd872559fb52aba16b91686b7e/lightning_utilities-0.12.0-py3-none-any.whl", hash = "sha256:b827f5768607e81ccc7b2ada1f50628168d1cc9f839509c7e87c04b59079e66c", size = 28487 },
        ]

        [[package]]
        name = "mace-torch"
        version = "0.3.9"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "ase" },
            { name = "configargparse" },
            { name = "cuequivariance-torch" },
            { name = "e3nn" },
            { name = "gitpython" },
            { name = "h5py" },
            { name = "matplotlib" },
            { name = "matscipy" },
            { name = "numpy" },
            { name = "opt-einsum" },
            { name = "pandas" },
            { name = "prettytable" },
            { name = "python-hostlist" },
            { name = "pyyaml" },
            { name = "torch", version = "2.2.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "torch", version = "2.5.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
            { name = "torch-ema" },
            { name = "torchmetrics" },
            { name = "tqdm" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/66/5b/aeb33e4a7f93f13350430725f1ac70844d97da2db3e8623f1e3544dca8ef/mace-torch-0.3.9.tar.gz", hash = "sha256:0bfe0037795ee68b385f3fc9dc07b502f2675102715d113bf1c2dad30536e602", size = 131183 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f8/d9/e1d4c461d059a8a6f8ef8644fb7b065970efef426a0bc46ed701940b59de/mace_torch-0.3.9-py3-none-any.whl", hash = "sha256:a37adb2c28a97f002abbe5ba0b397d778f358c1e6b77c3dbf3f133730be83cf8", size = 152776 },
        ]

        [[package]]
        name = "markupsafe"
        version = "3.0.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b2/97/5d42485e71dfc078108a86d6de8fa46db44a1a9295e89c5d6d4a06e23a62/markupsafe-3.0.2.tar.gz", hash = "sha256:ee55d3edf80167e48ea11a923c7386f4669df67d7994554387f84e7d8b0a2bf0", size = 20537 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/04/90/d08277ce111dd22f77149fd1a5d4653eeb3b3eaacbdfcbae5afb2600eebd/MarkupSafe-3.0.2-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:7e94c425039cde14257288fd61dcfb01963e658efbc0ff54f5306b06054700f8", size = 14357 },
            { url = "https://files.pythonhosted.org/packages/04/e1/6e2194baeae0bca1fae6629dc0cbbb968d4d941469cbab11a3872edff374/MarkupSafe-3.0.2-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:9e2d922824181480953426608b81967de705c3cef4d1af983af849d7bd619158", size = 12393 },
            { url = "https://files.pythonhosted.org/packages/1d/69/35fa85a8ece0a437493dc61ce0bb6d459dcba482c34197e3efc829aa357f/MarkupSafe-3.0.2-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:38a9ef736c01fccdd6600705b09dc574584b89bea478200c5fbf112a6b0d5579", size = 21732 },
            { url = "https://files.pythonhosted.org/packages/22/35/137da042dfb4720b638d2937c38a9c2df83fe32d20e8c8f3185dbfef05f7/MarkupSafe-3.0.2-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:bbcb445fa71794da8f178f0f6d66789a28d7319071af7a496d4d507ed566270d", size = 20866 },
            { url = "https://files.pythonhosted.org/packages/29/28/6d029a903727a1b62edb51863232152fd335d602def598dade38996887f0/MarkupSafe-3.0.2-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:57cb5a3cf367aeb1d316576250f65edec5bb3be939e9247ae594b4bcbc317dfb", size = 20964 },
            { url = "https://files.pythonhosted.org/packages/cc/cd/07438f95f83e8bc028279909d9c9bd39e24149b0d60053a97b2bc4f8aa51/MarkupSafe-3.0.2-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:3809ede931876f5b2ec92eef964286840ed3540dadf803dd570c3b7e13141a3b", size = 21977 },
            { url = "https://files.pythonhosted.org/packages/29/01/84b57395b4cc062f9c4c55ce0df7d3108ca32397299d9df00fedd9117d3d/MarkupSafe-3.0.2-cp310-cp310-musllinux_1_2_i686.whl", hash = "sha256:e07c3764494e3776c602c1e78e298937c3315ccc9043ead7e685b7f2b8d47b3c", size = 21366 },
            { url = "https://files.pythonhosted.org/packages/bd/6e/61ebf08d8940553afff20d1fb1ba7294b6f8d279df9fd0c0db911b4bbcfd/MarkupSafe-3.0.2-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:b424c77b206d63d500bcb69fa55ed8d0e6a3774056bdc4839fc9298a7edca171", size = 21091 },
            { url = "https://files.pythonhosted.org/packages/11/23/ffbf53694e8c94ebd1e7e491de185124277964344733c45481f32ede2499/MarkupSafe-3.0.2-cp310-cp310-win32.whl", hash = "sha256:fcabf5ff6eea076f859677f5f0b6b5c1a51e70a376b0579e0eadef8db48c6b50", size = 15065 },
            { url = "https://files.pythonhosted.org/packages/44/06/e7175d06dd6e9172d4a69a72592cb3f7a996a9c396eee29082826449bbc3/MarkupSafe-3.0.2-cp310-cp310-win_amd64.whl", hash = "sha256:6af100e168aa82a50e186c82875a5893c5597a0c1ccdb0d8b40240b1f28b969a", size = 15514 },
            { url = "https://files.pythonhosted.org/packages/6b/28/bbf83e3f76936960b850435576dd5e67034e200469571be53f69174a2dfd/MarkupSafe-3.0.2-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:9025b4018f3a1314059769c7bf15441064b2207cb3f065e6ea1e7359cb46db9d", size = 14353 },
            { url = "https://files.pythonhosted.org/packages/6c/30/316d194b093cde57d448a4c3209f22e3046c5bb2fb0820b118292b334be7/MarkupSafe-3.0.2-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:93335ca3812df2f366e80509ae119189886b0f3c2b81325d39efdb84a1e2ae93", size = 12392 },
            { url = "https://files.pythonhosted.org/packages/f2/96/9cdafba8445d3a53cae530aaf83c38ec64c4d5427d975c974084af5bc5d2/MarkupSafe-3.0.2-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:2cb8438c3cbb25e220c2ab33bb226559e7afb3baec11c4f218ffa7308603c832", size = 23984 },
            { url = "https://files.pythonhosted.org/packages/f1/a4/aefb044a2cd8d7334c8a47d3fb2c9f328ac48cb349468cc31c20b539305f/MarkupSafe-3.0.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a123e330ef0853c6e822384873bef7507557d8e4a082961e1defa947aa59ba84", size = 23120 },
            { url = "https://files.pythonhosted.org/packages/8d/21/5e4851379f88f3fad1de30361db501300d4f07bcad047d3cb0449fc51f8c/MarkupSafe-3.0.2-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:1e084f686b92e5b83186b07e8a17fc09e38fff551f3602b249881fec658d3eca", size = 23032 },
            { url = "https://files.pythonhosted.org/packages/00/7b/e92c64e079b2d0d7ddf69899c98842f3f9a60a1ae72657c89ce2655c999d/MarkupSafe-3.0.2-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:d8213e09c917a951de9d09ecee036d5c7d36cb6cb7dbaece4c71a60d79fb9798", size = 24057 },
            { url = "https://files.pythonhosted.org/packages/f9/ac/46f960ca323037caa0a10662ef97d0a4728e890334fc156b9f9e52bcc4ca/MarkupSafe-3.0.2-cp311-cp311-musllinux_1_2_i686.whl", hash = "sha256:5b02fb34468b6aaa40dfc198d813a641e3a63b98c2b05a16b9f80b7ec314185e", size = 23359 },
            { url = "https://files.pythonhosted.org/packages/69/84/83439e16197337b8b14b6a5b9c2105fff81d42c2a7c5b58ac7b62ee2c3b1/MarkupSafe-3.0.2-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:0bff5e0ae4ef2e1ae4fdf2dfd5b76c75e5c2fa4132d05fc1b0dabcd20c7e28c4", size = 23306 },
            { url = "https://files.pythonhosted.org/packages/9a/34/a15aa69f01e2181ed8d2b685c0d2f6655d5cca2c4db0ddea775e631918cd/MarkupSafe-3.0.2-cp311-cp311-win32.whl", hash = "sha256:6c89876f41da747c8d3677a2b540fb32ef5715f97b66eeb0c6b66f5e3ef6f59d", size = 15094 },
            { url = "https://files.pythonhosted.org/packages/da/b8/3a3bd761922d416f3dc5d00bfbed11f66b1ab89a0c2b6e887240a30b0f6b/MarkupSafe-3.0.2-cp311-cp311-win_amd64.whl", hash = "sha256:70a87b411535ccad5ef2f1df5136506a10775d267e197e4cf531ced10537bd6b", size = 15521 },
            { url = "https://files.pythonhosted.org/packages/22/09/d1f21434c97fc42f09d290cbb6350d44eb12f09cc62c9476effdb33a18aa/MarkupSafe-3.0.2-cp312-cp312-macosx_10_13_universal2.whl", hash = "sha256:9778bd8ab0a994ebf6f84c2b949e65736d5575320a17ae8984a77fab08db94cf", size = 14274 },
            { url = "https://files.pythonhosted.org/packages/6b/b0/18f76bba336fa5aecf79d45dcd6c806c280ec44538b3c13671d49099fdd0/MarkupSafe-3.0.2-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:846ade7b71e3536c4e56b386c2a47adf5741d2d8b94ec9dc3e92e5e1ee1e2225", size = 12348 },
            { url = "https://files.pythonhosted.org/packages/e0/25/dd5c0f6ac1311e9b40f4af06c78efde0f3b5cbf02502f8ef9501294c425b/MarkupSafe-3.0.2-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:1c99d261bd2d5f6b59325c92c73df481e05e57f19837bdca8413b9eac4bd8028", size = 24149 },
            { url = "https://files.pythonhosted.org/packages/f3/f0/89e7aadfb3749d0f52234a0c8c7867877876e0a20b60e2188e9850794c17/MarkupSafe-3.0.2-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:e17c96c14e19278594aa4841ec148115f9c7615a47382ecb6b82bd8fea3ab0c8", size = 23118 },
            { url = "https://files.pythonhosted.org/packages/d5/da/f2eeb64c723f5e3777bc081da884b414671982008c47dcc1873d81f625b6/MarkupSafe-3.0.2-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:88416bd1e65dcea10bc7569faacb2c20ce071dd1f87539ca2ab364bf6231393c", size = 22993 },
            { url = "https://files.pythonhosted.org/packages/da/0e/1f32af846df486dce7c227fe0f2398dc7e2e51d4a370508281f3c1c5cddc/MarkupSafe-3.0.2-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:2181e67807fc2fa785d0592dc2d6206c019b9502410671cc905d132a92866557", size = 24178 },
            { url = "https://files.pythonhosted.org/packages/c4/f6/bb3ca0532de8086cbff5f06d137064c8410d10779c4c127e0e47d17c0b71/MarkupSafe-3.0.2-cp312-cp312-musllinux_1_2_i686.whl", hash = "sha256:52305740fe773d09cffb16f8ed0427942901f00adedac82ec8b67752f58a1b22", size = 23319 },
            { url = "https://files.pythonhosted.org/packages/a2/82/8be4c96ffee03c5b4a034e60a31294daf481e12c7c43ab8e34a1453ee48b/MarkupSafe-3.0.2-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:ad10d3ded218f1039f11a75f8091880239651b52e9bb592ca27de44eed242a48", size = 23352 },
            { url = "https://files.pythonhosted.org/packages/51/ae/97827349d3fcffee7e184bdf7f41cd6b88d9919c80f0263ba7acd1bbcb18/MarkupSafe-3.0.2-cp312-cp312-win32.whl", hash = "sha256:0f4ca02bea9a23221c0182836703cbf8930c5e9454bacce27e767509fa286a30", size = 15097 },
            { url = "https://files.pythonhosted.org/packages/c1/80/a61f99dc3a936413c3ee4e1eecac96c0da5ed07ad56fd975f1a9da5bc630/MarkupSafe-3.0.2-cp312-cp312-win_amd64.whl", hash = "sha256:8e06879fc22a25ca47312fbe7c8264eb0b662f6db27cb2d3bbbc74b1df4b9b87", size = 15601 },
            { url = "https://files.pythonhosted.org/packages/83/0e/67eb10a7ecc77a0c2bbe2b0235765b98d164d81600746914bebada795e97/MarkupSafe-3.0.2-cp313-cp313-macosx_10_13_universal2.whl", hash = "sha256:ba9527cdd4c926ed0760bc301f6728ef34d841f405abf9d4f959c478421e4efd", size = 14274 },
            { url = "https://files.pythonhosted.org/packages/2b/6d/9409f3684d3335375d04e5f05744dfe7e9f120062c9857df4ab490a1031a/MarkupSafe-3.0.2-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:f8b3d067f2e40fe93e1ccdd6b2e1d16c43140e76f02fb1319a05cf2b79d99430", size = 12352 },
            { url = "https://files.pythonhosted.org/packages/d2/f5/6eadfcd3885ea85fe2a7c128315cc1bb7241e1987443d78c8fe712d03091/MarkupSafe-3.0.2-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:569511d3b58c8791ab4c2e1285575265991e6d8f8700c7be0e88f86cb0672094", size = 24122 },
            { url = "https://files.pythonhosted.org/packages/0c/91/96cf928db8236f1bfab6ce15ad070dfdd02ed88261c2afafd4b43575e9e9/MarkupSafe-3.0.2-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:15ab75ef81add55874e7ab7055e9c397312385bd9ced94920f2802310c930396", size = 23085 },
            { url = "https://files.pythonhosted.org/packages/c2/cf/c9d56af24d56ea04daae7ac0940232d31d5a8354f2b457c6d856b2057d69/MarkupSafe-3.0.2-cp313-cp313-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:f3818cb119498c0678015754eba762e0d61e5b52d34c8b13d770f0719f7b1d79", size = 22978 },
            { url = "https://files.pythonhosted.org/packages/2a/9f/8619835cd6a711d6272d62abb78c033bda638fdc54c4e7f4272cf1c0962b/MarkupSafe-3.0.2-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:cdb82a876c47801bb54a690c5ae105a46b392ac6099881cdfb9f6e95e4014c6a", size = 24208 },
            { url = "https://files.pythonhosted.org/packages/f9/bf/176950a1792b2cd2102b8ffeb5133e1ed984547b75db47c25a67d3359f77/MarkupSafe-3.0.2-cp313-cp313-musllinux_1_2_i686.whl", hash = "sha256:cabc348d87e913db6ab4aa100f01b08f481097838bdddf7c7a84b7575b7309ca", size = 23357 },
            { url = "https://files.pythonhosted.org/packages/ce/4f/9a02c1d335caabe5c4efb90e1b6e8ee944aa245c1aaaab8e8a618987d816/MarkupSafe-3.0.2-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:444dcda765c8a838eaae23112db52f1efaf750daddb2d9ca300bcae1039adc5c", size = 23344 },
            { url = "https://files.pythonhosted.org/packages/ee/55/c271b57db36f748f0e04a759ace9f8f759ccf22b4960c270c78a394f58be/MarkupSafe-3.0.2-cp313-cp313-win32.whl", hash = "sha256:bcf3e58998965654fdaff38e58584d8937aa3096ab5354d493c77d1fdd66d7a1", size = 15101 },
            { url = "https://files.pythonhosted.org/packages/29/88/07df22d2dd4df40aba9f3e402e6dc1b8ee86297dddbad4872bd5e7b0094f/MarkupSafe-3.0.2-cp313-cp313-win_amd64.whl", hash = "sha256:e6a2a455bd412959b57a172ce6328d2dd1f01cb2135efda2e4576e8a23fa3b0f", size = 15603 },
            { url = "https://files.pythonhosted.org/packages/62/6a/8b89d24db2d32d433dffcd6a8779159da109842434f1dd2f6e71f32f738c/MarkupSafe-3.0.2-cp313-cp313t-macosx_10_13_universal2.whl", hash = "sha256:b5a6b3ada725cea8a5e634536b1b01c30bcdcd7f9c6fff4151548d5bf6b3a36c", size = 14510 },
            { url = "https://files.pythonhosted.org/packages/7a/06/a10f955f70a2e5a9bf78d11a161029d278eeacbd35ef806c3fd17b13060d/MarkupSafe-3.0.2-cp313-cp313t-macosx_11_0_arm64.whl", hash = "sha256:a904af0a6162c73e3edcb969eeeb53a63ceeb5d8cf642fade7d39e7963a22ddb", size = 12486 },
            { url = "https://files.pythonhosted.org/packages/34/cf/65d4a571869a1a9078198ca28f39fba5fbb910f952f9dbc5220afff9f5e6/MarkupSafe-3.0.2-cp313-cp313t-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:4aa4e5faecf353ed117801a068ebab7b7e09ffb6e1d5e412dc852e0da018126c", size = 25480 },
            { url = "https://files.pythonhosted.org/packages/0c/e3/90e9651924c430b885468b56b3d597cabf6d72be4b24a0acd1fa0e12af67/MarkupSafe-3.0.2-cp313-cp313t-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c0ef13eaeee5b615fb07c9a7dadb38eac06a0608b41570d8ade51c56539e509d", size = 23914 },
            { url = "https://files.pythonhosted.org/packages/66/8c/6c7cf61f95d63bb866db39085150df1f2a5bd3335298f14a66b48e92659c/MarkupSafe-3.0.2-cp313-cp313t-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:d16a81a06776313e817c951135cf7340a3e91e8c1ff2fac444cfd75fffa04afe", size = 23796 },
            { url = "https://files.pythonhosted.org/packages/bb/35/cbe9238ec3f47ac9a7c8b3df7a808e7cb50fe149dc7039f5f454b3fba218/MarkupSafe-3.0.2-cp313-cp313t-musllinux_1_2_aarch64.whl", hash = "sha256:6381026f158fdb7c72a168278597a5e3a5222e83ea18f543112b2662a9b699c5", size = 25473 },
            { url = "https://files.pythonhosted.org/packages/e6/32/7621a4382488aa283cc05e8984a9c219abad3bca087be9ec77e89939ded9/MarkupSafe-3.0.2-cp313-cp313t-musllinux_1_2_i686.whl", hash = "sha256:3d79d162e7be8f996986c064d1c7c817f6df3a77fe3d6859f6f9e7be4b8c213a", size = 24114 },
            { url = "https://files.pythonhosted.org/packages/0d/80/0985960e4b89922cb5a0bac0ed39c5b96cbc1a536a99f30e8c220a996ed9/MarkupSafe-3.0.2-cp313-cp313t-musllinux_1_2_x86_64.whl", hash = "sha256:131a3c7689c85f5ad20f9f6fb1b866f402c445b220c19fe4308c0b147ccd2ad9", size = 24098 },
            { url = "https://files.pythonhosted.org/packages/82/78/fedb03c7d5380df2427038ec8d973587e90561b2d90cd472ce9254cf348b/MarkupSafe-3.0.2-cp313-cp313t-win32.whl", hash = "sha256:ba8062ed2cf21c07a9e295d5b8a2a5ce678b913b45fdf68c32d95d6c1291e0b6", size = 15208 },
            { url = "https://files.pythonhosted.org/packages/4f/65/6079a46068dfceaeabb5dcad6d674f5f5c61a6fa5673746f42a9f4c233b3/MarkupSafe-3.0.2-cp313-cp313t-win_amd64.whl", hash = "sha256:e444a31f8db13eb18ada366ab3cf45fd4b31e4db1236a4448f68778c1d1a5a2f", size = 15739 },
        ]

        [[package]]
        name = "matgl"
        version = "1.1.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "ase" },
            { name = "dgl" },
            { name = "lightning" },
            { name = "pydantic" },
            { name = "pymatgen" },
            { name = "torch", version = "2.2.1", source = { registry = "https://pypi.org/simple" } },
            { name = "torchdata" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/64/d6/0ad42a2efe7bc799df0bf4805cc90866f35aba2bc366de7372c2202d8e37/matgl-1.1.3.tar.gz", hash = "sha256:6b09b3a7a68a1f6fe47bed8c7408540353e46371e32c1a1ac2bde347a6885a14", size = 215012 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b9/48/33e646984c473c168528ab34af957f8346b47dfc5edb539210bacbcc31a4/matgl-1.1.3-py3-none-any.whl", hash = "sha256:edd966d8bc2fe3b78ea920aee80b33eeb4799bda4631068396c73490cf370ba9", size = 223267 },
        ]

        [[package]]
        name = "matplotlib"
        version = "3.10.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "contourpy" },
            { name = "cycler" },
            { name = "fonttools" },
            { name = "kiwisolver" },
            { name = "numpy" },
            { name = "packaging" },
            { name = "pillow" },
            { name = "pyparsing" },
            { name = "python-dateutil" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/68/dd/fa2e1a45fce2d09f4aea3cee169760e672c8262325aa5796c49d543dc7e6/matplotlib-3.10.0.tar.gz", hash = "sha256:b886d02a581b96704c9d1ffe55709e49b4d2d52709ccebc4be42db856e511278", size = 36686418 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/09/ec/3cdff7b5239adaaacefcc4f77c316dfbbdf853c4ed2beec467e0fec31b9f/matplotlib-3.10.0-cp310-cp310-macosx_10_12_x86_64.whl", hash = "sha256:2c5829a5a1dd5a71f0e31e6e8bb449bc0ee9dbfb05ad28fc0c6b55101b3a4be6", size = 8160551 },
            { url = "https://files.pythonhosted.org/packages/41/f2/b518f2c7f29895c9b167bf79f8529c63383ae94eaf49a247a4528e9a148d/matplotlib-3.10.0-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:a2a43cbefe22d653ab34bb55d42384ed30f611bcbdea1f8d7f431011a2e1c62e", size = 8034853 },
            { url = "https://files.pythonhosted.org/packages/ed/8d/45754b4affdb8f0d1a44e4e2bcd932cdf35b256b60d5eda9f455bb293ed0/matplotlib-3.10.0-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:607b16c8a73943df110f99ee2e940b8a1cbf9714b65307c040d422558397dac5", size = 8446724 },
            { url = "https://files.pythonhosted.org/packages/09/5a/a113495110ae3e3395c72d82d7bc4802902e46dc797f6b041e572f195c56/matplotlib-3.10.0-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:01d2b19f13aeec2e759414d3bfe19ddfb16b13a1250add08d46d5ff6f9be83c6", size = 8583905 },
            { url = "https://files.pythonhosted.org/packages/12/b1/8b1655b4c9ed4600c817c419f7eaaf70082630efd7556a5b2e77a8a3cdaf/matplotlib-3.10.0-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:5e6c6461e1fc63df30bf6f80f0b93f5b6784299f721bc28530477acd51bfc3d1", size = 9395223 },
            { url = "https://files.pythonhosted.org/packages/5a/85/b9a54d64585a6b8737a78a61897450403c30f39e0bd3214270bb0b96f002/matplotlib-3.10.0-cp310-cp310-win_amd64.whl", hash = "sha256:994c07b9d9fe8d25951e3202a68c17900679274dadfc1248738dcfa1bd40d7f3", size = 8025355 },
            { url = "https://files.pythonhosted.org/packages/0c/f1/e37f6c84d252867d7ddc418fff70fc661cfd363179263b08e52e8b748e30/matplotlib-3.10.0-cp311-cp311-macosx_10_12_x86_64.whl", hash = "sha256:fd44fc75522f58612ec4a33958a7e5552562b7705b42ef1b4f8c0818e304a363", size = 8171677 },
            { url = "https://files.pythonhosted.org/packages/c7/8b/92e9da1f28310a1f6572b5c55097b0c0ceb5e27486d85fb73b54f5a9b939/matplotlib-3.10.0-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:c58a9622d5dbeb668f407f35f4e6bfac34bb9ecdcc81680c04d0258169747997", size = 8044945 },
            { url = "https://files.pythonhosted.org/packages/c5/cb/49e83f0fd066937a5bd3bc5c5d63093703f3637b2824df8d856e0558beef/matplotlib-3.10.0-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:845d96568ec873be63f25fa80e9e7fae4be854a66a7e2f0c8ccc99e94a8bd4ef", size = 8458269 },
            { url = "https://files.pythonhosted.org/packages/b2/7d/2d873209536b9ee17340754118a2a17988bc18981b5b56e6715ee07373ac/matplotlib-3.10.0-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:5439f4c5a3e2e8eab18e2f8c3ef929772fd5641876db71f08127eed95ab64683", size = 8599369 },
            { url = "https://files.pythonhosted.org/packages/b8/03/57d6cbbe85c61fe4cbb7c94b54dce443d68c21961830833a1f34d056e5ea/matplotlib-3.10.0-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:4673ff67a36152c48ddeaf1135e74ce0d4bce1bbf836ae40ed39c29edf7e2765", size = 9405992 },
            { url = "https://files.pythonhosted.org/packages/14/cf/e382598f98be11bf51dd0bc60eca44a517f6793e3dc8b9d53634a144620c/matplotlib-3.10.0-cp311-cp311-win_amd64.whl", hash = "sha256:7e8632baebb058555ac0cde75db885c61f1212e47723d63921879806b40bec6a", size = 8034580 },
            { url = "https://files.pythonhosted.org/packages/44/c7/6b2d8cb7cc251d53c976799cacd3200add56351c175ba89ab9cbd7c1e68a/matplotlib-3.10.0-cp312-cp312-macosx_10_13_x86_64.whl", hash = "sha256:4659665bc7c9b58f8c00317c3c2a299f7f258eeae5a5d56b4c64226fca2f7c59", size = 8172465 },
            { url = "https://files.pythonhosted.org/packages/42/2a/6d66d0fba41e13e9ca6512a0a51170f43e7e7ed3a8dfa036324100775612/matplotlib-3.10.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:d44cb942af1693cced2604c33a9abcef6205601c445f6d0dc531d813af8a2f5a", size = 8043300 },
            { url = "https://files.pythonhosted.org/packages/90/60/2a60342b27b90a16bada939a85e29589902b41073f59668b904b15ea666c/matplotlib-3.10.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:a994f29e968ca002b50982b27168addfd65f0105610b6be7fa515ca4b5307c95", size = 8448936 },
            { url = "https://files.pythonhosted.org/packages/a7/b2/d872fc3d753516870d520595ddd8ce4dd44fa797a240999f125f58521ad7/matplotlib-3.10.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:9b0558bae37f154fffda54d779a592bc97ca8b4701f1c710055b609a3bac44c8", size = 8594151 },
            { url = "https://files.pythonhosted.org/packages/f4/bd/b2f60cf7f57d014ab33e4f74602a2b5bdc657976db8196bbc022185f6f9c/matplotlib-3.10.0-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:503feb23bd8c8acc75541548a1d709c059b7184cde26314896e10a9f14df5f12", size = 9400347 },
            { url = "https://files.pythonhosted.org/packages/9f/6e/264673e64001b99d747aff5a288eca82826c024437a3694e19aed1decf46/matplotlib-3.10.0-cp312-cp312-win_amd64.whl", hash = "sha256:c40ba2eb08b3f5de88152c2333c58cee7edcead0a2a0d60fcafa116b17117adc", size = 8039144 },
            { url = "https://files.pythonhosted.org/packages/72/11/1b2a094d95dcb6e6edd4a0b238177c439006c6b7a9fe8d31801237bf512f/matplotlib-3.10.0-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:96f2886f5c1e466f21cc41b70c5a0cd47bfa0015eb2d5793c88ebce658600e25", size = 8173073 },
            { url = "https://files.pythonhosted.org/packages/0d/c4/87b6ad2723070511a411ea719f9c70fde64605423b184face4e94986de9d/matplotlib-3.10.0-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:12eaf48463b472c3c0f8dbacdbf906e573013df81a0ab82f0616ea4b11281908", size = 8043892 },
            { url = "https://files.pythonhosted.org/packages/57/69/cb0812a136550b21361335e9ffb7d459bf6d13e03cb7b015555d5143d2d6/matplotlib-3.10.0-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:2fbbabc82fde51391c4da5006f965e36d86d95f6ee83fb594b279564a4c5d0d2", size = 8450532 },
            { url = "https://files.pythonhosted.org/packages/ea/3a/bab9deb4fb199c05e9100f94d7f1c702f78d3241e6a71b784d2b88d7bebd/matplotlib-3.10.0-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:ad2e15300530c1a94c63cfa546e3b7864bd18ea2901317bae8bbf06a5ade6dcf", size = 8593905 },
            { url = "https://files.pythonhosted.org/packages/8b/66/742fd242f989adc1847ddf5f445815f73ad7c46aa3440690cc889cfa423c/matplotlib-3.10.0-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:3547d153d70233a8496859097ef0312212e2689cdf8d7ed764441c77604095ae", size = 9399609 },
            { url = "https://files.pythonhosted.org/packages/fa/d6/54cee7142cef7d910a324a7aedf335c0c147b03658b54d49ec48166f10a6/matplotlib-3.10.0-cp313-cp313-win_amd64.whl", hash = "sha256:c55b20591ced744aa04e8c3e4b7543ea4d650b6c3c4b208c08a05b4010e8b442", size = 8039076 },
            { url = "https://files.pythonhosted.org/packages/43/14/815d072dc36e88753433bfd0385113405efb947e6895ff7b4d2e8614a33b/matplotlib-3.10.0-cp313-cp313t-macosx_10_13_x86_64.whl", hash = "sha256:9ade1003376731a971e398cc4ef38bb83ee8caf0aee46ac6daa4b0506db1fd06", size = 8211000 },
            { url = "https://files.pythonhosted.org/packages/9a/76/34e75f364194ec352678adcb540964be6f35ec7d3d8c75ebcb17e6839359/matplotlib-3.10.0-cp313-cp313t-macosx_11_0_arm64.whl", hash = "sha256:95b710fea129c76d30be72c3b38f330269363fbc6e570a5dd43580487380b5ff", size = 8087707 },
            { url = "https://files.pythonhosted.org/packages/c3/2b/b6bc0dff6a72d333bc7df94a66e6ce662d224e43daa8ad8ae4eaa9a77f55/matplotlib-3.10.0-cp313-cp313t-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:5cdbaf909887373c3e094b0318d7ff230b2ad9dcb64da7ade654182872ab2593", size = 8477384 },
            { url = "https://files.pythonhosted.org/packages/c2/2d/b5949fb2b76e9b47ab05e25a5f5f887c70de20d8b0cbc704a4e2ee71c786/matplotlib-3.10.0-cp313-cp313t-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:d907fddb39f923d011875452ff1eca29a9e7f21722b873e90db32e5d8ddff12e", size = 8610334 },
            { url = "https://files.pythonhosted.org/packages/d6/9a/6e3c799d5134d9af44b01c787e1360bee38cf51850506ea2e743a787700b/matplotlib-3.10.0-cp313-cp313t-musllinux_1_2_x86_64.whl", hash = "sha256:3b427392354d10975c1d0f4ee18aa5844640b512d5311ef32efd4dd7db106ede", size = 9406777 },
            { url = "https://files.pythonhosted.org/packages/0e/dd/e6ae97151e5ed648ab2ea48885bc33d39202b640eec7a2910e2c843f7ac0/matplotlib-3.10.0-cp313-cp313t-win_amd64.whl", hash = "sha256:5fd41b0ec7ee45cd960a8e71aea7c946a28a0b8a4dcee47d2856b2af051f334c", size = 8109742 },
            { url = "https://files.pythonhosted.org/packages/32/5f/29def7ce4e815ab939b56280976ee35afffb3bbdb43f332caee74cb8c951/matplotlib-3.10.0-pp310-pypy310_pp73-macosx_10_15_x86_64.whl", hash = "sha256:81713dd0d103b379de4516b861d964b1d789a144103277769238c732229d7f03", size = 8155500 },
            { url = "https://files.pythonhosted.org/packages/de/6d/d570383c9f7ca799d0a54161446f9ce7b17d6c50f2994b653514bcaa108f/matplotlib-3.10.0-pp310-pypy310_pp73-macosx_11_0_arm64.whl", hash = "sha256:359f87baedb1f836ce307f0e850d12bb5f1936f70d035561f90d41d305fdacea", size = 8032398 },
            { url = "https://files.pythonhosted.org/packages/c9/b4/680aa700d99b48e8c4393fa08e9ab8c49c0555ee6f4c9c0a5e8ea8dfde5d/matplotlib-3.10.0-pp310-pypy310_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:ae80dc3a4add4665cf2faa90138384a7ffe2a4e37c58d83e115b54287c4f06ef", size = 8587361 },
        ]

        [[package]]
        name = "matscipy"
        version = "1.1.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "ase" },
            { name = "numpy" },
            { name = "packaging" },
            { name = "scipy" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/67/a4/99d104ebaab040212eb46e9e78b0c264d79fa1396305745ad1d8f974fe59/matscipy-1.1.1.tar.gz", hash = "sha256:2d806d27bfcb99c6e365e0e20cee08e71952ce37b5df3667a1b955dbe26138c2", size = 10362807 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d5/30/a2b676f877d812c8025fe213184fd13b67faa3602a1220681f90bb962f53/matscipy-1.1.1-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:586897ab3eb8657ad41d93395c1185ed26739a39e33a0c555f188996a530d05d", size = 443591 },
            { url = "https://files.pythonhosted.org/packages/98/68/b4ce7491c0bea05586567ab84320ada9b26ce904db5883d8fdbeefeacc2d/matscipy-1.1.1-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:c4b101fa81d2206a33a80fad5640a7189d0982c2dbe9f200314739592b2d25d8", size = 443956 },
            { url = "https://files.pythonhosted.org/packages/27/e8/46389744f5dad0ef3797e64dceda4c998e3aeff40aaccb84f9159a220c9a/matscipy-1.1.1-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:6eb023e71578678550ce713a9559c2704077100ae63b0bc252e385bd244a6e2b", size = 447878 },
            { url = "https://files.pythonhosted.org/packages/d8/a3/7d8941b1d749a745e1ab9761983f11fd91618ccfaa56662b2787c15e8334/matscipy-1.1.1-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:233b8f6a27aeabe27f2288318ad5e24ee7476547eb0ba5e236d1f320e2a90e02", size = 448829 },
            { url = "https://files.pythonhosted.org/packages/36/b7/119092a4cd46c571ba1f717ac49a57b4ca59463c36b8ad53afeb66f677b9/matscipy-1.1.1-cp310-cp310-win_amd64.whl", hash = "sha256:a676d134e1aa31da17ea12d63eb3648f78df4c6e1e283b29b270a5017e2ba4aa", size = 565893 },
            { url = "https://files.pythonhosted.org/packages/c6/08/e27c620d821ff7653cc93c6211046354c7aa422b2289a2c202aeab6d7ac4/matscipy-1.1.1-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:1865a7de629d356289867f1298ff08a280976fd20174a9749699cb1cb44397cb", size = 443590 },
            { url = "https://files.pythonhosted.org/packages/e0/4d/8be5b139fbcb889f24a3edd2bba157575f931e91408309f231906e11ea72/matscipy-1.1.1-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:9c4824ff40a187d0a84ca0b341aa449109398798d717014f7f59e2660ecd6f05", size = 443958 },
            { url = "https://files.pythonhosted.org/packages/2d/eb/cdf942573eacbc7a267bb7d8f84529096434d12a0d8e8b05b6b51ccd65a9/matscipy-1.1.1-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:15ebc86f7ef293fa5161bccf9573f20f7cecced9fea8f1efe833ae67775cf45f", size = 447879 },
            { url = "https://files.pythonhosted.org/packages/77/79/c12cca097156f55a71102a400d02fd352c6adb8b3d16464c4b4782d84147/matscipy-1.1.1-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:1b32fd06e1b97f2df632ca5289863efef522e11c7e5ca62023bee8b01de0a274", size = 448830 },
            { url = "https://files.pythonhosted.org/packages/84/a3/76dbe44f0e1efe020886e841ad72a9101282a66f92009d9bbf995cbdf4aa/matscipy-1.1.1-cp311-cp311-win_amd64.whl", hash = "sha256:c39429604ad02865057ebe611ef15f077facc58890c33cc46449889a48539fd9", size = 565894 },
            { url = "https://files.pythonhosted.org/packages/f5/ba/e082a33beeace6493c300ac0ddf6d7346cfb0d7ce4f06e4336ba39bd8863/matscipy-1.1.1-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:661bdb24472d192effce98ffb80b9b90ed481238a27bab0856c88ea6cdf6b2fd", size = 443556 },
            { url = "https://files.pythonhosted.org/packages/4e/4b/321270220155ab2985520e27a6bdd5818f1d7a908dd066b15afd5f39d8e5/matscipy-1.1.1-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:845948900663640a6b1d7b2e8d20956dd443dbfa3dfbd1b2ceae45982ac0caf0", size = 443717 },
            { url = "https://files.pythonhosted.org/packages/39/ae/75c17ed5b45fa2fb8e529fe0fa524878b759f4bc9affbd14ea95ce52a485/matscipy-1.1.1-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:7180d932332dd8d3e00223954da44e9571167f58c768c2cee48cf7f85f0c4f27", size = 447973 },
            { url = "https://files.pythonhosted.org/packages/a7/e2/51247a22f02979020cb95b6480822478c8491b7fb6f792eff125dd15fb1a/matscipy-1.1.1-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f8088bf43f3b383784321ad044bb03031c6c6a94038a0a7ec3c80b9122ded39b", size = 449034 },
            { url = "https://files.pythonhosted.org/packages/77/11/5284ce41986d69f71116c7b7d5dde57da76b5bccf1dba10abc06958aa086/matscipy-1.1.1-cp312-cp312-win_amd64.whl", hash = "sha256:15eccc94d60f6b7365712a611bb144f37b647a665f745a4dd2c0ed170a534c2d", size = 565994 },
        ]

        [[package]]
        name = "monty"
        version = "2025.1.9"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "numpy" },
            { name = "ruamel-yaml" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/70/32/aa5deaaad86987722384c238a77432fe06c61639cea2b565e4bcd8c229bb/monty-2025.1.9.tar.gz", hash = "sha256:edb680b01ea1e59225cb666634b0dd2b2393eef07f3d45748445db92e1f1006d", size = 85180 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/62/b7/a3dc997342c82fe0f19ef77dd5d693ba42b55a94956f03059d53a91f45d9/monty-2025.1.9-py3-none-any.whl", hash = "sha256:6bdff83803580561efcd9e009d02287b020ad833292730672ccb277aee5362f3", size = 51503 },
        ]

        [[package]]
        name = "mpmath"
        version = "1.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e0/47/dd32fa426cc72114383ac549964eecb20ecfd886d1e5ccf5340b55b02f57/mpmath-1.3.0.tar.gz", hash = "sha256:7a28eb2a9774d00c7bc92411c19a89209d5da7c4c9a9e227be8330a23a25b91f", size = 508106 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/43/e3/7d92a15f894aa0c9c4b49b8ee9ac9850d6e63b03c9c32c0367a13ae62209/mpmath-1.3.0-py3-none-any.whl", hash = "sha256:a0b2b9fe80bbcd81a6647ff13108738cfb482d481d826cc0e02f5b35e5c88d2c", size = 536198 },
        ]

        [[package]]
        name = "multidict"
        version = "6.1.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "typing-extensions", marker = "python_full_version < '3.11'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d6/be/504b89a5e9ca731cd47487e91c469064f8ae5af93b7259758dcfc2b9c848/multidict-6.1.0.tar.gz", hash = "sha256:22ae2ebf9b0c69d206c003e2f6a914ea33f0a932d4aa16f236afc049d9958f4a", size = 64002 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/29/68/259dee7fd14cf56a17c554125e534f6274c2860159692a414d0b402b9a6d/multidict-6.1.0-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:3380252550e372e8511d49481bd836264c009adb826b23fefcc5dd3c69692f60", size = 48628 },
            { url = "https://files.pythonhosted.org/packages/50/79/53ba256069fe5386a4a9e80d4e12857ced9de295baf3e20c68cdda746e04/multidict-6.1.0-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:99f826cbf970077383d7de805c0681799491cb939c25450b9b5b3ced03ca99f1", size = 29327 },
            { url = "https://files.pythonhosted.org/packages/ff/10/71f1379b05b196dae749b5ac062e87273e3f11634f447ebac12a571d90ae/multidict-6.1.0-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:a114d03b938376557927ab23f1e950827c3b893ccb94b62fd95d430fd0e5cf53", size = 29689 },
            { url = "https://files.pythonhosted.org/packages/71/45/70bac4f87438ded36ad4793793c0095de6572d433d98575a5752629ef549/multidict-6.1.0-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:b1c416351ee6271b2f49b56ad7f308072f6f44b37118d69c2cad94f3fa8a40d5", size = 126639 },
            { url = "https://files.pythonhosted.org/packages/80/cf/17f35b3b9509b4959303c05379c4bfb0d7dd05c3306039fc79cf035bbac0/multidict-6.1.0-cp310-cp310-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:6b5d83030255983181005e6cfbac1617ce9746b219bc2aad52201ad121226581", size = 134315 },
            { url = "https://files.pythonhosted.org/packages/ef/1f/652d70ab5effb33c031510a3503d4d6efc5ec93153562f1ee0acdc895a57/multidict-6.1.0-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:3e97b5e938051226dc025ec80980c285b053ffb1e25a3db2a3aa3bc046bf7f56", size = 129471 },
            { url = "https://files.pythonhosted.org/packages/a6/64/2dd6c4c681688c0165dea3975a6a4eab4944ea30f35000f8b8af1df3148c/multidict-6.1.0-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:d618649d4e70ac6efcbba75be98b26ef5078faad23592f9b51ca492953012429", size = 124585 },
            { url = "https://files.pythonhosted.org/packages/87/56/e6ee5459894c7e554b57ba88f7257dc3c3d2d379cb15baaa1e265b8c6165/multidict-6.1.0-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:10524ebd769727ac77ef2278390fb0068d83f3acb7773792a5080f2b0abf7748", size = 116957 },
            { url = "https://files.pythonhosted.org/packages/36/9e/616ce5e8d375c24b84f14fc263c7ef1d8d5e8ef529dbc0f1df8ce71bb5b8/multidict-6.1.0-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:ff3827aef427c89a25cc96ded1759271a93603aba9fb977a6d264648ebf989db", size = 128609 },
            { url = "https://files.pythonhosted.org/packages/8c/4f/4783e48a38495d000f2124020dc96bacc806a4340345211b1ab6175a6cb4/multidict-6.1.0-cp310-cp310-musllinux_1_2_i686.whl", hash = "sha256:06809f4f0f7ab7ea2cabf9caca7d79c22c0758b58a71f9d32943ae13c7ace056", size = 123016 },
            { url = "https://files.pythonhosted.org/packages/3e/b3/4950551ab8fc39862ba5e9907dc821f896aa829b4524b4deefd3e12945ab/multidict-6.1.0-cp310-cp310-musllinux_1_2_ppc64le.whl", hash = "sha256:f179dee3b863ab1c59580ff60f9d99f632f34ccb38bf67a33ec6b3ecadd0fd76", size = 133542 },
            { url = "https://files.pythonhosted.org/packages/96/4d/f0ce6ac9914168a2a71df117935bb1f1781916acdecbb43285e225b484b8/multidict-6.1.0-cp310-cp310-musllinux_1_2_s390x.whl", hash = "sha256:aaed8b0562be4a0876ee3b6946f6869b7bcdb571a5d1496683505944e268b160", size = 130163 },
            { url = "https://files.pythonhosted.org/packages/be/72/17c9f67e7542a49dd252c5ae50248607dfb780bcc03035907dafefb067e3/multidict-6.1.0-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:3c8b88a2ccf5493b6c8da9076fb151ba106960a2df90c2633f342f120751a9e7", size = 126832 },
            { url = "https://files.pythonhosted.org/packages/71/9f/72d719e248cbd755c8736c6d14780533a1606ffb3fbb0fbd77da9f0372da/multidict-6.1.0-cp310-cp310-win32.whl", hash = "sha256:4a9cb68166a34117d6646c0023c7b759bf197bee5ad4272f420a0141d7eb03a0", size = 26402 },
            { url = "https://files.pythonhosted.org/packages/04/5a/d88cd5d00a184e1ddffc82aa2e6e915164a6d2641ed3606e766b5d2f275a/multidict-6.1.0-cp310-cp310-win_amd64.whl", hash = "sha256:20b9b5fbe0b88d0bdef2012ef7dee867f874b72528cf1d08f1d59b0e3850129d", size = 28800 },
            { url = "https://files.pythonhosted.org/packages/93/13/df3505a46d0cd08428e4c8169a196131d1b0c4b515c3649829258843dde6/multidict-6.1.0-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:3efe2c2cb5763f2f1b275ad2bf7a287d3f7ebbef35648a9726e3b69284a4f3d6", size = 48570 },
            { url = "https://files.pythonhosted.org/packages/f0/e1/a215908bfae1343cdb72f805366592bdd60487b4232d039c437fe8f5013d/multidict-6.1.0-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:c7053d3b0353a8b9de430a4f4b4268ac9a4fb3481af37dfe49825bf45ca24156", size = 29316 },
            { url = "https://files.pythonhosted.org/packages/70/0f/6dc70ddf5d442702ed74f298d69977f904960b82368532c88e854b79f72b/multidict-6.1.0-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:27e5fc84ccef8dfaabb09d82b7d179c7cf1a3fbc8a966f8274fcb4ab2eb4cadb", size = 29640 },
            { url = "https://files.pythonhosted.org/packages/d8/6d/9c87b73a13d1cdea30b321ef4b3824449866bd7f7127eceed066ccb9b9ff/multidict-6.1.0-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:0e2b90b43e696f25c62656389d32236e049568b39320e2735d51f08fd362761b", size = 131067 },
            { url = "https://files.pythonhosted.org/packages/cc/1e/1b34154fef373371fd6c65125b3d42ff5f56c7ccc6bfff91b9b3c60ae9e0/multidict-6.1.0-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:d83a047959d38a7ff552ff94be767b7fd79b831ad1cd9920662db05fec24fe72", size = 138507 },
            { url = "https://files.pythonhosted.org/packages/fb/e0/0bc6b2bac6e461822b5f575eae85da6aae76d0e2a79b6665d6206b8e2e48/multidict-6.1.0-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:d1a9dd711d0877a1ece3d2e4fea11a8e75741ca21954c919406b44e7cf971304", size = 133905 },
            { url = "https://files.pythonhosted.org/packages/ba/af/73d13b918071ff9b2205fcf773d316e0f8fefb4ec65354bbcf0b10908cc6/multidict-6.1.0-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:ec2abea24d98246b94913b76a125e855eb5c434f7c46546046372fe60f666351", size = 129004 },
            { url = "https://files.pythonhosted.org/packages/74/21/23960627b00ed39643302d81bcda44c9444ebcdc04ee5bedd0757513f259/multidict-6.1.0-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4867cafcbc6585e4b678876c489b9273b13e9fff9f6d6d66add5e15d11d926cb", size = 121308 },
            { url = "https://files.pythonhosted.org/packages/8b/5c/cf282263ffce4a596ed0bb2aa1a1dddfe1996d6a62d08842a8d4b33dca13/multidict-6.1.0-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:5b48204e8d955c47c55b72779802b219a39acc3ee3d0116d5080c388970b76e3", size = 132608 },
            { url = "https://files.pythonhosted.org/packages/d7/3e/97e778c041c72063f42b290888daff008d3ab1427f5b09b714f5a8eff294/multidict-6.1.0-cp311-cp311-musllinux_1_2_i686.whl", hash = "sha256:d8fff389528cad1618fb4b26b95550327495462cd745d879a8c7c2115248e399", size = 127029 },
            { url = "https://files.pythonhosted.org/packages/47/ac/3efb7bfe2f3aefcf8d103e9a7162572f01936155ab2f7ebcc7c255a23212/multidict-6.1.0-cp311-cp311-musllinux_1_2_ppc64le.whl", hash = "sha256:a7a9541cd308eed5e30318430a9c74d2132e9a8cb46b901326272d780bf2d423", size = 137594 },
            { url = "https://files.pythonhosted.org/packages/42/9b/6c6e9e8dc4f915fc90a9b7798c44a30773dea2995fdcb619870e705afe2b/multidict-6.1.0-cp311-cp311-musllinux_1_2_s390x.whl", hash = "sha256:da1758c76f50c39a2efd5e9859ce7d776317eb1dd34317c8152ac9251fc574a3", size = 134556 },
            { url = "https://files.pythonhosted.org/packages/1d/10/8e881743b26aaf718379a14ac58572a240e8293a1c9d68e1418fb11c0f90/multidict-6.1.0-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:c943a53e9186688b45b323602298ab727d8865d8c9ee0b17f8d62d14b56f0753", size = 130993 },
            { url = "https://files.pythonhosted.org/packages/45/84/3eb91b4b557442802d058a7579e864b329968c8d0ea57d907e7023c677f2/multidict-6.1.0-cp311-cp311-win32.whl", hash = "sha256:90f8717cb649eea3504091e640a1b8568faad18bd4b9fcd692853a04475a4b80", size = 26405 },
            { url = "https://files.pythonhosted.org/packages/9f/0b/ad879847ecbf6d27e90a6eabb7eff6b62c129eefe617ea45eae7c1f0aead/multidict-6.1.0-cp311-cp311-win_amd64.whl", hash = "sha256:82176036e65644a6cc5bd619f65f6f19781e8ec2e5330f51aa9ada7504cc1926", size = 28795 },
            { url = "https://files.pythonhosted.org/packages/fd/16/92057c74ba3b96d5e211b553895cd6dc7cc4d1e43d9ab8fafc727681ef71/multidict-6.1.0-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:b04772ed465fa3cc947db808fa306d79b43e896beb677a56fb2347ca1a49c1fa", size = 48713 },
            { url = "https://files.pythonhosted.org/packages/94/3d/37d1b8893ae79716179540b89fc6a0ee56b4a65fcc0d63535c6f5d96f217/multidict-6.1.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:6180c0ae073bddeb5a97a38c03f30c233e0a4d39cd86166251617d1bbd0af436", size = 29516 },
            { url = "https://files.pythonhosted.org/packages/a2/12/adb6b3200c363062f805275b4c1e656be2b3681aada66c80129932ff0bae/multidict-6.1.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:071120490b47aa997cca00666923a83f02c7fbb44f71cf7f136df753f7fa8761", size = 29557 },
            { url = "https://files.pythonhosted.org/packages/47/e9/604bb05e6e5bce1e6a5cf80a474e0f072e80d8ac105f1b994a53e0b28c42/multidict-6.1.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:50b3a2710631848991d0bf7de077502e8994c804bb805aeb2925a981de58ec2e", size = 130170 },
            { url = "https://files.pythonhosted.org/packages/7e/13/9efa50801785eccbf7086b3c83b71a4fb501a4d43549c2f2f80b8787d69f/multidict-6.1.0-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:b58c621844d55e71c1b7f7c498ce5aa6985d743a1a59034c57a905b3f153c1ef", size = 134836 },
            { url = "https://files.pythonhosted.org/packages/bf/0f/93808b765192780d117814a6dfcc2e75de6dcc610009ad408b8814dca3ba/multidict-6.1.0-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:55b6d90641869892caa9ca42ff913f7ff1c5ece06474fbd32fb2cf6834726c95", size = 133475 },
            { url = "https://files.pythonhosted.org/packages/d3/c8/529101d7176fe7dfe1d99604e48d69c5dfdcadb4f06561f465c8ef12b4df/multidict-6.1.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:4b820514bfc0b98a30e3d85462084779900347e4d49267f747ff54060cc33925", size = 131049 },
            { url = "https://files.pythonhosted.org/packages/ca/0c/fc85b439014d5a58063e19c3a158a889deec399d47b5269a0f3b6a2e28bc/multidict-6.1.0-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:10a9b09aba0c5b48c53761b7c720aaaf7cf236d5fe394cd399c7ba662d5f9966", size = 120370 },
            { url = "https://files.pythonhosted.org/packages/db/46/d4416eb20176492d2258fbd47b4abe729ff3b6e9c829ea4236f93c865089/multidict-6.1.0-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:1e16bf3e5fc9f44632affb159d30a437bfe286ce9e02754759be5536b169b305", size = 125178 },
            { url = "https://files.pythonhosted.org/packages/5b/46/73697ad7ec521df7de5531a32780bbfd908ded0643cbe457f981a701457c/multidict-6.1.0-cp312-cp312-musllinux_1_2_i686.whl", hash = "sha256:76f364861c3bfc98cbbcbd402d83454ed9e01a5224bb3a28bf70002a230f73e2", size = 119567 },
            { url = "https://files.pythonhosted.org/packages/cd/ed/51f060e2cb0e7635329fa6ff930aa5cffa17f4c7f5c6c3ddc3500708e2f2/multidict-6.1.0-cp312-cp312-musllinux_1_2_ppc64le.whl", hash = "sha256:820c661588bd01a0aa62a1283f20d2be4281b086f80dad9e955e690c75fb54a2", size = 129822 },
            { url = "https://files.pythonhosted.org/packages/df/9e/ee7d1954b1331da3eddea0c4e08d9142da5f14b1321c7301f5014f49d492/multidict-6.1.0-cp312-cp312-musllinux_1_2_s390x.whl", hash = "sha256:0e5f362e895bc5b9e67fe6e4ded2492d8124bdf817827f33c5b46c2fe3ffaca6", size = 128656 },
            { url = "https://files.pythonhosted.org/packages/77/00/8538f11e3356b5d95fa4b024aa566cde7a38aa7a5f08f4912b32a037c5dc/multidict-6.1.0-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:3ec660d19bbc671e3a6443325f07263be452c453ac9e512f5eb935e7d4ac28b3", size = 125360 },
            { url = "https://files.pythonhosted.org/packages/be/05/5d334c1f2462d43fec2363cd00b1c44c93a78c3925d952e9a71caf662e96/multidict-6.1.0-cp312-cp312-win32.whl", hash = "sha256:58130ecf8f7b8112cdb841486404f1282b9c86ccb30d3519faf301b2e5659133", size = 26382 },
            { url = "https://files.pythonhosted.org/packages/a3/bf/f332a13486b1ed0496d624bcc7e8357bb8053823e8cd4b9a18edc1d97e73/multidict-6.1.0-cp312-cp312-win_amd64.whl", hash = "sha256:188215fc0aafb8e03341995e7c4797860181562380f81ed0a87ff455b70bf1f1", size = 28529 },
            { url = "https://files.pythonhosted.org/packages/22/67/1c7c0f39fe069aa4e5d794f323be24bf4d33d62d2a348acdb7991f8f30db/multidict-6.1.0-cp313-cp313-macosx_10_13_universal2.whl", hash = "sha256:d569388c381b24671589335a3be6e1d45546c2988c2ebe30fdcada8457a31008", size = 48771 },
            { url = "https://files.pythonhosted.org/packages/3c/25/c186ee7b212bdf0df2519eacfb1981a017bda34392c67542c274651daf23/multidict-6.1.0-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:052e10d2d37810b99cc170b785945421141bf7bb7d2f8799d431e7db229c385f", size = 29533 },
            { url = "https://files.pythonhosted.org/packages/67/5e/04575fd837e0958e324ca035b339cea174554f6f641d3fb2b4f2e7ff44a2/multidict-6.1.0-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:f90c822a402cb865e396a504f9fc8173ef34212a342d92e362ca498cad308e28", size = 29595 },
            { url = "https://files.pythonhosted.org/packages/d3/b2/e56388f86663810c07cfe4a3c3d87227f3811eeb2d08450b9e5d19d78876/multidict-6.1.0-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:b225d95519a5bf73860323e633a664b0d85ad3d5bede6d30d95b35d4dfe8805b", size = 130094 },
            { url = "https://files.pythonhosted.org/packages/6c/ee/30ae9b4186a644d284543d55d491fbd4239b015d36b23fea43b4c94f7052/multidict-6.1.0-cp313-cp313-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:23bfd518810af7de1116313ebd9092cb9aa629beb12f6ed631ad53356ed6b86c", size = 134876 },
            { url = "https://files.pythonhosted.org/packages/84/c7/70461c13ba8ce3c779503c70ec9d0345ae84de04521c1f45a04d5f48943d/multidict-6.1.0-cp313-cp313-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:5c09fcfdccdd0b57867577b719c69e347a436b86cd83747f179dbf0cc0d4c1f3", size = 133500 },
            { url = "https://files.pythonhosted.org/packages/4a/9f/002af221253f10f99959561123fae676148dd730e2daa2cd053846a58507/multidict-6.1.0-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:bf6bea52ec97e95560af5ae576bdac3aa3aae0b6758c6efa115236d9e07dae44", size = 131099 },
            { url = "https://files.pythonhosted.org/packages/82/42/d1c7a7301d52af79d88548a97e297f9d99c961ad76bbe6f67442bb77f097/multidict-6.1.0-cp313-cp313-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:57feec87371dbb3520da6192213c7d6fc892d5589a93db548331954de8248fd2", size = 120403 },
            { url = "https://files.pythonhosted.org/packages/68/f3/471985c2c7ac707547553e8f37cff5158030d36bdec4414cb825fbaa5327/multidict-6.1.0-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:0c3f390dc53279cbc8ba976e5f8035eab997829066756d811616b652b00a23a3", size = 125348 },
            { url = "https://files.pythonhosted.org/packages/67/2c/e6df05c77e0e433c214ec1d21ddd203d9a4770a1f2866a8ca40a545869a0/multidict-6.1.0-cp313-cp313-musllinux_1_2_i686.whl", hash = "sha256:59bfeae4b25ec05b34f1956eaa1cb38032282cd4dfabc5056d0a1ec4d696d3aa", size = 119673 },
            { url = "https://files.pythonhosted.org/packages/c5/cd/bc8608fff06239c9fb333f9db7743a1b2eafe98c2666c9a196e867a3a0a4/multidict-6.1.0-cp313-cp313-musllinux_1_2_ppc64le.whl", hash = "sha256:b2f59caeaf7632cc633b5cf6fc449372b83bbdf0da4ae04d5be36118e46cc0aa", size = 129927 },
            { url = "https://files.pythonhosted.org/packages/44/8e/281b69b7bc84fc963a44dc6e0bbcc7150e517b91df368a27834299a526ac/multidict-6.1.0-cp313-cp313-musllinux_1_2_s390x.whl", hash = "sha256:37bb93b2178e02b7b618893990941900fd25b6b9ac0fa49931a40aecdf083fe4", size = 128711 },
            { url = "https://files.pythonhosted.org/packages/12/a4/63e7cd38ed29dd9f1881d5119f272c898ca92536cdb53ffe0843197f6c85/multidict-6.1.0-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:4e9f48f58c2c523d5a06faea47866cd35b32655c46b443f163d08c6d0ddb17d6", size = 125519 },
            { url = "https://files.pythonhosted.org/packages/38/e0/4f5855037a72cd8a7a2f60a3952d9aa45feedb37ae7831642102604e8a37/multidict-6.1.0-cp313-cp313-win32.whl", hash = "sha256:3a37ffb35399029b45c6cc33640a92bef403c9fd388acce75cdc88f58bd19a81", size = 26426 },
            { url = "https://files.pythonhosted.org/packages/7e/a5/17ee3a4db1e310b7405f5d25834460073a8ccd86198ce044dfaf69eac073/multidict-6.1.0-cp313-cp313-win_amd64.whl", hash = "sha256:e9aa71e15d9d9beaad2c6b9319edcdc0a49a43ef5c0a4c8265ca9ee7d6c67774", size = 28531 },
            { url = "https://files.pythonhosted.org/packages/99/b7/b9e70fde2c0f0c9af4cc5277782a89b66d35948ea3369ec9f598358c3ac5/multidict-6.1.0-py3-none-any.whl", hash = "sha256:48e171e52d1c4d33888e529b999e5900356b9ae588c2f09a52dcefb158b27506", size = 10051 },
        ]

        [[package]]
        name = "narwhals"
        version = "1.25.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/59/9f/6fa2646cdab958abd89d1b2804be2cdee54c931a308cff95d282d63d437e/narwhals-1.25.2.tar.gz", hash = "sha256:37594746fc06fe4a588967a34a2974b1f3a7ad6ff1571b6e31ac5e58c9591000", size = 247191 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7f/0c/66a308d86c5607a2538401602875c146ae967d69e0118f579be07386cc40/narwhals-1.25.2-py3-none-any.whl", hash = "sha256:e645f7fc1f8c0a3563a6cdcd0191586cdf88470ad90f0818abba7ceb6c181b00", size = 305490 },
        ]

        [[package]]
        name = "networkx"
        version = "3.4.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/fd/1d/06475e1cd5264c0b870ea2cc6fdb3e37177c1e565c43f56ff17a10e3937f/networkx-3.4.2.tar.gz", hash = "sha256:307c3669428c5362aab27c8a1260aa8f47c4e91d3891f48be0141738d8d053e1", size = 2151368 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b9/54/dd730b32ea14ea797530a4479b2ed46a6fb250f682a9cfb997e968bf0261/networkx-3.4.2-py3-none-any.whl", hash = "sha256:df5d4365b724cf81b8c6a7312509d0c22386097011ad1abe274afd5e9d3bbc5f", size = 1723263 },
        ]

        [[package]]
        name = "numpy"
        version = "1.26.4"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/65/6e/09db70a523a96d25e115e71cc56a6f9031e7b8cd166c1ac8438307c14058/numpy-1.26.4.tar.gz", hash = "sha256:2a02aba9ed12e4ac4eb3ea9421c420301a0c6460d9830d74a9df87efa4912010", size = 15786129 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a7/94/ace0fdea5241a27d13543ee117cbc65868e82213fb31a8eb7fe9ff23f313/numpy-1.26.4-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:9ff0f4f29c51e2803569d7a51c2304de5554655a60c5d776e35b4a41413830d0", size = 20631468 },
            { url = "https://files.pythonhosted.org/packages/20/f7/b24208eba89f9d1b58c1668bc6c8c4fd472b20c45573cb767f59d49fb0f6/numpy-1.26.4-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:2e4ee3380d6de9c9ec04745830fd9e2eccb3e6cf790d39d7b98ffd19b0dd754a", size = 13966411 },
            { url = "https://files.pythonhosted.org/packages/fc/a5/4beee6488160798683eed5bdb7eead455892c3b4e1f78d79d8d3f3b084ac/numpy-1.26.4-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:d209d8969599b27ad20994c8e41936ee0964e6da07478d6c35016bc386b66ad4", size = 14219016 },
            { url = "https://files.pythonhosted.org/packages/4b/d7/ecf66c1cd12dc28b4040b15ab4d17b773b87fa9d29ca16125de01adb36cd/numpy-1.26.4-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:ffa75af20b44f8dba823498024771d5ac50620e6915abac414251bd971b4529f", size = 18240889 },
            { url = "https://files.pythonhosted.org/packages/24/03/6f229fe3187546435c4f6f89f6d26c129d4f5bed40552899fcf1f0bf9e50/numpy-1.26.4-cp310-cp310-musllinux_1_1_aarch64.whl", hash = "sha256:62b8e4b1e28009ef2846b4c7852046736bab361f7aeadeb6a5b89ebec3c7055a", size = 13876746 },
            { url = "https://files.pythonhosted.org/packages/39/fe/39ada9b094f01f5a35486577c848fe274e374bbf8d8f472e1423a0bbd26d/numpy-1.26.4-cp310-cp310-musllinux_1_1_x86_64.whl", hash = "sha256:a4abb4f9001ad2858e7ac189089c42178fcce737e4169dc61321660f1a96c7d2", size = 18078620 },
            { url = "https://files.pythonhosted.org/packages/d5/ef/6ad11d51197aad206a9ad2286dc1aac6a378059e06e8cf22cd08ed4f20dc/numpy-1.26.4-cp310-cp310-win32.whl", hash = "sha256:bfe25acf8b437eb2a8b2d49d443800a5f18508cd811fea3181723922a8a82b07", size = 5972659 },
            { url = "https://files.pythonhosted.org/packages/19/77/538f202862b9183f54108557bfda67e17603fc560c384559e769321c9d92/numpy-1.26.4-cp310-cp310-win_amd64.whl", hash = "sha256:b97fe8060236edf3662adfc2c633f56a08ae30560c56310562cb4f95500022d5", size = 15808905 },
            { url = "https://files.pythonhosted.org/packages/11/57/baae43d14fe163fa0e4c47f307b6b2511ab8d7d30177c491960504252053/numpy-1.26.4-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:4c66707fabe114439db9068ee468c26bbdf909cac0fb58686a42a24de1760c71", size = 20630554 },
            { url = "https://files.pythonhosted.org/packages/1a/2e/151484f49fd03944c4a3ad9c418ed193cfd02724e138ac8a9505d056c582/numpy-1.26.4-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:edd8b5fe47dab091176d21bb6de568acdd906d1887a4584a15a9a96a1dca06ef", size = 13997127 },
            { url = "https://files.pythonhosted.org/packages/79/ae/7e5b85136806f9dadf4878bf73cf223fe5c2636818ba3ab1c585d0403164/numpy-1.26.4-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:7ab55401287bfec946ced39700c053796e7cc0e3acbef09993a9ad2adba6ca6e", size = 14222994 },
            { url = "https://files.pythonhosted.org/packages/3a/d0/edc009c27b406c4f9cbc79274d6e46d634d139075492ad055e3d68445925/numpy-1.26.4-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:666dbfb6ec68962c033a450943ded891bed2d54e6755e35e5835d63f4f6931d5", size = 18252005 },
            { url = "https://files.pythonhosted.org/packages/09/bf/2b1aaf8f525f2923ff6cfcf134ae5e750e279ac65ebf386c75a0cf6da06a/numpy-1.26.4-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:96ff0b2ad353d8f990b63294c8986f1ec3cb19d749234014f4e7eb0112ceba5a", size = 13885297 },
            { url = "https://files.pythonhosted.org/packages/df/a0/4e0f14d847cfc2a633a1c8621d00724f3206cfeddeb66d35698c4e2cf3d2/numpy-1.26.4-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:60dedbb91afcbfdc9bc0b1f3f402804070deed7392c23eb7a7f07fa857868e8a", size = 18093567 },
            { url = "https://files.pythonhosted.org/packages/d2/b7/a734c733286e10a7f1a8ad1ae8c90f2d33bf604a96548e0a4a3a6739b468/numpy-1.26.4-cp311-cp311-win32.whl", hash = "sha256:1af303d6b2210eb850fcf03064d364652b7120803a0b872f5211f5234b399f20", size = 5968812 },
            { url = "https://files.pythonhosted.org/packages/3f/6b/5610004206cf7f8e7ad91c5a85a8c71b2f2f8051a0c0c4d5916b76d6cbb2/numpy-1.26.4-cp311-cp311-win_amd64.whl", hash = "sha256:cd25bcecc4974d09257ffcd1f098ee778f7834c3ad767fe5db785be9a4aa9cb2", size = 15811913 },
            { url = "https://files.pythonhosted.org/packages/95/12/8f2020a8e8b8383ac0177dc9570aad031a3beb12e38847f7129bacd96228/numpy-1.26.4-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:b3ce300f3644fb06443ee2222c2201dd3a89ea6040541412b8fa189341847218", size = 20335901 },
            { url = "https://files.pythonhosted.org/packages/75/5b/ca6c8bd14007e5ca171c7c03102d17b4f4e0ceb53957e8c44343a9546dcc/numpy-1.26.4-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:03a8c78d01d9781b28a6989f6fa1bb2c4f2d51201cf99d3dd875df6fbd96b23b", size = 13685868 },
            { url = "https://files.pythonhosted.org/packages/79/f8/97f10e6755e2a7d027ca783f63044d5b1bc1ae7acb12afe6a9b4286eac17/numpy-1.26.4-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:9fad7dcb1aac3c7f0584a5a8133e3a43eeb2fe127f47e3632d43d677c66c102b", size = 13925109 },
            { url = "https://files.pythonhosted.org/packages/0f/50/de23fde84e45f5c4fda2488c759b69990fd4512387a8632860f3ac9cd225/numpy-1.26.4-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:675d61ffbfa78604709862923189bad94014bef562cc35cf61d3a07bba02a7ed", size = 17950613 },
            { url = "https://files.pythonhosted.org/packages/4c/0c/9c603826b6465e82591e05ca230dfc13376da512b25ccd0894709b054ed0/numpy-1.26.4-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:ab47dbe5cc8210f55aa58e4805fe224dac469cde56b9f731a4c098b91917159a", size = 13572172 },
            { url = "https://files.pythonhosted.org/packages/76/8c/2ba3902e1a0fc1c74962ea9bb33a534bb05984ad7ff9515bf8d07527cadd/numpy-1.26.4-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:1dda2e7b4ec9dd512f84935c5f126c8bd8b9f2fc001e9f54af255e8c5f16b0e0", size = 17786643 },
            { url = "https://files.pythonhosted.org/packages/28/4a/46d9e65106879492374999e76eb85f87b15328e06bd1550668f79f7b18c6/numpy-1.26.4-cp312-cp312-win32.whl", hash = "sha256:50193e430acfc1346175fcbdaa28ffec49947a06918b7b92130744e81e640110", size = 5677803 },
            { url = "https://files.pythonhosted.org/packages/16/2e/86f24451c2d530c88daf997cb8d6ac622c1d40d19f5a031ed68a4b73a374/numpy-1.26.4-cp312-cp312-win_amd64.whl", hash = "sha256:08beddf13648eb95f8d867350f6a018a4be2e5ad54c8d8caed89ebca558b2818", size = 15517754 },
        ]

        [[package]]
        name = "nvidia-cublas-cu12"
        version = "12.1.3.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/37/6d/121efd7382d5b0284239f4ab1fc1590d86d34ed4a4a2fdb13b30ca8e5740/nvidia_cublas_cu12-12.1.3.1-py3-none-manylinux1_x86_64.whl", hash = "sha256:ee53ccca76a6fc08fb9701aa95b6ceb242cdaab118c3bb152af4e579af792728", size = 410594774 },
            { url = "https://files.pythonhosted.org/packages/c5/ef/32a375b74bea706c93deea5613552f7c9104f961b21df423f5887eca713b/nvidia_cublas_cu12-12.1.3.1-py3-none-win_amd64.whl", hash = "sha256:2b964d60e8cf11b5e1073d179d85fa340c120e99b3067558f3cf98dd69d02906", size = 439918445 },
        ]

        [[package]]
        name = "nvidia-cublas-cu12"
        version = "12.4.5.8"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7f/7f/7fbae15a3982dc9595e49ce0f19332423b260045d0a6afe93cdbe2f1f624/nvidia_cublas_cu12-12.4.5.8-py3-none-manylinux2014_aarch64.whl", hash = "sha256:0f8aa1706812e00b9f19dfe0cdb3999b092ccb8ca168c0db5b8ea712456fd9b3", size = 363333771 },
            { url = "https://files.pythonhosted.org/packages/ae/71/1c91302526c45ab494c23f61c7a84aa568b8c1f9d196efa5993957faf906/nvidia_cublas_cu12-12.4.5.8-py3-none-manylinux2014_x86_64.whl", hash = "sha256:2fc8da60df463fdefa81e323eef2e36489e1c94335b5358bcb38360adf75ac9b", size = 363438805 },
            { url = "https://files.pythonhosted.org/packages/e2/2a/4f27ca96232e8b5269074a72e03b4e0d43aa68c9b965058b1684d07c6ff8/nvidia_cublas_cu12-12.4.5.8-py3-none-win_amd64.whl", hash = "sha256:5a796786da89203a0657eda402bcdcec6180254a8ac22d72213abc42069522dc", size = 396895858 },
        ]

        [[package]]
        name = "nvidia-cuda-cupti-cu12"
        version = "12.1.105"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7e/00/6b218edd739ecfc60524e585ba8e6b00554dd908de2c9c66c1af3e44e18d/nvidia_cuda_cupti_cu12-12.1.105-py3-none-manylinux1_x86_64.whl", hash = "sha256:e54fde3983165c624cb79254ae9818a456eb6e87a7fd4d56a2352c24ee542d7e", size = 14109015 },
            { url = "https://files.pythonhosted.org/packages/d0/56/0021e32ea2848c24242f6b56790bd0ccc8bf99f973ca790569c6ca028107/nvidia_cuda_cupti_cu12-12.1.105-py3-none-win_amd64.whl", hash = "sha256:bea8236d13a0ac7190bd2919c3e8e6ce1e402104276e6f9694479e48bb0eb2a4", size = 10154340 },
        ]

        [[package]]
        name = "nvidia-cuda-cupti-cu12"
        version = "12.4.127"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/93/b5/9fb3d00386d3361b03874246190dfec7b206fd74e6e287b26a8fcb359d95/nvidia_cuda_cupti_cu12-12.4.127-py3-none-manylinux2014_aarch64.whl", hash = "sha256:79279b35cf6f91da114182a5ce1864997fd52294a87a16179ce275773799458a", size = 12354556 },
            { url = "https://files.pythonhosted.org/packages/67/42/f4f60238e8194a3106d06a058d494b18e006c10bb2b915655bd9f6ea4cb1/nvidia_cuda_cupti_cu12-12.4.127-py3-none-manylinux2014_x86_64.whl", hash = "sha256:9dec60f5ac126f7bb551c055072b69d85392b13311fcc1bcda2202d172df30fb", size = 13813957 },
            { url = "https://files.pythonhosted.org/packages/f3/79/8cf313ec17c58ccebc965568e5bcb265cdab0a1df99c4e674bb7a3b99bfe/nvidia_cuda_cupti_cu12-12.4.127-py3-none-win_amd64.whl", hash = "sha256:5688d203301ab051449a2b1cb6690fbe90d2b372f411521c86018b950f3d7922", size = 9938035 },
        ]

        [[package]]
        name = "nvidia-cuda-nvrtc-cu12"
        version = "12.1.105"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/9f/c64c03f49d6fbc56196664d05dba14e3a561038a81a638eeb47f4d4cfd48/nvidia_cuda_nvrtc_cu12-12.1.105-py3-none-manylinux1_x86_64.whl", hash = "sha256:339b385f50c309763ca65456ec75e17bbefcbbf2893f462cb8b90584cd27a1c2", size = 23671734 },
            { url = "https://files.pythonhosted.org/packages/ad/1d/f76987c4f454eb86e0b9a0e4f57c3bf1ac1d13ad13cd1a4da4eb0e0c0ce9/nvidia_cuda_nvrtc_cu12-12.1.105-py3-none-win_amd64.whl", hash = "sha256:0a98a522d9ff138b96c010a65e145dc1b4850e9ecb75a0172371793752fd46ed", size = 19331863 },
        ]

        [[package]]
        name = "nvidia-cuda-nvrtc-cu12"
        version = "12.4.127"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/77/aa/083b01c427e963ad0b314040565ea396f914349914c298556484f799e61b/nvidia_cuda_nvrtc_cu12-12.4.127-py3-none-manylinux2014_aarch64.whl", hash = "sha256:0eedf14185e04b76aa05b1fea04133e59f465b6f960c0cbf4e37c3cb6b0ea198", size = 24133372 },
            { url = "https://files.pythonhosted.org/packages/2c/14/91ae57cd4db3f9ef7aa99f4019cfa8d54cb4caa7e00975df6467e9725a9f/nvidia_cuda_nvrtc_cu12-12.4.127-py3-none-manylinux2014_x86_64.whl", hash = "sha256:a178759ebb095827bd30ef56598ec182b85547f1508941a3d560eb7ea1fbf338", size = 24640306 },
            { url = "https://files.pythonhosted.org/packages/7c/30/8c844bfb770f045bcd8b2c83455c5afb45983e1a8abf0c4e5297b481b6a5/nvidia_cuda_nvrtc_cu12-12.4.127-py3-none-win_amd64.whl", hash = "sha256:a961b2f1d5f17b14867c619ceb99ef6fcec12e46612711bcec78eb05068a60ec", size = 19751955 },
        ]

        [[package]]
        name = "nvidia-cuda-runtime-cu12"
        version = "12.1.105"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/eb/d5/c68b1d2cdfcc59e72e8a5949a37ddb22ae6cade80cd4a57a84d4c8b55472/nvidia_cuda_runtime_cu12-12.1.105-py3-none-manylinux1_x86_64.whl", hash = "sha256:6e258468ddf5796e25f1dc591a31029fa317d97a0a94ed93468fc86301d61e40", size = 823596 },
            { url = "https://files.pythonhosted.org/packages/9f/e2/7a2b4b5064af56ea8ea2d8b2776c0f2960d95c88716138806121ae52a9c9/nvidia_cuda_runtime_cu12-12.1.105-py3-none-win_amd64.whl", hash = "sha256:dfb46ef84d73fababab44cf03e3b83f80700d27ca300e537f85f636fac474344", size = 821226 },
        ]

        [[package]]
        name = "nvidia-cuda-runtime-cu12"
        version = "12.4.127"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a1/aa/b656d755f474e2084971e9a297def515938d56b466ab39624012070cb773/nvidia_cuda_runtime_cu12-12.4.127-py3-none-manylinux2014_aarch64.whl", hash = "sha256:961fe0e2e716a2a1d967aab7caee97512f71767f852f67432d572e36cb3a11f3", size = 894177 },
            { url = "https://files.pythonhosted.org/packages/ea/27/1795d86fe88ef397885f2e580ac37628ed058a92ed2c39dc8eac3adf0619/nvidia_cuda_runtime_cu12-12.4.127-py3-none-manylinux2014_x86_64.whl", hash = "sha256:64403288fa2136ee8e467cdc9c9427e0434110899d07c779f25b5c068934faa5", size = 883737 },
            { url = "https://files.pythonhosted.org/packages/a8/8b/450e93fab75d85a69b50ea2d5fdd4ff44541e0138db16f9cd90123ef4de4/nvidia_cuda_runtime_cu12-12.4.127-py3-none-win_amd64.whl", hash = "sha256:09c2e35f48359752dfa822c09918211844a3d93c100a715d79b59591130c5e1e", size = 878808 },
        ]

        [[package]]
        name = "nvidia-cudnn-cu12"
        version = "8.9.2.26"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        dependencies = [
            { name = "nvidia-cublas-cu12", version = "12.1.3.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ff/74/a2e2be7fb83aaedec84f391f082cf765dfb635e7caa9b49065f73e4835d8/nvidia_cudnn_cu12-8.9.2.26-py3-none-manylinux1_x86_64.whl", hash = "sha256:5ccb288774fdfb07a7e7025ffec286971c06d8d7b4fb162525334616d7629ff9", size = 731725872 },
        ]

        [[package]]
        name = "nvidia-cudnn-cu12"
        version = "9.1.0.70"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        dependencies = [
            { name = "nvidia-cublas-cu12", version = "12.4.5.8", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9f/fd/713452cd72343f682b1c7b9321e23829f00b842ceaedcda96e742ea0b0b3/nvidia_cudnn_cu12-9.1.0.70-py3-none-manylinux2014_x86_64.whl", hash = "sha256:165764f44ef8c61fcdfdfdbe769d687e06374059fbb388b6c89ecb0e28793a6f", size = 664752741 },
            { url = "https://files.pythonhosted.org/packages/3f/d0/f90ee6956a628f9f04bf467932c0a25e5a7e706a684b896593c06c82f460/nvidia_cudnn_cu12-9.1.0.70-py3-none-win_amd64.whl", hash = "sha256:6278562929433d68365a07a4a1546c237ba2849852c0d4b2262a486e805b977a", size = 679925892 },
        ]

        [[package]]
        name = "nvidia-cufft-cu12"
        version = "11.0.2.54"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/86/94/eb540db023ce1d162e7bea9f8f5aa781d57c65aed513c33ee9a5123ead4d/nvidia_cufft_cu12-11.0.2.54-py3-none-manylinux1_x86_64.whl", hash = "sha256:794e3948a1aa71fd817c3775866943936774d1c14e7628c74f6f7417224cdf56", size = 121635161 },
            { url = "https://files.pythonhosted.org/packages/f7/57/7927a3aa0e19927dfed30256d1c854caf991655d847a4e7c01fe87e3d4ac/nvidia_cufft_cu12-11.0.2.54-py3-none-win_amd64.whl", hash = "sha256:d9ac353f78ff89951da4af698f80870b1534ed69993f10a4cf1d96f21357e253", size = 121344196 },
        ]

        [[package]]
        name = "nvidia-cufft-cu12"
        version = "11.2.1.3"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        dependencies = [
            { name = "nvidia-nvjitlink-cu12", version = "12.4.127", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7a/8a/0e728f749baca3fbeffad762738276e5df60851958be7783af121a7221e7/nvidia_cufft_cu12-11.2.1.3-py3-none-manylinux2014_aarch64.whl", hash = "sha256:5dad8008fc7f92f5ddfa2101430917ce2ffacd86824914c82e28990ad7f00399", size = 211422548 },
            { url = "https://files.pythonhosted.org/packages/27/94/3266821f65b92b3138631e9c8e7fe1fb513804ac934485a8d05776e1dd43/nvidia_cufft_cu12-11.2.1.3-py3-none-manylinux2014_x86_64.whl", hash = "sha256:f083fc24912aa410be21fa16d157fed2055dab1cc4b6934a0e03cba69eb242b9", size = 211459117 },
            { url = "https://files.pythonhosted.org/packages/f6/ee/3f3f8e9874f0be5bbba8fb4b62b3de050156d159f8b6edc42d6f1074113b/nvidia_cufft_cu12-11.2.1.3-py3-none-win_amd64.whl", hash = "sha256:d802f4954291101186078ccbe22fc285a902136f974d369540fd4a5333d1440b", size = 210576476 },
        ]

        [[package]]
        name = "nvidia-curand-cu12"
        version = "10.3.2.106"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/44/31/4890b1c9abc496303412947fc7dcea3d14861720642b49e8ceed89636705/nvidia_curand_cu12-10.3.2.106-py3-none-manylinux1_x86_64.whl", hash = "sha256:9d264c5036dde4e64f1de8c50ae753237c12e0b1348738169cd0f8a536c0e1e0", size = 56467784 },
            { url = "https://files.pythonhosted.org/packages/5c/97/4c9c7c79efcdf5b70374241d48cf03b94ef6707fd18ea0c0f53684931d0b/nvidia_curand_cu12-10.3.2.106-py3-none-win_amd64.whl", hash = "sha256:75b6b0c574c0037839121317e17fd01f8a69fd2ef8e25853d826fec30bdba74a", size = 55995813 },
        ]

        [[package]]
        name = "nvidia-curand-cu12"
        version = "10.3.5.147"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/80/9c/a79180e4d70995fdf030c6946991d0171555c6edf95c265c6b2bf7011112/nvidia_curand_cu12-10.3.5.147-py3-none-manylinux2014_aarch64.whl", hash = "sha256:1f173f09e3e3c76ab084aba0de819c49e56614feae5c12f69883f4ae9bb5fad9", size = 56314811 },
            { url = "https://files.pythonhosted.org/packages/8a/6d/44ad094874c6f1b9c654f8ed939590bdc408349f137f9b98a3a23ccec411/nvidia_curand_cu12-10.3.5.147-py3-none-manylinux2014_x86_64.whl", hash = "sha256:a88f583d4e0bb643c49743469964103aa59f7f708d862c3ddb0fc07f851e3b8b", size = 56305206 },
            { url = "https://files.pythonhosted.org/packages/1c/22/2573503d0d4e45673c263a313f79410e110eb562636b0617856fdb2ff5f6/nvidia_curand_cu12-10.3.5.147-py3-none-win_amd64.whl", hash = "sha256:f307cc191f96efe9e8f05a87096abc20d08845a841889ef78cb06924437f6771", size = 55799918 },
        ]

        [[package]]
        name = "nvidia-cusolver-cu12"
        version = "11.4.5.107"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        dependencies = [
            { name = "nvidia-cublas-cu12", version = "12.1.3.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "nvidia-cusparse-cu12", version = "12.1.0.106", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "nvidia-nvjitlink-cu12", version = "12.8.61", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bc/1d/8de1e5c67099015c834315e333911273a8c6aaba78923dd1d1e25fc5f217/nvidia_cusolver_cu12-11.4.5.107-py3-none-manylinux1_x86_64.whl", hash = "sha256:8a7ec542f0412294b15072fa7dab71d31334014a69f953004ea7a118206fe0dd", size = 124161928 },
            { url = "https://files.pythonhosted.org/packages/b8/80/8fca0bf819122a631c3976b6fc517c1b10741b643b94046bd8dd451522c5/nvidia_cusolver_cu12-11.4.5.107-py3-none-win_amd64.whl", hash = "sha256:74e0c3a24c78612192a74fcd90dd117f1cf21dea4822e66d89e8ea80e3cd2da5", size = 121643081 },
        ]

        [[package]]
        name = "nvidia-cusolver-cu12"
        version = "11.6.1.9"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        dependencies = [
            { name = "nvidia-cublas-cu12", version = "12.4.5.8", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
            { name = "nvidia-cusparse-cu12", version = "12.3.1.170", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
            { name = "nvidia-nvjitlink-cu12", version = "12.4.127", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/46/6b/a5c33cf16af09166845345275c34ad2190944bcc6026797a39f8e0a282e0/nvidia_cusolver_cu12-11.6.1.9-py3-none-manylinux2014_aarch64.whl", hash = "sha256:d338f155f174f90724bbde3758b7ac375a70ce8e706d70b018dd3375545fc84e", size = 127634111 },
            { url = "https://files.pythonhosted.org/packages/3a/e1/5b9089a4b2a4790dfdea8b3a006052cfecff58139d5a4e34cb1a51df8d6f/nvidia_cusolver_cu12-11.6.1.9-py3-none-manylinux2014_x86_64.whl", hash = "sha256:19e33fa442bcfd085b3086c4ebf7e8debc07cfe01e11513cc6d332fd918ac260", size = 127936057 },
            { url = "https://files.pythonhosted.org/packages/f2/be/d435b7b020e854d5d5a682eb5de4328fd62f6182507406f2818280e206e2/nvidia_cusolver_cu12-11.6.1.9-py3-none-win_amd64.whl", hash = "sha256:e77314c9d7b694fcebc84f58989f3aa4fb4cb442f12ca1a9bde50f5e8f6d1b9c", size = 125224015 },
        ]

        [[package]]
        name = "nvidia-cusparse-cu12"
        version = "12.1.0.106"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        dependencies = [
            { name = "nvidia-nvjitlink-cu12", version = "12.8.61", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/65/5b/cfaeebf25cd9fdec14338ccb16f6b2c4c7fa9163aefcf057d86b9cc248bb/nvidia_cusparse_cu12-12.1.0.106-py3-none-manylinux1_x86_64.whl", hash = "sha256:f3b50f42cf363f86ab21f720998517a659a48131e8d538dc02f8768237bd884c", size = 195958278 },
            { url = "https://files.pythonhosted.org/packages/0f/95/48fdbba24c93614d1ecd35bc6bdc6087bd17cbacc3abc4b05a9c2a1ca232/nvidia_cusparse_cu12-12.1.0.106-py3-none-win_amd64.whl", hash = "sha256:b798237e81b9719373e8fae8d4f091b70a0cf09d9d85c95a557e11df2d8e9a5a", size = 195414588 },
        ]

        [[package]]
        name = "nvidia-cusparse-cu12"
        version = "12.3.1.170"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        dependencies = [
            { name = "nvidia-nvjitlink-cu12", version = "12.4.127", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/96/a9/c0d2f83a53d40a4a41be14cea6a0bf9e668ffcf8b004bd65633f433050c0/nvidia_cusparse_cu12-12.3.1.170-py3-none-manylinux2014_aarch64.whl", hash = "sha256:9d32f62896231ebe0480efd8a7f702e143c98cfaa0e8a76df3386c1ba2b54df3", size = 207381987 },
            { url = "https://files.pythonhosted.org/packages/db/f7/97a9ea26ed4bbbfc2d470994b8b4f338ef663be97b8f677519ac195e113d/nvidia_cusparse_cu12-12.3.1.170-py3-none-manylinux2014_x86_64.whl", hash = "sha256:ea4f11a2904e2a8dc4b1833cc1b5181cde564edd0d5cd33e3c168eff2d1863f1", size = 207454763 },
            { url = "https://files.pythonhosted.org/packages/a2/e0/3155ca539760a8118ec94cc279b34293309bcd14011fc724f87f31988843/nvidia_cusparse_cu12-12.3.1.170-py3-none-win_amd64.whl", hash = "sha256:9bc90fb087bc7b4c15641521f31c0371e9a612fc2ba12c338d3ae032e6b6797f", size = 204684315 },
        ]

        [[package]]
        name = "nvidia-ml-py3"
        version = "7.352.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/6d/64/cce82bddb80c0b0f5c703bbdafa94bfb69a1c5ad7a79cff00b482468f0d3/nvidia-ml-py3-7.352.0.tar.gz", hash = "sha256:390f02919ee9d73fe63a98c73101061a6b37fa694a793abf56673320f1f51277", size = 19494 }

        [[package]]
        name = "nvidia-nccl-cu12"
        version = "2.19.3"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/00/d0d4e48aef772ad5aebcf70b73028f88db6e5640b36c38e90445b7a57c45/nvidia_nccl_cu12-2.19.3-py3-none-manylinux1_x86_64.whl", hash = "sha256:a9734707a2c96443331c1e48c717024aa6678a0e2a4cb66b2c364d18cee6b48d", size = 165987969 },
        ]

        [[package]]
        name = "nvidia-nccl-cu12"
        version = "2.21.5"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/df/99/12cd266d6233f47d00daf3a72739872bdc10267d0383508b0b9c84a18bb6/nvidia_nccl_cu12-2.21.5-py3-none-manylinux2014_x86_64.whl", hash = "sha256:8579076d30a8c24988834445f8d633c697d42397e92ffc3f63fa26766d25e0a0", size = 188654414 },
        ]

        [[package]]
        name = "nvidia-nvjitlink-cu12"
        version = "12.4.127"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/02/45/239d52c05074898a80a900f49b1615d81c07fceadd5ad6c4f86a987c0bc4/nvidia_nvjitlink_cu12-12.4.127-py3-none-manylinux2014_aarch64.whl", hash = "sha256:4abe7fef64914ccfa909bc2ba39739670ecc9e820c83ccc7a6ed414122599b83", size = 20552510 },
            { url = "https://files.pythonhosted.org/packages/ff/ff/847841bacfbefc97a00036e0fce5a0f086b640756dc38caea5e1bb002655/nvidia_nvjitlink_cu12-12.4.127-py3-none-manylinux2014_x86_64.whl", hash = "sha256:06b3b9b25bf3f8af351d664978ca26a16d2c5127dbd53c0497e28d1fb9611d57", size = 21066810 },
            { url = "https://files.pythonhosted.org/packages/81/19/0babc919031bee42620257b9a911c528f05fb2688520dcd9ca59159ffea8/nvidia_nvjitlink_cu12-12.4.127-py3-none-win_amd64.whl", hash = "sha256:fd9020c501d27d135f983c6d3e244b197a7ccad769e34df53a42e276b0e25fa1", size = 95336325 },
        ]

        [[package]]
        name = "nvidia-nvjitlink-cu12"
        version = "12.8.61"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/03/f8/9d85593582bd99b8d7c65634d2304780aefade049b2b94d96e44084be90b/nvidia_nvjitlink_cu12-12.8.61-py3-none-manylinux2010_x86_64.manylinux_2_12_x86_64.whl", hash = "sha256:45fd79f2ae20bd67e8bc411055939049873bfd8fac70ff13bd4865e0b9bdab17", size = 39243473 },
            { url = "https://files.pythonhosted.org/packages/af/53/698f3758f48c5fcb1112721e40cc6714da3980d3c7e93bae5b29dafa9857/nvidia_nvjitlink_cu12-12.8.61-py3-none-manylinux2014_aarch64.manylinux_2_17_aarch64.whl", hash = "sha256:9b80ecab31085dda3ce3b41d043be0ec739216c3fc633b8abe212d5a30026df0", size = 38374634 },
            { url = "https://files.pythonhosted.org/packages/7f/c6/0d1b2bfeb2ef42c06db0570c4d081e5cde4450b54c09e43165126cfe6ff6/nvidia_nvjitlink_cu12-12.8.61-py3-none-win_amd64.whl", hash = "sha256:1166a964d25fdc0eae497574d38824305195a5283324a21ccb0ce0c802cbf41c", size = 268514099 },
        ]

        [[package]]
        name = "nvidia-nvtx-cu12"
        version = "12.1.105"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/da/d3/8057f0587683ed2fcd4dbfbdfdfa807b9160b809976099d36b8f60d08f03/nvidia_nvtx_cu12-12.1.105-py3-none-manylinux1_x86_64.whl", hash = "sha256:dc21cf308ca5691e7c04d962e213f8a4aa9bbfa23d95412f452254c2caeb09e5", size = 99138 },
            { url = "https://files.pythonhosted.org/packages/b8/d7/bd7cb2d95ac6ac6e8d05bfa96cdce69619f1ef2808e072919044c2d47a8c/nvidia_nvtx_cu12-12.1.105-py3-none-win_amd64.whl", hash = "sha256:65f4d98982b31b60026e0e6de73fbdfc09d08a96f4656dd3665ca616a11e1e82", size = 66307 },
        ]

        [[package]]
        name = "nvidia-nvtx-cu12"
        version = "12.4.127"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/06/39/471f581edbb7804b39e8063d92fc8305bdc7a80ae5c07dbe6ea5c50d14a5/nvidia_nvtx_cu12-12.4.127-py3-none-manylinux2014_aarch64.whl", hash = "sha256:7959ad635db13edf4fc65c06a6e9f9e55fc2f92596db928d169c0bb031e88ef3", size = 100417 },
            { url = "https://files.pythonhosted.org/packages/87/20/199b8713428322a2f22b722c62b8cc278cc53dffa9705d744484b5035ee9/nvidia_nvtx_cu12-12.4.127-py3-none-manylinux2014_x86_64.whl", hash = "sha256:781e950d9b9f60d8241ccea575b32f5105a5baf4c2351cab5256a24869f12a1a", size = 99144 },
            { url = "https://files.pythonhosted.org/packages/54/1b/f77674fbb73af98843be25803bbd3b9a4f0a96c75b8d33a2854a5c7d2d77/nvidia_nvtx_cu12-12.4.127-py3-none-win_amd64.whl", hash = "sha256:641dccaaa1139f3ffb0d3164b4b84f9d253397e38246a4f2f36728b48566d485", size = 66307 },
        ]

        [[package]]
        name = "opt-einsum"
        version = "3.4.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8c/b9/2ac072041e899a52f20cf9510850ff58295003aa75525e58343591b0cbfb/opt_einsum-3.4.0.tar.gz", hash = "sha256:96ca72f1b886d148241348783498194c577fa30a8faac108586b14f1ba4473ac", size = 63004 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/23/cd/066e86230ae37ed0be70aae89aabf03ca8d9f39c8aea0dec8029455b5540/opt_einsum-3.4.0-py3-none-any.whl", hash = "sha256:69bb92469f86a1565195ece4ac0323943e83477171b91d24c35afe028a90d7cd", size = 71932 },
        ]

        [[package]]
        name = "opt-einsum-fx"
        version = "0.1.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "opt-einsum" },
            { name = "packaging" },
            { name = "torch", version = "2.2.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "torch", version = "2.5.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/93/de/856dab99be0360c7275fee075eb0450a2ec82a54c4c33689606f62e9615b/opt_einsum_fx-0.1.4.tar.gz", hash = "sha256:7eeb7f91ecb70be65e6179c106ea7f64fc1db6319e3d1289a4518b384f81e74f", size = 12969 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/8d/4c/e0370709aaf9d7ceb68f975cac559751e75954429a77e83202e680606560/opt_einsum_fx-0.1.4-py3-none-any.whl", hash = "sha256:85f489f4c7c31fd88d5faf9669c09e61ec37a30098809fdcfe2a08a9e42f23c9", size = 13213 },
        ]

        [[package]]
        name = "packaging"
        version = "24.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d0/63/68dbb6eb2de9cb10ee4c9c14a0148804425e13c4fb20d61cce69f53106da/packaging-24.2.tar.gz", hash = "sha256:c228a6dc5e932d346bc5739379109d49e8853dd8223571c7c5b55260edc0b97f", size = 163950 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/88/ef/eb23f262cca3c0c4eb7ab1933c3b1f03d021f2c48f54763065b6f0e321be/packaging-24.2-py3-none-any.whl", hash = "sha256:09abb1bccd265c01f4a3aa3f7a7db064b36514d2cba19a2f694fe6150451a759", size = 65451 },
        ]

        [[package]]
        name = "palettable"
        version = "3.3.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/dc/3d/a5854d60850485bff12f28abfe0e17f503e866763bed61aed4990b604530/palettable-3.3.3.tar.gz", hash = "sha256:094dd7d9a5fc1cca4854773e5c1fc6a315b33bd5b3a8f47064928facaf0490a8", size = 106639 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cf/f7/3367feadd4ab56783b0971c9b7edfbdd68e0c70ce877949a5dd2117ed4a0/palettable-3.3.3-py2.py3-none-any.whl", hash = "sha256:74e9e7d7fe5a9be065e02397558ed1777b2df0b793a6f4ce1a5ee74f74fb0caa", size = 332251 },
        ]

        [[package]]
        name = "pandas"
        version = "2.2.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "numpy" },
            { name = "python-dateutil" },
            { name = "pytz" },
            { name = "tzdata" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9c/d6/9f8431bacc2e19dca897724cd097b1bb224a6ad5433784a44b587c7c13af/pandas-2.2.3.tar.gz", hash = "sha256:4f18ba62b61d7e192368b84517265a99b4d7ee8912f8708660fb4a366cc82667", size = 4399213 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/aa/70/c853aec59839bceed032d52010ff5f1b8d87dc3114b762e4ba2727661a3b/pandas-2.2.3-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:1948ddde24197a0f7add2bdc4ca83bf2b1ef84a1bc8ccffd95eda17fd836ecb5", size = 12580827 },
            { url = "https://files.pythonhosted.org/packages/99/f2/c4527768739ffa4469b2b4fff05aa3768a478aed89a2f271a79a40eee984/pandas-2.2.3-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:381175499d3802cde0eabbaf6324cce0c4f5d52ca6f8c377c29ad442f50f6348", size = 11303897 },
            { url = "https://files.pythonhosted.org/packages/ed/12/86c1747ea27989d7a4064f806ce2bae2c6d575b950be087837bdfcabacc9/pandas-2.2.3-cp310-cp310-manylinux2014_aarch64.manylinux_2_17_aarch64.whl", hash = "sha256:d9c45366def9a3dd85a6454c0e7908f2b3b8e9c138f5dc38fed7ce720d8453ed", size = 66480908 },
            { url = "https://files.pythonhosted.org/packages/44/50/7db2cd5e6373ae796f0ddad3675268c8d59fb6076e66f0c339d61cea886b/pandas-2.2.3-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:86976a1c5b25ae3f8ccae3a5306e443569ee3c3faf444dfd0f41cda24667ad57", size = 13064210 },
            { url = "https://files.pythonhosted.org/packages/61/61/a89015a6d5536cb0d6c3ba02cebed51a95538cf83472975275e28ebf7d0c/pandas-2.2.3-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:b8661b0238a69d7aafe156b7fa86c44b881387509653fdf857bebc5e4008ad42", size = 16754292 },
            { url = "https://files.pythonhosted.org/packages/ce/0d/4cc7b69ce37fac07645a94e1d4b0880b15999494372c1523508511b09e40/pandas-2.2.3-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:37e0aced3e8f539eccf2e099f65cdb9c8aa85109b0be6e93e2baff94264bdc6f", size = 14416379 },
            { url = "https://files.pythonhosted.org/packages/31/9e/6ebb433de864a6cd45716af52a4d7a8c3c9aaf3a98368e61db9e69e69a9c/pandas-2.2.3-cp310-cp310-win_amd64.whl", hash = "sha256:56534ce0746a58afaf7942ba4863e0ef81c9c50d3f0ae93e9497d6a41a057645", size = 11598471 },
            { url = "https://files.pythonhosted.org/packages/a8/44/d9502bf0ed197ba9bf1103c9867d5904ddcaf869e52329787fc54ed70cc8/pandas-2.2.3-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:66108071e1b935240e74525006034333f98bcdb87ea116de573a6a0dccb6c039", size = 12602222 },
            { url = "https://files.pythonhosted.org/packages/52/11/9eac327a38834f162b8250aab32a6781339c69afe7574368fffe46387edf/pandas-2.2.3-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:7c2875855b0ff77b2a64a0365e24455d9990730d6431b9e0ee18ad8acee13dbd", size = 11321274 },
            { url = "https://files.pythonhosted.org/packages/45/fb/c4beeb084718598ba19aa9f5abbc8aed8b42f90930da861fcb1acdb54c3a/pandas-2.2.3-cp311-cp311-manylinux2014_aarch64.manylinux_2_17_aarch64.whl", hash = "sha256:cd8d0c3be0515c12fed0bdbae072551c8b54b7192c7b1fda0ba56059a0179698", size = 15579836 },
            { url = "https://files.pythonhosted.org/packages/cd/5f/4dba1d39bb9c38d574a9a22548c540177f78ea47b32f99c0ff2ec499fac5/pandas-2.2.3-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c124333816c3a9b03fbeef3a9f230ba9a737e9e5bb4060aa2107a86cc0a497fc", size = 13058505 },
            { url = "https://files.pythonhosted.org/packages/b9/57/708135b90391995361636634df1f1130d03ba456e95bcf576fada459115a/pandas-2.2.3-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:63cc132e40a2e084cf01adf0775b15ac515ba905d7dcca47e9a251819c575ef3", size = 16744420 },
            { url = "https://files.pythonhosted.org/packages/86/4a/03ed6b7ee323cf30404265c284cee9c65c56a212e0a08d9ee06984ba2240/pandas-2.2.3-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:29401dbfa9ad77319367d36940cd8a0b3a11aba16063e39632d98b0e931ddf32", size = 14440457 },
            { url = "https://files.pythonhosted.org/packages/ed/8c/87ddf1fcb55d11f9f847e3c69bb1c6f8e46e2f40ab1a2d2abadb2401b007/pandas-2.2.3-cp311-cp311-win_amd64.whl", hash = "sha256:3fc6873a41186404dad67245896a6e440baacc92f5b716ccd1bc9ed2995ab2c5", size = 11617166 },
            { url = "https://files.pythonhosted.org/packages/17/a3/fb2734118db0af37ea7433f57f722c0a56687e14b14690edff0cdb4b7e58/pandas-2.2.3-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:b1d432e8d08679a40e2a6d8b2f9770a5c21793a6f9f47fdd52c5ce1948a5a8a9", size = 12529893 },
            { url = "https://files.pythonhosted.org/packages/e1/0c/ad295fd74bfac85358fd579e271cded3ac969de81f62dd0142c426b9da91/pandas-2.2.3-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:a5a1595fe639f5988ba6a8e5bc9649af3baf26df3998a0abe56c02609392e0a4", size = 11363475 },
            { url = "https://files.pythonhosted.org/packages/c6/2a/4bba3f03f7d07207481fed47f5b35f556c7441acddc368ec43d6643c5777/pandas-2.2.3-cp312-cp312-manylinux2014_aarch64.manylinux_2_17_aarch64.whl", hash = "sha256:5de54125a92bb4d1c051c0659e6fcb75256bf799a732a87184e5ea503965bce3", size = 15188645 },
            { url = "https://files.pythonhosted.org/packages/38/f8/d8fddee9ed0d0c0f4a2132c1dfcf0e3e53265055da8df952a53e7eaf178c/pandas-2.2.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:fffb8ae78d8af97f849404f21411c95062db1496aeb3e56f146f0355c9989319", size = 12739445 },
            { url = "https://files.pythonhosted.org/packages/20/e8/45a05d9c39d2cea61ab175dbe6a2de1d05b679e8de2011da4ee190d7e748/pandas-2.2.3-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:6dfcb5ee8d4d50c06a51c2fffa6cff6272098ad6540aed1a76d15fb9318194d8", size = 16359235 },
            { url = "https://files.pythonhosted.org/packages/1d/99/617d07a6a5e429ff90c90da64d428516605a1ec7d7bea494235e1c3882de/pandas-2.2.3-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:062309c1b9ea12a50e8ce661145c6aab431b1e99530d3cd60640e255778bd43a", size = 14056756 },
            { url = "https://files.pythonhosted.org/packages/29/d4/1244ab8edf173a10fd601f7e13b9566c1b525c4f365d6bee918e68381889/pandas-2.2.3-cp312-cp312-win_amd64.whl", hash = "sha256:59ef3764d0fe818125a5097d2ae867ca3fa64df032331b7e0917cf5d7bf66b13", size = 11504248 },
            { url = "https://files.pythonhosted.org/packages/64/22/3b8f4e0ed70644e85cfdcd57454686b9057c6c38d2f74fe4b8bc2527214a/pandas-2.2.3-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:f00d1345d84d8c86a63e476bb4955e46458b304b9575dcf71102b5c705320015", size = 12477643 },
            { url = "https://files.pythonhosted.org/packages/e4/93/b3f5d1838500e22c8d793625da672f3eec046b1a99257666c94446969282/pandas-2.2.3-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:3508d914817e153ad359d7e069d752cdd736a247c322d932eb89e6bc84217f28", size = 11281573 },
            { url = "https://files.pythonhosted.org/packages/f5/94/6c79b07f0e5aab1dcfa35a75f4817f5c4f677931d4234afcd75f0e6a66ca/pandas-2.2.3-cp313-cp313-manylinux2014_aarch64.manylinux_2_17_aarch64.whl", hash = "sha256:22a9d949bfc9a502d320aa04e5d02feab689d61da4e7764b62c30b991c42c5f0", size = 15196085 },
            { url = "https://files.pythonhosted.org/packages/e8/31/aa8da88ca0eadbabd0a639788a6da13bb2ff6edbbb9f29aa786450a30a91/pandas-2.2.3-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f3a255b2c19987fbbe62a9dfd6cff7ff2aa9ccab3fc75218fd4b7530f01efa24", size = 12711809 },
            { url = "https://files.pythonhosted.org/packages/ee/7c/c6dbdb0cb2a4344cacfb8de1c5808ca885b2e4dcfde8008266608f9372af/pandas-2.2.3-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:800250ecdadb6d9c78eae4990da62743b857b470883fa27f652db8bdde7f6659", size = 16356316 },
            { url = "https://files.pythonhosted.org/packages/57/b7/8b757e7d92023b832869fa8881a992696a0bfe2e26f72c9ae9f255988d42/pandas-2.2.3-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:6374c452ff3ec675a8f46fd9ab25c4ad0ba590b71cf0656f8b6daa5202bca3fb", size = 14022055 },
            { url = "https://files.pythonhosted.org/packages/3b/bc/4b18e2b8c002572c5a441a64826252ce5da2aa738855747247a971988043/pandas-2.2.3-cp313-cp313-win_amd64.whl", hash = "sha256:61c5ad4043f791b61dd4752191d9f07f0ae412515d59ba8f005832a532f8736d", size = 11481175 },
            { url = "https://files.pythonhosted.org/packages/76/a3/a5d88146815e972d40d19247b2c162e88213ef51c7c25993942c39dbf41d/pandas-2.2.3-cp313-cp313t-macosx_10_13_x86_64.whl", hash = "sha256:3b71f27954685ee685317063bf13c7709a7ba74fc996b84fc6821c59b0f06468", size = 12615650 },
            { url = "https://files.pythonhosted.org/packages/9c/8c/f0fd18f6140ddafc0c24122c8a964e48294acc579d47def376fef12bcb4a/pandas-2.2.3-cp313-cp313t-macosx_11_0_arm64.whl", hash = "sha256:38cf8125c40dae9d5acc10fa66af8ea6fdf760b2714ee482ca691fc66e6fcb18", size = 11290177 },
            { url = "https://files.pythonhosted.org/packages/ed/f9/e995754eab9c0f14c6777401f7eece0943840b7a9fc932221c19d1abee9f/pandas-2.2.3-cp313-cp313t-manylinux2014_aarch64.manylinux_2_17_aarch64.whl", hash = "sha256:ba96630bc17c875161df3818780af30e43be9b166ce51c9a18c1feae342906c2", size = 14651526 },
            { url = "https://files.pythonhosted.org/packages/25/b0/98d6ae2e1abac4f35230aa756005e8654649d305df9a28b16b9ae4353bff/pandas-2.2.3-cp313-cp313t-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:1db71525a1538b30142094edb9adc10be3f3e176748cd7acc2240c2f2e5aa3a4", size = 11871013 },
            { url = "https://files.pythonhosted.org/packages/cc/57/0f72a10f9db6a4628744c8e8f0df4e6e21de01212c7c981d31e50ffc8328/pandas-2.2.3-cp313-cp313t-musllinux_1_2_aarch64.whl", hash = "sha256:15c0e1e02e93116177d29ff83e8b1619c93ddc9c49083f237d4312337a61165d", size = 15711620 },
            { url = "https://files.pythonhosted.org/packages/ab/5f/b38085618b950b79d2d9164a711c52b10aefc0ae6833b96f626b7021b2ed/pandas-2.2.3-cp313-cp313t-musllinux_1_2_x86_64.whl", hash = "sha256:ad5b65698ab28ed8d7f18790a0dc58005c7629f227be9ecc1072aa74c0c1d43a", size = 13098436 },
        ]

        [[package]]
        name = "pillow"
        version = "11.1.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/f3/af/c097e544e7bd278333db77933e535098c259609c4eb3b85381109602fb5b/pillow-11.1.0.tar.gz", hash = "sha256:368da70808b36d73b4b390a8ffac11069f8a5c85f29eff1f1b01bcf3ef5b2a20", size = 46742715 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/50/1c/2dcea34ac3d7bc96a1fd1bd0a6e06a57c67167fec2cff8d95d88229a8817/pillow-11.1.0-cp310-cp310-macosx_10_10_x86_64.whl", hash = "sha256:e1abe69aca89514737465752b4bcaf8016de61b3be1397a8fc260ba33321b3a8", size = 3229983 },
            { url = "https://files.pythonhosted.org/packages/14/ca/6bec3df25e4c88432681de94a3531cc738bd85dea6c7aa6ab6f81ad8bd11/pillow-11.1.0-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:c640e5a06869c75994624551f45e5506e4256562ead981cce820d5ab39ae2192", size = 3101831 },
            { url = "https://files.pythonhosted.org/packages/d4/2c/668e18e5521e46eb9667b09e501d8e07049eb5bfe39d56be0724a43117e6/pillow-11.1.0-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:a07dba04c5e22824816b2615ad7a7484432d7f540e6fa86af60d2de57b0fcee2", size = 4314074 },
            { url = "https://files.pythonhosted.org/packages/02/80/79f99b714f0fc25f6a8499ecfd1f810df12aec170ea1e32a4f75746051ce/pillow-11.1.0-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:e267b0ed063341f3e60acd25c05200df4193e15a4a5807075cd71225a2386e26", size = 4394933 },
            { url = "https://files.pythonhosted.org/packages/81/aa/8d4ad25dc11fd10a2001d5b8a80fdc0e564ac33b293bdfe04ed387e0fd95/pillow-11.1.0-cp310-cp310-manylinux_2_28_aarch64.whl", hash = "sha256:bd165131fd51697e22421d0e467997ad31621b74bfc0b75956608cb2906dda07", size = 4353349 },
            { url = "https://files.pythonhosted.org/packages/84/7a/cd0c3eaf4a28cb2a74bdd19129f7726277a7f30c4f8424cd27a62987d864/pillow-11.1.0-cp310-cp310-manylinux_2_28_x86_64.whl", hash = "sha256:abc56501c3fd148d60659aae0af6ddc149660469082859fa7b066a298bde9482", size = 4476532 },
            { url = "https://files.pythonhosted.org/packages/8f/8b/a907fdd3ae8f01c7670dfb1499c53c28e217c338b47a813af8d815e7ce97/pillow-11.1.0-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:54ce1c9a16a9561b6d6d8cb30089ab1e5eb66918cb47d457bd996ef34182922e", size = 4279789 },
            { url = "https://files.pythonhosted.org/packages/6f/9a/9f139d9e8cccd661c3efbf6898967a9a337eb2e9be2b454ba0a09533100d/pillow-11.1.0-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:73ddde795ee9b06257dac5ad42fcb07f3b9b813f8c1f7f870f402f4dc54b5269", size = 4413131 },
            { url = "https://files.pythonhosted.org/packages/a8/68/0d8d461f42a3f37432203c8e6df94da10ac8081b6d35af1c203bf3111088/pillow-11.1.0-cp310-cp310-win32.whl", hash = "sha256:3a5fe20a7b66e8135d7fd617b13272626a28278d0e578c98720d9ba4b2439d49", size = 2291213 },
            { url = "https://files.pythonhosted.org/packages/14/81/d0dff759a74ba87715509af9f6cb21fa21d93b02b3316ed43bda83664db9/pillow-11.1.0-cp310-cp310-win_amd64.whl", hash = "sha256:b6123aa4a59d75f06e9dd3dac5bf8bc9aa383121bb3dd9a7a612e05eabc9961a", size = 2625725 },
            { url = "https://files.pythonhosted.org/packages/ce/1f/8d50c096a1d58ef0584ddc37e6f602828515219e9d2428e14ce50f5ecad1/pillow-11.1.0-cp310-cp310-win_arm64.whl", hash = "sha256:a76da0a31da6fcae4210aa94fd779c65c75786bc9af06289cd1c184451ef7a65", size = 2375213 },
            { url = "https://files.pythonhosted.org/packages/dd/d6/2000bfd8d5414fb70cbbe52c8332f2283ff30ed66a9cde42716c8ecbe22c/pillow-11.1.0-cp311-cp311-macosx_10_10_x86_64.whl", hash = "sha256:e06695e0326d05b06833b40b7ef477e475d0b1ba3a6d27da1bb48c23209bf457", size = 3229968 },
            { url = "https://files.pythonhosted.org/packages/d9/45/3fe487010dd9ce0a06adf9b8ff4f273cc0a44536e234b0fad3532a42c15b/pillow-11.1.0-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:96f82000e12f23e4f29346e42702b6ed9a2f2fea34a740dd5ffffcc8c539eb35", size = 3101806 },
            { url = "https://files.pythonhosted.org/packages/e3/72/776b3629c47d9d5f1c160113158a7a7ad177688d3a1159cd3b62ded5a33a/pillow-11.1.0-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:a3cd561ded2cf2bbae44d4605837221b987c216cff94f49dfeed63488bb228d2", size = 4322283 },
            { url = "https://files.pythonhosted.org/packages/e4/c2/e25199e7e4e71d64eeb869f5b72c7ddec70e0a87926398785ab944d92375/pillow-11.1.0-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f189805c8be5ca5add39e6f899e6ce2ed824e65fb45f3c28cb2841911da19070", size = 4402945 },
            { url = "https://files.pythonhosted.org/packages/c1/ed/51d6136c9d5911f78632b1b86c45241c712c5a80ed7fa7f9120a5dff1eba/pillow-11.1.0-cp311-cp311-manylinux_2_28_aarch64.whl", hash = "sha256:dd0052e9db3474df30433f83a71b9b23bd9e4ef1de13d92df21a52c0303b8ab6", size = 4361228 },
            { url = "https://files.pythonhosted.org/packages/48/a4/fbfe9d5581d7b111b28f1d8c2762dee92e9821bb209af9fa83c940e507a0/pillow-11.1.0-cp311-cp311-manylinux_2_28_x86_64.whl", hash = "sha256:837060a8599b8f5d402e97197d4924f05a2e0d68756998345c829c33186217b1", size = 4484021 },
            { url = "https://files.pythonhosted.org/packages/39/db/0b3c1a5018117f3c1d4df671fb8e47d08937f27519e8614bbe86153b65a5/pillow-11.1.0-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:aa8dd43daa836b9a8128dbe7d923423e5ad86f50a7a14dc688194b7be5c0dea2", size = 4287449 },
            { url = "https://files.pythonhosted.org/packages/d9/58/bc128da7fea8c89fc85e09f773c4901e95b5936000e6f303222490c052f3/pillow-11.1.0-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:0a2f91f8a8b367e7a57c6e91cd25af510168091fb89ec5146003e424e1558a96", size = 4419972 },
            { url = "https://files.pythonhosted.org/packages/5f/bb/58f34379bde9fe197f51841c5bbe8830c28bbb6d3801f16a83b8f2ad37df/pillow-11.1.0-cp311-cp311-win32.whl", hash = "sha256:c12fc111ef090845de2bb15009372175d76ac99969bdf31e2ce9b42e4b8cd88f", size = 2291201 },
            { url = "https://files.pythonhosted.org/packages/3a/c6/fce9255272bcf0c39e15abd2f8fd8429a954cf344469eaceb9d0d1366913/pillow-11.1.0-cp311-cp311-win_amd64.whl", hash = "sha256:fbd43429d0d7ed6533b25fc993861b8fd512c42d04514a0dd6337fb3ccf22761", size = 2625686 },
            { url = "https://files.pythonhosted.org/packages/c8/52/8ba066d569d932365509054859f74f2a9abee273edcef5cd75e4bc3e831e/pillow-11.1.0-cp311-cp311-win_arm64.whl", hash = "sha256:f7955ecf5609dee9442cbface754f2c6e541d9e6eda87fad7f7a989b0bdb9d71", size = 2375194 },
            { url = "https://files.pythonhosted.org/packages/95/20/9ce6ed62c91c073fcaa23d216e68289e19d95fb8188b9fb7a63d36771db8/pillow-11.1.0-cp312-cp312-macosx_10_13_x86_64.whl", hash = "sha256:2062ffb1d36544d42fcaa277b069c88b01bb7298f4efa06731a7fd6cc290b81a", size = 3226818 },
            { url = "https://files.pythonhosted.org/packages/b9/d8/f6004d98579a2596c098d1e30d10b248798cceff82d2b77aa914875bfea1/pillow-11.1.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:a85b653980faad27e88b141348707ceeef8a1186f75ecc600c395dcac19f385b", size = 3101662 },
            { url = "https://files.pythonhosted.org/packages/08/d9/892e705f90051c7a2574d9f24579c9e100c828700d78a63239676f960b74/pillow-11.1.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:9409c080586d1f683df3f184f20e36fb647f2e0bc3988094d4fd8c9f4eb1b3b3", size = 4329317 },
            { url = "https://files.pythonhosted.org/packages/8c/aa/7f29711f26680eab0bcd3ecdd6d23ed6bce180d82e3f6380fb7ae35fcf3b/pillow-11.1.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:7fdadc077553621911f27ce206ffcbec7d3f8d7b50e0da39f10997e8e2bb7f6a", size = 4412999 },
            { url = "https://files.pythonhosted.org/packages/c8/c4/8f0fe3b9e0f7196f6d0bbb151f9fba323d72a41da068610c4c960b16632a/pillow-11.1.0-cp312-cp312-manylinux_2_28_aarch64.whl", hash = "sha256:93a18841d09bcdd774dcdc308e4537e1f867b3dec059c131fde0327899734aa1", size = 4368819 },
            { url = "https://files.pythonhosted.org/packages/38/0d/84200ed6a871ce386ddc82904bfadc0c6b28b0c0ec78176871a4679e40b3/pillow-11.1.0-cp312-cp312-manylinux_2_28_x86_64.whl", hash = "sha256:9aa9aeddeed452b2f616ff5507459e7bab436916ccb10961c4a382cd3e03f47f", size = 4496081 },
            { url = "https://files.pythonhosted.org/packages/84/9c/9bcd66f714d7e25b64118e3952d52841a4babc6d97b6d28e2261c52045d4/pillow-11.1.0-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:3cdcdb0b896e981678eee140d882b70092dac83ac1cdf6b3a60e2216a73f2b91", size = 4296513 },
            { url = "https://files.pythonhosted.org/packages/db/61/ada2a226e22da011b45f7104c95ebda1b63dcbb0c378ad0f7c2a710f8fd2/pillow-11.1.0-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:36ba10b9cb413e7c7dfa3e189aba252deee0602c86c309799da5a74009ac7a1c", size = 4431298 },
            { url = "https://files.pythonhosted.org/packages/e7/c4/fc6e86750523f367923522014b821c11ebc5ad402e659d8c9d09b3c9d70c/pillow-11.1.0-cp312-cp312-win32.whl", hash = "sha256:cfd5cd998c2e36a862d0e27b2df63237e67273f2fc78f47445b14e73a810e7e6", size = 2291630 },
            { url = "https://files.pythonhosted.org/packages/08/5c/2104299949b9d504baf3f4d35f73dbd14ef31bbd1ddc2c1b66a5b7dfda44/pillow-11.1.0-cp312-cp312-win_amd64.whl", hash = "sha256:a697cd8ba0383bba3d2d3ada02b34ed268cb548b369943cd349007730c92bddf", size = 2626369 },
            { url = "https://files.pythonhosted.org/packages/37/f3/9b18362206b244167c958984b57c7f70a0289bfb59a530dd8af5f699b910/pillow-11.1.0-cp312-cp312-win_arm64.whl", hash = "sha256:4dd43a78897793f60766563969442020e90eb7847463eca901e41ba186a7d4a5", size = 2375240 },
            { url = "https://files.pythonhosted.org/packages/b3/31/9ca79cafdce364fd5c980cd3416c20ce1bebd235b470d262f9d24d810184/pillow-11.1.0-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:ae98e14432d458fc3de11a77ccb3ae65ddce70f730e7c76140653048c71bfcbc", size = 3226640 },
            { url = "https://files.pythonhosted.org/packages/ac/0f/ff07ad45a1f172a497aa393b13a9d81a32e1477ef0e869d030e3c1532521/pillow-11.1.0-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:cc1331b6d5a6e144aeb5e626f4375f5b7ae9934ba620c0ac6b3e43d5e683a0f0", size = 3101437 },
            { url = "https://files.pythonhosted.org/packages/08/2f/9906fca87a68d29ec4530be1f893149e0cb64a86d1f9f70a7cfcdfe8ae44/pillow-11.1.0-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:758e9d4ef15d3560214cddbc97b8ef3ef86ce04d62ddac17ad39ba87e89bd3b1", size = 4326605 },
            { url = "https://files.pythonhosted.org/packages/b0/0f/f3547ee15b145bc5c8b336401b2d4c9d9da67da9dcb572d7c0d4103d2c69/pillow-11.1.0-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:b523466b1a31d0dcef7c5be1f20b942919b62fd6e9a9be199d035509cbefc0ec", size = 4411173 },
            { url = "https://files.pythonhosted.org/packages/b1/df/bf8176aa5db515c5de584c5e00df9bab0713548fd780c82a86cba2c2fedb/pillow-11.1.0-cp313-cp313-manylinux_2_28_aarch64.whl", hash = "sha256:9044b5e4f7083f209c4e35aa5dd54b1dd5b112b108648f5c902ad586d4f945c5", size = 4369145 },
            { url = "https://files.pythonhosted.org/packages/de/7c/7433122d1cfadc740f577cb55526fdc39129a648ac65ce64db2eb7209277/pillow-11.1.0-cp313-cp313-manylinux_2_28_x86_64.whl", hash = "sha256:3764d53e09cdedd91bee65c2527815d315c6b90d7b8b79759cc48d7bf5d4f114", size = 4496340 },
            { url = "https://files.pythonhosted.org/packages/25/46/dd94b93ca6bd555588835f2504bd90c00d5438fe131cf01cfa0c5131a19d/pillow-11.1.0-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:31eba6bbdd27dde97b0174ddf0297d7a9c3a507a8a1480e1e60ef914fe23d352", size = 4296906 },
            { url = "https://files.pythonhosted.org/packages/a8/28/2f9d32014dfc7753e586db9add35b8a41b7a3b46540e965cb6d6bc607bd2/pillow-11.1.0-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:b5d658fbd9f0d6eea113aea286b21d3cd4d3fd978157cbf2447a6035916506d3", size = 4431759 },
            { url = "https://files.pythonhosted.org/packages/33/48/19c2cbe7403870fbe8b7737d19eb013f46299cdfe4501573367f6396c775/pillow-11.1.0-cp313-cp313-win32.whl", hash = "sha256:f86d3a7a9af5d826744fabf4afd15b9dfef44fe69a98541f666f66fbb8d3fef9", size = 2291657 },
            { url = "https://files.pythonhosted.org/packages/3b/ad/285c556747d34c399f332ba7c1a595ba245796ef3e22eae190f5364bb62b/pillow-11.1.0-cp313-cp313-win_amd64.whl", hash = "sha256:593c5fd6be85da83656b93ffcccc2312d2d149d251e98588b14fbc288fd8909c", size = 2626304 },
            { url = "https://files.pythonhosted.org/packages/e5/7b/ef35a71163bf36db06e9c8729608f78dedf032fc8313d19bd4be5c2588f3/pillow-11.1.0-cp313-cp313-win_arm64.whl", hash = "sha256:11633d58b6ee5733bde153a8dafd25e505ea3d32e261accd388827ee987baf65", size = 2375117 },
            { url = "https://files.pythonhosted.org/packages/79/30/77f54228401e84d6791354888549b45824ab0ffde659bafa67956303a09f/pillow-11.1.0-cp313-cp313t-macosx_10_13_x86_64.whl", hash = "sha256:70ca5ef3b3b1c4a0812b5c63c57c23b63e53bc38e758b37a951e5bc466449861", size = 3230060 },
            { url = "https://files.pythonhosted.org/packages/ce/b1/56723b74b07dd64c1010fee011951ea9c35a43d8020acd03111f14298225/pillow-11.1.0-cp313-cp313t-macosx_11_0_arm64.whl", hash = "sha256:8000376f139d4d38d6851eb149b321a52bb8893a88dae8ee7d95840431977081", size = 3106192 },
            { url = "https://files.pythonhosted.org/packages/e1/cd/7bf7180e08f80a4dcc6b4c3a0aa9e0b0ae57168562726a05dc8aa8fa66b0/pillow-11.1.0-cp313-cp313t-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:9ee85f0696a17dd28fbcfceb59f9510aa71934b483d1f5601d1030c3c8304f3c", size = 4446805 },
            { url = "https://files.pythonhosted.org/packages/97/42/87c856ea30c8ed97e8efbe672b58c8304dee0573f8c7cab62ae9e31db6ae/pillow-11.1.0-cp313-cp313t-manylinux_2_28_x86_64.whl", hash = "sha256:dd0e081319328928531df7a0e63621caf67652c8464303fd102141b785ef9547", size = 4530623 },
            { url = "https://files.pythonhosted.org/packages/ff/41/026879e90c84a88e33fb00cc6bd915ac2743c67e87a18f80270dfe3c2041/pillow-11.1.0-cp313-cp313t-musllinux_1_2_x86_64.whl", hash = "sha256:e63e4e5081de46517099dc30abe418122f54531a6ae2ebc8680bcd7096860eab", size = 4465191 },
            { url = "https://files.pythonhosted.org/packages/e5/fb/a7960e838bc5df57a2ce23183bfd2290d97c33028b96bde332a9057834d3/pillow-11.1.0-cp313-cp313t-win32.whl", hash = "sha256:dda60aa465b861324e65a78c9f5cf0f4bc713e4309f83bc387be158b077963d9", size = 2295494 },
            { url = "https://files.pythonhosted.org/packages/d7/6c/6ec83ee2f6f0fda8d4cf89045c6be4b0373ebfc363ba8538f8c999f63fcd/pillow-11.1.0-cp313-cp313t-win_amd64.whl", hash = "sha256:ad5db5781c774ab9a9b2c4302bbf0c1014960a0a7be63278d13ae6fdf88126fe", size = 2631595 },
            { url = "https://files.pythonhosted.org/packages/cf/6c/41c21c6c8af92b9fea313aa47c75de49e2f9a467964ee33eb0135d47eb64/pillow-11.1.0-cp313-cp313t-win_arm64.whl", hash = "sha256:67cd427c68926108778a9005f2a04adbd5e67c442ed21d95389fe1d595458756", size = 2377651 },
            { url = "https://files.pythonhosted.org/packages/fa/c5/389961578fb677b8b3244fcd934f720ed25a148b9a5cc81c91bdf59d8588/pillow-11.1.0-pp310-pypy310_pp73-macosx_10_15_x86_64.whl", hash = "sha256:8c730dc3a83e5ac137fbc92dfcfe1511ce3b2b5d7578315b63dbbb76f7f51d90", size = 3198345 },
            { url = "https://files.pythonhosted.org/packages/c4/fa/803c0e50ffee74d4b965229e816af55276eac1d5806712de86f9371858fd/pillow-11.1.0-pp310-pypy310_pp73-macosx_11_0_arm64.whl", hash = "sha256:7d33d2fae0e8b170b6a6c57400e077412240f6f5bb2a342cf1ee512a787942bb", size = 3072938 },
            { url = "https://files.pythonhosted.org/packages/dc/67/2a3a5f8012b5d8c63fe53958ba906c1b1d0482ebed5618057ef4d22f8076/pillow-11.1.0-pp310-pypy310_pp73-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:a8d65b38173085f24bc07f8b6c505cbb7418009fa1a1fcb111b1f4961814a442", size = 3400049 },
            { url = "https://files.pythonhosted.org/packages/e5/a0/514f0d317446c98c478d1872497eb92e7cde67003fed74f696441e647446/pillow-11.1.0-pp310-pypy310_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:015c6e863faa4779251436db398ae75051469f7c903b043a48f078e437656f83", size = 3422431 },
            { url = "https://files.pythonhosted.org/packages/cd/00/20f40a935514037b7d3f87adfc87d2c538430ea625b63b3af8c3f5578e72/pillow-11.1.0-pp310-pypy310_pp73-manylinux_2_28_aarch64.whl", hash = "sha256:d44ff19eea13ae4acdaaab0179fa68c0c6f2f45d66a4d8ec1eda7d6cecbcc15f", size = 3446208 },
            { url = "https://files.pythonhosted.org/packages/28/3c/7de681727963043e093c72e6c3348411b0185eab3263100d4490234ba2f6/pillow-11.1.0-pp310-pypy310_pp73-manylinux_2_28_x86_64.whl", hash = "sha256:d3d8da4a631471dfaf94c10c85f5277b1f8e42ac42bade1ac67da4b4a7359b73", size = 3509746 },
            { url = "https://files.pythonhosted.org/packages/41/67/936f9814bdd74b2dfd4822f1f7725ab5d8ff4103919a1664eb4874c58b2f/pillow-11.1.0-pp310-pypy310_pp73-win_amd64.whl", hash = "sha256:4637b88343166249fe8aa94e7c4a62a180c4b3898283bb5d3d2fd5fe10d8e4e0", size = 2626353 },
        ]

        [[package]]
        name = "plotly"
        version = "6.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "narwhals" },
            { name = "packaging" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9c/80/761c14012d6daf18e12b6d1e4f6b218e999bcceb694d7a9b180154f9e4db/plotly-6.0.0.tar.gz", hash = "sha256:c4aad38b8c3d65e4a5e7dd308b084143b9025c2cc9d5317fc1f1d30958db87d3", size = 8111782 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0e/77/a946f38b57fb88e736c71fbdd737a1aebd27b532bda0779c137f357cf5fc/plotly-6.0.0-py3-none-any.whl", hash = "sha256:f708871c3a9349a68791ff943a5781b1ec04de7769ea69068adcd9202e57653a", size = 14805949 },
        ]

        [[package]]
        name = "prettytable"
        version = "3.14.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "wcwidth" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/02/7b/18bb59d7c3a4ac9ac7d986cfe49dd3c2e5f5ae3e65ca3db8816764e0c1df/prettytable-3.14.0.tar.gz", hash = "sha256:b804b8d51db23959b96b329094debdbbdf10c8c3aa75958c5988cfd7f78501dd", size = 61747 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/5b/a2/fa0679e7a64b564074a8aa680c664ba8e6770166034f035a22ce74e55ae5/prettytable-3.14.0-py3-none-any.whl", hash = "sha256:61d5c68f04a94acc73c7aac64f0f380f5bed4d2959d59edc6e4cbb7a0e7b55c4", size = 31894 },
        ]

        [[package]]
        name = "propcache"
        version = "0.2.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/20/c8/2a13f78d82211490855b2fb303b6721348d0787fdd9a12ac46d99d3acde1/propcache-0.2.1.tar.gz", hash = "sha256:3f77ce728b19cb537714499928fe800c3dda29e8d9428778fc7c186da4c09a64", size = 41735 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a7/a5/0ea64c9426959ef145a938e38c832fc551843481d356713ececa9a8a64e8/propcache-0.2.1-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:6b3f39a85d671436ee3d12c017f8fdea38509e4f25b28eb25877293c98c243f6", size = 79296 },
            { url = "https://files.pythonhosted.org/packages/76/5a/916db1aba735f55e5eca4733eea4d1973845cf77dfe67c2381a2ca3ce52d/propcache-0.2.1-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:39d51fbe4285d5db5d92a929e3e21536ea3dd43732c5b177c7ef03f918dff9f2", size = 45622 },
            { url = "https://files.pythonhosted.org/packages/2d/62/685d3cf268b8401ec12b250b925b21d152b9d193b7bffa5fdc4815c392c2/propcache-0.2.1-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:6445804cf4ec763dc70de65a3b0d9954e868609e83850a47ca4f0cb64bd79fea", size = 45133 },
            { url = "https://files.pythonhosted.org/packages/4d/3d/31c9c29ee7192defc05aa4d01624fd85a41cf98e5922aaed206017329944/propcache-0.2.1-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:f9479aa06a793c5aeba49ce5c5692ffb51fcd9a7016e017d555d5e2b0045d212", size = 204809 },
            { url = "https://files.pythonhosted.org/packages/10/a1/e4050776f4797fc86140ac9a480d5dc069fbfa9d499fe5c5d2fa1ae71f07/propcache-0.2.1-cp310-cp310-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:d9631c5e8b5b3a0fda99cb0d29c18133bca1e18aea9effe55adb3da1adef80d3", size = 219109 },
            { url = "https://files.pythonhosted.org/packages/c9/c0/e7ae0df76343d5e107d81e59acc085cea5fd36a48aa53ef09add7503e888/propcache-0.2.1-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:3156628250f46a0895f1f36e1d4fbe062a1af8718ec3ebeb746f1d23f0c5dc4d", size = 217368 },
            { url = "https://files.pythonhosted.org/packages/fc/e1/e0a2ed6394b5772508868a977d3238f4afb2eebaf9976f0b44a8d347ad63/propcache-0.2.1-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:6b6fb63ae352e13748289f04f37868099e69dba4c2b3e271c46061e82c745634", size = 205124 },
            { url = "https://files.pythonhosted.org/packages/50/c1/e388c232d15ca10f233c778bbdc1034ba53ede14c207a72008de45b2db2e/propcache-0.2.1-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:887d9b0a65404929641a9fabb6452b07fe4572b269d901d622d8a34a4e9043b2", size = 195463 },
            { url = "https://files.pythonhosted.org/packages/0a/fd/71b349b9def426cc73813dbd0f33e266de77305e337c8c12bfb0a2a82bfb/propcache-0.2.1-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:a96dc1fa45bd8c407a0af03b2d5218392729e1822b0c32e62c5bf7eeb5fb3958", size = 198358 },
            { url = "https://files.pythonhosted.org/packages/02/f2/d7c497cd148ebfc5b0ae32808e6c1af5922215fe38c7a06e4e722fe937c8/propcache-0.2.1-cp310-cp310-musllinux_1_2_armv7l.whl", hash = "sha256:a7e65eb5c003a303b94aa2c3852ef130230ec79e349632d030e9571b87c4698c", size = 195560 },
            { url = "https://files.pythonhosted.org/packages/bb/57/f37041bbe5e0dfed80a3f6be2612a3a75b9cfe2652abf2c99bef3455bbad/propcache-0.2.1-cp310-cp310-musllinux_1_2_i686.whl", hash = "sha256:999779addc413181912e984b942fbcc951be1f5b3663cd80b2687758f434c583", size = 196895 },
            { url = "https://files.pythonhosted.org/packages/83/36/ae3cc3e4f310bff2f064e3d2ed5558935cc7778d6f827dce74dcfa125304/propcache-0.2.1-cp310-cp310-musllinux_1_2_ppc64le.whl", hash = "sha256:19a0f89a7bb9d8048d9c4370c9c543c396e894c76be5525f5e1ad287f1750ddf", size = 207124 },
            { url = "https://files.pythonhosted.org/packages/8c/c4/811b9f311f10ce9d31a32ff14ce58500458443627e4df4ae9c264defba7f/propcache-0.2.1-cp310-cp310-musllinux_1_2_s390x.whl", hash = "sha256:1ac2f5fe02fa75f56e1ad473f1175e11f475606ec9bd0be2e78e4734ad575034", size = 210442 },
            { url = "https://files.pythonhosted.org/packages/18/dd/a1670d483a61ecac0d7fc4305d91caaac7a8fc1b200ea3965a01cf03bced/propcache-0.2.1-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:574faa3b79e8ebac7cb1d7930f51184ba1ccf69adfdec53a12f319a06030a68b", size = 203219 },
            { url = "https://files.pythonhosted.org/packages/f9/2d/30ced5afde41b099b2dc0c6573b66b45d16d73090e85655f1a30c5a24e07/propcache-0.2.1-cp310-cp310-win32.whl", hash = "sha256:03ff9d3f665769b2a85e6157ac8b439644f2d7fd17615a82fa55739bc97863f4", size = 40313 },
            { url = "https://files.pythonhosted.org/packages/23/84/bd9b207ac80da237af77aa6e153b08ffa83264b1c7882495984fcbfcf85c/propcache-0.2.1-cp310-cp310-win_amd64.whl", hash = "sha256:2d3af2e79991102678f53e0dbf4c35de99b6b8b58f29a27ca0325816364caaba", size = 44428 },
            { url = "https://files.pythonhosted.org/packages/bc/0f/2913b6791ebefb2b25b4efd4bb2299c985e09786b9f5b19184a88e5778dd/propcache-0.2.1-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:1ffc3cca89bb438fb9c95c13fc874012f7b9466b89328c3c8b1aa93cdcfadd16", size = 79297 },
            { url = "https://files.pythonhosted.org/packages/cf/73/af2053aeccd40b05d6e19058419ac77674daecdd32478088b79375b9ab54/propcache-0.2.1-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:f174bbd484294ed9fdf09437f889f95807e5f229d5d93588d34e92106fbf6717", size = 45611 },
            { url = "https://files.pythonhosted.org/packages/3c/09/8386115ba7775ea3b9537730e8cf718d83bbf95bffe30757ccf37ec4e5da/propcache-0.2.1-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:70693319e0b8fd35dd863e3e29513875eb15c51945bf32519ef52927ca883bc3", size = 45146 },
            { url = "https://files.pythonhosted.org/packages/03/7a/793aa12f0537b2e520bf09f4c6833706b63170a211ad042ca71cbf79d9cb/propcache-0.2.1-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:b480c6a4e1138e1aa137c0079b9b6305ec6dcc1098a8ca5196283e8a49df95a9", size = 232136 },
            { url = "https://files.pythonhosted.org/packages/f1/38/b921b3168d72111769f648314100558c2ea1d52eb3d1ba7ea5c4aa6f9848/propcache-0.2.1-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:d27b84d5880f6d8aa9ae3edb253c59d9f6642ffbb2c889b78b60361eed449787", size = 239706 },
            { url = "https://files.pythonhosted.org/packages/14/29/4636f500c69b5edea7786db3c34eb6166f3384b905665ce312a6e42c720c/propcache-0.2.1-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:857112b22acd417c40fa4595db2fe28ab900c8c5fe4670c7989b1c0230955465", size = 238531 },
            { url = "https://files.pythonhosted.org/packages/85/14/01fe53580a8e1734ebb704a3482b7829a0ef4ea68d356141cf0994d9659b/propcache-0.2.1-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:cf6c4150f8c0e32d241436526f3c3f9cbd34429492abddbada2ffcff506c51af", size = 231063 },
            { url = "https://files.pythonhosted.org/packages/33/5c/1d961299f3c3b8438301ccfbff0143b69afcc30c05fa28673cface692305/propcache-0.2.1-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:66d4cfda1d8ed687daa4bc0274fcfd5267873db9a5bc0418c2da19273040eeb7", size = 220134 },
            { url = "https://files.pythonhosted.org/packages/00/d0/ed735e76db279ba67a7d3b45ba4c654e7b02bc2f8050671ec365d8665e21/propcache-0.2.1-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:c2f992c07c0fca81655066705beae35fc95a2fa7366467366db627d9f2ee097f", size = 220009 },
            { url = "https://files.pythonhosted.org/packages/75/90/ee8fab7304ad6533872fee982cfff5a53b63d095d78140827d93de22e2d4/propcache-0.2.1-cp311-cp311-musllinux_1_2_armv7l.whl", hash = "sha256:4a571d97dbe66ef38e472703067021b1467025ec85707d57e78711c085984e54", size = 212199 },
            { url = "https://files.pythonhosted.org/packages/eb/ec/977ffaf1664f82e90737275873461695d4c9407d52abc2f3c3e24716da13/propcache-0.2.1-cp311-cp311-musllinux_1_2_i686.whl", hash = "sha256:bb6178c241278d5fe853b3de743087be7f5f4c6f7d6d22a3b524d323eecec505", size = 214827 },
            { url = "https://files.pythonhosted.org/packages/57/48/031fb87ab6081764054821a71b71942161619549396224cbb242922525e8/propcache-0.2.1-cp311-cp311-musllinux_1_2_ppc64le.whl", hash = "sha256:ad1af54a62ffe39cf34db1aa6ed1a1873bd548f6401db39d8e7cd060b9211f82", size = 228009 },
            { url = "https://files.pythonhosted.org/packages/1a/06/ef1390f2524850838f2390421b23a8b298f6ce3396a7cc6d39dedd4047b0/propcache-0.2.1-cp311-cp311-musllinux_1_2_s390x.whl", hash = "sha256:e7048abd75fe40712005bcfc06bb44b9dfcd8e101dda2ecf2f5aa46115ad07ca", size = 231638 },
            { url = "https://files.pythonhosted.org/packages/38/2a/101e6386d5a93358395da1d41642b79c1ee0f3b12e31727932b069282b1d/propcache-0.2.1-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:160291c60081f23ee43d44b08a7e5fb76681221a8e10b3139618c5a9a291b84e", size = 222788 },
            { url = "https://files.pythonhosted.org/packages/db/81/786f687951d0979007e05ad9346cd357e50e3d0b0f1a1d6074df334b1bbb/propcache-0.2.1-cp311-cp311-win32.whl", hash = "sha256:819ce3b883b7576ca28da3861c7e1a88afd08cc8c96908e08a3f4dd64a228034", size = 40170 },
            { url = "https://files.pythonhosted.org/packages/cf/59/7cc7037b295d5772eceb426358bb1b86e6cab4616d971bd74275395d100d/propcache-0.2.1-cp311-cp311-win_amd64.whl", hash = "sha256:edc9fc7051e3350643ad929df55c451899bb9ae6d24998a949d2e4c87fb596d3", size = 44404 },
            { url = "https://files.pythonhosted.org/packages/4c/28/1d205fe49be8b1b4df4c50024e62480a442b1a7b818e734308bb0d17e7fb/propcache-0.2.1-cp312-cp312-macosx_10_13_universal2.whl", hash = "sha256:081a430aa8d5e8876c6909b67bd2d937bfd531b0382d3fdedb82612c618bc41a", size = 79588 },
            { url = "https://files.pythonhosted.org/packages/21/ee/fc4d893f8d81cd4971affef2a6cb542b36617cd1d8ce56b406112cb80bf7/propcache-0.2.1-cp312-cp312-macosx_10_13_x86_64.whl", hash = "sha256:d2ccec9ac47cf4e04897619c0e0c1a48c54a71bdf045117d3a26f80d38ab1fb0", size = 45825 },
            { url = "https://files.pythonhosted.org/packages/4a/de/bbe712f94d088da1d237c35d735f675e494a816fd6f54e9db2f61ef4d03f/propcache-0.2.1-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:14d86fe14b7e04fa306e0c43cdbeebe6b2c2156a0c9ce56b815faacc193e320d", size = 45357 },
            { url = "https://files.pythonhosted.org/packages/7f/14/7ae06a6cf2a2f1cb382586d5a99efe66b0b3d0c6f9ac2f759e6f7af9d7cf/propcache-0.2.1-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:049324ee97bb67285b49632132db351b41e77833678432be52bdd0289c0e05e4", size = 241869 },
            { url = "https://files.pythonhosted.org/packages/cc/59/227a78be960b54a41124e639e2c39e8807ac0c751c735a900e21315f8c2b/propcache-0.2.1-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:1cd9a1d071158de1cc1c71a26014dcdfa7dd3d5f4f88c298c7f90ad6f27bb46d", size = 247884 },
            { url = "https://files.pythonhosted.org/packages/84/58/f62b4ffaedf88dc1b17f04d57d8536601e4e030feb26617228ef930c3279/propcache-0.2.1-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:98110aa363f1bb4c073e8dcfaefd3a5cea0f0834c2aab23dda657e4dab2f53b5", size = 248486 },
            { url = "https://files.pythonhosted.org/packages/1c/07/ebe102777a830bca91bbb93e3479cd34c2ca5d0361b83be9dbd93104865e/propcache-0.2.1-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:647894f5ae99c4cf6bb82a1bb3a796f6e06af3caa3d32e26d2350d0e3e3faf24", size = 243649 },
            { url = "https://files.pythonhosted.org/packages/ed/bc/4f7aba7f08f520376c4bb6a20b9a981a581b7f2e385fa0ec9f789bb2d362/propcache-0.2.1-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:bfd3223c15bebe26518d58ccf9a39b93948d3dcb3e57a20480dfdd315356baff", size = 229103 },
            { url = "https://files.pythonhosted.org/packages/fe/d5/04ac9cd4e51a57a96f78795e03c5a0ddb8f23ec098b86f92de028d7f2a6b/propcache-0.2.1-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:d71264a80f3fcf512eb4f18f59423fe82d6e346ee97b90625f283df56aee103f", size = 226607 },
            { url = "https://files.pythonhosted.org/packages/e3/f0/24060d959ea41d7a7cc7fdbf68b31852331aabda914a0c63bdb0e22e96d6/propcache-0.2.1-cp312-cp312-musllinux_1_2_armv7l.whl", hash = "sha256:e73091191e4280403bde6c9a52a6999d69cdfde498f1fdf629105247599b57ec", size = 221153 },
            { url = "https://files.pythonhosted.org/packages/77/a7/3ac76045a077b3e4de4859a0753010765e45749bdf53bd02bc4d372da1a0/propcache-0.2.1-cp312-cp312-musllinux_1_2_i686.whl", hash = "sha256:3935bfa5fede35fb202c4b569bb9c042f337ca4ff7bd540a0aa5e37131659348", size = 222151 },
            { url = "https://files.pythonhosted.org/packages/e7/af/5e29da6f80cebab3f5a4dcd2a3240e7f56f2c4abf51cbfcc99be34e17f0b/propcache-0.2.1-cp312-cp312-musllinux_1_2_ppc64le.whl", hash = "sha256:f508b0491767bb1f2b87fdfacaba5f7eddc2f867740ec69ece6d1946d29029a6", size = 233812 },
            { url = "https://files.pythonhosted.org/packages/8c/89/ebe3ad52642cc5509eaa453e9f4b94b374d81bae3265c59d5c2d98efa1b4/propcache-0.2.1-cp312-cp312-musllinux_1_2_s390x.whl", hash = "sha256:1672137af7c46662a1c2be1e8dc78cb6d224319aaa40271c9257d886be4363a6", size = 238829 },
            { url = "https://files.pythonhosted.org/packages/e9/2f/6b32f273fa02e978b7577159eae7471b3cfb88b48563b1c2578b2d7ca0bb/propcache-0.2.1-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:b74c261802d3d2b85c9df2dfb2fa81b6f90deeef63c2db9f0e029a3cac50b518", size = 230704 },
            { url = "https://files.pythonhosted.org/packages/5c/2e/f40ae6ff5624a5f77edd7b8359b208b5455ea113f68309e2b00a2e1426b6/propcache-0.2.1-cp312-cp312-win32.whl", hash = "sha256:d09c333d36c1409d56a9d29b3a1b800a42c76a57a5a8907eacdbce3f18768246", size = 40050 },
            { url = "https://files.pythonhosted.org/packages/3b/77/a92c3ef994e47180862b9d7d11e37624fb1c00a16d61faf55115d970628b/propcache-0.2.1-cp312-cp312-win_amd64.whl", hash = "sha256:c214999039d4f2a5b2073ac506bba279945233da8c786e490d411dfc30f855c1", size = 44117 },
            { url = "https://files.pythonhosted.org/packages/0f/2a/329e0547cf2def8857157f9477669043e75524cc3e6251cef332b3ff256f/propcache-0.2.1-cp313-cp313-macosx_10_13_universal2.whl", hash = "sha256:aca405706e0b0a44cc6bfd41fbe89919a6a56999157f6de7e182a990c36e37bc", size = 77002 },
            { url = "https://files.pythonhosted.org/packages/12/2d/c4df5415e2382f840dc2ecbca0eeb2293024bc28e57a80392f2012b4708c/propcache-0.2.1-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:12d1083f001ace206fe34b6bdc2cb94be66d57a850866f0b908972f90996b3e9", size = 44639 },
            { url = "https://files.pythonhosted.org/packages/d0/5a/21aaa4ea2f326edaa4e240959ac8b8386ea31dedfdaa636a3544d9e7a408/propcache-0.2.1-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:d93f3307ad32a27bda2e88ec81134b823c240aa3abb55821a8da553eed8d9439", size = 44049 },
            { url = "https://files.pythonhosted.org/packages/4e/3e/021b6cd86c0acc90d74784ccbb66808b0bd36067a1bf3e2deb0f3845f618/propcache-0.2.1-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ba278acf14471d36316159c94a802933d10b6a1e117b8554fe0d0d9b75c9d536", size = 224819 },
            { url = "https://files.pythonhosted.org/packages/3c/57/c2fdeed1b3b8918b1770a133ba5c43ad3d78e18285b0c06364861ef5cc38/propcache-0.2.1-cp313-cp313-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:4e6281aedfca15301c41f74d7005e6e3f4ca143584ba696ac69df4f02f40d629", size = 229625 },
            { url = "https://files.pythonhosted.org/packages/9d/81/70d4ff57bf2877b5780b466471bebf5892f851a7e2ca0ae7ffd728220281/propcache-0.2.1-cp313-cp313-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:5b750a8e5a1262434fb1517ddf64b5de58327f1adc3524a5e44c2ca43305eb0b", size = 232934 },
            { url = "https://files.pythonhosted.org/packages/3c/b9/bb51ea95d73b3fb4100cb95adbd4e1acaf2cbb1fd1083f5468eeb4a099a8/propcache-0.2.1-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:bf72af5e0fb40e9babf594308911436c8efde3cb5e75b6f206c34ad18be5c052", size = 227361 },
            { url = "https://files.pythonhosted.org/packages/f1/20/3c6d696cd6fd70b29445960cc803b1851a1131e7a2e4ee261ee48e002bcd/propcache-0.2.1-cp313-cp313-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:b2d0a12018b04f4cb820781ec0dffb5f7c7c1d2a5cd22bff7fb055a2cb19ebce", size = 213904 },
            { url = "https://files.pythonhosted.org/packages/a1/cb/1593bfc5ac6d40c010fa823f128056d6bc25b667f5393781e37d62f12005/propcache-0.2.1-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:e800776a79a5aabdb17dcc2346a7d66d0777e942e4cd251defeb084762ecd17d", size = 212632 },
            { url = "https://files.pythonhosted.org/packages/6d/5c/e95617e222be14a34c709442a0ec179f3207f8a2b900273720501a70ec5e/propcache-0.2.1-cp313-cp313-musllinux_1_2_armv7l.whl", hash = "sha256:4160d9283bd382fa6c0c2b5e017acc95bc183570cd70968b9202ad6d8fc48dce", size = 207897 },
            { url = "https://files.pythonhosted.org/packages/8e/3b/56c5ab3dc00f6375fbcdeefdede5adf9bee94f1fab04adc8db118f0f9e25/propcache-0.2.1-cp313-cp313-musllinux_1_2_i686.whl", hash = "sha256:30b43e74f1359353341a7adb783c8f1b1c676367b011709f466f42fda2045e95", size = 208118 },
            { url = "https://files.pythonhosted.org/packages/86/25/d7ef738323fbc6ebcbce33eb2a19c5e07a89a3df2fded206065bd5e868a9/propcache-0.2.1-cp313-cp313-musllinux_1_2_ppc64le.whl", hash = "sha256:58791550b27d5488b1bb52bc96328456095d96206a250d28d874fafe11b3dfaf", size = 217851 },
            { url = "https://files.pythonhosted.org/packages/b3/77/763e6cef1852cf1ba740590364ec50309b89d1c818e3256d3929eb92fabf/propcache-0.2.1-cp313-cp313-musllinux_1_2_s390x.whl", hash = "sha256:0f022d381747f0dfe27e99d928e31bc51a18b65bb9e481ae0af1380a6725dd1f", size = 222630 },
            { url = "https://files.pythonhosted.org/packages/4f/e9/0f86be33602089c701696fbed8d8c4c07b6ee9605c5b7536fd27ed540c5b/propcache-0.2.1-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:297878dc9d0a334358f9b608b56d02e72899f3b8499fc6044133f0d319e2ec30", size = 216269 },
            { url = "https://files.pythonhosted.org/packages/cc/02/5ac83217d522394b6a2e81a2e888167e7ca629ef6569a3f09852d6dcb01a/propcache-0.2.1-cp313-cp313-win32.whl", hash = "sha256:ddfab44e4489bd79bda09d84c430677fc7f0a4939a73d2bba3073036f487a0a6", size = 39472 },
            { url = "https://files.pythonhosted.org/packages/f4/33/d6f5420252a36034bc8a3a01171bc55b4bff5df50d1c63d9caa50693662f/propcache-0.2.1-cp313-cp313-win_amd64.whl", hash = "sha256:556fc6c10989f19a179e4321e5d678db8eb2924131e64652a51fe83e4c3db0e1", size = 43363 },
            { url = "https://files.pythonhosted.org/packages/41/b6/c5319caea262f4821995dca2107483b94a3345d4607ad797c76cb9c36bcc/propcache-0.2.1-py3-none-any.whl", hash = "sha256:52277518d6aae65536e9cea52d4e7fd2f7a66f4aa2d30ed3f2fcea620ace3c54", size = 11818 },
        ]

        [[package]]
        name = "psutil"
        version = "6.1.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/1f/5a/07871137bb752428aa4b659f910b399ba6f291156bdea939be3e96cae7cb/psutil-6.1.1.tar.gz", hash = "sha256:cf8496728c18f2d0b45198f06895be52f36611711746b7f30c464b422b50e2f5", size = 508502 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/61/99/ca79d302be46f7bdd8321089762dd4476ee725fce16fc2b2e1dbba8cac17/psutil-6.1.1-cp36-abi3-macosx_10_9_x86_64.whl", hash = "sha256:fc0ed7fe2231a444fc219b9c42d0376e0a9a1a72f16c5cfa0f68d19f1a0663e8", size = 247511 },
            { url = "https://files.pythonhosted.org/packages/0b/6b/73dbde0dd38f3782905d4587049b9be64d76671042fdcaf60e2430c6796d/psutil-6.1.1-cp36-abi3-macosx_11_0_arm64.whl", hash = "sha256:0bdd4eab935276290ad3cb718e9809412895ca6b5b334f5a9111ee6d9aff9377", size = 248985 },
            { url = "https://files.pythonhosted.org/packages/17/38/c319d31a1d3f88c5b79c68b3116c129e5133f1822157dd6da34043e32ed6/psutil-6.1.1-cp36-abi3-manylinux_2_12_i686.manylinux2010_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:b6e06c20c05fe95a3d7302d74e7097756d4ba1247975ad6905441ae1b5b66003", size = 284488 },
            { url = "https://files.pythonhosted.org/packages/9c/39/0f88a830a1c8a3aba27fededc642da37613c57cbff143412e3536f89784f/psutil-6.1.1-cp36-abi3-manylinux_2_12_x86_64.manylinux2010_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:97f7cb9921fbec4904f522d972f0c0e1f4fabbdd4e0287813b21215074a0f160", size = 287477 },
            { url = "https://files.pythonhosted.org/packages/47/da/99f4345d4ddf2845cb5b5bd0d93d554e84542d116934fde07a0c50bd4e9f/psutil-6.1.1-cp36-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:33431e84fee02bc84ea36d9e2c4a6d395d479c9dd9bba2376c1f6ee8f3a4e0b3", size = 289017 },
            { url = "https://files.pythonhosted.org/packages/38/53/bd755c2896f4461fd4f36fa6a6dcb66a88a9e4b9fd4e5b66a77cf9d4a584/psutil-6.1.1-cp37-abi3-win32.whl", hash = "sha256:eaa912e0b11848c4d9279a93d7e2783df352b082f40111e078388701fd479e53", size = 250602 },
            { url = "https://files.pythonhosted.org/packages/7b/d7/7831438e6c3ebbfa6e01a927127a6cb42ad3ab844247f3c5b96bea25d73d/psutil-6.1.1-cp37-abi3-win_amd64.whl", hash = "sha256:f35cfccb065fff93529d2afb4a2e89e363fe63ca1e4a5da22b603a85833c2649", size = 254444 },
        ]

        [[package]]
        name = "pybtex"
        version = "0.24.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "latexcodec" },
            { name = "pyyaml" },
            { name = "six" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/46/9b/fd39836a6397fb363446d83075a7b9c2cc562f4c449292e039ed36084376/pybtex-0.24.0.tar.gz", hash = "sha256:818eae35b61733e5c007c3fcd2cfb75ed1bc8b4173c1f70b56cc4c0802d34755", size = 402879 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ad/5f/40d8e90f985a05133a8895fc454c6127ecec3de8b095dd35bba91382f803/pybtex-0.24.0-py2.py3-none-any.whl", hash = "sha256:e1e0c8c69998452fea90e9179aa2a98ab103f3eed894405b7264e517cc2fcc0f", size = 561354 },
        ]

        [[package]]
        name = "pydantic"
        version = "2.10.6"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "annotated-types" },
            { name = "pydantic-core" },
            { name = "typing-extensions" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b7/ae/d5220c5c52b158b1de7ca89fc5edb72f304a70a4c540c84c8844bf4008de/pydantic-2.10.6.tar.gz", hash = "sha256:ca5daa827cce33de7a42be142548b0096bf05a7e7b365aebfa5f8eeec7128236", size = 761681 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f4/3c/8cc1cc84deffa6e25d2d0c688ebb80635dfdbf1dbea3e30c541c8cf4d860/pydantic-2.10.6-py3-none-any.whl", hash = "sha256:427d664bf0b8a2b34ff5dd0f5a18df00591adcee7198fbd71981054cef37b584", size = 431696 },
        ]

        [[package]]
        name = "pydantic-core"
        version = "2.27.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "typing-extensions" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/fc/01/f3e5ac5e7c25833db5eb555f7b7ab24cd6f8c322d3a3ad2d67a952dc0abc/pydantic_core-2.27.2.tar.gz", hash = "sha256:eb026e5a4c1fee05726072337ff51d1efb6f59090b7da90d30ea58625b1ffb39", size = 413443 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3a/bc/fed5f74b5d802cf9a03e83f60f18864e90e3aed7223adaca5ffb7a8d8d64/pydantic_core-2.27.2-cp310-cp310-macosx_10_12_x86_64.whl", hash = "sha256:2d367ca20b2f14095a8f4fa1210f5a7b78b8a20009ecced6b12818f455b1e9fa", size = 1895938 },
            { url = "https://files.pythonhosted.org/packages/71/2a/185aff24ce844e39abb8dd680f4e959f0006944f4a8a0ea372d9f9ae2e53/pydantic_core-2.27.2-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:491a2b73db93fab69731eaee494f320faa4e093dbed776be1a829c2eb222c34c", size = 1815684 },
            { url = "https://files.pythonhosted.org/packages/c3/43/fafabd3d94d159d4f1ed62e383e264f146a17dd4d48453319fd782e7979e/pydantic_core-2.27.2-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:7969e133a6f183be60e9f6f56bfae753585680f3b7307a8e555a948d443cc05a", size = 1829169 },
            { url = "https://files.pythonhosted.org/packages/a2/d1/f2dfe1a2a637ce6800b799aa086d079998959f6f1215eb4497966efd2274/pydantic_core-2.27.2-cp310-cp310-manylinux_2_17_armv7l.manylinux2014_armv7l.whl", hash = "sha256:3de9961f2a346257caf0aa508a4da705467f53778e9ef6fe744c038119737ef5", size = 1867227 },
            { url = "https://files.pythonhosted.org/packages/7d/39/e06fcbcc1c785daa3160ccf6c1c38fea31f5754b756e34b65f74e99780b5/pydantic_core-2.27.2-cp310-cp310-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:e2bb4d3e5873c37bb3dd58714d4cd0b0e6238cebc4177ac8fe878f8b3aa8e74c", size = 2037695 },
            { url = "https://files.pythonhosted.org/packages/7a/67/61291ee98e07f0650eb756d44998214231f50751ba7e13f4f325d95249ab/pydantic_core-2.27.2-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:280d219beebb0752699480fe8f1dc61ab6615c2046d76b7ab7ee38858de0a4e7", size = 2741662 },
            { url = "https://files.pythonhosted.org/packages/32/90/3b15e31b88ca39e9e626630b4c4a1f5a0dfd09076366f4219429e6786076/pydantic_core-2.27.2-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:47956ae78b6422cbd46f772f1746799cbb862de838fd8d1fbd34a82e05b0983a", size = 1993370 },
            { url = "https://files.pythonhosted.org/packages/ff/83/c06d333ee3a67e2e13e07794995c1535565132940715931c1c43bfc85b11/pydantic_core-2.27.2-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.whl", hash = "sha256:14d4a5c49d2f009d62a2a7140d3064f686d17a5d1a268bc641954ba181880236", size = 1996813 },
            { url = "https://files.pythonhosted.org/packages/7c/f7/89be1c8deb6e22618a74f0ca0d933fdcb8baa254753b26b25ad3acff8f74/pydantic_core-2.27.2-cp310-cp310-musllinux_1_1_aarch64.whl", hash = "sha256:337b443af21d488716f8d0b6164de833e788aa6bd7e3a39c005febc1284f4962", size = 2005287 },
            { url = "https://files.pythonhosted.org/packages/b7/7d/8eb3e23206c00ef7feee17b83a4ffa0a623eb1a9d382e56e4aa46fd15ff2/pydantic_core-2.27.2-cp310-cp310-musllinux_1_1_armv7l.whl", hash = "sha256:03d0f86ea3184a12f41a2d23f7ccb79cdb5a18e06993f8a45baa8dfec746f0e9", size = 2128414 },
            { url = "https://files.pythonhosted.org/packages/4e/99/fe80f3ff8dd71a3ea15763878d464476e6cb0a2db95ff1c5c554133b6b83/pydantic_core-2.27.2-cp310-cp310-musllinux_1_1_x86_64.whl", hash = "sha256:7041c36f5680c6e0f08d922aed302e98b3745d97fe1589db0a3eebf6624523af", size = 2155301 },
            { url = "https://files.pythonhosted.org/packages/2b/a3/e50460b9a5789ca1451b70d4f52546fa9e2b420ba3bfa6100105c0559238/pydantic_core-2.27.2-cp310-cp310-win32.whl", hash = "sha256:50a68f3e3819077be2c98110c1f9dcb3817e93f267ba80a2c05bb4f8799e2ff4", size = 1816685 },
            { url = "https://files.pythonhosted.org/packages/57/4c/a8838731cb0f2c2a39d3535376466de6049034d7b239c0202a64aaa05533/pydantic_core-2.27.2-cp310-cp310-win_amd64.whl", hash = "sha256:e0fd26b16394ead34a424eecf8a31a1f5137094cabe84a1bcb10fa6ba39d3d31", size = 1982876 },
            { url = "https://files.pythonhosted.org/packages/c2/89/f3450af9d09d44eea1f2c369f49e8f181d742f28220f88cc4dfaae91ea6e/pydantic_core-2.27.2-cp311-cp311-macosx_10_12_x86_64.whl", hash = "sha256:8e10c99ef58cfdf2a66fc15d66b16c4a04f62bca39db589ae8cba08bc55331bc", size = 1893421 },
            { url = "https://files.pythonhosted.org/packages/9e/e3/71fe85af2021f3f386da42d291412e5baf6ce7716bd7101ea49c810eda90/pydantic_core-2.27.2-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:26f32e0adf166a84d0cb63be85c562ca8a6fa8de28e5f0d92250c6b7e9e2aff7", size = 1814998 },
            { url = "https://files.pythonhosted.org/packages/a6/3c/724039e0d848fd69dbf5806894e26479577316c6f0f112bacaf67aa889ac/pydantic_core-2.27.2-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:8c19d1ea0673cd13cc2f872f6c9ab42acc4e4f492a7ca9d3795ce2b112dd7e15", size = 1826167 },
            { url = "https://files.pythonhosted.org/packages/2b/5b/1b29e8c1fb5f3199a9a57c1452004ff39f494bbe9bdbe9a81e18172e40d3/pydantic_core-2.27.2-cp311-cp311-manylinux_2_17_armv7l.manylinux2014_armv7l.whl", hash = "sha256:5e68c4446fe0810e959cdff46ab0a41ce2f2c86d227d96dc3847af0ba7def306", size = 1865071 },
            { url = "https://files.pythonhosted.org/packages/89/6c/3985203863d76bb7d7266e36970d7e3b6385148c18a68cc8915fd8c84d57/pydantic_core-2.27.2-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:d9640b0059ff4f14d1f37321b94061c6db164fbe49b334b31643e0528d100d99", size = 2036244 },
            { url = "https://files.pythonhosted.org/packages/0e/41/f15316858a246b5d723f7d7f599f79e37493b2e84bfc789e58d88c209f8a/pydantic_core-2.27.2-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:40d02e7d45c9f8af700f3452f329ead92da4c5f4317ca9b896de7ce7199ea459", size = 2737470 },
            { url = "https://files.pythonhosted.org/packages/a8/7c/b860618c25678bbd6d1d99dbdfdf0510ccb50790099b963ff78a124b754f/pydantic_core-2.27.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:1c1fd185014191700554795c99b347d64f2bb637966c4cfc16998a0ca700d048", size = 1992291 },
            { url = "https://files.pythonhosted.org/packages/bf/73/42c3742a391eccbeab39f15213ecda3104ae8682ba3c0c28069fbcb8c10d/pydantic_core-2.27.2-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.whl", hash = "sha256:d81d2068e1c1228a565af076598f9e7451712700b673de8f502f0334f281387d", size = 1994613 },
            { url = "https://files.pythonhosted.org/packages/94/7a/941e89096d1175d56f59340f3a8ebaf20762fef222c298ea96d36a6328c5/pydantic_core-2.27.2-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:1a4207639fb02ec2dbb76227d7c751a20b1a6b4bc52850568e52260cae64ca3b", size = 2002355 },
            { url = "https://files.pythonhosted.org/packages/6e/95/2359937a73d49e336a5a19848713555605d4d8d6940c3ec6c6c0ca4dcf25/pydantic_core-2.27.2-cp311-cp311-musllinux_1_1_armv7l.whl", hash = "sha256:3de3ce3c9ddc8bbd88f6e0e304dea0e66d843ec9de1b0042b0911c1663ffd474", size = 2126661 },
            { url = "https://files.pythonhosted.org/packages/2b/4c/ca02b7bdb6012a1adef21a50625b14f43ed4d11f1fc237f9d7490aa5078c/pydantic_core-2.27.2-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:30c5f68ded0c36466acede341551106821043e9afaad516adfb6e8fa80a4e6a6", size = 2153261 },
            { url = "https://files.pythonhosted.org/packages/72/9d/a241db83f973049a1092a079272ffe2e3e82e98561ef6214ab53fe53b1c7/pydantic_core-2.27.2-cp311-cp311-win32.whl", hash = "sha256:c70c26d2c99f78b125a3459f8afe1aed4d9687c24fd677c6a4436bc042e50d6c", size = 1812361 },
            { url = "https://files.pythonhosted.org/packages/e8/ef/013f07248041b74abd48a385e2110aa3a9bbfef0fbd97d4e6d07d2f5b89a/pydantic_core-2.27.2-cp311-cp311-win_amd64.whl", hash = "sha256:08e125dbdc505fa69ca7d9c499639ab6407cfa909214d500897d02afb816e7cc", size = 1982484 },
            { url = "https://files.pythonhosted.org/packages/10/1c/16b3a3e3398fd29dca77cea0a1d998d6bde3902fa2706985191e2313cc76/pydantic_core-2.27.2-cp311-cp311-win_arm64.whl", hash = "sha256:26f0d68d4b235a2bae0c3fc585c585b4ecc51382db0e3ba402a22cbc440915e4", size = 1867102 },
            { url = "https://files.pythonhosted.org/packages/d6/74/51c8a5482ca447871c93e142d9d4a92ead74de6c8dc5e66733e22c9bba89/pydantic_core-2.27.2-cp312-cp312-macosx_10_12_x86_64.whl", hash = "sha256:9e0c8cfefa0ef83b4da9588448b6d8d2a2bf1a53c3f1ae5fca39eb3061e2f0b0", size = 1893127 },
            { url = "https://files.pythonhosted.org/packages/d3/f3/c97e80721735868313c58b89d2de85fa80fe8dfeeed84dc51598b92a135e/pydantic_core-2.27.2-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:83097677b8e3bd7eaa6775720ec8e0405f1575015a463285a92bfdfe254529ef", size = 1811340 },
            { url = "https://files.pythonhosted.org/packages/9e/91/840ec1375e686dbae1bd80a9e46c26a1e0083e1186abc610efa3d9a36180/pydantic_core-2.27.2-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:172fce187655fece0c90d90a678424b013f8fbb0ca8b036ac266749c09438cb7", size = 1822900 },
            { url = "https://files.pythonhosted.org/packages/f6/31/4240bc96025035500c18adc149aa6ffdf1a0062a4b525c932065ceb4d868/pydantic_core-2.27.2-cp312-cp312-manylinux_2_17_armv7l.manylinux2014_armv7l.whl", hash = "sha256:519f29f5213271eeeeb3093f662ba2fd512b91c5f188f3bb7b27bc5973816934", size = 1869177 },
            { url = "https://files.pythonhosted.org/packages/fa/20/02fbaadb7808be578317015c462655c317a77a7c8f0ef274bc016a784c54/pydantic_core-2.27.2-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:05e3a55d124407fffba0dd6b0c0cd056d10e983ceb4e5dbd10dda135c31071d6", size = 2038046 },
            { url = "https://files.pythonhosted.org/packages/06/86/7f306b904e6c9eccf0668248b3f272090e49c275bc488a7b88b0823444a4/pydantic_core-2.27.2-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:9c3ed807c7b91de05e63930188f19e921d1fe90de6b4f5cd43ee7fcc3525cb8c", size = 2685386 },
            { url = "https://files.pythonhosted.org/packages/8d/f0/49129b27c43396581a635d8710dae54a791b17dfc50c70164866bbf865e3/pydantic_core-2.27.2-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:6fb4aadc0b9a0c063206846d603b92030eb6f03069151a625667f982887153e2", size = 1997060 },
            { url = "https://files.pythonhosted.org/packages/0d/0f/943b4af7cd416c477fd40b187036c4f89b416a33d3cc0ab7b82708a667aa/pydantic_core-2.27.2-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.whl", hash = "sha256:28ccb213807e037460326424ceb8b5245acb88f32f3d2777427476e1b32c48c4", size = 2004870 },
            { url = "https://files.pythonhosted.org/packages/35/40/aea70b5b1a63911c53a4c8117c0a828d6790483f858041f47bab0b779f44/pydantic_core-2.27.2-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:de3cd1899e2c279b140adde9357c4495ed9d47131b4a4eaff9052f23398076b3", size = 1999822 },
            { url = "https://files.pythonhosted.org/packages/f2/b3/807b94fd337d58effc5498fd1a7a4d9d59af4133e83e32ae39a96fddec9d/pydantic_core-2.27.2-cp312-cp312-musllinux_1_1_armv7l.whl", hash = "sha256:220f892729375e2d736b97d0e51466252ad84c51857d4d15f5e9692f9ef12be4", size = 2130364 },
            { url = "https://files.pythonhosted.org/packages/fc/df/791c827cd4ee6efd59248dca9369fb35e80a9484462c33c6649a8d02b565/pydantic_core-2.27.2-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:a0fcd29cd6b4e74fe8ddd2c90330fd8edf2e30cb52acda47f06dd615ae72da57", size = 2158303 },
            { url = "https://files.pythonhosted.org/packages/9b/67/4e197c300976af185b7cef4c02203e175fb127e414125916bf1128b639a9/pydantic_core-2.27.2-cp312-cp312-win32.whl", hash = "sha256:1e2cb691ed9834cd6a8be61228471d0a503731abfb42f82458ff27be7b2186fc", size = 1834064 },
            { url = "https://files.pythonhosted.org/packages/1f/ea/cd7209a889163b8dcca139fe32b9687dd05249161a3edda62860430457a5/pydantic_core-2.27.2-cp312-cp312-win_amd64.whl", hash = "sha256:cc3f1a99a4f4f9dd1de4fe0312c114e740b5ddead65bb4102884b384c15d8bc9", size = 1989046 },
            { url = "https://files.pythonhosted.org/packages/bc/49/c54baab2f4658c26ac633d798dab66b4c3a9bbf47cff5284e9c182f4137a/pydantic_core-2.27.2-cp312-cp312-win_arm64.whl", hash = "sha256:3911ac9284cd8a1792d3cb26a2da18f3ca26c6908cc434a18f730dc0db7bfa3b", size = 1885092 },
            { url = "https://files.pythonhosted.org/packages/41/b1/9bc383f48f8002f99104e3acff6cba1231b29ef76cfa45d1506a5cad1f84/pydantic_core-2.27.2-cp313-cp313-macosx_10_12_x86_64.whl", hash = "sha256:7d14bd329640e63852364c306f4d23eb744e0f8193148d4044dd3dacdaacbd8b", size = 1892709 },
            { url = "https://files.pythonhosted.org/packages/10/6c/e62b8657b834f3eb2961b49ec8e301eb99946245e70bf42c8817350cbefc/pydantic_core-2.27.2-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:82f91663004eb8ed30ff478d77c4d1179b3563df6cdb15c0817cd1cdaf34d154", size = 1811273 },
            { url = "https://files.pythonhosted.org/packages/ba/15/52cfe49c8c986e081b863b102d6b859d9defc63446b642ccbbb3742bf371/pydantic_core-2.27.2-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:71b24c7d61131bb83df10cc7e687433609963a944ccf45190cfc21e0887b08c9", size = 1823027 },
            { url = "https://files.pythonhosted.org/packages/b1/1c/b6f402cfc18ec0024120602bdbcebc7bdd5b856528c013bd4d13865ca473/pydantic_core-2.27.2-cp313-cp313-manylinux_2_17_armv7l.manylinux2014_armv7l.whl", hash = "sha256:fa8e459d4954f608fa26116118bb67f56b93b209c39b008277ace29937453dc9", size = 1868888 },
            { url = "https://files.pythonhosted.org/packages/bd/7b/8cb75b66ac37bc2975a3b7de99f3c6f355fcc4d89820b61dffa8f1e81677/pydantic_core-2.27.2-cp313-cp313-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:ce8918cbebc8da707ba805b7fd0b382816858728ae7fe19a942080c24e5b7cd1", size = 2037738 },
            { url = "https://files.pythonhosted.org/packages/c8/f1/786d8fe78970a06f61df22cba58e365ce304bf9b9f46cc71c8c424e0c334/pydantic_core-2.27.2-cp313-cp313-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:eda3f5c2a021bbc5d976107bb302e0131351c2ba54343f8a496dc8783d3d3a6a", size = 2685138 },
            { url = "https://files.pythonhosted.org/packages/a6/74/d12b2cd841d8724dc8ffb13fc5cef86566a53ed358103150209ecd5d1999/pydantic_core-2.27.2-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:bd8086fa684c4775c27f03f062cbb9eaa6e17f064307e86b21b9e0abc9c0f02e", size = 1997025 },
            { url = "https://files.pythonhosted.org/packages/a0/6e/940bcd631bc4d9a06c9539b51f070b66e8f370ed0933f392db6ff350d873/pydantic_core-2.27.2-cp313-cp313-manylinux_2_5_i686.manylinux1_i686.whl", hash = "sha256:8d9b3388db186ba0c099a6d20f0604a44eabdeef1777ddd94786cdae158729e4", size = 2004633 },
            { url = "https://files.pythonhosted.org/packages/50/cc/a46b34f1708d82498c227d5d80ce615b2dd502ddcfd8376fc14a36655af1/pydantic_core-2.27.2-cp313-cp313-musllinux_1_1_aarch64.whl", hash = "sha256:7a66efda2387de898c8f38c0cf7f14fca0b51a8ef0b24bfea5849f1b3c95af27", size = 1999404 },
            { url = "https://files.pythonhosted.org/packages/ca/2d/c365cfa930ed23bc58c41463bae347d1005537dc8db79e998af8ba28d35e/pydantic_core-2.27.2-cp313-cp313-musllinux_1_1_armv7l.whl", hash = "sha256:18a101c168e4e092ab40dbc2503bdc0f62010e95d292b27827871dc85450d7ee", size = 2130130 },
            { url = "https://files.pythonhosted.org/packages/f4/d7/eb64d015c350b7cdb371145b54d96c919d4db516817f31cd1c650cae3b21/pydantic_core-2.27.2-cp313-cp313-musllinux_1_1_x86_64.whl", hash = "sha256:ba5dd002f88b78a4215ed2f8ddbdf85e8513382820ba15ad5ad8955ce0ca19a1", size = 2157946 },
            { url = "https://files.pythonhosted.org/packages/a4/99/bddde3ddde76c03b65dfd5a66ab436c4e58ffc42927d4ff1198ffbf96f5f/pydantic_core-2.27.2-cp313-cp313-win32.whl", hash = "sha256:1ebaf1d0481914d004a573394f4be3a7616334be70261007e47c2a6fe7e50130", size = 1834387 },
            { url = "https://files.pythonhosted.org/packages/71/47/82b5e846e01b26ac6f1893d3c5f9f3a2eb6ba79be26eef0b759b4fe72946/pydantic_core-2.27.2-cp313-cp313-win_amd64.whl", hash = "sha256:953101387ecf2f5652883208769a79e48db18c6df442568a0b5ccd8c2723abee", size = 1990453 },
            { url = "https://files.pythonhosted.org/packages/51/b2/b2b50d5ecf21acf870190ae5d093602d95f66c9c31f9d5de6062eb329ad1/pydantic_core-2.27.2-cp313-cp313-win_arm64.whl", hash = "sha256:ac4dbfd1691affb8f48c2c13241a2e3b60ff23247cbcf981759c768b6633cf8b", size = 1885186 },
            { url = "https://files.pythonhosted.org/packages/46/72/af70981a341500419e67d5cb45abe552a7c74b66326ac8877588488da1ac/pydantic_core-2.27.2-pp310-pypy310_pp73-macosx_10_12_x86_64.whl", hash = "sha256:2bf14caea37e91198329b828eae1618c068dfb8ef17bb33287a7ad4b61ac314e", size = 1891159 },
            { url = "https://files.pythonhosted.org/packages/ad/3d/c5913cccdef93e0a6a95c2d057d2c2cba347815c845cda79ddd3c0f5e17d/pydantic_core-2.27.2-pp310-pypy310_pp73-macosx_11_0_arm64.whl", hash = "sha256:b0cb791f5b45307caae8810c2023a184c74605ec3bcbb67d13846c28ff731ff8", size = 1768331 },
            { url = "https://files.pythonhosted.org/packages/f6/f0/a3ae8fbee269e4934f14e2e0e00928f9346c5943174f2811193113e58252/pydantic_core-2.27.2-pp310-pypy310_pp73-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:688d3fd9fcb71f41c4c015c023d12a79d1c4c0732ec9eb35d96e3388a120dcf3", size = 1822467 },
            { url = "https://files.pythonhosted.org/packages/d7/7a/7bbf241a04e9f9ea24cd5874354a83526d639b02674648af3f350554276c/pydantic_core-2.27.2-pp310-pypy310_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:3d591580c34f4d731592f0e9fe40f9cc1b430d297eecc70b962e93c5c668f15f", size = 1979797 },
            { url = "https://files.pythonhosted.org/packages/4f/5f/4784c6107731f89e0005a92ecb8a2efeafdb55eb992b8e9d0a2be5199335/pydantic_core-2.27.2-pp310-pypy310_pp73-manylinux_2_5_i686.manylinux1_i686.whl", hash = "sha256:82f986faf4e644ffc189a7f1aafc86e46ef70372bb153e7001e8afccc6e54133", size = 1987839 },
            { url = "https://files.pythonhosted.org/packages/6d/a7/61246562b651dff00de86a5f01b6e4befb518df314c54dec187a78d81c84/pydantic_core-2.27.2-pp310-pypy310_pp73-musllinux_1_1_aarch64.whl", hash = "sha256:bec317a27290e2537f922639cafd54990551725fc844249e64c523301d0822fc", size = 1998861 },
            { url = "https://files.pythonhosted.org/packages/86/aa/837821ecf0c022bbb74ca132e117c358321e72e7f9702d1b6a03758545e2/pydantic_core-2.27.2-pp310-pypy310_pp73-musllinux_1_1_armv7l.whl", hash = "sha256:0296abcb83a797db256b773f45773da397da75a08f5fcaef41f2044adec05f50", size = 2116582 },
            { url = "https://files.pythonhosted.org/packages/81/b0/5e74656e95623cbaa0a6278d16cf15e10a51f6002e3ec126541e95c29ea3/pydantic_core-2.27.2-pp310-pypy310_pp73-musllinux_1_1_x86_64.whl", hash = "sha256:0d75070718e369e452075a6017fbf187f788e17ed67a3abd47fa934d001863d9", size = 2151985 },
            { url = "https://files.pythonhosted.org/packages/63/37/3e32eeb2a451fddaa3898e2163746b0cffbbdbb4740d38372db0490d67f3/pydantic_core-2.27.2-pp310-pypy310_pp73-win_amd64.whl", hash = "sha256:7e17b560be3c98a8e3aa66ce828bdebb9e9ac6ad5466fba92eb74c4c95cb1151", size = 2004715 },
        ]

        [[package]]
        name = "pymatgen"
        version = "2025.1.24"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "joblib" },
            { name = "matplotlib" },
            { name = "monty" },
            { name = "networkx" },
            { name = "numpy" },
            { name = "palettable" },
            { name = "pandas" },
            { name = "plotly" },
            { name = "pybtex" },
            { name = "requests" },
            { name = "ruamel-yaml" },
            { name = "scipy" },
            { name = "spglib" },
            { name = "sympy", version = "1.13.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet'" },
            { name = "sympy", version = "1.13.3", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "tabulate" },
            { name = "tqdm" },
            { name = "uncertainties" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c1/2c/d84b6bc425fcc77fbfb9c280b467969faa1924324204e79fbf6de4146427/pymatgen-2025.1.24.tar.gz", hash = "sha256:1b3d246ee33f521526d6f6f0da09654da7873d278c57a2fcc495e2f3f40b1ed7", size = 3171878 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/aa/f2/694e0cc5a432d509934542f193c973ae5b747e8d69ca36fb9238363ec2c5/pymatgen-2025.1.24-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:893780cf4dc25b2561c82d9ff510849c228b740d184b861e804fd031e1ccbbd7", size = 3729974 },
            { url = "https://files.pythonhosted.org/packages/98/28/c6573f8feddbbb3e279c4ea8c08c355dd968b31c5b88afcf95be35804a07/pymatgen-2025.1.24-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:469249051cfce2ddcb5e1f63f8cd2298b30955e9a8083c876934d75e6c536f13", size = 3710893 },
            { url = "https://files.pythonhosted.org/packages/93/78/f8e62a354ace0a334c684f8f2c9fc4360256d5ec449e2d63ef979bb5c981/pymatgen-2025.1.24-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:825068c92967987ea2f19dc8db13b2140144dbfba0e31305413adf4b89ee0f26", size = 5018696 },
            { url = "https://files.pythonhosted.org/packages/86/d0/c9b2ab2ec1ffd0c4d7981e78cfe80afed1f8e177f17d4428250b3d78f57c/pymatgen-2025.1.24-cp310-cp310-win32.whl", hash = "sha256:83f9c1e6660ecd0c528996fdddf5a0d19d9a0d18686b5a6f3850048b03d3d313", size = 3665939 },
            { url = "https://files.pythonhosted.org/packages/a8/cb/e0e76229e1cc76e10ce701880d8e4bd59428012838bfaa2568589a746ff8/pymatgen-2025.1.24-cp310-cp310-win_amd64.whl", hash = "sha256:b6fb401e918cad1d413a6b4492a8a4b05610aec02b32d67647f327087145de75", size = 3706563 },
            { url = "https://files.pythonhosted.org/packages/9a/ef/e8fb3b3d0a4f240641a853e0306047a8e11adc845cc23a47e8a46d64edec/pymatgen-2025.1.24-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:85256dcce38c7fe69ffd6a6cf83516a805a1f13a8b46270fabadae3e3d281ec1", size = 3729900 },
            { url = "https://files.pythonhosted.org/packages/6a/ee/68ba8e5ad2f7c8cd16e39c3027453290055c347e3dba208a867dece86777/pymatgen-2025.1.24-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:c3283cf0819f4024aba19934ef71512e150f83079085c998fe67841d8d54d33f", size = 3685293 },
            { url = "https://files.pythonhosted.org/packages/24/00/dfc19642d32f454bbc7b5444485eafbc1d533679485a1b40717faa019641/pymatgen-2025.1.24-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:bd61a482acea459e37e6b317573a685ed205f6e24f8e0c3df3914f0e5413da13", size = 5139966 },
            { url = "https://files.pythonhosted.org/packages/c3/0a/443d8a0fc55661d0db41cc0239c1b7e76eb9dff732a235fa7ace6c6b7a17/pymatgen-2025.1.24-cp311-cp311-win32.whl", hash = "sha256:aa180d66df213fc6cf17cacf33db6fa3cd755decbcc81acd26913bd599b8ca03", size = 3665082 },
            { url = "https://files.pythonhosted.org/packages/5e/33/d83a73304ba451435046eb788f5194edf2431b16d7e3adbbd77ca067dede/pymatgen-2025.1.24-cp311-cp311-win_amd64.whl", hash = "sha256:5932fc8690cb133105272104000dce293d2b161bc05e4a94dd1aaff1d998e62e", size = 3707094 },
            { url = "https://files.pythonhosted.org/packages/86/20/a185868bc0c63aa19fd3f4f333c46f3ba81d9bfb7f2b14e76e856fa380b3/pymatgen-2025.1.24-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:f565f05c71e4c7b0481791592874bd247fca51aaa77fc827223c8e8413fb4bfe", size = 3731724 },
            { url = "https://files.pythonhosted.org/packages/a5/7b/ca3f915bf7abd3346348cfbdbaeaa5e70423ce08eb33f9794e12ffeaf3fc/pymatgen-2025.1.24-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:9a9b5f363771ad0ddcea5e9918de1c8f0c0de50ff5413ef7f257d0ae37e17347", size = 3712144 },
            { url = "https://files.pythonhosted.org/packages/df/0a/bad4d6266cfb3851091aa6a682704d8b16285a928aec379e92f66fb17c73/pymatgen-2025.1.24-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:d378e6013411115f5ca5001ff6694137dc16171271131afd7048ddd31369138a", size = 5128292 },
            { url = "https://files.pythonhosted.org/packages/e1/eb/7472a26a08b5d0d1fdab3f4dfebdf3e282514cb9087fd302ce503ffb7b56/pymatgen-2025.1.24-cp312-cp312-win32.whl", hash = "sha256:8e6ca1d212e1c005f555806ecf5568ffe853f3baf0597719c06ed6a15a1c58d7", size = 3665259 },
            { url = "https://files.pythonhosted.org/packages/ac/07/0cb7eb8cdeccc1b3530e9dfeb0a653d23e725d88eb12916ff4e532b81b4e/pymatgen-2025.1.24-cp312-cp312-win_amd64.whl", hash = "sha256:1b3b07c51cabd4dd0a575ae8f7f5f59c69cbc2042e5eb8d0a2fae564efe373db", size = 3706785 },
        ]

        [[package]]
        name = "pyparsing"
        version = "3.2.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8b/1a/3544f4f299a47911c2ab3710f534e52fea62a633c96806995da5d25be4b2/pyparsing-3.2.1.tar.gz", hash = "sha256:61980854fd66de3a90028d679a954d5f2623e83144b5afe5ee86f43d762e5f0a", size = 1067694 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/1c/a7/c8a2d361bf89c0d9577c934ebb7421b25dc84bf3a8e3ac0a40aed9acc547/pyparsing-3.2.1-py3-none-any.whl", hash = "sha256:506ff4f4386c4cec0590ec19e6302d3aedb992fdc02c761e90416f158dacf8e1", size = 107716 },
        ]

        [[package]]
        name = "python-dateutil"
        version = "2.9.0.post0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "six" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/66/c0/0c8b6ad9f17a802ee498c46e004a0eb49bc148f2fd230864601a86dcf6db/python-dateutil-2.9.0.post0.tar.gz", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 342432 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ec/57/56b9bcc3c9c6a792fcbaf139543cee77261f3651ca9da0c93f5c1221264b/python_dateutil-2.9.0.post0-py2.py3-none-any.whl", hash = "sha256:a8b2bc7bffae282281c8140a97d3aa9c14da0b136dfe83f850eea9a5f7470427", size = 229892 },
        ]

        [[package]]
        name = "python-hostlist"
        version = "2.2.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/2e/dd/c9ef1591ca0d0b4f2197751ecfbdf5c718cf4381bc4d7199eeba2dc90c42/python-hostlist-2.2.1.tar.gz", hash = "sha256:bbf7ca58835f84c6991084a661b409bc9cb3359c9f71be9b752442ba9225a0a5", size = 37216 }

        [[package]]
        name = "pytorch-lightning"
        version = "2.5.0.post0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "fsspec", extra = ["http"] },
            { name = "lightning-utilities" },
            { name = "packaging" },
            { name = "pyyaml" },
            { name = "torch", version = "2.2.1", source = { registry = "https://pypi.org/simple" } },
            { name = "torchmetrics" },
            { name = "tqdm" },
            { name = "typing-extensions" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/6d/b1/3c1d08db3feb1dfcd5be1b9f2406455f3740f7525d7ea1b9244f67b11cb5/pytorch_lightning-2.5.0.post0.tar.gz", hash = "sha256:347235bf8573b4ebcf507a0dd755fcb9ce58c420c77220a9756a6edca0418532", size = 631450 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/02/df/0c7e4582b74264fe2179e78fcdeb9313f680d40ffe1dd4b078da5a2cbf82/pytorch_lightning-2.5.0.post0-py3-none-any.whl", hash = "sha256:c86bf4fded58b386f312f75337696a9b2d57077b858b3b9524400a03a0179b3a", size = 819282 },
        ]

        [[package]]
        name = "pytz"
        version = "2025.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/5f/57/df1c9157c8d5a05117e455d66fd7cf6dbc46974f832b1058ed4856785d8a/pytz-2025.1.tar.gz", hash = "sha256:c2db42be2a2518b28e65f9207c4d05e6ff547d1efa4086469ef855e4ab70178e", size = 319617 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/eb/38/ac33370d784287baa1c3d538978b5e2ea064d4c1b93ffbd12826c190dd10/pytz-2025.1-py2.py3-none-any.whl", hash = "sha256:89dd22dca55b46eac6eda23b2d72721bf1bdfef212645d81513ef5d03038de57", size = 507930 },
        ]

        [[package]]
        name = "pyyaml"
        version = "6.0.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/54/ed/79a089b6be93607fa5cdaedf301d7dfb23af5f25c398d5ead2525b063e17/pyyaml-6.0.2.tar.gz", hash = "sha256:d584d9ec91ad65861cc08d42e834324ef890a082e591037abe114850ff7bbc3e", size = 130631 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9b/95/a3fac87cb7158e231b5a6012e438c647e1a87f09f8e0d123acec8ab8bf71/PyYAML-6.0.2-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:0a9a2848a5b7feac301353437eb7d5957887edbf81d56e903999a75a3d743086", size = 184199 },
            { url = "https://files.pythonhosted.org/packages/c7/7a/68bd47624dab8fd4afbfd3c48e3b79efe09098ae941de5b58abcbadff5cb/PyYAML-6.0.2-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:29717114e51c84ddfba879543fb232a6ed60086602313ca38cce623c1d62cfbf", size = 171758 },
            { url = "https://files.pythonhosted.org/packages/49/ee/14c54df452143b9ee9f0f29074d7ca5516a36edb0b4cc40c3f280131656f/PyYAML-6.0.2-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:8824b5a04a04a047e72eea5cec3bc266db09e35de6bdfe34c9436ac5ee27d237", size = 718463 },
            { url = "https://files.pythonhosted.org/packages/4d/61/de363a97476e766574650d742205be468921a7b532aa2499fcd886b62530/PyYAML-6.0.2-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:7c36280e6fb8385e520936c3cb3b8042851904eba0e58d277dca80a5cfed590b", size = 719280 },
            { url = "https://files.pythonhosted.org/packages/6b/4e/1523cb902fd98355e2e9ea5e5eb237cbc5f3ad5f3075fa65087aa0ecb669/PyYAML-6.0.2-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:ec031d5d2feb36d1d1a24380e4db6d43695f3748343d99434e6f5f9156aaa2ed", size = 751239 },
            { url = "https://files.pythonhosted.org/packages/b7/33/5504b3a9a4464893c32f118a9cc045190a91637b119a9c881da1cf6b7a72/PyYAML-6.0.2-cp310-cp310-musllinux_1_1_aarch64.whl", hash = "sha256:936d68689298c36b53b29f23c6dbb74de12b4ac12ca6cfe0e047bedceea56180", size = 695802 },
            { url = "https://files.pythonhosted.org/packages/5c/20/8347dcabd41ef3a3cdc4f7b7a2aff3d06598c8779faa189cdbf878b626a4/PyYAML-6.0.2-cp310-cp310-musllinux_1_1_x86_64.whl", hash = "sha256:23502f431948090f597378482b4812b0caae32c22213aecf3b55325e049a6c68", size = 720527 },
            { url = "https://files.pythonhosted.org/packages/be/aa/5afe99233fb360d0ff37377145a949ae258aaab831bde4792b32650a4378/PyYAML-6.0.2-cp310-cp310-win32.whl", hash = "sha256:2e99c6826ffa974fe6e27cdb5ed0021786b03fc98e5ee3c5bfe1fd5015f42b99", size = 144052 },
            { url = "https://files.pythonhosted.org/packages/b5/84/0fa4b06f6d6c958d207620fc60005e241ecedceee58931bb20138e1e5776/PyYAML-6.0.2-cp310-cp310-win_amd64.whl", hash = "sha256:a4d3091415f010369ae4ed1fc6b79def9416358877534caf6a0fdd2146c87a3e", size = 161774 },
            { url = "https://files.pythonhosted.org/packages/f8/aa/7af4e81f7acba21a4c6be026da38fd2b872ca46226673c89a758ebdc4fd2/PyYAML-6.0.2-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:cc1c1159b3d456576af7a3e4d1ba7e6924cb39de8f67111c735f6fc832082774", size = 184612 },
            { url = "https://files.pythonhosted.org/packages/8b/62/b9faa998fd185f65c1371643678e4d58254add437edb764a08c5a98fb986/PyYAML-6.0.2-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:1e2120ef853f59c7419231f3bf4e7021f1b936f6ebd222406c3b60212205d2ee", size = 172040 },
            { url = "https://files.pythonhosted.org/packages/ad/0c/c804f5f922a9a6563bab712d8dcc70251e8af811fce4524d57c2c0fd49a4/PyYAML-6.0.2-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:5d225db5a45f21e78dd9358e58a98702a0302f2659a3c6cd320564b75b86f47c", size = 736829 },
            { url = "https://files.pythonhosted.org/packages/51/16/6af8d6a6b210c8e54f1406a6b9481febf9c64a3109c541567e35a49aa2e7/PyYAML-6.0.2-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:5ac9328ec4831237bec75defaf839f7d4564be1e6b25ac710bd1a96321cc8317", size = 764167 },
            { url = "https://files.pythonhosted.org/packages/75/e4/2c27590dfc9992f73aabbeb9241ae20220bd9452df27483b6e56d3975cc5/PyYAML-6.0.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:3ad2a3decf9aaba3d29c8f537ac4b243e36bef957511b4766cb0057d32b0be85", size = 762952 },
            { url = "https://files.pythonhosted.org/packages/9b/97/ecc1abf4a823f5ac61941a9c00fe501b02ac3ab0e373c3857f7d4b83e2b6/PyYAML-6.0.2-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:ff3824dc5261f50c9b0dfb3be22b4567a6f938ccce4587b38952d85fd9e9afe4", size = 735301 },
            { url = "https://files.pythonhosted.org/packages/45/73/0f49dacd6e82c9430e46f4a027baa4ca205e8b0a9dce1397f44edc23559d/PyYAML-6.0.2-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:797b4f722ffa07cc8d62053e4cff1486fa6dc094105d13fea7b1de7d8bf71c9e", size = 756638 },
            { url = "https://files.pythonhosted.org/packages/22/5f/956f0f9fc65223a58fbc14459bf34b4cc48dec52e00535c79b8db361aabd/PyYAML-6.0.2-cp311-cp311-win32.whl", hash = "sha256:11d8f3dd2b9c1207dcaf2ee0bbbfd5991f571186ec9cc78427ba5bd32afae4b5", size = 143850 },
            { url = "https://files.pythonhosted.org/packages/ed/23/8da0bbe2ab9dcdd11f4f4557ccaf95c10b9811b13ecced089d43ce59c3c8/PyYAML-6.0.2-cp311-cp311-win_amd64.whl", hash = "sha256:e10ce637b18caea04431ce14fabcf5c64a1c61ec9c56b071a4b7ca131ca52d44", size = 161980 },
            { url = "https://files.pythonhosted.org/packages/86/0c/c581167fc46d6d6d7ddcfb8c843a4de25bdd27e4466938109ca68492292c/PyYAML-6.0.2-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:c70c95198c015b85feafc136515252a261a84561b7b1d51e3384e0655ddf25ab", size = 183873 },
            { url = "https://files.pythonhosted.org/packages/a8/0c/38374f5bb272c051e2a69281d71cba6fdb983413e6758b84482905e29a5d/PyYAML-6.0.2-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:ce826d6ef20b1bc864f0a68340c8b3287705cae2f8b4b1d932177dcc76721725", size = 173302 },
            { url = "https://files.pythonhosted.org/packages/c3/93/9916574aa8c00aa06bbac729972eb1071d002b8e158bd0e83a3b9a20a1f7/PyYAML-6.0.2-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:1f71ea527786de97d1a0cc0eacd1defc0985dcf6b3f17bb77dcfc8c34bec4dc5", size = 739154 },
            { url = "https://files.pythonhosted.org/packages/95/0f/b8938f1cbd09739c6da569d172531567dbcc9789e0029aa070856f123984/PyYAML-6.0.2-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:9b22676e8097e9e22e36d6b7bda33190d0d400f345f23d4065d48f4ca7ae0425", size = 766223 },
            { url = "https://files.pythonhosted.org/packages/b9/2b/614b4752f2e127db5cc206abc23a8c19678e92b23c3db30fc86ab731d3bd/PyYAML-6.0.2-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:80bab7bfc629882493af4aa31a4cfa43a4c57c83813253626916b8c7ada83476", size = 767542 },
            { url = "https://files.pythonhosted.org/packages/d4/00/dd137d5bcc7efea1836d6264f049359861cf548469d18da90cd8216cf05f/PyYAML-6.0.2-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:0833f8694549e586547b576dcfaba4a6b55b9e96098b36cdc7ebefe667dfed48", size = 731164 },
            { url = "https://files.pythonhosted.org/packages/c9/1f/4f998c900485e5c0ef43838363ba4a9723ac0ad73a9dc42068b12aaba4e4/PyYAML-6.0.2-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:8b9c7197f7cb2738065c481a0461e50ad02f18c78cd75775628afb4d7137fb3b", size = 756611 },
            { url = "https://files.pythonhosted.org/packages/df/d1/f5a275fdb252768b7a11ec63585bc38d0e87c9e05668a139fea92b80634c/PyYAML-6.0.2-cp312-cp312-win32.whl", hash = "sha256:ef6107725bd54b262d6dedcc2af448a266975032bc85ef0172c5f059da6325b4", size = 140591 },
            { url = "https://files.pythonhosted.org/packages/0c/e8/4f648c598b17c3d06e8753d7d13d57542b30d56e6c2dedf9c331ae56312e/PyYAML-6.0.2-cp312-cp312-win_amd64.whl", hash = "sha256:7e7401d0de89a9a855c839bc697c079a4af81cf878373abd7dc625847d25cbd8", size = 156338 },
            { url = "https://files.pythonhosted.org/packages/ef/e3/3af305b830494fa85d95f6d95ef7fa73f2ee1cc8ef5b495c7c3269fb835f/PyYAML-6.0.2-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:efdca5630322a10774e8e98e1af481aad470dd62c3170801852d752aa7a783ba", size = 181309 },
            { url = "https://files.pythonhosted.org/packages/45/9f/3b1c20a0b7a3200524eb0076cc027a970d320bd3a6592873c85c92a08731/PyYAML-6.0.2-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:50187695423ffe49e2deacb8cd10510bc361faac997de9efef88badc3bb9e2d1", size = 171679 },
            { url = "https://files.pythonhosted.org/packages/7c/9a/337322f27005c33bcb656c655fa78325b730324c78620e8328ae28b64d0c/PyYAML-6.0.2-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:0ffe8360bab4910ef1b9e87fb812d8bc0a308b0d0eef8c8f44e0254ab3b07133", size = 733428 },
            { url = "https://files.pythonhosted.org/packages/a3/69/864fbe19e6c18ea3cc196cbe5d392175b4cf3d5d0ac1403ec3f2d237ebb5/PyYAML-6.0.2-cp313-cp313-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:17e311b6c678207928d649faa7cb0d7b4c26a0ba73d41e99c4fff6b6c3276484", size = 763361 },
            { url = "https://files.pythonhosted.org/packages/04/24/b7721e4845c2f162d26f50521b825fb061bc0a5afcf9a386840f23ea19fa/PyYAML-6.0.2-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:70b189594dbe54f75ab3a1acec5f1e3faa7e8cf2f1e08d9b561cb41b845f69d5", size = 759523 },
            { url = "https://files.pythonhosted.org/packages/2b/b2/e3234f59ba06559c6ff63c4e10baea10e5e7df868092bf9ab40e5b9c56b6/PyYAML-6.0.2-cp313-cp313-musllinux_1_1_aarch64.whl", hash = "sha256:41e4e3953a79407c794916fa277a82531dd93aad34e29c2a514c2c0c5fe971cc", size = 726660 },
            { url = "https://files.pythonhosted.org/packages/fe/0f/25911a9f080464c59fab9027482f822b86bf0608957a5fcc6eaac85aa515/PyYAML-6.0.2-cp313-cp313-musllinux_1_1_x86_64.whl", hash = "sha256:68ccc6023a3400877818152ad9a1033e3db8625d899c72eacb5a668902e4d652", size = 751597 },
            { url = "https://files.pythonhosted.org/packages/14/0d/e2c3b43bbce3cf6bd97c840b46088a3031085179e596d4929729d8d68270/PyYAML-6.0.2-cp313-cp313-win32.whl", hash = "sha256:bc2fa7c6b47d6bc618dd7fb02ef6fdedb1090ec036abab80d4681424b84c1183", size = 140527 },
            { url = "https://files.pythonhosted.org/packages/fa/de/02b54f42487e3d3c6efb3f89428677074ca7bf43aae402517bc7cca949f3/PyYAML-6.0.2-cp313-cp313-win_amd64.whl", hash = "sha256:8388ee1976c416731879ac16da0aff3f63b286ffdd57cdeb95f3f2e085687563", size = 156446 },
        ]

        [[package]]
        name = "requests"
        version = "2.32.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "certifi" },
            { name = "charset-normalizer" },
            { name = "idna" },
            { name = "urllib3" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/63/70/2bf7780ad2d390a8d301ad0b550f1581eadbd9a20f896afe06353c2a2913/requests-2.32.3.tar.gz", hash = "sha256:55365417734eb18255590a9ff9eb97e9e1da868d4ccd6402399eaf68af20a760", size = 131218 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/9b/335f9764261e915ed497fcdeb11df5dfd6f7bf257d4a6a2a686d80da4d54/requests-2.32.3-py3-none-any.whl", hash = "sha256:70761cfe03c773ceb22aa2f671b4757976145175cdfca038c02654d061d6dcc6", size = 64928 },
        ]

        [[package]]
        name = "ruamel-yaml"
        version = "0.18.10"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "ruamel-yaml-clib", marker = "python_full_version < '3.13' and platform_python_implementation == 'CPython'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ea/46/f44d8be06b85bc7c4d8c95d658be2b68f27711f279bf9dd0612a5e4794f5/ruamel.yaml-0.18.10.tar.gz", hash = "sha256:20c86ab29ac2153f80a428e1254a8adf686d3383df04490514ca3b79a362db58", size = 143447 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/36/dfc1ebc0081e6d39924a2cc53654497f967a084a436bb64402dfce4254d9/ruamel.yaml-0.18.10-py3-none-any.whl", hash = "sha256:30f22513ab2301b3d2b577adc121c6471f28734d3d9728581245f1e76468b4f1", size = 117729 },
        ]

        [[package]]
        name = "ruamel-yaml-clib"
        version = "0.2.12"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/20/84/80203abff8ea4993a87d823a5f632e4d92831ef75d404c9fc78d0176d2b5/ruamel.yaml.clib-0.2.12.tar.gz", hash = "sha256:6c8fbb13ec503f99a91901ab46e0b07ae7941cd527393187039aec586fdfd36f", size = 225315 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/70/57/40a958e863e299f0c74ef32a3bde9f2d1ea8d69669368c0c502a0997f57f/ruamel.yaml.clib-0.2.12-cp310-cp310-macosx_13_0_arm64.whl", hash = "sha256:11f891336688faf5156a36293a9c362bdc7c88f03a8a027c2c1d8e0bcde998e5", size = 131301 },
            { url = "https://files.pythonhosted.org/packages/98/a8/29a3eb437b12b95f50a6bcc3d7d7214301c6c529d8fdc227247fa84162b5/ruamel.yaml.clib-0.2.12-cp310-cp310-manylinux2014_aarch64.whl", hash = "sha256:a606ef75a60ecf3d924613892cc603b154178ee25abb3055db5062da811fd969", size = 633728 },
            { url = "https://files.pythonhosted.org/packages/35/6d/ae05a87a3ad540259c3ad88d71275cbd1c0f2d30ae04c65dcbfb6dcd4b9f/ruamel.yaml.clib-0.2.12-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:fd5415dded15c3822597455bc02bcd66e81ef8b7a48cb71a33628fc9fdde39df", size = 722230 },
            { url = "https://files.pythonhosted.org/packages/7f/b7/20c6f3c0b656fe609675d69bc135c03aac9e3865912444be6339207b6648/ruamel.yaml.clib-0.2.12-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:f66efbc1caa63c088dead1c4170d148eabc9b80d95fb75b6c92ac0aad2437d76", size = 686712 },
            { url = "https://files.pythonhosted.org/packages/cd/11/d12dbf683471f888d354dac59593873c2b45feb193c5e3e0f2ebf85e68b9/ruamel.yaml.clib-0.2.12-cp310-cp310-musllinux_1_1_i686.whl", hash = "sha256:22353049ba4181685023b25b5b51a574bce33e7f51c759371a7422dcae5402a6", size = 663936 },
            { url = "https://files.pythonhosted.org/packages/72/14/4c268f5077db5c83f743ee1daeb236269fa8577133a5cfa49f8b382baf13/ruamel.yaml.clib-0.2.12-cp310-cp310-musllinux_1_1_x86_64.whl", hash = "sha256:932205970b9f9991b34f55136be327501903f7c66830e9760a8ffb15b07f05cd", size = 696580 },
            { url = "https://files.pythonhosted.org/packages/30/fc/8cd12f189c6405a4c1cf37bd633aa740a9538c8e40497c231072d0fef5cf/ruamel.yaml.clib-0.2.12-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:a52d48f4e7bf9005e8f0a89209bf9a73f7190ddf0489eee5eb51377385f59f2a", size = 663393 },
            { url = "https://files.pythonhosted.org/packages/80/29/c0a017b704aaf3cbf704989785cd9c5d5b8ccec2dae6ac0c53833c84e677/ruamel.yaml.clib-0.2.12-cp310-cp310-win32.whl", hash = "sha256:3eac5a91891ceb88138c113f9db04f3cebdae277f5d44eaa3651a4f573e6a5da", size = 100326 },
            { url = "https://files.pythonhosted.org/packages/3a/65/fa39d74db4e2d0cd252355732d966a460a41cd01c6353b820a0952432839/ruamel.yaml.clib-0.2.12-cp310-cp310-win_amd64.whl", hash = "sha256:ab007f2f5a87bd08ab1499bdf96f3d5c6ad4dcfa364884cb4549aa0154b13a28", size = 118079 },
            { url = "https://files.pythonhosted.org/packages/fb/8f/683c6ad562f558cbc4f7c029abcd9599148c51c54b5ef0f24f2638da9fbb/ruamel.yaml.clib-0.2.12-cp311-cp311-macosx_13_0_arm64.whl", hash = "sha256:4a6679521a58256a90b0d89e03992c15144c5f3858f40d7c18886023d7943db6", size = 132224 },
            { url = "https://files.pythonhosted.org/packages/3c/d2/b79b7d695e2f21da020bd44c782490578f300dd44f0a4c57a92575758a76/ruamel.yaml.clib-0.2.12-cp311-cp311-manylinux2014_aarch64.whl", hash = "sha256:d84318609196d6bd6da0edfa25cedfbabd8dbde5140a0a23af29ad4b8f91fb1e", size = 641480 },
            { url = "https://files.pythonhosted.org/packages/68/6e/264c50ce2a31473a9fdbf4fa66ca9b2b17c7455b31ef585462343818bd6c/ruamel.yaml.clib-0.2.12-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:bb43a269eb827806502c7c8efb7ae7e9e9d0573257a46e8e952f4d4caba4f31e", size = 739068 },
            { url = "https://files.pythonhosted.org/packages/86/29/88c2567bc893c84d88b4c48027367c3562ae69121d568e8a3f3a8d363f4d/ruamel.yaml.clib-0.2.12-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:811ea1594b8a0fb466172c384267a4e5e367298af6b228931f273b111f17ef52", size = 703012 },
            { url = "https://files.pythonhosted.org/packages/11/46/879763c619b5470820f0cd6ca97d134771e502776bc2b844d2adb6e37753/ruamel.yaml.clib-0.2.12-cp311-cp311-musllinux_1_1_i686.whl", hash = "sha256:cf12567a7b565cbf65d438dec6cfbe2917d3c1bdddfce84a9930b7d35ea59642", size = 704352 },
            { url = "https://files.pythonhosted.org/packages/02/80/ece7e6034256a4186bbe50dee28cd032d816974941a6abf6a9d65e4228a7/ruamel.yaml.clib-0.2.12-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:7dd5adc8b930b12c8fc5b99e2d535a09889941aa0d0bd06f4749e9a9397c71d2", size = 737344 },
            { url = "https://files.pythonhosted.org/packages/f0/ca/e4106ac7e80efbabdf4bf91d3d32fc424e41418458251712f5672eada9ce/ruamel.yaml.clib-0.2.12-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:1492a6051dab8d912fc2adeef0e8c72216b24d57bd896ea607cb90bb0c4981d3", size = 714498 },
            { url = "https://files.pythonhosted.org/packages/67/58/b1f60a1d591b771298ffa0428237afb092c7f29ae23bad93420b1eb10703/ruamel.yaml.clib-0.2.12-cp311-cp311-win32.whl", hash = "sha256:bd0a08f0bab19093c54e18a14a10b4322e1eacc5217056f3c063bd2f59853ce4", size = 100205 },
            { url = "https://files.pythonhosted.org/packages/b4/4f/b52f634c9548a9291a70dfce26ca7ebce388235c93588a1068028ea23fcc/ruamel.yaml.clib-0.2.12-cp311-cp311-win_amd64.whl", hash = "sha256:a274fb2cb086c7a3dea4322ec27f4cb5cc4b6298adb583ab0e211a4682f241eb", size = 118185 },
            { url = "https://files.pythonhosted.org/packages/48/41/e7a405afbdc26af961678474a55373e1b323605a4f5e2ddd4a80ea80f628/ruamel.yaml.clib-0.2.12-cp312-cp312-macosx_14_0_arm64.whl", hash = "sha256:20b0f8dc160ba83b6dcc0e256846e1a02d044e13f7ea74a3d1d56ede4e48c632", size = 133433 },
            { url = "https://files.pythonhosted.org/packages/ec/b0/b850385604334c2ce90e3ee1013bd911aedf058a934905863a6ea95e9eb4/ruamel.yaml.clib-0.2.12-cp312-cp312-manylinux2014_aarch64.whl", hash = "sha256:943f32bc9dedb3abff9879edc134901df92cfce2c3d5c9348f172f62eb2d771d", size = 647362 },
            { url = "https://files.pythonhosted.org/packages/44/d0/3f68a86e006448fb6c005aee66565b9eb89014a70c491d70c08de597f8e4/ruamel.yaml.clib-0.2.12-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:95c3829bb364fdb8e0332c9931ecf57d9be3519241323c5274bd82f709cebc0c", size = 754118 },
            { url = "https://files.pythonhosted.org/packages/52/a9/d39f3c5ada0a3bb2870d7db41901125dbe2434fa4f12ca8c5b83a42d7c53/ruamel.yaml.clib-0.2.12-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:749c16fcc4a2b09f28843cda5a193e0283e47454b63ec4b81eaa2242f50e4ccd", size = 706497 },
            { url = "https://files.pythonhosted.org/packages/b0/fa/097e38135dadd9ac25aecf2a54be17ddf6e4c23e43d538492a90ab3d71c6/ruamel.yaml.clib-0.2.12-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bf165fef1f223beae7333275156ab2022cffe255dcc51c27f066b4370da81e31", size = 698042 },
            { url = "https://files.pythonhosted.org/packages/ec/d5/a659ca6f503b9379b930f13bc6b130c9f176469b73b9834296822a83a132/ruamel.yaml.clib-0.2.12-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:32621c177bbf782ca5a18ba4d7af0f1082a3f6e517ac2a18b3974d4edf349680", size = 745831 },
            { url = "https://files.pythonhosted.org/packages/db/5d/36619b61ffa2429eeaefaab4f3374666adf36ad8ac6330d855848d7d36fd/ruamel.yaml.clib-0.2.12-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:b82a7c94a498853aa0b272fd5bc67f29008da798d4f93a2f9f289feb8426a58d", size = 715692 },
            { url = "https://files.pythonhosted.org/packages/b1/82/85cb92f15a4231c89b95dfe08b09eb6adca929ef7df7e17ab59902b6f589/ruamel.yaml.clib-0.2.12-cp312-cp312-win32.whl", hash = "sha256:e8c4ebfcfd57177b572e2040777b8abc537cdef58a2120e830124946aa9b42c5", size = 98777 },
            { url = "https://files.pythonhosted.org/packages/d7/8f/c3654f6f1ddb75daf3922c3d8fc6005b1ab56671ad56ffb874d908bfa668/ruamel.yaml.clib-0.2.12-cp312-cp312-win_amd64.whl", hash = "sha256:0467c5965282c62203273b838ae77c0d29d7638c8a4e3a1c8bdd3602c10904e4", size = 115523 },
            { url = "https://files.pythonhosted.org/packages/29/00/4864119668d71a5fa45678f380b5923ff410701565821925c69780356ffa/ruamel.yaml.clib-0.2.12-cp313-cp313-macosx_14_0_arm64.whl", hash = "sha256:4c8c5d82f50bb53986a5e02d1b3092b03622c02c2eb78e29bec33fd9593bae1a", size = 132011 },
            { url = "https://files.pythonhosted.org/packages/7f/5e/212f473a93ae78c669ffa0cb051e3fee1139cb2d385d2ae1653d64281507/ruamel.yaml.clib-0.2.12-cp313-cp313-manylinux2014_aarch64.whl", hash = "sha256:e7e3736715fbf53e9be2a79eb4db68e4ed857017344d697e8b9749444ae57475", size = 642488 },
            { url = "https://files.pythonhosted.org/packages/1f/8f/ecfbe2123ade605c49ef769788f79c38ddb1c8fa81e01f4dbf5cf1a44b16/ruamel.yaml.clib-0.2.12-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:0b7e75b4965e1d4690e93021adfcecccbca7d61c7bddd8e22406ef2ff20d74ef", size = 745066 },
            { url = "https://files.pythonhosted.org/packages/e2/a9/28f60726d29dfc01b8decdb385de4ced2ced9faeb37a847bd5cf26836815/ruamel.yaml.clib-0.2.12-cp313-cp313-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:96777d473c05ee3e5e3c3e999f5d23c6f4ec5b0c38c098b3a5229085f74236c6", size = 701785 },
            { url = "https://files.pythonhosted.org/packages/84/7e/8e7ec45920daa7f76046578e4f677a3215fe8f18ee30a9cb7627a19d9b4c/ruamel.yaml.clib-0.2.12-cp313-cp313-musllinux_1_1_i686.whl", hash = "sha256:3bc2a80e6420ca8b7d3590791e2dfc709c88ab9152c00eeb511c9875ce5778bf", size = 693017 },
            { url = "https://files.pythonhosted.org/packages/c5/b3/d650eaade4ca225f02a648321e1ab835b9d361c60d51150bac49063b83fa/ruamel.yaml.clib-0.2.12-cp313-cp313-musllinux_1_1_x86_64.whl", hash = "sha256:e188d2699864c11c36cdfdada94d781fd5d6b0071cd9c427bceb08ad3d7c70e1", size = 741270 },
            { url = "https://files.pythonhosted.org/packages/87/b8/01c29b924dcbbed75cc45b30c30d565d763b9c4d540545a0eeecffb8f09c/ruamel.yaml.clib-0.2.12-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:4f6f3eac23941b32afccc23081e1f50612bdbe4e982012ef4f5797986828cd01", size = 709059 },
            { url = "https://files.pythonhosted.org/packages/30/8c/ed73f047a73638257aa9377ad356bea4d96125b305c34a28766f4445cc0f/ruamel.yaml.clib-0.2.12-cp313-cp313-win32.whl", hash = "sha256:6442cb36270b3afb1b4951f060eccca1ce49f3d087ca1ca4563a6eb479cb3de6", size = 98583 },
            { url = "https://files.pythonhosted.org/packages/b0/85/e8e751d8791564dd333d5d9a4eab0a7a115f7e349595417fd50ecae3395c/ruamel.yaml.clib-0.2.12-cp313-cp313-win_amd64.whl", hash = "sha256:e5b8daf27af0b90da7bb903a876477a9e6d7270be6146906b276605997c7e9a3", size = 115190 },
        ]

        [[package]]
        name = "scipy"
        version = "1.15.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "numpy" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/76/c6/8eb0654ba0c7d0bb1bf67bf8fbace101a8e4f250f7722371105e8b6f68fc/scipy-1.15.1.tar.gz", hash = "sha256:033a75ddad1463970c96a88063a1df87ccfddd526437136b6ee81ff0312ebdf6", size = 59407493 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/86/53/b204ce5a4433f1864001b9d16f103b9c25f5002a602ae83585d0ea5f9c4a/scipy-1.15.1-cp310-cp310-macosx_10_13_x86_64.whl", hash = "sha256:c64ded12dcab08afff9e805a67ff4480f5e69993310e093434b10e85dc9d43e1", size = 41414518 },
            { url = "https://files.pythonhosted.org/packages/c7/fc/54ffa7a8847f7f303197a6ba65a66104724beba2e38f328135a78f0dc480/scipy-1.15.1-cp310-cp310-macosx_12_0_arm64.whl", hash = "sha256:5b190b935e7db569960b48840e5bef71dc513314cc4e79a1b7d14664f57fd4ff", size = 32519265 },
            { url = "https://files.pythonhosted.org/packages/f1/77/a98b8ba03d6f371dc31a38719affd53426d4665729dcffbed4afe296784a/scipy-1.15.1-cp310-cp310-macosx_14_0_arm64.whl", hash = "sha256:4b17d4220df99bacb63065c76b0d1126d82bbf00167d1730019d2a30d6ae01ea", size = 24792859 },
            { url = "https://files.pythonhosted.org/packages/a7/78/70bb9f0df7444b18b108580934bfef774822e28fd34a68e5c263c7d2828a/scipy-1.15.1-cp310-cp310-macosx_14_0_x86_64.whl", hash = "sha256:63b9b6cd0333d0eb1a49de6f834e8aeaefe438df8f6372352084535ad095219e", size = 27886506 },
            { url = "https://files.pythonhosted.org/packages/14/a7/f40f6033e06de4176ddd6cc8c3ae9f10a226c3bca5d6b4ab883bc9914a14/scipy-1.15.1-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:9f151e9fb60fbf8e52426132f473221a49362091ce7a5e72f8aa41f8e0da4f25", size = 38375041 },
            { url = "https://files.pythonhosted.org/packages/17/03/390a1c5c61fd76b0fa4b3c5aa3bdd7e60f6c46f712924f1a9df5705ec046/scipy-1.15.1-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:21e10b1dd56ce92fba3e786007322542361984f8463c6d37f6f25935a5a6ef52", size = 40597556 },
            { url = "https://files.pythonhosted.org/packages/4e/70/fa95b3ae026b97eeca58204a90868802e5155ac71b9d7bdee92b68115dd3/scipy-1.15.1-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:5dff14e75cdbcf07cdaa1c7707db6017d130f0af9ac41f6ce443a93318d6c6e0", size = 42938505 },
            { url = "https://files.pythonhosted.org/packages/d6/07/427859116bdd71847c898180f01802691f203c3e2455a1eb496130ff07c5/scipy-1.15.1-cp310-cp310-win_amd64.whl", hash = "sha256:f82fcf4e5b377f819542fbc8541f7b5fbcf1c0017d0df0bc22c781bf60abc4d8", size = 43909663 },
            { url = "https://files.pythonhosted.org/packages/8e/2e/7b71312da9c2dabff53e7c9a9d08231bc34d9d8fdabe88a6f1155b44591c/scipy-1.15.1-cp311-cp311-macosx_10_13_x86_64.whl", hash = "sha256:5bd8d27d44e2c13d0c1124e6a556454f52cd3f704742985f6b09e75e163d20d2", size = 41424362 },
            { url = "https://files.pythonhosted.org/packages/81/8c/ab85f1aa1cc200c796532a385b6ebf6a81089747adc1da7482a062acc46c/scipy-1.15.1-cp311-cp311-macosx_12_0_arm64.whl", hash = "sha256:be3deeb32844c27599347faa077b359584ba96664c5c79d71a354b80a0ad0ce0", size = 32535910 },
            { url = "https://files.pythonhosted.org/packages/3b/9c/6f4b787058daa8d8da21ddff881b4320e28de4704a65ec147adb50cb2230/scipy-1.15.1-cp311-cp311-macosx_14_0_arm64.whl", hash = "sha256:5eb0ca35d4b08e95da99a9f9c400dc9f6c21c424298a0ba876fdc69c7afacedf", size = 24809398 },
            { url = "https://files.pythonhosted.org/packages/16/2b/949460a796df75fc7a1ee1becea202cf072edbe325ebe29f6d2029947aa7/scipy-1.15.1-cp311-cp311-macosx_14_0_x86_64.whl", hash = "sha256:74bb864ff7640dea310a1377d8567dc2cb7599c26a79ca852fc184cc851954ac", size = 27918045 },
            { url = "https://files.pythonhosted.org/packages/5f/36/67fe249dd7ccfcd2a38b25a640e3af7e59d9169c802478b6035ba91dfd6d/scipy-1.15.1-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:667f950bf8b7c3a23b4199db24cb9bf7512e27e86d0e3813f015b74ec2c6e3df", size = 38332074 },
            { url = "https://files.pythonhosted.org/packages/fc/da/452e1119e6f720df3feb588cce3c42c5e3d628d4bfd4aec097bd30b7de0c/scipy-1.15.1-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:395be70220d1189756068b3173853029a013d8c8dd5fd3d1361d505b2aa58fa7", size = 40588469 },
            { url = "https://files.pythonhosted.org/packages/7f/71/5f94aceeac99a4941478af94fe9f459c6752d497035b6b0761a700f5f9ff/scipy-1.15.1-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:ce3a000cd28b4430426db2ca44d96636f701ed12e2b3ca1f2b1dd7abdd84b39a", size = 42965214 },
            { url = "https://files.pythonhosted.org/packages/af/25/caa430865749d504271757cafd24066d596217e83326155993980bc22f97/scipy-1.15.1-cp311-cp311-win_amd64.whl", hash = "sha256:3fe1d95944f9cf6ba77aa28b82dd6bb2a5b52f2026beb39ecf05304b8392864b", size = 43896034 },
            { url = "https://files.pythonhosted.org/packages/d8/6e/a9c42d0d39e09ed7fd203d0ac17adfea759cba61ab457671fe66e523dbec/scipy-1.15.1-cp312-cp312-macosx_10_13_x86_64.whl", hash = "sha256:c09aa9d90f3500ea4c9b393ee96f96b0ccb27f2f350d09a47f533293c78ea776", size = 41478318 },
            { url = "https://files.pythonhosted.org/packages/04/ee/e3e535c81828618878a7433992fecc92fa4df79393f31a8fea1d05615091/scipy-1.15.1-cp312-cp312-macosx_12_0_arm64.whl", hash = "sha256:0ac102ce99934b162914b1e4a6b94ca7da0f4058b6d6fd65b0cef330c0f3346f", size = 32596696 },
            { url = "https://files.pythonhosted.org/packages/c4/5e/b1b0124be8e76f87115f16b8915003eec4b7060298117715baf13f51942c/scipy-1.15.1-cp312-cp312-macosx_14_0_arm64.whl", hash = "sha256:09c52320c42d7f5c7748b69e9f0389266fd4f82cf34c38485c14ee976cb8cb04", size = 24870366 },
            { url = "https://files.pythonhosted.org/packages/14/36/c00cb73eefda85946172c27913ab995c6ad4eee00fa4f007572e8c50cd51/scipy-1.15.1-cp312-cp312-macosx_14_0_x86_64.whl", hash = "sha256:cdde8414154054763b42b74fe8ce89d7f3d17a7ac5dd77204f0e142cdc9239e9", size = 28007461 },
            { url = "https://files.pythonhosted.org/packages/68/94/aff5c51b3799349a9d1e67a056772a0f8a47db371e83b498d43467806557/scipy-1.15.1-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:4c9d8fc81d6a3b6844235e6fd175ee1d4c060163905a2becce8e74cb0d7554ce", size = 38068174 },
            { url = "https://files.pythonhosted.org/packages/b0/3c/0de11ca154e24a57b579fb648151d901326d3102115bc4f9a7a86526ce54/scipy-1.15.1-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:0fb57b30f0017d4afa5fe5f5b150b8f807618819287c21cbe51130de7ccdaed2", size = 40249869 },
            { url = "https://files.pythonhosted.org/packages/15/09/472e8d0a6b33199d1bb95e49bedcabc0976c3724edd9b0ef7602ccacf41e/scipy-1.15.1-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:491d57fe89927fa1aafbe260f4cfa5ffa20ab9f1435025045a5315006a91b8f5", size = 42629068 },
            { url = "https://files.pythonhosted.org/packages/ff/ba/31c7a8131152822b3a2cdeba76398ffb404d81d640de98287d236da90c49/scipy-1.15.1-cp312-cp312-win_amd64.whl", hash = "sha256:900f3fa3db87257510f011c292a5779eb627043dd89731b9c461cd16ef76ab3d", size = 43621992 },
            { url = "https://files.pythonhosted.org/packages/2b/bf/dd68965a4c5138a630eeed0baec9ae96e5d598887835bdde96cdd2fe4780/scipy-1.15.1-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:100193bb72fbff37dbd0bf14322314fc7cbe08b7ff3137f11a34d06dc0ee6b85", size = 41441136 },
            { url = "https://files.pythonhosted.org/packages/ef/5e/4928581312922d7e4d416d74c416a660addec4dd5ea185401df2269ba5a0/scipy-1.15.1-cp313-cp313-macosx_12_0_arm64.whl", hash = "sha256:2114a08daec64980e4b4cbdf5bee90935af66d750146b1d2feb0d3ac30613692", size = 32533699 },
            { url = "https://files.pythonhosted.org/packages/32/90/03f99c43041852837686898c66767787cd41c5843d7a1509c39ffef683e9/scipy-1.15.1-cp313-cp313-macosx_14_0_arm64.whl", hash = "sha256:6b3e71893c6687fc5e29208d518900c24ea372a862854c9888368c0b267387ab", size = 24807289 },
            { url = "https://files.pythonhosted.org/packages/9d/52/bfe82b42ae112eaba1af2f3e556275b8727d55ac6e4932e7aef337a9d9d4/scipy-1.15.1-cp313-cp313-macosx_14_0_x86_64.whl", hash = "sha256:837299eec3d19b7e042923448d17d95a86e43941104d33f00da7e31a0f715d3c", size = 27929844 },
            { url = "https://files.pythonhosted.org/packages/f6/77/54ff610bad600462c313326acdb035783accc6a3d5f566d22757ad297564/scipy-1.15.1-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:82add84e8a9fb12af5c2c1a3a3f1cb51849d27a580cb9e6bd66226195142be6e", size = 38031272 },
            { url = "https://files.pythonhosted.org/packages/f1/26/98585cbf04c7cf503d7eb0a1966df8a268154b5d923c5fe0c1ed13154c49/scipy-1.15.1-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:070d10654f0cb6abd295bc96c12656f948e623ec5f9a4eab0ddb1466c000716e", size = 40210217 },
            { url = "https://files.pythonhosted.org/packages/fd/3f/3d2285eb6fece8bc5dbb2f9f94d61157d61d155e854fd5fea825b8218f12/scipy-1.15.1-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:55cc79ce4085c702ac31e49b1e69b27ef41111f22beafb9b49fea67142b696c4", size = 42587785 },
            { url = "https://files.pythonhosted.org/packages/48/7d/5b5251984bf0160d6533695a74a5fddb1fa36edd6f26ffa8c871fbd4782a/scipy-1.15.1-cp313-cp313-win_amd64.whl", hash = "sha256:c352c1b6d7cac452534517e022f8f7b8d139cd9f27e6fbd9f3cbd0bfd39f5bef", size = 43640439 },
            { url = "https://files.pythonhosted.org/packages/e7/b8/0e092f592d280496de52e152582030f8a270b194f87f890e1a97c5599b81/scipy-1.15.1-cp313-cp313t-macosx_10_13_x86_64.whl", hash = "sha256:0458839c9f873062db69a03de9a9765ae2e694352c76a16be44f93ea45c28d2b", size = 41619862 },
            { url = "https://files.pythonhosted.org/packages/f6/19/0b6e1173aba4db9e0b7aa27fe45019857fb90d6904038b83927cbe0a6c1d/scipy-1.15.1-cp313-cp313t-macosx_12_0_arm64.whl", hash = "sha256:af0b61c1de46d0565b4b39c6417373304c1d4f5220004058bdad3061c9fa8a95", size = 32610387 },
            { url = "https://files.pythonhosted.org/packages/e7/02/754aae3bd1fa0f2479ade3cfdf1732ecd6b05853f63eee6066a32684563a/scipy-1.15.1-cp313-cp313t-macosx_14_0_arm64.whl", hash = "sha256:71ba9a76c2390eca6e359be81a3e879614af3a71dfdabb96d1d7ab33da6f2364", size = 24883814 },
            { url = "https://files.pythonhosted.org/packages/1f/ac/d7906201604a2ea3b143bb0de51b3966f66441ba50b7dc182c4505b3edf9/scipy-1.15.1-cp313-cp313t-macosx_14_0_x86_64.whl", hash = "sha256:14eaa373c89eaf553be73c3affb11ec6c37493b7eaaf31cf9ac5dffae700c2e0", size = 27944865 },
            { url = "https://files.pythonhosted.org/packages/84/9d/8f539002b5e203723af6a6f513a45e0a7671e9dabeedb08f417ac17e4edc/scipy-1.15.1-cp313-cp313t-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f735bc41bd1c792c96bc426dece66c8723283695f02df61dcc4d0a707a42fc54", size = 39883261 },
            { url = "https://files.pythonhosted.org/packages/97/c0/62fd3bab828bcccc9b864c5997645a3b86372a35941cdaf677565c25c98d/scipy-1.15.1-cp313-cp313t-musllinux_1_2_x86_64.whl", hash = "sha256:2722a021a7929d21168830790202a75dbb20b468a8133c74a2c0230c72626b6c", size = 42093299 },
            { url = "https://files.pythonhosted.org/packages/e4/1f/5d46a8d94e9f6d2c913cbb109e57e7eed914de38ea99e2c4d69a9fc93140/scipy-1.15.1-cp313-cp313t-win_amd64.whl", hash = "sha256:bc7136626261ac1ed988dca56cfc4ab5180f75e0ee52e58f1e6aa74b5f3eacd5", size = 43181730 },
        ]

        [[package]]
        name = "setuptools"
        version = "75.8.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/92/ec/089608b791d210aec4e7f97488e67ab0d33add3efccb83a056cbafe3a2a6/setuptools-75.8.0.tar.gz", hash = "sha256:c5afc8f407c626b8313a86e10311dd3f661c6cd9c09d4bf8c15c0e11f9f2b0e6", size = 1343222 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/69/8a/b9dc7678803429e4a3bc9ba462fa3dd9066824d3c607490235c6a796be5a/setuptools-75.8.0-py3-none-any.whl", hash = "sha256:e3982f444617239225d675215d51f6ba05f845d4eec313da4418fdbb56fb27e3", size = 1228782 },
        ]

        [[package]]
        name = "six"
        version = "1.17.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/94/e7/b2c673351809dca68a0e064b6af791aa332cf192da575fd474ed7d6f16a2/six-1.17.0.tar.gz", hash = "sha256:ff70335d468e7eb6ec65b95b99d3a2836546063f63acc5171de367e834932a81", size = 34031 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b7/ce/149a00dd41f10bc29e5921b496af8b574d8413afcd5e30dfa0ed46c2cc5e/six-1.17.0-py2.py3-none-any.whl", hash = "sha256:4721f391ed90541fddacab5acf947aa0d3dc7d27b2e1e8eda2be8970586c3274", size = 11050 },
        ]

        [[package]]
        name = "smmap"
        version = "5.0.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/44/cd/a040c4b3119bbe532e5b0732286f805445375489fceaec1f48306068ee3b/smmap-5.0.2.tar.gz", hash = "sha256:26ea65a03958fa0c8a1c7e8c7a58fdc77221b8910f6be2131affade476898ad5", size = 22329 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/04/be/d09147ad1ec7934636ad912901c5fd7667e1c858e19d355237db0d0cd5e4/smmap-5.0.2-py3-none-any.whl", hash = "sha256:b30115f0def7d7531d22a0fb6502488d879e75b260a9db4d0819cfb25403af5e", size = 24303 },
        ]

        [[package]]
        name = "spglib"
        version = "2.5.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "numpy" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/10/8e/2c53fa9027543c3624cdd8b7ac9f6d611464dfc86eb786465e422a8c024c/spglib-2.5.0.tar.gz", hash = "sha256:f8bb638897be91b9dbd4c085d9fde1f69048f5949e20f3832cb9438e57418d4b", size = 2822241 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/32/ab/f9436120f1282b5bc6af7d63c5b1c3296f86acdc09927069631b092c051b/spglib-2.5.0-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:993f4b2d2f0a133d22fcf2d6f3fad9beeee3527c5453d5619e6518d7af253c85", size = 1064068 },
            { url = "https://files.pythonhosted.org/packages/99/9f/e6465a4e0856e35576d0cef7e44e10bdf8aa010dea0969c1ad3c50112c7c/spglib-2.5.0-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:b0b070ddbaad0e97ec18efef602af57caf0045c7187180b1eaea843cb5877c8b", size = 1052798 },
            { url = "https://files.pythonhosted.org/packages/67/2f/330e67f98932ac1433472ba224e5f5319efdacf3fde149f09ccae4ee82f8/spglib-2.5.0-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:12c3eb5d302dd74d07d35ad53180f7f91c69b58473e48b510aae90567f2bb90f", size = 1069986 },
            { url = "https://files.pythonhosted.org/packages/c7/70/68c4d51809a558784b822d7d6e2469696d3efd6039f76e6a7b1b4f7a3422/spglib-2.5.0-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a8cf1f4082d8685686d7b3cf8d6ade4182ba09f13cc49eb2481201c0aa4579c9", size = 1075722 },
            { url = "https://files.pythonhosted.org/packages/d6/43/941c82f6d0665cd6f2af29a4547b24e5e06367bec2e2b73ab903c62674b7/spglib-2.5.0-cp310-cp310-win_amd64.whl", hash = "sha256:b16ffc3497736263a53b629768ec8aae707d595bbd702deac3a6dd39e045537b", size = 300736 },
            { url = "https://files.pythonhosted.org/packages/c9/06/c52cc06a44b265d49bf8e006c45de82de1aa6fbad113e9b84bb6e7f4e8b6/spglib-2.5.0-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:6d9d812eea06e09a1f725755f21efa7451c48636cd24abb64ffa2cd82b8003dd", size = 1064065 },
            { url = "https://files.pythonhosted.org/packages/ca/41/0ea6f102558c70e8d86a7127a8cc6b1a5cd765931b2baba8b82d66a4df93/spglib-2.5.0-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:c36c549442d6e422c932f528dff13af5eb766a5d1985d1d14d65777b1d69f136", size = 1052797 },
            { url = "https://files.pythonhosted.org/packages/22/9b/66891820345f06de8bd0723f9ea075ed62be40eda8d7a0a56b93d9dbf978/spglib-2.5.0-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:67c67a3457dcafd8c533b2c3bebd2f25dcdbef3d7b6ec122f1e983218e0cdeee", size = 1069984 },
            { url = "https://files.pythonhosted.org/packages/b6/61/f301b774e5cdbc9b618d9946eaa269d8e563ccad06a34c0bd33f276be8db/spglib-2.5.0-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:6b2f6085d5f7d8fff63055fdea2955e9667f07934263f23f8db12d667228c2a2", size = 1075722 },
            { url = "https://files.pythonhosted.org/packages/b6/27/4412242308119edef9f525afdc17af91560e64945af299676538576b4848/spglib-2.5.0-cp311-cp311-win_amd64.whl", hash = "sha256:d5bc51265dc3a4ca77026f668a1884427d1c7f5f32669948336105a71bde745b", size = 300719 },
            { url = "https://files.pythonhosted.org/packages/cd/62/b0147f156841fc361657393b644c096e4daa4ecd92b0c28d7026e102e977/spglib-2.5.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2fe0014743694bed8fad0c21df0083086dc8aa9a6f61106fc1831edb9e67d260", size = 1064057 },
            { url = "https://files.pythonhosted.org/packages/a6/f6/6eb33d67df543eca3038adb26f59b6309ee42b1cbf9365acc1b5ca587fad/spglib-2.5.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:80fc33a21df2097d5f2182550719f761b3c3230e60bfa5f91dcf5683dc44d6b0", size = 1052792 },
            { url = "https://files.pythonhosted.org/packages/63/d4/411dc94bba3e1ccff68c23b3a6b858aa75991f4d8af4e9a81fa6fe2719b8/spglib-2.5.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:4cb174c6e0d3e2f824a5b4c0f46c2460ce9052c5ef88909058fe982885e47939", size = 1069956 },
            { url = "https://files.pythonhosted.org/packages/14/c3/e90b94178e01e3af7b54c245216bcc3910f01dc2ec5ca7579c43ad3acdf9/spglib-2.5.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:b7b0b73ad5a352cd1e0544631b3ab98980a366e2b1bd7c804ab8c3f6ea45a6d8", size = 1075752 },
            { url = "https://files.pythonhosted.org/packages/95/5a/a4854a97c7e5e693bd6fd31212a5b3e66386847c89f984947f2113253fd3/spglib-2.5.0-cp312-cp312-win_amd64.whl", hash = "sha256:2f3bd027c40b74cc5297de3898d4badee32f8e90d24ac0449cd4266ee5111ba3", size = 300721 },
        ]

        [[package]]
        name = "sympy"
        version = "1.13.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform == 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform == 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform == 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        dependencies = [
            { name = "mpmath", marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ca/99/5a5b6f19ff9f083671ddf7b9632028436167cd3d33e11015754e41b249a4/sympy-1.13.1.tar.gz", hash = "sha256:9cebf7e04ff162015ce31c9c6c9144daa34a93bd082f54fd8f12deca4f47515f", size = 7533040 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b2/fe/81695a1aa331a842b582453b605175f419fe8540355886031328089d840a/sympy-1.13.1-py3-none-any.whl", hash = "sha256:db36cdc64bf61b9b24578b6f7bab1ecdd2452cf008f34faa33776680c26d66f8", size = 6189177 },
        ]

        [[package]]
        name = "sympy"
        version = "1.13.3"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform == 'win32'",
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform == 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform == 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        dependencies = [
            { name = "mpmath", marker = "extra == 'extra-4-test-m3gnet'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/11/8a/5a7fd6284fa8caac23a26c9ddf9c30485a48169344b4bd3b0f02fef1890f/sympy-1.13.3.tar.gz", hash = "sha256:b27fd2c6530e0ab39e275fc9b683895367e51d5da91baa8d3d64db2565fec4d9", size = 7533196 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/99/ff/c87e0622b1dadea79d2fb0b25ade9ed98954c9033722eb707053d310d4f3/sympy-1.13.3-py3-none-any.whl", hash = "sha256:54612cf55a62755ee71824ce692986f23c88ffa77207b30c1368eda4a7060f73", size = 6189483 },
        ]

        [[package]]
        name = "tabulate"
        version = "0.9.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ec/fe/802052aecb21e3797b8f7902564ab6ea0d60ff8ca23952079064155d1ae1/tabulate-0.9.0.tar.gz", hash = "sha256:0095b12bf5966de529c0feb1fa08671671b3368eec77d7ef7ab114be2c068b3c", size = 81090 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/40/44/4a5f08c96eb108af5cb50b41f76142f0afa346dfa99d5296fe7202a11854/tabulate-0.9.0-py3-none-any.whl", hash = "sha256:024ca478df22e9340661486f85298cff5f6dcdba14f3813e8830015b9ed1948f", size = 35252 },
        ]

        [[package]]
        name = "test"
        version = "0.0.1"
        source = { virtual = "." }
        dependencies = [
            { name = "mace-torch" },
        ]

        [package.optional-dependencies]
        chgnet = [
            { name = "chgnet" },
        ]
        m3gnet = [
            { name = "matgl" },
            { name = "torch", version = "2.2.1", source = { registry = "https://pypi.org/simple" } },
            { name = "torchdata" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "chgnet", marker = "extra == 'chgnet'", specifier = "==0.4.0" },
            { name = "mace-torch", specifier = "==0.3.9" },
            { name = "matgl", marker = "extra == 'm3gnet'", specifier = "==1.1.3" },
            { name = "torch", marker = "extra == 'm3gnet'", specifier = "==2.2.1" },
            { name = "torchdata", marker = "extra == 'm3gnet'", specifier = "==0.7.1" },
        ]
        provides-extras = ["chgnet", "m3gnet"]

        [[package]]
        name = "torch"
        version = "2.2.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform == 'win32'",
            "python_full_version >= '3.12' and sys_platform != 'win32'",
            "python_full_version == '3.11.*' and sys_platform == 'win32'",
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform == 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        dependencies = [
            { name = "filelock", marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "fsspec", marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "jinja2", marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "networkx", marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "nvidia-cublas-cu12", version = "12.1.3.1", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cuda-cupti-cu12", version = "12.1.105", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cuda-nvrtc-cu12", version = "12.1.105", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cuda-runtime-cu12", version = "12.1.105", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cudnn-cu12", version = "8.9.2.26", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cufft-cu12", version = "11.0.2.54", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-curand-cu12", version = "10.3.2.106", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cusolver-cu12", version = "11.4.5.107", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cusparse-cu12", version = "12.1.0.106", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-nccl-cu12", version = "2.19.3", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-nvtx-cu12", version = "12.1.105", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "sympy", version = "1.13.3", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "triton", version = "2.2.0", source = { registry = "https://pypi.org/simple" }, marker = "(python_full_version < '3.12' and platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-m3gnet') or (python_full_version >= '3.12' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "typing-extensions", marker = "extra == 'extra-4-test-m3gnet'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a7/ad/fbe7d4cffb76da4e478438853b51305361c719cff929ab70a808e7fb75e7/torch-2.2.1-cp310-cp310-manylinux1_x86_64.whl", hash = "sha256:8d3bad336dd2c93c6bcb3268e8e9876185bda50ebde325ef211fb565c7d15273", size = 755527312 },
            { url = "https://files.pythonhosted.org/packages/ed/ab/37003a4a7ff095827b5f21f82a77d2d980ebdfb2cf4b05a47e708453b1f4/torch-2.2.1-cp310-cp310-manylinux2014_aarch64.whl", hash = "sha256:5297f13370fdaca05959134b26a06a7f232ae254bf2e11a50eddec62525c9006", size = 86611545 },
            { url = "https://files.pythonhosted.org/packages/7d/df/2c3f3a838b4fa5334ab79a5e0e4efacfb9a1a2fa1ab9e4d343be655fcb64/torch-2.2.1-cp310-cp310-win_amd64.whl", hash = "sha256:5f5dee8433798888ca1415055f5e3faf28a3bad660e4c29e1014acd3275ab11a", size = 198569259 },
            { url = "https://files.pythonhosted.org/packages/6c/46/d4f23bc1ad24e3addb060aebd1ac6203b38e4c48806ac92a6eb992dbe216/torch-2.2.1-cp310-none-macosx_10_9_x86_64.whl", hash = "sha256:b6d78338acabf1fb2e88bf4559d837d30230cf9c3e4337261f4d83200df1fcbe", size = 150792988 },
            { url = "https://files.pythonhosted.org/packages/fe/95/f1af6858fcb923defd2fb0ddb5f4635d8c0dcfc34c2443eee969c2068166/torch-2.2.1-cp310-none-macosx_11_0_arm64.whl", hash = "sha256:6ab3ea2e29d1aac962e905142bbe50943758f55292f1b4fdfb6f4792aae3323e", size = 59707276 },
            { url = "https://files.pythonhosted.org/packages/2c/df/5810707da6f2fd4be57f0cc417987c0fa16a2eecf0b1b71f82ea555dc619/torch-2.2.1-cp311-cp311-manylinux1_x86_64.whl", hash = "sha256:d86664ec85902967d902e78272e97d1aff1d331f7619d398d3ffab1c9b8e9157", size = 755552143 },
            { url = "https://files.pythonhosted.org/packages/cd/fd/2121f53629c433589273a2e8f71d29705e98024e0abe2360e63b852e80bb/torch-2.2.1-cp311-cp311-manylinux2014_aarch64.whl", hash = "sha256:d6227060f268894f92c61af0a44c0d8212e19cb98d05c20141c73312d923bc0a", size = 86619053 },
            { url = "https://files.pythonhosted.org/packages/59/1f/4975d1ab3ed2244053876321ef65bc02935daed67da76c6e7d65900772a3/torch-2.2.1-cp311-cp311-win_amd64.whl", hash = "sha256:77e990af75fb1675490deb374d36e726f84732cd5677d16f19124934b2409ce9", size = 198572606 },
            { url = "https://files.pythonhosted.org/packages/15/23/5d44decce68370cfeef76943bca3f2fb5b0057b7ab39f9d3b1feb53d64b8/torch-2.2.1-cp311-none-macosx_10_9_x86_64.whl", hash = "sha256:46085e328d9b738c261f470231e987930f4cc9472d9ffb7087c7a1343826ac51", size = 150800683 },
            { url = "https://files.pythonhosted.org/packages/3e/b9/256ab23c859cbcd7d6fb7cb46417a07eac817881a0a68df8ea0c18f45221/torch-2.2.1-cp311-none-macosx_11_0_arm64.whl", hash = "sha256:2d9e7e5ecbb002257cf98fae13003abbd620196c35f85c9e34c2adfb961321ec", size = 59714930 },
            { url = "https://files.pythonhosted.org/packages/e4/7e/2f5e867229b4f96c21d1de759ecd2589a1468342ed570ef275bcd02f677f/torch-2.2.1-cp312-cp312-manylinux1_x86_64.whl", hash = "sha256:ada53aebede1c89570e56861b08d12ba4518a1f8b82d467c32665ec4d1f4b3c8", size = 755471734 },
            { url = "https://files.pythonhosted.org/packages/44/94/e049a0570bbac32faafa2374ade5397ab67579e3a8e71ce6948f4f39c9e4/torch-2.2.1-cp312-cp312-manylinux2014_aarch64.whl", hash = "sha256:be21d4c41ecebed9e99430dac87de1439a8c7882faf23bba7fea3fea7b906ac1", size = 86524159 },
            { url = "https://files.pythonhosted.org/packages/d0/ed/9014bef608b318664c2e8b77888449eb605023cdc6c33fad6c47f1a79596/torch-2.2.1-cp312-cp312-win_amd64.whl", hash = "sha256:79848f46196750367dcdf1d2132b722180b9d889571e14d579ae82d2f50596c5", size = 198519367 },
            { url = "https://files.pythonhosted.org/packages/9e/4b/19527706ab1d1f91cdafe0160407a61e5865e74592ea911f92a81c583125/torch-2.2.1-cp312-none-macosx_10_9_x86_64.whl", hash = "sha256:7ee804847be6be0032fbd2d1e6742fea2814c92bebccb177f0d3b8e92b2d2b18", size = 150801561 },
            { url = "https://files.pythonhosted.org/packages/55/42/4442a5f7ea264168382ac0e4b8e114c0a49252dc3a88a034dfaf836db198/torch-2.2.1-cp312-none-macosx_11_0_arm64.whl", hash = "sha256:84b2fb322ab091039fdfe74e17442ff046b258eb5e513a28093152c5b07325a7", size = 59678393 },
        ]

        [[package]]
        name = "torch"
        version = "2.5.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform == 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform == 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform == 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        dependencies = [
            { name = "filelock", marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
            { name = "fsspec", marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
            { name = "jinja2", marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
            { name = "networkx", marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
            { name = "nvidia-cublas-cu12", version = "12.4.5.8", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-chgnet') or (platform_machine == 'x86_64' and sys_platform == 'linux' and extra != 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cuda-cupti-cu12", version = "12.4.127", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-chgnet') or (platform_machine == 'x86_64' and sys_platform == 'linux' and extra != 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cuda-nvrtc-cu12", version = "12.4.127", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-chgnet') or (platform_machine == 'x86_64' and sys_platform == 'linux' and extra != 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cuda-runtime-cu12", version = "12.4.127", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-chgnet') or (platform_machine == 'x86_64' and sys_platform == 'linux' and extra != 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cudnn-cu12", version = "9.1.0.70", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-chgnet') or (platform_machine == 'x86_64' and sys_platform == 'linux' and extra != 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cufft-cu12", version = "11.2.1.3", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-chgnet') or (platform_machine == 'x86_64' and sys_platform == 'linux' and extra != 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-curand-cu12", version = "10.3.5.147", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-chgnet') or (platform_machine == 'x86_64' and sys_platform == 'linux' and extra != 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cusolver-cu12", version = "11.6.1.9", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-chgnet') or (platform_machine == 'x86_64' and sys_platform == 'linux' and extra != 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-cusparse-cu12", version = "12.3.1.170", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-chgnet') or (platform_machine == 'x86_64' and sys_platform == 'linux' and extra != 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-nccl-cu12", version = "2.21.5", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-chgnet') or (platform_machine == 'x86_64' and sys_platform == 'linux' and extra != 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-nvjitlink-cu12", version = "12.4.127", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-chgnet') or (platform_machine == 'x86_64' and sys_platform == 'linux' and extra != 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "nvidia-nvtx-cu12", version = "12.4.127", source = { registry = "https://pypi.org/simple" }, marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-chgnet') or (platform_machine == 'x86_64' and sys_platform == 'linux' and extra != 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "setuptools", marker = "(python_full_version >= '3.12' and extra == 'extra-4-test-chgnet') or (python_full_version >= '3.12' and extra != 'extra-4-test-m3gnet') or (extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "sympy", version = "1.13.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
            { name = "triton", version = "3.1.0", source = { registry = "https://pypi.org/simple" }, marker = "(python_full_version < '3.13' and platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-4-test-chgnet') or (python_full_version < '3.13' and platform_machine == 'x86_64' and sys_platform == 'linux' and extra != 'extra-4-test-m3gnet') or (python_full_version >= '3.13' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (platform_machine != 'x86_64' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet') or (sys_platform != 'linux' and extra == 'extra-4-test-chgnet' and extra == 'extra-4-test-m3gnet')" },
            { name = "typing-extensions", marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/ef/834af4a885b31a0b32fff2d80e1e40f771e1566ea8ded55347502440786a/torch-2.5.1-cp310-cp310-manylinux1_x86_64.whl", hash = "sha256:71328e1bbe39d213b8721678f9dcac30dfc452a46d586f1d514a6aa0a99d4744", size = 906446312 },
            { url = "https://files.pythonhosted.org/packages/69/f0/46e74e0d145f43fa506cb336eaefb2d240547e4ce1f496e442711093ab25/torch-2.5.1-cp310-cp310-manylinux2014_aarch64.whl", hash = "sha256:34bfa1a852e5714cbfa17f27c49d8ce35e1b7af5608c4bc6e81392c352dbc601", size = 91919522 },
            { url = "https://files.pythonhosted.org/packages/a5/13/1eb674c8efbd04d71e4a157ceba991904f633e009a584dd65dccbafbb648/torch-2.5.1-cp310-cp310-win_amd64.whl", hash = "sha256:32a037bd98a241df6c93e4c789b683335da76a2ac142c0973675b715102dc5fa", size = 203088048 },
            { url = "https://files.pythonhosted.org/packages/a9/9d/e0860474ee0ff8f6ef2c50ec8f71a250f38d78a9b9df9fd241ad3397a65b/torch-2.5.1-cp310-none-macosx_11_0_arm64.whl", hash = "sha256:23d062bf70776a3d04dbe74db950db2a5245e1ba4f27208a87f0d743b0d06e86", size = 63877046 },
            { url = "https://files.pythonhosted.org/packages/d1/35/e8b2daf02ce933e4518e6f5682c72fd0ed66c15910ea1fb4168f442b71c4/torch-2.5.1-cp311-cp311-manylinux1_x86_64.whl", hash = "sha256:de5b7d6740c4b636ef4db92be922f0edc425b65ed78c5076c43c42d362a45457", size = 906474467 },
            { url = "https://files.pythonhosted.org/packages/40/04/bd91593a4ca178ece93ca55f27e2783aa524aaccbfda66831d59a054c31e/torch-2.5.1-cp311-cp311-manylinux2014_aarch64.whl", hash = "sha256:340ce0432cad0d37f5a31be666896e16788f1adf8ad7be481196b503dad675b9", size = 91919450 },
            { url = "https://files.pythonhosted.org/packages/0d/4a/e51420d46cfc90562e85af2fee912237c662ab31140ab179e49bd69401d6/torch-2.5.1-cp311-cp311-win_amd64.whl", hash = "sha256:603c52d2fe06433c18b747d25f5c333f9c1d58615620578c326d66f258686f9a", size = 203098237 },
            { url = "https://files.pythonhosted.org/packages/d0/db/5d9cbfbc7968d79c5c09a0bc0bc3735da079f2fd07cc10498a62b320a480/torch-2.5.1-cp311-none-macosx_11_0_arm64.whl", hash = "sha256:31f8c39660962f9ae4eeec995e3049b5492eb7360dd4f07377658ef4d728fa4c", size = 63884466 },
            { url = "https://files.pythonhosted.org/packages/8b/5c/36c114d120bfe10f9323ed35061bc5878cc74f3f594003854b0ea298942f/torch-2.5.1-cp312-cp312-manylinux1_x86_64.whl", hash = "sha256:ed231a4b3a5952177fafb661213d690a72caaad97d5824dd4fc17ab9e15cec03", size = 906389343 },
            { url = "https://files.pythonhosted.org/packages/6d/69/d8ada8b6e0a4257556d5b4ddeb4345ea8eeaaef3c98b60d1cca197c7ad8e/torch-2.5.1-cp312-cp312-manylinux2014_aarch64.whl", hash = "sha256:3f4b7f10a247e0dcd7ea97dc2d3bfbfc90302ed36d7f3952b0008d0df264e697", size = 91811673 },
            { url = "https://files.pythonhosted.org/packages/5f/ba/607d013b55b9fd805db2a5c2662ec7551f1910b4eef39653eeaba182c5b2/torch-2.5.1-cp312-cp312-win_amd64.whl", hash = "sha256:73e58e78f7d220917c5dbfad1a40e09df9929d3b95d25e57d9f8558f84c9a11c", size = 203046841 },
            { url = "https://files.pythonhosted.org/packages/57/6c/bf52ff061da33deb9f94f4121fde7ff3058812cb7d2036c97bc167793bd1/torch-2.5.1-cp312-none-macosx_11_0_arm64.whl", hash = "sha256:8c712df61101964eb11910a846514011f0b6f5920c55dbf567bff8a34163d5b1", size = 63858109 },
            { url = "https://files.pythonhosted.org/packages/69/72/20cb30f3b39a9face296491a86adb6ff8f1a47a897e4d14667e6cf89d5c3/torch-2.5.1-cp313-cp313-manylinux1_x86_64.whl", hash = "sha256:9b61edf3b4f6e3b0e0adda8b3960266b9009d02b37555971f4d1c8f7a05afed7", size = 906393265 },
        ]

        [[package]]
        name = "torch-ema"
        version = "0.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "torch", version = "2.2.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "torch", version = "2.5.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/45/af/db7d0c8b26a13062d9b85bdcf8d977acd8a51057fb6edca9eb30613ef5ef/torch_ema-0.3.tar.gz", hash = "sha256:5a3595405fa311995f01291a1d4a9242d6be08a0fc9db29152ec6cfd586ea414", size = 5486 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c0/f1/a47c831436595cffbfd69a19ea129a23627120b13f4886499e58775329d1/torch_ema-0.3-py3-none-any.whl", hash = "sha256:823ad8da5c10bc1eebcec739cc3f521aa9573229fe04e5673c304d29f1433279", size = 5475 },
        ]

        [[package]]
        name = "torchdata"
        version = "0.7.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "requests" },
            { name = "torch", version = "2.2.1", source = { registry = "https://pypi.org/simple" } },
            { name = "urllib3" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d0/97/6f8f2384d9efb2d1bb6966b5300852d704f4943656360e9352e3cc3358b8/torchdata-0.7.1-cp310-cp310-macosx_10_13_x86_64.whl", hash = "sha256:042db39edbc961c50a36c45b89aea4b099858140c13746c7cc7a87b1cc219d0c", size = 1801802 },
            { url = "https://files.pythonhosted.org/packages/0f/45/7c4674a8a4ac83f0e130d0991d61ff96a231656112bb7c50618d77ab0a8f/torchdata-0.7.1-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:2d2c8482313dd52652caff99dc530433d898a12bb80bc33a0a4d1680d63272e0", size = 4815667 },
            { url = "https://files.pythonhosted.org/packages/39/18/6f0d33df4b9fe4d44a779c2c7cc7cb042535a336f051bb0e5b5387844ee6/torchdata-0.7.1-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:eed10b1f9265b30161a8cd129a96cfc7da202cfc70acf8b6a21fd29e18274ca3", size = 4657548 },
            { url = "https://files.pythonhosted.org/packages/0e/06/0c916f27ef9f5a566b555f07c82c94fb9277fcabe0fcbf4dfe4505dcb28a/torchdata-0.7.1-cp310-cp310-win_amd64.whl", hash = "sha256:36c591d0910ede6a496d4fccd389f27e7bccabf8b6a8722712ecf28b98bda8ae", size = 1330841 },
            { url = "https://files.pythonhosted.org/packages/ad/9a/8b3c64a141b58228419110858acdd5eae7a1b54db9dd8f22a2af956ac53d/torchdata-0.7.1-cp311-cp311-macosx_10_13_x86_64.whl", hash = "sha256:91a78c677a3e4e2447d888587f7ea0b4ddde81ca95adf709ba0d3dc9a4e9542d", size = 1801812 },
            { url = "https://files.pythonhosted.org/packages/35/b2/7ed3a80ae0673b940f2af14281dc02dee0f667c6094e6dcd399fa35249a7/torchdata-0.7.1-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:fa325d628aa6125c6b46b6fa27c94150ca9276edbba1042d3eb3cd9c1039b5a9", size = 4815618 },
            { url = "https://files.pythonhosted.org/packages/c1/8d/b17138a9ad7e47dd602587dbcc142bd98374e0c16c0806c2026d8db54242/torchdata-0.7.1-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:d256535648dfb94d1226f233768c6798d1841edfdbf0a09b2115e6cbbda614f9", size = 4657579 },
            { url = "https://files.pythonhosted.org/packages/da/8d/e0413f91944f931cb5c685cbd6330ad450f9d5466c466822d25761ca772d/torchdata-0.7.1-cp311-cp311-win_amd64.whl", hash = "sha256:7460a5fa298e7cd5cef98e8e6455d481e5c73d39a462a89a38918389c8153e20", size = 1330404 },
            { url = "https://files.pythonhosted.org/packages/e2/c8/34eda2bd6beb8a11c06cf905db74092bdbc3dec51a48f4f22cc474866a0a/torchdata-0.7.1-py3-none-any.whl", hash = "sha256:9f9476a26987d90fa3f87cb09ec82b78ce6031ddcaa91851c9fa9f732a987ab8", size = 184418 },
        ]

        [[package]]
        name = "torchmetrics"
        version = "1.6.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "lightning-utilities" },
            { name = "numpy" },
            { name = "packaging" },
            { name = "torch", version = "2.2.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-m3gnet'" },
            { name = "torch", version = "2.5.1", source = { registry = "https://pypi.org/simple" }, marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/14/c5/8d916585d4d6eb158105c21b28cd4b0ed296d74e499bf8f104368de16619/torchmetrics-1.6.1.tar.gz", hash = "sha256:a5dc236694b392180949fdd0a0fcf2b57135c8b600e557c725e077eb41e53e64", size = 540022 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9d/e1/84066ff60a20dfa63f4d9d8ddc280d5ed323b7f06504dbb51c523b690116/torchmetrics-1.6.1-py3-none-any.whl", hash = "sha256:c3090aa2341129e994c0a659abb6d4140ae75169a6ebf45bffc16c5cb553b38e", size = 927305 },
        ]

        [[package]]
        name = "tqdm"
        version = "4.67.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "colorama", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/a8/4b/29b4ef32e036bb34e4ab51796dd745cdba7ed47ad142a9f4a1eb8e0c744d/tqdm-4.67.1.tar.gz", hash = "sha256:f8aef9c52c08c13a65f30ea34f4e5aac3fd1a34959879d7e59e63027286627f2", size = 169737 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl", hash = "sha256:26445eca388f82e72884e0d580d5464cd801a3ea01e63e5601bdff9ba6a48de2", size = 78540 },
        ]

        [[package]]
        name = "triton"
        version = "2.2.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version == '3.11.*' and sys_platform != 'win32'",
            "python_full_version < '3.11' and sys_platform != 'win32'",
        ]
        dependencies = [
            { name = "filelock", marker = "extra == 'extra-4-test-m3gnet'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/95/05/ed974ce87fe8c8843855daa2136b3409ee1c126707ab54a8b72815c08b49/triton-2.2.0-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a2294514340cfe4e8f4f9e5c66c702744c4a117d25e618bd08469d0bfed1e2e5", size = 167900779 },
            { url = "https://files.pythonhosted.org/packages/bd/ac/3974caaa459bf2c3a244a84be8d17561f631f7d42af370fc311defeca2fb/triton-2.2.0-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:da58a152bddb62cafa9a857dd2bc1f886dbf9f9c90a2b5da82157cd2b34392b0", size = 167928356 },
            { url = "https://files.pythonhosted.org/packages/0e/49/2e1bbae4542b8f624e409540b4197e37ab22a88e8685e99debe721cc2b50/triton-2.2.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:0af58716e721460a61886668b205963dc4d1e4ac20508cc3f623aef0d70283d5", size = 167933985 },
        ]

        [[package]]
        name = "triton"
        version = "3.1.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and sys_platform != 'win32' and extra == 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version >= '3.12' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version == '3.11.*' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
            "python_full_version < '3.11' and extra != 'extra-4-test-chgnet' and extra != 'extra-4-test-m3gnet'",
        ]
        dependencies = [
            { name = "filelock", marker = "extra == 'extra-4-test-chgnet' or extra != 'extra-4-test-m3gnet'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/98/29/69aa56dc0b2eb2602b553881e34243475ea2afd9699be042316842788ff5/triton-3.1.0-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:6b0dd10a925263abbe9fa37dcde67a5e9b2383fc269fdf59f5657cac38c5d1d8", size = 209460013 },
            { url = "https://files.pythonhosted.org/packages/86/17/d9a5cf4fcf46291856d1e90762e36cbabd2a56c7265da0d1d9508c8e3943/triton-3.1.0-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:0f34f6e7885d1bf0eaaf7ba875a5f0ce6f3c13ba98f9503651c1e6dc6757ed5c", size = 209506424 },
            { url = "https://files.pythonhosted.org/packages/78/eb/65f5ba83c2a123f6498a3097746607e5b2f16add29e36765305e4ac7fdd8/triton-3.1.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c8182f42fd8080a7d39d666814fa36c5e30cc00ea7eeeb1a2983dbb4c99a0fdc", size = 209551444 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.12.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/df/db/f35a00659bc03fec321ba8bce9420de607a1d37f8342eee1863174c69557/typing_extensions-4.12.2.tar.gz", hash = "sha256:1a7ead55c7e559dd4dee8856e3a88b41225abfe1ce8df57b7c13915fe121ffb8", size = 85321 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl", hash = "sha256:04e5ca0351e0f3f85c6853954072df659d0d13fac324d0072316b67d7794700d", size = 37438 },
        ]

        [[package]]
        name = "tzdata"
        version = "2025.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/43/0f/fa4723f22942480be4ca9527bbde8d43f6c3f2fe8412f00e7f5f6746bc8b/tzdata-2025.1.tar.gz", hash = "sha256:24894909e88cdb28bd1636c6887801df64cb485bd593f2fd83ef29075a81d694", size = 194950 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0f/dd/84f10e23edd882c6f968c21c2434fe67bd4a528967067515feca9e611e5e/tzdata-2025.1-py2.py3-none-any.whl", hash = "sha256:7e127113816800496f027041c570f50bcd464a020098a3b6b199517772303639", size = 346762 },
        ]

        [[package]]
        name = "uncertainties"
        version = "3.2.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a0/b0/f926a3faf468b9784bdecb8d9328b531743937ead284b2e8d406d96e8b0f/uncertainties-3.2.2.tar.gz", hash = "sha256:e62c86fdc64429828229de6ab4e11466f114907e6bd343c077858994cc12e00b", size = 143865 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fa/fc/97711d2a502881d871e3cf2d2645e21e7f8e4d4fd9a56937557790cade6a/uncertainties-3.2.2-py3-none-any.whl", hash = "sha256:fd8543355952f4052786ed4150acaf12e23117bd0f5bd03ea07de466bce646e7", size = 58266 },
        ]

        [[package]]
        name = "urllib3"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/aa/63/e53da845320b757bf29ef6a9062f5c669fe997973f966045cb019c3f4b66/urllib3-2.3.0.tar.gz", hash = "sha256:f8c5449b3cf0861679ce7e0503c7b44b5ec981bec0d1d3795a07f1ba96f0204d", size = 307268 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c8/19/4ec628951a74043532ca2cf5d97b7b14863931476d117c471e8e2b1eb39f/urllib3-2.3.0-py3-none-any.whl", hash = "sha256:1cee9ad369867bfdbbb48b7dd50374c0967a0bb7710050facf0dd6911440e3df", size = 128369 },
        ]

        [[package]]
        name = "wcwidth"
        version = "0.2.13"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/6c/63/53559446a878410fc5a5974feb13d31d78d752eb18aeba59c7fef1af7598/wcwidth-0.2.13.tar.gz", hash = "sha256:72ea0c06399eb286d978fdedb6923a9eb47e1c486ce63e9b4e64fc18303972b5", size = 101301 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fd/84/fd2ba7aafacbad3c4201d395674fc6348826569da3c0937e75505ead3528/wcwidth-0.2.13-py2.py3-none-any.whl", hash = "sha256:3da69048e4540d84af32131829ff948f1e022c1c6bdb8d6102117aac784f6859", size = 34166 },
        ]

        [[package]]
        name = "yarl"
        version = "1.18.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "multidict" },
            { name = "propcache" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b7/9d/4b94a8e6d2b51b599516a5cb88e5bc99b4d8d4583e468057eaa29d5f0918/yarl-1.18.3.tar.gz", hash = "sha256:ac1801c45cbf77b6c99242eeff4fffb5e4e73a800b5c4ad4fc0be5def634d2e1", size = 181062 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d2/98/e005bc608765a8a5569f58e650961314873c8469c333616eb40bff19ae97/yarl-1.18.3-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:7df647e8edd71f000a5208fe6ff8c382a1de8edfbccdbbfe649d263de07d8c34", size = 141458 },
            { url = "https://files.pythonhosted.org/packages/df/5d/f8106b263b8ae8a866b46d9be869ac01f9b3fb7f2325f3ecb3df8003f796/yarl-1.18.3-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:c69697d3adff5aa4f874b19c0e4ed65180ceed6318ec856ebc423aa5850d84f7", size = 94365 },
            { url = "https://files.pythonhosted.org/packages/56/3e/d8637ddb9ba69bf851f765a3ee288676f7cf64fb3be13760c18cbc9d10bd/yarl-1.18.3-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:602d98f2c2d929f8e697ed274fbadc09902c4025c5a9963bf4e9edfc3ab6f7ed", size = 92181 },
            { url = "https://files.pythonhosted.org/packages/76/f9/d616a5c2daae281171de10fba41e1c0e2d8207166fc3547252f7d469b4e1/yarl-1.18.3-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:c654d5207c78e0bd6d749f6dae1dcbbfde3403ad3a4b11f3c5544d9906969dde", size = 315349 },
            { url = "https://files.pythonhosted.org/packages/bb/b4/3ea5e7b6f08f698b3769a06054783e434f6d59857181b5c4e145de83f59b/yarl-1.18.3-cp310-cp310-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:5094d9206c64181d0f6e76ebd8fb2f8fe274950a63890ee9e0ebfd58bf9d787b", size = 330494 },
            { url = "https://files.pythonhosted.org/packages/55/f1/e0fc810554877b1b67420568afff51b967baed5b53bcc983ab164eebf9c9/yarl-1.18.3-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:35098b24e0327fc4ebdc8ffe336cee0a87a700c24ffed13161af80124b7dc8e5", size = 326927 },
            { url = "https://files.pythonhosted.org/packages/a9/42/b1753949b327b36f210899f2dd0a0947c0c74e42a32de3f8eb5c7d93edca/yarl-1.18.3-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:3236da9272872443f81fedc389bace88408f64f89f75d1bdb2256069a8730ccc", size = 319703 },
            { url = "https://files.pythonhosted.org/packages/f0/6d/e87c62dc9635daefb064b56f5c97df55a2e9cc947a2b3afd4fd2f3b841c7/yarl-1.18.3-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:e2c08cc9b16f4f4bc522771d96734c7901e7ebef70c6c5c35dd0f10845270bcd", size = 310246 },
            { url = "https://files.pythonhosted.org/packages/e3/ef/e2e8d1785cdcbd986f7622d7f0098205f3644546da7919c24b95790ec65a/yarl-1.18.3-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:80316a8bd5109320d38eef8833ccf5f89608c9107d02d2a7f985f98ed6876990", size = 319730 },
            { url = "https://files.pythonhosted.org/packages/fc/15/8723e22345bc160dfde68c4b3ae8b236e868f9963c74015f1bc8a614101c/yarl-1.18.3-cp310-cp310-musllinux_1_2_armv7l.whl", hash = "sha256:c1e1cc06da1491e6734f0ea1e6294ce00792193c463350626571c287c9a704db", size = 321681 },
            { url = "https://files.pythonhosted.org/packages/86/09/bf764e974f1516efa0ae2801494a5951e959f1610dd41edbfc07e5e0f978/yarl-1.18.3-cp310-cp310-musllinux_1_2_i686.whl", hash = "sha256:fea09ca13323376a2fdfb353a5fa2e59f90cd18d7ca4eaa1fd31f0a8b4f91e62", size = 324812 },
            { url = "https://files.pythonhosted.org/packages/f6/4c/20a0187e3b903c97d857cf0272d687c1b08b03438968ae8ffc50fe78b0d6/yarl-1.18.3-cp310-cp310-musllinux_1_2_ppc64le.whl", hash = "sha256:e3b9fd71836999aad54084906f8663dffcd2a7fb5cdafd6c37713b2e72be1760", size = 337011 },
            { url = "https://files.pythonhosted.org/packages/c9/71/6244599a6e1cc4c9f73254a627234e0dad3883ece40cc33dce6265977461/yarl-1.18.3-cp310-cp310-musllinux_1_2_s390x.whl", hash = "sha256:757e81cae69244257d125ff31663249b3013b5dc0a8520d73694aed497fb195b", size = 338132 },
            { url = "https://files.pythonhosted.org/packages/af/f5/e0c3efaf74566c4b4a41cb76d27097df424052a064216beccae8d303c90f/yarl-1.18.3-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:b1771de9944d875f1b98a745bc547e684b863abf8f8287da8466cf470ef52690", size = 331849 },
            { url = "https://files.pythonhosted.org/packages/8a/b8/3d16209c2014c2f98a8f658850a57b716efb97930aebf1ca0d9325933731/yarl-1.18.3-cp310-cp310-win32.whl", hash = "sha256:8874027a53e3aea659a6d62751800cf6e63314c160fd607489ba5c2edd753cf6", size = 84309 },
            { url = "https://files.pythonhosted.org/packages/fd/b7/2e9a5b18eb0fe24c3a0e8bae994e812ed9852ab4fd067c0107fadde0d5f0/yarl-1.18.3-cp310-cp310-win_amd64.whl", hash = "sha256:93b2e109287f93db79210f86deb6b9bbb81ac32fc97236b16f7433db7fc437d8", size = 90484 },
            { url = "https://files.pythonhosted.org/packages/40/93/282b5f4898d8e8efaf0790ba6d10e2245d2c9f30e199d1a85cae9356098c/yarl-1.18.3-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:8503ad47387b8ebd39cbbbdf0bf113e17330ffd339ba1144074da24c545f0069", size = 141555 },
            { url = "https://files.pythonhosted.org/packages/6d/9c/0a49af78df099c283ca3444560f10718fadb8a18dc8b3edf8c7bd9fd7d89/yarl-1.18.3-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:02ddb6756f8f4517a2d5e99d8b2f272488e18dd0bfbc802f31c16c6c20f22193", size = 94351 },
            { url = "https://files.pythonhosted.org/packages/5a/a1/205ab51e148fdcedad189ca8dd587794c6f119882437d04c33c01a75dece/yarl-1.18.3-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:67a283dd2882ac98cc6318384f565bffc751ab564605959df4752d42483ad889", size = 92286 },
            { url = "https://files.pythonhosted.org/packages/ed/fe/88b690b30f3f59275fb674f5f93ddd4a3ae796c2b62e5bb9ece8a4914b83/yarl-1.18.3-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:d980e0325b6eddc81331d3f4551e2a333999fb176fd153e075c6d1c2530aa8a8", size = 340649 },
            { url = "https://files.pythonhosted.org/packages/07/eb/3b65499b568e01f36e847cebdc8d7ccb51fff716dbda1ae83c3cbb8ca1c9/yarl-1.18.3-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:b643562c12680b01e17239be267bc306bbc6aac1f34f6444d1bded0c5ce438ca", size = 356623 },
            { url = "https://files.pythonhosted.org/packages/33/46/f559dc184280b745fc76ec6b1954de2c55595f0ec0a7614238b9ebf69618/yarl-1.18.3-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:c017a3b6df3a1bd45b9fa49a0f54005e53fbcad16633870104b66fa1a30a29d8", size = 354007 },
            { url = "https://files.pythonhosted.org/packages/af/ba/1865d85212351ad160f19fb99808acf23aab9a0f8ff31c8c9f1b4d671fc9/yarl-1.18.3-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:75674776d96d7b851b6498f17824ba17849d790a44d282929c42dbb77d4f17ae", size = 344145 },
            { url = "https://files.pythonhosted.org/packages/94/cb/5c3e975d77755d7b3d5193e92056b19d83752ea2da7ab394e22260a7b824/yarl-1.18.3-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ccaa3a4b521b780a7e771cc336a2dba389a0861592bbce09a476190bb0c8b4b3", size = 336133 },
            { url = "https://files.pythonhosted.org/packages/19/89/b77d3fd249ab52a5c40859815765d35c91425b6bb82e7427ab2f78f5ff55/yarl-1.18.3-cp311-cp311-musllinux_1_2_aarch64.whl", hash = "sha256:2d06d3005e668744e11ed80812e61efd77d70bb7f03e33c1598c301eea20efbb", size = 347967 },
            { url = "https://files.pythonhosted.org/packages/35/bd/f6b7630ba2cc06c319c3235634c582a6ab014d52311e7d7c22f9518189b5/yarl-1.18.3-cp311-cp311-musllinux_1_2_armv7l.whl", hash = "sha256:9d41beda9dc97ca9ab0b9888cb71f7539124bc05df02c0cff6e5acc5a19dcc6e", size = 346397 },
            { url = "https://files.pythonhosted.org/packages/18/1a/0b4e367d5a72d1f095318344848e93ea70da728118221f84f1bf6c1e39e7/yarl-1.18.3-cp311-cp311-musllinux_1_2_i686.whl", hash = "sha256:ba23302c0c61a9999784e73809427c9dbedd79f66a13d84ad1b1943802eaaf59", size = 350206 },
            { url = "https://files.pythonhosted.org/packages/b5/cf/320fff4367341fb77809a2d8d7fe75b5d323a8e1b35710aafe41fdbf327b/yarl-1.18.3-cp311-cp311-musllinux_1_2_ppc64le.whl", hash = "sha256:6748dbf9bfa5ba1afcc7556b71cda0d7ce5f24768043a02a58846e4a443d808d", size = 362089 },
            { url = "https://files.pythonhosted.org/packages/57/cf/aadba261d8b920253204085268bad5e8cdd86b50162fcb1b10c10834885a/yarl-1.18.3-cp311-cp311-musllinux_1_2_s390x.whl", hash = "sha256:0b0cad37311123211dc91eadcb322ef4d4a66008d3e1bdc404808992260e1a0e", size = 366267 },
            { url = "https://files.pythonhosted.org/packages/54/58/fb4cadd81acdee6dafe14abeb258f876e4dd410518099ae9a35c88d8097c/yarl-1.18.3-cp311-cp311-musllinux_1_2_x86_64.whl", hash = "sha256:0fb2171a4486bb075316ee754c6d8382ea6eb8b399d4ec62fde2b591f879778a", size = 359141 },
            { url = "https://files.pythonhosted.org/packages/9a/7a/4c571597589da4cd5c14ed2a0b17ac56ec9ee7ee615013f74653169e702d/yarl-1.18.3-cp311-cp311-win32.whl", hash = "sha256:61b1a825a13bef4a5f10b1885245377d3cd0bf87cba068e1d9a88c2ae36880e1", size = 84402 },
            { url = "https://files.pythonhosted.org/packages/ae/7b/8600250b3d89b625f1121d897062f629883c2f45339623b69b1747ec65fa/yarl-1.18.3-cp311-cp311-win_amd64.whl", hash = "sha256:b9d60031cf568c627d028239693fd718025719c02c9f55df0a53e587aab951b5", size = 91030 },
            { url = "https://files.pythonhosted.org/packages/33/85/bd2e2729752ff4c77338e0102914897512e92496375e079ce0150a6dc306/yarl-1.18.3-cp312-cp312-macosx_10_13_universal2.whl", hash = "sha256:1dd4bdd05407ced96fed3d7f25dbbf88d2ffb045a0db60dbc247f5b3c5c25d50", size = 142644 },
            { url = "https://files.pythonhosted.org/packages/ff/74/1178322cc0f10288d7eefa6e4a85d8d2e28187ccab13d5b844e8b5d7c88d/yarl-1.18.3-cp312-cp312-macosx_10_13_x86_64.whl", hash = "sha256:7c33dd1931a95e5d9a772d0ac5e44cac8957eaf58e3c8da8c1414de7dd27c576", size = 94962 },
            { url = "https://files.pythonhosted.org/packages/be/75/79c6acc0261e2c2ae8a1c41cf12265e91628c8c58ae91f5ff59e29c0787f/yarl-1.18.3-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:25b411eddcfd56a2f0cd6a384e9f4f7aa3efee14b188de13048c25b5e91f1640", size = 92795 },
            { url = "https://files.pythonhosted.org/packages/6b/32/927b2d67a412c31199e83fefdce6e645247b4fb164aa1ecb35a0f9eb2058/yarl-1.18.3-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:436c4fc0a4d66b2badc6c5fc5ef4e47bb10e4fd9bf0c79524ac719a01f3607c2", size = 332368 },
            { url = "https://files.pythonhosted.org/packages/19/e5/859fca07169d6eceeaa4fde1997c91d8abde4e9a7c018e371640c2da2b71/yarl-1.18.3-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:e35ef8683211db69ffe129a25d5634319a677570ab6b2eba4afa860f54eeaf75", size = 342314 },
            { url = "https://files.pythonhosted.org/packages/08/75/76b63ccd91c9e03ab213ef27ae6add2e3400e77e5cdddf8ed2dbc36e3f21/yarl-1.18.3-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:84b2deecba4a3f1a398df819151eb72d29bfeb3b69abb145a00ddc8d30094512", size = 341987 },
            { url = "https://files.pythonhosted.org/packages/1a/e1/a097d5755d3ea8479a42856f51d97eeff7a3a7160593332d98f2709b3580/yarl-1.18.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:00e5a1fea0fd4f5bfa7440a47eff01d9822a65b4488f7cff83155a0f31a2ecba", size = 336914 },
            { url = "https://files.pythonhosted.org/packages/0b/42/e1b4d0e396b7987feceebe565286c27bc085bf07d61a59508cdaf2d45e63/yarl-1.18.3-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:d0e883008013c0e4aef84dcfe2a0b172c4d23c2669412cf5b3371003941f72bb", size = 325765 },
            { url = "https://files.pythonhosted.org/packages/7e/18/03a5834ccc9177f97ca1bbb245b93c13e58e8225276f01eedc4cc98ab820/yarl-1.18.3-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:5a3f356548e34a70b0172d8890006c37be92995f62d95a07b4a42e90fba54272", size = 344444 },
            { url = "https://files.pythonhosted.org/packages/c8/03/a713633bdde0640b0472aa197b5b86e90fbc4c5bc05b727b714cd8a40e6d/yarl-1.18.3-cp312-cp312-musllinux_1_2_armv7l.whl", hash = "sha256:ccd17349166b1bee6e529b4add61727d3f55edb7babbe4069b5764c9587a8cc6", size = 340760 },
            { url = "https://files.pythonhosted.org/packages/eb/99/f6567e3f3bbad8fd101886ea0276c68ecb86a2b58be0f64077396cd4b95e/yarl-1.18.3-cp312-cp312-musllinux_1_2_i686.whl", hash = "sha256:b958ddd075ddba5b09bb0be8a6d9906d2ce933aee81100db289badbeb966f54e", size = 346484 },
            { url = "https://files.pythonhosted.org/packages/8e/a9/84717c896b2fc6cb15bd4eecd64e34a2f0a9fd6669e69170c73a8b46795a/yarl-1.18.3-cp312-cp312-musllinux_1_2_ppc64le.whl", hash = "sha256:c7d79f7d9aabd6011004e33b22bc13056a3e3fb54794d138af57f5ee9d9032cb", size = 359864 },
            { url = "https://files.pythonhosted.org/packages/1e/2e/d0f5f1bef7ee93ed17e739ec8dbcb47794af891f7d165fa6014517b48169/yarl-1.18.3-cp312-cp312-musllinux_1_2_s390x.whl", hash = "sha256:4891ed92157e5430874dad17b15eb1fda57627710756c27422200c52d8a4e393", size = 364537 },
            { url = "https://files.pythonhosted.org/packages/97/8a/568d07c5d4964da5b02621a517532adb8ec5ba181ad1687191fffeda0ab6/yarl-1.18.3-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:ce1af883b94304f493698b00d0f006d56aea98aeb49d75ec7d98cd4a777e9285", size = 357861 },
            { url = "https://files.pythonhosted.org/packages/7d/e3/924c3f64b6b3077889df9a1ece1ed8947e7b61b0a933f2ec93041990a677/yarl-1.18.3-cp312-cp312-win32.whl", hash = "sha256:f91c4803173928a25e1a55b943c81f55b8872f0018be83e3ad4938adffb77dd2", size = 84097 },
            { url = "https://files.pythonhosted.org/packages/34/45/0e055320daaabfc169b21ff6174567b2c910c45617b0d79c68d7ab349b02/yarl-1.18.3-cp312-cp312-win_amd64.whl", hash = "sha256:7e2ee16578af3b52ac2f334c3b1f92262f47e02cc6193c598502bd46f5cd1477", size = 90399 },
            { url = "https://files.pythonhosted.org/packages/30/c7/c790513d5328a8390be8f47be5d52e141f78b66c6c48f48d241ca6bd5265/yarl-1.18.3-cp313-cp313-macosx_10_13_universal2.whl", hash = "sha256:90adb47ad432332d4f0bc28f83a5963f426ce9a1a8809f5e584e704b82685dcb", size = 140789 },
            { url = "https://files.pythonhosted.org/packages/30/aa/a2f84e93554a578463e2edaaf2300faa61c8701f0898725842c704ba5444/yarl-1.18.3-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:913829534200eb0f789d45349e55203a091f45c37a2674678744ae52fae23efa", size = 94144 },
            { url = "https://files.pythonhosted.org/packages/c6/fc/d68d8f83714b221a85ce7866832cba36d7c04a68fa6a960b908c2c84f325/yarl-1.18.3-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:ef9f7768395923c3039055c14334ba4d926f3baf7b776c923c93d80195624782", size = 91974 },
            { url = "https://files.pythonhosted.org/packages/56/4e/d2563d8323a7e9a414b5b25341b3942af5902a2263d36d20fb17c40411e2/yarl-1.18.3-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:88a19f62ff30117e706ebc9090b8ecc79aeb77d0b1f5ec10d2d27a12bc9f66d0", size = 333587 },
            { url = "https://files.pythonhosted.org/packages/25/c9/cfec0bc0cac8d054be223e9f2c7909d3e8442a856af9dbce7e3442a8ec8d/yarl-1.18.3-cp313-cp313-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:e17c9361d46a4d5addf777c6dd5eab0715a7684c2f11b88c67ac37edfba6c482", size = 344386 },
            { url = "https://files.pythonhosted.org/packages/ab/5d/4c532190113b25f1364d25f4c319322e86232d69175b91f27e3ebc2caf9a/yarl-1.18.3-cp313-cp313-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:1a74a13a4c857a84a845505fd2d68e54826a2cd01935a96efb1e9d86c728e186", size = 345421 },
            { url = "https://files.pythonhosted.org/packages/23/d1/6cdd1632da013aa6ba18cee4d750d953104a5e7aac44e249d9410a972bf5/yarl-1.18.3-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:41f7ce59d6ee7741af71d82020346af364949314ed3d87553763a2df1829cc58", size = 339384 },
            { url = "https://files.pythonhosted.org/packages/9a/c4/6b3c39bec352e441bd30f432cda6ba51681ab19bb8abe023f0d19777aad1/yarl-1.18.3-cp313-cp313-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:f52a265001d830bc425f82ca9eabda94a64a4d753b07d623a9f2863fde532b53", size = 326689 },
            { url = "https://files.pythonhosted.org/packages/23/30/07fb088f2eefdc0aa4fc1af4e3ca4eb1a3aadd1ce7d866d74c0f124e6a85/yarl-1.18.3-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:82123d0c954dc58db301f5021a01854a85bf1f3bb7d12ae0c01afc414a882ca2", size = 345453 },
            { url = "https://files.pythonhosted.org/packages/63/09/d54befb48f9cd8eec43797f624ec37783a0266855f4930a91e3d5c7717f8/yarl-1.18.3-cp313-cp313-musllinux_1_2_armv7l.whl", hash = "sha256:2ec9bbba33b2d00999af4631a3397d1fd78290c48e2a3e52d8dd72db3a067ac8", size = 341872 },
            { url = "https://files.pythonhosted.org/packages/91/26/fd0ef9bf29dd906a84b59f0cd1281e65b0c3e08c6aa94b57f7d11f593518/yarl-1.18.3-cp313-cp313-musllinux_1_2_i686.whl", hash = "sha256:fbd6748e8ab9b41171bb95c6142faf068f5ef1511935a0aa07025438dd9a9bc1", size = 347497 },
            { url = "https://files.pythonhosted.org/packages/d9/b5/14ac7a256d0511b2ac168d50d4b7d744aea1c1aa20c79f620d1059aab8b2/yarl-1.18.3-cp313-cp313-musllinux_1_2_ppc64le.whl", hash = "sha256:877d209b6aebeb5b16c42cbb377f5f94d9e556626b1bfff66d7b0d115be88d0a", size = 359981 },
            { url = "https://files.pythonhosted.org/packages/ca/b3/d493221ad5cbd18bc07e642894030437e405e1413c4236dd5db6e46bcec9/yarl-1.18.3-cp313-cp313-musllinux_1_2_s390x.whl", hash = "sha256:b464c4ab4bfcb41e3bfd3f1c26600d038376c2de3297760dfe064d2cb7ea8e10", size = 366229 },
            { url = "https://files.pythonhosted.org/packages/04/56/6a3e2a5d9152c56c346df9b8fb8edd2c8888b1e03f96324d457e5cf06d34/yarl-1.18.3-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:8d39d351e7faf01483cc7ff7c0213c412e38e5a340238826be7e0e4da450fdc8", size = 360383 },
            { url = "https://files.pythonhosted.org/packages/fd/b7/4b3c7c7913a278d445cc6284e59b2e62fa25e72758f888b7a7a39eb8423f/yarl-1.18.3-cp313-cp313-win32.whl", hash = "sha256:61ee62ead9b68b9123ec24bc866cbef297dd266175d53296e2db5e7f797f902d", size = 310152 },
            { url = "https://files.pythonhosted.org/packages/f5/d5/688db678e987c3e0fb17867970700b92603cadf36c56e5fb08f23e822a0c/yarl-1.18.3-cp313-cp313-win_amd64.whl", hash = "sha256:578e281c393af575879990861823ef19d66e2b1d0098414855dd367e234f5b3c", size = 315723 },
            { url = "https://files.pythonhosted.org/packages/f5/4b/a06e0ec3d155924f77835ed2d167ebd3b211a7b0853da1cf8d8414d784ef/yarl-1.18.3-py3-none-any.whl", hash = "sha256:b57f4f58099328dfb26c6a771d09fb20dbbae81d20cfb66141251ea063bd101b", size = 45109 },
        ]
        "###
        );
    });

    Ok(())
}

#[test]
fn overlapping_resolution_markers() -> Result<()> {
    let context = TestContext::new("3.10").with_exclude_newer("2025-01-30T00:00Z");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "ads-mega-model"
        version = "0.1.0"
        requires-python = "==3.10.*"
        dependencies = [
            "wandb==0.17.6",
        ]

        [project.optional-dependencies]
        cpu = [
          "torch==2.2.2",
        ]
        cu118 = [
          "torch==2.2.2",
        ]

        [tool.uv]
        conflicts = [
          [
            { extra = "cpu" },
            { extra = "cu118" },
          ],
        ]

        [tool.uv.sources]
        torch = [
          { index = "pytorch-cpu", extra = "cpu", marker = "platform_system != 'Darwin'" },
        ]

        [[tool.uv.index]]
        name = "pytorch-cpu"
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cpu"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 45 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock,
            @r###"
        version = 1
        revision = 1
        requires-python = "==3.10.*"
        resolution-markers = [
            "sys_platform == 'linux' and extra != 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118'",
            "sys_platform != 'linux' and extra != 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118'",
            "platform_machine != 'aarch64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra != 'extra-14-ads-mega-model-cu118'",
            "platform_machine == 'aarch64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra != 'extra-14-ads-mega-model-cu118'",
            "sys_platform != 'darwin' and sys_platform != 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra != 'extra-14-ads-mega-model-cu118'",
            "sys_platform == 'darwin' and extra == 'extra-14-ads-mega-model-cpu' and extra != 'extra-14-ads-mega-model-cu118'",
            "sys_platform == 'linux' and extra != 'extra-14-ads-mega-model-cpu' and extra != 'extra-14-ads-mega-model-cu118'",
            "sys_platform != 'linux' and extra != 'extra-14-ads-mega-model-cpu' and extra != 'extra-14-ads-mega-model-cu118'",
        ]
        conflicts = [[
            { package = "ads-mega-model", extra = "cpu" },
            { package = "ads-mega-model", extra = "cu118" },
        ]]

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

        [[package]]
        name = "ads-mega-model"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "wandb" },
        ]

        [package.optional-dependencies]
        cpu = [
            { name = "torch", version = "2.2.2", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cpu" }, marker = "platform_machine == 'aarch64' and sys_platform == 'linux'" },
            { name = "torch", version = "2.2.2", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform == 'darwin'" },
            { name = "torch", version = "2.2.2+cpu", source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cpu" }, marker = "(platform_machine != 'aarch64' and sys_platform == 'linux') or (sys_platform != 'darwin' and sys_platform != 'linux')" },
        ]
        cu118 = [
            { name = "torch", version = "2.2.2", source = { registry = "https://pypi.org/simple" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "torch", marker = "sys_platform == 'darwin' and extra == 'cpu'", specifier = "==2.2.2" },
            { name = "torch", marker = "sys_platform != 'darwin' and extra == 'cpu'", specifier = "==2.2.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cpu", conflict = { package = "ads-mega-model", extra = "cpu" } },
            { name = "torch", marker = "extra == 'cu118'", specifier = "==2.2.2" },
            { name = "wandb", specifier = "==0.17.6" },
        ]
        provides-extras = ["cpu", "cu118"]

        [[package]]
        name = "certifi"
        version = "2024.12.14"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/0f/bd/1d41ee578ce09523c81a15426705dd20969f5abf006d1afe8aeff0dd776a/certifi-2024.12.14.tar.gz", hash = "sha256:b650d30f370c2b724812bee08008be0c4163b163ddaec3f2546c1caf65f191db", size = 166010 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a5/32/8f6669fc4798494966bf446c8c4a162e0b5d893dff088afddf76414f70e1/certifi-2024.12.14-py3-none-any.whl", hash = "sha256:1275f7a45be9464efc1173084eaa30f866fe2e47d389406136d332ed4967ec56", size = 164927 },
        ]

        [[package]]
        name = "charset-normalizer"
        version = "3.4.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/b0/572805e227f01586461c80e0fd25d65a2115599cc9dad142fee4b747c357/charset_normalizer-3.4.1.tar.gz", hash = "sha256:44251f18cd68a75b56585dd00dae26183e102cd5e0f9f1466e6df5da2ed64ea3", size = 123188 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0d/58/5580c1716040bc89206c77d8f74418caf82ce519aae06450393ca73475d1/charset_normalizer-3.4.1-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:91b36a978b5ae0ee86c394f5a54d6ef44db1de0815eb43de826d41d21e4af3de", size = 198013 },
            { url = "https://files.pythonhosted.org/packages/d0/11/00341177ae71c6f5159a08168bcb98c6e6d196d372c94511f9f6c9afe0c6/charset_normalizer-3.4.1-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:7461baadb4dc00fd9e0acbe254e3d7d2112e7f92ced2adc96e54ef6501c5f176", size = 141285 },
            { url = "https://files.pythonhosted.org/packages/01/09/11d684ea5819e5a8f5100fb0b38cf8d02b514746607934134d31233e02c8/charset_normalizer-3.4.1-cp310-cp310-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:e218488cd232553829be0664c2292d3af2eeeb94b32bea483cf79ac6a694e037", size = 151449 },
            { url = "https://files.pythonhosted.org/packages/08/06/9f5a12939db324d905dc1f70591ae7d7898d030d7662f0d426e2286f68c9/charset_normalizer-3.4.1-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:80ed5e856eb7f30115aaf94e4a08114ccc8813e6ed1b5efa74f9f82e8509858f", size = 143892 },
            { url = "https://files.pythonhosted.org/packages/93/62/5e89cdfe04584cb7f4d36003ffa2936681b03ecc0754f8e969c2becb7e24/charset_normalizer-3.4.1-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:b010a7a4fd316c3c484d482922d13044979e78d1861f0e0650423144c616a46a", size = 146123 },
            { url = "https://files.pythonhosted.org/packages/a9/ac/ab729a15c516da2ab70a05f8722ecfccc3f04ed7a18e45c75bbbaa347d61/charset_normalizer-3.4.1-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4532bff1b8421fd0a320463030c7520f56a79c9024a4e88f01c537316019005a", size = 147943 },
            { url = "https://files.pythonhosted.org/packages/03/d2/3f392f23f042615689456e9a274640c1d2e5dd1d52de36ab8f7955f8f050/charset_normalizer-3.4.1-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:d973f03c0cb71c5ed99037b870f2be986c3c05e63622c017ea9816881d2dd247", size = 142063 },
            { url = "https://files.pythonhosted.org/packages/f2/e3/e20aae5e1039a2cd9b08d9205f52142329f887f8cf70da3650326670bddf/charset_normalizer-3.4.1-cp310-cp310-musllinux_1_2_i686.whl", hash = "sha256:3a3bd0dcd373514dcec91c411ddb9632c0d7d92aed7093b8c3bbb6d69ca74408", size = 150578 },
            { url = "https://files.pythonhosted.org/packages/8d/af/779ad72a4da0aed925e1139d458adc486e61076d7ecdcc09e610ea8678db/charset_normalizer-3.4.1-cp310-cp310-musllinux_1_2_ppc64le.whl", hash = "sha256:d9c3cdf5390dcd29aa8056d13e8e99526cda0305acc038b96b30352aff5ff2bb", size = 153629 },
            { url = "https://files.pythonhosted.org/packages/c2/b6/7aa450b278e7aa92cf7732140bfd8be21f5f29d5bf334ae987c945276639/charset_normalizer-3.4.1-cp310-cp310-musllinux_1_2_s390x.whl", hash = "sha256:2bdfe3ac2e1bbe5b59a1a63721eb3b95fc9b6817ae4a46debbb4e11f6232428d", size = 150778 },
            { url = "https://files.pythonhosted.org/packages/39/f4/d9f4f712d0951dcbfd42920d3db81b00dd23b6ab520419626f4023334056/charset_normalizer-3.4.1-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:eab677309cdb30d047996b36d34caeda1dc91149e4fdca0b1a039b3f79d9a807", size = 146453 },
            { url = "https://files.pythonhosted.org/packages/49/2b/999d0314e4ee0cff3cb83e6bc9aeddd397eeed693edb4facb901eb8fbb69/charset_normalizer-3.4.1-cp310-cp310-win32.whl", hash = "sha256:c0429126cf75e16c4f0ad00ee0eae4242dc652290f940152ca8c75c3a4b6ee8f", size = 95479 },
            { url = "https://files.pythonhosted.org/packages/2d/ce/3cbed41cff67e455a386fb5e5dd8906cdda2ed92fbc6297921f2e4419309/charset_normalizer-3.4.1-cp310-cp310-win_amd64.whl", hash = "sha256:9f0b8b1c6d84c8034a44893aba5e767bf9c7a211e313a9605d9c617d7083829f", size = 102790 },
            { url = "https://files.pythonhosted.org/packages/0e/f6/65ecc6878a89bb1c23a086ea335ad4bf21a588990c3f535a227b9eea9108/charset_normalizer-3.4.1-py3-none-any.whl", hash = "sha256:d98b1668f06378c6dbefec3b92299716b931cd4e6061f3c875a71ced1780ab85", size = 49767 },
        ]

        [[package]]
        name = "click"
        version = "8.1.8"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "colorama", marker = "sys_platform == 'win32' or (extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b9/2e/0090cbf739cee7d23781ad4b89a9894a41538e4fcf4c31dcdd705b78eb8b/click-8.1.8.tar.gz", hash = "sha256:ed53c9d8990d83c2a27deae68e4ee337473f6330c040a31d4225c9574d16096a", size = 226593 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7e/d4/7ebdbd03970677812aac39c869717059dbb71a4cfc033ca6e5221787892c/click-8.1.8-py3-none-any.whl", hash = "sha256:63c132bbbed01578a06712a2d1f497bb62d9c1c0d329b7903a866228027263b2", size = 98188 },
        ]

        [[package]]
        name = "colorama"
        version = "0.4.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 },
        ]

        [[package]]
        name = "docker-pycreds"
        version = "0.4.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "six" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c5/e6/d1f6c00b7221e2d7c4b470132c931325c8b22c51ca62417e300f5ce16009/docker-pycreds-0.4.0.tar.gz", hash = "sha256:6ce3270bcaf404cc4c3e27e4b6c70d3521deae82fb508767870fdbf772d584d4", size = 8754 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f5/e8/f6bd1eee09314e7e6dee49cbe2c5e22314ccdb38db16c9fc72d2fa80d054/docker_pycreds-0.4.0-py2.py3-none-any.whl", hash = "sha256:7266112468627868005106ec19cd0d722702d2b7d5912a28e19b826c3d37af49", size = 8982 },
        ]

        [[package]]
        name = "filelock"
        version = "3.17.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/dc/9c/0b15fb47b464e1b663b1acd1253a062aa5feecb07d4e597daea542ebd2b5/filelock-3.17.0.tar.gz", hash = "sha256:ee4e77401ef576ebb38cd7f13b9b28893194acc20a8e68e18730ba9c0e54660e", size = 18027 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/89/ec/00d68c4ddfedfe64159999e5f8a98fb8442729a63e2077eb9dcd89623d27/filelock-3.17.0-py3-none-any.whl", hash = "sha256:533dc2f7ba78dc2f0f531fc6c4940addf7b70a481e269a5a3b93be94ffbe8338", size = 16164 },
        ]

        [[package]]
        name = "fsspec"
        version = "2024.12.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/11/de70dee31455c546fbc88301971ec03c328f3d1138cfba14263f651e9551/fsspec-2024.12.0.tar.gz", hash = "sha256:670700c977ed2fb51e0d9f9253177ed20cbde4a3e5c0283cc5385b5870c8533f", size = 291600 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/de/86/5486b0188d08aa643e127774a99bac51ffa6cf343e3deb0583956dca5b22/fsspec-2024.12.0-py3-none-any.whl", hash = "sha256:b520aed47ad9804237ff878b504267a3b0b441e97508bd6d2d8774e3db85cee2", size = 183862 },
        ]

        [[package]]
        name = "gitdb"
        version = "4.0.12"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "smmap" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/72/94/63b0fc47eb32792c7ba1fe1b694daec9a63620db1e313033d18140c2320a/gitdb-4.0.12.tar.gz", hash = "sha256:5ef71f855d191a3326fcfbc0d5da835f26b13fbcba60c32c21091c349ffdb571", size = 394684 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a0/61/5c78b91c3143ed5c14207f463aecfc8f9dbb5092fb2869baf37c273b2705/gitdb-4.0.12-py3-none-any.whl", hash = "sha256:67073e15955400952c6565cc3e707c554a4eea2e428946f7a4c162fab9bd9bcf", size = 62794 },
        ]

        [[package]]
        name = "gitpython"
        version = "3.1.44"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "gitdb" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c0/89/37df0b71473153574a5cdef8f242de422a0f5d26d7a9e231e6f169b4ad14/gitpython-3.1.44.tar.gz", hash = "sha256:c87e30b26253bf5418b01b0660f818967f3c503193838337fe5e573331249269", size = 214196 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/1d/9a/4114a9057db2f1462d5c8f8390ab7383925fe1ac012eaa42402ad65c2963/GitPython-3.1.44-py3-none-any.whl", hash = "sha256:9e0e10cda9bed1ee64bc9a6de50e7e38a9c9943241cd7f585f6df3ed28011110", size = 207599 },
        ]

        [[package]]
        name = "idna"
        version = "3.10"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/f1/70/7703c29685631f5a7590aa73f1f1d3fa9a380e654b86af429e0934a32f7d/idna-3.10.tar.gz", hash = "sha256:12f65c9b470abda6dc35cf8e63cc574b1c52b11df2c86030af0ac09b01b13ea9", size = 190490 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/76/c6/c88e154df9c4e1a2a66ccf0005a88dfb2650c1dffb6f5ce603dfbd452ce3/idna-3.10-py3-none-any.whl", hash = "sha256:946d195a0d259cbba61165e88e65941f16e9b36ea6ddb97f00452bae8b1287d3", size = 70442 },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.5"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/af/92/b3130cbbf5591acf9ade8708c365f3238046ac7cb8ccba6e81abccb0ccff/jinja2-3.1.5.tar.gz", hash = "sha256:8fefff8dc3034e27bb80d67c671eb8a9bc424c0ef4c0826edbff304cceff43bb", size = 244674 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bd/0f/2ba5fbcd631e3e88689309dbe978c5769e883e4b84ebfe7da30b43275c5a/jinja2-3.1.5-py3-none-any.whl", hash = "sha256:aba0f4dc9ed8013c424088f68a5c226f7d6097ed89b246d7749c2ec4175c6adb", size = 134596 },
        ]

        [[package]]
        name = "markupsafe"
        version = "3.0.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b2/97/5d42485e71dfc078108a86d6de8fa46db44a1a9295e89c5d6d4a06e23a62/markupsafe-3.0.2.tar.gz", hash = "sha256:ee55d3edf80167e48ea11a923c7386f4669df67d7994554387f84e7d8b0a2bf0", size = 20537 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/04/90/d08277ce111dd22f77149fd1a5d4653eeb3b3eaacbdfcbae5afb2600eebd/MarkupSafe-3.0.2-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:7e94c425039cde14257288fd61dcfb01963e658efbc0ff54f5306b06054700f8", size = 14357 },
            { url = "https://files.pythonhosted.org/packages/04/e1/6e2194baeae0bca1fae6629dc0cbbb968d4d941469cbab11a3872edff374/MarkupSafe-3.0.2-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:9e2d922824181480953426608b81967de705c3cef4d1af983af849d7bd619158", size = 12393 },
            { url = "https://files.pythonhosted.org/packages/1d/69/35fa85a8ece0a437493dc61ce0bb6d459dcba482c34197e3efc829aa357f/MarkupSafe-3.0.2-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:38a9ef736c01fccdd6600705b09dc574584b89bea478200c5fbf112a6b0d5579", size = 21732 },
            { url = "https://files.pythonhosted.org/packages/22/35/137da042dfb4720b638d2937c38a9c2df83fe32d20e8c8f3185dbfef05f7/MarkupSafe-3.0.2-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:bbcb445fa71794da8f178f0f6d66789a28d7319071af7a496d4d507ed566270d", size = 20866 },
            { url = "https://files.pythonhosted.org/packages/29/28/6d029a903727a1b62edb51863232152fd335d602def598dade38996887f0/MarkupSafe-3.0.2-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:57cb5a3cf367aeb1d316576250f65edec5bb3be939e9247ae594b4bcbc317dfb", size = 20964 },
            { url = "https://files.pythonhosted.org/packages/cc/cd/07438f95f83e8bc028279909d9c9bd39e24149b0d60053a97b2bc4f8aa51/MarkupSafe-3.0.2-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:3809ede931876f5b2ec92eef964286840ed3540dadf803dd570c3b7e13141a3b", size = 21977 },
            { url = "https://files.pythonhosted.org/packages/29/01/84b57395b4cc062f9c4c55ce0df7d3108ca32397299d9df00fedd9117d3d/MarkupSafe-3.0.2-cp310-cp310-musllinux_1_2_i686.whl", hash = "sha256:e07c3764494e3776c602c1e78e298937c3315ccc9043ead7e685b7f2b8d47b3c", size = 21366 },
            { url = "https://files.pythonhosted.org/packages/bd/6e/61ebf08d8940553afff20d1fb1ba7294b6f8d279df9fd0c0db911b4bbcfd/MarkupSafe-3.0.2-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:b424c77b206d63d500bcb69fa55ed8d0e6a3774056bdc4839fc9298a7edca171", size = 21091 },
            { url = "https://files.pythonhosted.org/packages/11/23/ffbf53694e8c94ebd1e7e491de185124277964344733c45481f32ede2499/MarkupSafe-3.0.2-cp310-cp310-win32.whl", hash = "sha256:fcabf5ff6eea076f859677f5f0b6b5c1a51e70a376b0579e0eadef8db48c6b50", size = 15065 },
            { url = "https://files.pythonhosted.org/packages/44/06/e7175d06dd6e9172d4a69a72592cb3f7a996a9c396eee29082826449bbc3/MarkupSafe-3.0.2-cp310-cp310-win_amd64.whl", hash = "sha256:6af100e168aa82a50e186c82875a5893c5597a0c1ccdb0d8b40240b1f28b969a", size = 15514 },
        ]

        [[package]]
        name = "mpmath"
        version = "1.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e0/47/dd32fa426cc72114383ac549964eecb20ecfd886d1e5ccf5340b55b02f57/mpmath-1.3.0.tar.gz", hash = "sha256:7a28eb2a9774d00c7bc92411c19a89209d5da7c4c9a9e227be8330a23a25b91f", size = 508106 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/43/e3/7d92a15f894aa0c9c4b49b8ee9ac9850d6e63b03c9c32c0367a13ae62209/mpmath-1.3.0-py3-none-any.whl", hash = "sha256:a0b2b9fe80bbcd81a6647ff13108738cfb482d481d826cc0e02f5b35e5c88d2c", size = 536198 },
        ]

        [[package]]
        name = "networkx"
        version = "3.4.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/fd/1d/06475e1cd5264c0b870ea2cc6fdb3e37177c1e565c43f56ff17a10e3937f/networkx-3.4.2.tar.gz", hash = "sha256:307c3669428c5362aab27c8a1260aa8f47c4e91d3891f48be0141738d8d053e1", size = 2151368 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b9/54/dd730b32ea14ea797530a4479b2ed46a6fb250f682a9cfb997e968bf0261/networkx-3.4.2-py3-none-any.whl", hash = "sha256:df5d4365b724cf81b8c6a7312509d0c22386097011ad1abe274afd5e9d3bbc5f", size = 1723263 },
        ]

        [[package]]
        name = "nvidia-cublas-cu12"
        version = "12.1.3.1"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/37/6d/121efd7382d5b0284239f4ab1fc1590d86d34ed4a4a2fdb13b30ca8e5740/nvidia_cublas_cu12-12.1.3.1-py3-none-manylinux1_x86_64.whl", hash = "sha256:ee53ccca76a6fc08fb9701aa95b6ceb242cdaab118c3bb152af4e579af792728", size = 410594774 },
            { url = "https://files.pythonhosted.org/packages/c5/ef/32a375b74bea706c93deea5613552f7c9104f961b21df423f5887eca713b/nvidia_cublas_cu12-12.1.3.1-py3-none-win_amd64.whl", hash = "sha256:2b964d60e8cf11b5e1073d179d85fa340c120e99b3067558f3cf98dd69d02906", size = 439918445 },
        ]

        [[package]]
        name = "nvidia-cuda-cupti-cu12"
        version = "12.1.105"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7e/00/6b218edd739ecfc60524e585ba8e6b00554dd908de2c9c66c1af3e44e18d/nvidia_cuda_cupti_cu12-12.1.105-py3-none-manylinux1_x86_64.whl", hash = "sha256:e54fde3983165c624cb79254ae9818a456eb6e87a7fd4d56a2352c24ee542d7e", size = 14109015 },
            { url = "https://files.pythonhosted.org/packages/d0/56/0021e32ea2848c24242f6b56790bd0ccc8bf99f973ca790569c6ca028107/nvidia_cuda_cupti_cu12-12.1.105-py3-none-win_amd64.whl", hash = "sha256:bea8236d13a0ac7190bd2919c3e8e6ce1e402104276e6f9694479e48bb0eb2a4", size = 10154340 },
        ]

        [[package]]
        name = "nvidia-cuda-nvrtc-cu12"
        version = "12.1.105"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/9f/c64c03f49d6fbc56196664d05dba14e3a561038a81a638eeb47f4d4cfd48/nvidia_cuda_nvrtc_cu12-12.1.105-py3-none-manylinux1_x86_64.whl", hash = "sha256:339b385f50c309763ca65456ec75e17bbefcbbf2893f462cb8b90584cd27a1c2", size = 23671734 },
            { url = "https://files.pythonhosted.org/packages/ad/1d/f76987c4f454eb86e0b9a0e4f57c3bf1ac1d13ad13cd1a4da4eb0e0c0ce9/nvidia_cuda_nvrtc_cu12-12.1.105-py3-none-win_amd64.whl", hash = "sha256:0a98a522d9ff138b96c010a65e145dc1b4850e9ecb75a0172371793752fd46ed", size = 19331863 },
        ]

        [[package]]
        name = "nvidia-cuda-runtime-cu12"
        version = "12.1.105"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/eb/d5/c68b1d2cdfcc59e72e8a5949a37ddb22ae6cade80cd4a57a84d4c8b55472/nvidia_cuda_runtime_cu12-12.1.105-py3-none-manylinux1_x86_64.whl", hash = "sha256:6e258468ddf5796e25f1dc591a31029fa317d97a0a94ed93468fc86301d61e40", size = 823596 },
            { url = "https://files.pythonhosted.org/packages/9f/e2/7a2b4b5064af56ea8ea2d8b2776c0f2960d95c88716138806121ae52a9c9/nvidia_cuda_runtime_cu12-12.1.105-py3-none-win_amd64.whl", hash = "sha256:dfb46ef84d73fababab44cf03e3b83f80700d27ca300e537f85f636fac474344", size = 821226 },
        ]

        [[package]]
        name = "nvidia-cudnn-cu12"
        version = "8.9.2.26"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "nvidia-cublas-cu12", marker = "(sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ff/74/a2e2be7fb83aaedec84f391f082cf765dfb635e7caa9b49065f73e4835d8/nvidia_cudnn_cu12-8.9.2.26-py3-none-manylinux1_x86_64.whl", hash = "sha256:5ccb288774fdfb07a7e7025ffec286971c06d8d7b4fb162525334616d7629ff9", size = 731725872 },
        ]

        [[package]]
        name = "nvidia-cufft-cu12"
        version = "11.0.2.54"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/86/94/eb540db023ce1d162e7bea9f8f5aa781d57c65aed513c33ee9a5123ead4d/nvidia_cufft_cu12-11.0.2.54-py3-none-manylinux1_x86_64.whl", hash = "sha256:794e3948a1aa71fd817c3775866943936774d1c14e7628c74f6f7417224cdf56", size = 121635161 },
            { url = "https://files.pythonhosted.org/packages/f7/57/7927a3aa0e19927dfed30256d1c854caf991655d847a4e7c01fe87e3d4ac/nvidia_cufft_cu12-11.0.2.54-py3-none-win_amd64.whl", hash = "sha256:d9ac353f78ff89951da4af698f80870b1534ed69993f10a4cf1d96f21357e253", size = 121344196 },
        ]

        [[package]]
        name = "nvidia-curand-cu12"
        version = "10.3.2.106"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/44/31/4890b1c9abc496303412947fc7dcea3d14861720642b49e8ceed89636705/nvidia_curand_cu12-10.3.2.106-py3-none-manylinux1_x86_64.whl", hash = "sha256:9d264c5036dde4e64f1de8c50ae753237c12e0b1348738169cd0f8a536c0e1e0", size = 56467784 },
            { url = "https://files.pythonhosted.org/packages/5c/97/4c9c7c79efcdf5b70374241d48cf03b94ef6707fd18ea0c0f53684931d0b/nvidia_curand_cu12-10.3.2.106-py3-none-win_amd64.whl", hash = "sha256:75b6b0c574c0037839121317e17fd01f8a69fd2ef8e25853d826fec30bdba74a", size = 55995813 },
        ]

        [[package]]
        name = "nvidia-cusolver-cu12"
        version = "11.4.5.107"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "nvidia-cublas-cu12", marker = "(sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "nvidia-cusparse-cu12", marker = "(sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "nvidia-nvjitlink-cu12", marker = "(sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bc/1d/8de1e5c67099015c834315e333911273a8c6aaba78923dd1d1e25fc5f217/nvidia_cusolver_cu12-11.4.5.107-py3-none-manylinux1_x86_64.whl", hash = "sha256:8a7ec542f0412294b15072fa7dab71d31334014a69f953004ea7a118206fe0dd", size = 124161928 },
            { url = "https://files.pythonhosted.org/packages/b8/80/8fca0bf819122a631c3976b6fc517c1b10741b643b94046bd8dd451522c5/nvidia_cusolver_cu12-11.4.5.107-py3-none-win_amd64.whl", hash = "sha256:74e0c3a24c78612192a74fcd90dd117f1cf21dea4822e66d89e8ea80e3cd2da5", size = 121643081 },
        ]

        [[package]]
        name = "nvidia-cusparse-cu12"
        version = "12.1.0.106"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "nvidia-nvjitlink-cu12", marker = "(sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/65/5b/cfaeebf25cd9fdec14338ccb16f6b2c4c7fa9163aefcf057d86b9cc248bb/nvidia_cusparse_cu12-12.1.0.106-py3-none-manylinux1_x86_64.whl", hash = "sha256:f3b50f42cf363f86ab21f720998517a659a48131e8d538dc02f8768237bd884c", size = 195958278 },
            { url = "https://files.pythonhosted.org/packages/0f/95/48fdbba24c93614d1ecd35bc6bdc6087bd17cbacc3abc4b05a9c2a1ca232/nvidia_cusparse_cu12-12.1.0.106-py3-none-win_amd64.whl", hash = "sha256:b798237e81b9719373e8fae8d4f091b70a0cf09d9d85c95a557e11df2d8e9a5a", size = 195414588 },
        ]

        [[package]]
        name = "nvidia-nccl-cu12"
        version = "2.19.3"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/00/d0d4e48aef772ad5aebcf70b73028f88db6e5640b36c38e90445b7a57c45/nvidia_nccl_cu12-2.19.3-py3-none-manylinux1_x86_64.whl", hash = "sha256:a9734707a2c96443331c1e48c717024aa6678a0e2a4cb66b2c364d18cee6b48d", size = 165987969 },
        ]

        [[package]]
        name = "nvidia-nvjitlink-cu12"
        version = "12.8.61"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/03/f8/9d85593582bd99b8d7c65634d2304780aefade049b2b94d96e44084be90b/nvidia_nvjitlink_cu12-12.8.61-py3-none-manylinux2010_x86_64.manylinux_2_12_x86_64.whl", hash = "sha256:45fd79f2ae20bd67e8bc411055939049873bfd8fac70ff13bd4865e0b9bdab17", size = 39243473 },
            { url = "https://files.pythonhosted.org/packages/af/53/698f3758f48c5fcb1112721e40cc6714da3980d3c7e93bae5b29dafa9857/nvidia_nvjitlink_cu12-12.8.61-py3-none-manylinux2014_aarch64.manylinux_2_17_aarch64.whl", hash = "sha256:9b80ecab31085dda3ce3b41d043be0ec739216c3fc633b8abe212d5a30026df0", size = 38374634 },
            { url = "https://files.pythonhosted.org/packages/7f/c6/0d1b2bfeb2ef42c06db0570c4d081e5cde4450b54c09e43165126cfe6ff6/nvidia_nvjitlink_cu12-12.8.61-py3-none-win_amd64.whl", hash = "sha256:1166a964d25fdc0eae497574d38824305195a5283324a21ccb0ce0c802cbf41c", size = 268514099 },
        ]

        [[package]]
        name = "nvidia-nvtx-cu12"
        version = "12.1.105"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/da/d3/8057f0587683ed2fcd4dbfbdfdfa807b9160b809976099d36b8f60d08f03/nvidia_nvtx_cu12-12.1.105-py3-none-manylinux1_x86_64.whl", hash = "sha256:dc21cf308ca5691e7c04d962e213f8a4aa9bbfa23d95412f452254c2caeb09e5", size = 99138 },
            { url = "https://files.pythonhosted.org/packages/b8/d7/bd7cb2d95ac6ac6e8d05bfa96cdce69619f1ef2808e072919044c2d47a8c/nvidia_nvtx_cu12-12.1.105-py3-none-win_amd64.whl", hash = "sha256:65f4d98982b31b60026e0e6de73fbdfc09d08a96f4656dd3665ca616a11e1e82", size = 66307 },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/13/fc/128cc9cb8f03208bdbf93d3aa862e16d376844a14f9a0ce5cf4507372de4/platformdirs-4.3.6.tar.gz", hash = "sha256:357fb2acbc885b0419afd3ce3ed34564c13c9b95c89360cd9563f73aa5e2b907", size = 21302 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3c/a6/bc1012356d8ece4d66dd75c4b9fc6c1f6650ddd5991e421177d9f8f671be/platformdirs-4.3.6-py3-none-any.whl", hash = "sha256:73e575e1408ab8103900836b97580d5307456908a03e92031bab39e4554cc3fb", size = 18439 },
        ]

        [[package]]
        name = "protobuf"
        version = "5.29.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/f7/d1/e0a911544ca9993e0f17ce6d3cc0932752356c1b0a834397f28e63479344/protobuf-5.29.3.tar.gz", hash = "sha256:5da0f41edaf117bde316404bad1a486cb4ededf8e4a54891296f648e8e076620", size = 424945 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/dc/7a/1e38f3cafa022f477ca0f57a1f49962f21ad25850c3ca0acd3b9d0091518/protobuf-5.29.3-cp310-abi3-win32.whl", hash = "sha256:3ea51771449e1035f26069c4c7fd51fba990d07bc55ba80701c78f886bf9c888", size = 422708 },
            { url = "https://files.pythonhosted.org/packages/61/fa/aae8e10512b83de633f2646506a6d835b151edf4b30d18d73afd01447253/protobuf-5.29.3-cp310-abi3-win_amd64.whl", hash = "sha256:a4fa6f80816a9a0678429e84973f2f98cbc218cca434abe8db2ad0bffc98503a", size = 434508 },
            { url = "https://files.pythonhosted.org/packages/dd/04/3eaedc2ba17a088961d0e3bd396eac764450f431621b58a04ce898acd126/protobuf-5.29.3-cp38-abi3-macosx_10_9_universal2.whl", hash = "sha256:a8434404bbf139aa9e1300dbf989667a83d42ddda9153d8ab76e0d5dcaca484e", size = 417825 },
            { url = "https://files.pythonhosted.org/packages/4f/06/7c467744d23c3979ce250397e26d8ad8eeb2bea7b18ca12ad58313c1b8d5/protobuf-5.29.3-cp38-abi3-manylinux2014_aarch64.whl", hash = "sha256:daaf63f70f25e8689c072cfad4334ca0ac1d1e05a92fc15c54eb9cf23c3efd84", size = 319573 },
            { url = "https://files.pythonhosted.org/packages/a8/45/2ebbde52ad2be18d3675b6bee50e68cd73c9e0654de77d595540b5129df8/protobuf-5.29.3-cp38-abi3-manylinux2014_x86_64.whl", hash = "sha256:c027e08a08be10b67c06bf2370b99c811c466398c357e615ca88c91c07f0910f", size = 319672 },
            { url = "https://files.pythonhosted.org/packages/fd/b2/ab07b09e0f6d143dfb839693aa05765257bceaa13d03bf1a696b78323e7a/protobuf-5.29.3-py3-none-any.whl", hash = "sha256:0a18ed4a24198528f2333802eb075e59dea9d679ab7a6c5efb017a59004d849f", size = 172550 },
        ]

        [[package]]
        name = "psutil"
        version = "6.1.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/1f/5a/07871137bb752428aa4b659f910b399ba6f291156bdea939be3e96cae7cb/psutil-6.1.1.tar.gz", hash = "sha256:cf8496728c18f2d0b45198f06895be52f36611711746b7f30c464b422b50e2f5", size = 508502 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/61/99/ca79d302be46f7bdd8321089762dd4476ee725fce16fc2b2e1dbba8cac17/psutil-6.1.1-cp36-abi3-macosx_10_9_x86_64.whl", hash = "sha256:fc0ed7fe2231a444fc219b9c42d0376e0a9a1a72f16c5cfa0f68d19f1a0663e8", size = 247511 },
            { url = "https://files.pythonhosted.org/packages/0b/6b/73dbde0dd38f3782905d4587049b9be64d76671042fdcaf60e2430c6796d/psutil-6.1.1-cp36-abi3-macosx_11_0_arm64.whl", hash = "sha256:0bdd4eab935276290ad3cb718e9809412895ca6b5b334f5a9111ee6d9aff9377", size = 248985 },
            { url = "https://files.pythonhosted.org/packages/17/38/c319d31a1d3f88c5b79c68b3116c129e5133f1822157dd6da34043e32ed6/psutil-6.1.1-cp36-abi3-manylinux_2_12_i686.manylinux2010_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:b6e06c20c05fe95a3d7302d74e7097756d4ba1247975ad6905441ae1b5b66003", size = 284488 },
            { url = "https://files.pythonhosted.org/packages/9c/39/0f88a830a1c8a3aba27fededc642da37613c57cbff143412e3536f89784f/psutil-6.1.1-cp36-abi3-manylinux_2_12_x86_64.manylinux2010_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:97f7cb9921fbec4904f522d972f0c0e1f4fabbdd4e0287813b21215074a0f160", size = 287477 },
            { url = "https://files.pythonhosted.org/packages/47/da/99f4345d4ddf2845cb5b5bd0d93d554e84542d116934fde07a0c50bd4e9f/psutil-6.1.1-cp36-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:33431e84fee02bc84ea36d9e2c4a6d395d479c9dd9bba2376c1f6ee8f3a4e0b3", size = 289017 },
            { url = "https://files.pythonhosted.org/packages/38/53/bd755c2896f4461fd4f36fa6a6dcb66a88a9e4b9fd4e5b66a77cf9d4a584/psutil-6.1.1-cp37-abi3-win32.whl", hash = "sha256:eaa912e0b11848c4d9279a93d7e2783df352b082f40111e078388701fd479e53", size = 250602 },
            { url = "https://files.pythonhosted.org/packages/7b/d7/7831438e6c3ebbfa6e01a927127a6cb42ad3ab844247f3c5b96bea25d73d/psutil-6.1.1-cp37-abi3-win_amd64.whl", hash = "sha256:f35cfccb065fff93529d2afb4a2e89e363fe63ca1e4a5da22b603a85833c2649", size = 254444 },
        ]

        [[package]]
        name = "pyyaml"
        version = "6.0.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/54/ed/79a089b6be93607fa5cdaedf301d7dfb23af5f25c398d5ead2525b063e17/pyyaml-6.0.2.tar.gz", hash = "sha256:d584d9ec91ad65861cc08d42e834324ef890a082e591037abe114850ff7bbc3e", size = 130631 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9b/95/a3fac87cb7158e231b5a6012e438c647e1a87f09f8e0d123acec8ab8bf71/PyYAML-6.0.2-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:0a9a2848a5b7feac301353437eb7d5957887edbf81d56e903999a75a3d743086", size = 184199 },
            { url = "https://files.pythonhosted.org/packages/c7/7a/68bd47624dab8fd4afbfd3c48e3b79efe09098ae941de5b58abcbadff5cb/PyYAML-6.0.2-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:29717114e51c84ddfba879543fb232a6ed60086602313ca38cce623c1d62cfbf", size = 171758 },
            { url = "https://files.pythonhosted.org/packages/49/ee/14c54df452143b9ee9f0f29074d7ca5516a36edb0b4cc40c3f280131656f/PyYAML-6.0.2-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:8824b5a04a04a047e72eea5cec3bc266db09e35de6bdfe34c9436ac5ee27d237", size = 718463 },
            { url = "https://files.pythonhosted.org/packages/4d/61/de363a97476e766574650d742205be468921a7b532aa2499fcd886b62530/PyYAML-6.0.2-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:7c36280e6fb8385e520936c3cb3b8042851904eba0e58d277dca80a5cfed590b", size = 719280 },
            { url = "https://files.pythonhosted.org/packages/6b/4e/1523cb902fd98355e2e9ea5e5eb237cbc5f3ad5f3075fa65087aa0ecb669/PyYAML-6.0.2-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:ec031d5d2feb36d1d1a24380e4db6d43695f3748343d99434e6f5f9156aaa2ed", size = 751239 },
            { url = "https://files.pythonhosted.org/packages/b7/33/5504b3a9a4464893c32f118a9cc045190a91637b119a9c881da1cf6b7a72/PyYAML-6.0.2-cp310-cp310-musllinux_1_1_aarch64.whl", hash = "sha256:936d68689298c36b53b29f23c6dbb74de12b4ac12ca6cfe0e047bedceea56180", size = 695802 },
            { url = "https://files.pythonhosted.org/packages/5c/20/8347dcabd41ef3a3cdc4f7b7a2aff3d06598c8779faa189cdbf878b626a4/PyYAML-6.0.2-cp310-cp310-musllinux_1_1_x86_64.whl", hash = "sha256:23502f431948090f597378482b4812b0caae32c22213aecf3b55325e049a6c68", size = 720527 },
            { url = "https://files.pythonhosted.org/packages/be/aa/5afe99233fb360d0ff37377145a949ae258aaab831bde4792b32650a4378/PyYAML-6.0.2-cp310-cp310-win32.whl", hash = "sha256:2e99c6826ffa974fe6e27cdb5ed0021786b03fc98e5ee3c5bfe1fd5015f42b99", size = 144052 },
            { url = "https://files.pythonhosted.org/packages/b5/84/0fa4b06f6d6c958d207620fc60005e241ecedceee58931bb20138e1e5776/PyYAML-6.0.2-cp310-cp310-win_amd64.whl", hash = "sha256:a4d3091415f010369ae4ed1fc6b79def9416358877534caf6a0fdd2146c87a3e", size = 161774 },
        ]

        [[package]]
        name = "requests"
        version = "2.32.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "certifi" },
            { name = "charset-normalizer" },
            { name = "idna" },
            { name = "urllib3" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/63/70/2bf7780ad2d390a8d301ad0b550f1581eadbd9a20f896afe06353c2a2913/requests-2.32.3.tar.gz", hash = "sha256:55365417734eb18255590a9ff9eb97e9e1da868d4ccd6402399eaf68af20a760", size = 131218 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/9b/335f9764261e915ed497fcdeb11df5dfd6f7bf257d4a6a2a686d80da4d54/requests-2.32.3-py3-none-any.whl", hash = "sha256:70761cfe03c773ceb22aa2f671b4757976145175cdfca038c02654d061d6dcc6", size = 64928 },
        ]

        [[package]]
        name = "sentry-sdk"
        version = "2.20.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "certifi" },
            { name = "urllib3" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/68/e8/6a366c0cd5e129dda6ecb20ff097f70b18182c248d4c27e813c21f98992a/sentry_sdk-2.20.0.tar.gz", hash = "sha256:afa82713a92facf847df3c6f63cec71eb488d826a50965def3d7722aa6f0fdab", size = 300125 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e6/0f/6f7e6cd0f4a141752caef3f79300148422fdf2b8b68b531f30b2b0c0cbda/sentry_sdk-2.20.0-py2.py3-none-any.whl", hash = "sha256:c359a1edf950eb5e80cffd7d9111f3dbeef57994cb4415df37d39fda2cf22364", size = 322576 },
        ]

        [[package]]
        name = "setproctitle"
        version = "1.3.4"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ae/4e/b09341b19b9ceb8b4c67298ab4a08ef7a4abdd3016c7bb152e9b6379031d/setproctitle-1.3.4.tar.gz", hash = "sha256:3b40d32a3e1f04e94231ed6dfee0da9e43b4f9c6b5450d53e6dd7754c34e0c50", size = 26456 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/52/f4/95937eb5c5370324a942ba90174c6d0fc7c5ad2f7f8ea989ccdbd6e1be5e/setproctitle-1.3.4-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:0f6661a69c68349172ba7b4d5dd65fec2b0917abc99002425ad78c3e58cf7595", size = 16855 },
            { url = "https://files.pythonhosted.org/packages/32/a6/d49dbb0d75d02d11db49151469e1fee740efa45de7288bffcc4d88d0c290/setproctitle-1.3.4-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:754bac5e470adac7f7ec2239c485cd0b75f8197ca8a5b86ffb20eb3a3676cc42", size = 11627 },
            { url = "https://files.pythonhosted.org/packages/2e/cd/73a0fc913db50c3b736750ce67824f1108c2173e5d043a16ef9874b4f988/setproctitle-1.3.4-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:f7bc7088c15150745baf66db62a4ced4507d44419eb66207b609f91b64a682af", size = 31187 },
            { url = "https://files.pythonhosted.org/packages/63/0f/74f9112f7f506acc01f085811c6d135751b6fa42d30207f53b25579d043a/setproctitle-1.3.4-cp310-cp310-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:a46ef3ecf61e4840fbc1145fdd38acf158d0da7543eda7b773ed2b30f75c2830", size = 32534 },
            { url = "https://files.pythonhosted.org/packages/3b/88/53eec2373745069d4c8a59d41ee2ef4a48949b77cccd0077c270261b238a/setproctitle-1.3.4-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ffcb09d5c0ffa043254ec9a734a73f3791fec8bf6333592f906bb2e91ed2af1a", size = 29657 },
            { url = "https://files.pythonhosted.org/packages/50/1c/a4d3d8c20bf3bbafd8c5038e7da09043a9d21450b6a73694ada11c01b58a/setproctitle-1.3.4-cp310-cp310-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:06c16b7a91cdc5d700271899e4383384a61aae83a3d53d0e2e5a266376083342", size = 30695 },
            { url = "https://files.pythonhosted.org/packages/a2/2a/9f290f0d10ea87a266d63025078eabfa040ad29ea10d815e167a5104de00/setproctitle-1.3.4-cp310-cp310-musllinux_1_2_aarch64.whl", hash = "sha256:9f9732e59863eaeedd3feef94b2b216cb86d40dda4fad2d0f0aaec3b31592716", size = 30340 },
            { url = "https://files.pythonhosted.org/packages/38/c4/5bfe02d4cdd16338973d452c7c6042abdd2827d90f7ce4e21bc003f2edb1/setproctitle-1.3.4-cp310-cp310-musllinux_1_2_i686.whl", hash = "sha256:e152f4ab9ea1632b5fecdd87cee354f2b2eb6e2dfc3aceb0eb36a01c1e12f94c", size = 29352 },
            { url = "https://files.pythonhosted.org/packages/b3/41/0dd85cef0e5a5a332bdda7b55738e950c2edecea3ae45c658990553d50f8/setproctitle-1.3.4-cp310-cp310-musllinux_1_2_ppc64le.whl", hash = "sha256:020ea47a79b2bbd7bd7b94b85ca956ba7cb026e82f41b20d2e1dac4008cead25", size = 31819 },
            { url = "https://files.pythonhosted.org/packages/d7/23/fbfacfed8805983a83324099484e47b9028d0d3c07a0fe017123eee3f580/setproctitle-1.3.4-cp310-cp310-musllinux_1_2_x86_64.whl", hash = "sha256:8c52b12b10e4057fc302bd09cb3e3f28bb382c30c044eb3396e805179a8260e4", size = 29745 },
            { url = "https://files.pythonhosted.org/packages/68/37/e18c5a00bfd1c4c2c815536d5c63a470e4364b571bd5096d38d0fe277bf5/setproctitle-1.3.4-cp310-cp310-win32.whl", hash = "sha256:a65a147f545f3fac86f11acb2d0b316d3e78139a9372317b7eb50561b2817ba0", size = 11358 },
            { url = "https://files.pythonhosted.org/packages/52/fd/1fae8c4c13af22d8d17816c44421085509a08dfa77f573d31447d6cd540c/setproctitle-1.3.4-cp310-cp310-win_amd64.whl", hash = "sha256:66821fada6426998762a3650a37fba77e814a249a95b1183011070744aff47f6", size = 12072 },
            { url = "https://files.pythonhosted.org/packages/2f/d0/775418662081d44b91da236ed4503e10e7008c9c5fd30193e13db388fbef/setproctitle-1.3.4-pp310-pypy310_pp73-macosx_11_0_arm64.whl", hash = "sha256:939d364a187b2adfbf6ae488664277e717d56c7951a4ddeb4f23b281bc50bfe5", size = 11153 },
            { url = "https://files.pythonhosted.org/packages/fd/1f/b3b82633336cd9908bf74cbc06dd533025b3d3c202437c4e3d0bc871ca13/setproctitle-1.3.4-pp310-pypy310_pp73-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:cb8a6a19be0cbf6da6fcbf3698b76c8af03fe83e4bd77c96c3922be3b88bf7da", size = 13310 },
            { url = "https://files.pythonhosted.org/packages/f5/89/887c6872ceed5ca344d25c8cc8a3f9b99bbcb25613c4b680476b29aabe23/setproctitle-1.3.4-pp310-pypy310_pp73-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:779006f9e1aade9522a40e8d9635115ab15dd82b7af8e655967162e9c01e2573", size = 12911 },
            { url = "https://files.pythonhosted.org/packages/b0/8d/9e4a4651b1c5845a9aec0d2c08c65768ba5ca2ec76598124b45d384a5f46/setproctitle-1.3.4-pp310-pypy310_pp73-win_amd64.whl", hash = "sha256:5519f2a7b8c535b0f1f77b30441476571373add72008230c81211ee17b423b57", size = 12105 },
        ]

        [[package]]
        name = "setuptools"
        version = "75.8.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/92/ec/089608b791d210aec4e7f97488e67ab0d33add3efccb83a056cbafe3a2a6/setuptools-75.8.0.tar.gz", hash = "sha256:c5afc8f407c626b8313a86e10311dd3f661c6cd9c09d4bf8c15c0e11f9f2b0e6", size = 1343222 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/69/8a/b9dc7678803429e4a3bc9ba462fa3dd9066824d3c607490235c6a796be5a/setuptools-75.8.0-py3-none-any.whl", hash = "sha256:e3982f444617239225d675215d51f6ba05f845d4eec313da4418fdbb56fb27e3", size = 1228782 },
        ]

        [[package]]
        name = "six"
        version = "1.17.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/94/e7/b2c673351809dca68a0e064b6af791aa332cf192da575fd474ed7d6f16a2/six-1.17.0.tar.gz", hash = "sha256:ff70335d468e7eb6ec65b95b99d3a2836546063f63acc5171de367e834932a81", size = 34031 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b7/ce/149a00dd41f10bc29e5921b496af8b574d8413afcd5e30dfa0ed46c2cc5e/six-1.17.0-py2.py3-none-any.whl", hash = "sha256:4721f391ed90541fddacab5acf947aa0d3dc7d27b2e1e8eda2be8970586c3274", size = 11050 },
        ]

        [[package]]
        name = "smmap"
        version = "5.0.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/44/cd/a040c4b3119bbe532e5b0732286f805445375489fceaec1f48306068ee3b/smmap-5.0.2.tar.gz", hash = "sha256:26ea65a03958fa0c8a1c7e8c7a58fdc77221b8910f6be2131affade476898ad5", size = 22329 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/04/be/d09147ad1ec7934636ad912901c5fd7667e1c858e19d355237db0d0cd5e4/smmap-5.0.2-py3-none-any.whl", hash = "sha256:b30115f0def7d7531d22a0fb6502488d879e75b260a9db4d0819cfb25403af5e", size = 24303 },
        ]

        [[package]]
        name = "sympy"
        version = "1.13.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "mpmath" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/11/8a/5a7fd6284fa8caac23a26c9ddf9c30485a48169344b4bd3b0f02fef1890f/sympy-1.13.3.tar.gz", hash = "sha256:b27fd2c6530e0ab39e275fc9b683895367e51d5da91baa8d3d64db2565fec4d9", size = 7533196 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/99/ff/c87e0622b1dadea79d2fb0b25ade9ed98954c9033722eb707053d310d4f3/sympy-1.13.3-py3-none-any.whl", hash = "sha256:54612cf55a62755ee71824ce692986f23c88ffa77207b30c1368eda4a7060f73", size = 6189483 },
        ]

        [[package]]
        name = "torch"
        version = "2.2.2"
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cpu" }
        resolution-markers = [
            "platform_machine == 'aarch64' and sys_platform == 'linux'",
        ]
        dependencies = [
            { name = "filelock", marker = "platform_machine == 'aarch64' and sys_platform == 'linux'" },
            { name = "fsspec", marker = "platform_machine == 'aarch64' and sys_platform == 'linux'" },
            { name = "jinja2", marker = "platform_machine == 'aarch64' and sys_platform == 'linux'" },
            { name = "networkx", marker = "platform_machine == 'aarch64' and sys_platform == 'linux'" },
            { name = "sympy", marker = "platform_machine == 'aarch64' and sys_platform == 'linux'" },
            { name = "typing-extensions", marker = "platform_machine == 'aarch64' and sys_platform == 'linux'" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/cpu/torch-2.2.2-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:3a2c075218081ef9c7bf8c55c706f236daebb753da41de498aca7163257380bd" },
            { url = "https://download.pytorch.org/whl/cpu/torch-2.2.2-cp310-none-macosx_10_9_x86_64.whl", hash = "sha256:e677c4d74db0cfc2b10923de1bde575d981cba54505ddc082b0508d964119850" },
            { url = "https://download.pytorch.org/whl/cpu/torch-2.2.2-cp310-none-macosx_11_0_arm64.whl", hash = "sha256:b520d14d2f2810ad5da758bea10caf7978ef3643565bc00f90de892e00d77925" },
        ]

        [[package]]
        name = "torch"
        version = "2.2.2"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform == 'linux' and extra != 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118'",
            "sys_platform != 'linux' and extra != 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118'",
            "sys_platform == 'darwin' and extra == 'extra-14-ads-mega-model-cpu' and extra != 'extra-14-ads-mega-model-cu118'",
        ]
        dependencies = [
            { name = "filelock" },
            { name = "fsspec" },
            { name = "jinja2" },
            { name = "networkx" },
            { name = "nvidia-cublas-cu12", marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (platform_machine != 'x86_64' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118') or (sys_platform != 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "nvidia-cuda-cupti-cu12", marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (platform_machine != 'x86_64' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118') or (sys_platform != 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "nvidia-cuda-nvrtc-cu12", marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (platform_machine != 'x86_64' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118') or (sys_platform != 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "nvidia-cuda-runtime-cu12", marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (platform_machine != 'x86_64' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118') or (sys_platform != 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "nvidia-cudnn-cu12", marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (platform_machine != 'x86_64' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118') or (sys_platform != 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "nvidia-cufft-cu12", marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (platform_machine != 'x86_64' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118') or (sys_platform != 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "nvidia-curand-cu12", marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (platform_machine != 'x86_64' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118') or (sys_platform != 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "nvidia-cusolver-cu12", marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (platform_machine != 'x86_64' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118') or (sys_platform != 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "nvidia-cusparse-cu12", marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (platform_machine != 'x86_64' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118') or (sys_platform != 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "nvidia-nccl-cu12", marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (platform_machine != 'x86_64' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118') or (sys_platform != 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "nvidia-nvtx-cu12", marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (platform_machine != 'x86_64' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118') or (sys_platform != 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "sympy" },
            { name = "triton", marker = "(platform_machine == 'x86_64' and sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (platform_machine != 'x86_64' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118') or (sys_platform != 'linux' and extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
            { name = "typing-extensions" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/33/b3/1fcc3bccfddadfd6845dcbfe26eb4b099f1dfea5aa0e5cfb92b3c98dba5b/torch-2.2.2-cp310-cp310-manylinux1_x86_64.whl", hash = "sha256:bc889d311a855dd2dfd164daf8cc903a6b7273a747189cebafdd89106e4ad585", size = 755526581 },
            { url = "https://files.pythonhosted.org/packages/c3/7c/aeb0c5789a3f10cf909640530cd75b314959b9d9914a4996ed2c7bf8779d/torch-2.2.2-cp310-cp310-manylinux2014_aarch64.whl", hash = "sha256:15dffa4cc3261fa73d02f0ed25f5fa49ecc9e12bf1ae0a4c1e7a88bbfaad9030", size = 86623646 },
            { url = "https://files.pythonhosted.org/packages/3a/81/684d99e536b20e869a7c1222cf1dd233311fb05d3628e9570992bfb65760/torch-2.2.2-cp310-cp310-win_amd64.whl", hash = "sha256:11e8fe261233aeabd67696d6b993eeb0896faa175c6b41b9a6c9f0334bdad1c5", size = 198579616 },
            { url = "https://files.pythonhosted.org/packages/3b/55/7192974ab13e5e5577f45d14ce70d42f5a9a686b4f57bbe8c9ab45c4a61a/torch-2.2.2-cp310-none-macosx_10_9_x86_64.whl", hash = "sha256:b2e2200b245bd9f263a0d41b6a2dab69c4aca635a01b30cca78064b0ef5b109e", size = 150788930 },
            { url = "https://files.pythonhosted.org/packages/33/6b/21496316c9b8242749ee2a9064406271efdf979e91d440e8a3806b5e84bf/torch-2.2.2-cp310-none-macosx_11_0_arm64.whl", hash = "sha256:877b3e6593b5e00b35bbe111b7057464e76a7dd186a287280d941b564b0563c2", size = 59707286 },
        ]

        [[package]]
        name = "torch"
        version = "2.2.2+cpu"
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cpu" }
        resolution-markers = [
            "platform_machine != 'aarch64' and sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]
        dependencies = [
            { name = "filelock", marker = "(platform_machine != 'aarch64' and sys_platform == 'linux') or (sys_platform != 'darwin' and sys_platform != 'linux')" },
            { name = "fsspec", marker = "(platform_machine != 'aarch64' and sys_platform == 'linux') or (sys_platform != 'darwin' and sys_platform != 'linux')" },
            { name = "jinja2", marker = "(platform_machine != 'aarch64' and sys_platform == 'linux') or (sys_platform != 'darwin' and sys_platform != 'linux')" },
            { name = "networkx", marker = "(platform_machine != 'aarch64' and sys_platform == 'linux') or (sys_platform != 'darwin' and sys_platform != 'linux')" },
            { name = "sympy", marker = "(platform_machine != 'aarch64' and sys_platform == 'linux') or (sys_platform != 'darwin' and sys_platform != 'linux')" },
            { name = "typing-extensions", marker = "(platform_machine != 'aarch64' and sys_platform == 'linux') or (sys_platform != 'darwin' and sys_platform != 'linux')" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/cpu/torch-2.2.2%2Bcpu-cp310-cp310-linux_x86_64.whl", hash = "sha256:02c4fac3c964e73f5f49003e0060c697f73b67c10cc23f51c592facb29e1bd53" },
            { url = "https://download.pytorch.org/whl/cpu/torch-2.2.2%2Bcpu-cp310-cp310-win_amd64.whl", hash = "sha256:fc29dda2795dd7220d769c5926b1c50ddac9b4827897e30a10467063691cdf54" },
        ]

        [[package]]
        name = "triton"
        version = "2.2.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "filelock", marker = "(sys_platform == 'linux' and extra == 'extra-14-ads-mega-model-cu118') or (extra == 'extra-14-ads-mega-model-cpu' and extra == 'extra-14-ads-mega-model-cu118')" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/95/05/ed974ce87fe8c8843855daa2136b3409ee1c126707ab54a8b72815c08b49/triton-2.2.0-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a2294514340cfe4e8f4f9e5c66c702744c4a117d25e618bd08469d0bfed1e2e5", size = 167900779 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.12.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/df/db/f35a00659bc03fec321ba8bce9420de607a1d37f8342eee1863174c69557/typing_extensions-4.12.2.tar.gz", hash = "sha256:1a7ead55c7e559dd4dee8856e3a88b41225abfe1ce8df57b7c13915fe121ffb8", size = 85321 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl", hash = "sha256:04e5ca0351e0f3f85c6853954072df659d0d13fac324d0072316b67d7794700d", size = 37438 },
        ]

        [[package]]
        name = "urllib3"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/aa/63/e53da845320b757bf29ef6a9062f5c669fe997973f966045cb019c3f4b66/urllib3-2.3.0.tar.gz", hash = "sha256:f8c5449b3cf0861679ce7e0503c7b44b5ec981bec0d1d3795a07f1ba96f0204d", size = 307268 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c8/19/4ec628951a74043532ca2cf5d97b7b14863931476d117c471e8e2b1eb39f/urllib3-2.3.0-py3-none-any.whl", hash = "sha256:1cee9ad369867bfdbbb48b7dd50374c0967a0bb7710050facf0dd6911440e3df", size = 128369 },
        ]

        [[package]]
        name = "wandb"
        version = "0.17.6"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "docker-pycreds" },
            { name = "gitpython" },
            { name = "platformdirs" },
            { name = "protobuf" },
            { name = "psutil" },
            { name = "pyyaml" },
            { name = "requests" },
            { name = "sentry-sdk" },
            { name = "setproctitle" },
            { name = "setuptools" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/6e/23/cd49f71a56865b798914442954da000e23cd9d6dbbc2d641ddc7cc6187d4/wandb-0.17.6.tar.gz", hash = "sha256:416739f293d59b95bcb189ecbaeb19b3a74ff2b80d501a90b93e6b6f9435c017", size = 6097478 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/84/ff/67a3b4bdd24d0885894446cccd92ab8f7930ee866978c15440699e42471e/wandb-0.17.6-py3-none-any.whl", hash = "sha256:30c4f110d406368a4653fa137ad04e8ba0a68dee25836de55c3ca384b23e30c2", size = 2793588 },
            { url = "https://files.pythonhosted.org/packages/6d/11/52eb8ec54f77a511e3ecb8523d577a21fb53bf2610884bf5626349a7e04c/wandb-0.17.6-py3-none-macosx_10_14_x86_64.whl", hash = "sha256:c554897b9b9a38490fa86d972975528b424daf5bcf35a5249ef2d25bf99986c2", size = 6866558 },
            { url = "https://files.pythonhosted.org/packages/06/62/7ef0315321d04194d652cc91015a9eb411d77003783426947f4b23b395e0/wandb-0.17.6-py3-none-macosx_11_0_arm64.whl", hash = "sha256:4015215335b4cff6268342339022382a38a7e086a8388c8b52df53bc6f18f39a", size = 6600513 },
            { url = "https://files.pythonhosted.org/packages/12/85/cd8126eb64bcad7b6175496fe5e2ac84d05689ae748cd458cf2a0e322af4/wandb-0.17.6-py3-none-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:d38c52fdd0add655e0c3d408d7d0a84a19ec559512b157b8d42e9f67d98b3004", size = 6712072 },
            { url = "https://files.pythonhosted.org/packages/b4/ca/7dfa74d65e76f3608834781f163bedefe3ead636d79f782c7668a0d89a7d/wandb-0.17.6-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:86743223a5f0c76a5c5ebc5a077aaa87221545ff8f12ba754c51a0bc1fe1e67e", size = 7062688 },
            { url = "https://files.pythonhosted.org/packages/df/b9/1d26752a7c9ff5b255c921e13a9c5176e21a0b77e53d3febf892d90c86a5/wandb-0.17.6-py3-none-win32.whl", hash = "sha256:247b1c9677fd633a460201f421d4fd4f370e7243d06257fab0ad1bb728ddcc1c", size = 6504208 },
            { url = "https://files.pythonhosted.org/packages/fc/2c/280b8891362967e54de2267454ac6033568b8412ec31225f00ecc11db7a6/wandb-0.17.6-py3-none-win_amd64.whl", hash = "sha256:51954f993b372c20812616838302183f0e3abf137614f05d80c7c17c307bfff9", size = 6504213 },
        ]
        "###
        );
    });

    Ok(())
}
