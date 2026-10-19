use std::path::Path;

use uv_configuration::{DependencyGroups, InstallOptions};
use uv_lock::{Installable, Lock, PylockToml, PylockTomlErrorKind};
use uv_normalize::{DefaultGroups, PackageName};

struct Project<'a> {
    lock: &'a Lock,
    name: PackageName,
}

impl<'a> Installable<'a> for Project<'a> {
    fn install_path(&self) -> &'a Path {
        Path::new(".")
    }

    fn lock(&self) -> &'a Lock {
        self.lock
    }

    fn roots(&self) -> impl Iterator<Item = &PackageName> {
        std::iter::once(&self.name)
    }

    fn project_name(&self) -> Option<&PackageName> {
        Some(&self.name)
    }
}

const LOCK: &str = r#"
version = 1
revision = 5
requires-python = ">=3.12"

[[package]]
name = "project"
version = "1.0.0"
source = { editable = "." }
dependencies = [{ name = "dependency" }, { name = "conditional", marker = "extra == 'empty' or extra == 'fast'" }]
[package.optional-dependencies]
fast = [
    { name = "helper", marker = "sys_platform == 'linux'" },
    { name = "project", extra = ["slow"], marker = "extra == 'slow'" },
]
slow = [{ name = "helper", marker = "sys_platform == 'win32'" }]
[package.dev-dependencies]
dev = [{ name = "helper" }]
empty-group = []
[package.metadata]
requires-dist = [
    { name = "dependency" },
    { name = "conditional", marker = "extra == 'empty' or extra == 'fast'" },
    { name = "helper", marker = "extra == 'fast' and sys_platform == 'linux'" },
    { name = "project", extra = ["slow"], marker = "extra == 'fast' and extra == 'slow'" },
    { name = "helper", marker = "extra == 'slow' and sys_platform == 'win32'" },
]
provides-extras = ["empty", "fast", "slow"]
[package.metadata.requires-dev]
dev = [{ name = "helper" }]
empty-group = []

[[package]]
name = "conditional"
version = "1.0.0"
source = { registry = "https://pypi.org/simple" }
wheels = [{ url = "https://files.pythonhosted.org/packages/conditional-1.0.0-py3-none-any.whl", hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }]

[[package]]
name = "dependency"
version = "1.0.0"
source = { registry = "https://pypi.org/simple" }
wheels = [{ url = "https://files.pythonhosted.org/packages/dependency-1.0.0-py3-none-any.whl", hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }]
[package.optional-dependencies]
feature = [{ name = "leaf" }]

[[package]]
name = "helper"
version = "1.0.0"
source = { registry = "https://pypi.org/simple" }
dependencies = [{ name = "dependency", extra = ["feature"] }]
wheels = [{ url = "https://files.pythonhosted.org/packages/helper-1.0.0-py3-none-any.whl", hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }]

[[package]]
name = "leaf"
version = "1.0.0"
source = { registry = "https://pypi.org/simple" }
wheels = [{ url = "https://files.pythonhosted.org/packages/leaf-1.0.0-py3-none-any.whl", hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }]

[[package]]
name = "unrelated"
version = "1.0.0"
source = { virtual = "unrelated" }
dependencies = [{ name = "dependency", marker = "extra != 'unrelated'" }]
"#;

fn export_project(lock: &Lock) -> Result<PylockToml, PylockTomlErrorKind> {
    let project = Project {
        lock,
        name: "project".parse()?,
    };
    let install_options =
        InstallOptions::new(true, false, false, false, false, false, vec![], vec![]);
    let groups = DependencyGroups::default().with_defaults(DefaultGroups::default());
    PylockToml::from_lock_with_selection_markers(
        &project,
        Path::new("."),
        &[],
        &groups,
        None,
        &install_options,
    )
}

/// Negative extra markers outside the selected project's dependency graph do not affect its export.
#[test]
fn selectable_extras_and_groups() -> Result<(), Box<dyn std::error::Error>> {
    let lock = Lock::from_toml(LOCK)?;
    let contents = export_project(&lock)?.to_toml()?;
    insta::assert_snapshot!(contents, @r#"
    lock-version = "1.0"
    created-by = "uv"
    requires-python = ">=3.12"
    extras = [
        "empty",
        "fast",
        "slow",
    ]
    dependency-groups = [
        "dev",
        "empty-group",
    ]

    [[packages]]
    name = "conditional"
    version = "1.0.0"
    marker = "'empty' in extras or 'fast' in extras"
    index = "https://pypi.org/simple"
    wheels = [{ url = "https://files.pythonhosted.org/packages/conditional-1.0.0-py3-none-any.whl", hashes = { sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" } }]

    [[packages]]
    name = "dependency"
    version = "1.0.0"
    index = "https://pypi.org/simple"
    wheels = [{ url = "https://files.pythonhosted.org/packages/dependency-1.0.0-py3-none-any.whl", hashes = { sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" } }]

    [[packages]]
    name = "helper"
    version = "1.0.0"
    marker = "(sys_platform == 'linux' and 'fast' in extras) or (sys_platform == 'win32' and 'slow' in extras) or 'dev' in dependency_groups"
    index = "https://pypi.org/simple"
    wheels = [{ url = "https://files.pythonhosted.org/packages/helper-1.0.0-py3-none-any.whl", hashes = { sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" } }]

    [[packages]]
    name = "leaf"
    version = "1.0.0"
    marker = "(sys_platform == 'linux' and 'fast' in extras) or (sys_platform == 'win32' and 'slow' in extras) or 'dev' in dependency_groups"
    index = "https://pypi.org/simple"
    wheels = [{ url = "https://files.pythonhosted.org/packages/leaf-1.0.0-py3-none-any.whl", hashes = { sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" } }]
    "#);

    Ok(())
}

#[test]
fn rejects_missing_group_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let lock = Lock::from_toml(&LOCK.replace("revision = 5", "revision = 3"))?;
    insta::assert_snapshot!(export_project(&lock).expect_err("expected missing group metadata"), @"A multi-use pylock.toml requires a lockfile with dependency group metadata; run `uv lock` to update it");
    Ok(())
}

#[test]
fn rejects_group_python_requirement() -> Result<(), Box<dyn std::error::Error>> {
    let lock = Lock::from_toml(&LOCK.replace(
        "[package.metadata]\nrequires-dist",
        "[package.group-requires-python]\ndev = '>=3.13'\n[package.metadata]\nrequires-dist",
    ))?;
    insta::assert_snapshot!(export_project(&lock).expect_err("expected a group Python requirement error"), @"A multi-use pylock.toml does not support dependency groups with a Python requirement");
    Ok(())
}

#[test]
fn rejects_negative_extra_marker() -> Result<(), Box<dyn std::error::Error>> {
    let lock =
        Lock::from_toml(&LOCK.replace("extra == 'empty' or extra == 'fast'", "extra != 'fast'"))?;
    insta::assert_snapshot!(export_project(&lock).expect_err("expected a negative extra marker error"), @"A multi-use pylock.toml does not support negative extra markers");
    Ok(())
}

#[test]
fn rejects_conflicts() -> Result<(), Box<dyn std::error::Error>> {
    let lock = Lock::from_toml(&LOCK.replace(
        "revision = 5",
        "revision = 5\nconflicts = [[{ package = \"project\", extra = \"fast\" }, { package = \"project\", extra = \"slow\" }]]",
    ))?;
    insta::assert_snapshot!(export_project(&lock).expect_err("expected a declared conflict error"), @"A multi-use pylock.toml does not support declared conflicts");
    Ok(())
}
