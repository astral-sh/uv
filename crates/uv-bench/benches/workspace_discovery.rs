//! Benchmarks over a large synthetic workspace with many non-trivial to parse `pyproject.toml`
//! files.

// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::hint::black_box;
use std::path::{Path, PathBuf};

use clap::Parser;
use criterion::{Criterion, criterion_group, criterion_main, measurement::WallTime};

use uv::GlobalInitialization;
use uv::commands::ExitStatus;
use uv_cache::Cache;
use uv_cli::Cli;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

const EXCLUDE_NEWER: &str = "2024-08-08";

/// Mirroring the airflow workspace size at time of writing.
const MEMBER_COUNT: usize = 127;
const OPTIONAL_DEPENDENCY_GROUP_COUNT: usize = 122;
const DEPENDENCY_GROUP_COUNT: usize = 64;
const UNUSED_ROOT_TABLE_COUNT: usize = 128;
const UNUSED_MEMBER_TABLE_COUNT: usize = 16;

fn provider_name(member_index: usize) -> String {
    format!("workspace-discovery-provider-{member_index:03}")
}

fn provider_requirement(member_index: usize) -> String {
    format!("{}>=0.0.0", provider_name(member_index))
}

/// Create a synthetic workspace with a root and many members, returning the directories to run
/// discovery from.
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
    let dependencies: Vec<String> = (0..MEMBER_COUNT).map(provider_requirement).collect();

    let optional_dependencies: toml::Table = (0..OPTIONAL_DEPENDENCY_GROUP_COUNT)
        .map(|group_index| {
            let dependencies: Vec<String> = (0..5)
                .map(|offset| {
                    let member_index = (group_index * 7 + offset * 11) % MEMBER_COUNT;
                    format!(
                        "{}; python_version >= '3.12'",
                        provider_requirement(member_index)
                    )
                })
                .collect();
            (
                format!("provider-set-{group_index:03}"),
                toml::Value::from(dependencies),
            )
        })
        .collect();

    let dependency_groups: toml::Table = (0..DEPENDENCY_GROUP_COUNT)
        .map(|group_index| {
            let dependencies: Vec<String> = (0..4)
                .map(|offset| {
                    let member_index = (group_index * 13 + offset * 17) % MEMBER_COUNT;
                    provider_requirement(member_index)
                })
                .collect();
            (
                format!("development-set-{group_index:03}"),
                toml::Value::from(dependencies),
            )
        })
        .collect();

    let sources: toml::Table = (0..MEMBER_COUNT)
        .map(|member_index| {
            (
                provider_name(member_index),
                toml::Value::from(toml::toml! { workspace = true }),
            )
        })
        .collect();

    // Generate some unrelated work for the toml parser, mimicking real tool configuration.
    let generated: toml::Table = (0..UNUSED_ROOT_TABLE_COUNT)
        .map(|table_index| {
            let enabled = table_index % 2 == 0;
            let owner = table_index % 12;
            (
                format!("root-section-{table_index:03}"),
                toml::toml! {
                    enabled = (enabled)
                    label = (format!("Generated root metadata section {table_index:03}"))
                    owner = (format!("workspace-team-{owner:02}"))
                    tags = ["benchmark", "root", (format!("section-{table_index:03}"))]
                    include = ["packages/provider-*", "plugins/**/*.py", "tests/**/*.py"]
                    exclude = ["build/**", "dist/**", ".cache/**"]
                    settings = { retries = 3, timeout = 30, strict = false }
                }
                .into(),
            )
        })
        .collect();

    let pyproject = toml::toml! {
        dependency-groups = (dependency_groups)

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
        dependencies = (dependencies)
        optional-dependencies = (optional_dependencies)

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

        [tool.uv]
        package = false
        sources = (sources)

        [tool.uv.workspace]
        members = ["packages/*"]

        [tool.linter]
        generated = (generated)
    };

    toml::to_string_pretty(&pyproject).expect("Failed to serialize root pyproject.toml")
}

