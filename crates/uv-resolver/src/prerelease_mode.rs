use rustc_hash::FxHashSet;

use pep508_rs::{MarkerEnvironment, VersionOrUrl};
use uv_normalize::PackageName;

use crate::Manifest;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum PreReleaseMode {
    /// Disallow all pre-release versions.
    Disallow,

    /// Allow all pre-release versions.
    Allow,

    /// Allow pre-release versions if all versions of a package are pre-release.
    IfNecessary,

    /// Allow pre-release versions for first-party packages with explicit pre-release markers in
    /// their version requirements.
    Explicit,

    /// Allow pre-release versions if all versions of a package are pre-release, or if the package
    /// has an explicit pre-release marker in its version requirements.
    #[default]
    IfNecessaryOrExplicit,
}

/// Like [`PreReleaseMode`], but with any additional information required to select a candidate,
/// like the set of direct dependencies.
#[derive(Debug, Clone)]
pub(crate) enum PreReleaseStrategy {
    /// Disallow all pre-release versions.
    Disallow,

    /// Allow all pre-release versions.
    Allow,

    /// Allow pre-release versions if all versions of a package are pre-release.
    IfNecessary,

    /// Allow pre-release versions for first-party packages with explicit pre-release markers in
    /// their version requirements.
    Explicit(FxHashSet<PackageName>),

    /// Allow pre-release versions if all versions of a package are pre-release, or if the package
    /// has an explicit pre-release marker in its version requirements.
    IfNecessaryOrExplicit(FxHashSet<PackageName>),
}

impl PreReleaseStrategy {
    pub(crate) fn from_mode(
        mode: PreReleaseMode,
        manifest: &Manifest,
        markers: &MarkerEnvironment,
    ) -> Self {
        match mode {
            PreReleaseMode::Disallow => Self::Disallow,
            PreReleaseMode::Allow => Self::Allow,
            PreReleaseMode::IfNecessary => Self::IfNecessary,
            PreReleaseMode::Explicit => Self::Explicit(
                manifest
                    .requirements
                    .iter()
                    .chain(manifest.constraints.iter())
                    .chain(manifest.overrides.iter())
                    .filter(|requirement| requirement.evaluate_markers(markers, &[]))
                    .chain(manifest.editables.iter().flat_map(|(editable, metadata)| {
                        metadata.requires_dist.iter().filter(|requirement| {
                            requirement.evaluate_markers(markers, &editable.extras)
                        })
                    }))
                    .filter(|requirement| {
                        let Some(version_or_url) = &requirement.version_or_url else {
                            return false;
                        };
                        let version_specifiers = match version_or_url {
                            VersionOrUrl::VersionSpecifier(version_specifiers) => {
                                version_specifiers
                            }
                            VersionOrUrl::Url(_) => return false,
                        };
                        version_specifiers
                            .iter()
                            .any(pep440_rs::VersionSpecifier::any_prerelease)
                    })
                    .map(|requirement| requirement.name.clone())
                    .collect(),
            ),
            PreReleaseMode::IfNecessaryOrExplicit => Self::IfNecessaryOrExplicit(
                manifest
                    .requirements
                    .iter()
                    .chain(manifest.constraints.iter())
                    .chain(manifest.overrides.iter())
                    .filter(|requirement| requirement.evaluate_markers(markers, &[]))
                    .chain(manifest.editables.iter().flat_map(|(editable, metadata)| {
                        metadata.requires_dist.iter().filter(|requirement| {
                            requirement.evaluate_markers(markers, &editable.extras)
                        })
                    }))
                    .filter(|requirement| {
                        let Some(version_or_url) = &requirement.version_or_url else {
                            return false;
                        };
                        let version_specifiers = match version_or_url {
                            VersionOrUrl::VersionSpecifier(version_specifiers) => {
                                version_specifiers
                            }
                            VersionOrUrl::Url(_) => return false,
                        };
                        version_specifiers
                            .iter()
                            .any(pep440_rs::VersionSpecifier::any_prerelease)
                    })
                    .map(|requirement| requirement.name.clone())
                    .collect(),
            ),
        }
    }

    /// Returns `true` if a [`PackageName`] is allowed to have pre-release versions.
    pub(crate) fn allows(&self, package: &PackageName) -> bool {
        match self {
            Self::Disallow => false,
            Self::Allow => true,
            Self::IfNecessary => false,
            Self::Explicit(packages) => packages.contains(package),
            Self::IfNecessaryOrExplicit(packages) => packages.contains(package),
        }
    }
}
