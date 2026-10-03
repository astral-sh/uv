use owo_colors::OwoColorize;
use uv_command_support::{UvError, dist_hints};
use uv_distribution_types::{DerivationChain, Name};
use uv_install_operations::Changelog;
use uv_resolve_operations::ExtrasWithoutSourceError;
use uv_resolver::{NoSolutionError, NoSolutionHeader, ResolveError};

/// An operation failure shared by environment resolution and installation.
#[derive(thiserror::Error, Debug)]
pub enum OperationsError {
    #[error(transparent)]
    Prepare(#[from] uv_installer::PrepareError),

    #[error("{header}")]
    NoSolution {
        header: NoSolutionHeader,
        #[source]
        source: Box<NoSolutionError>,
    },

    #[error(transparent)]
    Resolve(#[from] ResolveError),

    #[error(transparent)]
    Uninstall(#[from] uv_installer::UninstallError),

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

    #[error("The environment is outdated; run `{}` to update the environment", "uv sync".cyan())]
    OutdatedEnvironment(Box<Changelog>),
}

impl OperationsError {
    /// Add the default heading when this operation is the final command error.
    ///
    /// Nested operation errors may already have a more specific heading from their caller.
    #[must_use]
    pub fn with_default_resolution_context(self) -> Self {
        match self {
            Self::Resolve(ResolveError::NoSolution(source)) => Self::NoSolution {
                header: NoSolutionHeader::new(source.environment().clone()),
                source,
            },
            error @ (Self::Prepare(_)
            | Self::NoSolution { .. }
            | Self::Resolve(_)
            | Self::Uninstall(_)
            | Self::Hash(_)
            | Self::Io(_)
            | Self::Fmt(_)
            | Self::Requirements(_)
            | Self::RequirementsWithContext { .. }
            | Self::Anyhow(_)
            | Self::OutdatedEnvironment(_)) => error,
        }
    }

    /// Set the command-specific context for a resolution failure.
    #[must_use]
    pub fn with_resolution_context(self, context: &'static str) -> Self {
        match self.with_default_resolution_context() {
            Self::NoSolution { header, source } => Self::NoSolution {
                header: header.with_context(context),
                source,
            },
            Self::Requirements(source) | Self::RequirementsWithContext { source, .. } => {
                Self::RequirementsWithContext { context, source }
            }
            error @ (Self::Prepare(_)
            | Self::Resolve(_)
            | Self::Uninstall(_)
            | Self::Hash(_)
            | Self::Io(_)
            | Self::Fmt(_)
            | Self::Anyhow(_)
            | Self::OutdatedEnvironment(_)) => error,
        }
    }

    /// Return whether this operation failure is an expected user-facing failure.
    pub fn is_user_failure(&self) -> bool {
        match self {
            Self::Prepare(error) => error.is_user_failure(),
            Self::NoSolution { .. } => true,
            Self::Resolve(error) => error.is_user_failure(),
            Self::Hash(_) | Self::OutdatedEnvironment(_) => true,
            Self::Requirements(error) | Self::RequirementsWithContext { source: error, .. } => {
                error.is_user_failure()
            }
            Self::Uninstall(_) | Self::Io(_) | Self::Fmt(_) | Self::Anyhow(_) => false,
        }
    }
}

impl uv_errors::Hinted for OperationsError {
    fn hints(&self) -> uv_errors::Hints<'_> {
        match self {
            Self::NoSolution { source, .. } => source.hints(),
            Self::Resolve(uv_resolver::ResolveError::Dist(_, dist, chain, error)) => {
                dist_hints(dist.name(), dist.version(), chain, error.hints())
            }
            Self::Resolve(uv_resolver::ResolveError::Dependencies(error, name, version, chain)) => {
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
            Self::Prepare(uv_installer::PrepareError::Dist(_, dist, chain, error)) => {
                dist_hints(dist.name(), dist.version(), chain, error.hints())
            }
            Self::Anyhow(err) => {
                for cause in err.chain() {
                    if let Some(extra_err) = cause.downcast_ref::<ExtrasWithoutSourceError>() {
                        return uv_errors::Hinted::hints(extra_err);
                    }
                }
                uv_errors::Hints::none()
            }
            Self::Prepare(_)
            | Self::Uninstall(_)
            | Self::Hash(_)
            | Self::Io(_)
            | Self::Fmt(_)
            | Self::Requirements(_)
            | Self::RequirementsWithContext { .. }
            | Self::OutdatedEnvironment(_) => uv_errors::Hints::none(),
        }
    }
}

impl From<uv_resolve_operations::Error> for OperationsError {
    fn from(error: uv_resolve_operations::Error) -> Self {
        match error {
            uv_resolve_operations::Error::NoSolution { header, source } => {
                Self::NoSolution { header, source }
            }
            uv_resolve_operations::Error::Resolve(error) => Self::Resolve(error),
            uv_resolve_operations::Error::Hash(error) => Self::Hash(error),
            uv_resolve_operations::Error::Io(error) => Self::Io(error),
            uv_resolve_operations::Error::Fmt(error) => Self::Fmt(error),
            uv_resolve_operations::Error::Requirements(error) => Self::Requirements(error),
            uv_resolve_operations::Error::RequirementsWithContext { context, source } => {
                Self::RequirementsWithContext { context, source }
            }
            uv_resolve_operations::Error::Anyhow(error) => Self::Anyhow(error),
        }
    }
}

impl From<uv_install_operations::Error> for OperationsError {
    fn from(error: uv_install_operations::Error) -> Self {
        match error {
            uv_install_operations::Error::Prepare(error) => Self::Prepare(error),
            uv_install_operations::Error::Uninstall(error) => Self::Uninstall(error),
            uv_install_operations::Error::Hash(error) => Self::Hash(error),
            uv_install_operations::Error::Io(error) => Self::Io(error),
            uv_install_operations::Error::Fmt(error) => Self::Fmt(error),
            uv_install_operations::Error::Anyhow(error) => Self::Anyhow(error),
            uv_install_operations::Error::OutdatedEnvironment(changelog) => {
                Self::OutdatedEnvironment(changelog)
            }
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
