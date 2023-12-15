#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum PreReleaseMode {
    /// Disallow all pre-release versions.
    Disallow,

    /// Allow all pre-release versions.
    Allow,

    /// Allow pre-release versions for packages with explicit pre-release markers in their version
    /// requirements, and packages without _any_ stable versions.
    #[default]
    IfRequested,
}
