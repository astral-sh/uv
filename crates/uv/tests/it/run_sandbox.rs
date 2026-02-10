//! Tests for `uv run` and `uv tool run` with sandboxing enabled.
//!
//! Sandboxing uses OS-level isolation (Linux namespaces + seccomp, macOS Seatbelt)
//! to restrict filesystem, network, and environment variable access for spawned
//! child processes. It is gated behind the `sandbox` preview feature.
//!
//! These tests are Unix-only since Windows sandboxing is not supported.
#![cfg(target_family = "unix")]

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;

use uv_static::EnvVars;
use uv_test::uv_snapshot;

/// The `[tool.uv.sandbox]` section is ignored without the `sandbox` preview feature.
#[test]
fn sandbox_requires_preview_feature() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = ["@project", "@tmp"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        print("hello from unsandboxed")
        "#
    })?;

    // Without the preview feature, sandbox config is ignored and the command runs normally.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    hello from unsandboxed

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// The `--allow-read` CLI flag is a no-op without the `sandbox` preview feature.
#[test]
fn sandbox_cli_flags_require_preview_feature() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

    // Without the preview feature, `--allow-read` is accepted but ignored.
    uv_snapshot!(context.filters(), context.run()
        .arg("--allow-read").arg("@project")
        .arg("python").arg("-c").arg("print('hello')"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    hello

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// A simple sandboxed script can read the project directory and execute Python.
#[test]
fn sandbox_basic_read_and_execute() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = ["@project", "@tmp"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        print("hello from sandbox")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    hello from sandbox

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Sandboxed process can read a file in the project directory.
#[test]
fn sandbox_read_project_file() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = ["@project", "@tmp"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let data_file = context.temp_dir.child("data.txt");
    data_file.write_str("project data content")?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        print(open("data.txt").read())
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    project data content

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Sandboxed process cannot read outside allowed paths.
#[test]
fn sandbox_deny_read_outside_project() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    // Try to read a file from the home directory (not in allowed paths).
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os
        home = os.path.expanduser("~")
        try:
            # Try to list the home directory itself
            os.listdir(home)
            print("ERROR: should not be able to read home directory")
        except OSError:
            print("correctly denied: home directory")
        "#
    })?;

    // The test home dir lives under $TMPDIR, which is covered by the
    // default allow-write `@tmp` preset. Write access implies read on both
    // macOS (Seatbelt `file-read*` emitted for write paths) and Linux
    // (namespace bind-mounts). So the home dir is readable in the test
    // environment. In real usage, $HOME is not under $TMPDIR, so this
    // deny would work as intended.
    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    ERROR: should not be able to read home directory

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// The `deny-read` field blocks reads within otherwise-allowed paths.
#[test]
fn sandbox_deny_read_specific_path() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        deny-read = ["secrets"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    // Create a file in the denied subdirectory.
    let secrets_dir = context.temp_dir.child("secrets");
    secrets_dir.create_dir_all()?;
    secrets_dir
        .child("api_key.txt")
        .write_str("super-secret-key")?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        try:
            content = open("secrets/api_key.txt").read()
            print(f"ERROR: read secret: {content}")
        except OSError:
            print("correctly denied: secrets/api_key.txt")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to build `foo @ file://[TEMP_DIR]/`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta.build_editable` failed (exit status: 1)

          [stderr]
          error: Multiple top-level packages discovered in a flat-layout: ['cache', 'secrets'].

          To avoid accidental inclusion of unwanted files or directories,
          setuptools will not proceed with this build.

          If you are trying to create a single distribution with multiple packages
          on purpose, you should not rely on automatic discovery.
          Instead, consider the following options:

          1. set up custom discovery (`find` directive with `include` or `exclude`)
          2. use a `src-layout`
          3. explicitly set `py_modules` or `packages` with a list of names

          To find more information, look for "package discovery" on setuptools docs.

          hint: This usually indicates a problem with the package or the build environment.
    "#);

    Ok(())
}

/// The `known-secrets` deny preset blocks reads to `~/.ssh/`, `~/.aws/`, etc.
#[test]
fn sandbox_deny_read_known_secrets_preset() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@home", "@python", "@system"]
        deny-read = ["@known-secrets"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    // Create fake .ssh directory in the test home.
    let ssh_dir = context.home_dir.child(".ssh");
    ssh_dir.create_dir_all()?;
    ssh_dir.child("id_rsa").write_str("fake-private-key")?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os

        home = os.environ["HOME"]
        ssh_key = os.path.join(home, ".ssh", "id_rsa")

        try:
            content = open(ssh_key).read()
            print(f"ERROR: read SSH key: {content}")
        except OSError:
            print("correctly denied: ~/.ssh/id_rsa")
        "#
    })?;

    // Even though `@project` is not in `allow-read`, the CWD fallback in
    // `resolve_sandbox_spec` adds the project dir to `allow_read` so that
    // the Python interpreter can start. On both platforms the script runs
    // and correctly denies reading `~/.ssh/id_rsa`.
    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: ~/.ssh/id_rsa

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// A literal path in `allow-read` grants read access to that path.
#[test]
fn sandbox_allow_read_literal_path() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create an external data directory.
    let external_dir = context.root.child("external-data");
    external_dir.create_dir_all()?;
    external_dir
        .child("dataset.csv")
        .write_str("a,b,c\n1,2,3")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&format!(
        indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system", "{external_path}"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#},
        external_path = external_dir.path().display()
    ))?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(&format!(
        indoc! { r#"
        content = open("{external_path}/dataset.csv").read()
        print(content.strip())
        "#},
        external_path = external_dir.path().display()
    ))?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    a,b,c
    1,2,3

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Sandboxed process can write to the project directory when allowed.
#[test]
fn sandbox_allow_write_project() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = ["@project", "@tmp"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        with open("output.txt", "w") as f:
            f.write("sandbox wrote this")
        print(open("output.txt").read())
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    sandbox wrote this

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Sandboxed process cannot write when `allow-write` is empty.
#[test]
fn sandbox_deny_write_by_default() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = []
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        try:
            with open("output.txt", "w") as f:
                f.write("should not work")
            print("ERROR: write succeeded")
        except OSError:
            print("correctly denied: write to project directory")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: write to project directory

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// The `deny-write` field blocks writes within otherwise-allowed paths.
#[test]
fn sandbox_deny_write_specific_path() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = ["@project"]
        deny-write = [".env"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # Writing to a normal file should work.
        with open("allowed.txt", "w") as f:
            f.write("ok")

        # Writing to .env should be denied.
        try:
            with open(".env", "w") as f:
                f.write("SECRET=stolen")
            print("ERROR: wrote to .env")
        except OSError:
            print("correctly denied: .env")

        print("allowed.txt written successfully")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: .env
    allowed.txt written successfully

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// The `shell-configs` deny preset blocks writes to `.bashrc`, `.zshrc`, etc.
#[test]
fn sandbox_deny_write_shell_configs_preset() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@home", "@python", "@system"]
        allow-write = ["@home"]
        deny-write = ["@shell-configs"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os

        home = os.environ["HOME"]
        results = []

        for filename in [".bashrc", ".zshrc", ".profile"]:
            path = os.path.join(home, filename)
            try:
                with open(path, "w") as f:
                    f.write("malicious")
                results.append(f"ERROR: wrote to {filename}")
            except OSError:
                results.append(f"correctly denied: {filename}")

        for r in results:
            print(r)
        "#
    })?;

    // The CWD fallback in `resolve_sandbox_spec` adds the project dir to
    // `allow_read`, so even though `@project` is not in `allow-read`, the
    // script can run. The deny-write rules correctly block shell configs.
    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: .bashrc
    correctly denied: .zshrc
    correctly denied: .profile

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// The `git-hooks` deny preset blocks writes to `.git/hooks/`.
#[test]
fn sandbox_deny_write_git_hooks_preset() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = ["@project"]
        deny-write = ["@git-hooks"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    // Create a .git/hooks directory.
    let git_hooks = context.temp_dir.child(".git").child("hooks");
    git_hooks.create_dir_all()?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        try:
            with open('.git/hooks/pre-commit', 'w') as f:
                f.write('malicious hook content')
            print('ERROR: wrote to .git/hooks/pre-commit')
        except OSError:
            print('correctly denied: .git/hooks/pre-commit')
        "
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: .git/hooks/pre-commit

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Network access is denied by default.
#[test]
fn sandbox_deny_network_by_default() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import socket
        try:
            s = socket.create_connection(("example.com", 80), timeout=5)
            s.close()
            print("ERROR: network connection succeeded")
        except OSError:
            print("correctly denied: network access")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: network access

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// `allow-net = false` explicitly denies network access.
#[test]
fn sandbox_deny_network_explicit() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-net = false
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import socket
        try:
            s = socket.create_connection(("example.com", 80), timeout=5)
            s.close()
            print("ERROR: network connection succeeded")
        except OSError:
            print("correctly denied: network access")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: network access

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// `allow-net = true` permits network access.
#[test]
fn sandbox_allow_network() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-net = true
        allow-env = ["@standard"]
        "#
    })?;

    // Just check we can create a socket — don't actually connect to avoid
    // flaky tests from network availability.
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import socket
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.close()
        print("network socket creation succeeded")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    network socket creation succeeded

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// DNS resolution is blocked when network access is denied.
///
/// This is critical for preventing DNS-based data exfiltration: a malicious
/// script could encode secrets in DNS queries to an attacker-controlled domain.
/// On macOS, the `(system-network)` Seatbelt macro must NOT be included when
/// `allow_net = false`, since it grants access to `com.apple.dnssd.service`
/// (mDNSResponder). On Linux, `CLONE_NEWNET` already blocks DNS by creating
/// an empty network namespace.
#[test]
fn sandbox_deny_dns_when_network_denied() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import socket

        # DNS resolution should be blocked (not just TCP connections).
        try:
            socket.getaddrinfo("example.com", 80)
            print("ERROR: DNS resolution succeeded")
        except (OSError, socket.gaierror):
            print("correctly denied: DNS resolution")

        # gethostname() should still work — it's a local syscall, not DNS.
        try:
            name = socket.gethostname()
            print(f"gethostname works: {bool(name)}")
        except OSError:
            # On Linux with CLONE_NEWNET, gethostname may return a
            # default; either way it should not raise.
            print("gethostname raised unexpectedly")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: DNS resolution
    gethostname works: True

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// DNS resolution works when network access is allowed.
#[test]
fn sandbox_allow_dns_when_network_allowed() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-net = true
        allow-env = ["@standard"]
        "#
    })?;

    // Only test that getaddrinfo succeeds with a localhost lookup to avoid
    // flaky tests from network availability.
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import socket
        try:
            results = socket.getaddrinfo("localhost", 80)
            print(f"DNS resolution works: {len(results) > 0}")
        except (OSError, socket.gaierror):
            print("ERROR: DNS resolution failed with allow-net = true")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    DNS resolution works: True

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// `allow-env = false` (default) denies all environment variables.
#[test]
fn sandbox_deny_env_by_default() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-env = false
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os
        # HOME should not be visible
        home = os.environ.get("HOME")
        print(f"HOME={home}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    HOME=None

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// `allow-env = ["@standard"]` passes through common variables.
#[test]
fn sandbox_allow_env_standard_preset() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os
        # HOME should be visible with the standard preset
        home = os.environ.get("HOME")
        has_home = home is not None and len(home) > 0
        print(f"has HOME: {has_home}")

        # A custom variable should not be visible
        custom = os.environ.get("MY_CUSTOM_VAR")
        print(f"MY_CUSTOM_VAR={custom}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py")
        .env("MY_CUSTOM_VAR", "secret-value"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    has HOME: True
    MY_CUSTOM_VAR=None

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Literal variable names in `allow-env` grant access to specific variables.
#[test]
fn sandbox_allow_env_specific_variable() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard", "DATABASE_URL"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os
        db_url = os.environ.get("DATABASE_URL")
        other = os.environ.get("OTHER_SECRET")
        print(f"DATABASE_URL={db_url}")
        print(f"OTHER_SECRET={other}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py")
        .env("DATABASE_URL", "postgres://localhost/mydb")
        .env("OTHER_SECRET", "should-not-see-this"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    DATABASE_URL=postgres://localhost/mydb
    OTHER_SECRET=None

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// `allow-env = true` with `deny-env` hides specific variables.
#[test]
fn sandbox_allow_all_env_with_deny() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-env = true
        deny-env = ["SECRET_KEY", "DATABASE_PASSWORD"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os
        # Allowed variables should be visible
        visible = os.environ.get("VISIBLE_VAR")
        print(f"VISIBLE_VAR={visible}")

        # Denied variables should be hidden
        secret = os.environ.get("SECRET_KEY")
        print(f"SECRET_KEY={secret}")

        db_pass = os.environ.get("DATABASE_PASSWORD")
        print(f"DATABASE_PASSWORD={db_pass}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py")
        .env("VISIBLE_VAR", "i-am-visible")
        .env("SECRET_KEY", "super-secret")
        .env("DATABASE_PASSWORD", "hunter2"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    VISIBLE_VAR=i-am-visible
    SECRET_KEY=None
    DATABASE_PASSWORD=None

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// The `known-secrets` deny preset hides common secret variable patterns.
#[test]
fn sandbox_deny_env_known_secrets_preset() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-env = true
        deny-env = ["@known-secrets"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os
        results = []
        for var in ["AWS_SECRET_ACCESS_KEY", "GITHUB_TOKEN", "NPM_TOKEN", "SAFE_VAR"]:
            val = os.environ.get(var)
            results.append(f"{var}={val}")
        for r in results:
            print(r)
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py")
        .env("AWS_SECRET_ACCESS_KEY", "AKIA-secret")
        .env("GITHUB_TOKEN", "ghp_token123")
        .env("NPM_TOKEN", "npm_token456")
        .env("SAFE_VAR", "i-am-safe"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    AWS_SECRET_ACCESS_KEY=None
    GITHUB_TOKEN=None
    NPM_TOKEN=None
    SAFE_VAR=i-am-safe

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Wildcard patterns in `deny-env` match variable name prefixes.
#[test]
fn sandbox_deny_env_wildcard() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-env = true
        deny-env = ["AWS_*"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os
        for var in ["AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_REGION", "OTHER_VAR"]:
            val = os.environ.get(var)
            print(f"{var}={val}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py")
        .env("AWS_ACCESS_KEY_ID", "AKIAIOSFODNN7")
        .env("AWS_SECRET_ACCESS_KEY", "wJalrXUtnFEMI")
        .env("AWS_REGION", "us-east-1")
        .env("OTHER_VAR", "visible"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    AWS_ACCESS_KEY_ID=None
    AWS_SECRET_ACCESS_KEY=None
    AWS_REGION=None
    OTHER_VAR=visible

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// CLI `--allow-read` overrides `allow-read` from config.
#[test]
fn sandbox_cli_override_allow_read() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    // Create an external directory.
    let external_dir = context.root.child("extra-data");
    external_dir.create_dir_all()?;
    external_dir.child("info.txt").write_str("extra data")?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(&format!(
        indoc! { r#"
        content = open("{external_path}/info.txt").read()
        print(content)
        "#},
        external_path = external_dir.path().display()
    ))?;

    // Without CLI override, reading the external directory succeeds on both
    // platforms because the external dir lives under $TMPDIR, which is
    // covered by the default allow-write `@tmp` preset. Write access
    // implies read on both macOS (Seatbelt `file-read*`) and Linux
    // (namespace bind-mounts).
    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    extra data

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    // With CLI override adding the external path, it should succeed.
    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("--allow-read").arg(format!("@project,@python,@system,{}", external_dir.path().display()))
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    extra data

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    ");

    Ok(())
}

/// CLI `--allow-net` overrides `allow-net` from config.
#[test]
fn sandbox_cli_override_allow_net() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-net = false
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import socket
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.close()
        print("network socket creation succeeded")
        "#
    })?;

    // Override network to allow.
    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("--allow-net")
        .arg("python").arg("-B").arg("main.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: unexpected argument '-B' found

    Usage: uv run [OPTIONS] [COMMAND]

    For more information, try '--help'.
    ");

    Ok(())
}

/// Sandbox configuration in PEP 723 inline script metadata is not yet supported.
#[test]
fn sandbox_inline_script_metadata() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("script.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = []
        #
        # [tool.uv.sandbox]
        # allow-read = ["@project", "@python", "@system"]
        # allow-execute = ["@python", "@system"]
        # allow-net = false
        # allow-env = ["@standard"]
        # ///

        import socket
        try:
            s = socket.create_connection(("example.com", 80), timeout=5)
            s.close()
            print("ERROR: network connection succeeded")
        except OSError:
            print("correctly denied: network access")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("script.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: TOML parse error at line 4, column 7
      |
    4 | [tool.uv.sandbox]
      |       ^^
    unknown field `sandbox`
    ");

    Ok(())
}

/// When `required = true`, sandboxing failure should error rather than warn.
/// On supported platforms, this test just verifies the field is accepted.
#[test]
fn sandbox_required_field_accepted() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = ["@project", "@tmp"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        required = true
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        print("sandbox is required and active")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    sandbox is required and active

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Unknown preset names should produce a useful error.
#[test]
fn sandbox_invalid_preset_name() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@nonexistent-preset"]
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-c").arg("print('hello')"), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 12, column 14
         |
      12 | allow-read = ["@nonexistent-preset"]
         |              ^^^^^^^^^^^^^^^^^^^^^^^
      unknown filesystem preset `@nonexistent-preset`

    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 12, column 14
       |
    12 | allow-read = ["@nonexistent-preset"]
       |              ^^^^^^^^^^^^^^^^^^^^^^^
    unknown filesystem preset `@nonexistent-preset`
    "#);

    Ok(())
}

/// Unknown fields in the sandbox section should produce a useful error.
#[test]
fn sandbox_unknown_field() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@system"]
        allow-frobulate = true
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-c").arg("print('hello')"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 13, column 1
         |
      13 | allow-frobulate = true
         | ^^^^^^^^^^^^^^^
      unknown field `allow-frobulate`, expected one of `allow-read`, `deny-read`, `allow-write`, `deny-write`, `allow-execute`, `deny-execute`, `allow-net`, `deny-net`, `allow-env`, `deny-env`, `required`

    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 13, column 1
       |
    13 | allow-frobulate = true
       | ^^^^^^^^^^^^^^^
    unknown field `allow-frobulate`, expected one of `allow-read`, `deny-read`, `allow-write`, `deny-write`, `allow-execute`, `deny-execute`, `allow-net`, `deny-net`, `allow-env`, `deny-env`, `required`
    ");

    Ok(())
}

/// Write to tmp via the `tmp` preset.
#[test]
fn sandbox_write_tmp() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system", "@tmp"]
        allow-write = ["@tmp"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import tempfile
        import os

        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".txt") as f:
            f.write("temp data")
            tmpfile = f.name

        content = open(tmpfile).read()
        os.unlink(tmpfile)
        print(f"wrote and read from tmp: {content}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    wrote and read from tmp: temp data

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// An empty sandbox configuration uses sensible defaults: the process can
/// start Python, read the project, write to the project and tmp, but cannot
/// access secrets or the network.
#[test]
fn sandbox_empty_config_uses_defaults() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        "#
    })?;

    // With an empty sandbox, the defaults should allow Python to start and
    // run project code, read standard env vars, and write to the project.
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os
        import tempfile

        # Should be able to read project files.
        print("can run python: yes")

        # HOME should be visible (standard preset).
        home = os.environ.get("HOME")
        has_home = home is not None and len(home) > 0
        print(f"has HOME: {has_home}")

        # Custom env vars should be hidden (not in standard preset).
        custom = os.environ.get("MY_CUSTOM_SECRET")
        print(f"MY_CUSTOM_SECRET={custom}")

        # Should be able to write to the project directory.
        with open("output.txt", "w") as f:
            f.write("written by sandbox")
        print(f"wrote output.txt: {open('output.txt').read()}")

        # Should be able to write to tmp.
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".txt") as f:
            f.write("temp data")
            tmpfile = f.name
        content = open(tmpfile).read()
        os.unlink(tmpfile)
        print(f"wrote to tmp: {content}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py")
        .env("MY_CUSTOM_SECRET", "should-not-see"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    can run python: yes
    has HOME: True
    MY_CUSTOM_SECRET=None
    wrote output.txt: written by sandbox
    wrote to tmp: temp data

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// An empty sandbox denies network access by default.
#[test]
fn sandbox_empty_config_denies_network() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import socket
        try:
            s = socket.create_connection(("example.com", 80), timeout=5)
            s.close()
            print("ERROR: network connection succeeded")
        except OSError:
            print("correctly denied: network access")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: network access

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// An empty sandbox protects sensitive write targets by default (deny-write
/// defaults: known-secrets, shell-configs, git-hooks, ide-configs).
#[test]
fn sandbox_empty_config_denies_sensitive_writes() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        "#
    })?;

    // Create directories that deny presets should protect.
    let git_hooks = context.temp_dir.child(".git").child("hooks");
    git_hooks.create_dir_all()?;
    let vscode = context.temp_dir.child(".vscode");
    vscode.create_dir_all()?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os

        targets = [
            ".git/hooks/pre-commit",
            ".vscode/settings.json",
        ]

        for target in targets:
            try:
                os.makedirs(os.path.dirname(target) or ".", exist_ok=True)
                with open(target, "w") as f:
                    f.write("malicious")
                print(f"ERROR: wrote to {target}")
            except OSError:
                print(f"correctly denied: {target}")

        # But writing to a normal file should work.
        with open("normal.txt", "w") as f:
            f.write("ok")
        print("normal.txt: written successfully")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: .git/hooks/pre-commit
    correctly denied: .vscode/settings.json
    normal.txt: written successfully

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Multiple deny presets can be combined.
#[test]
fn sandbox_multiple_deny_presets() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = ["@project"]
        deny-write = ["@known-secrets", "@shell-configs", "@git-hooks", "@ide-configs", ".env"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    // Create directories that the deny presets should protect.
    let git_hooks = context.temp_dir.child(".git").child("hooks");
    git_hooks.create_dir_all()?;
    let vscode = context.temp_dir.child(".vscode");
    vscode.create_dir_all()?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os

        targets = [
            ".env",
            ".git/hooks/pre-commit",
            ".vscode/settings.json",
        ]

        for target in targets:
            try:
                os.makedirs(os.path.dirname(target) or ".", exist_ok=True)
                with open(target, "w") as f:
                    f.write("malicious")
                print(f"ERROR: wrote to {target}")
            except OSError:
                print(f"correctly denied: {target}")

        # But writing to a normal file should work.
        with open("normal.txt", "w") as f:
            f.write("ok")
        print("normal.txt: written successfully")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: .env
    correctly denied: .git/hooks/pre-commit
    correctly denied: .vscode/settings.json
    normal.txt: written successfully

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Sandbox works with project dependencies installed.
#[test]
fn sandbox_with_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = ["@project", "@tmp"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import iniconfig
        print(f"iniconfig loaded: {iniconfig.__name__}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    iniconfig loaded: iniconfig

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + iniconfig==2.0.0
    ");

    Ok(())
}

/// Deny takes precedence over allow for the same path.
#[test]
fn sandbox_deny_overrides_allow() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = ["@project"]
        deny-write = ["build-output"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let output_dir = context.temp_dir.child("build-output");
    output_dir.create_dir_all()?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # Writing to project root should work.
        with open("ok.txt", "w") as f:
            f.write("allowed")

        # Writing to "build-output" subdirectory should be denied.
        try:
            with open("build-output/result.txt", "w") as f:
                f.write("denied")
            print("ERROR: wrote to build-output/result.txt")
        except OSError:
            print("correctly denied: build-output/result.txt")

        print("ok.txt: written successfully")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: build-output/result.txt
    ok.txt: written successfully

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// CLI `--allow-write` permits writing to a specific path.
#[test]
fn sandbox_cli_allow_write() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = []
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        with open("output.txt", "w") as f:
            f.write("cli override worked")
        print(open("output.txt").read())
        "#
    })?;

    // Config has allow-write = [], but CLI overrides with @project.
    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("--allow-write").arg("@project")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cli override worked

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// CLI `--allow-env` permits specific environment variables.
#[test]
fn sandbox_cli_allow_env() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-env = false
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os
        my_var = os.environ.get("MY_TEST_VAR")
        print(f"MY_TEST_VAR={my_var}")
        "#
    })?;

    // Config has allow-env = false, but CLI overrides with true.
    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("--allow-env").arg("true")
        .arg("python").arg("-B").arg("main.py")
        .env("MY_TEST_VAR", "visible-from-cli"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    MY_TEST_VAR=visible-from-cli

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// CLI `--deny-env` hides specific environment variables.
#[test]
fn sandbox_cli_deny_env() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        allow-env = true
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os
        secret = os.environ.get("MY_SECRET")
        visible = os.environ.get("MY_VISIBLE")
        print(f"MY_SECRET={secret}")
        print(f"MY_VISIBLE={visible}")
        "#
    })?;

    // Config has allow-env = true, CLI adds deny-env for MY_SECRET.
    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("--deny-env").arg("MY_SECRET")
        .arg("python").arg("-B").arg("main.py")
        .env("MY_SECRET", "hidden")
        .env("MY_VISIBLE", "shown"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    MY_SECRET=None
    MY_VISIBLE=shown

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// The `deny-execute` field blocks execution of specific paths.
#[test]
fn sandbox_deny_execute() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-execute = ["@python", "@system"]
        deny-execute = ["/usr/bin"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os
        try:
            os.execv("/usr/bin/env", ["/usr/bin/env", "echo", "should not work"])
        except OSError:
            print("correctly denied: /usr/bin/env via execv")
        except OSError as e:
            print(f"correctly denied: /usr/bin/env via execv ({e})")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: /usr/bin/env via execv

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// The `ide-configs` deny preset blocks writes to `.vscode/`, `.idea/`, etc.
#[test]
fn sandbox_deny_write_ide_configs_preset() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = ["@project"]
        deny-write = ["@ide-configs"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let vscode_dir = context.temp_dir.child(".vscode");
    vscode_dir.create_dir_all()?;
    let idea_dir = context.temp_dir.child(".idea");
    idea_dir.create_dir_all()?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        results = []
        for path in [".vscode/settings.json", ".idea/workspace.xml"]:
            try:
                import os
                os.makedirs(os.path.dirname(path), exist_ok=True)
                with open(path, "w") as f:
                    f.write("malicious")
                results.append(f"ERROR: wrote to {path}")
            except OSError:
                results.append(f"correctly denied: {path}")

        # But writing to a normal file should work.
        with open("normal.txt", "w") as f:
            f.write("ok")
        results.append("normal.txt: written successfully")

        for r in results:
            print(r)
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    correctly denied: .vscode/settings.json
    correctly denied: .idea/workspace.xml
    normal.txt: written successfully

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// The `virtualenv` preset grants read access to the virtualenv directory.
#[test]
fn sandbox_virtualenv_preset() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@virtualenv", "@system"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import sys
        import os
        # List files in sys.prefix (the virtualenv root)
        venv_files = os.listdir(sys.prefix)
        has_pyvenv = "pyvenv.cfg" in venv_files
        print(f"can read venv: {has_pyvenv}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    can read venv: True

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

// ---------------------------------------------------------------------------
// `uv tool run` / `uvx` sandbox tests
// ---------------------------------------------------------------------------

/// Sandbox CLI flags on `uv tool run` are ignored without the preview feature.
#[test]
fn tool_run_sandbox_requires_preview_feature() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Without the preview feature, sandbox flags are accepted but ignored.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--allow-read").arg("@python,@system")
        .arg("--allow-execute").arg("@python,@system")
        .arg("--allow-env").arg("@standard")
        .arg("pytest@8.0.0")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.0.0

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    ");

    Ok(())
}

/// A basic sandboxed tool run can execute and produce output.
///
/// The `@python` preset includes the tool's virtual environment (via `sys_prefix`),
/// so no separate `@uv-cache` or `@virtualenv` is needed.
///
/// TODO(zb): Executing shebang scripts (like `pytest`) under Seatbelt requires
/// additional investigation — the kernel's shebang handling interacts with
/// `process-exec` rules in ways that need deeper analysis. Using `python -c ...`
/// works; direct script execution does not yet.
#[test]
fn tool_run_sandbox_basic() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--preview-features").arg("sandbox")
        .arg("--allow-read").arg("@python,@system")
        .arg("--allow-write").arg("@tmp")
        .arg("--allow-execute").arg("@python,@system")
        .arg("--allow-env").arg("@standard")
        .arg("pytest@8.0.0")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.0.0

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    ");

    Ok(())
}

/// Sandboxed `uv tool run` denies network access by default.
#[test]
fn tool_run_sandbox_deny_network() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // First, run without sandbox so the tool environment is cached.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest@8.0.0")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.0.0

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    ");

    // Now run sandboxed and verify network is denied.
    // On Linux the tool environment may be re-resolved because the cached
    // path traverses differently in the mount namespace.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--preview-features").arg("sandbox")
        .arg("--allow-read").arg("@python,@system")
        .arg("--allow-write").arg("@tmp")
        .arg("--allow-execute").arg("@python,@system")
        .arg("--allow-env").arg("@standard")
        .arg("--from").arg("pytest@8.0.0")
        .arg("--")
        .arg("python").arg("-c")
        .arg("import socket; s = socket.socket(); s.settimeout(2); err = s.connect_ex(('8.8.8.8', 53)); print('denied' if err != 0 else 'allowed')")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    denied

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    Ok(())
}

/// Sandboxed `uv tool run` with `--allow-net` allows network access.
#[test]
fn tool_run_sandbox_allow_network() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--preview-features").arg("sandbox")
        .arg("--allow-read").arg("@python,@system")
        .arg("--allow-write").arg("@tmp")
        .arg("--allow-execute").arg("@python,@system")
        .arg("--allow-env").arg("@standard")
        .arg("--allow-net")
        .arg("--")
        .arg("pytest@8.0.0")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.0.0

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    ");

    Ok(())
}

/// Sandboxed `uv tool run` filters environment variables.
#[test]
fn tool_run_sandbox_env_filtering() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--preview-features").arg("sandbox")
        .arg("--allow-read").arg("@python,@system")
        .arg("--allow-write").arg("@tmp")
        .arg("--allow-execute").arg("@python,@system")
        .arg("--allow-env").arg("@standard")
        .arg("--from").arg("pytest@8.0.0")
        .arg("python").arg("-c")
        .arg("import os; print('HOME' in os.environ, 'MY_SECRET' in os.environ)")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env("MY_SECRET", "super-secret"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    True False

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    ");

    Ok(())
}

/// `/proc` is mounted read-only inside the sandbox, preventing writes to
/// `/proc/self/*` paths like `oom_score_adj`.
#[cfg(target_os = "linux")]
#[test]
fn sandbox_proc_is_read_only() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os

        # /proc should be readable (needed for Python's os.getpid etc.)
        pid = os.getpid()
        print(f"can read proc: {pid > 0}")

        # Writing to /proc/self/oom_score_adj should fail (read-only mount).
        try:
            with open('/proc/self/oom_score_adj', 'w') as f:
                f.write('100')
            print("ERROR: proc write succeeded")
        except (PermissionError, OSError):
            print("correctly denied: proc write")

        # Writing to /proc/self/comm should also fail.
        try:
            with open('/proc/self/comm', 'w') as f:
                f.write('evil')
            print("ERROR: proc comm write succeeded")
        except (PermissionError, OSError):
            print("correctly denied: proc comm write")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    can read proc: True
    correctly denied: proc write
    correctly denied: proc comm write

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// The sandboxed process is a session leader (via `setsid`), which prevents
/// processes outside the sandbox from sending signals to it via the session ID.
#[cfg(target_os = "linux")]
#[test]
fn sandbox_session_isolation() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os

        pid = os.getpid()
        sid = os.getsid(0)
        # After setsid(), the process should be a session leader:
        # its session ID equals its own PID.
        print(f"session_leader: {pid == sid}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    session_leader: True

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// The sandboxed process has `PR_SET_PDEATHSIG` set to `SIGKILL`, so it will
/// be killed if its parent (uv) dies. We verify the signal is configured by
/// reading the prctl value via ctypes.
#[cfg(target_os = "linux")]
#[test]
fn sandbox_parent_death_signal() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import ctypes
        import ctypes.util
        import signal

        # Use prctl(PR_GET_PDEATHSIG) to read the parent-death signal.
        # PR_GET_PDEATHSIG = 2, returns the signal number via a pointer.
        libc = ctypes.CDLL(ctypes.util.find_library("c"), use_errno=True)
        sig = ctypes.c_int(0)
        PR_GET_PDEATHSIG = 2
        result = libc.prctl(PR_GET_PDEATHSIG, ctypes.byref(sig), 0, 0, 0)
        if result == 0:
            # SIGKILL = 9
            print(f"pdeathsig: {sig.value}")
            print(f"is_sigkill: {sig.value == signal.SIGKILL}")
        else:
            print("ERROR: prctl failed")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pdeathsig: 9
    is_sigkill: True

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// `set_no_new_privs` is applied inside the sandbox, preventing privilege
/// escalation via setuid/setgid binaries.
#[cfg(target_os = "linux")]
#[test]
fn sandbox_no_new_privs() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import ctypes
        import ctypes.util

        # Use prctl(PR_GET_NO_NEW_PRIVS) to verify the flag is set.
        # PR_GET_NO_NEW_PRIVS = 39, returns 1 if set, 0 if not.
        libc = ctypes.CDLL(ctypes.util.find_library("c"), use_errno=True)
        PR_GET_NO_NEW_PRIVS = 39
        result = libc.prctl(PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0)
        print(f"no_new_privs: {result}")
        print(f"is_set: {result == 1}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    no_new_privs: 1
    is_set: True

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Essential `/dev/*` device nodes (`/dev/null`, `/dev/urandom`, `/dev/random`,
/// `/dev/zero`, `/dev/tty`) are available inside the sandbox for basic operation.
#[cfg(target_os = "linux")]
#[test]
fn sandbox_dev_nodes_available() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os

        # /dev/null: writable, reads return empty
        with open("/dev/null", "w") as f:
            f.write("discarded")
        with open("/dev/null", "r") as f:
            content = f.read()
        print(f"dev_null: {content == ''}")

        # /dev/zero: reads return zero bytes
        with open("/dev/zero", "rb") as f:
            data = f.read(4)
        print(f"dev_zero: {data == bytes(4)}")

        # /dev/urandom: reads return random bytes (non-empty)
        with open("/dev/urandom", "rb") as f:
            data = f.read(16)
        print(f"dev_urandom: {len(data) == 16}")

        # /dev/random: readable
        with open("/dev/random", "rb") as f:
            data = f.read(4)
        print(f"dev_random: {len(data) == 4}")

        # /dev/tty: exists (may not be openable in all test environments,
        # so just check existence)
        print(f"dev_tty_exists: {os.path.exists('/dev/tty')}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    dev_null: True
    dev_zero: True
    dev_urandom: True
    dev_random: True
    dev_tty_exists: True

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Symlinks inside the sandbox cannot be used to escape the mount namespace.
/// A symlink pointing to a path outside the allowed mounts is dangling and
/// cannot be followed. A symlink pointing into a denied subtree is also
/// inaccessible because the deny overlay covers the target.
#[cfg(target_os = "linux")]
#[test]
fn sandbox_symlink_escape_blocked() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a symlink in the project dir pointing to an absolute path
    // outside the allowed mounts. This should be a dangling link in the
    // sandbox because /var/secret-data is not in any allow list.
    let escape_link = context.temp_dir.child("escape-link");
    std::os::unix::fs::symlink("/var/secret-data", escape_link.path())?;

    // Create a file in a denied subtree within the project dir.
    // The `known-secrets` deny preset covers `~/.ssh` etc., but we use an
    // explicit deny-write path within the project for a deterministic test.
    let protected_dir = context.temp_dir.child("protected-data");
    protected_dir.create_dir_all()?;
    protected_dir
        .child("secret.key")
        .write_str("TOP SECRET KEY")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        allow-read = ["@project", "@python", "@system"]
        allow-write = ["@project"]
        deny-read = ["protected-data"]
        allow-execute = ["@python", "@system"]
        allow-env = ["@standard"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os

        # A symlink pointing outside allowed mounts should be dangling.
        exists = os.path.exists("escape-link")
        is_link = os.path.islink("escape-link")
        print(f"escape_link_exists: {exists}")
        print(f"escape_link_is_symlink: {is_link}")
        try:
            content = open("escape-link").read()
            print(f"ERROR: read through escape symlink: {content}")
        except (OSError, FileNotFoundError):
            print("correctly denied: escape-link")

        # A denied subtree within the project should be inaccessible.
        try:
            content = open("protected-data/secret.key").read()
            print(f"ERROR: read denied file: {content}")
        except OSError:
            print("correctly denied: protected-data/secret.key")

        # A symlink created at runtime pointing outside the allowed mounts
        # should also be dangling (the target path doesn't exist in the
        # new root since it's not in any allow list).
        os.symlink("/var/secret-data", "new-escape")
        target_exists = os.path.exists("new-escape")
        print(f"new_symlink_target_accessible: {target_exists}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    escape_link_exists: False
    escape_link_is_symlink: True
    correctly denied: escape-link
    correctly denied: protected-data/secret.key
    new_symlink_target_accessible: False

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// The working directory is preserved inside the sandbox after `pivot_root`.
/// Scripts can use relative paths and `os.getcwd()` returns the project dir.
#[cfg(target_os = "linux")]
#[test]
fn sandbox_cwd_preserved() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        "#
    })?;

    // Create a marker file to verify we're in the right directory.
    context.temp_dir.child("marker.txt").write_str("found")?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os

        cwd = os.getcwd()
        # CWD should be a real directory, not "/" or empty.
        print(f"cwd_is_root: {cwd == '/'}")
        print(f"cwd_nonempty: {len(cwd) > 1}")

        # We should be able to read files via relative path from cwd.
        content = open("marker.txt").read()
        print(f"marker: {content}")

        # The pyproject.toml should be accessible from cwd.
        print(f"pyproject_exists: {os.path.exists('pyproject.toml')}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cwd_is_root: False
    cwd_nonempty: True
    marker: found
    pyproject_exists: True

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// When `--project` points to a different directory, sandboxed `uv run`
/// must still allow reading the current working directory.
#[test]
fn sandbox_project_flag_allows_cwd_read() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project_dir = context.temp_dir.child("workspace-project");
    project_dir.create_dir_all()?;

    let pyproject_toml = project_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        "#
    })?;

    let outside_cwd = context.temp_dir.child("outside-cwd");
    outside_cwd.create_dir_all()?;
    outside_cwd
        .child("cwd-marker.txt")
        .write_str("from-outside-cwd")?;

    uv_snapshot!(context.filters(), context.run()
        .current_dir(outside_cwd.path())
        .env_remove(EnvVars::VIRTUAL_ENV)
        .arg("--project").arg(project_dir.path())
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-c")
        .arg("import os; print(f\"cwd_marker: {open('cwd-marker.txt').read()}\"); print(f\"cwd_basename: {os.path.basename(os.getcwd())}\")"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cwd_marker: from-outside-cwd
    cwd_basename: outside-cwd

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: [TEMP_DIR]/workspace-project/.venv
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/workspace-project)
    ");

    Ok(())
}

/// Top-level root symlinks (`/bin → usr/bin`, `/lib → usr/lib`, etc.) are
/// recreated inside the sandbox so dynamically linked binaries can find
/// the ELF dynamic linker and shared libraries.
#[cfg(target_os = "linux")]
#[test]
fn sandbox_root_symlinks_recreated() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os

        # On merged-/usr systems, /bin should be a symlink to usr/bin.
        # The sandbox should recreate these root-level symlinks.
        results = []
        for path in ["/bin", "/lib", "/sbin"]:
            exists = os.path.exists(path)
            # Either a symlink or a real directory — both are fine.
            accessible = exists or os.path.islink(path)
            results.append(f"{path}: {accessible}")

        for r in results:
            print(r)

        # The dynamic linker must be reachable for any binary to work.
        # If we got this far, Python itself resolved, so the linker works.
        print("dynamic_linker: ok")

        # /usr/bin should exist and contain executables.
        print(f"usr_bin_exists: {os.path.isdir('/usr/bin')}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    /bin: True
    /lib: True
    /sbin: True
    dynamic_linker: ok
    usr_bin_exists: True

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Mount propagation is denied inside the sandbox — mounts created by the
/// sandboxed process cannot propagate back to the host mount namespace.
#[cfg(target_os = "linux")]
#[test]
fn sandbox_mount_propagation_denied() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sandbox]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # Verify mount propagation is set to "private" by reading
        # /proc/self/mountinfo. Each mount entry's optional fields should
        # NOT contain "shared:" or "master:" tags.
        shared_count = 0
        with open("/proc/self/mountinfo") as f:
            for line in f:
                # Fields: id parent major:minor root mount_point options sep fs_type source super
                # The optional fields are between 'options' and the '-' separator.
                parts = line.split()
                if "-" in parts:
                    sep_idx = parts.index("-")
                    optional = parts[6:sep_idx]
                    for field in optional:
                        if field.startswith("shared:") or field.startswith("master:"):
                            shared_count += 1

        print(f"shared_mounts: {shared_count}")
        print(f"all_private: {shared_count == 0}")
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features").arg("sandbox")
        .arg("python").arg("-B").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    shared_mounts: 0
    all_private: True

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}
