//! `uv-audit` provides types and interfaces for auditing Python dependencies.

pub use service::ProjectStatusAudit;
pub use service::VulnerabilityServiceFormat;
pub use service::osv;
pub use types::{
    AdverseStatus, Dependency, Finding, ProjectStatus, Vulnerability, VulnerabilityID,
};

mod service;
mod types;
