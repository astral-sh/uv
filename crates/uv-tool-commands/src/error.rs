use uv_command_support::UvError;
use uv_environment_operations::EnvironmentError;

/// A failure while finding or creating an environment for a tool invocation.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ToolError {
    #[error(transparent)]
    Environment(#[from] EnvironmentError),
    #[error(transparent)]
    Tool(Box<uv_tool::Error>),
    #[error(transparent)]
    Requirements(#[from] uv_requirements::Error),
    #[error(transparent)]
    Python(Box<uv_python_discovery::Error>),
    #[error(transparent)]
    Resolve(Box<uv_resolve_operations::Error>),
    #[error(transparent)]
    ClientBuild(#[from] uv_client::ClientBuildError),
    #[error(transparent)]
    Client(#[from] uv_client::Error),
    #[error(transparent)]
    Tags(#[from] uv_platform_tags::TagsError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl From<uv_tool::Error> for ToolError {
    fn from(error: uv_tool::Error) -> Self {
        Self::Tool(Box::new(error))
    }
}

impl From<uv_python_discovery::Error> for ToolError {
    fn from(error: uv_python_discovery::Error) -> Self {
        Self::Python(Box::new(error))
    }
}

impl From<uv_resolve_operations::Error> for ToolError {
    fn from(error: uv_resolve_operations::Error) -> Self {
        Self::Resolve(Box::new(error))
    }
}

impl From<ToolError> for UvError {
    fn from(error: ToolError) -> Self {
        match error {
            ToolError::Environment(error) => Self::from(error),
            ToolError::Resolve(error) => Self::from(*error),
            ToolError::Requirements(error) => {
                Self::from(uv_resolve_operations::Error::Requirements(error))
            }
            ToolError::Python(error) => Self::unexpected((*error).into()),
            ToolError::Client(error) => Self::unexpected(error.into()),
            error @ (ToolError::Tool(_)
            | ToolError::ClientBuild(_)
            | ToolError::Tags(_)
            | ToolError::Io(_)
            | ToolError::Anyhow(_)) => Self::unexpected(error.into()),
        }
    }
}

/// A failure while preparing or validating an existing tool lockfile.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ToolLockError {
    #[error(transparent)]
    Validation(#[from] uv_lock_operations::LockValidationError),
    #[error(transparent)]
    ClientBuild(#[from] uv_client::ClientBuildError),
    #[error(transparent)]
    FlatIndex(Box<uv_client::FlatIndexError>),
    #[error(transparent)]
    HashStrategy(#[from] uv_types::HashStrategyError),
}

impl From<uv_client::FlatIndexError> for ToolLockError {
    fn from(error: uv_client::FlatIndexError) -> Self {
        Self::FlatIndex(Box::new(error))
    }
}
