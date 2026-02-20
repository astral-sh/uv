//! Vulnerability services.

use crate::types::{Dependency, Finding};
use indexmap::IndexMap;

pub mod osv;

/// Represents a vulnerability service, like OSV or PyPI's PYSEC.
#[async_trait::async_trait]
pub trait VulnerabilityService {
    /// The error type for this service. This will vary by service.
    type Error;

    /// Query the service for a single dependency, returning any findings.
    async fn query<'a>(&self, dependency: &Dependency<'a>)
    -> Result<Vec<Finding<'a>>, Self::Error>;

    /// Query the service for a batch of dependencies, returning any findings.
    ///
    /// This is a blanket implementation; individual services can override this with a more efficient
    /// implementation if they support batch queries.
    async fn query_batch<'a>(
        &self,
        dependencies: &[Dependency<'a>],
    ) -> Result<IndexMap<Dependency<'a>, Vec<Finding<'a>>>, Self::Error> {
        let mut results = IndexMap::new();
        for dependency in dependencies {
            let findings = self.query(dependency).await?;
            results
                .entry(dependency.clone())
                .or_insert_with(Vec::new)
                .extend(findings);
        }
        Ok(results)
    }
}
