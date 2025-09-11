use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, LazyLock};

use itertools::Itertools;
use jiff::Timestamp;
use owo_colors::OwoColorize;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serializer;
use toml_edit::{Array, ArrayOfTables, InlineTable, Item, Table, Value, value};
use tracing::debug;
use url::Url;

use uv_cache_key::RepositoryUrl;
use uv_configuration::{BuildOptions, Constraints, InstallTarget};
use uv_distribution::{DistributionDatabase, FlatRequiresDist};
use uv_distribution_filename::{
    BuildTag, DistExtension, ExtensionError, SourceDistExtension, WheelFilename,
};
use uv_distribution_types::{
    BuiltDist, DependencyMetadata, DirectUrlBuiltDist, DirectUrlSourceDist, DirectorySourceDist,
    Dist, DistributionMetadata, File, FileLocation, GitSourceDist, IndexLocations, IndexMetadata,
    IndexUrl, Name, PathBuiltDist, PathSourceDist, RegistryBuiltDist, RegistryBuiltWheel,
    RegistrySourceDist, RegistryVariantsJson, RemoteSource, Requirement, RequirementSource,
    RequiresPython, ResolvedDist, SimplifiedMarkerTree, StaticMetadata, ToUrlError, UrlString,
    VariantsJson,
};
use uv_fs::{PortablePath, PortablePathBuf, relative_to};
use uv_git::{RepositoryReference, ResolvedRepositoryReference};
use uv_git_types::{GitOid, GitReference, GitUrl, GitUrlParseError};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_pep508::{MarkerEnvironment, MarkerTree, VerbatimUrl, VerbatimUrlError, split_scheme};
use uv_platform_tags::{
    AbiTag, IncompatibleTag, LanguageTag, PlatformTag, TagCompatibility, TagPriority, Tags,
};
use uv_pypi_types::{
    ConflictKind, Conflicts, HashAlgorithm, HashDigest, HashDigests, Hashes, ParsedArchiveUrl,
    ParsedGitUrl,
};
use uv_redacted::DisplaySafeUrl;
use uv_small_str::SmallString;
use uv_types::{BuildContext, HashStrategy};
use uv_workspace::{Editability, WorkspaceMember};

use crate::fork_strategy::ForkStrategy;
pub(crate) use crate::lock::export::PylockTomlPackage;
pub use crate::lock::export::RequirementsTxtExport;
pub use crate::lock::export::{PylockToml, PylockTomlErrorKind};
pub use crate::lock::installable::Installable;
pub use crate::lock::map::PackageMap;
pub use crate::lock::tree::TreeDisplay;
use crate::resolution::{AnnotatedDist, ResolutionGraphNode};
use crate::universal_marker::{ConflictMarker, UniversalMarker};
use crate::{
    ExcludeNewer, ExcludeNewerTimestamp, InMemoryIndex, MetadataResponse, PrereleaseMode,
    ResolutionMode, ResolverOutput,
};

mod export;
mod installable;
mod map;
mod tree;

/// The current version of the lockfile format.
pub const VERSION: u32 = 1;

/// The current revision of the lockfile format.
const REVISION: u32 = 3;

static LINUX_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let pep508 = MarkerTree::from_str("os_name == 'posix' and sys_platform == 'linux'").unwrap();
    UniversalMarker::new(pep508, ConflictMarker::TRUE)
});
static WINDOWS_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let pep508 = MarkerTree::from_str("os_name == 'nt' and sys_platform == 'win32'").unwrap();
    UniversalMarker::new(pep508, ConflictMarker::TRUE)
});
static MAC_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let pep508 = MarkerTree::from_str("os_name == 'posix' and sys_platform == 'darwin'").unwrap();
    UniversalMarker::new(pep508, ConflictMarker::TRUE)
});
static ANDROID_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let pep508 = MarkerTree::from_str("sys_platform == 'android'").unwrap();
    UniversalMarker::new(pep508, ConflictMarker::TRUE)
});
static ARM_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let pep508 =
        MarkerTree::from_str("platform_machine == 'aarch64' or platform_machine == 'arm64' or platform_machine == 'ARM64'")
            .unwrap();
    UniversalMarker::new(pep508, ConflictMarker::TRUE)
});
static X86_64_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let pep508 =
        MarkerTree::from_str("platform_machine == 'x86_64' or platform_machine == 'amd64' or platform_machine == 'AMD64'")
            .unwrap();
    UniversalMarker::new(pep508, ConflictMarker::TRUE)
});
static X86_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let pep508 = MarkerTree::from_str(
        "platform_machine == 'i686' or platform_machine == 'i386' or platform_machine == 'win32' or platform_machine == 'x86'",
    )
    .unwrap();
    UniversalMarker::new(pep508, ConflictMarker::TRUE)
});
static LINUX_ARM_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let mut marker = *LINUX_MARKERS;
    marker.and(*ARM_MARKERS);
    marker
});
static LINUX_X86_64_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let mut marker = *LINUX_MARKERS;
    marker.and(*X86_64_MARKERS);
    marker
});
static LINUX_X86_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let mut marker = *LINUX_MARKERS;
    marker.and(*X86_MARKERS);
    marker
});
static WINDOWS_ARM_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let mut marker = *WINDOWS_MARKERS;
    marker.and(*ARM_MARKERS);
    marker
});
static WINDOWS_X86_64_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let mut marker = *WINDOWS_MARKERS;
    marker.and(*X86_64_MARKERS);
    marker
});
static WINDOWS_X86_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let mut marker = *WINDOWS_MARKERS;
    marker.and(*X86_MARKERS);
    marker
});
static MAC_ARM_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let mut marker = *MAC_MARKERS;
    marker.and(*ARM_MARKERS);
    marker
});
static MAC_X86_64_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let mut marker = *MAC_MARKERS;
    marker.and(*X86_64_MARKERS);
    marker
});
static MAC_X86_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let mut marker = *MAC_MARKERS;
    marker.and(*X86_MARKERS);
    marker
});
static ANDROID_ARM_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let mut marker = *ANDROID_MARKERS;
    marker.and(*ARM_MARKERS);
    marker
});
static ANDROID_X86_64_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let mut marker = *ANDROID_MARKERS;
    marker.and(*X86_64_MARKERS);
    marker
});
static ANDROID_X86_MARKERS: LazyLock<UniversalMarker> = LazyLock::new(|| {
    let mut marker = *ANDROID_MARKERS;
    marker.and(*X86_MARKERS);
    marker
});

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(try_from = "LockWire")]
pub struct Lock {
    /// The (major) version of the lockfile format.
    ///
    /// Changes to the major version indicate backwards- and forwards-incompatible changes to the
    /// lockfile format. A given uv version only supports a single major version of the lockfile
    /// format.
    ///
    /// In other words, a version of uv that supports version 2 of the lockfile format will not be
    /// able to read lockfiles generated under version 1 or 3.
    version: u32,
    /// The revision of the lockfile format.
    ///
    /// Changes to the revision indicate backwards-compatible changes to the lockfile format.
    /// In other words, versions of uv that only support revision 1 _will_ be able to read lockfiles
    /// with a revision greater than 1 (though they may ignore newer fields).
    revision: u32,
    /// If this lockfile was built from a forking resolution with non-identical forks, store the
    /// forks in the lockfile so we can recreate them in subsequent resolutions.
    fork_markers: Vec<UniversalMarker>,
    /// The conflicting groups/extras specified by the user.
    conflicts: Conflicts,
    /// The list of supported environments specified by the user.
    supported_environments: Vec<MarkerTree>,
    /// The list of required platforms specified by the user.
    required_environments: Vec<MarkerTree>,
    /// The range of supported Python versions.
    requires_python: RequiresPython,
    /// We discard the lockfile if these options don't match.
    options: ResolverOptions,
    /// The actual locked version and their metadata.
    packages: Vec<Package>,
    /// A map from package ID to index in `packages`.
    ///
    /// This can be used to quickly lookup the full package for any ID
    /// in this lock. For example, the dependencies for each package are
    /// listed as package IDs. This map can be used to find the full
    /// package for each such dependency.
    ///
    /// It is guaranteed that every package in this lock has an entry in
    /// this map, and that every dependency for every package has an ID
    /// that exists in this map. That is, there are no dependencies that don't
    /// have a corresponding locked package entry in the same lockfile.
    by_id: FxHashMap<PackageId, usize>,
    /// The input requirements to the resolution.
    manifest: ResolverManifest,
}

impl Lock {
    /// Initialize a [`Lock`] from a [`ResolverOutput`].
    pub fn from_resolution(resolution: &ResolverOutput, root: &Path) -> Result<Self, LockError> {
        let mut packages = BTreeMap::new();
        let requires_python = resolution.requires_python.clone();

        // Determine the set of packages included at multiple versions.
        let mut seen = FxHashSet::default();
        let mut duplicates = FxHashSet::default();
        for node_index in resolution.graph.node_indices() {
            let ResolutionGraphNode::Dist(dist) = &resolution.graph[node_index] else {
                continue;
            };
            if !dist.is_base() {
                continue;
            }
            if !seen.insert(dist.name()) {
                duplicates.insert(dist.name());
            }
        }

        // Lock all base packages.
        for node_index in resolution.graph.node_indices() {
            let ResolutionGraphNode::Dist(dist) = &resolution.graph[node_index] else {
                continue;
            };
            if !dist.is_base() {
                continue;
            }

            // If there are multiple distributions for the same package, include the markers of all
            // forks that included the current distribution.
            let fork_markers = if duplicates.contains(dist.name()) {
                resolution
                    .fork_markers
                    .iter()
                    .filter(|fork_markers| !fork_markers.is_disjoint(dist.marker))
                    .copied()
                    .collect()
            } else {
                vec![]
            };

            let mut package = Package::from_annotated_dist(dist, fork_markers, root)?;
            Self::remove_unreachable_wheels(resolution, &requires_python, node_index, &mut package);

            // Add all dependencies
            for edge in resolution.graph.edges(node_index) {
                let ResolutionGraphNode::Dist(dependency_dist) = &resolution.graph[edge.target()]
                else {
                    continue;
                };
                let marker = *edge.weight();
                package.add_dependency(&requires_python, dependency_dist, marker, root)?;
            }

            let id = package.id.clone();
            if let Some(locked_dist) = packages.insert(id, package) {
                return Err(LockErrorKind::DuplicatePackage {
                    id: locked_dist.id.clone(),
                }
                .into());
            }
        }

        // Lock all extras and development dependencies.
        for node_index in resolution.graph.node_indices() {
            let ResolutionGraphNode::Dist(dist) = &resolution.graph[node_index] else {
                continue;
            };
            if let Some(extra) = dist.extra.as_ref() {
                let id = PackageId::from_annotated_dist(dist, root)?;
                let Some(package) = packages.get_mut(&id) else {
                    return Err(LockErrorKind::MissingExtraBase {
                        id,
                        extra: extra.clone(),
                    }
                    .into());
                };
                for edge in resolution.graph.edges(node_index) {
                    let ResolutionGraphNode::Dist(dependency_dist) =
                        &resolution.graph[edge.target()]
                    else {
                        continue;
                    };
                    let marker = *edge.weight();
                    package.add_optional_dependency(
                        &requires_python,
                        extra.clone(),
                        dependency_dist,
                        marker,
                        root,
                    )?;
                }
            }
            if let Some(group) = dist.group.as_ref() {
                let id = PackageId::from_annotated_dist(dist, root)?;
                let Some(package) = packages.get_mut(&id) else {
                    return Err(LockErrorKind::MissingDevBase {
                        id,
                        group: group.clone(),
                    }
                    .into());
                };
                for edge in resolution.graph.edges(node_index) {
                    let ResolutionGraphNode::Dist(dependency_dist) =
                        &resolution.graph[edge.target()]
                    else {
                        continue;
                    };
                    let marker = *edge.weight();
                    package.add_group_dependency(
                        &requires_python,
                        group.clone(),
                        dependency_dist,
                        marker,
                        root,
                    )?;
                }
            }
        }

        let packages = packages.into_values().collect();
        let (exclude_newer, exclude_newer_package) = {
            let exclude_newer = &resolution.options.exclude_newer;
            let global_exclude_newer = exclude_newer.global;
            let package_exclude_newer = if exclude_newer.package.is_empty() {
                None
            } else {
                Some(exclude_newer.package.clone().into_inner())
            };
            (global_exclude_newer, package_exclude_newer)
        };

        let options = ResolverOptions {
            resolution_mode: resolution.options.resolution_mode,
            prerelease_mode: resolution.options.prerelease_mode,
            fork_strategy: resolution.options.fork_strategy,
            exclude_newer,
            exclude_newer_package,
        };
        let lock = Self::new(
            VERSION,
            REVISION,
            packages,
            requires_python,
            options,
            ResolverManifest::default(),
            Conflicts::empty(),
            vec![],
            vec![],
            resolution.fork_markers.clone(),
        )?;
        Ok(lock)
    }

    /// Remove wheels that can't be selected for installation due to environment markers.
    ///
    /// For example, a package included under `sys_platform == 'win32'` does not need Linux
    /// wheels.
    fn remove_unreachable_wheels(
        graph: &ResolverOutput,
        requires_python: &RequiresPython,
        node_index: NodeIndex,
        locked_dist: &mut Package,
    ) {
        // Remove wheels that don't match `requires-python` and can't be selected for installation.
        locked_dist
            .wheels
            .retain(|wheel| requires_python.matches_wheel_tag(&wheel.filename));

        // Filter by platform tags.
        locked_dist.wheels.retain(|wheel| {
            // Naively, we'd check whether `platform_system == 'Linux'` is disjoint, or
            // `os_name == 'posix'` is disjoint, or `sys_platform == 'linux'` is disjoint (each on its
            // own sufficient to exclude linux wheels), but due to
            // `(A ∩ (B ∩ C) = ∅) => ((A ∩ B = ∅) or (A ∩ C = ∅))`
            // a single disjointness check with the intersection is sufficient, so we have one
            // constant per platform.
            let platform_tags = wheel.filename.platform_tags();

            if platform_tags.iter().all(PlatformTag::is_any) {
                return true;
            }

            if platform_tags.iter().all(PlatformTag::is_linux) {
                if platform_tags.iter().all(PlatformTag::is_arm) {
                    if graph.graph[node_index]
                        .marker()
                        .is_disjoint(*LINUX_ARM_MARKERS)
                    {
                        return false;
                    }
                } else if platform_tags.iter().all(PlatformTag::is_x86_64) {
                    if graph.graph[node_index]
                        .marker()
                        .is_disjoint(*LINUX_X86_64_MARKERS)
                    {
                        return false;
                    }
                } else if platform_tags.iter().all(PlatformTag::is_x86) {
                    if graph.graph[node_index]
                        .marker()
                        .is_disjoint(*LINUX_X86_MARKERS)
                    {
                        return false;
                    }
                } else if graph.graph[node_index].marker().is_disjoint(*LINUX_MARKERS) {
                    return false;
                }
            }

            if platform_tags.iter().all(PlatformTag::is_windows) {
                if platform_tags.iter().all(PlatformTag::is_arm) {
                    if graph.graph[node_index]
                        .marker()
                        .is_disjoint(*WINDOWS_ARM_MARKERS)
                    {
                        return false;
                    }
                } else if platform_tags.iter().all(PlatformTag::is_x86_64) {
                    if graph.graph[node_index]
                        .marker()
                        .is_disjoint(*WINDOWS_X86_64_MARKERS)
                    {
                        return false;
                    }
                } else if platform_tags.iter().all(PlatformTag::is_x86) {
                    if graph.graph[node_index]
                        .marker()
                        .is_disjoint(*WINDOWS_X86_MARKERS)
                    {
                        return false;
                    }
                } else if graph.graph[node_index]
                    .marker()
                    .is_disjoint(*WINDOWS_MARKERS)
                {
                    return false;
                }
            }

            if platform_tags.iter().all(PlatformTag::is_macos) {
                if platform_tags.iter().all(PlatformTag::is_arm) {
                    if graph.graph[node_index]
                        .marker()
                        .is_disjoint(*MAC_ARM_MARKERS)
                    {
                        return false;
                    }
                } else if platform_tags.iter().all(PlatformTag::is_x86_64) {
                    if graph.graph[node_index]
                        .marker()
                        .is_disjoint(*MAC_X86_64_MARKERS)
                    {
                        return false;
                    }
                } else if platform_tags.iter().all(PlatformTag::is_x86) {
                    if graph.graph[node_index]
                        .marker()
                        .is_disjoint(*MAC_X86_MARKERS)
                    {
                        return false;
                    }
                } else if graph.graph[node_index].marker().is_disjoint(*MAC_MARKERS) {
                    return false;
                }
            }

            if platform_tags.iter().all(PlatformTag::is_android) {
                if platform_tags.iter().all(PlatformTag::is_arm) {
                    if graph.graph[node_index]
                        .marker()
                        .is_disjoint(*ANDROID_ARM_MARKERS)
                    {
                        return false;
                    }
                } else if platform_tags.iter().all(PlatformTag::is_x86_64) {
                    if graph.graph[node_index]
                        .marker()
                        .is_disjoint(*ANDROID_X86_64_MARKERS)
                    {
                        return false;
                    }
                } else if platform_tags.iter().all(PlatformTag::is_x86) {
                    if graph.graph[node_index]
                        .marker()
                        .is_disjoint(*ANDROID_X86_MARKERS)
                    {
                        return false;
                    }
                } else if graph.graph[node_index]
                    .marker()
                    .is_disjoint(*ANDROID_MARKERS)
                {
                    return false;
                }
            }

            if platform_tags.iter().all(PlatformTag::is_arm) {
                if graph.graph[node_index].marker().is_disjoint(*ARM_MARKERS) {
                    return false;
                }
            }

            if platform_tags.iter().all(PlatformTag::is_x86_64) {
                if graph.graph[node_index]
                    .marker()
                    .is_disjoint(*X86_64_MARKERS)
                {
                    return false;
                }
            }

            if platform_tags.iter().all(PlatformTag::is_x86) {
                if graph.graph[node_index].marker().is_disjoint(*X86_MARKERS) {
                    return false;
                }
            }

            true
        });
    }

    /// Initialize a [`Lock`] from a list of [`Package`] entries.
    fn new(
        version: u32,
        revision: u32,
        mut packages: Vec<Package>,
        requires_python: RequiresPython,
        options: ResolverOptions,
        manifest: ResolverManifest,
        conflicts: Conflicts,
        supported_environments: Vec<MarkerTree>,
        required_environments: Vec<MarkerTree>,
        fork_markers: Vec<UniversalMarker>,
    ) -> Result<Self, LockError> {
        // Put all dependencies for each package in a canonical order and
        // check for duplicates.
        for package in &mut packages {
            package.dependencies.sort();
            for windows in package.dependencies.windows(2) {
                let (dep1, dep2) = (&windows[0], &windows[1]);
                if dep1 == dep2 {
                    return Err(LockErrorKind::DuplicateDependency {
                        id: package.id.clone(),
                        dependency: dep1.clone(),
                    }
                    .into());
                }
            }

            // Perform the same validation for optional dependencies.
            for (extra, dependencies) in &mut package.optional_dependencies {
                dependencies.sort();
                for windows in dependencies.windows(2) {
                    let (dep1, dep2) = (&windows[0], &windows[1]);
                    if dep1 == dep2 {
                        return Err(LockErrorKind::DuplicateOptionalDependency {
                            id: package.id.clone(),
                            extra: extra.clone(),
                            dependency: dep1.clone(),
                        }
                        .into());
                    }
                }
            }

            // Perform the same validation for dev dependencies.
            for (group, dependencies) in &mut package.dependency_groups {
                dependencies.sort();
                for windows in dependencies.windows(2) {
                    let (dep1, dep2) = (&windows[0], &windows[1]);
                    if dep1 == dep2 {
                        return Err(LockErrorKind::DuplicateDevDependency {
                            id: package.id.clone(),
                            group: group.clone(),
                            dependency: dep1.clone(),
                        }
                        .into());
                    }
                }
            }
        }
        packages.sort_by(|dist1, dist2| dist1.id.cmp(&dist2.id));

        // Check for duplicate package IDs and also build up the map for
        // packages keyed by their ID.
        let mut by_id = FxHashMap::default();
        for (i, dist) in packages.iter().enumerate() {
            if by_id.insert(dist.id.clone(), i).is_some() {
                return Err(LockErrorKind::DuplicatePackage {
                    id: dist.id.clone(),
                }
                .into());
            }
        }

        // Build up a map from ID to extras.
        let mut extras_by_id = FxHashMap::default();
        for dist in &packages {
            for extra in dist.optional_dependencies.keys() {
                extras_by_id
                    .entry(dist.id.clone())
                    .or_insert_with(FxHashSet::default)
                    .insert(extra.clone());
            }
        }

        // Remove any non-existent extras (e.g., extras that were requested but don't exist).
        for dist in &mut packages {
            for dep in dist
                .dependencies
                .iter_mut()
                .chain(dist.optional_dependencies.values_mut().flatten())
                .chain(dist.dependency_groups.values_mut().flatten())
            {
                dep.extra.retain(|extra| {
                    extras_by_id
                        .get(&dep.package_id)
                        .is_some_and(|extras| extras.contains(extra))
                });
            }
        }

        // Check that every dependency has an entry in `by_id`. If any don't,
        // it implies we somehow have a dependency with no corresponding locked
        // package.
        for dist in &packages {
            for dep in &dist.dependencies {
                if !by_id.contains_key(&dep.package_id) {
                    return Err(LockErrorKind::UnrecognizedDependency {
                        id: dist.id.clone(),
                        dependency: dep.clone(),
                    }
                    .into());
                }
            }

            // Perform the same validation for optional dependencies.
            for dependencies in dist.optional_dependencies.values() {
                for dep in dependencies {
                    if !by_id.contains_key(&dep.package_id) {
                        return Err(LockErrorKind::UnrecognizedDependency {
                            id: dist.id.clone(),
                            dependency: dep.clone(),
                        }
                        .into());
                    }
                }
            }

            // Perform the same validation for dev dependencies.
            for dependencies in dist.dependency_groups.values() {
                for dep in dependencies {
                    if !by_id.contains_key(&dep.package_id) {
                        return Err(LockErrorKind::UnrecognizedDependency {
                            id: dist.id.clone(),
                            dependency: dep.clone(),
                        }
                        .into());
                    }
                }
            }

            // Also check that our sources are consistent with whether we have
            // hashes or not.
            if let Some(requires_hash) = dist.id.source.requires_hash() {
                for wheel in &dist.wheels {
                    if requires_hash != wheel.hash.is_some() {
                        return Err(LockErrorKind::Hash {
                            id: dist.id.clone(),
                            artifact_type: "wheel",
                            expected: requires_hash,
                        }
                        .into());
                    }
                }
            }
        }
        let lock = Self {
            version,
            revision,
            fork_markers,
            conflicts,
            supported_environments,
            required_environments,
            requires_python,
            options,
            packages,
            by_id,
            manifest,
        };
        Ok(lock)
    }

    /// Record the requirements that were used to generate this lock.
    #[must_use]
    pub fn with_manifest(mut self, manifest: ResolverManifest) -> Self {
        self.manifest = manifest;
        self
    }

    /// Record the conflicting groups that were used to generate this lock.
    #[must_use]
    pub fn with_conflicts(mut self, conflicts: Conflicts) -> Self {
        self.conflicts = conflicts;
        self
    }

    /// Record the supported environments that were used to generate this lock.
    #[must_use]
    pub fn with_supported_environments(mut self, supported_environments: Vec<MarkerTree>) -> Self {
        // We "complexify" the markers given, since the supported
        // environments given might be coming directly from what's written in
        // `pyproject.toml`, and those are assumed to be simplified (i.e.,
        // they assume `requires-python` is true). But a `Lock` always uses
        // non-simplified markers internally, so we need to re-complexify them
        // here.
        //
        // The nice thing about complexifying is that it's a no-op if the
        // markers given have already been complexified.
        self.supported_environments = supported_environments
            .into_iter()
            .map(|marker| self.requires_python.complexify_markers(marker))
            .collect();
        self
    }

    /// Record the required platforms that were used to generate this lock.
    #[must_use]
    pub fn with_required_environments(mut self, required_environments: Vec<MarkerTree>) -> Self {
        self.required_environments = required_environments
            .into_iter()
            .map(|marker| self.requires_python.complexify_markers(marker))
            .collect();
        self
    }

    /// Returns `true` if this [`Lock`] includes `provides-extra` metadata.
    pub fn supports_provides_extra(&self) -> bool {
        // `provides-extra` was added in Version 1 Revision 1.
        (self.version(), self.revision()) >= (1, 1)
    }

    /// Returns `true` if this [`Lock`] includes entries for empty `dependency-group` metadata.
    pub fn includes_empty_groups(&self) -> bool {
        // Empty dependency groups are included as of https://github.com/astral-sh/uv/pull/8598,
        // but Version 1 Revision 1 is the first revision published after that change.
        (self.version(), self.revision()) >= (1, 1)
    }

