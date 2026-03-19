//! Vulnerability services.

pub mod osv;

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum VulnerabilityService {
    Osv,
}
