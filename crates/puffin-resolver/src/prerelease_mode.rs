use rustc_hash::FxHashSet;

use pep508_rs::{Requirement, VersionOrUrl};
use uv_normalize::PackageName;

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
    pub(crate) fn from_mode(mode: PreReleaseMode, direct_dependencies: &[Requirement]) -> Self {
        match mode {
            PreReleaseMode::Disallow => Self::Disallow,
            PreReleaseMode::Allow => Self::Allow,
            PreReleaseMode::IfNecessary => Self::IfNecessary,
            PreReleaseMode::Explicit => Self::Explicit(
                direct_dependencies
                    .iter()
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
                direct_dependencies
                    .iter()
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
}
