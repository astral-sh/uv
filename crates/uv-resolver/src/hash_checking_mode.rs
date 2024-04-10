#[derive(Debug, Default, Clone, Copy)]
pub enum HashCheckingMode {
    /// Hash-checking mode is disabled.
    #[default]
    Disabled,
    /// Hash-checking mode is enabled.
    Enabled,
}

impl HashCheckingMode {
    /// Returns `true` if hash-checking is enabled.
    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}
