#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ResolutionMode {
    /// Resolve the highest compatible version of each package.
    #[default]
    Highest,
    /// Resolve the lowest compatible version of each package.
    Lowest,
    /// Resolve the lowest compatible version of any direct dependencies, and the highest
    /// compatible version of any transitive dependencies.
    LowestDirect,
}

impl std::fmt::Display for ResolutionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Highest => write!(f, "highest"),
            Self::Lowest => write!(f, "lowest"),
            Self::LowestDirect => write!(f, "lowest-direct"),
        }
    }
}
