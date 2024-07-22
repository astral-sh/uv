use pypi_types::RequirementSource;

use pep508_rs::MarkerEnvironment;
use uv_normalize::PackageName;

use crate::resolver::ForkSet;
use crate::{DependencyMode, Manifest, ResolverMarkers};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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

impl std::fmt::Display for PreReleaseMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disallow => write!(f, "disallow"),
            Self::Allow => write!(f, "allow"),
            Self::IfNecessary => write!(f, "if-necessary"),
            Self::Explicit => write!(f, "explicit"),
            Self::IfNecessaryOrExplicit => write!(f, "if-necessary-or-explicit"),
        }
    }
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
    Explicit(ForkSet),

    /// Allow pre-release versions if all versions of a package are pre-release, or if the package
    /// has an explicit pre-release marker in its version requirements.
    IfNecessaryOrExplicit(ForkSet),
}

impl PreReleaseStrategy {
    pub(crate) fn from_mode(
        mode: PreReleaseMode,
        manifest: &Manifest,
        markers: Option<&MarkerEnvironment>,
        dependencies: DependencyMode,
    ) -> Self {
        let mut packages = ForkSet::default();

        match mode {
            PreReleaseMode::Disallow => Self::Disallow,
            PreReleaseMode::Allow => Self::Allow,
            PreReleaseMode::IfNecessary => Self::IfNecessary,
            _ => {
                for requirement in manifest.requirements(markers, dependencies) {
                    let RequirementSource::Registry { specifier, .. } = &requirement.source else {
                        continue;
                    };

                    if specifier
                        .iter()
                        .any(pep440_rs::VersionSpecifier::any_prerelease)
                    {
                        packages.add(&requirement, ());
                    }
                }

                match mode {
                    PreReleaseMode::Explicit => Self::Explicit(packages),
                    PreReleaseMode::IfNecessaryOrExplicit => Self::IfNecessaryOrExplicit(packages),
                    _ => unreachable!(),
                }
            }
        }
    }

    /// Returns `true` if a [`PackageName`] is allowed to have pre-release versions.
    pub(crate) fn allows(
        &self,
        package_name: &PackageName,
        markers: &ResolverMarkers,
    ) -> AllowPreRelease {
        match self {
            PreReleaseStrategy::Disallow => AllowPreRelease::No,
            PreReleaseStrategy::Allow => AllowPreRelease::Yes,
            PreReleaseStrategy::IfNecessary => AllowPreRelease::IfNecessary,
            PreReleaseStrategy::Explicit(packages) => {
                if packages.contains(package_name, markers) {
                    AllowPreRelease::Yes
                } else {
                    AllowPreRelease::No
                }
            }
            PreReleaseStrategy::IfNecessaryOrExplicit(packages) => {
                if packages.contains(package_name, markers) {
                    AllowPreRelease::Yes
                } else {
                    AllowPreRelease::IfNecessary
                }
            }
        }
    }
}

/// The pre-release strategy for a given package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AllowPreRelease {
    /// Allow all pre-release versions.
    Yes,

    /// Disallow all pre-release versions.
    No,

    /// Allow pre-release versions if all versions of this package are pre-release.
    IfNecessary,
}
