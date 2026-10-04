use owo_colors::OwoColorize;
use uv_command_support::{UvError, dist_hints};
use uv_distribution_types::Name;

use crate::Changelog;

/// An error while preparing or installing distributions.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Prepare(#[from] uv_installer::PrepareError),
    #[error(transparent)]
    Uninstall(#[from] uv_installer::UninstallError),
    #[error(transparent)]
    Hash(#[from] uv_types::HashStrategyError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    #[error("The environment is outdated; run `{}` to update the environment", "uv sync".cyan())]
    OutdatedEnvironment(Box<Changelog>),
}

impl Error {
    /// Return the changes required by an environment that failed an up-to-date check.
    pub fn outdated_environment(&self) -> Option<&Changelog> {
        match self {
            Self::OutdatedEnvironment(changelog) => Some(changelog),
            Self::Prepare(_)
            | Self::Uninstall(_)
            | Self::Hash(_)
            | Self::Io(_)
            | Self::Fmt(_)
            | Self::Anyhow(_) => None,
        }
    }

    /// Return whether this operation failure is an expected user-facing failure.
    pub fn is_user_failure(&self) -> bool {
        match self {
            Self::Prepare(error) => error.is_user_failure(),
            Self::Hash(_) | Self::OutdatedEnvironment(_) => true,
            Self::Uninstall(_) | Self::Io(_) | Self::Fmt(_) | Self::Anyhow(_) => false,
        }
    }
}

impl From<Error> for UvError {
    fn from(error: Error) -> Self {
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
            Self::Prepare(uv_installer::PrepareError::Dist(_, dist, chain, error)) => {
                dist_hints(dist.name(), dist.version(), chain, error.hints())
            }
            Self::Prepare(_)
            | Self::Uninstall(_)
            | Self::Hash(_)
            | Self::Io(_)
            | Self::Fmt(_)
            | Self::Anyhow(_)
            | Self::OutdatedEnvironment(_) => uv_errors::Hints::none(),
        }
    }
}
