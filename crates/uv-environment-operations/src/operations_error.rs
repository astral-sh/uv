use uv_command_support::UvError;
use uv_install_operations::Changelog;
use uv_resolver::NoSolutionError;

/// A failure from resolving or installing an environment's dependencies.
#[derive(thiserror::Error, Debug)]
pub enum OperationsError {
    #[error(transparent)]
    Resolve(#[from] uv_resolve_operations::Error),

    #[error(transparent)]
    Install(#[from] uv_install_operations::Error),
}

impl OperationsError {
    /// Add the default heading when this operation is the final command error.
    #[must_use]
    pub fn with_default_resolution_context(self) -> Self {
        match self {
            Self::Resolve(error) => Self::Resolve(error.with_default_resolution_context()),
            error @ Self::Install(_) => error,
        }
    }

    /// Set the command-specific context for a resolution failure.
    #[must_use]
    pub fn with_resolution_context(self, context: &'static str) -> Self {
        match self {
            Self::Resolve(error) => Self::Resolve(error.with_resolution_context(context)),
            error @ Self::Install(_) => error,
        }
    }

    /// Return the solver failure for an unsatisfiable resolution.
    pub fn as_no_solution(&self) -> Option<&NoSolutionError> {
        match self {
            Self::Resolve(error) => error.as_no_solution(),
            Self::Install(_) => None,
        }
    }

    /// Return the changes required by an environment that failed an up-to-date check.
    pub fn outdated_environment(&self) -> Option<&Changelog> {
        match self {
            Self::Resolve(_) => None,
            Self::Install(error) => error.outdated_environment(),
        }
    }

    /// Return whether this operation failure is an expected user-facing failure.
    pub fn is_user_failure(&self) -> bool {
        match self {
            Self::Resolve(error) => error.is_user_failure(),
            Self::Install(error) => error.is_user_failure(),
        }
    }
}

impl uv_errors::Hinted for OperationsError {
    fn hints(&self) -> uv_errors::Hints<'_> {
        match self {
            Self::Resolve(error) => error.hints(),
            Self::Install(error) => error.hints(),
        }
    }
}

impl From<OperationsError> for UvError {
    fn from(error: OperationsError) -> Self {
        let error = error.with_default_resolution_context();
        if error.is_user_failure() {
            Self::user(error)
        } else {
            Self::unexpected(error.into())
        }
    }
}
