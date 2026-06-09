use std::fmt;

use serde::{Deserialize, Serialize};

/// Strategy for handling upload failures during `uv publish`.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PublishFailureStrategy {
    /// Stop on the first failure.
    StopFirst,
    /// Continue uploading all files, report errors at the end.
    KeepGoing,
    /// Continue only if at least one upload already succeeded.
    #[default]
    KeepGoingAfterSuccess,
}

impl fmt::Display for PublishFailureStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StopFirst => write!(f, "stop-first"),
            Self::KeepGoing => write!(f, "keep-going"),
            Self::KeepGoingAfterSuccess => write!(f, "keep-going-after-success"),
        }
    }
}
