pub use workspace::{
    check_nested_workspaces, DiscoveryOptions, InstallTarget, MemberDiscovery, ProjectWorkspace,
    VirtualProject, Workspace, WorkspaceError, WorkspaceMember,
};

pub mod dependency_groups;
pub mod pyproject;
pub mod pyproject_mut;
mod workspace;
