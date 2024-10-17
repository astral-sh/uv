/// The origin of a piece of configuration.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum Origin {
    /// The setting was provided via the CLI.
    Cli,
    /// The setting was provided via a user-level configuration file.
    User,
    /// The setting was provided via a project-level configuration file.
    Project,
    /// The setting was provided via a `requirements.txt` file.
    RequirementsTxt,
}
