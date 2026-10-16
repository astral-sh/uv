pub use crate::extras::*;
pub use crate::lookahead::*;
pub use crate::script::{
    ScriptRequirementsError, script_extra_build_requires, script_specification,
};
pub use crate::source_tree::*;
pub use crate::sources::*;
pub use crate::specification::*;
pub use crate::unnamed::*;
pub use uv_configuration::RequirementsInput;

use uv_distribution::DistributionDatabase;
use uv_distribution_types::{Dist, DistErrorKind, Requirement};
use uv_normalize::PackageName;
use uv_types::BuildContext;

mod extras;
mod lookahead;
mod script;
mod source_tree;
mod sources;
mod specification;
mod unnamed;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} `{1}`")]
    Dist(
        DistErrorKind,
        Box<Dist>,
        #[source] Box<uv_distribution::Error>,
    ),

    #[error(transparent)]
    Distribution(#[from] Box<uv_distribution::Error>),

    #[error(transparent)]
    DistributionTypes(#[from] uv_distribution_types::Error),

    #[error(transparent)]
    HashStrategy(#[from] uv_types::HashStrategyError),

    #[error(transparent)]
    FlatIndex(#[from] Box<uv_client::FlatIndexError>),

    #[error(transparent)]
    WheelFilename(#[from] uv_distribution_filename::WheelFilenameError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    /// Return whether this is an expected user-facing failure.
    pub fn is_user_failure(&self) -> bool {
        match self {
            Self::Dist(_, _, error) | Self::Distribution(error) => error.is_user_failure(),
            Self::FlatIndex(error) => error.is_user_failure(),
            Self::DistributionTypes(_) | Self::HashStrategy(_) | Self::WheelFilename(_) => true,
            Self::Io(error) => matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::InvalidInput
            ),
        }
    }

    /// Create an [`Error`] from a distribution error.
    fn from_dist(dist: Dist, err: uv_distribution::Error) -> Self {
        Self::Dist(
            DistErrorKind::from_dist(&dist, &err),
            Box::new(dist),
            Box::new(err),
        )
    }
}

/// Convert a [`Requirement`] into a [`Dist`], discovering remote archive filenames as needed.
pub(crate) async fn required_dist<Context: BuildContext>(
    requirement: &Requirement,
    database: &DistributionDatabase<'_, Context>,
) -> Result<Option<Dist>, Error> {
    let Some(url) = requirement.source.to_verbatim_parsed_url() else {
        return Ok(None);
    };
    database
        .resolve_url(&requirement.name, &url)
        .await
        .map(Some)
        .map_err(|error| Error::Distribution(Box::new(error)))
}

/// Validate names when metadata is reused across requirements for the same URL.
pub(crate) fn validate_metadata_name(
    expected: &PackageName,
    actual: &PackageName,
) -> Result<(), Error> {
    if expected != actual {
        return Err(Error::Distribution(Box::new(
            uv_distribution::Error::WheelMetadataNameMismatch {
                given: expected.clone(),
                metadata: actual.clone(),
            },
        )));
    }
    Ok(())
}
