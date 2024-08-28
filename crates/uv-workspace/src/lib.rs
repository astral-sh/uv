pub use workspace::{
    check_nested_workspaces, DiscoveryOptions, MemberDiscovery, ProjectWorkspace, VirtualProject,
    Workspace, WorkspaceError, WorkspaceMember,
};

pub mod pyproject;
pub mod pyproject_mut;
mod workspace;
