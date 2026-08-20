/// The artifacts to retain when compiling a dependency resolution.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ArtifactPolicy {
    /// Include every available wheel and source distribution.
    #[default]
    All,
    /// Include wheels, but never include source distributions.
    WheelsOnly,
    /// Include source distributions when wheels do not cover the target environments.
    NecessarySdists,
    /// Include source distributions only for package versions without any wheels.
    SourceOnly,
}
