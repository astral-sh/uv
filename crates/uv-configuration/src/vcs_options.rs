use serde::Deserialize;
use std::str::FromStr;

/// The version control system to use.
#[derive(Clone, Copy, Debug, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum VersionControl {
    /// Use Git for version control.
    #[default]
    Git,

    /// Do not use version control.
    None,
}

impl FromStr for VersionControl {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "git" => Ok(VersionControl::Git),
            "none" => Ok(VersionControl::None),
            other => Err(format!("unknown vcs specification: `{other}`")),
        }
    }
}

impl std::fmt::Display for VersionControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionControl::Git => write!(f, "git"),
            VersionControl::None => write!(f, "none"),
        }
    }
}
