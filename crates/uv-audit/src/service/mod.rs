//! Vulnerability services.

pub mod osv;
pub mod project_status;

/// The shape of the vulnerability service.
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum VulnerabilityServiceFormat {
    Osv,
}
