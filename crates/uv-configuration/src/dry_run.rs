#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DryRun {
    /// The operation should execute in dry run mode.
    Enabled,
    /// The operation should execute in normal mode.
    #[default]
    Disabled,
}

impl DryRun {
    /// Determine the [`DryRun`] setting based on the command-line arguments.
    pub fn from_args(dry_run: bool) -> Self {
        if dry_run {
            DryRun::Enabled
        } else {
            DryRun::Disabled
        }
    }

    /// Returns `true` if dry run mode is enabled.
    pub const fn enabled(&self) -> bool {
        matches!(self, DryRun::Enabled)
    }
}
