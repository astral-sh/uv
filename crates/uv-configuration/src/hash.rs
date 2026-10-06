#[derive(Debug, Default, Copy, Clone)]
pub enum HashCheckingMode {
    /// Hashes should be validated against a pre-defined list of hashes. Every requirement must
    /// itself be hashable (e.g., Git dependencies are forbidden) _and_ have a hash in the lockfile.
    Require,
    /// Hashes should be validated, if present, but ignored if absent.
    #[default]
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

    /// Apply the `--require-hashes` setting from a requirements file.
    pub fn from_requirements_txt(mode: Option<Self>, require_hashes: bool) -> Option<Self> {
        if require_hashes {
            Some(Self::Require)
        } else {
            mode
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

/// The configured hash requirement for build dependencies, before applying command defaults.
#[derive(Debug, Default, Copy, Clone)]
pub enum BuildHashChecking {
    /// Use the command's default checking mode and trust hashes declared by requirements.
    #[default]
    Default,
    /// Require hashes declared before the build backend runs.
    Require,
}

impl BuildHashChecking {
    /// Resolve verification and trust together, using the command's default checking mode.
    pub fn resolve(self, default: Option<HashCheckingMode>) -> BuildHashPolicy {
        match self {
            Self::Require => BuildHashPolicy::Require(BuildHashSources::StaticRequirements),
            Self::Default => match default {
                None => BuildHashPolicy::Disabled,
                Some(HashCheckingMode::Verify) => BuildHashPolicy::Verify,
                Some(HashCheckingMode::Require) => {
                    BuildHashPolicy::Require(BuildHashSources::AllRequirements)
                }
            },
        }
    }
}

/// The effective verification and trust policy for build dependencies.
#[derive(Debug, Copy, Clone)]
pub enum BuildHashPolicy {
    /// Do not verify build dependency hashes.
    Disabled,
    /// Verify supplied hashes, including hashes declared by the build backend.
    Verify,
    /// Require hashes from the specified sources.
    Require(BuildHashSources),
}

impl BuildHashPolicy {
    /// The verification mode to use when constructing the build dependency hash strategy.
    pub fn checking(self) -> Option<HashCheckingMode> {
        match self {
            Self::Disabled => None,
            Self::Verify => Some(HashCheckingMode::Verify),
            Self::Require(_) => Some(HashCheckingMode::Require),
        }
    }

    /// The requirement declarations that may contribute trusted hashes.
    pub fn sources(self) -> BuildHashSources {
        match self {
            Self::Disabled | Self::Verify => BuildHashSources::AllRequirements,
            Self::Require(sources) => sources,
        }
    }
}

/// Sources that may introduce trusted hashes for build dependencies.
#[derive(Debug, Copy, Clone)]
pub enum BuildHashSources {
    /// Constraints, static requirements, and requirements returned by the build backend.
    AllRequirements,
    /// Constraints and static requirements, excluding requirements returned by the build backend.
    StaticRequirements,
}
