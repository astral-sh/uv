use uv_pep440::{Version, VersionSpecifierBuildError};

use crate::Vulnerability;

/// Collects a list of dependencies that can be updated to fix a vulnerability
pub fn collect_updates(
    vulnerabilities: &[&Vulnerability],
) -> Result<Option<Version>, VersionSpecifierBuildError> {
    Ok(vulnerabilities
        .iter()
        .filter_map(|vulnerability| vulnerability.semver_compatible_fix().ok().flatten())
        .max()
        .cloned())
}
