/// The origin of a project-level configuration file.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub enum ProjectOrigin {
    /// The setting was provided via a `pyproject.toml` file.
    PyprojectToml,
    /// The setting was provided via a `uv.toml` file adjacent to a `pyproject.toml`.
    UvToml,
}

/// The origin of a piece of configuration.
#[derive(Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub enum Origin {
    /// The setting was provided via the CLI.
    Cli,
    /// The setting was provided via a user-level configuration file.
    User,
    /// The setting was provided via a system-level configuration file.
    System,
    /// The setting was provided via a project-level configuration file.
    Project(ProjectOrigin),
    /// The setting was provided via a `requirements.txt` file.
    RequirementsTxt,
}

impl std::fmt::Debug for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cli => f.write_str("Cli"),
            Self::User => f.write_str("User"),
            Self::System => f.write_str("System"),
            Self::Project(_) => f.write_str("Project"),
            Self::RequirementsTxt => f.write_str("RequirementsTxt"),
        }
    }
}
