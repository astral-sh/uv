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
        const EXTRA_BUILD_DEPENDENCIES = 1 << 5;
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
            Self::EXTRA_BUILD_DEPENDENCIES => "extra-build-dependencies",
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
                "extra-build-dependencies" => Self::EXTRA_BUILD_DEPENDENCIES,
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

    pub fn from_args(
        preview: bool,
        no_preview: bool,
        preview_features: &[PreviewFeatures],
    ) -> Self {
        if no_preview {
            return Self::default();
        }

        if preview {
            return Self::all();
        }

        let mut flags = PreviewFeatures::empty();

        for features in preview_features {
            flags |= *features;
        }

        Self { flags }
    }

    pub fn is_enabled(&self, flag: PreviewFeatures) -> bool {
        self.flags.contains(flag)
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
        let preview = Preview::from_args(true, true, &[]);
        assert_eq!(preview.to_string(), "disabled");

        // Test preview (all features)
        let preview = Preview::from_args(true, false, &[]);
        assert_eq!(preview.to_string(), "enabled");

        // Test specific features
        let features = vec![
            PreviewFeatures::PYTHON_UPGRADE,
            PreviewFeatures::JSON_OUTPUT,
        ];
        let preview = Preview::from_args(false, false, &features);
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
            PreviewFeatures::EXTRA_BUILD_DEPENDENCIES.flag_as_str(),
            "extra-build-dependencies"
        );
    }

    #[test]
    #[should_panic(expected = "`flag_as_str` can only be used for exactly one feature flag")]
    fn test_as_str_multiple_flags_panics() {
        let features = PreviewFeatures::PYTHON_UPGRADE | PreviewFeatures::JSON_OUTPUT;
        let _ = features.flag_as_str();
    }
}
