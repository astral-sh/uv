#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum BuildDependencyStrategy {
    /// Use the latest compatible version of each build dependency.
    #[default]
    Latest,
    /// Prefer the versions pinned in the lockfile, if available.
    PreferLocked,
}

impl std::fmt::Display for BuildDependencyStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Latest => write!(f, "latest"),
            Self::PreferLocked => write!(f, "prefer-locked"),
        }
    }
}
