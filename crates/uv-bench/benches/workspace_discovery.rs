use std::hint::black_box;
use std::path::{Path, PathBuf};

use criterion::{Criterion, criterion_group, criterion_main, measurement::WallTime};

use uv_cache::Cache;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

/// Mirroring the airflow workspace size at time of writing.
const MEMBER_COUNT: usize = 127;
const OPTIONAL_DEPENDENCY_GROUP_COUNT: usize = 122;
const DEPENDENCY_GROUP_COUNT: usize = 64;
const UNUSED_ROOT_TABLE_COUNT: usize = 128;
const UNUSED_MEMBER_TABLE_COUNT: usize = 16;

fn provider_name(member_index: usize) -> String {
    format!("workspace-discovery-provider-{member_index:03}")
}

/// Create a synthetic workspace with a root and [`MEMBER_COUNT`] members, returning the
/// directories to run discovery from.
fn create_workspace(root: &Path) -> Vec<PathBuf> {
    let mut discovery_roots = Vec::with_capacity(MEMBER_COUNT + 1);
    discovery_roots.push(root.to_path_buf());

    for member_index in 0..MEMBER_COUNT {
        let member_root = root
            .join("packages")
            .join(format!("provider-{member_index:03}"));
        fs_err::create_dir_all(&member_root).expect("Failed to create workspace member directory");
        fs_err::write(
            member_root.join("pyproject.toml"),
            member_pyproject(member_index),
        )
        .expect("Failed to write workspace member pyproject.toml");
        discovery_roots.push(member_root);
    }

    fs_err::write(root.join("pyproject.toml"), root_pyproject())
        .expect("Failed to write workspace root pyproject.toml");

    discovery_roots
}

fn root_pyproject() -> String {
    let mut pyproject = toml::toml! {
        [build-system]
        requires = ["uv_build>=0.11.0,<10000"]
        build-backend = "uv_build"

        [project]
        name = "workspace-discovery-benchmark"
        version = "0.0.0"
        description = "A generated workspace used to benchmark repeated workspace discovery"
        requires-python = ">=3.12"
        license = "MIT"
        keywords = ["benchmark", "generated", "monorepo", "workspace"]
        classifiers = [
            "Development Status :: 4 - Beta",
            "Programming Language :: Python :: 3",
            "Programming Language :: Python :: 3.12",
            "Programming Language :: Python :: 3.13",
            "Programming Language :: Python :: 3.14",
            "Programming Language :: Python :: 3.15",
            "Typing :: Typed",
        ]
        dependencies = []

        [project.optional-dependencies]

        [project.urls]
        Documentation = "https://example.com/docs"
        Repository = "https://example.com/repository"
        Issues = "https://example.com/issues"

        [project.scripts]
        workspace-discovery-admin = "workspace_discovery.cli:main"
        workspace-discovery-report = "workspace_discovery.reports:main"

        [project.entry-points."workspace_discovery.hooks"]
        initialize = "workspace_discovery.hooks:initialize"
        validate = "workspace_discovery.hooks:validate"
        finalize = "workspace_discovery.hooks:finalize"

        [dependency-groups]

        [tool.uv.workspace]
        members = ["packages/*"]

        [tool.uv.sources]

        [tool.workspace_discovery.generated]
    };

    let dependencies = pyproject["project"]["dependencies"].as_array_mut().unwrap();
    for member_index in 0..MEMBER_COUNT {
        dependencies.push(format!("{}>=0.0.0", provider_name(member_index)).into());
    }

    let optional_dependencies = pyproject["project"]["optional-dependencies"]
        .as_table_mut()
        .unwrap();
    for group_index in 0..OPTIONAL_DEPENDENCY_GROUP_COUNT {
        let mut dependencies = Vec::with_capacity(5);
        for offset in 0..5 {
            let member_index = (group_index * 7 + offset * 11) % MEMBER_COUNT;
            dependencies.push(
                format!(
                    "{}; python_version >= '3.12'",
                    format!("{}>=0.0.0", provider_name(member_index))
                )
                .into(),
            );
        }
        optional_dependencies.insert(
            format!("provider-set-{group_index:03}"),
            toml::Value::Array(dependencies),
        );
    }

    let dependency_groups = pyproject["dependency-groups"].as_table_mut().unwrap();
    for group_index in 0..DEPENDENCY_GROUP_COUNT {
        let mut dependencies = Vec::with_capacity(4);
        for offset in 0..4 {
            let member_index = (group_index * 13 + offset * 17) % MEMBER_COUNT;
            dependencies.push(format!("{}>=0.0.0", provider_name(member_index)).into());
        }
        dependency_groups.insert(
            format!("development-set-{group_index:03}"),
            toml::Value::Array(dependencies),
        );
    }

    let sources = pyproject["tool"]["uv"]["sources"].as_table_mut().unwrap();
    for member_index in 0..MEMBER_COUNT {
        sources.insert(
            provider_name(member_index),
            toml::Value::Table(toml::toml! {
                workspace = true
            }),
        );
    }

    // Generate some unrelated work for the toml parser, mimicking real tool configuration.
    let generated = pyproject["tool"]["workspace_discovery"]["generated"]
        .as_table_mut()
        .unwrap();
    for table_index in 0..UNUSED_ROOT_TABLE_COUNT {
        let enabled = table_index % 2 == 0;
        let owner = table_index % 12;
        generated.insert(
            format!("root-section-{table_index:03}"),
            toml::Value::Table(toml::toml! {
                enabled = (enabled)
                label = (format!("Generated root metadata section {table_index:03}"))
                owner = (format!("workspace-team-{owner:02}"))
                tags = ["benchmark", "root", (format!("section-{table_index:03}"))]
                include = ["packages/provider-*", "plugins/**/*.py", "tests/**/*.py"]
                exclude = ["build/**", "dist/**", ".cache/**"]
                settings = { retries = 3, timeout = 30, strict = false }
            }),
        );
    }

    toml::to_string_pretty(&pyproject).unwrap()
}

