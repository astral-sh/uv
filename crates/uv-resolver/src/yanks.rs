use rustc_hash::{FxHashMap, FxHashSet};

use pep440_rs::Version;
use pep508_rs::MarkerEnvironment;
use uv_normalize::PackageName;

use crate::Manifest;

/// A set of package versions that are permitted, even if they're marked as yanked by the
/// relevant index.
#[derive(Debug, Default)]
pub(crate) struct AllowedYanks(FxHashMap<PackageName, FxHashSet<Version>>);

impl AllowedYanks {
    pub(crate) fn from_manifest(manifest: &Manifest, markers: &MarkerEnvironment) -> Self {
        let mut allowed_yanks = FxHashMap::<PackageName, FxHashSet<Version>>::default();
        for requirement in manifest
            .requirements
            .iter()
            .chain(manifest.constraints.iter())
            .chain(manifest.overrides.iter())
            .chain(manifest.preferences.iter())
            .filter(|requirement| requirement.evaluate_markers(markers, &[]))
            .chain(manifest.editables.iter().flat_map(|(editable, metadata)| {
                metadata
                    .requires_dist
                    .iter()
                    .filter(|requirement| requirement.evaluate_markers(markers, &editable.extras))
            }))
        {
            let Some(pep508_rs::VersionOrUrl::VersionSpecifier(specifiers)) =
                &requirement.version_or_url
            else {
                continue;
            };
            let [specifier] = specifiers.as_ref() else {
                continue;
            };
            if matches!(
                specifier.operator(),
                pep440_rs::Operator::Equal | pep440_rs::Operator::ExactEqual
            ) {
                allowed_yanks
                    .entry(requirement.name.clone())
                    .or_default()
                    .insert(specifier.version().clone());
            }
        }
        Self(allowed_yanks)
    }

    /// Returns `true` if the given package version is allowed, even if it's marked as yanked by
    /// the relevant index.
    pub(crate) fn allowed(&self, package_name: &PackageName, version: &Version) -> bool {
        self.0
            .get(package_name)
            .is_some_and(|allowed_yanks| allowed_yanks.contains(version))
    }
}
