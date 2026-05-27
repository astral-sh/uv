use std::collections::HashMap;

use itertools::Itertools as _;
use uv_pep440::Version;

use crate::{Dependency, Vulnerability};

/// Gets the version to update for a specific dependency
fn collect_updates_for_one_dependency<'a>(
    vulnerabilities: &[&'a Vulnerability],
) -> Option<(&'a Vulnerability, &'a Version)> {
    vulnerabilities
        .iter()
        .filter_map(|vulnerability| {
            vulnerability
                .semver_compatible_fix()
                .ok()
                .flatten()
                .map(|fix| (*vulnerability, fix))
        })
        .max_by_key(|(_, version)| *version)
}

/// Returns a map of version IDs to their suggested fix version
pub fn get_fixable_dependencies<'a>(
    vulnerabilities: &[&'a Vulnerability],
) -> HashMap<&'a Dependency, &'a Version> {
    let groups = vulnerabilities.iter().chunk_by(|vulnerability| {
        (
            vulnerability.dependency.name(),
            vulnerability.dependency.version(),
        )
    });

    groups
        .into_iter()
        .filter_map(|(_, vulnerabilities)| {
            collect_updates_for_one_dependency(&vulnerabilities.copied().collect::<Vec<_>>())
                .map(|(vulnerability, fix)| (&vulnerability.dependency, fix))
        })
        .collect()
}
