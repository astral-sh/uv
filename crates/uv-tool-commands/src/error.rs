use uv_command_support::UvError;
use uv_environment_operations::{EnvironmentError, OperationsError};

/// A failure while finding or creating an environment for a tool invocation.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ToolError {
    #[error(transparent)]
    Environment(#[from] EnvironmentError),
    #[error(transparent)]
    Tool(Box<uv_tool::Error>),
}

impl From<uv_tool::Error> for ToolError {
    fn from(error: uv_tool::Error) -> Self {
        Self::Tool(Box::new(error))
    }
}

impl From<uv_requirements::Error> for ToolError {
    fn from(error: uv_requirements::Error) -> Self {
        Self::Environment(error.into())
    }
}

impl From<uv_python::Error> for ToolError {
    fn from(error: uv_python::Error) -> Self {
        Self::Environment(error.into())
    }
}

impl From<uv_resolve_operations::Error> for ToolError {
    fn from(error: uv_resolve_operations::Error) -> Self {
        Self::Environment(error.into())
    }
}

impl From<uv_client::ClientBuildError> for ToolError {
    fn from(error: uv_client::ClientBuildError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<uv_client::Error> for ToolError {
    fn from(error: uv_client::Error) -> Self {
        Self::Environment(error.into())
    }
}

impl From<OperationsError> for ToolError {
    fn from(error: OperationsError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<anyhow::Error> for ToolError {
    fn from(error: anyhow::Error) -> Self {
        Self::Environment(error.into())
    }
}

impl From<ToolError> for UvError {
    fn from(error: ToolError) -> Self {
        match error {
            ToolError::Environment(error) => Self::from(error),
            error @ ToolError::Tool(_) => Self::unexpected(error.into()),
        }
    }
}
