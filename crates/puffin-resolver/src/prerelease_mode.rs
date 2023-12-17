#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum PreReleaseMode {
    /// Disallow all pre-release versions.
    Disallow,

    /// Allow all pre-release versions.
    Allow,

    /// Allow pre-release versions only if no stable version is available given the current
    /// constraints.
    #[default]
    IfNecessary,
}
