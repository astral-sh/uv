use uv_distribution_types::{DerivationChain, Name};
use uv_resolver::{NoSolutionError, NoSolutionHeader, ResolveError};

use crate::commands::UvError;
use crate::commands::diagnostics::dist_hints;

/// An error while reading requirements or resolving dependencies.
#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("{header}")]
    NoSolution {
        header: NoSolutionHeader,
        #[source]
        source: Box<NoSolutionError>,
    },
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Hash(#[from] uv_types::HashStrategyError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),
    #[error(transparent)]
    Requirements(#[from] uv_requirements::Error),
    #[error("Failed to resolve {context} requirement")]
    RequirementsWithContext {
        context: &'static str,
        #[source]
        source: uv_requirements::Error,
    },
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl Error {
    /// Return the solver failure for an unsatisfiable resolution.
    pub(crate) fn as_no_solution(&self) -> Option<&NoSolutionError> {
        match self {
            Self::NoSolution { source, .. } | Self::Resolve(ResolveError::NoSolution(source)) => {
                Some(source)
            }
            Self::Resolve(_)
            | Self::Hash(_)
            | Self::Io(_)
            | Self::Fmt(_)
            | Self::Requirements(_)
            | Self::RequirementsWithContext { .. }
            | Self::Anyhow(_) => None,
        }
    }

    /// Add the default heading when this operation is the final command error.
    #[must_use]
    fn with_default_resolution_context(self) -> Self {
        match self {
            Self::Resolve(ResolveError::NoSolution(source)) => Self::NoSolution {
                header: NoSolutionHeader::new(source.environment().clone()),
                source,
            },
            error @ (Self::NoSolution { .. }
            | Self::Resolve(_)
            | Self::Hash(_)
            | Self::Io(_)
            | Self::Fmt(_)
            | Self::Requirements(_)
            | Self::RequirementsWithContext { .. }
            | Self::Anyhow(_)) => error,
        }
    }

    /// Set the command-specific context for a resolution failure.
    #[must_use]
    pub(crate) fn with_resolution_context(self, context: &'static str) -> Self {
        match self.with_default_resolution_context() {
            Self::NoSolution { header, source } => Self::NoSolution {
                header: header.with_context(context),
                source,
            },
            Self::Requirements(source) | Self::RequirementsWithContext { source, .. } => {
                Self::RequirementsWithContext { context, source }
            }
            error @ (Self::Resolve(_)
            | Self::Hash(_)
            | Self::Io(_)
            | Self::Fmt(_)
            | Self::Anyhow(_)) => error,
        }
    }

    /// Return whether this operation failure is an expected user-facing failure.
    fn is_user_failure(&self) -> bool {
        match self {
            Self::NoSolution { .. } | Self::Hash(_) => true,
            Self::Resolve(error) => error.is_user_failure(),
            Self::Requirements(error) | Self::RequirementsWithContext { source: error, .. } => {
                error.is_user_failure()
            }
            Self::Io(_) | Self::Fmt(_) | Self::Anyhow(_) => false,
        }
    }
}

impl From<Error> for UvError {
    fn from(error: Error) -> Self {
        let error = error.with_default_resolution_context();
        if error.is_user_failure() {
            Self::User(error.into())
        } else {
            Self::Unexpected(error.into())
        }
    }
}

impl uv_errors::Hinted for Error {
    fn hints(&self) -> uv_errors::Hints<'_> {
        match self {
            Self::NoSolution { source, .. } => source.hints(),
            Self::Resolve(ResolveError::Dist(_, dist, chain, error)) => {
                dist_hints(dist.name(), dist.version(), chain, error.hints())
            }
            Self::Resolve(ResolveError::Dependencies(error, name, version, chain)) => {
                dist_hints(name, Some(version), chain, error.hints())
            }
            Self::Resolve(error) => error.hints(),
            Self::Requirements(uv_requirements::Error::Dist(_, dist, error))
            | Self::RequirementsWithContext {
                source: uv_requirements::Error::Dist(_, dist, error),
                ..
            } => dist_hints(
                dist.name(),
                dist.version(),
                &DerivationChain::default(),
                error.hints(),
            ),
            Self::Anyhow(error) => {
                for cause in error.chain() {
                    if let Some(error) = cause.downcast_ref::<ExtrasWithoutSourceError>() {
                        return uv_errors::Hinted::hints(error);
                    }
                }
                uv_errors::Hints::none()
            }
            Self::Hash(_)
            | Self::Io(_)
            | Self::Fmt(_)
            | Self::Requirements(_)
            | Self::RequirementsWithContext { .. } => uv_errors::Hints::none(),
        }
    }
}

/// Extras were requested but no valid source was provided.
#[derive(Debug, thiserror::Error)]
#[error(
    "Requesting extras requires a `pylock.toml`, `pyproject.toml`, `setup.cfg`, or `setup.py` file"
)]
pub(crate) struct ExtrasWithoutSourceError {
    pub(crate) has_editable: bool,
}

impl uv_errors::Hinted for ExtrasWithoutSourceError {
    fn hints(&self) -> uv_errors::Hints<'_> {
        uv_errors::Hints::from(if self.has_editable {
            "Use `<dir>[extra]` syntax or `-r <file>` instead"
        } else {
            "Use `package[extra]` syntax instead"
        })
    }
}