fn member_pyproject(member_index: usize) -> String {
    let mut dependencies: Vec<String> = (1..=4)
        .filter_map(|offset| member_index.checked_sub(offset))
        .map(provider_requirement)
        .collect();
    // That's a real, usable PyPI dependency.
    dependencies.push("sniffio>=1,<2".to_string());

    let entry_points: toml::Table = [(
        format!("provider-{member_index:03}"),
        format!("workspace_discovery_provider_{member_index:03}.plugin:Provider").into(),
    )]
    .into_iter()
    .collect();

    // Add some unrelated work for the toml parser, mimicking real tool configuration.
    let generated: toml::Table = (0..UNUSED_MEMBER_TABLE_COUNT)
        .map(|table_index| {
            let enabled = table_index % 3 != 0;
            let priority = table_index % 5;
            (
                format!("member-section-{table_index:02}"),
                toml::toml! {
                    member = (member_index)
                    label = (format!("Provider {member_index:03} metadata section {table_index:02}"))
                    enabled = (enabled)
                    capabilities = ["discover", "validate", "report", "archive"]
                    paths = ["src/**/*.py", "tests/**/*.py", "resources/**/*"]
                    metadata = { priority = (priority), retries = 2, experimental = true }
                }
                .into(),
            )
        })
        .collect();

    let pyproject = toml::toml! {
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
        test = ["pytest>=8", "pytest-asyncio>=0.23,<0.24"]
        lint = ["tqdm", "anyio>=4,<5"]

        [project.urls]
        Documentation = "https://example.com/providers"
        Source = "https://example.com/repository"

        [project.entry-points]
        "workspace_discovery.providers" = (entry_points)

        [tool.uv]
        package = false

        [tool.linter]
        generated = (generated)
    };

    toml::to_string_pretty(&pyproject).expect("Failed to serialize member pyproject.toml")
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

fn run_python_version_synthetic_workspace(c: &mut Criterion<WallTime>) {
    let workspace_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    create_workspace(workspace_dir.path());

    let cache_dir = workspace_dir.path().join(".uv-cache");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    let workspace_dir = workspace_dir.path().to_string_lossy().to_string();
    let cache_dir = cache_dir.to_string_lossy().to_string();

    // Prime cache with PyPI packages.
    run_cli(
        &runtime,
        run_python_version_cli(&workspace_dir, &cache_dir, false),
        GlobalInitialization::Initialize,
        "Failed to warm synthetic workspace run benchmark",
    );
    // Warm cache reading
    run_cli(
        &runtime,
        run_python_version_cli(&workspace_dir, &cache_dir, true),
        GlobalInitialization::Reuse,
        "Failed to warm offline synthetic workspace run benchmark",
    );

    c.bench_function("run_python_version_synthetic_workspace", |b| {
        b.iter(|| {
            let cli =
                run_python_version_cli(black_box(&workspace_dir), black_box(&cache_dir), true);
            run_cli(
                &runtime,
                cli,
                GlobalInitialization::Reuse,
                "Failed to run synthetic workspace benchmark",
            );
        });
    });
}

fn run_python_version_cli(workspace_dir: &str, cache_dir: &str, offline: bool) -> Cli {
    let mut args = vec![
        "uv",
        "run",
        "--directory",
        workspace_dir,
        "--cache-dir",
        cache_dir,
        "--exclude-newer",
        EXCLUDE_NEWER,
        "--quiet",
    ];
    if offline {
        args.push("--offline");
    }
    args.extend(["python", "-V"]);

    Cli::try_parse_from(args).expect("Failed to parse synthetic workspace run benchmark arguments")
}

fn run_cli(
    runtime: &tokio::runtime::Runtime,
    cli: Cli,
    global_initialization: GlobalInitialization,
    message: &str,
) {
    let status = runtime
        .block_on(uv::run(cli, global_initialization))
        .expect(message);
    assert!(
        matches!(status, ExitStatus::Success | ExitStatus::External(0)),
        "{message}"
    );
}

criterion_group!(
    workspace_discovery,
    discover_workspace_from_all_members,
    run_python_version_synthetic_workspace
);
criterion_main!(workspace_discovery);
