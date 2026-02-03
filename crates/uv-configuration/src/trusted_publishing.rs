use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum TrustedPublishing {
    /// Attempt trusted publishing when we're in a supported environment, continue if that fails.
    ///
    /// Supported environments include GitHub Actions and GitLab CI/CD.
    #[default]
    Automatic,
    // Force trusted publishing.
    Always,
    // Never try to get a trusted publishing token.
    Never,
}
