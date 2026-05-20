//! `uv-audit` provides types and interfaces for auditing Python dependencies.

pub mod service;
pub mod types;
pub use service::ProjectStatusAudit;
pub use service::VulnerabilityServiceFormat;
pub use service::osv;
pub use types::{
    AdverseStatus, Dependency, Finding, ProjectStatus, Vulnerability, VulnerabilityID,
};
