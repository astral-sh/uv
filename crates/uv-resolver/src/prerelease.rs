use uv_distribution_types::RequirementSource;
use uv_normalize::PackageName;
use uv_pep440::{Operator, VersionSpecifiers};

use crate::resolver::ForkSet;
use crate::{DependencyMode, Manifest, ResolverEnvironment};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PrereleaseMode {
    /// Disallow all pre-release versions.
    Disallow,

    /// Allow all pre-release versions.
    Allow,

    /// Allow pre-release versions when no stable candidate satisfies the active constraints.
    IfNecessary,

    /// Allow pre-release versions when an active direct or transitive requirement contains an
    /// explicit pre-release marker.
    Explicit,

    /// Allow pre-release versions when no stable candidate satisfies the active constraints, or
    /// when an active direct or transitive requirement contains an explicit pre-release marker.
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

    /// Allow pre-release versions when no stable candidate satisfies the active constraints.
    IfNecessary,

    /// Allow pre-release versions when an active requirement contains an explicit pre-release
    /// marker.
    Explicit(ForkSet),

    /// Allow pre-release versions when no stable candidate satisfies the active constraints, or
    /// when an active requirement contains an explicit pre-release marker.
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
                for requirement in manifest.candidate_selection_requirements(env, dependencies) {
                    let RequirementSource::Registry { specifier, .. } = &requirement.source else {
                        continue;
                    };

                    if contains_prerelease(specifier) {
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

    /// Returns the pre-release candidate selection policy for a package.
    ///
    /// An explicit transitive dependency is represented by a PubGrub package, so its
    /// authorization is part of the partial solution and follows normal backtracking. When no
    /// explicit authorization is active, pre-releases remain in the candidate universe but are
    /// considered only after stable candidates. Keeping the candidate universe fixed is required
    /// for PubGrub's learned incompatibilities to remain valid.
    pub(crate) fn selection(
        &self,
        package_name: &PackageName,
        env: &ResolverEnvironment,
        explicit_prerelease: bool,
    ) -> PrereleaseSelection {
        match self {
            Self::Disallow => PrereleaseSelection::Disallow,
            Self::Allow => PrereleaseSelection::Allow,
            Self::IfNecessary => PrereleaseSelection::PreferStable,
            Self::Explicit(packages) => {
                if explicit_prerelease || packages.contains(package_name, env) {
                    PrereleaseSelection::Allow
                } else {
                    PrereleaseSelection::Disallow
                }
            }
            Self::IfNecessaryOrExplicit(packages) => {
                if explicit_prerelease || packages.contains(package_name, env) {
                    PrereleaseSelection::Allow
                } else {
                    PrereleaseSelection::PreferStable
                }
            }
        }
    }
}

/// Returns `true` if the specifiers explicitly mention a pre-release version.
///
/// Exclusions do not opt a package into pre-releases. For example, `!=1.0a1` should not change
/// which candidate kinds are considered.
pub(crate) fn contains_prerelease(specifiers: &VersionSpecifiers) -> bool {
    specifiers
        .iter()
        .filter(|specifier| {
            !matches!(
                specifier.operator(),
                Operator::NotEqual | Operator::NotEqualStar
            )
        })
        .any(uv_pep440::VersionSpecifier::any_prerelease)
}

/// How pre-release candidates participate in version selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrereleaseSelection {
    /// Do not consider pre-release candidates.
    Disallow,
    /// Consider stable and pre-release candidates in normal version order.
    Allow,
    /// Prefer stable candidates, falling back to pre-releases only after stable candidates are
    /// exhausted.
    PreferStable,
}
