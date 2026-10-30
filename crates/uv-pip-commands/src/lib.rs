//! Commands for inspecting and modifying Python environments.

use uv_configuration::HashCheckingMode;
use uv_normalize::PackageName;
use uv_redacted::SafeUrl;

pub use pylock::PylockResolutionError;

pub mod check;
pub mod compile;
pub mod freeze;
pub mod install;
pub mod list;
pub mod show;
pub mod sync;
pub mod tree;
pub mod uninstall;

mod install_report;
mod pylock;
mod reporters;

/// A reconstructed requirement must remain readable after its credentials are removed.
fn check_requirement_url(name: &PackageName, url: &SafeUrl) -> anyhow::Result<()> {
    if url.is_ambiguous_without_credentials() {
        anyhow::bail!(
            "Cannot write a requirement for `{name}`: its URL path or fragment is ambiguous after removing credentials"
        );
    }
    Ok(())
}

/// Require build hashes independently of runtime checking. Otherwise, verify supplied build
/// hashes only when runtime checking is enabled.
fn resolve_build_hash_checking(
    hash_checking: Option<HashCheckingMode>,
    build_hash_checking: HashCheckingMode,
) -> Option<HashCheckingMode> {
    match build_hash_checking {
        HashCheckingMode::Require => Some(HashCheckingMode::Require),
        HashCheckingMode::Verify => hash_checking.map(|_| HashCheckingMode::Verify),
    }
}
