// CLI flags for controlling Python resolution policy.
use clap::Args;

#[derive(Debug, Clone, Default, Args)]
pub struct ResolutionFlags {
    /// Restrict resolver to a single Python version (e.g. 3.11.9)
    #[arg(long = "resolve-python-only")]
    pub resolve_python_only: Option<String>,

    /// Restrict resolver to a semver range (e.g. ">=3.10,<3.13")
    #[arg(long = "resolve-python-range")]
    pub resolve_python_range: Option<String>,
}

impl ResolutionFlags {
    pub fn is_set(&self) -> bool {
        self.resolve_python_only.is_some() || self.resolve_python_range.is_some()
    }
}
