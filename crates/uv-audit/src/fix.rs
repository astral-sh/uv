use itertools::Itertools as _;
use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::{Operator, Version, VersionSpecifier, VersionSpecifierBuildError};
use uv_pep508::MarkerTree;

use crate::Vulnerability;

fn dep_version_as_requirement(
    name: PackageName,
    version: Version,
) -> Result<Requirement, VersionSpecifierBuildError> {
    Ok(Requirement {
        name,
        source: RequirementSource::Registry {
            specifier: VersionSpecifier::from_version(Operator::Equal, version)?.into(),
            index: None,
            conflict: None,
        },
        extras: Box::new([]),
        groups: Box::new([]),
        marker: MarkerTree::default(),
        origin: None,
    })
}

/// Gets the version to update for a specific dependency.
///
/// The caller should have already checked that all these vulnerabilities
/// apply to the same dependency
fn collect_update_version_for_one_dependency<'a>(
    package_name: &PackageName,
    vulnerabilities: impl Iterator<Item = &'a Vulnerability>,
) -> Option<Requirement> {
    vulnerabilities
        .filter_map(|vulnerability| {
            vulnerability
                .semver_compatible_fix()
                .ok()
                .flatten()
                .and_then(|fix| dep_version_as_requirement(package_name.clone(), fix.clone()).ok())
        })
        .max_by(|a, b| {
            a.source
                .version_specifiers()
                .cmp(&b.source.version_specifiers())
        })
}

/// Returns a list of semver-compatible requirements to update for each vulnerable dependency
pub fn get_fixable_dependencies(vulnerabilities: &[&Vulnerability]) -> Vec<Requirement> {
    let groups = vulnerabilities.iter().chunk_by(|vulnerability| {
        (
            vulnerability.dependency.name(),
            vulnerability.dependency.version(),
        )
    });

    groups
        .into_iter()
        .filter_map(|((name, _), vulnerabilities)| {
            collect_update_version_for_one_dependency(name, vulnerabilities.copied())
        })
        .collect()
}
