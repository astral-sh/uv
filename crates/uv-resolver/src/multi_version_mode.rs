#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MultiVersionMode {
    /// Resolve the highest compatible version of each package.
    #[default]
    Fewest,
    /// Resolve the lowest compatible version of each package.
    Latest,
}

impl std::fmt::Display for MultiVersionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fewest => write!(f, "fewest"),
            Self::Latest => write!(f, "latest"),
        }
    }
}
