use thiserror::Error;

pub use crate::cache::{AuditCache, DatabaseMetadata};
pub use crate::database::DatabaseManager;
pub use crate::matcher::{
    DatabaseStats, FixAnalysis, FixSuggestion, MatcherConfig, VulnerabilityMatcher,
};
pub use crate::osv::{OsvAdvisory, OsvClient};
pub use crate::pypa::{PypaAdvisory, PypaParser};
pub use crate::report::{AuditReport, AuditSummary, ReportGenerator};
pub use crate::sarif::SarifGenerator;
pub use crate::scanner::{DependencyScanner, DependencySource, DependencyStats, ScannedDependency};
pub use crate::vulnerability::{
    Severity, VersionRange, Vulnerability, VulnerabilityDatabase, VulnerabilityMatch,
};

mod cache;
mod database;
mod matcher;
mod osv;
mod pypa;
mod report;
mod sarif;
mod scanner;
mod vulnerability;

/// Errors that can occur during security auditing.
#[derive(Debug, Error)]
pub enum AuditError {
    #[error("Failed to download vulnerability database")]
    DatabaseDownload(#[from] reqwest::Error),

    #[error("Failed to parse OSV advisory: {0}")]
    AdvisoryParse(String, #[source] serde_json::Error),

    #[error("Failed to parse PyPA advisory: {0}")]
    PypaAdvisoryParse(String, #[source] serde_yaml::Error),

    #[error("No advisories found in database: {0}")]
    EmptyDatabase(String),

    #[error("Failed to read project dependencies")]
    DependencyRead(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("Failed to parse lock file")]
    LockFileParse(#[from] toml::de::Error),

    #[error("Cache operation failed")]
    Cache(#[from] std::io::Error),

    #[error("Cache not found: {0}")]
    CacheNotFound(String),

    #[error("Invalid version constraint: {0}")]
    InvalidVersion(String),

    #[error("No dependency information found. Run 'uv lock' to generate a lock file.")]
    NoDependencyInfo,

    #[error("Invalid dependency specification: {0}")]
    InvalidDependency(String),

    #[error("ZIP extraction failed")]
    ZipExtraction(#[from] async_zip::error::ZipError),

    #[error("String parsing failed")]
    StringParse(#[from] core::str::Utf8Error),

    #[error("Workspace discovery failed")]
    WorkspaceDiscovery(#[from] uv_workspace::WorkspaceError),

    #[error("JSON serialization/deserialization failed")]
    Json(#[from] serde_json::Error),

    #[error("Database integrity check failed: {0}")]
    DatabaseIntegrity(String),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, AuditError>;
