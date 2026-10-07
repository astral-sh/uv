/// Whether to use the active virtual environment instead of the project or script environment.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ActiveEnvironment {
    /// Ignore a mismatched active environment and warn.
    #[default]
    Warn,
    /// Ignore the active environment without warning.
    Ignore,
    /// Prefer the active environment, if one is set.
    Prefer,
}

impl ActiveEnvironment {
    /// Suppress mismatch warnings while preserving an explicit preference for the active environment.
    #[must_use]
    pub fn without_warning(self) -> Self {
        match self {
            Self::Warn | Self::Ignore => Self::Ignore,
            Self::Prefer => Self::Prefer,
        }
    }
}

impl From<Option<bool>> for ActiveEnvironment {
    fn from(active: Option<bool>) -> Self {
        match active {
            None => Self::Warn,
            Some(false) => Self::Ignore,
            Some(true) => Self::Prefer,
        }
    }
}
