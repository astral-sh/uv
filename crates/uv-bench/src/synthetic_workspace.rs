use std::fmt::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indoc::{formatdoc, indoc};

pub const SYNTHETIC_MEMBER_COUNT: usize = 127;
const OPTIONAL_DEPENDENCY_GROUP_COUNT: usize = 122;
const DEPENDENCY_GROUP_COUNT: usize = 64;
const UNUSED_ROOT_TABLE_COUNT: usize = 128;
const UNUSED_MEMBER_TABLE_COUNT: usize = 16;

pub struct SyntheticWorkspace {
    root: tempfile::TempDir,
    discovery_roots: Vec<PathBuf>,
}

impl SyntheticWorkspace {
    pub fn create() -> Result<Self> {
        let root = tempfile::tempdir().context("Failed to create synthetic workspace")?;
        let mut discovery_roots = Vec::with_capacity(SYNTHETIC_MEMBER_COUNT + 1);
        discovery_roots.push(root.path().to_path_buf());

        for member_index in 0..SYNTHETIC_MEMBER_COUNT {
            let member_root = root
                .path()
                .join("packages")
                .join(format!("provider-{member_index:03}"));
            fs_err::create_dir_all(&member_root).with_context(|| {
                format!(
                    "Failed to create synthetic workspace member `{}`",
                    member_root.display()
                )
            })?;
            fs_err::write(
                member_root.join("pyproject.toml"),
                member_pyproject(member_index),
            )
            .with_context(|| {
                format!(
                    "Failed to write synthetic workspace member `{}`",
                    member_root.display()
                )
            })?;
            discovery_roots.push(member_root);
        }

        fs_err::write(root.path().join("pyproject.toml"), root_pyproject())
            .context("Failed to write synthetic workspace root")?;

        Ok(Self {
            root,
            discovery_roots,
        })
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }

    pub fn discovery_roots(&self) -> &[PathBuf] {
        &self.discovery_roots
    }
}

