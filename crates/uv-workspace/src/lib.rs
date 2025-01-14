pub use workspace::{
    DiscoveryOptions, MemberDiscovery, ProjectWorkspace, VirtualProject, Workspace, WorkspaceError,
    WorkspaceMember,
};

pub mod dependency_groups;
pub mod pyproject;
pub mod pyproject_mut;
mod workspace;
