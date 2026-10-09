use std::fmt::Display;

/// The user-facing command used to invoke a tool run.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ToolRunCommand {
    /// via the `uvx` alias
    Uvx,
    /// via `uv tool run`
    ToolRun,
}

impl Display for ToolRunCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Uvx => write!(f, "uvx"),
            Self::ToolRun => write!(f, "uv tool run"),
        }
    }
}
