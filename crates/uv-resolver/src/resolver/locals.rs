use rustc_hash::FxHashMap;

use pep440_rs::{Operator, Version, VersionSpecifier};
use pep508_rs::{MarkerEnvironment, VersionOrUrl};
use uv_normalize::PackageName;

use crate::Manifest;

#[derive(Debug, Default)]
pub(crate) struct Locals {
    /// A map of package names to their associated, required local versions.
    required: FxHashMap<PackageName, Version>,
}

impl Locals {
    /// Determine the set of permitted local versions in the [`Manifest`].
    pub(crate) fn from_manifest(manifest: &Manifest, markers: &MarkerEnvironment) -> Self {
        let mut required: FxHashMap<PackageName, Version> = FxHashMap::default();

        // Add all direct requirements and constraints. There's no need to look for conflicts,
        // since conflicting versions will be tracked upstream.
        for requirement in manifest
            .requirements
            .iter()
            .filter(|requirement| requirement.evaluate_markers(markers, &[]))
            .chain(
                manifest
                    .constraints
                    .iter()
                    .filter(|requirement| requirement.evaluate_markers(markers, &[])),
            )
            .chain(manifest.editables.iter().flat_map(|(editable, metadata)| {
                metadata
                    .requires_dist
                    .iter()
                    .filter(|requirement| requirement.evaluate_markers(markers, &editable.extras))
            }))
            .chain(
                manifest
                    .overrides
                    .iter()
                    .filter(|requirement| requirement.evaluate_markers(markers, &[])),
            )
        {
            if let Some(VersionOrUrl::VersionSpecifier(specifiers)) =
                requirement.version_or_url.as_ref()
            {
                for specifier in specifiers.iter() {
                    if let Some(version) = to_local(specifier) {
                        required.insert(requirement.name.clone(), version.clone());
                    }
                }
            }
        }

        Self { required }
    }

    /// Return the local [`Version`] to which a package is pinned, if any.
    pub(crate) fn get(&self, package: &PackageName) -> Option<&Version> {
        self.required.get(package)
    }

    /// Given a specifier that may include the version _without_ a local segment, return a specifier
    /// that includes the local segment from the expected version.
    pub(crate) fn map(local: &Version, specifier: &VersionSpecifier) -> VersionSpecifier {
        match specifier.operator() {
            Operator::Equal | Operator::EqualStar => {
                // Given `foo==1.0.0`, if the local version is `1.0.0+local`, map to
                // `foo==1.0.0+local`. This has the intended effect of allowing `1.0.0+local`.
                if is_compatible(local, specifier.version()) {
                    VersionSpecifier::new(Operator::Equal, local.clone())
                } else {
                    specifier.clone()
                }
            }
            Operator::NotEqual | Operator::NotEqualStar => {
                // Given `foo!=1.0.0`, if the local version is `1.0.0+local`, map to
                // `foo!=1.0.0+local`. This has the intended effect of disallowing `1.0.0+local`.
                // There's no risk of including `foo @ 1.0.0` in the resolution, since we _know_
                // `foo @ 1.0.0+local` is required and would conflict.
                if is_compatible(local, specifier.version()) {
                    VersionSpecifier::new(Operator::NotEqual, local.clone())
                } else {
                    specifier.clone()
                }
            }
            Operator::LessThanEqual => {
                // Given `foo<=1.0.0`, if the local version is `1.0.0+local`, map to
                // `foo<=1.0.0+local`. This has the intended effect of allowing `1.0.0+local`.
                // There's no risk of including `foo @ 1.0.0.post1` in the resolution, since we
                // _know_ `foo @ 1.0.0+local` is required and would conflict.
                if is_compatible(local, specifier.version()) {
                    VersionSpecifier::new(Operator::LessThanEqual, local.clone())
                } else {
                    specifier.clone()
                }
            }
            Operator::GreaterThan => {
                // Given `foo>1.0.0`, if the local version is `1.0.0+local`, map to
                // `foo>1.0.0+local`. This has the intended effect of disallowing `1.0.0+local`.
                if is_compatible(local, specifier.version()) {
                    VersionSpecifier::new(Operator::GreaterThan, local.clone())
                } else {
                    specifier.clone()
                }
            }
            Operator::ExactEqual => {
                // Given `foo===1.0.0`, `1.0.0+local` is already disallowed.
                specifier.clone()
            }
            Operator::TildeEqual => {
                // Given `foo~=1.0.0`, `foo~=1.0.0+local` is already allowed.
                specifier.clone()
            }
            Operator::LessThan => {
                // Given `foo<1.0.0`, `1.0.0+local` is already disallowed.
                specifier.clone()
            }
            Operator::GreaterThanEqual => {
                // Given `foo>=1.0.0`, `foo>1.0.0+local` is already allowed.
                specifier.clone()
            }
        }
    }
}

/// Returns `true` if a provided version is compatible with the expected local version.
///
/// The versions are compatible if they are the same including their local segment, or the
/// same except for the local segment, which is empty in the provided version.
///
/// For example, if the expected version is `1.0.0+local` and the provided version is `1.0.0+other`,
/// this function will return `false`.
///
/// If the expected version is `1.0.0+local` and the provided version is `1.0.0`, the function will
/// return `true`.
fn is_compatible(expected: &Version, provided: &Version) -> bool {
    // The requirements should be the same, ignoring local segments.
    if expected.clone().without_local() != provided.clone().without_local() {
        return false;
    }

    // If the provided version has a local segment, it should be the same as the expected
    // version.
    if provided.local().is_empty() {
        true
    } else {
        expected.local() == provided.local()
    }
}

/// If a [`VersionSpecifier`] represents exact equality against a local version, return the local
/// version.
fn to_local(specifier: &VersionSpecifier) -> Option<&Version> {
    if !matches!(specifier.operator(), Operator::Equal | Operator::ExactEqual) {
        return None;
    };

    if specifier.version().local().is_empty() {
        return None;
    }

    Some(specifier.version())
}
