//! Given a set of requirements, find a set of compatible packages.

use std::borrow::Cow;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use itertools::Itertools;
use tracing::{debug, trace, warn};

use uv_configuration::{BuildOptions, IndexStrategy, RequiredVersion, ResolutionPolicy};
use uv_distribution_types::{ConflictItem, ConflictItemRef, ConflictKind, ConflictKindRef, RequiresPython, RequiresPythonRange};
use uv_fs::Simplified;
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::{MarkerEnvironment, MarkerTree};
use uv_pypi_types::{ArtifactUrl, DistInfoMetadata, InstalledMetadataJson, InstalledWheelMetadata, PackageHash, PackageName, ProjectName, ProjectUrls, ResolverMarkerEnvironment, SupportedEnvironments};

use crate::pubgrub::{PubGrubDependency, PubGrubPackage};
use crate::resolver::ForkState;
use crate::universal_marker::{ConflictMarker, UniversalMarker};
use crate::{PythonRequirement, ResolveError};

fn policy_allows(version: &Version, policy: &ResolutionPolicy) -> bool {
    match policy {
        ResolutionPolicy::Only(v) => {
            if let Ok(spec_version) = v.parse::<Version>() { version == &spec_version } else { false }
        }
        ResolutionPolicy::Range(spec) => {
            if let Ok(specs) = spec.parse::<VersionSpecifiers>() { specs.contains(version) } else { true }
        }
    }
}

#[cfg(test)]
mod tests_policy {
    use super::*;

    #[test]
    fn allows_only_exact_match() {
        let v = "3.12.3".parse::<Version>().unwrap();
        assert!(policy_allows(&v, &ResolutionPolicy::Only("3.12.3".into())));
        assert!(!policy_allows(&v, &ResolutionPolicy::Only("3.12.4".into())));
    }

    #[test]
    fn allows_range_spec() {
        let v = "3.12.3".parse::<Version>().unwrap();
        assert!(policy_allows(&v, &ResolutionPolicy::Range(">=3.12,<3.13".into())));
        assert!(!policy_allows(&v, &ResolutionPolicy::Range(">=3.13".into())));
    }
}
