use uv_pypi_types::RequirementSource;

use crate::resolver::ForkSet;
use crate::{DependencyMode, Manifest, ResolverEnvironment};

use uv_normalize::PackageName;
use uv_pep440::Operator;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PrereleaseMode {
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

impl std::fmt::Display for PrereleaseMode {
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

/// Like [`PrereleaseMode`], but with any additional information required to select a candidate,
/// like the set of direct dependencies.
#[derive(Debug, Clone)]
pub(crate) enum PrereleaseStrategy {
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

impl PrereleaseStrategy {
    pub(crate) fn from_mode(
        mode: PrereleaseMode,
        manifest: &Manifest,
        env: &ResolverEnvironment,
        dependencies: DependencyMode,
    ) -> Self {
        let mut packages = ForkSet::default();

        match mode {
            PrereleaseMode::Disallow => Self::Disallow,
            PrereleaseMode::Allow => Self::Allow,
            PrereleaseMode::IfNecessary => Self::IfNecessary,
            _ => {
                for requirement in manifest.requirements(env, dependencies) {
                    let RequirementSource::Registry { specifier, .. } = &requirement.source else {
                        continue;
                    };

                    if specifier
                        .iter()
                        .filter(|spec| {
                            !matches!(spec.operator(), Operator::NotEqual | Operator::NotEqualStar)
                        })
                        .any(uv_pep440::VersionSpecifier::any_prerelease)
                    {
                        packages.add(&requirement, ());
                    }
                }

                match mode {
                    PrereleaseMode::Explicit => Self::Explicit(packages),
                    PrereleaseMode::IfNecessaryOrExplicit => Self::IfNecessaryOrExplicit(packages),
                    _ => unreachable!(),
                }
            }
        }
    }

    /// Returns `true` if a [`PackageName`] is allowed to have pre-release versions.
    pub(crate) fn allows(
        &self,
        package_name: &PackageName,
        env: &ResolverEnvironment,
    ) -> AllowPrerelease {
        match self {
            PrereleaseStrategy::Disallow => AllowPrerelease::No,
            PrereleaseStrategy::Allow => AllowPrerelease::Yes,
            PrereleaseStrategy::IfNecessary => AllowPrerelease::IfNecessary,
            PrereleaseStrategy::Explicit(packages) => {
                if packages.contains(package_name, env) {
                    AllowPrerelease::Yes
                } else {
                    AllowPrerelease::No
                }
            }
            PrereleaseStrategy::IfNecessaryOrExplicit(packages) => {
                if packages.contains(package_name, env) {
                    AllowPrerelease::Yes
                } else {
                    AllowPrerelease::IfNecessary
                }
            }
        }
    }
}

/// The pre-release strategy for a given package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AllowPrerelease {
    /// Allow all pre-release versions.
    Yes,

    /// Disallow all pre-release versions.
    No,

    /// Allow pre-release versions if all versions of this package are pre-release.
    IfNecessary,
}
