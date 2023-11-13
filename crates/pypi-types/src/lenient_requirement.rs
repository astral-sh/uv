use pep440_rs::{Pep440Error, VersionSpecifiers};
use serde::{de, Deserialize, Deserializer, Serialize};
use std::str::FromStr;
use tracing::warn;

/// Like [`VersionSpecifiers`], but attempts to correct some common errors in user-provided requirements.
///
/// We turn `>=3.x.*` into `>=3.x`
#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
pub struct LenientVersionSpecifiers(VersionSpecifiers);

impl FromStr for LenientVersionSpecifiers {
    type Err = Pep440Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match VersionSpecifiers::from_str(s) {
            Ok(specifiers) => Ok(Self(specifiers)),
            Err(err) => {
                // Given `>=3.5.*`, rewrite to `>=3.5`.
                let patched = match s {
                    ">=3.12.*" => Some(">=3.12"),
                    ">=3.11.*" => Some(">=3.11"),
                    ">=3.10.*" => Some(">=3.10"),
                    ">=3.9.*" => Some(">=3.9"),
                    ">=3.8.*" => Some(">=3.8"),
                    ">=3.7.*" => Some(">=3.7"),
                    ">=3.6.*" => Some(">=3.6"),
                    ">=3.5.*" => Some(">=3.5"),
                    ">=3.4.*" => Some(">=3.4"),
                    ">=3.3.*" => Some(">=3.3"),
                    ">=3.2.*" => Some(">=3.2"),
                    ">=3.1.*" => Some(">=3.1"),
                    ">=3.0.*" => Some(">=3.0"),
                    ">=3.12," => Some(">=3.12"),
                    ">=3.11," => Some(">=3.11"),
                    ">=3.10," => Some(">=3.10"),
                    ">=3.9," => Some(">=3.9"),
                    ">=3.8," => Some(">=3.8"),
                    ">=3.7," => Some(">=3.7"),
                    ">=3.6," => Some(">=3.6"),
                    ">=3.5," => Some(">=3.5"),
                    ">=3.4," => Some(">=3.4"),
                    ">=3.3," => Some(">=3.3"),
                    ">=3.2," => Some(">=3.2"),
                    ">=3.1," => Some(">=3.1"),
                    ">=3.0," => Some(">=3.0"),
                    ">=2.7,!=3.0*,!=3.1*,!=3.2*" => Some(">=2.7,!=3.0.*,!=3.1.*,!=3.2.*"),
                    _ => None,
                };
                if let Some(patched) = patched {
                    if let Ok(specifier) = VersionSpecifiers::from_str(patched) {
                        warn!(
                        "Correcting invalid wildcard bound on version specifier (before: `{s}`; after: `{patched}`)",
                    );
                        return Ok(Self(specifier));
                    }
                }
                Err(err)
            }
        }
    }
}

impl From<LenientVersionSpecifiers> for VersionSpecifiers {
    fn from(specifiers: LenientVersionSpecifiers) -> Self {
        specifiers.0
    }
}

impl<'de> Deserialize<'de> for LenientVersionSpecifiers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(de::Error::custom)
    }
}
