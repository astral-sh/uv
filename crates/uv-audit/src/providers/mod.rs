use async_trait::async_trait;
use std::fmt;

use crate::{Result, VulnerabilityDatabase};

pub(crate) use self::osv::OsvSource;
pub(crate) use self::pypa::PypaSource;
pub(crate) use self::pypi::PypiSource;

pub(crate) mod osv;
mod pypa;
mod pypi;

/// Trait for vulnerability data sources
#[async_trait]
pub trait VulnerabilityProvider: Send + Sync {
    /// Name of the vulnerability source
    fn name(&self) -> &'static str;

    /// Fetch vulnerabilities for the given packages
    async fn fetch_vulnerabilities(
        &self,
        packages: &[(String, String)], // (name, version) pairs
    ) -> Result<VulnerabilityDatabase>;
}

/// Enum representing available vulnerability sources
pub enum VulnerabilitySource {
    /// `PyPA` Advisory Database (ZIP download)
    PypaZip(PypaSource),
    /// PyPI JSON API
    Pypi(PypiSource),
    /// OSV.dev batch API
    Osv(OsvSource),
}

impl VulnerabilitySource {
    /// Create a new vulnerability source from the CLI option
    pub fn new(
        source: uv_cli::VulnerabilitySource,
        cache: crate::AuditCache,
        no_cache: bool,
    ) -> Self {
        match source {
            uv_cli::VulnerabilitySource::PypaZip => {
                // PypaSource is now self-contained with direct PyPA parsing
                VulnerabilitySource::PypaZip(PypaSource::new(cache, no_cache))
            }
            uv_cli::VulnerabilitySource::Pypi => {
                // PypiSource directly queries PyPI API
                VulnerabilitySource::Pypi(PypiSource::new(cache, no_cache))
            }
            uv_cli::VulnerabilitySource::Osv => {
                // OsvSource directly queries OSV API
                VulnerabilitySource::Osv(OsvSource::new(cache, no_cache))
            }
        }
    }

    /// Get the name of the source
    pub fn name(&self) -> &'static str {
        match self {
            VulnerabilitySource::PypaZip(s) => s.name(),
            VulnerabilitySource::Pypi(s) => s.name(),
            VulnerabilitySource::Osv(s) => s.name(),
        }
    }

    /// Fetch vulnerabilities for the given packages
    pub async fn fetch_vulnerabilities(
        &self,
        packages: &[(String, String)],
    ) -> Result<VulnerabilityDatabase> {
        match self {
            VulnerabilitySource::PypaZip(s) => s.fetch_vulnerabilities(packages).await,
            VulnerabilitySource::Pypi(s) => s.fetch_vulnerabilities(packages).await,
            VulnerabilitySource::Osv(s) => s.fetch_vulnerabilities(packages).await,
        }
    }
}

impl fmt::Debug for VulnerabilitySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VulnerabilitySource({})", self.name())
    }
}
