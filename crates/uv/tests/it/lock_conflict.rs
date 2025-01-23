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
            lock, @r#"
        version = 1
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
        provides-extras = ["extra1", "extra2"]
        requires-dist = [
            { name = "sortedcontainers", marker = "extra == 'extra1'", specifier = "==2.3.0" },
            { name = "sortedcontainers", marker = "extra == 'extra2'", specifier = "==2.4.0" },
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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["extra1", "extra2", "project3"]
        requires-dist = [
            { name = "sortedcontainers", marker = "extra == 'extra1'", specifier = "==2.2.0" },
            { name = "sortedcontainers", marker = "extra == 'extra2'", specifier = "==2.3.0" },
            { name = "sortedcontainers", marker = "extra == 'project3'", specifier = "==2.4.0" },
        ]

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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["extra1", "extra2", "project3", "project4"]
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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["extra1", "extra2"]
        requires-dist = [
            { name = "sortedcontainers", marker = "extra == 'extra1'", specifier = "==2.3.0" },
            { name = "sortedcontainers", marker = "extra == 'extra2'", specifier = "==2.4.0" },
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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = []
        requires-dist = []

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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = []
        requires-dist = []

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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["extra1"]
        requires-dist = [{ name = "sortedcontainers", marker = "extra == 'extra1'", specifier = "==2.4.0" }]

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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["cu118", "cu124"]
        requires-dist = [
            { name = "jinja2", marker = "extra == 'cu118'", specifier = "==3.1.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = []
        requires-dist = []

        [package.metadata.requires-dev]
        cu118 = [{ name = "jinja2", specifier = "==3.1.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu118", conflict = { package = "project", group = "cu118" } }]
        cu124 = [{ name = "jinja2", specifier = "==3.1.3", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu124", conflict = { package = "project", group = "cu124" } }]
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["cu118", "cu124"]
        requires-dist = [
            { name = "jinja2", extras = ["i18n"], marker = "extra == 'cu118'", specifier = "==3.1.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", extras = ["i18n"], marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["cu118", "cu124"]
        requires-dist = [
            { name = "jinja2", marker = "sys_platform == 'darwin' and extra == 'cu118'", specifier = "==3.1.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", marker = "sys_platform != 'darwin' and extra == 'cu118'", specifier = "==3.1.2" },
            { name = "jinja2", marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["foo", "bar", "baz"]
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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = []
        requires-dist = []

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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["foo"]
        requires-dist = [{ name = "idna", marker = "extra == 'foo'", specifier = "==3.5" }]

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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["foo", "bar"]
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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = []
        requires-dist = []

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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["foo"]
        requires-dist = [
            { name = "anyio", marker = "extra == 'foo'" },
            { name = "idna", marker = "extra == 'foo'", specifier = "==3.5" },
        ]

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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["foo", "bar"]
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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = []
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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["foo"]
        requires-dist = [
            { name = "anyio" },
            { name = "idna", marker = "extra == 'foo'", specifier = "==3.5" },
        ]

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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["x1"]
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
        provides-extras = ["x2", "x3"]
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
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["cu118", "cu124"]
        requires-dist = [
            { name = "jinja2", extras = ["i18n"], marker = "extra == 'cu118'", specifier = "==3.1.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", extras = ["i18n"], marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        "#
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
            lock, @r#"
        version = 1
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
        provides-extras = ["cu118", "cu124"]
        requires-dist = [
            { name = "jinja2", marker = "sys_platform == 'darwin' and extra == 'cu118'", specifier = "==3.1.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu118", conflict = { package = "project", extra = "cu118" } },
            { name = "jinja2", marker = "sys_platform != 'darwin' and extra == 'cu118'", specifier = "==3.1.2" },
            { name = "jinja2", marker = "extra == 'cu124'", specifier = "==3.1.3", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]
        "#
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
            @r#"
        version = 1
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
        provides-extras = ["foo", "bar", "extra-3-pkg-foo"]
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
        "#
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
            @r#"
        version = 1
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
            { name = "alembic" },
            { name = "apache-airflow-providers-common-sql", version = "1.8.1", source = { registry = "https://pypi.org/simple" } },
            { name = "apache-airflow-providers-ftp", version = "3.6.1", source = { registry = "https://pypi.org/simple" } },
            { name = "apache-airflow-providers-http", version = "4.7.0", source = { registry = "https://pypi.org/simple" } },
            { name = "apache-airflow-providers-imap", version = "3.4.0", source = { registry = "https://pypi.org/simple" } },
            { name = "apache-airflow-providers-sqlite", version = "3.5.0", source = { registry = "https://pypi.org/simple" } },
            { name = "argcomplete" },
            { name = "attrs" },
            { name = "blinker" },
            { name = "cattrs" },
            { name = "colorlog" },
            { name = "configupdater" },
            { name = "connexion", extra = ["flask", "swagger-ui"] },
            { name = "cron-descriptor" },
            { name = "croniter" },
            { name = "cryptography" },
            { name = "deprecated" },
            { name = "dill" },
            { name = "flask" },
            { name = "flask-appbuilder", version = "4.1.4", source = { registry = "https://pypi.org/simple" } },
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
            { name = "pygments" },
            { name = "pyjwt" },
            { name = "python-daemon" },
            { name = "python-dateutil" },
            { name = "python-nvd3" },
            { name = "python-slugify" },
            { name = "rich" },
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
            { name = "apache-airflow", version = "2.5.0", source = { registry = "https://pypi.org/simple" } },
            { name = "sqlparse" },
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
            { name = "apache-airflow", version = "2.5.0", source = { registry = "https://pypi.org/simple" } },
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
            { name = "aiohttp" },
            { name = "apache-airflow", version = "2.5.0", source = { registry = "https://pypi.org/simple" } },
            { name = "asgiref" },
            { name = "requests" },
            { name = "requests-toolbelt" },
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
            { name = "apache-airflow", version = "2.5.0", source = { registry = "https://pypi.org/simple" } },
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
            { name = "apache-airflow", version = "2.5.0", source = { registry = "https://pypi.org/simple" } },
            { name = "apache-airflow-providers-common-sql", version = "1.8.1", source = { registry = "https://pypi.org/simple" } },
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
            { name = "apispec", version = "3.3.2", source = { registry = "https://pypi.org/simple" }, extra = ["yaml"] },
            { name = "click" },
            { name = "colorama" },
            { name = "email-validator" },
            { name = "flask" },
            { name = "flask-babel" },
            { name = "flask-jwt-extended" },
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
        provides-extras = ["x1", "x2"]
        requires-dist = [
            { name = "apache-airflow", marker = "extra == 'x1'", specifier = "==2.5.0" },
            { name = "apache-airflow", marker = "extra == 'x2'", specifier = "==2.6.0" },
            { name = "quickpath-airflow-operator", specifier = "==1.0.2" },
        ]

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
        "#
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
            @r#"
        version = 1
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
        provides-extras = ["x1", "x2"]
        requires-dist = [
            { name = "idna", marker = "sys_platform == 'linux' and extra == 'x1'", specifier = "==3.6" },
            { name = "idna", marker = "sys_platform != 'linux' and extra == 'x1'", specifier = "==3.5" },
            { name = "markupsafe", marker = "sys_platform == 'linux' and extra == 'x2'", specifier = "==2.1.0" },
            { name = "markupsafe", marker = "sys_platform != 'linux' and extra == 'x2'", specifier = "==2.0.0" },
        ]
        "#
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
            @r#"
        version = 1
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
        provides-extras = ["cpu", "cu118"]
        requires-dist = [
            { name = "torch", marker = "sys_platform == 'darwin' and extra == 'cpu'", specifier = "==2.2.2" },
            { name = "torch", marker = "sys_platform != 'darwin' and extra == 'cpu'", specifier = "==2.2.2", index = "https://astral-sh.github.io/pytorch-mirror/whl/cpu", conflict = { package = "ads-mega-model", extra = "cpu" } },
            { name = "torch", marker = "extra == 'cu118'", specifier = "==2.2.2" },
            { name = "wandb", specifier = "==0.17.6" },
        ]

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
        "#
        );
    });

    Ok(())
}
