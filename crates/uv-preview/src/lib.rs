#[cfg(feature = "schemars")]
use std::borrow::Cow;
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

use thiserror::Error;
use uv_warnings::warn_user_once;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct PreviewFeatures: u32 {
        const PYTHON_INSTALL_DEFAULT = 1 << 0;
        const PYTHON_UPGRADE = 1 << 1;
        const JSON_OUTPUT = 1 << 2;
        const PYLOCK = 1 << 3;
        const ADD_BOUNDS = 1 << 4;
        const PACKAGE_CONFLICTS = 1 << 5;
        const EXTRA_BUILD_DEPENDENCIES = 1 << 6;
        const DETECT_MODULE_CONFLICTS = 1 << 7;
        const FORMAT = 1 << 8;
        const NATIVE_AUTH = 1 << 9;
        const S3_ENDPOINT = 1 << 10;
    }
}

impl PreviewFeatures {
    /// Returns the string representation of a single preview feature flag.
    ///
    /// Panics if given a combination of flags.
    fn flag_as_str(self) -> &'static str {
        match self {
            Self::PYTHON_INSTALL_DEFAULT => "python-install-default",
            Self::PYTHON_UPGRADE => "python-upgrade",
            Self::JSON_OUTPUT => "json-output",
            Self::PYLOCK => "pylock",
            Self::ADD_BOUNDS => "add-bounds",
            Self::PACKAGE_CONFLICTS => "package-conflicts",
            Self::EXTRA_BUILD_DEPENDENCIES => "extra-build-dependencies",
            Self::DETECT_MODULE_CONFLICTS => "detect-module-conflicts",
            Self::FORMAT => "format",
            Self::NATIVE_AUTH => "native-auth",
            Self::S3_ENDPOINT => "s3-endpoint",
            _ => panic!("`flag_as_str` can only be used for exactly one feature flag"),
        }
    }
}

impl Display for PreviewFeatures {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            write!(f, "none")
        } else {
            let features: Vec<&str> = self.iter().map(Self::flag_as_str).collect();
            write!(f, "{}", features.join(","))
        }
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for PreviewFeatures {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("PreviewFeatures")
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let choices: Vec<&str> = Self::all().iter().map(Self::flag_as_str).collect();
        schemars::json_schema!({
            "type": "string",
            "enum": choices,
        })
    }
}

impl<'de> serde::Deserialize<'de> for PreviewFeatures {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = PreviewFeatures;

            fn expecting(&self, f: &mut Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                PreviewFeatures::from_str(v).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl serde::Serialize for PreviewFeatures {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let features: Vec<&str> = self.iter().map(Self::flag_as_str).collect();
        features.serialize(serializer)
    }
}

#[derive(Debug, Error, Clone)]
pub enum PreviewFeaturesParseError {
    #[error("Empty string in preview features: {0}")]
    Empty(String),
}

impl FromStr for PreviewFeatures {
    type Err = PreviewFeaturesParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut flags = Self::empty();

        for part in s.split(',') {
            let part = part.trim();
            if part.is_empty() {
                return Err(PreviewFeaturesParseError::Empty(
                    "Empty string in preview features".to_string(),
                ));
            }

            let flag = match part {
                "python-install-default" => Self::PYTHON_INSTALL_DEFAULT,
                "python-upgrade" => Self::PYTHON_UPGRADE,
                "json-output" => Self::JSON_OUTPUT,
                "pylock" => Self::PYLOCK,
                "add-bounds" => Self::ADD_BOUNDS,
                "package-conflicts" => Self::PACKAGE_CONFLICTS,
                "extra-build-dependencies" => Self::EXTRA_BUILD_DEPENDENCIES,
                "detect-module-conflicts" => Self::DETECT_MODULE_CONFLICTS,
                "format" => Self::FORMAT,
                "native-auth" => Self::NATIVE_AUTH,
                "s3-endpoint" => Self::S3_ENDPOINT,
                _ => {
                    warn_user_once!("Unknown preview feature: `{part}`");
                    continue;
                }
            };

            flags |= flag;
        }

        Ok(flags)
    }
}

pub enum PreviewFeaturesMode {
    EnableAll,
    DisableAll,
    Selection(PreviewFeatures),
}

impl PreviewFeaturesMode {
    pub fn from_bool(b: bool) -> Self {
        if b { Self::EnableAll } else { Self::DisableAll }
    }
}

