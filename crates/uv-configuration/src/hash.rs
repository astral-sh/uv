#[derive(Debug, Copy, Clone)]
pub enum HashCheckingMode {
    /// Hashes should be validated against a pre-defined list of hashes. Every requirement must
    /// itself be hashable (e.g., Git dependencies are forbidden) _and_ have a hash in the lockfile.
    Require,
    /// Hashes should be validated, if present, but ignored if absent.
    Verify,
}

impl HashCheckingMode {
    /// Return the [`HashCheckingMode`] from the command-line arguments, if any.
    pub fn from_args(require_hashes: bool, verify_hashes: bool) -> Option<Self> {
        if require_hashes {
            Some(Self::Require)
        } else if verify_hashes {
            Some(Self::Verify)
        } else {
            None
        }
    }

    /// Returns `true` if the hash checking mode is `Require`.
    pub fn is_require(&self) -> bool {
        matches!(self, Self::Require)
    }
}

impl std::fmt::Display for HashCheckingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Require => write!(f, "--require-hashes"),
            Self::Verify => write!(f, "--verify-hashes"),
        }
    }
}
