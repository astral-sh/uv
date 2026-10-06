use anyhow::Result;
use uv_cache::Cache;
use uv_workspace::pyproject::PyProjectToml;
use uv_workspace::{DiscoveryOptions, VirtualProject, Workspace, WorkspaceCache};

#[tokio::test]
async fn cached_project_workspace_can_be_exclusively_updated() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    let path = root.join("pyproject.toml");
    let document = "[project]\nname = 'root'\nversion = '1.0'\n[tool.uv.workspace]\nmembers = []\n";
    fs_err::write(&path, document)?;
    let cache = Cache::from_path(root.join("cache"));
    let workspaces = WorkspaceCache::default();
    drop(Workspace::discover(root, &DiscoveryOptions::default(), &cache, &workspaces).await?);
    let project =
        VirtualProject::discover(root, &DiscoveryOptions::default(), &cache, &workspaces).await?;
    let parsed = PyProjectToml::from_string(document.to_owned(), &path)?;
    assert!(project.update_member(parsed, &workspaces)?.is_some());
    Ok(())
}