    /// Returns the lockfile version.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Returns the lockfile revision.
    pub fn revision(&self) -> u32 {
        self.revision
    }

    /// Returns the number of packages in the lockfile.
    pub fn len(&self) -> usize {
        self.packages.len()
    }

    /// Returns `true` if the lockfile contains no packages.
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }

    /// Returns the [`Package`] entries in this lock.
    pub fn packages(&self) -> &[Package] {
        &self.packages
    }

    /// Returns the supported Python version range for the lockfile, if present.
    pub fn requires_python(&self) -> &RequiresPython {
        &self.requires_python
    }

    /// Returns the resolution mode used to generate this lock.
    pub fn resolution_mode(&self) -> ResolutionMode {
        self.options.resolution_mode
    }

    /// Returns the pre-release mode used to generate this lock.
    pub fn prerelease_mode(&self) -> PrereleaseMode {
        self.options.prerelease_mode
    }

    /// Returns the multi-version mode used to generate this lock.
    pub fn fork_strategy(&self) -> ForkStrategy {
        self.options.fork_strategy
    }

    /// Returns the exclude newer setting used to generate this lock.
    pub fn exclude_newer(&self) -> ExcludeNewer {
        self.options.exclude_newer()
    }

    /// Returns the conflicting groups that were used to generate this lock.
    pub fn conflicts(&self) -> &Conflicts {
        &self.conflicts
    }

    /// Returns the supported environments that were used to generate this lock.
    pub fn supported_environments(&self) -> &[MarkerTree] {
        &self.supported_environments
    }

    /// Returns the required platforms that were used to generate this lock.
    pub fn required_environments(&self) -> &[MarkerTree] {
        &self.required_environments
    }

    /// Returns the workspace members that were used to generate this lock.
    pub fn members(&self) -> &BTreeSet<PackageName> {
        &self.manifest.members
    }

    /// Returns requirements provided to the resolver, exclusive of the workspace members.
    ///
    /// These are requirements that are attached to the project, but not to any of its
    /// workspace members. For example, the requirements in a PEP 723 script would be included here.
    pub fn requirements(&self) -> &BTreeSet<Requirement> {
        &self.manifest.requirements
    }

    /// Returns the dependency groups that were used to generate this lock.
    pub fn dependency_groups(&self) -> &BTreeMap<GroupName, BTreeSet<Requirement>> {
        &self.manifest.dependency_groups
    }

    /// Returns the build constraints that were used to generate this lock.
    pub fn build_constraints(&self, root: &Path) -> Constraints {
        Constraints::from_requirements(
            self.manifest
                .build_constraints
                .iter()
                .cloned()
                .map(|requirement| requirement.to_absolute(root)),
        )
    }

    /// Return the workspace root used to generate this lock.
    pub fn root(&self) -> Option<&Package> {
        self.packages.iter().find(|package| {
            let (Source::Editable(path) | Source::Virtual(path)) = &package.id.source else {
                return false;
            };
            path.as_ref() == Path::new("")
        })
    }

    /// Returns the supported environments that were used to generate this
    /// lock.
    ///
    /// The markers returned here are "simplified" with respect to the lock
    /// file's `requires-python` setting. This means these should only be used
    /// for direct comparison purposes with the supported environments written
    /// by a human in `pyproject.toml`. (Think of "supported environments" in
    /// `pyproject.toml` as having an implicit `and python_full_version >=
    /// '{requires-python-bound}'` attached to each one.)
    pub fn simplified_supported_environments(&self) -> Vec<MarkerTree> {
        self.supported_environments()
            .iter()
            .copied()
            .map(|marker| self.simplify_environment(marker))
            .collect()
    }

    /// Returns the required platforms that were used to generate this
    /// lock.
    pub fn simplified_required_environments(&self) -> Vec<MarkerTree> {
        self.required_environments()
            .iter()
            .copied()
            .map(|marker| self.simplify_environment(marker))
            .collect()
    }

    /// Simplify the given marker environment with respect to the lockfile's
    /// `requires-python` setting.
    pub fn simplify_environment(&self, marker: MarkerTree) -> MarkerTree {
        self.requires_python.simplify_markers(marker)
    }

    /// If this lockfile was built from a forking resolution with non-identical forks, return the
    /// markers of those forks, otherwise `None`.
    pub fn fork_markers(&self) -> &[UniversalMarker] {
        self.fork_markers.as_slice()
    }

    /// Checks whether the fork markers cover the entire supported marker space.
    ///
    /// Returns the actually covered and the expected marker space on validation error.
    pub fn check_marker_coverage(&self) -> Result<(), (MarkerTree, MarkerTree)> {
        let fork_markers_union = if self.fork_markers().is_empty() {
            self.requires_python.to_marker_tree()
        } else {
            let mut fork_markers_union = MarkerTree::FALSE;
            for fork_marker in self.fork_markers() {
                fork_markers_union.or(fork_marker.pep508());
            }
            fork_markers_union
        };
        let mut environments_union = if !self.supported_environments.is_empty() {
            let mut environments_union = MarkerTree::FALSE;
            for fork_marker in &self.supported_environments {
                environments_union.or(*fork_marker);
            }
            environments_union
        } else {
            MarkerTree::TRUE
        };
        // When a user defines environments, they are implicitly constrained by requires-python.
        environments_union.and(self.requires_python.to_marker_tree());
        if fork_markers_union.negate().is_disjoint(environments_union) {
            Ok(())
        } else {
            Err((fork_markers_union, environments_union))
        }
    }

    /// Checks whether the new requires-python specification is disjoint with
    /// the fork markers in this lock file.
    ///
    /// If they are disjoint, then the union of the fork markers along with the
    /// given requires-python specification (converted to a marker tree) are
    /// returned.
    ///
    /// When disjoint, the fork markers in the lock file should be dropped and
    /// not used.
    pub fn requires_python_coverage(
        &self,
        new_requires_python: &RequiresPython,
    ) -> Result<(), (MarkerTree, MarkerTree)> {
        let fork_markers_union = if self.fork_markers().is_empty() {
            self.requires_python.to_marker_tree()
        } else {
            let mut fork_markers_union = MarkerTree::FALSE;
            for fork_marker in self.fork_markers() {
                fork_markers_union.or(fork_marker.pep508());
            }
            fork_markers_union
        };
        let new_requires_python = new_requires_python.to_marker_tree();
        if fork_markers_union.is_disjoint(new_requires_python) {
            Err((fork_markers_union, new_requires_python))
        } else {
            Ok(())
        }
    }

    /// Returns the TOML representation of this lockfile.
    pub fn to_toml(&self) -> Result<String, toml_edit::ser::Error> {
        // Catch a lockfile where the union of fork markers doesn't cover the supported
        // environments.
        debug_assert!(self.check_marker_coverage().is_ok());

        // We construct a TOML document manually instead of going through Serde to enable
        // the use of inline tables.
        let mut doc = toml_edit::DocumentMut::new();
        doc.insert("version", value(i64::from(self.version)));

        if self.revision > 0 {
            doc.insert("revision", value(i64::from(self.revision)));
        }

        doc.insert("requires-python", value(self.requires_python.to_string()));

        if !self.fork_markers.is_empty() {
            let fork_markers = each_element_on_its_line_array(
                simplified_universal_markers(&self.fork_markers, &self.requires_python).into_iter(),
            );
            if !fork_markers.is_empty() {
                doc.insert("resolution-markers", value(fork_markers));
            }
        }

        if !self.supported_environments.is_empty() {
            let supported_environments = each_element_on_its_line_array(
                self.supported_environments
                    .iter()
                    .copied()
                    .map(|marker| SimplifiedMarkerTree::new(&self.requires_python, marker))
                    .filter_map(SimplifiedMarkerTree::try_to_string),
            );
            doc.insert("supported-markers", value(supported_environments));
        }

        if !self.required_environments.is_empty() {
            let required_environments = each_element_on_its_line_array(
                self.required_environments
                    .iter()
                    .copied()
                    .map(|marker| SimplifiedMarkerTree::new(&self.requires_python, marker))
                    .filter_map(SimplifiedMarkerTree::try_to_string),
            );
            doc.insert("required-markers", value(required_environments));
        }

        if !self.conflicts.is_empty() {
            let mut list = Array::new();
            for set in self.conflicts.iter() {
                list.push(each_element_on_its_line_array(set.iter().map(|item| {
                    let mut table = InlineTable::new();
                    table.insert("package", Value::from(item.package().to_string()));
                    match item.kind() {
                        ConflictKind::Project => {}
                        ConflictKind::Extra(extra) => {
                            table.insert("extra", Value::from(extra.to_string()));
                        }
                        ConflictKind::Group(group) => {
                            table.insert("group", Value::from(group.to_string()));
                        }
                    }
                    table
                })));
            }
            doc.insert("conflicts", value(list));
        }

        // Write the settings that were used to generate the resolution.
        // This enables us to invalidate the lockfile if the user changes
        // their settings.
        {
            let mut options_table = Table::new();

            if self.options.resolution_mode != ResolutionMode::default() {
                options_table.insert(
                    "resolution-mode",
                    value(self.options.resolution_mode.to_string()),
                );
            }
            if self.options.prerelease_mode != PrereleaseMode::default() {
                options_table.insert(
                    "prerelease-mode",
                    value(self.options.prerelease_mode.to_string()),
                );
            }
            if self.options.fork_strategy != ForkStrategy::default() {
                options_table.insert(
                    "fork-strategy",
                    value(self.options.fork_strategy.to_string()),
                );
            }
            let exclude_newer = &self.options.exclude_newer();
            if !exclude_newer.is_empty() {
                // Always serialize global exclude-newer as a string
                if let Some(global) = exclude_newer.global {
                    options_table.insert("exclude-newer", value(global.to_string()));
                }

                // Serialize package-specific exclusions as a separate field
                if !exclude_newer.package.is_empty() {
                    let mut package_table = toml_edit::Table::new();
                    for (name, timestamp) in &exclude_newer.package {
                        package_table.insert(name.as_ref(), value(timestamp.to_string()));
                    }
                    options_table.insert("exclude-newer-package", Item::Table(package_table));
                }
            }

            if !options_table.is_empty() {
                doc.insert("options", Item::Table(options_table));
            }
        }

        // Write the manifest that was used to generate the resolution.
        {
            let mut manifest_table = Table::new();

            if !self.manifest.members.is_empty() {
                manifest_table.insert(
                    "members",
                    value(each_element_on_its_line_array(
                        self.manifest
                            .members
                            .iter()
                            .map(std::string::ToString::to_string),
                    )),
                );
            }

            if !self.manifest.requirements.is_empty() {
                let requirements = self
                    .manifest
                    .requirements
                    .iter()
                    .map(|requirement| {
                        serde::Serialize::serialize(
                            &requirement,
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let requirements = match requirements.as_slice() {
                    [] => Array::new(),
                    [requirement] => Array::from_iter([requirement]),
                    requirements => each_element_on_its_line_array(requirements.iter()),
                };
                manifest_table.insert("requirements", value(requirements));
            }

            if !self.manifest.constraints.is_empty() {
                let constraints = self
                    .manifest
                    .constraints
                    .iter()
                    .map(|requirement| {
                        serde::Serialize::serialize(
                            &requirement,
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let constraints = match constraints.as_slice() {
                    [] => Array::new(),
                    [requirement] => Array::from_iter([requirement]),
                    constraints => each_element_on_its_line_array(constraints.iter()),
                };
                manifest_table.insert("constraints", value(constraints));
            }

            if !self.manifest.overrides.is_empty() {
                let overrides = self
                    .manifest
                    .overrides
                    .iter()
                    .map(|requirement| {
                        serde::Serialize::serialize(
                            &requirement,
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let overrides = match overrides.as_slice() {
                    [] => Array::new(),
                    [requirement] => Array::from_iter([requirement]),
                    overrides => each_element_on_its_line_array(overrides.iter()),
                };
                manifest_table.insert("overrides", value(overrides));
            }

            if !self.manifest.build_constraints.is_empty() {
                let build_constraints = self
                    .manifest
                    .build_constraints
                    .iter()
                    .map(|requirement| {
                        serde::Serialize::serialize(
                            &requirement,
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let build_constraints = match build_constraints.as_slice() {
                    [] => Array::new(),
                    [requirement] => Array::from_iter([requirement]),
                    build_constraints => each_element_on_its_line_array(build_constraints.iter()),
                };
                manifest_table.insert("build-constraints", value(build_constraints));
            }

            if !self.manifest.dependency_groups.is_empty() {
                let mut dependency_groups = Table::new();
                for (extra, requirements) in &self.manifest.dependency_groups {
                    let requirements = requirements
                        .iter()
                        .map(|requirement| {
                            serde::Serialize::serialize(
                                &requirement,
                                toml_edit::ser::ValueSerializer::new(),
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let requirements = match requirements.as_slice() {
                        [] => Array::new(),
                        [requirement] => Array::from_iter([requirement]),
                        requirements => each_element_on_its_line_array(requirements.iter()),
                    };
                    if !requirements.is_empty() {
                        dependency_groups.insert(extra.as_ref(), value(requirements));
                    }
                }
                if !dependency_groups.is_empty() {
                    manifest_table.insert("dependency-groups", Item::Table(dependency_groups));
                }
            }

            if !self.manifest.dependency_metadata.is_empty() {
                let mut tables = ArrayOfTables::new();
                for metadata in &self.manifest.dependency_metadata {
                    let mut table = Table::new();
                    table.insert("name", value(metadata.name.to_string()));
                    if let Some(version) = metadata.version.as_ref() {
                        table.insert("version", value(version.to_string()));
                    }
                    if !metadata.requires_dist.is_empty() {
                        table.insert(
                            "requires-dist",
                            value(serde::Serialize::serialize(
                                &metadata.requires_dist,
                                toml_edit::ser::ValueSerializer::new(),
                            )?),
                        );
                    }
                    if let Some(requires_python) = metadata.requires_python.as_ref() {
                        table.insert("requires-python", value(requires_python.to_string()));
                    }
                    if !metadata.provides_extras.is_empty() {
                        table.insert(
                            "provides-extras",
                            value(serde::Serialize::serialize(
                                &metadata.provides_extras,
                                toml_edit::ser::ValueSerializer::new(),
                            )?),
                        );
                    }
                    tables.push(table);
                }
                manifest_table.insert("dependency-metadata", Item::ArrayOfTables(tables));
            }

            if !manifest_table.is_empty() {
                doc.insert("manifest", Item::Table(manifest_table));
            }
        }

        // Count the number of packages for each package name. When
        // there's only one package for a particular package name (the
        // overwhelmingly common case), we can omit some data (like source and
        // version) on dependency edges since it is strictly redundant.
        let mut dist_count_by_name: FxHashMap<PackageName, u64> = FxHashMap::default();
        for dist in &self.packages {
            *dist_count_by_name.entry(dist.id.name.clone()).or_default() += 1;
        }

        let mut packages = ArrayOfTables::new();
        for dist in &self.packages {
            packages.push(dist.to_toml(&self.requires_python, &dist_count_by_name)?);
        }

        doc.insert("package", Item::ArrayOfTables(packages));
        Ok(doc.to_string())
    }

    /// Returns the package with the given name. If there are multiple
    /// matching packages, then an error is returned. If there are no
    /// matching packages, then `Ok(None)` is returned.
    fn find_by_name(&self, name: &PackageName) -> Result<Option<&Package>, String> {
        let mut found_dist = None;
        for dist in &self.packages {
            if &dist.id.name == name {
                if found_dist.is_some() {
                    return Err(format!("found multiple packages matching `{name}`"));
                }
                found_dist = Some(dist);
            }
        }
        Ok(found_dist)
    }

    /// Returns the package with the given name.
    ///
    /// If there are multiple matching packages, returns the package that
    /// corresponds to the given marker tree.
    ///
    /// If there are multiple packages that are relevant to the current
    /// markers, then an error is returned.
    ///
    /// If there are no matching packages, then `Ok(None)` is returned.
    fn find_by_markers(
        &self,
        name: &PackageName,
        marker_env: &MarkerEnvironment,
    ) -> Result<Option<&Package>, String> {
        let mut found_dist = None;
        for dist in &self.packages {
            if &dist.id.name == name {
                if dist.fork_markers.is_empty()
                    || dist
                        .fork_markers
                        .iter()
                        .any(|marker| marker.evaluate_no_extras(marker_env))
                {
                    if found_dist.is_some() {
                        return Err(format!("found multiple packages matching `{name}`"));
                    }
                    found_dist = Some(dist);
                }
            }
        }
        Ok(found_dist)
    }

    fn find_by_id(&self, id: &PackageId) -> &Package {
        let index = *self.by_id.get(id).expect("locked package for ID");

        (self.packages.get(index).expect("valid index for package")) as _
    }

    /// Return a [`SatisfiesResult`] if the given extras do not match the [`Package`] metadata.
    fn satisfies_provides_extra<'lock>(
        &self,
        provides_extra: Box<[ExtraName]>,
        package: &'lock Package,
    ) -> SatisfiesResult<'lock> {
        if !self.supports_provides_extra() {
            return SatisfiesResult::Satisfied;
        }

        let expected: BTreeSet<_> = provides_extra.iter().collect();
        let actual: BTreeSet<_> = package.metadata.provides_extras.iter().collect();

        if expected != actual {
            let expected = Box::into_iter(provides_extra).collect();
            return SatisfiesResult::MismatchedPackageProvidesExtra(
                &package.id.name,
                package.id.version.as_ref(),
                expected,
                actual,
            );
        }

        SatisfiesResult::Satisfied
    }

    /// Return a [`SatisfiesResult`] if the given requirements do not match the [`Package`] metadata.
    #[allow(clippy::unused_self)]
    fn satisfies_requires_dist<'lock>(
        &self,
        requires_dist: Box<[Requirement]>,
        dependency_groups: BTreeMap<GroupName, Box<[Requirement]>>,
        package: &'lock Package,
        root: &Path,
    ) -> Result<SatisfiesResult<'lock>, LockError> {
        // Special-case: if the version is dynamic, compare the flattened requirements.
        let flattened = if package.is_dynamic() {
            Some(
                FlatRequiresDist::from_requirements(requires_dist.clone(), &package.id.name)
                    .into_iter()
                    .map(|requirement| {
                        normalize_requirement(requirement, root, &self.requires_python)
                    })
                    .collect::<Result<BTreeSet<_>, _>>()?,
            )
        } else {
            None
        };

        // Validate the `requires-dist` metadata.
        let expected: BTreeSet<_> = Box::into_iter(requires_dist)
            .map(|requirement| normalize_requirement(requirement, root, &self.requires_python))
            .collect::<Result<_, _>>()?;
        let actual: BTreeSet<_> = package
            .metadata
            .requires_dist
            .iter()
            .cloned()
            .map(|requirement| normalize_requirement(requirement, root, &self.requires_python))
            .collect::<Result<_, _>>()?;

        if expected != actual && flattened.is_none_or(|expected| expected != actual) {
            return Ok(SatisfiesResult::MismatchedPackageRequirements(
                &package.id.name,
                package.id.version.as_ref(),
                expected,
                actual,
            ));
        }

        // Validate the `dependency-groups` metadata.
        let expected: BTreeMap<GroupName, BTreeSet<Requirement>> = dependency_groups
            .into_iter()
            .filter(|(_, requirements)| self.includes_empty_groups() || !requirements.is_empty())
            .map(|(group, requirements)| {
                Ok::<_, LockError>((
                    group,
                    Box::into_iter(requirements)
                        .map(|requirement| {
                            normalize_requirement(requirement, root, &self.requires_python)
                        })
                        .collect::<Result<_, _>>()?,
                ))
            })
            .collect::<Result<_, _>>()?;
        let actual: BTreeMap<GroupName, BTreeSet<Requirement>> = package
            .metadata
            .dependency_groups
            .iter()
            .filter(|(_, requirements)| self.includes_empty_groups() || !requirements.is_empty())
            .map(|(group, requirements)| {
                Ok::<_, LockError>((
                    group.clone(),
                    requirements
                        .iter()
                        .cloned()
                        .map(|requirement| {
                            normalize_requirement(requirement, root, &self.requires_python)
                        })
                        .collect::<Result<_, _>>()?,
                ))
            })
            .collect::<Result<_, _>>()?;

        if expected != actual {
            return Ok(SatisfiesResult::MismatchedPackageDependencyGroups(
                &package.id.name,
                package.id.version.as_ref(),
                expected,
                actual,
            ));
        }

        Ok(SatisfiesResult::Satisfied)
    }

    /// Convert the [`Lock`] to a [`Resolution`] using the given marker environment, tags, and root.
    pub async fn satisfies<Context: BuildContext>(
        &self,
        root: &Path,
        packages: &BTreeMap<PackageName, WorkspaceMember>,
        members: &[PackageName],
        required_members: &BTreeMap<PackageName, Editability>,
        requirements: &[Requirement],
        constraints: &[Requirement],
        overrides: &[Requirement],
        build_constraints: &[Requirement],
        dependency_groups: &BTreeMap<GroupName, Vec<Requirement>>,
        dependency_metadata: &DependencyMetadata,
        indexes: Option<&IndexLocations>,
        tags: &Tags,
        hasher: &HashStrategy,
        index: &InMemoryIndex,
        database: &DistributionDatabase<'_, Context>,
    ) -> Result<SatisfiesResult<'_>, LockError> {
        let mut queue: VecDeque<&Package> = VecDeque::new();
        let mut seen = FxHashSet::default();

        // Validate that the lockfile was generated with the same root members.
        {
            let expected = members.iter().cloned().collect::<BTreeSet<_>>();
            let actual = &self.manifest.members;
            if expected != *actual {
                return Ok(SatisfiesResult::MismatchedMembers(expected, actual));
            }
        }

        // Validate that the member sources have not changed (e.g., that they've switched from
        // virtual to non-virtual or vice versa).
        for (name, member) in packages {
            let source = self.find_by_name(name).ok().flatten();

            // Determine whether the member was required by any other member.
            let value = required_members.get(name);
            let is_required_member = value.is_some();
            let editability = value.copied().flatten();

            // Verify that the member is virtual (or not).
            let expected_virtual = !member.pyproject_toml().is_package(!is_required_member);
            let actual_virtual =
                source.map(|package| matches!(package.id.source, Source::Virtual(..)));
            if actual_virtual != Some(expected_virtual) {
                return Ok(SatisfiesResult::MismatchedVirtual(
                    name.clone(),
                    expected_virtual,
                ));
            }

            // Verify that the member is editable (or not).
            let expected_editable = if expected_virtual {
                false
            } else {
                editability.unwrap_or(true)
            };
            let actual_editable =
                source.map(|package| matches!(package.id.source, Source::Editable(..)));
            if actual_editable != Some(expected_editable) {
                return Ok(SatisfiesResult::MismatchedEditable(
                    name.clone(),
                    expected_editable,
                ));
            }
        }

        // Validate that the lockfile was generated with the same requirements.
        {
            let expected: BTreeSet<_> = requirements
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, root, &self.requires_python))
                .collect::<Result<_, _>>()?;
            let actual: BTreeSet<_> = self
                .manifest
                .requirements
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, root, &self.requires_python))
                .collect::<Result<_, _>>()?;
            if expected != actual {
                return Ok(SatisfiesResult::MismatchedRequirements(expected, actual));
            }
        }

        // Validate that the lockfile was generated with the same constraints.
        {
            let expected: BTreeSet<_> = constraints
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, root, &self.requires_python))
                .collect::<Result<_, _>>()?;
            let actual: BTreeSet<_> = self
                .manifest
                .constraints
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, root, &self.requires_python))
                .collect::<Result<_, _>>()?;
            if expected != actual {
                return Ok(SatisfiesResult::MismatchedConstraints(expected, actual));
            }
        }

        // Validate that the lockfile was generated with the same overrides.
        {
            let expected: BTreeSet<_> = overrides
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, root, &self.requires_python))
                .collect::<Result<_, _>>()?;
            let actual: BTreeSet<_> = self
                .manifest
                .overrides
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, root, &self.requires_python))
                .collect::<Result<_, _>>()?;
            if expected != actual {
                return Ok(SatisfiesResult::MismatchedOverrides(expected, actual));
            }
        }

        // Validate that the lockfile was generated with the same build constraints.
        {
            let expected: BTreeSet<_> = build_constraints
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, root, &self.requires_python))
                .collect::<Result<_, _>>()?;
            let actual: BTreeSet<_> = self
                .manifest
                .build_constraints
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, root, &self.requires_python))
                .collect::<Result<_, _>>()?;
            if expected != actual {
                return Ok(SatisfiesResult::MismatchedBuildConstraints(
                    expected, actual,
                ));
            }
        }

        // Validate that the lockfile was generated with the dependency groups.
        {
            let expected: BTreeMap<GroupName, BTreeSet<Requirement>> = dependency_groups
                .iter()
                .filter(|(_, requirements)| !requirements.is_empty())
                .map(|(group, requirements)| {
                    Ok::<_, LockError>((
                        group.clone(),
                        requirements
                            .iter()
                            .cloned()
                            .map(|requirement| {
                                normalize_requirement(requirement, root, &self.requires_python)
                            })
                            .collect::<Result<_, _>>()?,
                    ))
                })
                .collect::<Result<_, _>>()?;
            let actual: BTreeMap<GroupName, BTreeSet<Requirement>> = self
                .manifest
                .dependency_groups
                .iter()
                .filter(|(_, requirements)| !requirements.is_empty())
                .map(|(group, requirements)| {
                    Ok::<_, LockError>((
                        group.clone(),
                        requirements
                            .iter()
                            .cloned()
                            .map(|requirement| {
                                normalize_requirement(requirement, root, &self.requires_python)
                            })
                            .collect::<Result<_, _>>()?,
                    ))
                })
                .collect::<Result<_, _>>()?;
            if expected != actual {
                return Ok(SatisfiesResult::MismatchedDependencyGroups(
                    expected, actual,
                ));
            }
        }

        // Validate that the lockfile was generated with the same static metadata.
        {
            let expected = dependency_metadata
                .values()
                .cloned()
                .collect::<BTreeSet<_>>();
            let actual = &self.manifest.dependency_metadata;
            if expected != *actual {
                return Ok(SatisfiesResult::MismatchedStaticMetadata(expected, actual));
            }
        }

        // Collect the set of available indexes (both `--index-url` and `--find-links` entries).
        let mut remotes = indexes.map(|locations| {
            locations
                .allowed_indexes()
                .into_iter()
                .filter_map(|index| match index.url() {
                    IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                        Some(UrlString::from(index.url().without_credentials().as_ref()))
                    }
                    IndexUrl::Path(_) => None,
                })
                .collect::<BTreeSet<_>>()
        });

        let mut locals = indexes.map(|locations| {
            locations
                .allowed_indexes()
                .into_iter()
                .filter_map(|index| match index.url() {
                    IndexUrl::Pypi(_) | IndexUrl::Url(_) => None,
                    IndexUrl::Path(url) => {
                        let path = url.to_file_path().ok()?;
                        let path = relative_to(&path, root)
                            .or_else(|_| std::path::absolute(path))
                            .ok()?
                            .into_boxed_path();
                        Some(path)
                    }
                })
                .collect::<BTreeSet<_>>()
        });

        // Add the workspace packages to the queue.
        for root_name in packages.keys() {
            let root = self
                .find_by_name(root_name)
                .expect("found too many packages matching root");

            let Some(root) = root else {
                // The package is not in the lockfile, so it can't be satisfied.
                return Ok(SatisfiesResult::MissingRoot(root_name.clone()));
            };

            // Add the base package.
            queue.push_back(root);
        }

        while let Some(package) = queue.pop_front() {
            // If the lockfile references an index that was not provided, we can't validate it.
            if let Source::Registry(index) = &package.id.source {
                match index {
                    RegistrySource::Url(url) => {
                        if remotes
                            .as_ref()
                            .is_some_and(|remotes| !remotes.contains(url))
                        {
                            let name = &package.id.name;
                            let version = &package
                                .id
                                .version
                                .as_ref()
                                .expect("version for registry source");
                            return Ok(SatisfiesResult::MissingRemoteIndex(name, version, url));
                        }
                    }
                    RegistrySource::Path(path) => {
                        if locals.as_ref().is_some_and(|locals| !locals.contains(path)) {
                            let name = &package.id.name;
                            let version = &package
                                .id
                                .version
                                .as_ref()
                                .expect("version for registry source");
                            return Ok(SatisfiesResult::MissingLocalIndex(name, version, path));
                        }
                    }
                }
            }

            // If the package is immutable, we don't need to validate it (or its dependencies).
            if package.id.source.is_immutable() {
                continue;
            }

            if let Some(version) = package.id.version.as_ref() {
                // For a non-dynamic package, fetch the metadata from the distribution database.
                let dist =
                    package.to_dist(root, TagPolicy::Preferred(tags), &BuildOptions::default())?;

                let metadata = {
                    let id = dist.version_id();
                    if let Some(archive) =
                        index
                            .distributions()
                            .get(&id)
                            .as_deref()
                            .and_then(|response| {
                                if let MetadataResponse::Found(archive, ..) = response {
                                    Some(archive)
                                } else {
                                    None
                                }
                            })
                    {
                        // If the metadata is already in the index, return it.
                        archive.metadata.clone()
                    } else {
                        // Run the PEP 517 build process to extract metadata from the source distribution.
                        let archive = database
                            .get_or_build_wheel_metadata(&dist, hasher.get(&dist))
                            .await
                            .map_err(|err| LockErrorKind::Resolution {
                                id: package.id.clone(),
                                err,
                            })?;

                        let metadata = archive.metadata.clone();

                        // Insert the metadata into the index.
                        index
                            .distributions()
                            .done(id, Arc::new(MetadataResponse::Found(archive)));

                        metadata
                    }
                };

                // If this is a local package, validate that it hasn't become dynamic (in which
                // case, we'd expect the version to be omitted).
                if package.id.source.is_source_tree() {
                    if metadata.dynamic {
                        return Ok(SatisfiesResult::MismatchedDynamic(&package.id.name, false));
                    }
                }

                // Validate the `version` metadata.
                if metadata.version != *version {
                    return Ok(SatisfiesResult::MismatchedVersion(
                        &package.id.name,
                        version.clone(),
                        Some(metadata.version.clone()),
                    ));
                }

                // Validate the `provides-extras` metadata.
                match self.satisfies_provides_extra(metadata.provides_extras, package) {
                    SatisfiesResult::Satisfied => {}
                    result => return Ok(result),
                }

                // Validate that the requirements are unchanged.
                match self.satisfies_requires_dist(
                    metadata.requires_dist,
                    metadata.dependency_groups,
                    package,
                    root,
                )? {
                    SatisfiesResult::Satisfied => {}
                    result => return Ok(result),
                }
            } else if let Some(source_tree) = package.id.source.as_source_tree() {
                // For dynamic packages, we don't need the version. We only need to know that the
                // package is still dynamic, and that the requirements are unchanged.
                //
                // If the distribution is a source tree, attempt to extract the requirements from the
                // `pyproject.toml` directly. The distribution database will do this too, but we can be
                // even more aggressive here since we _only_ need the requirements. So, for example,
                // even if the version is dynamic, we can still extract the requirements without
                // performing a build, unlike in the database where we typically construct a "complete"
                // metadata object.
                let metadata = database
                    .requires_dist(root.join(source_tree))
                    .await
                    .map_err(|err| LockErrorKind::Resolution {
                        id: package.id.clone(),
                        err,
                    })?;

                let satisfied = metadata.is_some_and(|metadata| {
                    // Validate that the package is still dynamic.
                    if !metadata.dynamic {
                        debug!("Static `requires-dist` for `{}` is out-of-date; falling back to distribution database", package.id);
                        return false;
                    }

                    // Validate that the extras are unchanged.
                    if let SatisfiesResult::Satisfied = self.satisfies_provides_extra(metadata.provides_extras, package, ) {
                        debug!("Static `provides-extra` for `{}` is up-to-date", package.id);
                    } else {
                        debug!("Static `provides-extra` for `{}` is out-of-date; falling back to distribution database", package.id);
                        return false;
                    }

                    // Validate that the requirements are unchanged.
                    match self.satisfies_requires_dist(metadata.requires_dist, metadata.dependency_groups, package, root) {
                        Ok(SatisfiesResult::Satisfied) => {
                            debug!("Static `requires-dist` for `{}` is up-to-date", package.id);
                        },
                        Ok(..) => {
                            debug!("Static `requires-dist` for `{}` is out-of-date; falling back to distribution database", package.id);
                            return false;
                        },
                        Err(..) => {
                            debug!("Static `requires-dist` for `{}` is invalid; falling back to distribution database", package.id);
                            return false;
                        },
                    }

                    true
                });

                // If the `requires-dist` metadata matches the requirements, we're done; otherwise,
                // fetch the "full" metadata, which may involve invoking the build system. In some
                // cases, build backends return metadata that does _not_ match the `pyproject.toml`
                // exactly. For example, `hatchling` will flatten any recursive (or self-referential)
                // extras, while `setuptools` will not.
                if !satisfied {
                    let dist = package.to_dist(
                        root,
                        TagPolicy::Preferred(tags),
                        &BuildOptions::default(),
                    )?;

                    let metadata = {
                        let id = dist.version_id();
                        if let Some(archive) =
                            index
                                .distributions()
                                .get(&id)
                                .as_deref()
                                .and_then(|response| {
                                    if let MetadataResponse::Found(archive, ..) = response {
                                        Some(archive)
                                    } else {
                                        None
                                    }
                                })
                        {
                            // If the metadata is already in the index, return it.
                            archive.metadata.clone()
                        } else {
                            // Run the PEP 517 build process to extract metadata from the source distribution.
                            let archive = database
                                .get_or_build_wheel_metadata(&dist, hasher.get(&dist))
                                .await
                                .map_err(|err| LockErrorKind::Resolution {
                                    id: package.id.clone(),
                                    err,
                                })?;

                            let metadata = archive.metadata.clone();

                            // Insert the metadata into the index.
                            index
                                .distributions()
                                .done(id, Arc::new(MetadataResponse::Found(archive)));

                            metadata
                        }
                    };

                    // Validate that the package is still dynamic.
                    if !metadata.dynamic {
                        return Ok(SatisfiesResult::MismatchedDynamic(&package.id.name, true));
                    }

                    // Validate that the extras are unchanged.
                    match self.satisfies_provides_extra(metadata.provides_extras, package) {
                        SatisfiesResult::Satisfied => {}
                        result => return Ok(result),
                    }

                    // Validate that the requirements are unchanged.
                    match self.satisfies_requires_dist(
                        metadata.requires_dist,
                        metadata.dependency_groups,
                        package,
                        root,
                    )? {
                        SatisfiesResult::Satisfied => {}
                        result => return Ok(result),
                    }
                }
            } else {
                return Ok(SatisfiesResult::MissingVersion(&package.id.name));
            }

            // Add any explicit indexes to the list of known locals or remotes. These indexes may
            // not be available as top-level configuration (i.e., if they're defined within a
            // workspace member), but we already validated that the dependencies are up-to-date, so
            // we can consider them "available".
            for requirement in &package.metadata.requires_dist {
                if let RequirementSource::Registry {
                    index: Some(index), ..
                } = &requirement.source
                {
                    match &index.url {
                        IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                            if let Some(remotes) = remotes.as_mut() {
                                remotes.insert(UrlString::from(
                                    index.url().without_credentials().as_ref(),
                                ));
                            }
                        }
                        IndexUrl::Path(url) => {
                            if let Some(locals) = locals.as_mut() {
                                if let Some(path) = url.to_file_path().ok().and_then(|path| {
                                    relative_to(&path, root)
                                        .or_else(|_| std::path::absolute(path))
                                        .ok()
                                }) {
                                    locals.insert(path.into_boxed_path());
                                }
                            }
                        }
                    }
                }
            }

            // Recurse.
            for dep in &package.dependencies {
                if seen.insert(&dep.package_id) {
                    let dep_dist = self.find_by_id(&dep.package_id);
                    queue.push_back(dep_dist);
                }
            }

            for dependencies in package.optional_dependencies.values() {
                for dep in dependencies {
                    if seen.insert(&dep.package_id) {
                        let dep_dist = self.find_by_id(&dep.package_id);
                        queue.push_back(dep_dist);
                    }
                }
            }

            for dependencies in package.dependency_groups.values() {
                for dep in dependencies {
                    if seen.insert(&dep.package_id) {
                        let dep_dist = self.find_by_id(&dep.package_id);
                        queue.push_back(dep_dist);
                    }
                }
            }
        }

        Ok(SatisfiesResult::Satisfied)
    }
}

