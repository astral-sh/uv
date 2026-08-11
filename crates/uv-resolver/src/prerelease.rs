use std::borrow::Cow;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

use rustc_hash::FxHashMap;
use uv_distribution_types::{Requirement, RequirementSource};
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

    /// Prefer stable versions, falling back to pre-release versions when necessary.
    #[default]
    IfNecessary,

    /// Prefer stable versions for first-party packages with explicit pre-release specifiers,
    /// falling back to pre-release versions when necessary. Disallow pre-release versions for all
    /// other packages.
    Explicit,

    /// Deprecated alias for `if-necessary`.
    #[deprecated(note = "use `if-necessary` instead")]
    IfNecessaryOrExplicit,
}

#[allow(deprecated)]
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

impl FromStr for PrereleaseMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "disallow" => Ok(Self::Disallow),
            "allow" => Ok(Self::Allow),
            "if-necessary" => Ok(Self::IfNecessary),
            "explicit" => Ok(Self::Explicit),
            #[allow(deprecated)]
            "if-necessary-or-explicit" => Ok(Self::IfNecessaryOrExplicit),
            _ => Err(format!(
                "expected one of `disallow`, `allow`, `if-necessary`, `explicit`, or `if-necessary-or-explicit`, found `{value}`"
            )),
        }
    }
}

/// A package-specific pre-release selection policy.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PrereleasePackageEntry {
    package: PackageName,
    mode: PrereleaseMode,
}

impl FromStr for PrereleasePackageEntry {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((package, mode)) = value.split_once('=') else {
            return Err(format!(
                "Invalid `prerelease-package` value `{value}`: expected format `PACKAGE=MODE`"
            ));
        };

        let package = PackageName::from_str(package).map_err(|err| {
            format!("Invalid `prerelease-package` package name `{package}`: {err}")
        })?;
        let mode = PrereleaseMode::from_str(mode)
            .map_err(|err| format!("Invalid `prerelease-package` mode: {err}"))?;

        Ok(Self { package, mode })
    }
}

/// Pre-release selection policies that apply to individual packages.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PrereleasePackage(FxHashMap<PackageName, PrereleaseMode>);

impl Deref for PrereleasePackage {
    type Target = FxHashMap<PackageName, PrereleaseMode>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PrereleasePackage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl FromIterator<PrereleasePackageEntry> for PrereleasePackage {
    fn from_iter<T: IntoIterator<Item = PrereleasePackageEntry>>(iter: T) -> Self {
        Self(
            iter.into_iter()
                .map(|entry| (entry.package, entry.mode))
                .collect(),
        )
    }
}

impl IntoIterator for PrereleasePackage {
    type Item = (PackageName, PrereleaseMode);
    type IntoIter = std::collections::hash_map::IntoIter<PackageName, PrereleaseMode>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a PrereleasePackage {
    type Item = (&'a PackageName, &'a PrereleaseMode);
    type IntoIter = std::collections::hash_map::Iter<'a, PackageName, PrereleaseMode>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl PrereleasePackage {
    /// Returns whether no package-specific policies are configured.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A pre-release selection policy that applies globally and to individual packages.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Prerelease {
    /// Global policy that applies to packages without a package-specific override.
    pub global: PrereleaseMode,
    /// Package-specific policies that override the global policy.
    pub package: PrereleasePackage,
}

impl Prerelease {
    /// Returns the effective pre-release selection policy for a package.
    pub fn mode(&self, package: &PackageName) -> PrereleaseMode {
        self.package.get(package).copied().unwrap_or(self.global)
    }
}

/// Like [`PrereleaseMode`], but with any additional information required to select a candidate,
/// like the set of direct dependencies.
#[derive(Debug, Clone)]
pub(crate) struct PrereleaseStrategy {
    default: PrereleasePolicy,
    package: FxHashMap<PackageName, PrereleasePolicy>,
}

#[derive(Debug, Clone)]
enum PrereleasePolicy {
    /// Disallow all pre-release versions.
    Disallow,

    /// Allow all pre-release versions.
    Allow,

    /// Prefer stable versions, falling back to pre-release versions when necessary.
    IfNecessary,

    /// Prefer stable versions for first-party packages with explicit pre-release specifiers,
    /// falling back to pre-release versions when necessary. Disallow pre-release versions for all
    /// other packages.
    Explicit(ForkSet),
}

impl PrereleaseStrategy {
    #[allow(deprecated)]
    pub(crate) fn from_prerelease(
        prerelease: &Prerelease,
        manifest: &Manifest,
        env: &ResolverEnvironment,
        dependencies: DependencyMode,
    ) -> Self {
        Self {
            default: Self::policy(prerelease.global, manifest, env, dependencies),
            package: prerelease
                .package
                .iter()
                .map(|(name, mode)| {
                    (
                        name.clone(),
                        Self::policy(*mode, manifest, env, dependencies),
                    )
                })
                .collect(),
        }
    }

    #[allow(deprecated)]
    fn policy(
        mode: PrereleaseMode,
        manifest: &Manifest,
        env: &ResolverEnvironment,
        dependencies: DependencyMode,
    ) -> PrereleasePolicy {
        match mode {
            PrereleaseMode::Disallow => PrereleasePolicy::Disallow,
            PrereleaseMode::Allow => PrereleasePolicy::Allow,
            PrereleaseMode::IfNecessary | PrereleaseMode::IfNecessaryOrExplicit => {
                PrereleasePolicy::IfNecessary
            }
            PrereleaseMode::Explicit => PrereleasePolicy::Explicit(Self::explicit_packages(
                manifest.candidate_selection_requirements(env, dependencies),
            )),
        }
    }

    fn explicit_packages<'a>(requirements: impl Iterator<Item = Cow<'a, Requirement>>) -> ForkSet {
        let mut packages = ForkSet::default();
        for requirement in requirements {
            let RequirementSource::Registry { specifier, .. } = &requirement.source else {
                continue;
            };

            if contains_prerelease(specifier) {
                packages.add(&requirement, ());
            }
        }
        packages
    }

    /// Returns the pre-release candidate selection policy for a package.
    ///
    /// Pre-releases remain in the candidate universe but, unless they are globally allowed, are
    /// considered only after stable candidates. Keeping the candidate universe fixed is required
    /// for PubGrub's learned incompatibilities to remain valid.
    pub(crate) fn selection(
        &self,
        package_name: &PackageName,
        env: &ResolverEnvironment,
    ) -> PrereleaseSelection {
        match self.package.get(package_name).unwrap_or(&self.default) {
            PrereleasePolicy::Disallow => PrereleaseSelection::Disallow,
            PrereleasePolicy::Allow => PrereleaseSelection::Allow,
            PrereleasePolicy::IfNecessary => PrereleaseSelection::PreferStable,
            PrereleasePolicy::Explicit(packages) => {
                if packages.contains(package_name, env) {
                    PrereleaseSelection::PreferStable
                } else {
                    PrereleaseSelection::Disallow
                }
            }
        }
    }
}

/// Returns `true` if the specifiers explicitly mention a pre-release version.
///
/// Exclusions do not opt a package into pre-releases. For example, `!=1.0a1` should not change
/// which candidate kinds are considered.
fn contains_prerelease(specifiers: &VersionSpecifiers) -> bool {
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
