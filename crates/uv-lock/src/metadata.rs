//! Metadata acquisition used when validating a lockfile.

use std::error::Error;
use std::path::Path;

use uv_distribution_types::{Dist, HashValidation, Metadata, RequiresDist};
use uv_pypi_types::PyProjectToml;

/// Supplies current metadata without coupling lock validation to a downloader or build backend.
pub trait LockMetadataProvider {
    type Error: Error + Send + Sync + 'static;

    fn is_offline(&self) -> bool;

    /// Read distribution metadata, verifying locked source hashes when supplied.
    fn metadata(
        &self,
        dist: &Dist,
        locked_hashes: Option<HashValidation<'_>>,
    ) -> impl Future<Output = Result<Metadata, Self::Error>>;

    /// Lower static source-tree requirements, if available.
    fn requires_dist(
        &self,
        path: &Path,
        pyproject: &PyProjectToml,
    ) -> impl Future<Output = Result<Option<RequiresDist>, Self::Error>>;
}
