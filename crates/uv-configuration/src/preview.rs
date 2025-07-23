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
    }
}

impl PreviewFeatures {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PYTHON_INSTALL_DEFAULT => "python-install-default",
            Self::PYTHON_UPGRADE => "python-upgrade",
            Self::JSON_OUTPUT => "json-output",
            Self::PYLOCK => "pylock",
            Self::ADD_BOUNDS => "add-bounds",
            _ => "unknown",
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
        let mut flags = PreviewFeatures::empty();

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
            self.flags
                .iter()
                .map(PreviewFeatures::as_str)
                .collect::<Vec<_>>()
                .join(", ")
                .fmt(f)
        }
    }
}
