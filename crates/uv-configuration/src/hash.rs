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
    ///
    /// By default, the hash checking mode is [`HashCheckingMode::Verify`]. If `--require-hashes` is
    /// passed, the hash checking mode is [`HashCheckingMode::Require`]. If `--no-verify-hashes` is
    /// passed, then no hash checking is performed.
    pub fn from_args(require_hashes: Option<bool>, verify_hashes: Option<bool>) -> Option<Self> {
        if require_hashes == Some(true) {
            // Given `--require-hashes`, always require hashes, regardless of any other flags.
            Some(Self::Require)
        } else if verify_hashes == Some(true) {
            // Given `--verify-hashes`, always verify hashes, regardless of any other flags.
            Some(Self::Verify)
        } else if verify_hashes == Some(false) {
            // Given `--no-verify-hashes` (without `--require-hashes`), do not verify hashes.
            None
        } else if require_hashes == Some(false) {
            // Given `--no-require-hashes` (without `--verify-hashes`), do not require hashes.
            None
        } else {
            // By default, verify hashes.
            Some(Self::Verify)
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
