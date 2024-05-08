use serde::{Deserialize, Deserializer, Serialize};
use std::path::PathBuf;
use uv_fs::Simplified;

/// Source of a dependency, e.g., a `-r requirements.txt` file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum SourceAnnotation {
    /// A `pyproject.toml` file.
    PyProject {
        path: PathBuf,
        project_name: Option<String>,
    },
    /// A `-c constraints.txt` file.
    Constraint(PathBuf),
    /// An `--override overrides.txt` file.
    Override(PathBuf),
    /// A `-r requirements.txt` file.
    Requirement(PathBuf),
}

impl<'de> Deserialize<'de> for SourceAnnotation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(SourceAnnotation::Requirement(PathBuf::from(s)))
    }
}

impl std::fmt::Display for SourceAnnotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Requirement(path) => {
                write!(f, "-r {}", path.user_display())
            }
            Self::Constraint(path) => {
                write!(f, "-c {}", path.user_display())
            }
            Self::Override(path) => {
                write!(f, "--override {}", path.user_display())
            }
            Self::PyProject { path, project_name } => {
                if let Some(project_name) = project_name {
                    write!(f, "{} ({})", project_name, path.user_display())
                } else {
                    write!(f, "{}", path.user_display())
                }
            }
        }
    }
}