fn member_pyproject(member_index: usize) -> String {
    let mut dependencies = Vec::with_capacity(5);
    for offset in 1..=4 {
        if let Some(dependency_index) = member_index.checked_sub(offset) {
            dependencies.push(format!("{}>=0.0.0", provider_name(dependency_index)));
        }
        dependencies.push("anyio>=4,<5".to_string());
    }

    let mut pyproject = toml::toml! {
        [project]
        name = (provider_name(member_index))
        version = "0.0.0"
        description = (format!("Generated provider package {member_index:03} for workspace discovery benchmarks"))
        requires-python = ">=3.11"
        license = "MIT"
        keywords = ["generated", "provider", "workspace"]
        classifiers = [
            "Programming Language :: Python :: 3",
            "Programming Language :: Python :: 3.12",
            "Programming Language :: Python :: 3.13",
            "Programming Language :: Python :: 3.14",
            "Programming Language :: Python :: 3.15",
            "Typing :: Typed",
        ]
        dependencies = (dependencies)

        [project.optional-dependencies]
        diagnostics = ["rich>=13", "typing-extensions>=4.10"]
        integrations = ["httpx>=0.27", "platformdirs>=4"]

        [dependency-groups]
        test = ["pytest>=8", "pytest-asyncio>=0.24"]
        lint = ["tqdm", "mypy>=1.15"]

        [project.urls]
        Documentation = "https://example.com/providers"
        Source = "https://example.com/repository"

        [project.entry-points."workspace_discovery.providers"]

        [tool.workspace_discovery.generated]
    };

    // Add some unrelated work for the toml parser, mimicking real tool configuration.
    let entry_points = pyproject["project"]["entry-points"]["workspace_discovery.providers"]
        .as_table_mut()
        .unwrap();
    entry_points.insert(
        format!("provider-{member_index:03}"),
        format!("workspace_discovery_provider_{member_index:03}.plugin:Provider").into(),
    );
    let generated = pyproject["tool"]["workspace_discovery"]["generated"]
        .as_table_mut()
        .unwrap();
    for table_index in 0..UNUSED_MEMBER_TABLE_COUNT {
        let enabled = table_index % 3 != 0;
        let priority = table_index % 5;
        generated.insert(
            format!("member-section-{table_index:02}"),
            toml::Value::Table(toml::toml! {
                member = (member_index)
                label = (format!("Provider {member_index:03} metadata section {table_index:02}"))
                enabled = (enabled)
                capabilities = ["discover", "validate", "report", "archive"]
                paths = ["src/**/*.py", "tests/**/*.py", "resources/**/*"]
                metadata = { priority = (priority), retries = 2, experimental = true }
            }),
        );
    }

    toml::to_string_pretty(&pyproject).unwrap()
}

fn discover_workspace_from_all_members(c: &mut Criterion<WallTime>) {
    let dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let discovery_roots = create_workspace(dir.path());
    let cache = Cache::from_path(dir.path().join(".uv-cache"));
    let options = DiscoveryOptions::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    c.bench_function("discover_workspace_from_all_members", |b| {
        b.iter(|| {
            let workspace_cache = WorkspaceCache::default();
            for root in &discovery_roots {
                let workspace = runtime
                    .block_on(Workspace::discover(
                        root,
                        &options,
                        &cache,
                        &workspace_cache,
                    ))
                    .expect("Failed to discover benchmark workspace");
                black_box(workspace);
            }
        });
    });
}

criterion_group!(workspace_discovery, discover_workspace_from_all_members);
criterion_main!(workspace_discovery);
