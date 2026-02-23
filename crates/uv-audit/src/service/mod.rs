//! Vulnerability services.

use crate::types::{Dependency, Finding};

pub mod osv;

/// Represents a vulnerability service, like OSV or PyPI's PYSEC.
#[async_trait::async_trait]
pub trait VulnerabilityService {
    /// The error type for this service. This will vary by service.
    type Error;

    /// Query the service for a single dependency, returning any findings.
    async fn query(&self, dependency: &Dependency) -> Result<Vec<Finding>, Self::Error>;

    /// Query the service for a batch of dependencies, returning any findings.
    ///
    /// This is a blanket implementation; individual services can override this with a more efficient
    /// implementation if they support batch queries.
    async fn query_batch(&self, dependencies: &[Dependency]) -> Result<Vec<Finding>, Self::Error> {
        let mut results = vec![];
        for dependency in dependencies {
            let findings = self.query(dependency).await?;
            results.extend(findings);
        }
        Ok(results)
    }
}
