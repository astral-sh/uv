use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

// Note that the ordering of the variants is significant, as when given a list of operations
// to perform, we sort them and apply them in order, so users don't have to think too hard about it.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum VersionBump {
    /// Increase the major version (e.g., 1.2.3 => 2.0.0)
    Major,
    /// Increase the minor version (e.g., 1.2.3 => 1.3.0)
    Minor,
    /// Increase the patch version (e.g., 1.2.3 => 1.2.4)
    Patch,
    /// Move from a pre-release to stable version (e.g., 1.2.3b4.post5.dev6 => 1.2.3)
    ///
    /// Removes all pre-release components, but will not remove "local" components.
    Stable,
    /// Increase the alpha version (e.g., 1.2.3a4 => 1.2.3a5)
    ///
    /// To move from a stable to a pre-release version, combine this with a stable component, e.g.,
    /// for 1.2.3 => 2.0.0a1, you'd also include [`VersionBump::Major`].
    Alpha,
    /// Increase the beta version (e.g., 1.2.3b4 => 1.2.3b5)
    ///
    /// To move from a stable to a pre-release version, combine this with a stable component, e.g.,
    /// for 1.2.3 => 2.0.0b1, you'd also include [`VersionBump::Major`].
    Beta,
    /// Increase the rc version (e.g., 1.2.3rc4 => 1.2.3rc5)
    ///
    /// To move from a stable to a pre-release version, combine this with a stable component, e.g.,
    /// for 1.2.3 => 2.0.0rc1, you'd also include [`VersionBump::Major`].]
    Rc,
    /// Increase the post version (e.g., 1.2.3.post5 => 1.2.3.post6)
    Post,
    /// Increase the dev version (e.g., 1.2.3a4.dev6 => 1.2.3.dev7)
    Dev,
}

impl Display for VersionBump {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let string = match self {
            Self::Major => "major",
            Self::Minor => "minor",
            Self::Patch => "patch",
            Self::Stable => "stable",
            Self::Alpha => "alpha",
            Self::Beta => "beta",
            Self::Rc => "rc",
            Self::Post => "post",
            Self::Dev => "dev",
        };
        string.fmt(f)
    }
}

impl FromStr for VersionBump {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "major" => Ok(Self::Major),
            "minor" => Ok(Self::Minor),
            "patch" => Ok(Self::Patch),
            "stable" => Ok(Self::Stable),
            "alpha" => Ok(Self::Alpha),
            "beta" => Ok(Self::Beta),
            "rc" => Ok(Self::Rc),
            "post" => Ok(Self::Post),
            "dev" => Ok(Self::Dev),
            _ => Err(format!("invalid bump component `{value}`")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VersionBumpSpec {
    pub bump: VersionBump,
    pub value: Option<u64>,
}

impl Display for VersionBumpSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.value {
            Some(value) => write!(f, "{}={value}", self.bump),
            None => self.bump.fmt(f),
        }
    }
}

impl FromStr for VersionBumpSpec {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (name, value) = match input.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (input, None),
        };

        let bump = name.parse::<VersionBump>()?;

        if bump == VersionBump::Stable && value.is_some() {
            return Err("`--bump stable` does not accept a value".to_string());
        }

        let value = match value {
            Some("") => {
                return Err("`--bump` values cannot be empty".to_string());
            }
            Some(raw) => Some(
                raw.parse::<u64>()
                    .map_err(|_| format!("invalid numeric value `{raw}` for `--bump {name}`"))?,
            ),
            None => None,
        };

        Ok(Self { bump, value })
    }
}
