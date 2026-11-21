use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

/// The tool that `uv check` runs.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum CheckTool {
    /// Use ty.
    #[default]
    Ty,
    /// Use Pyrefly.
    Pyrefly,
}

impl CheckTool {
    /// The name of the tool's package and executable.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ty => "ty",
            Self::Pyrefly => "pyrefly",
        }
    }
}

impl Display for CheckTool {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
