use std::str::FromStr;

use uv_distribution_filename::{SourceDistFilename, WheelFilename};
use uv_distribution_types::RemoteSource;
use uv_pep440::{Operator, Version, VersionSpecifier, VersionSpecifierBuildError};
use uv_pep508::PackageName;
use uv_pypi_types::RequirementSource;

use crate::resolver::ForkMap;
use crate::{DependencyMode, Manifest, ResolverMarkers};

/// A map of package names to their associated, required local versions across all forks.
#[derive(Debug, Default, Clone)]
pub(crate) struct Locals(ForkMap<Version>);

impl Locals {
    /// Determine the set of permitted local versions in the [`Manifest`].
    pub(crate) fn from_manifest(
        manifest: &Manifest,
        markers: &ResolverMarkers,
        dependencies: DependencyMode,
    ) -> Self {
        let mut locals = ForkMap::default();

        // Add all direct requirements and constraints. There's no need to look for conflicts,
        // since conflicts will be enforced by the solver.
        for requirement in manifest.requirements(markers, dependencies) {
            if let Some(local) = from_source(&requirement.source) {
                locals.add(&requirement, local);
            }
        }

        Self(locals)
    }

    /// Return a list of local versions that are compatible with a package in the given fork.
    pub(crate) fn get(
        &self,
        package_name: &PackageName,
        markers: &ResolverMarkers,
    ) -> Vec<&Version> {
        self.0.get(package_name, markers)
    }

    /// Given a specifier that may include the version _without_ a local segment, return a specifier
    /// that includes the local segment from the expected version.
    pub(crate) fn map(
        local: &Version,
        specifier: &VersionSpecifier,
    ) -> Result<VersionSpecifier, VersionSpecifierBuildError> {
        match specifier.operator() {
            Operator::Equal | Operator::EqualStar => {
                // Given `foo==1.0.0`, if the local version is `1.0.0+local`, map to
                // `foo==1.0.0+local`.
                //
                // This has the intended effect of allowing `1.0.0+local`.
                if is_compatible(local, specifier.version()) {
                    VersionSpecifier::from_version(Operator::Equal, local.clone())
                } else {
                    Ok(specifier.clone())
                }
            }
            Operator::NotEqual | Operator::NotEqualStar => {
                // Given `foo!=1.0.0`, if the local version is `1.0.0+local`, map to
                // `foo!=1.0.0+local`.
                //
                // This has the intended effect of disallowing `1.0.0+local`.
                //
                // There's no risk of accidentally including `foo @ 1.0.0` in the resolution, since
                // we _know_ `foo @ 1.0.0+local` is required and would therefore conflict.
                if is_compatible(local, specifier.version()) {
                    VersionSpecifier::from_version(Operator::NotEqual, local.clone())
                } else {
                    Ok(specifier.clone())
                }
            }
            Operator::LessThanEqual => {
                // Given `foo<=1.0.0`, if the local version is `1.0.0+local`, map to
                // `foo==1.0.0+local`.
                //
                // This has the intended effect of allowing `1.0.0+local`.
                //
                // Since `foo==1.0.0+local` is already required, we know that to satisfy
                // `foo<=1.0.0`, we _must_ satisfy `foo==1.0.0+local`. We _could_ map to
                // `foo<=1.0.0+local`, but local versions are _not_ allowed in exclusive ordered
                // specifiers, so introducing `foo<=1.0.0+local` would risk breaking invariants.
                if is_compatible(local, specifier.version()) {
                    VersionSpecifier::from_version(Operator::Equal, local.clone())
                } else {
                    Ok(specifier.clone())
                }
            }
            Operator::GreaterThan => {
                // Given `foo>1.0.0`, `foo @ 1.0.0+local` is already (correctly) disallowed.
                Ok(specifier.clone())
            }
            Operator::ExactEqual => {
                // Given `foo===1.0.0`, `1.0.0+local` is already (correctly) disallowed.
                Ok(specifier.clone())
            }
            Operator::TildeEqual => {
                // Given `foo~=1.0.0`, `foo~=1.0.0+local` is already (correctly) allowed.
                Ok(specifier.clone())
            }
            Operator::LessThan => {
                // Given `foo<1.0.0`, `1.0.0+local` is already (correctly) disallowed.
                Ok(specifier.clone())
            }
            Operator::GreaterThanEqual => {
                // Given `foo>=1.0.0`, `foo @ 1.0.0+local` is already (correctly) allowed.
                Ok(specifier.clone())
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

/// If a [`VersionSpecifier`] contains an exact equality specifier for a local version,
/// returns the local version.
pub(crate) fn from_source(source: &RequirementSource) -> Option<Version> {
    match source {
        // Extract all local versions from specifiers that require an exact version (e.g.,
        // `==1.0.0+local`).
        RequirementSource::Registry {
            specifier: version, ..
        } => version
            .iter()
            .filter(|specifier| {
                matches!(specifier.operator(), Operator::Equal | Operator::ExactEqual)
            })
            .filter(|specifier| !specifier.version().local().is_empty())
            .map(|specifier| specifier.version().clone())
            // It's technically possible for there to be multiple local segments here.
            // For example, `a==1.0+foo,==1.0+bar`. However, in that case resolution
            // will fail later.
            .next(),
        // Exact a local version from a URL, if it includes a fully-qualified filename (e.g.,
        // `torch-2.2.1%2Bcu118-cp311-cp311-linux_x86_64.whl`).
        RequirementSource::Url { url, .. } => url
            .filename()
            .ok()
            .and_then(|filename| {
                if let Ok(filename) = WheelFilename::from_str(&filename) {
                    Some(filename.version)
                } else if let Ok(filename) =
                    SourceDistFilename::parsed_normalized_filename(&filename)
                {
                    Some(filename.version)
                } else {
                    None
                }
            })
            .filter(uv_pep440::Version::is_local),
        RequirementSource::Git { .. } => None,
        RequirementSource::Path {
            install_path: path, ..
        } => path
            .file_name()
            .and_then(|filename| {
                let filename = filename.to_string_lossy();
                if let Ok(filename) = WheelFilename::from_str(&filename) {
                    Some(filename.version)
                } else if let Ok(filename) =
                    SourceDistFilename::parsed_normalized_filename(&filename)
                {
                    Some(filename.version)
                } else {
                    None
                }
            })
            .filter(uv_pep440::Version::is_local),
        RequirementSource::Directory { .. } => None,
    }
}

#[cfg(test)]
mod tests;
