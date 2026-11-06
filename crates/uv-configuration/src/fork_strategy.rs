#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ForkStrategy {
    /// Optimize for selecting the fewest number of versions for each package. Older versions may
    /// be preferred if they are compatible with a wider range of supported Python versions or
    /// platforms.
    Fewest,
    /// Prefer the latest compatible version for each supported Python version. Platforms can
    /// select a newer release with a source distribution or a `py3-none-any` wheel when
    /// an upper bound applies only to some of them.
    #[default]
    RequiresPython,
}

impl std::fmt::Display for ForkStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fewest => write!(f, "fewest"),
            Self::RequiresPython => write!(f, "requires-python"),
        }
    }
}
