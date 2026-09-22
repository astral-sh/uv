use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use uv_normalize::PackageName;
use uv_pep440::Version;

/// Configuration lookups made by a runtime resolution, including unsuccessful lookups.
///
/// Consultations from discarded candidates and inactive marker branches are retained conservatively.
/// Complete package scopes are retained, so empty exact scopes continue to shadow fallback scopes.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ResolutionLookups {
    /// Dependency names whose global constraints were consulted.
    #[serde(default)]
    pub constraints: BTreeSet<PackageName>,
    /// Dependency names whose global overrides were consulted.
    #[serde(default)]
    pub overrides: BTreeSet<PackageName>,
    /// Dependency names whose global exclusions were consulted.
    #[serde(default)]
    pub exclusions: BTreeSet<PackageName>,
    /// Parent packages whose constraint scopes were consulted.
    #[serde(default)]
    pub scoped_constraints: BTreeSet<PackageName>,
    /// Parent packages whose override scopes were consulted.
    #[serde(default)]
    pub scoped_overrides: BTreeSet<PackageName>,
    /// Parent packages whose exclusion scopes were consulted.
    #[serde(default)]
    pub scoped_exclusions: BTreeSet<PackageName>,
    /// Names whose candidate policy was consulted. Constraints, overrides, and exclusions can
    /// contribute to this policy even when their package scopes are not selected.
    #[serde(default)]
    pub candidate_policy: BTreeSet<PackageName>,
    /// Names whose package version lists were requested, consulting package-specific upload cutoffs.
    #[serde(default)]
    pub exclude_newer: BTreeSet<PackageName>,
    /// Static metadata queries, including their version context.
    #[serde(default)]
    pub dependency_metadata: BTreeSet<DependencyMetadataQuery>,
}

/// A static metadata lookup. An absent version uses the direct-source lookup semantics.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DependencyMetadataQuery {
    pub name: PackageName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<Version>,
}

/// A shared recorder for a single runtime resolution.
///
/// Build resolutions have no recorder, even when fetching runtime metadata invokes a build backend.
/// Clones share consultations across resolver forks and concurrent metadata requests.
#[derive(Debug, Default, Clone)]
pub struct ResolutionRecorder(Arc<Mutex<ResolutionLookups>>);

impl ResolutionRecorder {
    /// Record a global constraint lookup, including misses.
    pub fn constraint(&self, name: &PackageName) {
        self.lookups().constraints.insert(name.clone());
    }

    /// Record a global override lookup, including misses.
    pub fn override_dependency(&self, name: &PackageName) {
        self.lookups().overrides.insert(name.clone());
    }

    /// Record a global exclusion lookup, including misses.
    pub fn exclusion(&self, name: &PackageName) {
        self.lookups().exclusions.insert(name.clone());
    }

    /// Record a lookup of constraints scoped to a parent package.
    pub fn scoped_constraint(&self, package: &PackageName) {
        self.lookups().scoped_constraints.insert(package.clone());
    }

    /// Record a lookup of overrides scoped to a parent package.
    pub fn scoped_override(&self, package: &PackageName) {
        self.lookups().scoped_overrides.insert(package.clone());
    }

    /// Record a lookup of exclusions scoped to a parent package.
    pub fn scoped_exclusion(&self, package: &PackageName) {
        self.lookups().scoped_exclusions.insert(package.clone());
    }

    /// Record a consultation of manifest-wide prerelease or yanked-version policy.
    pub fn candidate_policy(&self, name: &PackageName) {
        self.lookups().candidate_policy.insert(name.clone());
    }

    /// Record the global settings that determine allowed URLs and explicit indexes.
    pub fn source_policy(&self, name: &PackageName) {
        let mut lookups = self.lookups();
        lookups.constraints.insert(name.clone());
        lookups.overrides.insert(name.clone());
        lookups.exclusions.insert(name.clone());
    }

    /// Record a package version request before checking the version cache.
    pub fn exclude_newer(&self, name: &PackageName) {
        self.lookups().exclude_newer.insert(name.clone());
    }

    /// Record a static metadata lookup before checking for a matching declaration.
    pub fn dependency_metadata(&self, name: &PackageName, version: Option<&Version>) {
        self.lookups()
            .dependency_metadata
            .insert(DependencyMetadataQuery {
                name: name.clone(),
                version: version.cloned(),
            });
    }

    /// Snapshot the consultations once runtime resolution has completed.
    pub fn snapshot(&self) -> ResolutionLookups {
        self.lookups().clone()
    }

    /// Acquire the shared lookup sets.
    fn lookups(&self) -> MutexGuard<'_, ResolutionLookups> {
        self.0.lock().expect("resolution lookups lock poisoned")
    }
}
