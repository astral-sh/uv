use std::path::Path;

use uv_configuration::InstallOptions;
use uv_lock::{Installable, Lock, PylockToml};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::{MarkerEnvironment, MarkerEnvironmentBuilder, MarkerTree};

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
"#;

#[test]
fn selectable_extras() -> Result<(), Box<dyn std::error::Error>> {
    let lock = Lock::from_toml(LOCK)?;
    let project = Project {
        lock: &lock,
        name: "project".parse()?,
    };
    let install_options =
        InstallOptions::new(true, false, false, false, false, false, vec![], vec![]);
    let exported = PylockToml::from_lock_with_selection_markers(
        &project,
        Path::new("."),
        &[],
        None,
        &install_options,
    )?;
    let contents = exported.to_toml()?;
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
    marker = "(sys_platform != 'linux' and sys_platform != 'win32' and 'dev' in dependency_groups) or (sys_platform == 'linux' and 'fast' in extras) or (sys_platform == 'win32' and 'slow' in extras) or (sys_platform == 'linux' and 'dev' in dependency_groups) or (sys_platform == 'win32' and 'dev' in dependency_groups)"
    index = "https://pypi.org/simple"
    wheels = [{ url = "https://files.pythonhosted.org/packages/helper-1.0.0-py3-none-any.whl", hashes = { sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" } }]

    [[packages]]
    name = "leaf"
    version = "1.0.0"
    marker = "(sys_platform != 'linux' and sys_platform != 'win32' and 'dev' in dependency_groups) or (sys_platform == 'linux' and 'fast' in extras) or (sys_platform == 'win32' and 'slow' in extras) or (sys_platform == 'linux' and 'dev' in dependency_groups) or (sys_platform == 'win32' and 'dev' in dependency_groups)"
    index = "https://pypi.org/simple"
    wheels = [{ url = "https://files.pythonhosted.org/packages/leaf-1.0.0-py3-none-any.whl", hashes = { sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" } }]
    "#);

    let document = toml::from_str::<toml::Value>(&contents)?;
    let packages = document
        .get("packages")
        .and_then(toml::Value::as_array)
        .ok_or("missing packages")?;
    let cases = [
        ("linux", vec![], vec![]),
        ("linux", vec!["empty"], vec![]),
        ("linux", vec!["fast"], vec![]),
        ("win32", vec!["fast"], vec![]),
        ("win32", vec!["slow"], vec![]),
        ("darwin", vec![], vec!["dev"]),
    ];
    let mut selections = Vec::new();
    for (platform, extras, groups) in cases {
        let markers = MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
            implementation_name: "cpython",
            implementation_version: "3.12.0",
            os_name: "posix",
            platform_machine: "arm64",
            platform_python_implementation: "CPython",
            platform_release: "test",
            platform_system: "test",
            platform_version: "test",
            python_full_version: "3.12.0",
            python_version: "3.12",
            sys_platform: platform,
        })?;
        let selected_extras = extras
            .iter()
            .map(|extra| extra.parse::<ExtraName>())
            .collect::<Result<Vec<_>, _>>()?;
        let selected_groups = groups
            .iter()
            .map(|group| group.parse::<GroupName>())
            .collect::<Result<Vec<_>, _>>()?;
        let mut installed = Vec::new();
        for package in packages {
            let marker = package
                .get("marker")
                .and_then(toml::Value::as_str)
                .map(str::parse::<MarkerTree>)
                .transpose()?
                .unwrap_or(MarkerTree::TRUE);
            if marker.evaluate_pep751(&markers, &selected_extras, &selected_groups) {
                installed.push(
                    package
                        .get("name")
                        .and_then(toml::Value::as_str)
                        .ok_or("missing package name")?,
                );
            }
        }
        selections.push(format!("{platform} {extras:?} {groups:?}: {installed:?}"));
    }
    insta::assert_snapshot!(selections.join("\n"), @r#"
    linux [] []: ["dependency"]
    linux ["empty"] []: ["conditional", "dependency"]
    linux ["fast"] []: ["conditional", "dependency", "helper", "leaf"]
    win32 ["fast"] []: ["conditional", "dependency"]
    win32 ["slow"] []: ["dependency", "helper", "leaf"]
    darwin [] ["dev"]: ["dependency", "helper", "leaf"]
    "#);
    Ok(())
}

#[test]
fn selectable_extras_rejects_unsupported_locks() -> Result<(), Box<dyn std::error::Error>> {
    let install_options =
        InstallOptions::new(true, false, false, false, false, false, vec![], vec![]);
    let cases = [
        (
            LOCK.replace("revision = 5", "revision = 3"),
            "A multi-use pylock.toml requires a lockfile with dependency group metadata; run `uv lock` to update it",
        ),
        (
            LOCK.replace(
                "[package.metadata]\nrequires-dist",
                "[package.group-requires-python]\ndev = \">=3.13\"\n[package.metadata]\nrequires-dist",
            ),
            "A multi-use pylock.toml does not support dependency groups with a Python requirement",
        ),
        (
            LOCK.replace("extra == 'empty' or extra == 'fast'", "extra != 'fast'"),
            "A multi-use pylock.toml does not support negative extra markers",
        ),
        (
            LOCK.replace(
                "revision = 5",
                "revision = 5\nconflicts = [[{ package = \"project\", extra = \"fast\" }, { package = \"project\", extra = \"slow\" }]]",
            ),
            "A multi-use pylock.toml does not support declared conflicts",
        ),
    ];
    for (input, expected) in cases {
        let lock = Lock::from_toml(&input)?;
        let project = Project {
            lock: &lock,
            name: "project".parse()?,
        };
        let error = PylockToml::from_lock_with_selection_markers(
            &project,
            Path::new("."),
            &[],
            None,
            &install_options,
        )
        .err()
        .ok_or("expected an unsupported lockfile error")?;
        assert_eq!(error.to_string(), expected);
    }
    Ok(())
}
