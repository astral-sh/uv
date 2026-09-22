use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

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
    /// Dependency names whose constraints, overrides, exclusions, or candidate policy were consulted.
    #[serde(default)]
    pub requirements: BTreeSet<PackageName>,
    /// Parent packages whose dependency scopes were consulted.
    #[serde(default)]
    pub packages: BTreeSet<PackageName>,
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

/// A shared recorder for a single runtime resolution. Disabled recorders do no work.
///
/// Build resolutions use their own, disabled recorder, even when fetching runtime metadata invokes
/// a build backend. Clones share consultations across resolver forks and concurrent metadata requests.
#[derive(Debug, Default, Clone)]
pub struct ResolutionRecorder(Option<Arc<Mutex<ResolutionLookups>>>);

impl ResolutionRecorder {
    /// Enable recording for a new resolution.
    pub fn enabled() -> Self {
        Self(Some(Arc::default()))
    }

    /// Return whether recording is enabled.
    pub fn is_enabled(&self) -> bool {
        self.0.is_some()
    }

    /// Record a requirement lookup before applying configuration or filtering its markers.
    pub fn requirement(&self, name: &PackageName) {
        if let Some(lookups) = &self.0 {
            lookups
                .lock()
                .expect("resolution lookups lock poisoned")
                .requirements
                .insert(name.clone());
        }
    }

    /// Record a package scope lookup, even when the package has no dependencies or matching scope.
    pub fn package(&self, name: &PackageName) {
        if let Some(lookups) = &self.0 {
            lookups
                .lock()
                .expect("resolution lookups lock poisoned")
                .packages
                .insert(name.clone());
        }
    }

    /// Record a static metadata lookup before checking for a matching declaration.
    pub fn dependency_metadata(&self, name: &PackageName, version: Option<&Version>) {
        if let Some(lookups) = &self.0 {
            lookups
                .lock()
                .expect("resolution lookups lock poisoned")
                .dependency_metadata
                .insert(DependencyMetadataQuery {
                    name: name.clone(),
                    version: version.cloned(),
                });
        }
    }

    /// Snapshot the consultations once runtime resolution has completed.
    pub fn snapshot(&self) -> Option<ResolutionLookups> {
        self.0.as_ref().map(|lookups| {
            lookups
                .lock()
                .expect("resolution lookups lock poisoned")
                .clone()
        })
    }
}