impl<'a, I> From<I> for PreviewFeaturesMode
where
    I: Iterator<Item = &'a PreviewFeatures>,
{
    fn from(features: I) -> Self {
        let flags = features.fold(PreviewFeatures::empty(), |f1, f2| f1 | *f2);
        Self::Selection(flags)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Preview {
    flags: PreviewFeatures,
}

impl Preview {
    pub fn new(flags: PreviewFeatures) -> Self {
        Self { flags }
    }

    pub fn all() -> Self {
        Self::new(PreviewFeatures::all())
    }

    pub fn is_enabled(&self, flag: PreviewFeatures) -> bool {
        self.flags.contains(flag)
    }
}

impl From<PreviewFeaturesMode> for Preview {
    fn from(mode: PreviewFeaturesMode) -> Self {
        match mode {
            PreviewFeaturesMode::EnableAll => Self::all(),
            PreviewFeaturesMode::DisableAll => Self::default(),
            PreviewFeaturesMode::Selection(flags) => Self { flags },
        }
    }
}

impl Display for Preview {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.flags.is_empty() {
            write!(f, "disabled")
        } else if self.flags == PreviewFeatures::all() {
            write!(f, "enabled")
        } else {
            write!(f, "{}", self.flags)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preview_features_from_str() {
        // Test single feature
        let features = PreviewFeatures::from_str("python-install-default").unwrap();
        assert_eq!(features, PreviewFeatures::PYTHON_INSTALL_DEFAULT);

        // Test multiple features
        let features = PreviewFeatures::from_str("python-upgrade,json-output").unwrap();
        assert!(features.contains(PreviewFeatures::PYTHON_UPGRADE));
        assert!(features.contains(PreviewFeatures::JSON_OUTPUT));
        assert!(!features.contains(PreviewFeatures::PYLOCK));

        // Test with whitespace
        let features = PreviewFeatures::from_str("pylock , add-bounds").unwrap();
        assert!(features.contains(PreviewFeatures::PYLOCK));
        assert!(features.contains(PreviewFeatures::ADD_BOUNDS));

        // Test empty string error
        assert!(PreviewFeatures::from_str("").is_err());
        assert!(PreviewFeatures::from_str("pylock,").is_err());
        assert!(PreviewFeatures::from_str(",pylock").is_err());

        // Test unknown feature (should be ignored with warning)
        let features = PreviewFeatures::from_str("unknown-feature,pylock").unwrap();
        assert!(features.contains(PreviewFeatures::PYLOCK));
        assert_eq!(features.bits().count_ones(), 1);
    }

    #[test]
    fn test_preview_features_display() {
        // Test empty
        let features = PreviewFeatures::empty();
        assert_eq!(features.to_string(), "none");

        // Test single feature
        let features = PreviewFeatures::PYTHON_INSTALL_DEFAULT;
        assert_eq!(features.to_string(), "python-install-default");

        // Test multiple features
        let features = PreviewFeatures::PYTHON_UPGRADE | PreviewFeatures::JSON_OUTPUT;
        assert_eq!(features.to_string(), "python-upgrade,json-output");
    }

    #[test]
    fn test_preview_display() {
        // Test disabled
        let preview = Preview::default();
        assert_eq!(preview.to_string(), "disabled");

        // Test enabled (all features)
        let preview = Preview::all();
        assert_eq!(preview.to_string(), "enabled");

        // Test specific features
        let preview = Preview::new(PreviewFeatures::PYTHON_UPGRADE | PreviewFeatures::PYLOCK);
        assert_eq!(preview.to_string(), "python-upgrade,pylock");
    }

    #[test]
    fn test_preview_from_args() {
        // Test no_preview
        let preview = Preview::from(PreviewFeaturesMode::DisableAll);
        assert_eq!(preview.to_string(), "disabled");

        // Test preview (all features)
        let preview = Preview::from(PreviewFeaturesMode::EnableAll);
        assert_eq!(preview.to_string(), "enabled");

        // Test specific features
        let preview = Preview::from(PreviewFeaturesMode::from(
            [
                PreviewFeatures::PYTHON_UPGRADE,
                PreviewFeatures::JSON_OUTPUT,
            ]
            .iter(),
        ));
        assert!(preview.is_enabled(PreviewFeatures::PYTHON_UPGRADE));
        assert!(preview.is_enabled(PreviewFeatures::JSON_OUTPUT));
        assert!(!preview.is_enabled(PreviewFeatures::PYLOCK));
    }

    #[test]
    fn test_as_str_single_flags() {
        assert_eq!(
            PreviewFeatures::PYTHON_INSTALL_DEFAULT.flag_as_str(),
            "python-install-default"
        );
        assert_eq!(
            PreviewFeatures::PYTHON_UPGRADE.flag_as_str(),
            "python-upgrade"
        );
        assert_eq!(PreviewFeatures::JSON_OUTPUT.flag_as_str(), "json-output");
        assert_eq!(PreviewFeatures::PYLOCK.flag_as_str(), "pylock");
        assert_eq!(PreviewFeatures::ADD_BOUNDS.flag_as_str(), "add-bounds");
        assert_eq!(
            PreviewFeatures::PACKAGE_CONFLICTS.flag_as_str(),
            "package-conflicts"
        );
        assert_eq!(
            PreviewFeatures::EXTRA_BUILD_DEPENDENCIES.flag_as_str(),
            "extra-build-dependencies"
        );
        assert_eq!(
            PreviewFeatures::DETECT_MODULE_CONFLICTS.flag_as_str(),
            "detect-module-conflicts"
        );
        assert_eq!(PreviewFeatures::FORMAT.flag_as_str(), "format");
        assert_eq!(PreviewFeatures::S3_ENDPOINT.flag_as_str(), "s3-endpoint");
    }

    #[test]
    #[should_panic(expected = "`flag_as_str` can only be used for exactly one feature flag")]
    fn test_as_str_multiple_flags_panics() {
        let features = PreviewFeatures::PYTHON_UPGRADE | PreviewFeatures::JSON_OUTPUT;
        let _ = features.flag_as_str();
    }
}
