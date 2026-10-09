//! Commands for inspecting and modifying Python environments.

use uv_configuration::HashCheckingMode;

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
