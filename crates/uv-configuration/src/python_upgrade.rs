/// The command that requested a managed Python upgrade.
#[derive(Debug, Clone, Copy)]
pub enum PythonUpgradeSource {
    /// The user invoked `uv python install --upgrade`
    Install,
    /// The user invoked `uv python upgrade`
    Upgrade,
}

impl std::fmt::Display for PythonUpgradeSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Install => write!(f, "uv python install --upgrade"),
            Self::Upgrade => write!(f, "uv python upgrade"),
        }
    }
}

/// Whether managed Python installations should be upgraded.
#[derive(Debug, Clone, Copy)]
pub enum PythonUpgrade {
    /// Python upgrades are enabled.
    Enabled(PythonUpgradeSource),
    /// Python upgrades are disabled.
    Disabled,
}
