use std::ops::{Deref, DerefMut};
use std::str::FromStr;

use rustc_hash::FxHashMap;
use uv_normalize::PackageName;

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
    pub fn is_empty(&self) -> bool {
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
