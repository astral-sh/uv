//! Auditing for [PEP 792] adverse project statuses.
//!
//! [PEP 792]: https://peps.python.org/pep-0792/

use futures::{StreamExt as _, stream};
use tokio::sync::Semaphore;
use tracing::trace;

use uv_client::{MetadataFormat, RegistryClient};
use uv_configuration::Concurrency;
use uv_distribution_types::{IndexCapabilities, IndexMetadataRef, IndexUrl};
use uv_normalize::PackageName;
use uv_pypi_types::{ProjectStatus as PypiProjectStatus, Status};

use crate::types::{self, AdverseStatus, Finding};

/// Audit projects for PEP 792 adverse status markers using a [`RegistryClient`].
pub struct ProjectStatusAudit<'a> {
    client: &'a RegistryClient,
    capabilities: &'a IndexCapabilities,
    concurrency: Concurrency,
}

impl<'a> ProjectStatusAudit<'a> {
    /// Create a new audit session backed by the given [`RegistryClient`].
    pub fn new(
        client: &'a RegistryClient,
        capabilities: &'a IndexCapabilities,
        concurrency: Concurrency,
    ) -> Self {
        Self {
            client,
            capabilities,
            concurrency,
        }
    }

    /// Query the project-level status of each project on its index.
    ///
    /// Transient per-project query failures (network errors, not-found, offline
    /// without a cache hit) are logged and dropped, on the principle that one
    /// misbehaving index should not invalidate the rest of the audit.
    pub async fn query_batch(&self, projects: &[(&PackageName, IndexUrl)]) -> Vec<Finding> {
        if projects.is_empty() {
            return Vec::new();
        }

        let semaphore = self.concurrency.downloads_semaphore.clone();

        stream::iter(projects)
            .map(|(name, index)| {
                let semaphore = semaphore.clone();
                async move { self.query_one(name, index, semaphore.as_ref()).await }
            })
            .buffer_unordered(self.concurrency.downloads)
            .filter_map(|finding| async move { finding })
            .collect()
            .await
    }

    async fn query_one(
        &self,
        name: &PackageName,
        index: &IndexUrl,
        semaphore: &Semaphore,
    ) -> Option<Finding> {
        let results = match self
            .client
            .simple_detail(
                name,
                Some(IndexMetadataRef::from(index)),
                self.capabilities,
                semaphore,
            )
            .await
        {
            Ok(results) => results,
            Err(err) => {
                trace!("Skipping project-status check for `{name}`: {err}");
                return None;
            }
        };

        let archive = results
            .into_iter()
            .map(|(_, format)| match format {
                MetadataFormat::Simple(archive) => archive,
                MetadataFormat::Flat(_) => {
                    unreachable!("Flat metadata should not be returned by `simple_detail`")
                }
            })
            .next()?;

        let project_status: PypiProjectStatus =
            match rkyv::deserialize::<PypiProjectStatus, rkyv::rancor::Error>(
                archive.project_status(),
            ) {
                Ok(project_status) => project_status,
                Err(err) => {
                    trace!("Failed to read archived project status for `{name}`: {err}");
                    return None;
                }
            };

        let status = to_adverse(project_status.status)?;
        let reason = project_status.reason.map(|reason| reason.to_string());
        Some(Finding::ProjectStatus(types::ProjectStatus {
            name: name.clone(),
            status,
            reason,
        }))
    }
}

/// Map a PEP 792 [`Status`] to its [`AdverseStatus`] counterpart, if any.
fn to_adverse(status: Status) -> Option<AdverseStatus> {
    match status {
        Status::Active => None,
        Status::Archived => Some(AdverseStatus::Archived),
        Status::Quarantined => Some(AdverseStatus::Quarantined),
        Status::Deprecated => Some(AdverseStatus::Deprecated),
    }
}
