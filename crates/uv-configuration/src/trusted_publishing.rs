use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum TrustedPublishing {
    /// Try trusted publishing when we're already in GitHub Actions, continue if that fails.
    #[default]
    Automatic,
    // Force trusted publishing.
    Always,
    // Never try to get a trusted publishing token.
    Never,
}