#[derive(Debug, Copy, Clone)]
enum TagPolicy<'tags> {
    /// Exclusively consider wheels that match the specified platform tags.
    Required(&'tags Tags),
    /// Prefer wheels that match the specified platform tags, but fall back to incompatible wheels
    /// if necessary.
    Preferred(&'tags Tags),
}

impl<'tags> TagPolicy<'tags> {
    /// Returns the platform tags to consider.
    fn tags(&self) -> &'tags Tags {
        match self {
            Self::Required(tags) | Self::Preferred(tags) => tags,
        }
    }
}

/// The result of checking if a lockfile satisfies a set of requirements.
#[derive(Debug)]
pub enum SatisfiesResult<'lock> {
    /// The lockfile satisfies the requirements.
    Satisfied,
    /// The lockfile uses a different set of workspace members.
    MismatchedMembers(BTreeSet<PackageName>, &'lock BTreeSet<PackageName>),
    /// A workspace member switched from virtual to non-virtual or vice versa.
    MismatchedVirtual(PackageName, bool),
    /// A workspace member switched from editable to non-editable or vice versa.
    MismatchedEditable(PackageName, bool),
    /// A source tree switched from dynamic to non-dynamic or vice versa.
    MismatchedDynamic(&'lock PackageName, bool),
    /// The lockfile uses a different set of version for its workspace members.
    MismatchedVersion(&'lock PackageName, Version, Option<Version>),
    /// The lockfile uses a different set of requirements.
    MismatchedRequirements(BTreeSet<Requirement>, BTreeSet<Requirement>),
    /// The lockfile uses a different set of constraints.
    MismatchedConstraints(BTreeSet<Requirement>, BTreeSet<Requirement>),
    /// The lockfile uses a different set of overrides.
    MismatchedOverrides(BTreeSet<Requirement>, BTreeSet<Requirement>),
    /// The lockfile uses a different set of build constraints.
    MismatchedBuildConstraints(BTreeSet<Requirement>, BTreeSet<Requirement>),
    /// The lockfile uses a different set of dependency groups.
    MismatchedDependencyGroups(
        BTreeMap<GroupName, BTreeSet<Requirement>>,
        BTreeMap<GroupName, BTreeSet<Requirement>>,
    ),
    /// The lockfile uses different static metadata.
    MismatchedStaticMetadata(BTreeSet<StaticMetadata>, &'lock BTreeSet<StaticMetadata>),
    /// The lockfile is missing a workspace member.
    MissingRoot(PackageName),
    /// The lockfile referenced a remote index that was not provided
    MissingRemoteIndex(&'lock PackageName, &'lock Version, &'lock UrlString),
    /// The lockfile referenced a local index that was not provided
    MissingLocalIndex(&'lock PackageName, &'lock Version, &'lock Path),
    /// A package in the lockfile contains different `requires-dist` metadata than expected.
    MismatchedPackageRequirements(
        &'lock PackageName,
        Option<&'lock Version>,
        BTreeSet<Requirement>,
        BTreeSet<Requirement>,
    ),
    /// A package in the lockfile contains different `provides-extra` metadata than expected.
    MismatchedPackageProvidesExtra(
        &'lock PackageName,
        Option<&'lock Version>,
        BTreeSet<ExtraName>,
        BTreeSet<&'lock ExtraName>,
    ),
    /// A package in the lockfile contains different `dependency-groups` metadata than expected.
    MismatchedPackageDependencyGroups(
        &'lock PackageName,
        Option<&'lock Version>,
        BTreeMap<GroupName, BTreeSet<Requirement>>,
        BTreeMap<GroupName, BTreeSet<Requirement>>,
    ),
    /// The lockfile is missing a version.
    MissingVersion(&'lock PackageName),
}

/// We discard the lockfile if these options match.
#[derive(Clone, Debug, Default, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct ResolverOptions {
    /// The [`ResolutionMode`] used to generate this lock.
    #[serde(default)]
    resolution_mode: ResolutionMode,
    /// The [`PrereleaseMode`] used to generate this lock.
    #[serde(default)]
    prerelease_mode: PrereleaseMode,
    /// The [`ForkStrategy`] used to generate this lock.
    #[serde(default)]
    fork_strategy: ForkStrategy,
    /// The global [`ExcludeNewer`] timestamp.
    exclude_newer: Option<ExcludeNewerTimestamp>,
    /// Package-specific [`ExcludeNewer`] timestamps.
    exclude_newer_package: Option<FxHashMap<PackageName, ExcludeNewerTimestamp>>,
}

impl ResolverOptions {
    /// Get the combined exclude-newer configuration.
    fn exclude_newer(&self) -> ExcludeNewer {
        ExcludeNewer::from_args(
            self.exclude_newer,
            self.exclude_newer_package
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    }
}

#[derive(Clone, Debug, Default, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ResolverManifest {
    /// The workspace members included in the lockfile.
    #[serde(default)]
    members: BTreeSet<PackageName>,
    /// The requirements provided to the resolver, exclusive of the workspace members.
    ///
    /// These are requirements that are attached to the project, but not to any of its
    /// workspace members. For example, the requirements in a PEP 723 script would be included here.
    #[serde(default)]
    requirements: BTreeSet<Requirement>,
    /// The dependency groups provided to the resolver, exclusive of the workspace members.
    ///
    /// These are dependency groups that are attached to the project, but not to any of its
    /// workspace members. For example, the dependency groups in a `pyproject.toml` without a
    /// `[project]` table would be included here.
    #[serde(default)]
    dependency_groups: BTreeMap<GroupName, BTreeSet<Requirement>>,
    /// The constraints provided to the resolver.
    #[serde(default)]
    constraints: BTreeSet<Requirement>,
    /// The overrides provided to the resolver.
    #[serde(default)]
    overrides: BTreeSet<Requirement>,
    /// The build constraints provided to the resolver.
    #[serde(default)]
    build_constraints: BTreeSet<Requirement>,
    /// The static metadata provided to the resolver.
    #[serde(default)]
    dependency_metadata: BTreeSet<StaticMetadata>,
}

impl ResolverManifest {
    /// Initialize a [`ResolverManifest`] with the given members, requirements, constraints, and
    /// overrides.
    pub fn new(
        members: impl IntoIterator<Item = PackageName>,
        requirements: impl IntoIterator<Item = Requirement>,
        constraints: impl IntoIterator<Item = Requirement>,
        overrides: impl IntoIterator<Item = Requirement>,
        build_constraints: impl IntoIterator<Item = Requirement>,
        dependency_groups: impl IntoIterator<Item = (GroupName, Vec<Requirement>)>,
        dependency_metadata: impl IntoIterator<Item = StaticMetadata>,
    ) -> Self {
        Self {
            members: members.into_iter().collect(),
            requirements: requirements.into_iter().collect(),
            constraints: constraints.into_iter().collect(),
            overrides: overrides.into_iter().collect(),
            build_constraints: build_constraints.into_iter().collect(),
            dependency_groups: dependency_groups
                .into_iter()
                .map(|(group, requirements)| (group, requirements.into_iter().collect()))
                .collect(),
            dependency_metadata: dependency_metadata.into_iter().collect(),
        }
    }

    /// Convert the manifest to a relative form using the given workspace.
    pub fn relative_to(self, root: &Path) -> Result<Self, io::Error> {
        Ok(Self {
            members: self.members,
            requirements: self
                .requirements
                .into_iter()
                .map(|requirement| requirement.relative_to(root))
                .collect::<Result<BTreeSet<_>, _>>()?,
            constraints: self
                .constraints
                .into_iter()
                .map(|requirement| requirement.relative_to(root))
                .collect::<Result<BTreeSet<_>, _>>()?,
            overrides: self
                .overrides
                .into_iter()
                .map(|requirement| requirement.relative_to(root))
                .collect::<Result<BTreeSet<_>, _>>()?,
            build_constraints: self
                .build_constraints
                .into_iter()
                .map(|requirement| requirement.relative_to(root))
                .collect::<Result<BTreeSet<_>, _>>()?,
            dependency_groups: self
                .dependency_groups
                .into_iter()
                .map(|(group, requirements)| {
                    Ok::<_, io::Error>((
                        group,
                        requirements
                            .into_iter()
                            .map(|requirement| requirement.relative_to(root))
                            .collect::<Result<BTreeSet<_>, _>>()?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?,
            dependency_metadata: self.dependency_metadata,
        })
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct LockWire {
    version: u32,
    revision: Option<u32>,
    requires_python: RequiresPython,
    /// If this lockfile was built from a forking resolution with non-identical forks, store the
    /// forks in the lockfile so we can recreate them in subsequent resolutions.
    #[serde(rename = "resolution-markers", default)]
    fork_markers: Vec<SimplifiedMarkerTree>,
    #[serde(rename = "supported-markers", default)]
    supported_environments: Vec<SimplifiedMarkerTree>,
    #[serde(rename = "required-markers", default)]
    required_environments: Vec<SimplifiedMarkerTree>,
    #[serde(rename = "conflicts", default)]
    conflicts: Option<Conflicts>,
    /// We discard the lockfile if these options match.
    #[serde(default)]
    options: ResolverOptions,
    #[serde(default)]
    manifest: ResolverManifest,
    #[serde(rename = "package", alias = "distribution", default)]
    packages: Vec<PackageWire>,
}

impl TryFrom<LockWire> for Lock {
    type Error = LockError;

    fn try_from(wire: LockWire) -> Result<Self, LockError> {
        // Count the number of sources for each package name. When
        // there's only one source for a particular package name (the
        // overwhelmingly common case), we can omit some data (like source and
        // version) on dependency edges since it is strictly redundant.
        let mut unambiguous_package_ids: FxHashMap<PackageName, PackageId> = FxHashMap::default();
        let mut ambiguous = FxHashSet::default();
        for dist in &wire.packages {
            if ambiguous.contains(&dist.id.name) {
                continue;
            }
            if let Some(id) = unambiguous_package_ids.remove(&dist.id.name) {
                ambiguous.insert(id.name);
                continue;
            }
            unambiguous_package_ids.insert(dist.id.name.clone(), dist.id.clone());
        }

        let packages = wire
            .packages
            .into_iter()
            .map(|dist| dist.unwire(&wire.requires_python, &unambiguous_package_ids))
            .collect::<Result<Vec<_>, _>>()?;
        let supported_environments = wire
            .supported_environments
            .into_iter()
            .map(|simplified_marker| simplified_marker.into_marker(&wire.requires_python))
            .collect();
        let required_environments = wire
            .required_environments
            .into_iter()
            .map(|simplified_marker| simplified_marker.into_marker(&wire.requires_python))
            .collect();
        let fork_markers = wire
            .fork_markers
            .into_iter()
            .map(|simplified_marker| simplified_marker.into_marker(&wire.requires_python))
            .map(UniversalMarker::from_combined)
            .collect();
        let lock = Self::new(
            wire.version,
            wire.revision.unwrap_or(0),
            packages,
            wire.requires_python,
            wire.options,
            wire.manifest,
            wire.conflicts.unwrap_or_else(Conflicts::empty),
            supported_environments,
            required_environments,
            fork_markers,
        )?;

        Ok(lock)
    }
}

/// Like [`Lock`], but limited to the version field. Used for error reporting: by limiting parsing
/// to the version field, we can verify compatibility for lockfiles that may otherwise be
/// unparsable.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LockVersion {
    version: u32,
}

impl LockVersion {
    /// Returns the lockfile version.
    pub fn version(&self) -> u32 {
        self.version
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub(crate) id: PackageId,
    sdist: Option<SourceDist>,
    wheels: Vec<Wheel>,
    /// The variants JSON file for the package version, if available.
    ///
    /// Named `variants-json` in `uv.lock`.
    variants_json: Option<VariantsJsonEntry>,
    /// If there are multiple versions or sources for the same package name, we add the markers of
    /// the fork(s) that contained this version or source, so we can set the correct preferences in
    /// the next resolution.
    ///
    /// Named `resolution-markers` in `uv.lock`.
    fork_markers: Vec<UniversalMarker>,
    /// The resolved dependencies of the package.
    dependencies: Vec<Dependency>,
    /// The resolved optional dependencies of the package.
    optional_dependencies: BTreeMap<ExtraName, Vec<Dependency>>,
    /// The resolved PEP 735 dependency groups of the package.
    dependency_groups: BTreeMap<GroupName, Vec<Dependency>>,
    /// The exact requirements from the package metadata.
    metadata: PackageMetadata,
}

impl Package {
    fn from_annotated_dist(
        annotated_dist: &AnnotatedDist,
        fork_markers: Vec<UniversalMarker>,
        root: &Path,
    ) -> Result<Self, LockError> {
        let id = PackageId::from_annotated_dist(annotated_dist, root)?;
        let sdist = SourceDist::from_annotated_dist(&id, annotated_dist)?;
        let wheels = Wheel::from_annotated_dist(annotated_dist)?;
        let variants_json = VariantsJsonEntry::from_annotated_dist(annotated_dist)?;
        let requires_dist = if id.source.is_immutable() {
            BTreeSet::default()
        } else {
            annotated_dist
                .metadata
                .as_ref()
                .expect("metadata is present")
                .requires_dist
                .iter()
                .cloned()
                .map(|requirement| requirement.relative_to(root))
                .collect::<Result<_, _>>()
                .map_err(LockErrorKind::RequirementRelativePath)?
        };
        let provides_extras = if id.source.is_immutable() {
            Box::default()
        } else {
            annotated_dist
                .metadata
                .as_ref()
                .expect("metadata is present")
                .provides_extras
                .clone()
        };
        let dependency_groups = if id.source.is_immutable() {
            BTreeMap::default()
        } else {
            annotated_dist
                .metadata
                .as_ref()
                .expect("metadata is present")
                .dependency_groups
                .iter()
                .map(|(group, requirements)| {
                    let requirements = requirements
                        .iter()
                        .cloned()
                        .map(|requirement| requirement.relative_to(root))
                        .collect::<Result<_, _>>()
                        .map_err(LockErrorKind::RequirementRelativePath)?;
                    Ok::<_, LockError>((group.clone(), requirements))
                })
                .collect::<Result<_, _>>()?
        };
        Ok(Self {
            id,
            sdist,
            wheels,
            variants_json,
            fork_markers,
            dependencies: vec![],
            optional_dependencies: BTreeMap::default(),
            dependency_groups: BTreeMap::default(),
            metadata: PackageMetadata {
                requires_dist,
                provides_extras,
                dependency_groups,
            },
        })
    }

    /// Add the [`AnnotatedDist`] as a dependency of the [`Package`].
    fn add_dependency(
        &mut self,
        requires_python: &RequiresPython,
        annotated_dist: &AnnotatedDist,
        marker: UniversalMarker,
        root: &Path,
    ) -> Result<(), LockError> {
        let new_dep =
            Dependency::from_annotated_dist(requires_python, annotated_dist, marker, root)?;
        for existing_dep in &mut self.dependencies {
            if existing_dep.package_id == new_dep.package_id
                // It's important that we do a comparison on
                // *simplified* markers here. In particular, when
                // we write markers out to the lock file, we use
                // "simplified" markers, or markers that are simplified
                // *given* that `requires-python` is satisfied. So if
                // we don't do equality based on what the simplified
                // marker is, we might wind up not merging dependencies
                // that ought to be merged and thus writing out extra
                // entries.
                //
                // For example, if `requires-python = '>=3.8'` and we
                // have `foo==1` and
                // `foo==1 ; python_version >= '3.8'` dependencies,
                // then they don't have equivalent complexified
                // markers, but their simplified markers are identical.
                //
                // NOTE: It does seem like perhaps this should
                // be implemented semantically/algebraically on
                // `MarkerTree` itself, but it wasn't totally clear
                // how to do that. I think `pep508` would need to
                // grow a concept of "requires python" and provide an
                // operation specifically for that.
                && existing_dep.simplified_marker == new_dep.simplified_marker
            {
                existing_dep.extra.extend(new_dep.extra);
                return Ok(());
            }
        }

        self.dependencies.push(new_dep);
        Ok(())
    }

    /// Add the [`AnnotatedDist`] as an optional dependency of the [`Package`].
    fn add_optional_dependency(
        &mut self,
        requires_python: &RequiresPython,
        extra: ExtraName,
        annotated_dist: &AnnotatedDist,
        marker: UniversalMarker,
        root: &Path,
    ) -> Result<(), LockError> {
        let dep = Dependency::from_annotated_dist(requires_python, annotated_dist, marker, root)?;
        let optional_deps = self.optional_dependencies.entry(extra).or_default();
        for existing_dep in &mut *optional_deps {
            if existing_dep.package_id == dep.package_id
                // See note in add_dependency for why we use
                // simplified markers here.
                && existing_dep.simplified_marker == dep.simplified_marker
            {
                existing_dep.extra.extend(dep.extra);
                return Ok(());
            }
        }

        optional_deps.push(dep);
        Ok(())
    }

    /// Add the [`AnnotatedDist`] to a dependency group of the [`Package`].
    fn add_group_dependency(
        &mut self,
        requires_python: &RequiresPython,
        group: GroupName,
        annotated_dist: &AnnotatedDist,
        marker: UniversalMarker,
        root: &Path,
    ) -> Result<(), LockError> {
        let dep = Dependency::from_annotated_dist(requires_python, annotated_dist, marker, root)?;
        let deps = self.dependency_groups.entry(group).or_default();
        for existing_dep in &mut *deps {
            if existing_dep.package_id == dep.package_id
                // See note in add_dependency for why we use
                // simplified markers here.
                && existing_dep.simplified_marker == dep.simplified_marker
            {
                existing_dep.extra.extend(dep.extra);
                return Ok(());
            }
        }

        deps.push(dep);
        Ok(())
    }

    /// Convert the [`Package`] to a [`Dist`] that can be used in installation.
    fn to_dist(
        &self,
        workspace_root: &Path,
        tag_policy: TagPolicy<'_>,
        build_options: &BuildOptions,
    ) -> Result<Dist, LockError> {
        let no_binary = build_options.no_binary_package(&self.id.name);
        let no_build = build_options.no_build_package(&self.id.name);

        if !no_binary {
            if let Some(best_wheel_index) = self.find_best_wheel(tag_policy) {
                return match &self.id.source {
                    Source::Registry(source) => {
                        let wheels = self
                            .wheels
                            .iter()
                            .map(|wheel| wheel.to_registry_wheel(source, workspace_root))
                            .collect::<Result<_, LockError>>()?;
                        let reg_built_dist = RegistryBuiltDist {
                            wheels,
                            best_wheel_index,
                            sdist: None,
                        };
                        Ok(Dist::Built(BuiltDist::Registry(reg_built_dist)))
                    }
                    Source::Path(path) => {
                        let filename: WheelFilename =
                            self.wheels[best_wheel_index].filename.clone();
                        let install_path = absolute_path(workspace_root, path)?;
                        let path_dist = PathBuiltDist {
                            filename,
                            url: verbatim_url(&install_path, &self.id)?,
                            install_path: absolute_path(workspace_root, path)?.into_boxed_path(),
                        };
                        let built_dist = BuiltDist::Path(path_dist);
                        Ok(Dist::Built(built_dist))
                    }
                    Source::Direct(url, direct) => {
                        let filename: WheelFilename =
                            self.wheels[best_wheel_index].filename.clone();
                        let url = DisplaySafeUrl::from(ParsedArchiveUrl {
                            url: url.to_url().map_err(LockErrorKind::InvalidUrl)?,
                            subdirectory: direct.subdirectory.clone(),
                            ext: DistExtension::Wheel,
                        });
                        let direct_dist = DirectUrlBuiltDist {
                            filename,
                            location: Box::new(url.clone()),
                            url: VerbatimUrl::from_url(url),
                        };
                        let built_dist = BuiltDist::DirectUrl(direct_dist);
                        Ok(Dist::Built(built_dist))
                    }
                    Source::Git(_, _) => Err(LockErrorKind::InvalidWheelSource {
                        id: self.id.clone(),
                        source_type: "Git",
                    }
                    .into()),
                    Source::Directory(_) => Err(LockErrorKind::InvalidWheelSource {
                        id: self.id.clone(),
                        source_type: "directory",
                    }
                    .into()),
                    Source::Editable(_) => Err(LockErrorKind::InvalidWheelSource {
                        id: self.id.clone(),
                        source_type: "editable",
                    }
                    .into()),
                    Source::Virtual(_) => Err(LockErrorKind::InvalidWheelSource {
                        id: self.id.clone(),
                        source_type: "virtual",
                    }
                    .into()),
                };
            }
        }

        if let Some(sdist) = self.to_source_dist(workspace_root)? {
            // Even with `--no-build`, allow virtual packages. (In the future, we may want to allow
            // any local source tree, or at least editable source trees, which we allow in
            // `uv pip`.)
            if !no_build || sdist.is_virtual() {
                return Ok(Dist::Source(sdist));
            }
        }

        match (no_binary, no_build) {
            (true, true) => Err(LockErrorKind::NoBinaryNoBuild {
                id: self.id.clone(),
            }
            .into()),
            (true, false) if self.id.source.is_wheel() => Err(LockErrorKind::NoBinaryWheelOnly {
                id: self.id.clone(),
            }
            .into()),
            (true, false) => Err(LockErrorKind::NoBinary {
                id: self.id.clone(),
            }
            .into()),
            (false, true) => Err(LockErrorKind::NoBuild {
                id: self.id.clone(),
            }
            .into()),
            (false, false) if self.id.source.is_wheel() => Err(LockError {
                kind: Box::new(LockErrorKind::IncompatibleWheelOnly {
                    id: self.id.clone(),
                }),
                hint: self.tag_hint(tag_policy),
            }),
            (false, false) => Err(LockError {
                kind: Box::new(LockErrorKind::NeitherSourceDistNorWheel {
                    id: self.id.clone(),
                }),
                hint: self.tag_hint(tag_policy),
            }),
        }
    }

    /// Generate a [`WheelTagHint`] based on wheel-tag incompatibilities.
    fn tag_hint(&self, tag_policy: TagPolicy<'_>) -> Option<WheelTagHint> {
        let filenames = self
            .wheels
            .iter()
            .map(|wheel| &wheel.filename)
            .collect::<Vec<_>>();
        WheelTagHint::from_wheels(
            &self.id.name,
            self.id.version.as_ref(),
            &filenames,
            tag_policy.tags(),
        )
    }

    /// Convert the source of this [`Package`] to a [`SourceDist`] that can be used in installation.
    ///
    /// Returns `Ok(None)` if the source cannot be converted because `self.sdist` is `None`. This is required
    /// for registry sources.
    fn to_source_dist(
        &self,
        workspace_root: &Path,
    ) -> Result<Option<uv_distribution_types::SourceDist>, LockError> {
        let sdist = match &self.id.source {
            Source::Path(path) => {
                // A direct path source can also be a wheel, so validate the extension.
                let DistExtension::Source(ext) = DistExtension::from_path(path).map_err(|err| {
                    LockErrorKind::MissingExtension {
                        id: self.id.clone(),
                        err,
                    }
                })?
                else {
                    return Ok(None);
                };
                let install_path = absolute_path(workspace_root, path)?;
                let path_dist = PathSourceDist {
                    name: self.id.name.clone(),
                    version: self.id.version.clone(),
                    url: verbatim_url(&install_path, &self.id)?,
                    install_path: install_path.into_boxed_path(),
                    ext,
                };
                uv_distribution_types::SourceDist::Path(path_dist)
            }
            Source::Directory(path) => {
                let install_path = absolute_path(workspace_root, path)?;
                let dir_dist = DirectorySourceDist {
                    name: self.id.name.clone(),
                    url: verbatim_url(&install_path, &self.id)?,
                    install_path: install_path.into_boxed_path(),
                    editable: Some(false),
                    r#virtual: Some(false),
                };
                uv_distribution_types::SourceDist::Directory(dir_dist)
            }
            Source::Editable(path) => {
                let install_path = absolute_path(workspace_root, path)?;
                let dir_dist = DirectorySourceDist {
                    name: self.id.name.clone(),
                    url: verbatim_url(&install_path, &self.id)?,
                    install_path: install_path.into_boxed_path(),
                    editable: Some(true),
                    r#virtual: Some(false),
                };
                uv_distribution_types::SourceDist::Directory(dir_dist)
            }
            Source::Virtual(path) => {
                let install_path = absolute_path(workspace_root, path)?;
                let dir_dist = DirectorySourceDist {
                    name: self.id.name.clone(),
                    url: verbatim_url(&install_path, &self.id)?,
                    install_path: install_path.into_boxed_path(),
                    editable: Some(false),
                    r#virtual: Some(true),
                };
                uv_distribution_types::SourceDist::Directory(dir_dist)
            }
            Source::Git(url, git) => {
                // Remove the fragment and query from the URL; they're already present in the
                // `GitSource`.
                let mut url = url.to_url().map_err(LockErrorKind::InvalidUrl)?;
                url.set_fragment(None);
                url.set_query(None);

                // Reconstruct the `GitUrl` from the `GitSource`.
                let git_url =
                    GitUrl::from_commit(url, GitReference::from(git.kind.clone()), git.precise)?;

                // Reconstruct the PEP 508-compatible URL from the `GitSource`.
                let url = DisplaySafeUrl::from(ParsedGitUrl {
                    url: git_url.clone(),
                    subdirectory: git.subdirectory.clone(),
                });

                let git_dist = GitSourceDist {
                    name: self.id.name.clone(),
                    url: VerbatimUrl::from_url(url),
                    git: Box::new(git_url),
                    subdirectory: git.subdirectory.clone(),
                };
                uv_distribution_types::SourceDist::Git(git_dist)
            }
            Source::Direct(url, direct) => {
                // A direct URL source can also be a wheel, so validate the extension.
                let DistExtension::Source(ext) =
                    DistExtension::from_path(url.base_str()).map_err(|err| {
                        LockErrorKind::MissingExtension {
                            id: self.id.clone(),
                            err,
                        }
                    })?
                else {
                    return Ok(None);
                };
                let location = url.to_url().map_err(LockErrorKind::InvalidUrl)?;
                let url = DisplaySafeUrl::from(ParsedArchiveUrl {
                    url: location.clone(),
                    subdirectory: direct.subdirectory.clone(),
                    ext: DistExtension::Source(ext),
                });
                let direct_dist = DirectUrlSourceDist {
                    name: self.id.name.clone(),
                    location: Box::new(location),
                    subdirectory: direct.subdirectory.clone(),
                    ext,
                    url: VerbatimUrl::from_url(url),
                };
                uv_distribution_types::SourceDist::DirectUrl(direct_dist)
            }
            Source::Registry(RegistrySource::Url(url)) => {
                let Some(ref sdist) = self.sdist else {
                    return Ok(None);
                };

                let name = &self.id.name;
                let version = self
                    .id
                    .version
                    .as_ref()
                    .expect("version for registry source");

                let file_url = sdist.url().ok_or_else(|| LockErrorKind::MissingUrl {
                    name: name.clone(),
                    version: version.clone(),
                })?;
                let filename = sdist
                    .filename()
                    .ok_or_else(|| LockErrorKind::MissingFilename {
                        id: self.id.clone(),
                    })?;
                let ext = SourceDistExtension::from_path(filename.as_ref()).map_err(|err| {
                    LockErrorKind::MissingExtension {
                        id: self.id.clone(),
                        err,
                    }
                })?;
                let file = Box::new(uv_distribution_types::File {
                    dist_info_metadata: false,
                    filename: SmallString::from(filename),
                    hashes: sdist.hash().map_or(HashDigests::empty(), |hash| {
                        HashDigests::from(hash.0.clone())
                    }),
                    requires_python: None,
                    size: sdist.size(),
                    upload_time_utc_ms: sdist.upload_time().map(Timestamp::as_millisecond),
                    url: FileLocation::AbsoluteUrl(file_url.clone()),
                    yanked: None,
                    zstd: None,
                });

                let index = IndexUrl::from(VerbatimUrl::from_url(
                    url.to_url().map_err(LockErrorKind::InvalidUrl)?,
                ));

                let reg_dist = RegistrySourceDist {
                    name: name.clone(),
                    version: version.clone(),
                    file,
                    ext,
                    index,
                    wheels: vec![],
                };
                uv_distribution_types::SourceDist::Registry(reg_dist)
            }
            Source::Registry(RegistrySource::Path(path)) => {
                let Some(ref sdist) = self.sdist else {
                    return Ok(None);
                };

                let name = &self.id.name;
                let version = self
                    .id
                    .version
                    .as_ref()
                    .expect("version for registry source");

                let file_url = match sdist {
                    SourceDist::Url { url: file_url, .. } => {
                        FileLocation::AbsoluteUrl(file_url.clone())
                    }
                    SourceDist::Path {
                        path: file_path, ..
                    } => {
                        let file_path = workspace_root.join(path).join(file_path);
                        let file_url =
                            DisplaySafeUrl::from_file_path(&file_path).map_err(|()| {
                                LockErrorKind::PathToUrl {
                                    path: file_path.into_boxed_path(),
                                }
                            })?;
                        FileLocation::AbsoluteUrl(UrlString::from(file_url))
                    }
                    SourceDist::Metadata { .. } => {
                        return Err(LockErrorKind::MissingPath {
                            name: name.clone(),
                            version: version.clone(),
                        }
                        .into());
                    }
                };
                let filename = sdist
                    .filename()
                    .ok_or_else(|| LockErrorKind::MissingFilename {
                        id: self.id.clone(),
                    })?;
                let ext = SourceDistExtension::from_path(filename.as_ref()).map_err(|err| {
                    LockErrorKind::MissingExtension {
                        id: self.id.clone(),
                        err,
                    }
                })?;
                let file = Box::new(uv_distribution_types::File {
                    dist_info_metadata: false,
                    filename: SmallString::from(filename),
                    hashes: sdist.hash().map_or(HashDigests::empty(), |hash| {
                        HashDigests::from(hash.0.clone())
                    }),
                    requires_python: None,
                    size: sdist.size(),
                    upload_time_utc_ms: sdist.upload_time().map(Timestamp::as_millisecond),
                    url: file_url,
                    yanked: None,
                    zstd: None,
                });

                let index = IndexUrl::from(
                    VerbatimUrl::from_absolute_path(workspace_root.join(path))
                        .map_err(LockErrorKind::RegistryVerbatimUrl)?,
                );

                let reg_dist = RegistrySourceDist {
                    name: name.clone(),
                    version: version.clone(),
                    file,
                    ext,
                    index,
                    wheels: vec![],
                };
                uv_distribution_types::SourceDist::Registry(reg_dist)
            }
        };

        Ok(Some(sdist))
    }

    /// Convert to a [`RegistryVariantsJson`] for installation.
    pub(crate) fn to_registry_variants_json(
        &self,
        workspace_root: &Path,
    ) -> Result<Option<RegistryVariantsJson>, LockError> {
        let Some(variants_json) = &self.variants_json else {
            return Ok(None);
        };

        let name = &self.id.name;
        let version = self
            .id
            .version
            .as_ref()
            .expect("version for registry source");
        let (file_url, index) = match &self.id.source {
            Source::Registry(RegistrySource::Url(url)) => {
                let file_url =
                    variants_json
                        .url
                        .url()
                        .ok_or_else(|| LockErrorKind::MissingUrl {
                            name: name.clone(),
                            version: version.clone(),
                        })?;
                let index = IndexUrl::from(VerbatimUrl::from_url(
                    url.to_url().map_err(LockErrorKind::InvalidUrl)?,
                ));
                (FileLocation::AbsoluteUrl(file_url.clone()), index)
            }
            Source::Registry(RegistrySource::Path(path)) => {
                let index = IndexUrl::from(
                    VerbatimUrl::from_absolute_path(workspace_root.join(path))
                        .map_err(LockErrorKind::RegistryVerbatimUrl)?,
                );
                match &variants_json.url {
                    VariantsJsonSource::Url { url: file_url } => {
                        (FileLocation::AbsoluteUrl(file_url.clone()), index)
                    }
                    VariantsJsonSource::Path { path: file_path } => {
                        let file_path = workspace_root.join(path).join(file_path);
                        let file_url =
                            DisplaySafeUrl::from_file_path(&file_path).map_err(|()| {
                                LockErrorKind::PathToUrl {
                                    path: file_path.into_boxed_path(),
                                }
                            })?;
                        (FileLocation::AbsoluteUrl(UrlString::from(file_url)), index)
                    }
                }
            }
            _ => todo!("Handle error: variants.json can only be used on a registry source"),
        };

        let filename = format!("{name}-{version}-variants.json");
        let file = File {
            dist_info_metadata: false,
            filename: SmallString::from(filename),
            hashes: variants_json.hash.iter().map(|h| h.0.clone()).collect(),
            requires_python: None,
            size: variants_json.size,
            upload_time_utc_ms: variants_json.upload_time.map(Timestamp::as_millisecond),
            url: file_url,
            yanked: None,
            zstd: None,
        };
        Ok(Some(RegistryVariantsJson {
            filename: VariantsJson {
                name: self.name().clone(),
                version: version.clone(),
            },
            file: Box::new(file),
            index,
        }))
    }

    fn to_toml(
        &self,
        requires_python: &RequiresPython,
        dist_count_by_name: &FxHashMap<PackageName, u64>,
    ) -> Result<Table, toml_edit::ser::Error> {
        let mut table = Table::new();

        self.id.to_toml(None, &mut table);

        if !self.fork_markers.is_empty() {
            let fork_markers = each_element_on_its_line_array(
                simplified_universal_markers(&self.fork_markers, requires_python).into_iter(),
            );
            if !fork_markers.is_empty() {
                table.insert("resolution-markers", value(fork_markers));
            }
        }

        if !self.dependencies.is_empty() {
            let deps = each_element_on_its_line_array(self.dependencies.iter().map(|dep| {
                dep.to_toml(requires_python, dist_count_by_name)
                    .into_inline_table()
            }));
            table.insert("dependencies", value(deps));
        }

        if !self.optional_dependencies.is_empty() {
            let mut optional_deps = Table::new();
            for (extra, deps) in &self.optional_dependencies {
                let deps = each_element_on_its_line_array(deps.iter().map(|dep| {
                    dep.to_toml(requires_python, dist_count_by_name)
                        .into_inline_table()
                }));
                if !deps.is_empty() {
                    optional_deps.insert(extra.as_ref(), value(deps));
                }
            }
            if !optional_deps.is_empty() {
                table.insert("optional-dependencies", Item::Table(optional_deps));
            }
        }

        if !self.dependency_groups.is_empty() {
            let mut dependency_groups = Table::new();
            for (extra, deps) in &self.dependency_groups {
                let deps = each_element_on_its_line_array(deps.iter().map(|dep| {
                    dep.to_toml(requires_python, dist_count_by_name)
                        .into_inline_table()
                }));
                if !deps.is_empty() {
                    dependency_groups.insert(extra.as_ref(), value(deps));
                }
            }
            if !dependency_groups.is_empty() {
                table.insert("dev-dependencies", Item::Table(dependency_groups));
            }
        }

        if let Some(ref sdist) = self.sdist {
            table.insert("sdist", value(sdist.to_toml()?));
        }

        if !self.wheels.is_empty() {
            let wheels = each_element_on_its_line_array(
                self.wheels
                    .iter()
                    .map(Wheel::to_toml)
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter(),
            );
            table.insert("wheels", value(wheels));
        }

        if let Some(ref variants_json) = self.variants_json {
            table.insert("variants-json", value(variants_json.to_toml()?));
        }

        // Write the package metadata, if non-empty.
        {
            let mut metadata_table = Table::new();

            if !self.metadata.requires_dist.is_empty() {
                let requires_dist = self
                    .metadata
                    .requires_dist
                    .iter()
                    .map(|requirement| {
                        serde::Serialize::serialize(
                            &requirement,
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let requires_dist = match requires_dist.as_slice() {
                    [] => Array::new(),
                    [requirement] => Array::from_iter([requirement]),
                    requires_dist => each_element_on_its_line_array(requires_dist.iter()),
                };
                metadata_table.insert("requires-dist", value(requires_dist));
            }

            if !self.metadata.dependency_groups.is_empty() {
                let mut dependency_groups = Table::new();
                for (extra, deps) in &self.metadata.dependency_groups {
                    let deps = deps
                        .iter()
                        .map(|requirement| {
                            serde::Serialize::serialize(
                                &requirement,
                                toml_edit::ser::ValueSerializer::new(),
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let deps = match deps.as_slice() {
                        [] => Array::new(),
                        [requirement] => Array::from_iter([requirement]),
                        deps => each_element_on_its_line_array(deps.iter()),
                    };
                    dependency_groups.insert(extra.as_ref(), value(deps));
                }
                if !dependency_groups.is_empty() {
                    metadata_table.insert("requires-dev", Item::Table(dependency_groups));
                }
            }

            if !self.metadata.provides_extras.is_empty() {
                let provides_extras = self
                    .metadata
                    .provides_extras
                    .iter()
                    .map(|extra| {
                        serde::Serialize::serialize(&extra, toml_edit::ser::ValueSerializer::new())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                // This is just a list of names, so linebreaking it is excessive.
                let provides_extras = Array::from_iter(provides_extras);
                metadata_table.insert("provides-extras", value(provides_extras));
            }

            if !metadata_table.is_empty() {
                table.insert("metadata", Item::Table(metadata_table));
            }
        }

        Ok(table)
    }

    fn find_best_wheel(&self, tag_policy: TagPolicy<'_>) -> Option<usize> {
        type WheelPriority<'lock> = (bool, TagPriority, Option<&'lock BuildTag>);

        let mut best: Option<(WheelPriority, usize)> = None;
        for (i, wheel) in self.wheels.iter().enumerate() {
            let TagCompatibility::Compatible(tag_priority) =
                wheel.filename.compatibility(tag_policy.tags())
            else {
                continue;
            };
            let build_tag = wheel.filename.build_tag();
            // Non-variant wheels before variant wheels.
            let wheel_priority = (wheel.filename.variant().is_none(), tag_priority, build_tag);
            match best {
                None => {
                    best = Some((wheel_priority, i));
                }
                Some((best_priority, _)) => {
                    if wheel_priority > best_priority {
                        best = Some((wheel_priority, i));
                    }
                }
            }
        }

        let best = best.map(|(_, i)| i);
        match tag_policy {
            TagPolicy::Required(_) => best,
            TagPolicy::Preferred(_) => best.or_else(|| self.wheels.first().map(|_| 0)),
        }
    }

    /// Returns the [`PackageName`] of the package.
    pub fn name(&self) -> &PackageName {
        &self.id.name
    }

    /// Returns the [`Version`] of the package.
    pub fn version(&self) -> Option<&Version> {
        self.id.version.as_ref()
    }

    /// Return the fork markers for this package, if any.
    pub fn fork_markers(&self) -> &[UniversalMarker] {
        self.fork_markers.as_slice()
    }

    /// Returns the [`IndexUrl`] for the package, if it is a registry source.
    pub fn index(&self, root: &Path) -> Result<Option<IndexUrl>, LockError> {
        match &self.id.source {
            Source::Registry(RegistrySource::Url(url)) => {
                let index = IndexUrl::from(VerbatimUrl::from_url(
                    url.to_url().map_err(LockErrorKind::InvalidUrl)?,
                ));
                Ok(Some(index))
            }
            Source::Registry(RegistrySource::Path(path)) => {
                let index = IndexUrl::from(
                    VerbatimUrl::from_absolute_path(root.join(path))
                        .map_err(LockErrorKind::RegistryVerbatimUrl)?,
                );
                Ok(Some(index))
            }
            _ => Ok(None),
        }
    }

    /// Returns all the hashes associated with this [`Package`].
    fn hashes(&self) -> HashDigests {
        let mut hashes = Vec::with_capacity(
            usize::from(self.sdist.as_ref().and_then(|sdist| sdist.hash()).is_some())
                + self
                    .wheels
                    .iter()
                    .map(|wheel| usize::from(wheel.hash.is_some()))
                    .sum::<usize>(),
        );
        if let Some(ref sdist) = self.sdist {
            if let Some(hash) = sdist.hash() {
                hashes.push(hash.0.clone());
            }
        }
        for wheel in &self.wheels {
            hashes.extend(wheel.hash.as_ref().map(|h| h.0.clone()));
            if let Some(zstd) = wheel.zstd.as_ref() {
                hashes.extend(zstd.hash.as_ref().map(|h| h.0.clone()));
            }
        }
        HashDigests::from(hashes)
    }

    /// Returns the [`ResolvedRepositoryReference`] for the package, if it is a Git source.
    pub fn as_git_ref(&self) -> Result<Option<ResolvedRepositoryReference>, LockError> {
        match &self.id.source {
            Source::Git(url, git) => Ok(Some(ResolvedRepositoryReference {
                reference: RepositoryReference {
                    url: RepositoryUrl::new(&url.to_url().map_err(LockErrorKind::InvalidUrl)?),
                    reference: GitReference::from(git.kind.clone()),
                },
                sha: git.precise,
            })),
            _ => Ok(None),
        }
    }

    /// Returns `true` if the package is a dynamic source tree.
    fn is_dynamic(&self) -> bool {
        self.id.version.is_none()
    }

    /// Returns the extras the package provides, if any.
    pub fn provides_extras(&self) -> &[ExtraName] {
        &self.metadata.provides_extras
    }

    /// Returns the dependency groups the package provides, if any.
    pub fn dependency_groups(&self) -> &BTreeMap<GroupName, BTreeSet<Requirement>> {
        &self.metadata.dependency_groups
    }

    /// Returns the dependencies of the package.
    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }

    /// Returns the optional dependencies of the package.
    pub fn optional_dependencies(&self) -> &BTreeMap<ExtraName, Vec<Dependency>> {
        &self.optional_dependencies
    }

    /// Returns the resolved PEP 735 dependency groups of the package.
    pub fn resolved_dependency_groups(&self) -> &BTreeMap<GroupName, Vec<Dependency>> {
        &self.dependency_groups
    }

    /// Returns an [`InstallTarget`] view for filtering decisions.
    pub fn as_install_target(&self) -> InstallTarget<'_> {
        InstallTarget {
            name: self.name(),
            is_local: self.id.source.is_local(),
        }
    }
}

/// Attempts to construct a `VerbatimUrl` from the given normalized `Path`.
fn verbatim_url(path: &Path, id: &PackageId) -> Result<VerbatimUrl, LockError> {
    let url =
        VerbatimUrl::from_normalized_path(path).map_err(|err| LockErrorKind::VerbatimUrl {
            id: id.clone(),
            err,
        })?;
    Ok(url)
}

/// Attempts to construct an absolute path from the given `Path`.
fn absolute_path(workspace_root: &Path, path: &Path) -> Result<PathBuf, LockError> {
    let path = uv_fs::normalize_absolute_path(&workspace_root.join(path))
        .map_err(LockErrorKind::AbsolutePath)?;
    Ok(path)
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PackageWire {
    #[serde(flatten)]
    id: PackageId,
    #[serde(default)]
    metadata: PackageMetadata,
    #[serde(default)]
    sdist: Option<SourceDist>,
    #[serde(default)]
    wheels: Vec<Wheel>,
    #[serde(default, rename = "variants-json")]
    variants_json: Option<VariantsJsonEntry>,
    #[serde(default, rename = "resolution-markers")]
    fork_markers: Vec<SimplifiedMarkerTree>,
    #[serde(default)]
    dependencies: Vec<DependencyWire>,
    #[serde(default)]
    optional_dependencies: BTreeMap<ExtraName, Vec<DependencyWire>>,
    #[serde(default, rename = "dev-dependencies", alias = "dependency-groups")]
    dependency_groups: BTreeMap<GroupName, Vec<DependencyWire>>,
}

#[derive(Clone, Default, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PackageMetadata {
    #[serde(default)]
    requires_dist: BTreeSet<Requirement>,
    #[serde(default)]
    provides_extras: Box<[ExtraName]>,
    #[serde(default, rename = "requires-dev", alias = "dependency-groups")]
    dependency_groups: BTreeMap<GroupName, BTreeSet<Requirement>>,
}

impl PackageMetadata {
    fn unwire(self, requires_python: &RequiresPython) -> Self {
        // We need to complexify these markers so things like
        // `requires_python < '0'` get normalized to False
        let unwire_requirements = |requirements: BTreeSet<Requirement>| -> BTreeSet<Requirement> {
            requirements
                .into_iter()
                .map(|mut requirement| {
                    let complexified_marker =
                        requires_python.complexify_markers(requirement.marker);
                    requirement.marker = complexified_marker;
                    requirement
                })
                .collect()
        };

        Self {
            requires_dist: unwire_requirements(self.requires_dist),
            provides_extras: self.provides_extras,
            dependency_groups: self
                .dependency_groups
                .into_iter()
                .map(|(group, requirements)| (group, unwire_requirements(requirements)))
                .collect(),
        }
    }
}

impl PackageWire {
    fn unwire(
        self,
        requires_python: &RequiresPython,
        unambiguous_package_ids: &FxHashMap<PackageName, PackageId>,
    ) -> Result<Package, LockError> {
        // Consistency check
        if let Some(version) = &self.id.version {
            for wheel in &self.wheels {
                if *version != wheel.filename.version
                    && *version != wheel.filename.version.clone().without_local()
                {
                    return Err(LockError::from(LockErrorKind::InconsistentVersions {
                        name: self.id.name,
                        version: version.clone(),
                        wheel: wheel.clone(),
                    }));
                }
            }
            // We can't check the source dist version since it does not need to contain the version
            // in the filename.
        }

        let unwire_deps = |deps: Vec<DependencyWire>| -> Result<Vec<Dependency>, LockError> {
            deps.into_iter()
                .map(|dep| dep.unwire(requires_python, unambiguous_package_ids))
                .collect()
        };

        Ok(Package {
            id: self.id,
            metadata: self.metadata.unwire(requires_python),
            sdist: self.sdist,
            wheels: self.wheels,
            variants_json: self.variants_json,
            fork_markers: self
                .fork_markers
                .into_iter()
                .map(|simplified_marker| simplified_marker.into_marker(requires_python))
                .map(UniversalMarker::from_combined)
                .collect(),
            dependencies: unwire_deps(self.dependencies)?,
            optional_dependencies: self
                .optional_dependencies
                .into_iter()
                .map(|(extra, deps)| Ok((extra, unwire_deps(deps)?)))
                .collect::<Result<_, LockError>>()?,
            dependency_groups: self
                .dependency_groups
                .into_iter()
                .map(|(group, deps)| Ok((group, unwire_deps(deps)?)))
                .collect::<Result<_, LockError>>()?,
        })
    }
}

/// Inside the lockfile, we match a dependency entry to a package entry through a key made up
/// of the name, the version and the source url.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct PackageId {
    pub(crate) name: PackageName,
    pub(crate) version: Option<Version>,
    source: Source,
}

impl PackageId {
    fn from_annotated_dist(annotated_dist: &AnnotatedDist, root: &Path) -> Result<Self, LockError> {
        // Identify the source of the package.
        let source = Source::from_resolved_dist(&annotated_dist.dist, root)?;
        // Omit versions for dynamic source trees.
        let version = if source.is_source_tree()
            && annotated_dist
                .metadata
                .as_ref()
                .is_some_and(|metadata| metadata.dynamic)
        {
            None
        } else {
            Some(annotated_dist.version.clone())
        };
        let name = annotated_dist.name.clone();
        Ok(Self {
            name,
            version,
            source,
        })
    }

    /// Writes this package ID inline into the table given.
    ///
    /// When a map is given, and if the package name in this ID is unambiguous
    /// (i.e., it has a count of 1 in the map), then the `version` and `source`
    /// fields are omitted. In all other cases, including when a map is not
    /// given, the `version` and `source` fields are written.
    fn to_toml(&self, dist_count_by_name: Option<&FxHashMap<PackageName, u64>>, table: &mut Table) {
        let count = dist_count_by_name.and_then(|map| map.get(&self.name).copied());
        table.insert("name", value(self.name.to_string()));
        if count.map(|count| count > 1).unwrap_or(true) {
            if let Some(version) = &self.version {
                table.insert("version", value(version.to_string()));
            }
            self.source.to_toml(table);
        }
    }
}

impl Display for PackageId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(version) = &self.version {
            write!(f, "{}=={} @ {}", self.name, version, self.source)
        } else {
            write!(f, "{} @ {}", self.name, self.source)
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PackageIdForDependency {
    name: PackageName,
    version: Option<Version>,
    source: Option<Source>,
}

impl PackageIdForDependency {
    fn unwire(
        self,
        unambiguous_package_ids: &FxHashMap<PackageName, PackageId>,
    ) -> Result<PackageId, LockError> {
        let unambiguous_package_id = unambiguous_package_ids.get(&self.name);
        let source = self.source.map(Ok::<_, LockError>).unwrap_or_else(|| {
            let Some(package_id) = unambiguous_package_id else {
                return Err(LockErrorKind::MissingDependencySource {
                    name: self.name.clone(),
                }
                .into());
            };
            Ok(package_id.source.clone())
        })?;
        let version = if let Some(version) = self.version {
            Some(version)
        } else {
            if let Some(package_id) = unambiguous_package_id {
                package_id.version.clone()
            } else {
                // If the package is a source tree, assume that the missing `self.version` field is
                // indicative of a dynamic version.
                if source.is_source_tree() {
                    None
                } else {
                    return Err(LockErrorKind::MissingDependencyVersion {
                        name: self.name.clone(),
                    }
                    .into());
                }
            }
        };
        Ok(PackageId {
            name: self.name,
            version,
            source,
        })
    }
}

impl From<PackageId> for PackageIdForDependency {
    fn from(id: PackageId) -> Self {
        Self {
            name: id.name,
            version: id.version,
            source: Some(id.source),
        }
    }
}

/// A unique identifier to differentiate between different sources for the same version of a
/// package.
///
/// NOTE: Care should be taken when adding variants to this enum. Namely, new
/// variants should be added without changing the relative ordering of other
/// variants. Otherwise, this could cause the lockfile to have a different
/// canonical ordering of sources.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
#[serde(try_from = "SourceWire")]
enum Source {
    /// A registry or `--find-links` index.
    Registry(RegistrySource),
    /// A Git repository.
    Git(UrlString, GitSource),
    /// A direct HTTP(S) URL.
    Direct(UrlString, DirectSource),
    /// A path to a local source or built archive.
    Path(Box<Path>),
    /// A path to a local directory.
    Directory(Box<Path>),
    /// A path to a local directory that should be installed as editable.
    Editable(Box<Path>),
    /// A path to a local directory that should not be built or installed.
    Virtual(Box<Path>),
}

impl Source {
    fn from_resolved_dist(resolved_dist: &ResolvedDist, root: &Path) -> Result<Self, LockError> {
        match *resolved_dist {
            // We pass empty installed packages for locking.
            ResolvedDist::Installed { .. } => unreachable!(),
            ResolvedDist::Installable { ref dist, .. } => Self::from_dist(dist, root),
        }
    }

    fn from_dist(dist: &Dist, root: &Path) -> Result<Self, LockError> {
        match *dist {
            Dist::Built(ref built_dist) => Self::from_built_dist(built_dist, root),
            Dist::Source(ref source_dist) => Self::from_source_dist(source_dist, root),
        }
    }

    fn from_built_dist(built_dist: &BuiltDist, root: &Path) -> Result<Self, LockError> {
        match *built_dist {
            BuiltDist::Registry(ref reg_dist) => Self::from_registry_built_dist(reg_dist, root),
            BuiltDist::DirectUrl(ref direct_dist) => Ok(Self::from_direct_built_dist(direct_dist)),
            BuiltDist::Path(ref path_dist) => Self::from_path_built_dist(path_dist, root),
        }
    }

    fn from_source_dist(
        source_dist: &uv_distribution_types::SourceDist,
        root: &Path,
    ) -> Result<Self, LockError> {
        match *source_dist {
            uv_distribution_types::SourceDist::Registry(ref reg_dist) => {
                Self::from_registry_source_dist(reg_dist, root)
            }
            uv_distribution_types::SourceDist::DirectUrl(ref direct_dist) => {
                Ok(Self::from_direct_source_dist(direct_dist))
            }
            uv_distribution_types::SourceDist::Git(ref git_dist) => {
                Ok(Self::from_git_dist(git_dist))
            }
            uv_distribution_types::SourceDist::Path(ref path_dist) => {
                Self::from_path_source_dist(path_dist, root)
            }
            uv_distribution_types::SourceDist::Directory(ref directory) => {
                Self::from_directory_source_dist(directory, root)
            }
        }
    }

    fn from_registry_built_dist(
        reg_dist: &RegistryBuiltDist,
        root: &Path,
    ) -> Result<Self, LockError> {
        Self::from_index_url(&reg_dist.best_wheel().index, root)
    }

    fn from_registry_source_dist(
        reg_dist: &RegistrySourceDist,
        root: &Path,
    ) -> Result<Self, LockError> {
        Self::from_index_url(&reg_dist.index, root)
    }

    fn from_direct_built_dist(direct_dist: &DirectUrlBuiltDist) -> Self {
        Self::Direct(
            normalize_url(direct_dist.url.to_url()),
            DirectSource { subdirectory: None },
        )
    }

    fn from_direct_source_dist(direct_dist: &DirectUrlSourceDist) -> Self {
        Self::Direct(
            normalize_url(direct_dist.url.to_url()),
            DirectSource {
                subdirectory: direct_dist.subdirectory.clone(),
            },
        )
    }

    fn from_path_built_dist(path_dist: &PathBuiltDist, root: &Path) -> Result<Self, LockError> {
        let path = relative_to(&path_dist.install_path, root)
            .or_else(|_| std::path::absolute(&path_dist.install_path))
            .map_err(LockErrorKind::DistributionRelativePath)?;
        Ok(Self::Path(path.into_boxed_path()))
    }

    fn from_path_source_dist(path_dist: &PathSourceDist, root: &Path) -> Result<Self, LockError> {
        let path = relative_to(&path_dist.install_path, root)
            .or_else(|_| std::path::absolute(&path_dist.install_path))
            .map_err(LockErrorKind::DistributionRelativePath)?;
        Ok(Self::Path(path.into_boxed_path()))
    }

    fn from_directory_source_dist(
        directory_dist: &DirectorySourceDist,
        root: &Path,
    ) -> Result<Self, LockError> {
        let path = relative_to(&directory_dist.install_path, root)
            .or_else(|_| std::path::absolute(&directory_dist.install_path))
            .map_err(LockErrorKind::DistributionRelativePath)?;
        if directory_dist.editable.unwrap_or(false) {
            Ok(Self::Editable(path.into_boxed_path()))
        } else if directory_dist.r#virtual.unwrap_or(false) {
            Ok(Self::Virtual(path.into_boxed_path()))
        } else {
            Ok(Self::Directory(path.into_boxed_path()))
        }
    }

    fn from_index_url(index_url: &IndexUrl, root: &Path) -> Result<Self, LockError> {
        match index_url {
            IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                // Remove any sensitive credentials from the index URL.
                let redacted = index_url.without_credentials();
                let source = RegistrySource::Url(UrlString::from(redacted.as_ref()));
                Ok(Self::Registry(source))
            }
            IndexUrl::Path(url) => {
                let path = url
                    .to_file_path()
                    .map_err(|()| LockErrorKind::UrlToPath { url: url.to_url() })?;
                let path = relative_to(&path, root)
                    .or_else(|_| std::path::absolute(&path))
                    .map_err(LockErrorKind::IndexRelativePath)?;
                let source = RegistrySource::Path(path.into_boxed_path());
                Ok(Self::Registry(source))
            }
        }
    }

    fn from_git_dist(git_dist: &GitSourceDist) -> Self {
        Self::Git(
            UrlString::from(locked_git_url(git_dist)),
            GitSource {
                kind: GitSourceKind::from(git_dist.git.reference().clone()),
                precise: git_dist.git.precise().unwrap_or_else(|| {
                    panic!("Git distribution is missing a precise hash: {git_dist}")
                }),
                subdirectory: git_dist.subdirectory.clone(),
            },
        )
    }

    /// Returns `true` if the source should be considered immutable.
    ///
    /// We assume that registry sources are immutable. In other words, we expect that once a
    /// package-version is published to a registry, its metadata will not change.
    ///
    /// We also assume that Git sources are immutable, since a Git source encodes a specific commit.
    fn is_immutable(&self) -> bool {
        matches!(self, Self::Registry(..) | Self::Git(_, _))
    }

    /// Returns `true` if the source is that of a wheel.
    fn is_wheel(&self) -> bool {
        match self {
            Self::Path(path) => {
                matches!(
                    DistExtension::from_path(path).ok(),
                    Some(DistExtension::Wheel)
                )
            }
            Self::Direct(url, _) => {
                matches!(
                    DistExtension::from_path(url.as_ref()).ok(),
                    Some(DistExtension::Wheel)
                )
            }
            Self::Directory(..) => false,
            Self::Editable(..) => false,
            Self::Virtual(..) => false,
            Self::Git(..) => false,
            Self::Registry(..) => false,
        }
    }

    /// Returns `true` if the source is that of a source tree.
    fn is_source_tree(&self) -> bool {
        match self {
            Self::Directory(..) | Self::Editable(..) | Self::Virtual(..) => true,
            Self::Path(..) | Self::Git(..) | Self::Registry(..) | Self::Direct(..) => false,
        }
    }

    /// Returns the path to the source tree, if the source is a source tree.
    fn as_source_tree(&self) -> Option<&Path> {
        match self {
            Self::Directory(path) | Self::Editable(path) | Self::Virtual(path) => Some(path),
            Self::Path(..) | Self::Git(..) | Self::Registry(..) | Self::Direct(..) => None,
        }
    }

    fn to_toml(&self, table: &mut Table) {
        let mut source_table = InlineTable::new();
        match self {
            Self::Registry(source) => match source {
                RegistrySource::Url(url) => {
                    source_table.insert("registry", Value::from(url.as_ref()));
                }
                RegistrySource::Path(path) => {
                    source_table.insert(
                        "registry",
                        Value::from(PortablePath::from(path).to_string()),
                    );
                }
            },
            Self::Git(url, _) => {
                source_table.insert("git", Value::from(url.as_ref()));
            }
            Self::Direct(url, DirectSource { subdirectory }) => {
                source_table.insert("url", Value::from(url.as_ref()));
                if let Some(ref subdirectory) = *subdirectory {
                    source_table.insert(
                        "subdirectory",
                        Value::from(PortablePath::from(subdirectory).to_string()),
                    );
                }
            }
            Self::Path(path) => {
                source_table.insert("path", Value::from(PortablePath::from(path).to_string()));
            }
            Self::Directory(path) => {
                source_table.insert(
                    "directory",
                    Value::from(PortablePath::from(path).to_string()),
                );
            }
            Self::Editable(path) => {
                source_table.insert(
                    "editable",
                    Value::from(PortablePath::from(path).to_string()),
                );
            }
            Self::Virtual(path) => {
                source_table.insert("virtual", Value::from(PortablePath::from(path).to_string()));
            }
        }
        table.insert("source", value(source_table));
    }

    /// Check if a package is local by examining its source.
    pub(crate) fn is_local(&self) -> bool {
        matches!(
            self,
            Self::Path(_) | Self::Directory(_) | Self::Editable(_) | Self::Virtual(_)
        )
    }
}

impl Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Registry(RegistrySource::Url(url)) | Self::Git(url, _) | Self::Direct(url, _) => {
                write!(f, "{}+{}", self.name(), url)
            }
            Self::Registry(RegistrySource::Path(path))
            | Self::Path(path)
            | Self::Directory(path)
            | Self::Editable(path)
            | Self::Virtual(path) => {
                write!(f, "{}+{}", self.name(), PortablePath::from(path))
            }
        }
    }
}

impl Source {
    fn name(&self) -> &str {
        match self {
            Self::Registry(..) => "registry",
            Self::Git(..) => "git",
            Self::Direct(..) => "direct",
            Self::Path(..) => "path",
            Self::Directory(..) => "directory",
            Self::Editable(..) => "editable",
            Self::Virtual(..) => "virtual",
        }
    }

    /// Returns `Some(true)` to indicate that the source kind _must_ include a
    /// hash.
    ///
    /// Returns `Some(false)` to indicate that the source kind _must not_
    /// include a hash.
    ///
    /// Returns `None` to indicate that the source kind _may_ include a hash.
    fn requires_hash(&self) -> Option<bool> {
        match self {
            Self::Registry(..) => None,
            Self::Direct(..) | Self::Path(..) => Some(true),
            Self::Git(..) | Self::Directory(..) | Self::Editable(..) | Self::Virtual(..) => {
                Some(false)
            }
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(untagged, rename_all = "kebab-case")]
enum SourceWire {
    Registry {
        registry: RegistrySourceWire,
    },
    Git {
        git: String,
    },
    Direct {
        url: UrlString,
        subdirectory: Option<PortablePathBuf>,
    },
    Path {
        path: PortablePathBuf,
    },
    Directory {
        directory: PortablePathBuf,
    },
    Editable {
        editable: PortablePathBuf,
    },
    Virtual {
        r#virtual: PortablePathBuf,
    },
}

impl TryFrom<SourceWire> for Source {
    type Error = LockError;

    fn try_from(wire: SourceWire) -> Result<Self, LockError> {
        #[allow(clippy::enum_glob_use)]
        use self::SourceWire::*;

        match wire {
            Registry { registry } => Ok(Self::Registry(registry.into())),
            Git { git } => {
                let url = DisplaySafeUrl::parse(&git)
                    .map_err(|err| SourceParseError::InvalidUrl {
                        given: git.to_string(),
                        err,
                    })
                    .map_err(LockErrorKind::InvalidGitSourceUrl)?;

                let git_source = GitSource::from_url(&url)
                    .map_err(|err| match err {
                        GitSourceError::InvalidSha => SourceParseError::InvalidSha {
                            given: git.to_string(),
                        },
                        GitSourceError::MissingSha => SourceParseError::MissingSha {
                            given: git.to_string(),
                        },
                    })
                    .map_err(LockErrorKind::InvalidGitSourceUrl)?;

                Ok(Self::Git(UrlString::from(url), git_source))
            }
            Direct { url, subdirectory } => Ok(Self::Direct(
                url,
                DirectSource {
                    subdirectory: subdirectory.map(Box::<std::path::Path>::from),
                },
            )),
            Path { path } => Ok(Self::Path(path.into())),
            Directory { directory } => Ok(Self::Directory(directory.into())),
            Editable { editable } => Ok(Self::Editable(editable.into())),
            Virtual { r#virtual } => Ok(Self::Virtual(r#virtual.into())),
        }
    }
}

/// The source for a registry, which could be a URL or a relative path.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
enum RegistrySource {
    /// Ex) `https://pypi.org/simple`
    Url(UrlString),
    /// Ex) `../path/to/local/index`
    Path(Box<Path>),
}

impl Display for RegistrySource {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Url(url) => write!(f, "{url}"),
            Self::Path(path) => write!(f, "{}", path.display()),
        }
    }
}

#[derive(Clone, Debug)]
enum RegistrySourceWire {
    /// Ex) `https://pypi.org/simple`
    Url(UrlString),
    /// Ex) `../path/to/local/index`
    Path(PortablePathBuf),
}

impl<'de> serde::de::Deserialize<'de> for RegistrySourceWire {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = RegistrySourceWire;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a valid URL or a file path")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if split_scheme(value).is_some() {
                    Ok(
                        serde::Deserialize::deserialize(serde::de::value::StrDeserializer::new(
                            value,
                        ))
                        .map(RegistrySourceWire::Url)?,
                    )
                } else {
                    Ok(
                        serde::Deserialize::deserialize(serde::de::value::StrDeserializer::new(
                            value,
                        ))
                        .map(RegistrySourceWire::Path)?,
                    )
                }
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl From<RegistrySourceWire> for RegistrySource {
    fn from(wire: RegistrySourceWire) -> Self {
        match wire {
            RegistrySourceWire::Url(url) => Self::Url(url),
            RegistrySourceWire::Path(path) => Self::Path(path.into()),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct DirectSource {
    subdirectory: Option<Box<Path>>,
}

/// NOTE: Care should be taken when adding variants to this enum. Namely, new
/// variants should be added without changing the relative ordering of other
/// variants. Otherwise, this could cause the lockfile to have a different
/// canonical ordering of package entries.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
struct GitSource {
    precise: GitOid,
    subdirectory: Option<Box<Path>>,
    kind: GitSourceKind,
}

/// An error that occurs when a source string could not be parsed.
#[derive(Clone, Debug, Eq, PartialEq)]
enum GitSourceError {
    InvalidSha,
    MissingSha,
}

impl GitSource {
    /// Extracts a Git source reference from the query pairs and the hash
    /// fragment in the given URL.
    fn from_url(url: &Url) -> Result<Self, GitSourceError> {
        let mut kind = GitSourceKind::DefaultBranch;
        let mut subdirectory = None;
        for (key, val) in url.query_pairs() {
            match &*key {
                "tag" => kind = GitSourceKind::Tag(val.into_owned()),
                "branch" => kind = GitSourceKind::Branch(val.into_owned()),
                "rev" => kind = GitSourceKind::Rev(val.into_owned()),
                "subdirectory" => subdirectory = Some(PortablePathBuf::from(val.as_ref()).into()),
                _ => {}
            }
        }
        let precise = GitOid::from_str(url.fragment().ok_or(GitSourceError::MissingSha)?)
            .map_err(|_| GitSourceError::InvalidSha)?;

        Ok(Self {
            precise,
            subdirectory,
            kind,
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
enum GitSourceKind {
    Tag(String),
    Branch(String),
    Rev(String),
    DefaultBranch,
}

/// Inspired by: <https://discuss.python.org/t/lock-files-again-but-this-time-w-sdists/46593>
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct SourceDistMetadata {
    /// A hash of the source distribution.
    hash: Option<Hash>,
    /// The size of the source distribution in bytes.
    ///
    /// This is only present for source distributions that come from registries.
    size: Option<u64>,
    /// The upload time of the source distribution.
    #[serde(alias = "upload_time")]
    upload_time: Option<Timestamp>,
}

/// A URL or file path where the source dist that was
/// locked against was found. The location does not need to exist in the
/// future, so this should be treated as only a hint to where to look
/// and/or recording where the source dist file originally came from.
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(from = "SourceDistWire")]
enum SourceDist {
    Url {
        url: UrlString,
        #[serde(flatten)]
        metadata: SourceDistMetadata,
    },
    Path {
        path: Box<Path>,
        #[serde(flatten)]
        metadata: SourceDistMetadata,
    },
    Metadata {
        #[serde(flatten)]
        metadata: SourceDistMetadata,
    },
}

impl SourceDist {
    fn filename(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::Metadata { .. } => None,
            Self::Url { url, .. } => url.filename().ok(),
            Self::Path { path, .. } => path.file_name().map(|filename| filename.to_string_lossy()),
        }
    }

    fn url(&self) -> Option<&UrlString> {
        match self {
            Self::Metadata { .. } => None,
            Self::Url { url, .. } => Some(url),
            Self::Path { .. } => None,
        }
    }

    pub(crate) fn hash(&self) -> Option<&Hash> {
        match self {
            Self::Metadata { metadata } => metadata.hash.as_ref(),
            Self::Url { metadata, .. } => metadata.hash.as_ref(),
            Self::Path { metadata, .. } => metadata.hash.as_ref(),
        }
    }

    pub(crate) fn size(&self) -> Option<u64> {
        match self {
            Self::Metadata { metadata } => metadata.size,
            Self::Url { metadata, .. } => metadata.size,
            Self::Path { metadata, .. } => metadata.size,
        }
    }

    pub(crate) fn upload_time(&self) -> Option<Timestamp> {
        match self {
            Self::Metadata { metadata } => metadata.upload_time,
            Self::Url { metadata, .. } => metadata.upload_time,
            Self::Path { metadata, .. } => metadata.upload_time,
        }
    }
}

impl SourceDist {
    fn from_annotated_dist(
        id: &PackageId,
        annotated_dist: &AnnotatedDist,
    ) -> Result<Option<Self>, LockError> {
        match annotated_dist.dist {
            // We pass empty installed packages for locking.
            ResolvedDist::Installed { .. } => unreachable!(),
            ResolvedDist::Installable { ref dist, .. } => Self::from_dist(
                id,
                dist,
                annotated_dist.hashes.as_slice(),
                annotated_dist.index(),
            ),
        }
    }

    fn from_dist(
        id: &PackageId,
        dist: &Dist,
        hashes: &[HashDigest],
        index: Option<&IndexUrl>,
    ) -> Result<Option<Self>, LockError> {
        match *dist {
            Dist::Built(BuiltDist::Registry(ref built_dist)) => {
                let Some(sdist) = built_dist.sdist.as_ref() else {
                    return Ok(None);
                };
                Self::from_registry_dist(sdist, index)
            }
            Dist::Built(_) => Ok(None),
            Dist::Source(ref source_dist) => Self::from_source_dist(id, source_dist, hashes, index),
        }
    }

    fn from_source_dist(
        id: &PackageId,
        source_dist: &uv_distribution_types::SourceDist,
        hashes: &[HashDigest],
        index: Option<&IndexUrl>,
    ) -> Result<Option<Self>, LockError> {
        match *source_dist {
            uv_distribution_types::SourceDist::Registry(ref reg_dist) => {
                Self::from_registry_dist(reg_dist, index)
            }
            uv_distribution_types::SourceDist::DirectUrl(_) => {
                Self::from_direct_dist(id, hashes).map(Some)
            }
            uv_distribution_types::SourceDist::Path(_) => {
                Self::from_path_dist(id, hashes).map(Some)
            }
            // An actual sdist entry in the lockfile is only required when
            // it's from a registry or a direct URL. Otherwise, it's strictly
            // redundant with the information in all other kinds of `source`.
            uv_distribution_types::SourceDist::Git(_)
            | uv_distribution_types::SourceDist::Directory(_) => Ok(None),
        }
    }

    fn from_registry_dist(
        reg_dist: &RegistrySourceDist,
        index: Option<&IndexUrl>,
    ) -> Result<Option<Self>, LockError> {
        // Reject distributions from registries that don't match the index URL, as can occur with
        // `--find-links`.
        if index.is_none_or(|index| *index != reg_dist.index) {
            return Ok(None);
        }

        match &reg_dist.index {
            IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                let url = normalize_file_location(&reg_dist.file.url)
                    .map_err(LockErrorKind::InvalidUrl)
                    .map_err(LockError::from)?;
                let hash = reg_dist.file.hashes.iter().max().cloned().map(Hash::from);
                let size = reg_dist.file.size;
                let upload_time = reg_dist
                    .file
                    .upload_time_utc_ms
                    .map(Timestamp::from_millisecond)
                    .transpose()
                    .map_err(LockErrorKind::InvalidTimestamp)?;
                Ok(Some(Self::Url {
                    url,
                    metadata: SourceDistMetadata {
                        hash,
                        size,
                        upload_time,
                    },
                }))
            }
            IndexUrl::Path(path) => {
                let index_path = path
                    .to_file_path()
                    .map_err(|()| LockErrorKind::UrlToPath { url: path.to_url() })?;
                let url = reg_dist
                    .file
                    .url
                    .to_url()
                    .map_err(LockErrorKind::InvalidUrl)?;

                if url.scheme() == "file" {
                    let reg_dist_path = url
                        .to_file_path()
                        .map_err(|()| LockErrorKind::UrlToPath { url })?;
                    let path = relative_to(&reg_dist_path, index_path)
                        .or_else(|_| std::path::absolute(&reg_dist_path))
                        .map_err(LockErrorKind::DistributionRelativePath)?
                        .into_boxed_path();
                    let hash = reg_dist.file.hashes.iter().max().cloned().map(Hash::from);
                    let size = reg_dist.file.size;
                    let upload_time = reg_dist
                        .file
                        .upload_time_utc_ms
                        .map(Timestamp::from_millisecond)
                        .transpose()
                        .map_err(LockErrorKind::InvalidTimestamp)?;
                    Ok(Some(Self::Path {
                        path,
                        metadata: SourceDistMetadata {
                            hash,
                            size,
                            upload_time,
                        },
                    }))
                } else {
                    let url = normalize_file_location(&reg_dist.file.url)
                        .map_err(LockErrorKind::InvalidUrl)
                        .map_err(LockError::from)?;
                    let hash = reg_dist.file.hashes.iter().max().cloned().map(Hash::from);
                    let size = reg_dist.file.size;
                    let upload_time = reg_dist
                        .file
                        .upload_time_utc_ms
                        .map(Timestamp::from_millisecond)
                        .transpose()
                        .map_err(LockErrorKind::InvalidTimestamp)?;
                    Ok(Some(Self::Url {
                        url,
                        metadata: SourceDistMetadata {
                            hash,
                            size,
                            upload_time,
                        },
                    }))
                }
            }
        }
    }

    fn from_direct_dist(id: &PackageId, hashes: &[HashDigest]) -> Result<Self, LockError> {
        let Some(hash) = hashes.iter().max().cloned().map(Hash::from) else {
            let kind = LockErrorKind::Hash {
                id: id.clone(),
                artifact_type: "direct URL source distribution",
                expected: true,
            };
            return Err(kind.into());
        };
        Ok(Self::Metadata {
            metadata: SourceDistMetadata {
                hash: Some(hash),
                size: None,
                upload_time: None,
            },
        })
    }

    fn from_path_dist(id: &PackageId, hashes: &[HashDigest]) -> Result<Self, LockError> {
        let Some(hash) = hashes.iter().max().cloned().map(Hash::from) else {
            let kind = LockErrorKind::Hash {
                id: id.clone(),
                artifact_type: "path source distribution",
                expected: true,
            };
            return Err(kind.into());
        };
        Ok(Self::Metadata {
            metadata: SourceDistMetadata {
                hash: Some(hash),
                size: None,
                upload_time: None,
            },
        })
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(untagged, rename_all = "kebab-case")]
enum SourceDistWire {
    Url {
        url: UrlString,
        #[serde(flatten)]
        metadata: SourceDistMetadata,
    },
    Path {
        path: PortablePathBuf,
        #[serde(flatten)]
        metadata: SourceDistMetadata,
    },
    Metadata {
        #[serde(flatten)]
        metadata: SourceDistMetadata,
    },
}

impl SourceDist {
    /// Returns the TOML representation of this source distribution.
    fn to_toml(&self) -> Result<InlineTable, toml_edit::ser::Error> {
        let mut table = InlineTable::new();
        match self {
            Self::Metadata { .. } => {}
            Self::Url { url, .. } => {
                table.insert("url", Value::from(url.as_ref()));
            }
            Self::Path { path, .. } => {
                table.insert("path", Value::from(PortablePath::from(path).to_string()));
            }
        }
        if let Some(hash) = self.hash() {
            table.insert("hash", Value::from(hash.to_string()));
        }
        if let Some(size) = self.size() {
            table.insert(
                "size",
                toml_edit::ser::ValueSerializer::new().serialize_u64(size)?,
            );
        }
        if let Some(upload_time) = self.upload_time() {
            table.insert("upload-time", Value::from(upload_time.to_string()));
        }
        Ok(table)
    }
}

impl From<SourceDistWire> for SourceDist {
    fn from(wire: SourceDistWire) -> Self {
        match wire {
            SourceDistWire::Url { url, metadata } => Self::Url { url, metadata },
            SourceDistWire::Path { path, metadata } => Self::Path {
                path: path.into(),
                metadata,
            },
            SourceDistWire::Metadata { metadata } => Self::Metadata { metadata },
        }
    }
}

impl From<GitReference> for GitSourceKind {
    fn from(value: GitReference) -> Self {
        match value {
            GitReference::Branch(branch) => Self::Branch(branch.to_string()),
            GitReference::Tag(tag) => Self::Tag(tag.to_string()),
            GitReference::BranchOrTag(rev) => Self::Rev(rev.to_string()),
            GitReference::BranchOrTagOrCommit(rev) => Self::Rev(rev.to_string()),
            GitReference::NamedRef(rev) => Self::Rev(rev.to_string()),
            GitReference::DefaultBranch => Self::DefaultBranch,
        }
    }
}

impl From<GitSourceKind> for GitReference {
    fn from(value: GitSourceKind) -> Self {
        match value {
            GitSourceKind::Branch(branch) => Self::Branch(branch),
            GitSourceKind::Tag(tag) => Self::Tag(tag),
            GitSourceKind::Rev(rev) => Self::from_rev(rev),
            GitSourceKind::DefaultBranch => Self::DefaultBranch,
        }
    }
}

/// Construct the lockfile-compatible [`DisplaySafeUrl`] for a [`GitSourceDist`].
fn locked_git_url(git_dist: &GitSourceDist) -> DisplaySafeUrl {
    let mut url = git_dist.git.repository().clone();

    // Remove the credentials.
    url.remove_credentials();

    // Clear out any existing state.
    url.set_fragment(None);
    url.set_query(None);

    // Put the subdirectory in the query.
    if let Some(subdirectory) = git_dist
        .subdirectory
        .as_deref()
        .map(PortablePath::from)
        .as_ref()
        .map(PortablePath::to_string)
    {
        url.query_pairs_mut()
            .append_pair("subdirectory", &subdirectory);
    }

    // Put the requested reference in the query.
    match git_dist.git.reference() {
        GitReference::Branch(branch) => {
            url.query_pairs_mut().append_pair("branch", branch.as_str());
        }
        GitReference::Tag(tag) => {
            url.query_pairs_mut().append_pair("tag", tag.as_str());
        }
        GitReference::BranchOrTag(rev)
        | GitReference::BranchOrTagOrCommit(rev)
        | GitReference::NamedRef(rev) => {
            url.query_pairs_mut().append_pair("rev", rev.as_str());
        }
        GitReference::DefaultBranch => {}
    }

    // Put the precise commit in the fragment.
    url.set_fragment(
        git_dist
            .git
            .precise()
            .as_ref()
            .map(GitOid::to_string)
            .as_deref(),
    );

    url
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
struct ZstdWheel {
    hash: Option<Hash>,
    size: Option<u64>,
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(from = "VariantsJsonWire")]
struct VariantsJsonEntry {
    /// A URL or file path (via `file://`) where the variants JSON file that was locked
    /// against was found. The location does not need to exist in the future,
    /// so this should be treated as only a hint to where to look and/or
    /// recording where the variants JSON file originally came from.
    #[serde(flatten)]
    url: VariantsJsonSource,
    /// A hash of the variants JSON file.
    ///
    /// This is only present for variants JSON files that come from registries and direct
    /// URLs. Files from git or path dependencies do not have hashes
    /// associated with them.
    hash: Option<Hash>,
    /// The size of the variants JSON file in bytes.
    ///
    /// This is only present for variants JSON files that come from registries.
    size: Option<u64>,
    /// The upload time of the variants JSON file.
    ///
    /// This is only present for variants JSON files that come from registries.
    upload_time: Option<Timestamp>,
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct VariantsJsonWire {
    /// A URL or file path (via `file://`) where the variants JSON file that was locked
    /// against was found.
    #[serde(flatten)]
    url: VariantsJsonSource,
    /// A hash of the variants JSON file.
    hash: Option<Hash>,
    /// The size of the variants JSON file in bytes.
    size: Option<u64>,
    /// The upload time of the variants JSON file.
    #[serde(alias = "upload_time")]
    upload_time: Option<Timestamp>,
}

impl VariantsJsonEntry {
    fn from_annotated_dist(annotated_dist: &AnnotatedDist) -> Result<Option<Self>, LockError> {
        match &annotated_dist.dist {
            // We pass empty installed packages for locking.
            ResolvedDist::Installed { .. } => unreachable!(),
            ResolvedDist::Installable { variants_json, .. } => {
                if let Some(variants_json) = variants_json {
                    let url = match &variants_json.index {
                        IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                            let url = normalize_file_location(&variants_json.file.url)
                                .map_err(LockErrorKind::InvalidUrl)
                                .map_err(LockError::from)?;
                            VariantsJsonSource::Url { url }
                        }
                        IndexUrl::Path(path) => {
                            let index_path = path
                                .to_file_path()
                                .map_err(|()| LockErrorKind::UrlToPath { url: path.to_url() })?;
                            let variants_url = variants_json
                                .file
                                .url
                                .to_url()
                                .map_err(LockErrorKind::InvalidUrl)?;

                            if variants_url.scheme() == "file" {
                                let variants_path = variants_url
                                    .to_file_path()
                                    .map_err(|()| LockErrorKind::UrlToPath { url: variants_url })?;
                                let path = relative_to(&variants_path, index_path)
                                    .or_else(|_| std::path::absolute(&variants_path))
                                    .map_err(LockErrorKind::DistributionRelativePath)?
                                    .into_boxed_path();
                                VariantsJsonSource::Path { path }
                            } else {
                                let url = normalize_file_location(&variants_json.file.url)
                                    .map_err(LockErrorKind::InvalidUrl)
                                    .map_err(LockError::from)?;
                                VariantsJsonSource::Url { url }
                            }
                        }
                    };

                    Ok(Some(Self {
                        url,
                        hash: variants_json
                            .file
                            .hashes
                            .iter()
                            .max()
                            .cloned()
                            .map(Hash::from),
                        size: variants_json.file.size,
                        upload_time: variants_json
                            .file
                            .upload_time_utc_ms
                            .map(Timestamp::from_millisecond)
                            .transpose()
                            .map_err(LockErrorKind::InvalidTimestamp)?,
                    }))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Returns the TOML representation of this variants JSON file.
    fn to_toml(&self) -> Result<InlineTable, toml_edit::ser::Error> {
        let mut table = InlineTable::new();
        match &self.url {
            VariantsJsonSource::Url { url } => {
                table.insert("url", Value::from(url.as_ref()));
            }
            VariantsJsonSource::Path { path } => {
                table.insert("path", Value::from(PortablePath::from(path).to_string()));
            }
        }
        if let Some(hash) = &self.hash {
            table.insert("hash", Value::from(hash.to_string()));
        }
        if let Some(size) = self.size {
            table.insert(
                "size",
                toml_edit::ser::ValueSerializer::new().serialize_u64(size)?,
            );
        }
        if let Some(upload_time) = self.upload_time {
            table.insert("upload-time", Value::from(upload_time.to_string()));
        }
        Ok(table)
    }
}

impl From<VariantsJsonWire> for VariantsJsonEntry {
    fn from(wire: VariantsJsonWire) -> Self {
        // TODO(konsti): Do we still need the wire type?
        Self {
            url: wire.url,
            hash: wire.hash,
            size: wire.size,
            upload_time: wire.upload_time,
        }
    }
}

/// Inspired by: <https://discuss.python.org/t/lock-files-again-but-this-time-w-sdists/46593>
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(try_from = "WheelWire")]
struct Wheel {
    /// A URL or file path (via `file://`) where the wheel that was locked
    /// against was found. The location does not need to exist in the future,
    /// so this should be treated as only a hint to where to look and/or
    /// recording where the wheel file originally came from.
    url: WheelWireSource,
    /// A hash of the built distribution.
    ///
    /// This is only present for wheels that come from registries and direct
    /// URLs. Wheels from git or path dependencies do not have hashes
    /// associated with them.
    hash: Option<Hash>,
    /// The size of the built distribution in bytes.
    ///
    /// This is only present for wheels that come from registries.
    size: Option<u64>,
    /// The upload time of the built distribution.
    ///
    /// This is only present for wheels that come from registries.
    upload_time: Option<Timestamp>,
    /// The filename of the wheel.
    ///
    /// This isn't part of the wire format since it's redundant with the
    /// URL. But we do use it for various things, and thus compute it at
    /// deserialization time. Not being able to extract a wheel filename from a
    /// wheel URL is thus a deserialization error.
    filename: WheelFilename,
    /// The zstandard-compressed wheel metadata, if any.
    zstd: Option<ZstdWheel>,
}

impl Wheel {
    fn from_annotated_dist(annotated_dist: &AnnotatedDist) -> Result<Vec<Self>, LockError> {
        match annotated_dist.dist {
            // We pass empty installed packages for locking.
            ResolvedDist::Installed { .. } => unreachable!(),
            ResolvedDist::Installable { ref dist, .. } => Self::from_dist(
                dist,
                annotated_dist.hashes.as_slice(),
                annotated_dist.index(),
            ),
        }
    }

    fn from_dist(
        dist: &Dist,
        hashes: &[HashDigest],
        index: Option<&IndexUrl>,
    ) -> Result<Vec<Self>, LockError> {
        match *dist {
            Dist::Built(ref built_dist) => Self::from_built_dist(built_dist, hashes, index),
            Dist::Source(uv_distribution_types::SourceDist::Registry(ref source_dist)) => {
                source_dist
                    .wheels
                    .iter()
                    .filter(|wheel| {
                        // Reject distributions from registries that don't match the index URL, as can occur with
                        // `--find-links`.
                        index.is_some_and(|index| *index == wheel.index)
                    })
                    .map(Self::from_registry_wheel)
                    .collect()
            }
            Dist::Source(_) => Ok(vec![]),
        }
    }

    fn from_built_dist(
        built_dist: &BuiltDist,
        hashes: &[HashDigest],
        index: Option<&IndexUrl>,
    ) -> Result<Vec<Self>, LockError> {
        match *built_dist {
            BuiltDist::Registry(ref reg_dist) => Self::from_registry_dist(reg_dist, index),
            BuiltDist::DirectUrl(ref direct_dist) => {
                Ok(vec![Self::from_direct_dist(direct_dist, hashes)])
            }
            BuiltDist::Path(ref path_dist) => Ok(vec![Self::from_path_dist(path_dist, hashes)]),
        }
    }

    fn from_registry_dist(
        reg_dist: &RegistryBuiltDist,
        index: Option<&IndexUrl>,
    ) -> Result<Vec<Self>, LockError> {
        reg_dist
            .wheels
            .iter()
            .filter(|wheel| {
                // Reject distributions from registries that don't match the index URL, as can occur with
                // `--find-links`.
                index.is_some_and(|index| *index == wheel.index)
            })
            .map(Self::from_registry_wheel)
            .collect()
    }

    fn from_registry_wheel(wheel: &RegistryBuiltWheel) -> Result<Self, LockError> {
        let url = match &wheel.index {
            IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                let url = normalize_file_location(&wheel.file.url)
                    .map_err(LockErrorKind::InvalidUrl)
                    .map_err(LockError::from)?;
                WheelWireSource::Url { url }
            }
            IndexUrl::Path(path) => {
                let index_path = path
                    .to_file_path()
                    .map_err(|()| LockErrorKind::UrlToPath { url: path.to_url() })?;
                let wheel_url = wheel.file.url.to_url().map_err(LockErrorKind::InvalidUrl)?;

                if wheel_url.scheme() == "file" {
                    let wheel_path = wheel_url
                        .to_file_path()
                        .map_err(|()| LockErrorKind::UrlToPath { url: wheel_url })?;
                    let path = relative_to(&wheel_path, index_path)
                        .or_else(|_| std::path::absolute(&wheel_path))
                        .map_err(LockErrorKind::DistributionRelativePath)?
                        .into_boxed_path();
                    WheelWireSource::Path { path }
                } else {
                    let url = normalize_file_location(&wheel.file.url)
                        .map_err(LockErrorKind::InvalidUrl)
                        .map_err(LockError::from)?;
                    WheelWireSource::Url { url }
                }
            }
        };
        let filename = wheel.filename.clone();
        let hash = wheel.file.hashes.iter().max().cloned().map(Hash::from);
        let size = wheel.file.size;
        let upload_time = wheel
            .file
            .upload_time_utc_ms
            .map(Timestamp::from_millisecond)
            .transpose()
            .map_err(LockErrorKind::InvalidTimestamp)?;
        let zstd = wheel.file.zstd.as_ref().map(|zstd| ZstdWheel {
            hash: zstd.hashes.iter().max().cloned().map(Hash::from),
            size: zstd.size,
        });
        Ok(Self {
            url,
            hash,
            size,
            upload_time,
            filename,
            zstd,
        })
    }

    fn from_direct_dist(direct_dist: &DirectUrlBuiltDist, hashes: &[HashDigest]) -> Self {
        Self {
            url: WheelWireSource::Url {
                url: normalize_url(direct_dist.url.to_url()),
            },
            hash: hashes.iter().max().cloned().map(Hash::from),
            size: None,
            upload_time: None,
            filename: direct_dist.filename.clone(),
            zstd: None,
        }
    }

    fn from_path_dist(path_dist: &PathBuiltDist, hashes: &[HashDigest]) -> Self {
        Self {
            url: WheelWireSource::Filename {
                filename: path_dist.filename.clone(),
            },
            hash: hashes.iter().max().cloned().map(Hash::from),
            size: None,
            upload_time: None,
            filename: path_dist.filename.clone(),
            zstd: None,
        }
    }

    pub(crate) fn to_registry_wheel(
        &self,
        source: &RegistrySource,
        root: &Path,
    ) -> Result<RegistryBuiltWheel, LockError> {
        let filename: WheelFilename = self.filename.clone();

        match source {
            RegistrySource::Url(url) => {
                let file_location = match &self.url {
                    WheelWireSource::Url { url: file_url } => {
                        FileLocation::AbsoluteUrl(file_url.clone())
                    }
                    WheelWireSource::Path { .. } | WheelWireSource::Filename { .. } => {
                        return Err(LockErrorKind::MissingUrl {
                            name: filename.name,
                            version: filename.version,
                        }
                        .into());
                    }
                };
                let file = Box::new(uv_distribution_types::File {
                    dist_info_metadata: false,
                    filename: SmallString::from(filename.to_string()),
                    hashes: self.hash.iter().map(|h| h.0.clone()).collect(),
                    requires_python: None,
                    size: self.size,
                    upload_time_utc_ms: self.upload_time.map(Timestamp::as_millisecond),
                    url: file_location,
                    yanked: None,
                    zstd: self
                        .zstd
                        .as_ref()
                        .map(|zstd| uv_distribution_types::Zstd {
                            hashes: zstd.hash.iter().map(|h| h.0.clone()).collect(),
                            size: zstd.size,
                        })
                        .map(Box::new),
                });
                let index = IndexUrl::from(VerbatimUrl::from_url(
                    url.to_url().map_err(LockErrorKind::InvalidUrl)?,
                ));
                Ok(RegistryBuiltWheel {
                    filename,
                    file,
                    index,
                })
            }
            RegistrySource::Path(index_path) => {
                let file_location = match &self.url {
                    WheelWireSource::Url { url: file_url } => {
                        FileLocation::AbsoluteUrl(file_url.clone())
                    }
                    WheelWireSource::Path { path: file_path } => {
                        let file_path = root.join(index_path).join(file_path);
                        let file_url =
                            DisplaySafeUrl::from_file_path(&file_path).map_err(|()| {
                                LockErrorKind::PathToUrl {
                                    path: file_path.into_boxed_path(),
                                }
                            })?;
                        FileLocation::AbsoluteUrl(UrlString::from(file_url))
                    }
                    WheelWireSource::Filename { .. } => {
                        return Err(LockErrorKind::MissingPath {
                            name: filename.name,
                            version: filename.version,
                        }
                        .into());
                    }
                };
                let file = Box::new(uv_distribution_types::File {
                    dist_info_metadata: false,
                    filename: SmallString::from(filename.to_string()),
                    hashes: self.hash.iter().map(|h| h.0.clone()).collect(),
                    requires_python: None,
                    size: self.size,
                    upload_time_utc_ms: self.upload_time.map(Timestamp::as_millisecond),
                    url: file_location,
                    yanked: None,
                    zstd: self
                        .zstd
                        .as_ref()
                        .map(|zstd| uv_distribution_types::Zstd {
                            hashes: zstd.hash.iter().map(|h| h.0.clone()).collect(),
                            size: zstd.size,
                        })
                        .map(Box::new),
                });
                let index = IndexUrl::from(
                    VerbatimUrl::from_absolute_path(root.join(index_path))
                        .map_err(LockErrorKind::RegistryVerbatimUrl)?,
                );
                Ok(RegistryBuiltWheel {
                    filename,
                    file,
                    index,
                })
            }
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct WheelWire {
    #[serde(flatten)]
    url: WheelWireSource,
    /// A hash of the built distribution.
    ///
    /// This is only present for wheels that come from registries and direct
    /// URLs. Wheels from git or path dependencies do not have hashes
    /// associated with them.
    hash: Option<Hash>,
    /// The size of the built distribution in bytes.
    ///
    /// This is only present for wheels that come from registries.
    size: Option<u64>,
    /// The upload time of the built distribution.
    ///
    /// This is only present for wheels that come from registries.
    #[serde(alias = "upload_time")]
    upload_time: Option<Timestamp>,
    /// The zstandard-compressed wheel metadata, if any.
    #[serde(alias = "zstd")]
    zstd: Option<ZstdWheel>,
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(untagged, rename_all = "kebab-case")]
enum WheelWireSource {
    /// Used for all wheels that come from remote sources.
    Url {
        /// A URL where the wheel that was locked against was found. The location
        /// does not need to exist in the future, so this should be treated as
        /// only a hint to where to look and/or recording where the wheel file
        /// originally came from.
        url: UrlString,
    },
    /// Used for wheels that come from local registries (like `--find-links`).
    Path {
        /// The path to the wheel, relative to the index.
        path: Box<Path>,
    },
    /// Used for path wheels.
    ///
    /// We only store the filename for path wheel, since we can't store a relative path in the url
    Filename {
        /// We duplicate the filename since a lot of code relies on having the filename on the
        /// wheel entry.
        filename: WheelFilename,
    },
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(untagged, rename_all = "kebab-case")]
enum VariantsJsonSource {
    /// Used for all variants JSON files that come from remote sources.
    Url {
        /// A URL where the variants JSON file that was locked against was found. The location
        /// does not need to exist in the future, so this should be treated as
        /// only a hint to where to look and/or recording where the variants JSON file
        /// originally came from.
        url: UrlString,
    },
    /// Used for variants JSON files that come from local registries (like `--find-links`).
    Path {
        /// The path to the variants JSON file, relative to the index.
        path: Box<Path>,
    },
}

impl VariantsJsonSource {
    fn url(&self) -> Option<&UrlString> {
        match &self {
            Self::Path { .. } => None,
            Self::Url { url, .. } => Some(url),
        }
    }
}

impl Wheel {
    /// Returns the TOML representation of this wheel.
    fn to_toml(&self) -> Result<InlineTable, toml_edit::ser::Error> {
        let mut table = InlineTable::new();
        match &self.url {
            WheelWireSource::Url { url } => {
                table.insert("url", Value::from(url.as_ref()));
            }
            WheelWireSource::Path { path } => {
                table.insert("path", Value::from(PortablePath::from(path).to_string()));
            }
            WheelWireSource::Filename { filename } => {
                table.insert("filename", Value::from(filename.to_string()));
            }
        }
        if let Some(ref hash) = self.hash {
            table.insert("hash", Value::from(hash.to_string()));
        }
        if let Some(size) = self.size {
            table.insert(
                "size",
                toml_edit::ser::ValueSerializer::new().serialize_u64(size)?,
            );
        }
        if let Some(upload_time) = self.upload_time {
            table.insert("upload-time", Value::from(upload_time.to_string()));
        }
        if let Some(zstd) = &self.zstd {
            let mut inner = InlineTable::new();
            if let Some(ref hash) = zstd.hash {
                inner.insert("hash", Value::from(hash.to_string()));
            }
            if let Some(size) = zstd.size {
                inner.insert(
                    "size",
                    toml_edit::ser::ValueSerializer::new().serialize_u64(size)?,
                );
            }
            table.insert("zstd", Value::from(inner));
        }
        Ok(table)
    }
}

impl TryFrom<WheelWire> for Wheel {
    type Error = String;

    fn try_from(wire: WheelWire) -> Result<Self, String> {
        let filename = match &wire.url {
            WheelWireSource::Url { url } => {
                let filename = url.filename().map_err(|err| err.to_string())?;
                filename.parse::<WheelFilename>().map_err(|err| {
                    format!("failed to parse `{filename}` as wheel filename: {err}")
                })?
            }
            WheelWireSource::Path { path } => {
                let filename = path
                    .file_name()
                    .and_then(|file_name| file_name.to_str())
                    .ok_or_else(|| {
                        format!("path `{}` has no filename component", path.display())
                    })?;
                filename.parse::<WheelFilename>().map_err(|err| {
                    format!("failed to parse `{filename}` as wheel filename: {err}")
                })?
            }
            WheelWireSource::Filename { filename } => filename.clone(),
        };

        Ok(Self {
            url: wire.url,
            hash: wire.hash,
            size: wire.size,
            upload_time: wire.upload_time,
            zstd: wire.zstd,
            filename,
        })
    }
}

/// A single dependency of a package in a lockfile.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct Dependency {
    package_id: PackageId,
    extra: BTreeSet<ExtraName>,
    /// A marker simplified from the PEP 508 marker in `complexified_marker`
    /// by assuming `requires-python` is satisfied. So if
    /// `requires-python = '>=3.8'`, then
    /// `python_version >= '3.8' and python_version < '3.12'`
    /// gets simplified to `python_version < '3.12'`.
    ///
    /// Generally speaking, this marker should not be exposed to
    /// anything outside this module unless it's for a specialized use
    /// case. But specifically, it should never be used to evaluate
    /// against a marker environment or for disjointness checks or any
    /// other kind of marker algebra.
    ///
    /// It exists because there are some cases where we do actually
    /// want to compare markers in their "simplified" form. For
    /// example, when collapsing the extras on duplicate dependencies.
    /// Even if a dependency has different complexified markers,
    /// they might have identical markers once simplified. And since
    /// `requires-python` applies to the entire lock file, it's
    /// acceptable to do comparisons on the simplified form.
    simplified_marker: SimplifiedMarkerTree,
    /// The "complexified" marker is a universal marker whose PEP 508
    /// marker can stand on its own independent of `requires-python`.
    /// It can be safely used for any kind of marker algebra.
    complexified_marker: UniversalMarker,
}

impl Dependency {
    fn new(
        requires_python: &RequiresPython,
        package_id: PackageId,
        extra: BTreeSet<ExtraName>,
        complexified_marker: UniversalMarker,
    ) -> Self {
        let simplified_marker =
            SimplifiedMarkerTree::new(requires_python, complexified_marker.combined());
        Self {
            package_id,
            extra,
            simplified_marker,
            complexified_marker,
        }
    }

    fn from_annotated_dist(
        requires_python: &RequiresPython,
        annotated_dist: &AnnotatedDist,
        complexified_marker: UniversalMarker,
        root: &Path,
    ) -> Result<Self, LockError> {
        let package_id = PackageId::from_annotated_dist(annotated_dist, root)?;
        let extra = annotated_dist.extra.iter().cloned().collect();
        Ok(Self::new(
            requires_python,
            package_id,
            extra,
            complexified_marker,
        ))
    }

    /// Returns the TOML representation of this dependency.
    fn to_toml(
        &self,
        _requires_python: &RequiresPython,
        dist_count_by_name: &FxHashMap<PackageName, u64>,
    ) -> Table {
        let mut table = Table::new();
        self.package_id
            .to_toml(Some(dist_count_by_name), &mut table);
        if !self.extra.is_empty() {
            let extra_array = self
                .extra
                .iter()
                .map(ToString::to_string)
                .collect::<Array>();
            table.insert("extra", value(extra_array));
        }
        if let Some(marker) = self.simplified_marker.try_to_string() {
            table.insert("marker", value(marker));
        }

        table
    }

    /// Returns the package name of this dependency.
    pub fn package_name(&self) -> &PackageName {
        &self.package_id.name
    }

    /// Returns the extras specified on this dependency.
    pub fn extra(&self) -> &BTreeSet<ExtraName> {
        &self.extra
    }
}

impl Display for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match (self.extra.is_empty(), self.package_id.version.as_ref()) {
            (true, Some(version)) => write!(f, "{}=={}", self.package_id.name, version),
            (true, None) => write!(f, "{}", self.package_id.name),
            (false, Some(version)) => write!(
                f,
                "{}[{}]=={}",
                self.package_id.name,
                self.extra.iter().join(","),
                version
            ),
            (false, None) => write!(
                f,
                "{}[{}]",
                self.package_id.name,
                self.extra.iter().join(",")
            ),
        }
    }
}

/// A single dependency of a package in a lockfile.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct DependencyWire {
    #[serde(flatten)]
    package_id: PackageIdForDependency,
    #[serde(default)]
    extra: BTreeSet<ExtraName>,
    #[serde(default)]
    marker: SimplifiedMarkerTree,
}

impl DependencyWire {
    fn unwire(
        self,
        requires_python: &RequiresPython,
        unambiguous_package_ids: &FxHashMap<PackageName, PackageId>,
    ) -> Result<Dependency, LockError> {
        let complexified_marker = self.marker.into_marker(requires_python);
        Ok(Dependency {
            package_id: self.package_id.unwire(unambiguous_package_ids)?,
            extra: self.extra,
            simplified_marker: self.marker,
            complexified_marker: UniversalMarker::from_combined(complexified_marker),
        })
    }
}

/// A single hash for a distribution artifact in a lockfile.
///
/// A hash is encoded as a single TOML string in the format
/// `{algorithm}:{digest}`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Hash(HashDigest);

impl From<HashDigest> for Hash {
    fn from(hd: HashDigest) -> Self {
        Self(hd)
    }
}

impl FromStr for Hash {
    type Err = HashParseError;

    fn from_str(s: &str) -> Result<Self, HashParseError> {
        let (algorithm, digest) = s.split_once(':').ok_or(HashParseError(
            "expected '{algorithm}:{digest}', but found no ':' in hash digest",
        ))?;
        let algorithm = algorithm
            .parse()
            .map_err(|_| HashParseError("unrecognized hash algorithm"))?;
        Ok(Self(HashDigest {
            algorithm,
            digest: digest.into(),
        }))
    }
}

impl Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}:{}", self.0.algorithm, self.0.digest)
    }
}

impl<'de> serde::Deserialize<'de> for Hash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = Hash;

            fn expecting(&self, f: &mut Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Hash::from_str(v).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl From<Hash> for Hashes {
    fn from(value: Hash) -> Self {
        match value.0.algorithm {
            HashAlgorithm::Md5 => Self {
                md5: Some(value.0.digest),
                sha256: None,
                sha384: None,
                sha512: None,
                blake2b: None,
            },
            HashAlgorithm::Sha256 => Self {
                md5: None,
                sha256: Some(value.0.digest),
                sha384: None,
                sha512: None,
                blake2b: None,
            },
            HashAlgorithm::Sha384 => Self {
                md5: None,
                sha256: None,
                sha384: Some(value.0.digest),
                sha512: None,
                blake2b: None,
            },
            HashAlgorithm::Sha512 => Self {
                md5: None,
                sha256: None,
                sha384: None,
                sha512: Some(value.0.digest),
                blake2b: None,
            },
            HashAlgorithm::Blake2b => Self {
                md5: None,
                sha256: None,
                sha384: None,
                sha512: None,
                blake2b: Some(value.0.digest),
            },
        }
    }
}

/// Convert a [`FileLocation`] into a normalized [`UrlString`].
fn normalize_file_location(location: &FileLocation) -> Result<UrlString, ToUrlError> {
    match location {
        FileLocation::AbsoluteUrl(absolute) => Ok(absolute.without_fragment().into_owned()),
        FileLocation::RelativeUrl(_, _) => Ok(normalize_url(location.to_url()?)),
    }
}

/// Convert a [`DisplaySafeUrl`] into a normalized [`UrlString`] by removing the fragment.
fn normalize_url(mut url: DisplaySafeUrl) -> UrlString {
    url.set_fragment(None);
    UrlString::from(url)
}

/// Normalize a [`Requirement`], which could come from a lockfile, a `pyproject.toml`, etc.
///
/// Performs the following steps:
///
/// 1. Removes any sensitive credentials.
/// 2. Ensures that the lock and install paths are appropriately framed with respect to the
///    current [`Workspace`].
/// 3. Removes the `origin` field, which is only used in `requirements.txt`.
/// 4. Simplifies the markers using the provided [`RequiresPython`] instance.
fn normalize_requirement(
    mut requirement: Requirement,
    root: &Path,
    requires_python: &RequiresPython,
) -> Result<Requirement, LockError> {
    // Sort the extras and groups for consistency.
    requirement.extras.sort();
    requirement.groups.sort();

    // Normalize the requirement source.
    match requirement.source {
        RequirementSource::Git {
            git,
            subdirectory,
            url: _,
        } => {
            // Reconstruct the Git URL.
            let git = {
                let mut repository = git.repository().clone();

                // Remove the credentials.
                repository.remove_credentials();

                // Remove the fragment and query from the URL; they're already present in the source.
                repository.set_fragment(None);
                repository.set_query(None);

                GitUrl::from_fields(repository, git.reference().clone(), git.precise())?
            };

            // Reconstruct the PEP 508 URL from the underlying data.
            let url = DisplaySafeUrl::from(ParsedGitUrl {
                url: git.clone(),
                subdirectory: subdirectory.clone(),
            });

            Ok(Requirement {
                name: requirement.name,
                extras: requirement.extras,
                groups: requirement.groups,
                marker: requires_python.simplify_markers(requirement.marker),
                source: RequirementSource::Git {
                    git,
                    subdirectory,
                    url: VerbatimUrl::from_url(url),
                },
                origin: None,
            })
        }
        RequirementSource::Path {
            install_path,
            ext,
            url: _,
        } => {
            let install_path =
                uv_fs::normalize_path_buf(root.join(&install_path)).into_boxed_path();
            let url = VerbatimUrl::from_normalized_path(&install_path)
                .map_err(LockErrorKind::RequirementVerbatimUrl)?;

            Ok(Requirement {
                name: requirement.name,
                extras: requirement.extras,
                groups: requirement.groups,
                marker: requires_python.simplify_markers(requirement.marker),
                source: RequirementSource::Path {
                    install_path,
                    ext,
                    url,
                },
                origin: None,
            })
        }
        RequirementSource::Directory {
            install_path,
            editable,
            r#virtual,
            url: _,
        } => {
            let install_path =
                uv_fs::normalize_path_buf(root.join(&install_path)).into_boxed_path();
            let url = VerbatimUrl::from_normalized_path(&install_path)
                .map_err(LockErrorKind::RequirementVerbatimUrl)?;

            Ok(Requirement {
                name: requirement.name,
                extras: requirement.extras,
                groups: requirement.groups,
                marker: requires_python.simplify_markers(requirement.marker),
                source: RequirementSource::Directory {
                    install_path,
                    editable: Some(editable.unwrap_or(false)),
                    r#virtual: Some(r#virtual.unwrap_or(false)),
                    url,
                },
                origin: None,
            })
        }
        RequirementSource::Registry {
            specifier,
            index,
            conflict,
        } => {
            // Round-trip the index to remove anything apart from the URL.
            let index = index
                .map(|index| index.url.into_url())
                .map(|mut index| {
                    index.remove_credentials();
                    index
                })
                .map(|index| IndexMetadata::from(IndexUrl::from(VerbatimUrl::from_url(index))));
            Ok(Requirement {
                name: requirement.name,
                extras: requirement.extras,
                groups: requirement.groups,
                marker: requires_python.simplify_markers(requirement.marker),
                source: RequirementSource::Registry {
                    specifier,
                    index,
                    conflict,
                },
                origin: None,
            })
        }
        RequirementSource::Url {
            mut location,
            subdirectory,
            ext,
            url: _,
        } => {
            // Remove the credentials.
            location.remove_credentials();

            // Remove the fragment from the URL; it's already present in the source.
            location.set_fragment(None);

            // Reconstruct the PEP 508 URL from the underlying data.
            let url = DisplaySafeUrl::from(ParsedArchiveUrl {
                url: location.clone(),
                subdirectory: subdirectory.clone(),
                ext,
            });

            Ok(Requirement {
                name: requirement.name,
                extras: requirement.extras,
                groups: requirement.groups,
                marker: requires_python.simplify_markers(requirement.marker),
                source: RequirementSource::Url {
                    location,
                    subdirectory,
                    ext,
                    url: VerbatimUrl::from_url(url),
                },
                origin: None,
            })
        }
    }
}

#[derive(Debug)]
pub struct LockError {
    kind: Box<LockErrorKind>,
    hint: Option<WheelTagHint>,
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.kind.source()
    }
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(hint) = &self.hint {
            write!(f, "\n\n{hint}")?;
        }
        Ok(())
    }
}

impl LockError {
    /// Returns true if the [`LockError`] is a resolver error.
    pub fn is_resolution(&self) -> bool {
        matches!(&*self.kind, LockErrorKind::Resolution { .. })
    }
}

impl<E> From<E> for LockError
where
    LockErrorKind: From<E>,
{
    fn from(err: E) -> Self {
        Self {
            kind: Box::new(LockErrorKind::from(err)),
            hint: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum WheelTagHint {
    /// None of the available wheels for a package have a compatible Python language tag (e.g.,
    /// `cp310` in `cp310-abi3-manylinux_2_17_x86_64.whl`).
    LanguageTags {
        package: PackageName,
        version: Option<Version>,
        tags: BTreeSet<LanguageTag>,
        best: Option<LanguageTag>,
    },
    /// None of the available wheels for a package have a compatible ABI tag (e.g., `abi3` in
    /// `cp310-abi3-manylinux_2_17_x86_64.whl`).
    AbiTags {
        package: PackageName,
        version: Option<Version>,
        tags: BTreeSet<AbiTag>,
        best: Option<AbiTag>,
    },
    /// None of the available wheels for a package have a compatible platform tag (e.g.,
    /// `manylinux_2_17_x86_64` in `cp310-abi3-manylinux_2_17_x86_64.whl`).
    PlatformTags {
        package: PackageName,
        version: Option<Version>,
        tags: BTreeSet<PlatformTag>,
        best: Option<PlatformTag>,
    },
}

impl WheelTagHint {
    /// Generate a [`WheelTagHint`] from the given (incompatible) wheels.
    fn from_wheels(
        name: &PackageName,
        version: Option<&Version>,
        filenames: &[&WheelFilename],
        tags: &Tags,
    ) -> Option<Self> {
        let incompatibility = filenames
            .iter()
            .map(|filename| {
                tags.compatibility(
                    filename.python_tags(),
                    filename.abi_tags(),
                    filename.platform_tags(),
                )
            })
            .max()?;
        match incompatibility {
            TagCompatibility::Incompatible(IncompatibleTag::Python) => {
                let best = tags.python_tag();
                let tags = Self::python_tags(filenames.iter().copied()).collect::<BTreeSet<_>>();
                if tags.is_empty() {
                    None
                } else {
                    Some(Self::LanguageTags {
                        package: name.clone(),
                        version: version.cloned(),
                        tags,
                        best,
                    })
                }
            }
            TagCompatibility::Incompatible(IncompatibleTag::Abi) => {
                let best = tags.abi_tag();
                let tags = Self::abi_tags(filenames.iter().copied())
                    // Ignore `none`, which is universally compatible.
                    //
                    // As an example, `none` can appear here if we're solving for Python 3.13, and
                    // the distribution includes a wheel for `cp312-none-macosx_11_0_arm64`.
                    //
                    // In that case, the wheel isn't compatible, but when solving for Python 3.13,
                    // the `cp312` Python tag _can_ be compatible (e.g., for `cp312-abi3-macosx_11_0_arm64.whl`),
                    // so this is considered an ABI incompatibility rather than Python incompatibility.
                    .filter(|tag| *tag != AbiTag::None)
                    .collect::<BTreeSet<_>>();
                if tags.is_empty() {
                    None
                } else {
                    Some(Self::AbiTags {
                        package: name.clone(),
                        version: version.cloned(),
                        tags,
                        best,
                    })
                }
            }
            TagCompatibility::Incompatible(IncompatibleTag::Platform) => {
                let best = tags.platform_tag().cloned();
                let tags = Self::platform_tags(filenames.iter().copied(), tags)
                    .cloned()
                    .collect::<BTreeSet<_>>();
                if tags.is_empty() {
                    None
                } else {
                    Some(Self::PlatformTags {
                        package: name.clone(),
                        version: version.cloned(),
                        tags,
                        best,
                    })
                }
            }
            _ => None,
        }
    }

    /// Returns an iterator over the compatible Python tags of the available wheels.
    fn python_tags<'a>(
        filenames: impl Iterator<Item = &'a WheelFilename> + 'a,
    ) -> impl Iterator<Item = LanguageTag> + 'a {
        filenames.flat_map(WheelFilename::python_tags).copied()
    }

    /// Returns an iterator over the compatible Python tags of the available wheels.
    fn abi_tags<'a>(
        filenames: impl Iterator<Item = &'a WheelFilename> + 'a,
    ) -> impl Iterator<Item = AbiTag> + 'a {
        filenames.flat_map(WheelFilename::abi_tags).copied()
    }

    /// Returns the set of platform tags for the distribution that are ABI-compatible with the given
    /// tags.
    fn platform_tags<'a>(
        filenames: impl Iterator<Item = &'a WheelFilename> + 'a,
        tags: &'a Tags,
    ) -> impl Iterator<Item = &'a PlatformTag> + 'a {
        filenames.flat_map(move |filename| {
            if filename.python_tags().iter().any(|wheel_py| {
                filename
                    .abi_tags()
                    .iter()
                    .any(|wheel_abi| tags.is_compatible_abi(*wheel_py, *wheel_abi))
            }) {
                filename.platform_tags().iter()
            } else {
                [].iter()
            }
        })
    }
}

impl std::fmt::Display for WheelTagHint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LanguageTags {
                package,
                version,
                tags,
                best,
            } => {
                if let Some(best) = best {
                    let s = if tags.len() == 1 { "" } else { "s" };
                    let best = if let Some(pretty) = best.pretty() {
                        format!("{} (`{}`)", pretty.cyan(), best.cyan())
                    } else {
                        format!("{}", best.cyan())
                    };
                    if let Some(version) = version {
                        write!(
                            f,
                            "{}{} You're using {}, but `{}` ({}) only has wheels with the following Python implementation tag{s}: {}",
                            "hint".bold().cyan(),
                            ":".bold(),
                            best,
                            package.cyan(),
                            format!("v{version}").cyan(),
                            tags.iter()
                                .map(|tag| format!("`{}`", tag.cyan()))
                                .join(", "),
                        )
                    } else {
                        write!(
                            f,
                            "{}{} You're using {}, but `{}` only has wheels with the following Python implementation tag{s}: {}",
                            "hint".bold().cyan(),
                            ":".bold(),
                            best,
                            package.cyan(),
                            tags.iter()
                                .map(|tag| format!("`{}`", tag.cyan()))
                                .join(", "),
                        )
                    }
                } else {
                    let s = if tags.len() == 1 { "" } else { "s" };
                    if let Some(version) = version {
                        write!(
                            f,
                            "{}{} Wheels are available for `{}` ({}) with the following Python implementation tag{s}: {}",
                            "hint".bold().cyan(),
                            ":".bold(),
                            package.cyan(),
                            format!("v{version}").cyan(),
                            tags.iter()
                                .map(|tag| format!("`{}`", tag.cyan()))
                                .join(", "),
                        )
                    } else {
                        write!(
                            f,
                            "{}{} Wheels are available for `{}` with the following Python implementation tag{s}: {}",
                            "hint".bold().cyan(),
                            ":".bold(),
                            package.cyan(),
                            tags.iter()
                                .map(|tag| format!("`{}`", tag.cyan()))
                                .join(", "),
                        )
                    }
                }
            }
            Self::AbiTags {
                package,
                version,
                tags,
                best,
            } => {
                if let Some(best) = best {
                    let s = if tags.len() == 1 { "" } else { "s" };
                    let best = if let Some(pretty) = best.pretty() {
                        format!("{} (`{}`)", pretty.cyan(), best.cyan())
                    } else {
                        format!("{}", best.cyan())
                    };
                    if let Some(version) = version {
                        write!(
                            f,
                            "{}{} You're using {}, but `{}` ({}) only has wheels with the following Python ABI tag{s}: {}",
                            "hint".bold().cyan(),
                            ":".bold(),
                            best,
                            package.cyan(),
                            format!("v{version}").cyan(),
                            tags.iter()
                                .map(|tag| format!("`{}`", tag.cyan()))
                                .join(", "),
                        )
                    } else {
                        write!(
                            f,
                            "{}{} You're using {}, but `{}` only has wheels with the following Python ABI tag{s}: {}",
                            "hint".bold().cyan(),
                            ":".bold(),
                            best,
                            package.cyan(),
                            tags.iter()
                                .map(|tag| format!("`{}`", tag.cyan()))
                                .join(", "),
                        )
                    }
                } else {
                    let s = if tags.len() == 1 { "" } else { "s" };
                    if let Some(version) = version {
                        write!(
                            f,
                            "{}{} Wheels are available for `{}` ({}) with the following Python ABI tag{s}: {}",
                            "hint".bold().cyan(),
                            ":".bold(),
                            package.cyan(),
                            format!("v{version}").cyan(),
                            tags.iter()
                                .map(|tag| format!("`{}`", tag.cyan()))
                                .join(", "),
                        )
                    } else {
                        write!(
                            f,
                            "{}{} Wheels are available for `{}` with the following Python ABI tag{s}: {}",
                            "hint".bold().cyan(),
                            ":".bold(),
                            package.cyan(),
                            tags.iter()
                                .map(|tag| format!("`{}`", tag.cyan()))
                                .join(", "),
                        )
                    }
                }
            }
            Self::PlatformTags {
                package,
                version,
                tags,
                best,
            } => {
                let s = if tags.len() == 1 { "" } else { "s" };
                if let Some(best) = best {
                    let best = if let Some(pretty) = best.pretty() {
                        format!("{} (`{}`)", pretty.cyan(), best.cyan())
                    } else {
                        format!("`{}`", best.cyan())
                    };
                    let package_ref = if let Some(version) = version {
                        format!("`{}` ({})", package.cyan(), format!("v{version}").cyan())
                    } else {
                        format!("`{}`", package.cyan())
                    };
                    writeln!(
                        f,
                        "{}{} You're on {}, but {} only has wheels for the following platform{s}: {}; consider adding your platform to `{}` to ensure uv resolves to a version with compatible wheels",
                        "hint".bold().cyan(),
                        ":".bold(),
                        best,
                        package_ref,
                        tags.iter()
                            .map(|tag| format!("`{}`", tag.cyan()))
                            .join(", "),
                        "tool.uv.required-environments".green()
                    )
                } else {
                    if let Some(version) = version {
                        write!(
                            f,
                            "{}{} Wheels are available for `{}` ({}) on the following platform{s}: {}",
                            "hint".bold().cyan(),
                            ":".bold(),
                            package.cyan(),
                            format!("v{version}").cyan(),
                            tags.iter()
                                .map(|tag| format!("`{}`", tag.cyan()))
                                .join(", "),
                        )
                    } else {
                        write!(
                            f,
                            "{}{} Wheels are available for `{}` on the following platform{s}: {}",
                            "hint".bold().cyan(),
                            ":".bold(),
                            package.cyan(),
                            tags.iter()
                                .map(|tag| format!("`{}`", tag.cyan()))
                                .join(", "),
                        )
                    }
                }
            }
        }
    }
}

/// An error that occurs when generating a `Lock` data structure.
///
/// These errors are sometimes the result of possible programming bugs.
/// For example, if there are two or more duplicative distributions given
/// to `Lock::new`, then an error is returned. It's likely that the fault
/// is with the caller somewhere in such cases.
#[derive(Debug, thiserror::Error)]
enum LockErrorKind {
    /// An error that occurs when multiple packages with the same
    /// ID were found.
    #[error("Found duplicate package `{id}`", id = id.cyan())]
    DuplicatePackage {
        /// The ID of the conflicting package.
        id: PackageId,
    },
    /// An error that occurs when there are multiple dependencies for the
    /// same package that have identical identifiers.
    #[error("For package `{id}`, found duplicate dependency `{dependency}`", id = id.cyan(), dependency = dependency.cyan())]
    DuplicateDependency {
        /// The ID of the package for which a duplicate dependency was
        /// found.
        id: PackageId,
        /// The ID of the conflicting dependency.
        dependency: Dependency,
    },
    /// An error that occurs when there are multiple dependencies for the
    /// same package that have identical identifiers, as part of the
    /// that package's optional dependencies.
    #[error("For package `{id}`, found duplicate dependency `{dependency}`", id = format!("{id}[{extra}]").cyan(), dependency = dependency.cyan())]
    DuplicateOptionalDependency {
        /// The ID of the package for which a duplicate dependency was
        /// found.
        id: PackageId,
        /// The name of the extra.
        extra: ExtraName,
        /// The ID of the conflicting dependency.
        dependency: Dependency,
    },
    /// An error that occurs when there are multiple dependencies for the
    /// same package that have identical identifiers, as part of the
    /// that package's development dependencies.
    #[error("For package `{id}`, found duplicate dependency `{dependency}`", id = format!("{id}:{group}").cyan(), dependency = dependency.cyan())]
    DuplicateDevDependency {
        /// The ID of the package for which a duplicate dependency was
        /// found.
        id: PackageId,
        /// The name of the dev dependency group.
        group: GroupName,
        /// The ID of the conflicting dependency.
        dependency: Dependency,
    },
    /// An error that occurs when the URL to a file for a wheel or
    /// source dist could not be converted to a structured `url::Url`.
    #[error(transparent)]
    InvalidUrl(
        /// The underlying error that occurred. This includes the
        /// errant URL in its error message.
        #[from]
        ToUrlError,
    ),
    /// An error that occurs when the extension can't be determined
    /// for a given wheel or source distribution.
    #[error("Failed to parse file extension for `{id}`; expected one of: {err}", id = id.cyan())]
    MissingExtension {
        /// The filename that was expected to have an extension.
        id: PackageId,
        /// The list of valid extensions that were expected.
        err: ExtensionError,
    },
    /// Failed to parse a Git source URL.
    #[error("Failed to parse Git URL")]
    InvalidGitSourceUrl(
        /// The underlying error that occurred. This includes the
        /// errant URL in the message.
        #[source]
        SourceParseError,
    ),
    #[error("Failed to parse timestamp")]
    InvalidTimestamp(
        /// The underlying error that occurred. This includes the
        /// errant timestamp in the message.
        #[source]
        jiff::Error,
    ),
    /// An error that occurs when there's an unrecognized dependency.
    ///
    /// That is, a dependency for a package that isn't in the lockfile.
    #[error("For package `{id}`, found dependency `{dependency}` with no locked package", id = id.cyan(), dependency = dependency.cyan())]
    UnrecognizedDependency {
        /// The ID of the package that has an unrecognized dependency.
        id: PackageId,
        /// The ID of the dependency that doesn't have a corresponding package
        /// entry.
        dependency: Dependency,
    },
    /// An error that occurs when a hash is expected (or not) for a particular
    /// artifact, but one was not found (or was).
    #[error("Since the package `{id}` comes from a {source} dependency, a hash was {expected} but one was not found for {artifact_type}", id = id.cyan(), source = id.source.name(), expected = if *expected { "expected" } else { "not expected" })]
    Hash {
        /// The ID of the package that has a missing hash.
        id: PackageId,
        /// The specific type of artifact, e.g., "source package"
        /// or "wheel".
        artifact_type: &'static str,
        /// When true, a hash is expected to be present.
        expected: bool,
    },
    /// An error that occurs when a package is included with an extra name,
    /// but no corresponding base package (i.e., without the extra) exists.
    #[error("Found package `{id}` with extra `{extra}` but no base package", id = id.cyan(), extra = extra.cyan())]
    MissingExtraBase {
        /// The ID of the package that has a missing base.
        id: PackageId,
        /// The extra name that was found.
        extra: ExtraName,
    },
    /// An error that occurs when a package is included with a development
    /// dependency group, but no corresponding base package (i.e., without
    /// the group) exists.
    #[error("Found package `{id}` with development dependency group `{group}` but no base package", id = id.cyan())]
    MissingDevBase {
        /// The ID of the package that has a missing base.
        id: PackageId,
        /// The development dependency group that was found.
        group: GroupName,
    },
    /// An error that occurs from an invalid lockfile where a wheel comes from a non-wheel source
    /// such as a directory.
    #[error("Wheels cannot come from {source_type} sources")]
    InvalidWheelSource {
        /// The ID of the distribution that has a missing base.
        id: PackageId,
        /// The kind of the invalid source.
        source_type: &'static str,
    },
    /// An error that occurs when a distribution indicates that it is sourced from a remote
    /// registry, but is missing a URL.
    #[error("Found registry distribution `{name}` ({version}) without a valid URL", name = name.cyan(), version = format!("v{version}").cyan())]
    MissingUrl {
        /// The name of the distribution that is missing a URL.
        name: PackageName,
        /// The version of the distribution that is missing a URL.
        version: Version,
    },
    /// An error that occurs when a distribution indicates that it is sourced from a local registry,
    /// but is missing a path.
    #[error("Found registry distribution `{name}` ({version}) without a valid path", name = name.cyan(), version = format!("v{version}").cyan())]
    MissingPath {
        /// The name of the distribution that is missing a path.
        name: PackageName,
        /// The version of the distribution that is missing a path.
        version: Version,
    },
    /// An error that occurs when a distribution indicates that it is sourced from a registry, but
    /// is missing a filename.
    #[error("Found registry distribution `{id}` without a valid filename", id = id.cyan())]
    MissingFilename {
        /// The ID of the distribution that is missing a filename.
        id: PackageId,
    },
    /// An error that occurs when a distribution is included with neither wheels nor a source
    /// distribution.
    #[error("Distribution `{id}` can't be installed because it doesn't have a source distribution or wheel for the current platform", id = id.cyan())]
    NeitherSourceDistNorWheel {
        /// The ID of the distribution.
        id: PackageId,
    },
    /// An error that occurs when a distribution is marked as both `--no-binary` and `--no-build`.
    #[error("Distribution `{id}` can't be installed because it is marked as both `--no-binary` and `--no-build`", id = id.cyan())]
    NoBinaryNoBuild {
        /// The ID of the distribution.
        id: PackageId,
    },
    /// An error that occurs when a distribution is marked as `--no-binary`, but no source
    /// distribution is available.
    #[error("Distribution `{id}` can't be installed because it is marked as `--no-binary` but has no source distribution", id = id.cyan())]
    NoBinary {
        /// The ID of the distribution.
        id: PackageId,
    },
    /// An error that occurs when a distribution is marked as `--no-build`, but no binary
    /// distribution is available.
    #[error("Distribution `{id}` can't be installed because it is marked as `--no-build` but has no binary distribution", id = id.cyan())]
    NoBuild {
        /// The ID of the distribution.
        id: PackageId,
    },
    /// An error that occurs when a wheel-only distribution is incompatible with the current
    /// platform.
    #[error("Distribution `{id}` can't be installed because the binary distribution is incompatible with the current platform", id = id.cyan())]
    IncompatibleWheelOnly {
        /// The ID of the distribution.
        id: PackageId,
    },
    /// An error that occurs when a wheel-only source is marked as `--no-binary`.
    #[error("Distribution `{id}` can't be installed because it is marked as `--no-binary` but is itself a binary distribution", id = id.cyan())]
    NoBinaryWheelOnly {
        /// The ID of the distribution.
        id: PackageId,
    },
    /// An error that occurs when converting between URLs and paths.
    #[error("Found dependency `{id}` with no locked distribution", id = id.cyan())]
    VerbatimUrl {
        /// The ID of the distribution that has a missing base.
        id: PackageId,
        /// The inner error we forward.
        #[source]
        err: VerbatimUrlError,
    },
    /// An error that occurs when parsing an existing requirement.
    #[error("Could not compute relative path between workspace and distribution")]
    DistributionRelativePath(
        /// The inner error we forward.
        #[source]
        io::Error,
    ),
    /// An error that occurs when converting an index URL to a relative path
    #[error("Could not compute relative path between workspace and index")]
    IndexRelativePath(
        /// The inner error we forward.
        #[source]
        io::Error,
    ),
    /// An error that occurs when converting a lockfile path from relative to absolute.
    #[error("Could not compute absolute path from workspace root and lockfile path")]
    AbsolutePath(
        /// The inner error we forward.
        #[source]
        io::Error,
    ),
    /// An error that occurs when an ambiguous `package.dependency` is
    /// missing a `version` field.
    #[error("Dependency `{name}` has missing `version` field but has more than one matching package", name = name.cyan())]
    MissingDependencyVersion {
        /// The name of the dependency that is missing a `version` field.
        name: PackageName,
    },
    /// An error that occurs when an ambiguous `package.dependency` is
    /// missing a `source` field.
    #[error("Dependency `{name}` has missing `source` field but has more than one matching package", name = name.cyan())]
    MissingDependencySource {
        /// The name of the dependency that is missing a `source` field.
        name: PackageName,
    },
    /// An error that occurs when parsing an existing requirement.
    #[error("Could not compute relative path between workspace and requirement")]
    RequirementRelativePath(
        /// The inner error we forward.
        #[source]
        io::Error,
    ),
    /// An error that occurs when parsing an existing requirement.
    #[error("Could not convert between URL and path")]
    RequirementVerbatimUrl(
        /// The inner error we forward.
        #[source]
        VerbatimUrlError,
    ),
    /// An error that occurs when parsing a registry's index URL.
    #[error("Could not convert between URL and path")]
    RegistryVerbatimUrl(
        /// The inner error we forward.
        #[source]
        VerbatimUrlError,
    ),
    /// An error that occurs when converting a path to a URL.
    #[error("Failed to convert path to URL: {path}", path = path.display().cyan())]
    PathToUrl { path: Box<Path> },
    /// An error that occurs when converting a URL to a path
    #[error("Failed to convert URL to path: {url}", url = url.cyan())]
    UrlToPath { url: DisplaySafeUrl },
    /// An error that occurs when multiple packages with the same
    /// name were found when identifying the root packages.
    #[error("Found multiple packages matching `{name}`", name = name.cyan())]
    MultipleRootPackages {
        /// The ID of the package.
        name: PackageName,
    },
    /// An error that occurs when a root package can't be found.
    #[error("Could not find root package `{name}`", name = name.cyan())]
    MissingRootPackage {
        /// The ID of the package.
        name: PackageName,
    },
    /// An error that occurs when resolving metadata for a package.
    #[error("Failed to generate package metadata for `{id}`", id = id.cyan())]
    Resolution {
        /// The ID of the distribution that failed to resolve.
        id: PackageId,
        /// The inner error we forward.
        #[source]
        err: uv_distribution::Error,
    },
    /// A package has inconsistent versions in a single entry
    // Using name instead of id since the version in the id is part of the conflict.
    #[error("The entry for package `{name}` v{version} has wheel `{wheel_filename}` with inconsistent version: v{wheel_version} ", name = name.cyan(), wheel_filename = wheel.filename, wheel_version = wheel.filename.version)]
    InconsistentVersions {
        /// The name of the package with the inconsistent entry.
        name: PackageName,
        /// The version of the package with the inconsistent entry.
        version: Version,
        /// The wheel with the inconsistent version.
        wheel: Wheel,
    },
    #[error(
        "Found conflicting extras `{package1}[{extra1}]` \
         and `{package2}[{extra2}]` enabled simultaneously"
    )]
    ConflictingExtra {
        package1: PackageName,
        extra1: ExtraName,
        package2: PackageName,
        extra2: ExtraName,
    },
    #[error(transparent)]
    GitUrlParse(#[from] GitUrlParseError),
    #[error("Failed to fetch and query variants for `{package_id}`")]
    VariantError {
        package_id: PackageId,
        #[source]
        err: uv_distribution::Error,
    },
}

/// An error that occurs when a source string could not be parsed.
#[derive(Debug, thiserror::Error)]
enum SourceParseError {
    /// An error that occurs when the URL in the source is invalid.
    #[error("Invalid URL in source `{given}`")]
    InvalidUrl {
        /// The source string given.
        given: String,
        /// The URL parse error.
        #[source]
        err: url::ParseError,
    },
    /// An error that occurs when a Git URL is missing a precise commit SHA.
    #[error("Missing SHA in source `{given}`")]
    MissingSha {
        /// The source string given.
        given: String,
    },
    /// An error that occurs when a Git URL has an invalid SHA.
    #[error("Invalid SHA in source `{given}`")]
    InvalidSha {
        /// The source string given.
        given: String,
    },
}

/// An error that occurs when a hash digest could not be parsed.
#[derive(Clone, Debug, Eq, PartialEq)]
struct HashParseError(&'static str);

impl std::error::Error for HashParseError {}

impl Display for HashParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        Display::fmt(self.0, f)
    }
}

/// Format an array so that each element is on its own line and has a trailing comma.
///
/// Example:
///
/// ```toml
/// dependencies = [
///     { name = "idna" },
///     { name = "sniffio" },
/// ]
/// ```
fn each_element_on_its_line_array(elements: impl Iterator<Item = impl Into<Value>>) -> Array {
    let mut array = elements
        .map(|item| {
            let mut value = item.into();
            // Each dependency is on its own line and indented.
            value.decor_mut().set_prefix("\n    ");
            value
        })
        .collect::<Array>();
    // With a trailing comma, inserting another entry doesn't change the preceding line,
    // reducing the diff noise.
    array.set_trailing_comma(true);
    // The line break between the last element's comma and the closing square bracket.
    array.set_trailing("\n");
    array
}

/// Returns the simplified string-ified version of each marker given.
///
/// Note that the marker strings returned will include conflict markers if they
/// are present.
fn simplified_universal_markers(
    markers: &[UniversalMarker],
    requires_python: &RequiresPython,
) -> Vec<String> {
    let mut pep508_only = vec![];
    let mut seen = FxHashSet::default();
    for marker in markers {
        let simplified =
            SimplifiedMarkerTree::new(requires_python, marker.pep508()).as_simplified_marker_tree();
        if seen.insert(simplified) {
            pep508_only.push(simplified);
        }
    }
    let any_overlap = pep508_only
        .iter()
        .tuple_combinations()
        .any(|(&marker1, &marker2)| !marker1.is_disjoint(marker2));
    let markers = if !any_overlap {
        pep508_only
    } else {
        markers
            .iter()
            .map(|marker| {
                SimplifiedMarkerTree::new(requires_python, marker.combined())
                    .as_simplified_marker_tree()
            })
            .collect()
    };
    markers
        .into_iter()
        .filter_map(MarkerTree::try_to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use uv_warnings::anstream;

    use super::*;

    /// Assert a given display snapshot, stripping ANSI color codes.
    macro_rules! assert_stripped_snapshot {
        ($expr:expr, @$snapshot:literal) => {{
            let expr = format!("{}", $expr);
            let expr = format!("{}", anstream::adapter::strip_str(&expr));
            insta::assert_snapshot!(expr, @$snapshot);
        }};
    }

    #[test]
    fn missing_dependency_source_unambiguous() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
version = "0.1.0"
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn missing_dependency_version_unambiguous() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
source = { registry = "https://pypi.org/simple" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn missing_dependency_source_version_unambiguous() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn missing_dependency_source_ambiguous() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "a"
version = "0.1.1"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
version = "0.1.0"
"#;
        let result = toml::from_str::<Lock>(data).unwrap_err();
        assert_stripped_snapshot!(result, @"Dependency `a` has missing `source` field but has more than one matching package");
    }

    #[test]
    fn missing_dependency_version_ambiguous() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "a"
version = "0.1.1"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
source = { registry = "https://pypi.org/simple" }
"#;
        let result = toml::from_str::<Lock>(data).unwrap_err();
        assert_stripped_snapshot!(result, @"Dependency `a` has missing `version` field but has more than one matching package");
    }

    #[test]
    fn missing_dependency_source_version_ambiguous() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "a"
version = "0.1.1"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
"#;
        let result = toml::from_str::<Lock>(data).unwrap_err();
        assert_stripped_snapshot!(result, @"Dependency `a` has missing `source` field but has more than one matching package");
    }

    #[test]
    fn missing_dependency_version_dynamic() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
source = { editable = "path/to/a" }

[[package]]
name = "a"
version = "0.1.1"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
source = { editable = "path/to/a" }
"#;
        let result = toml::from_str::<Lock>(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn hash_optional_missing() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { registry = "https://pypi.org/simple" }
wheels = [{ url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl" }]
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn hash_optional_present() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { registry = "https://pypi.org/simple" }
wheels = [{ url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8" }]
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn hash_required_present() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { path = "file:///foo/bar" }
wheels = [{ url = "file:///foo/bar/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8" }]
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn source_direct_no_subdir() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { url = "https://burntsushi.net" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn source_direct_has_subdir() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { url = "https://burntsushi.net", subdirectory = "wat/foo/bar" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn source_directory() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { directory = "path/to/dir" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn source_editable() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { editable = "path/to/dir" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }
}