fn root_pyproject() -> String {
    let mut pyproject = String::from(indoc! {r#"
        [build-system]
        requires = ["uv_build>=0.8.0,<0.9.0"]
        build-backend = "uv_build"

        [project]
        name = "synthetic-large-workspace"
        version = "0.0.0"
        description = "A generated workspace used to benchmark repeated workspace discovery"
        requires-python = ">=3.11"
        license = "MIT"
        keywords = ["benchmark", "generated", "monorepo", "workspace"]
        classifiers = [
            "Development Status :: 4 - Beta",
            "Programming Language :: Python :: 3",
            "Programming Language :: Python :: 3.11",
            "Programming Language :: Python :: 3.12",
            "Programming Language :: Python :: 3.13",
            "Typing :: Typed",
        ]
        dependencies = [
    "#});

    for member_index in 0..SYNTHETIC_MEMBER_COUNT {
        writeln!(
            pyproject,
            "    \"synthetic-provider-{member_index:03}>=0.0.0\","
        )
        .unwrap();
    }
    pyproject.push_str("]\n\n[project.optional-dependencies]\n");

    for group_index in 0..OPTIONAL_DEPENDENCY_GROUP_COUNT {
        writeln!(pyproject, "provider-set-{group_index:03} = [").unwrap();
        for offset in 0..5 {
            let member_index = (group_index * 7 + offset * 11) % SYNTHETIC_MEMBER_COUNT;
            writeln!(
                pyproject,
                "    \"synthetic-provider-{member_index:03}>=0.0.0; python_version >= '3.11'\","
            )
            .unwrap();
        }
        pyproject.push_str("]\n");
    }

    pyproject.push_str("\n[dependency-groups]\n");
    for group_index in 0..DEPENDENCY_GROUP_COUNT {
        writeln!(pyproject, "development-set-{group_index:03} = [").unwrap();
        for offset in 0..4 {
            let member_index = (group_index * 13 + offset * 17) % SYNTHETIC_MEMBER_COUNT;
            writeln!(
                pyproject,
                "    \"synthetic-provider-{member_index:03}>=0.0.0\","
            )
            .unwrap();
        }
        pyproject.push_str("]\n");
    }

    pyproject.push_str("\n[tool.uv.workspace]\nmembers = [\n");
    for member_index in 0..SYNTHETIC_MEMBER_COUNT {
        writeln!(pyproject, "    \"packages/provider-{member_index:03}\",").unwrap();
    }
    pyproject.push_str("]\n\n[tool.uv.sources]\n");
    for member_index in 0..SYNTHETIC_MEMBER_COUNT {
        writeln!(
            pyproject,
            "synthetic-provider-{member_index:03} = {{ workspace = true }}"
        )
        .unwrap();
    }

    pyproject.push_str(indoc! {r#"

        [project.urls]
        Documentation = "https://example.invalid/docs"
        Repository = "https://example.invalid/repository"
        Issues = "https://example.invalid/issues"

        [project.scripts]
        synthetic-admin = "synthetic_workspace.cli:main"
        synthetic-report = "synthetic_workspace.reports:main"

        [project.entry-points."synthetic.hooks"]
        initialize = "synthetic_workspace.hooks:initialize"
        validate = "synthetic_workspace.hooks:validate"
        finalize = "synthetic_workspace.hooks:finalize"
    "#});

    for table_index in 0..UNUSED_ROOT_TABLE_COUNT {
        let enabled = table_index % 2 == 0;
        let owner = table_index % 12;
        writeln!(
            pyproject,
            "{}",
            formatdoc! {r#"
                [tool.synthetic.generated.root-section-{table_index:03}]
                enabled = {enabled}
                label = "Generated root metadata section {table_index:03}"
                owner = "workspace-team-{owner:02}"
                tags = ["benchmark", "root", "section-{table_index:03}"]
                include = ["packages/provider-*", "plugins/**/*.py", "tests/**/*.py"]
                exclude = ["build/**", "dist/**", ".cache/**"]
                settings = {{ retries = 3, timeout = 30, strict = false }}
            "#}
        )
        .unwrap();
    }

    pyproject
}

fn member_pyproject(member_index: usize) -> String {
    let mut pyproject = formatdoc! {r#"
        [project]
        name = "synthetic-provider-{member_index:03}"
        version = "0.0.0"
        description = "Generated provider package {member_index:03} for workspace discovery benchmarks"
        requires-python = ">=3.11"
        license = "MIT"
        keywords = ["generated", "provider", "workspace"]
        classifiers = [
            "Programming Language :: Python :: 3",
            "Programming Language :: Python :: 3.11",
            "Typing :: Typed",
        ]
        dependencies = [
    "#};

    for offset in 1..=4 {
        if let Some(dependency_index) = member_index.checked_sub(offset) {
            writeln!(
                pyproject,
                "    \"synthetic-provider-{dependency_index:03}>=0.0.0\","
            )
            .unwrap();
        }
    }
    pyproject.push_str(indoc! {r#"
        ]

        [project.optional-dependencies]
        diagnostics = ["rich>=13", "typing-extensions>=4.10"]
        integrations = ["httpx>=0.27", "platformdirs>=4"]

        [dependency-groups]
        test = ["pytest>=8", "pytest-asyncio>=0.24"]
        lint = ["ruff>=0.11", "mypy>=1.15"]

        [project.urls]
        Documentation = "https://example.invalid/providers"
        Source = "https://example.invalid/repository"

        [project.entry-points."synthetic.providers"]
    "#});
    writeln!(
        pyproject,
        "provider-{member_index:03} = \"synthetic_provider_{member_index:03}.plugin:Provider\""
    )
    .unwrap();

    for table_index in 0..UNUSED_MEMBER_TABLE_COUNT {
        let enabled = table_index % 3 != 0;
        let priority = table_index % 5;
        writeln!(
            pyproject,
            "{}",
            formatdoc! {r#"
                [tool.synthetic.generated.member-section-{table_index:02}]
                member = {member_index}
                label = "Provider {member_index:03} metadata section {table_index:02}"
                enabled = {enabled}
                capabilities = ["discover", "validate", "report", "archive"]
                paths = ["src/**/*.py", "tests/**/*.py", "resources/**/*"]
                metadata = {{ priority = {priority}, retries = 2, experimental = true }}
            "#}
        )
        .unwrap();
    }

    pyproject
}
