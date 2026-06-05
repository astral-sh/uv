use anyhow::Result;
use uv_bench::synthetic_workspace::{SYNTHETIC_MEMBER_COUNT, SyntheticWorkspace};
use uv_cache::Cache;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

#[tokio::test]
async fn synthetic_workspace_has_expected_scale() -> Result<()> {
    let fixture = SyntheticWorkspace::create()?;
    let root_pyproject = fs_err::read_to_string(fixture.root().join("pyproject.toml"))?;
    let member_pyproject =
        fs_err::read_to_string(fixture.root().join("packages/provider-000/pyproject.toml"))?;
    let cache = Cache::from_path(fixture.root().join(".uv-cache"));
    let workspace = Workspace::discover(
        fixture.root(),
        &DiscoveryOptions::default(),
        &cache,
        &WorkspaceCache::default(),
    )
    .await?;

    assert!(root_pyproject.lines().count() > 2_500);
    assert!(root_pyproject.len() > 100_000);
    assert!(member_pyproject.lines().count() > 150);
    assert!(member_pyproject.len() > 5_000);
    assert_eq!(fixture.discovery_roots().len(), SYNTHETIC_MEMBER_COUNT + 1);
    assert_eq!(workspace.packages().len(), SYNTHETIC_MEMBER_COUNT + 1);
    assert_eq!(workspace.sources().len(), SYNTHETIC_MEMBER_COUNT);

    Ok(())
}
