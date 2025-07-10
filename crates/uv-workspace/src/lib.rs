pub use workspace::{
    DiscoveryOptions, MemberDiscovery, ProjectWorkspace, RequiresPythonSources, VirtualProject,
    Workspace, WorkspaceCache, WorkspaceError, WorkspaceMember,
};

pub mod dependency_groups;
pub mod pyproject;
pub mod pyproject_mut;
mod workspace;
