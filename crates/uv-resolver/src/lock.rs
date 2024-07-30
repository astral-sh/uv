#![allow(clippy::default_trait_access)]

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::{Debug, Display};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use either::Either;
use itertools::Itertools;
use path_slash::PathExt;
use petgraph::visit::EdgeRef;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Deserializer};
use toml_edit::{value, Array, ArrayOfTables, InlineTable, Item, Table, Value};
use url::Url;

use cache_key::RepositoryUrl;
use distribution_filename::WheelFilename;
use distribution_types::{
    BuiltDist, DirectUrlBuiltDist, DirectUrlSourceDist, DirectorySourceDist, Dist,
    DistributionMetadata, FileLocation, GitSourceDist, HashComparison, IndexUrl, Name,
    PathBuiltDist, PathSourceDist, PrioritizedDist, RegistryBuiltDist, RegistryBuiltWheel,
    RegistrySourceDist, RemoteSource, Resolution, ResolvedDist, SourceDistCompatibility,
    ToUrlError, UrlString, VersionId, WheelCompatibility,
};
use pep440_rs::{Version, VersionSpecifier};
use pep508_rs::{
    ExtraOperator, MarkerEnvironment, MarkerExpression, MarkerTree, VerbatimUrl, VerbatimUrlError,
};
use platform_tags::{TagCompatibility, TagPriority, Tags};
use pypi_types::{
    HashDigest, ParsedArchiveUrl, ParsedGitUrl, ParsedUrl, Requirement, RequirementSource,
};
use uv_configuration::{ExtrasSpecification, Upgrade};
use uv_distribution::{ArchiveMetadata, Metadata};
use uv_git::{GitReference, GitSha, RepositoryReference, ResolvedRepositoryReference};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_workspace::VirtualProject;

use crate::resolution::{AnnotatedDist, ResolutionGraphNode};
use crate::resolver::FxOnceMap;
use crate::{
    ExcludeNewer, InMemoryIndex, MetadataResponse, PreReleaseMode, RequiresPython, ResolutionGraph,
    ResolutionMode, VersionMap, VersionsResponse,
};

/// The current version of the lockfile format.
const VERSION: u32 = 1;

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(try_from = "LockWire")]
pub struct Lock {
    version: u32,
    /// If this lockfile was built from a forking resolution with non-identical forks, store the
    /// forks in the lockfile so we can recreate them in subsequent resolutions.
    #[serde(rename = "environment-markers")]
    fork_markers: Option<BTreeSet<MarkerTree>>,
    /// The range of supported Python versions.
    requires_python: Option<RequiresPython>,
    /// The [`ResolutionMode`] used to generate this lock.
    resolution_mode: ResolutionMode,
    /// The [`PreReleaseMode`] used to generate this lock.
    prerelease_mode: PreReleaseMode,
    /// The [`ExcludeNewer`] used to generate this lock.
    exclude_newer: Option<ExcludeNewer>,
    /// The actual locked version and their metadata.
    distributions: Vec<Distribution>,
    /// A map from distribution ID to index in `distributions`.
    ///
    /// This can be used to quickly lookup the full distribution for any ID
    /// in this lock. For example, the dependencies for each distribution are
    /// listed as distributions IDs. This map can be used to find the full
    /// distribution for each such dependency.
    ///
    /// It is guaranteed that every distribution in this lock has an entry in
    /// this map, and that every dependency for every distribution has an ID
    /// that exists in this map. That is, there are no dependencies that don't
    /// have a corresponding locked distribution entry in the same lockfile.
    by_id: FxHashMap<DistributionId, usize>,
}

impl Lock {
    /// Initialize a [`Lock`] from a [`ResolutionGraph`].
    pub fn from_resolution_graph(graph: &ResolutionGraph) -> Result<Self, LockError> {
        let mut locked_dists = BTreeMap::new();

        // Lock all base packages.
        for node_index in graph.petgraph.node_indices() {
            let ResolutionGraphNode::Dist(dist) = &graph.petgraph[node_index] else {
                continue;
            };
            if dist.is_base() {
                let fork_markers = graph
                    .fork_markers(dist.name(), &dist.version, dist.dist.version_or_url().url())
                    .cloned();
                let mut locked_dist = Distribution::from_annotated_dist(dist, fork_markers)?;

                // Add all dependencies
                for edge in graph.petgraph.edges(node_index) {
                    let ResolutionGraphNode::Dist(dependency_dist) = &graph.petgraph[edge.target()]
                    else {
                        continue;
                    };
                    let marker = edge.weight().as_ref();
                    locked_dist.add_dependency(dependency_dist, marker);
                }
                let id = locked_dist.id.clone();
                if let Some(locked_dist) = locked_dists.insert(id, locked_dist) {
                    return Err(LockErrorKind::DuplicateDistribution {
                        id: locked_dist.id.clone(),
                    }
                    .into());
                }
            }
        }

        // Lock all extras and development dependencies.
        for node_index in graph.petgraph.node_indices() {
            let ResolutionGraphNode::Dist(dist) = &graph.petgraph[node_index] else {
                continue;
            };
            if let Some(extra) = dist.extra.as_ref() {
                let id = DistributionId::from_annotated_dist(dist);
                let Some(locked_dist) = locked_dists.get_mut(&id) else {
                    return Err(LockErrorKind::MissingExtraBase {
                        id,
                        extra: extra.clone(),
                    }
                    .into());
                };
                for edge in graph.petgraph.edges(node_index) {
                    let ResolutionGraphNode::Dist(dependency_dist) = &graph.petgraph[edge.target()]
                    else {
                        continue;
                    };
                    let marker = edge.weight().as_ref();
                    locked_dist.add_optional_dependency(extra.clone(), dependency_dist, marker);
                }
            }
            if let Some(group) = dist.dev.as_ref() {
                let id = DistributionId::from_annotated_dist(dist);
                let Some(locked_dist) = locked_dists.get_mut(&id) else {
                    return Err(LockErrorKind::MissingDevBase {
                        id,
                        group: group.clone(),
                    }
                    .into());
                };
                for edge in graph.petgraph.edges(node_index) {
                    let ResolutionGraphNode::Dist(dependency_dist) = &graph.petgraph[edge.target()]
                    else {
                        continue;
                    };
                    let marker = edge.weight().as_ref();
                    locked_dist.add_dev_dependency(group.clone(), dependency_dist, marker);
                }
            }
        }

        let distributions = locked_dists.into_values().collect();
        let requires_python = graph.requires_python.clone();
        let options = graph.options;
        let lock = Self::new(
            VERSION,
            distributions,
            requires_python,
            options.resolution_mode,
            options.prerelease_mode,
            options.exclude_newer,
            graph.fork_markers.clone(),
        )?;
        Ok(lock)
    }

    /// Initialize a [`Lock`] from a list of [`Distribution`] entries.
    fn new(
        version: u32,
        mut distributions: Vec<Distribution>,
        requires_python: Option<RequiresPython>,
        resolution_mode: ResolutionMode,
        prerelease_mode: PreReleaseMode,
        exclude_newer: Option<ExcludeNewer>,
        fork_markers: Option<BTreeSet<MarkerTree>>,
    ) -> Result<Self, LockError> {
        // Put all dependencies for each distribution in a canonical order and
        // check for duplicates.
        for dist in &mut distributions {
            dist.dependencies.sort();
            for windows in dist.dependencies.windows(2) {
                let (dep1, dep2) = (&windows[0], &windows[1]);
                if dep1 == dep2 {
                    return Err(LockErrorKind::DuplicateDependency {
                        id: dist.id.clone(),
                        dependency: dep1.clone(),
                    }
                    .into());
                }
            }

            // Perform the same validation for optional dependencies.
            for (extra, dependencies) in &mut dist.optional_dependencies {
                dependencies.sort();
                for windows in dependencies.windows(2) {
                    let (dep1, dep2) = (&windows[0], &windows[1]);
                    if dep1 == dep2 {
                        return Err(LockErrorKind::DuplicateOptionalDependency {
                            id: dist.id.clone(),
                            extra: extra.clone(),
                            dependency: dep1.clone(),
                        }
                        .into());
                    }
                }
            }

            // Perform the same validation for dev dependencies.
            for (group, dependencies) in &mut dist.dev_dependencies {
                dependencies.sort();
                for windows in dependencies.windows(2) {
                    let (dep1, dep2) = (&windows[0], &windows[1]);
                    if dep1 == dep2 {
                        return Err(LockErrorKind::DuplicateDevDependency {
                            id: dist.id.clone(),
                            group: group.clone(),
                            dependency: dep1.clone(),
                        }
                        .into());
                    }
                }
            }

            // Remove wheels that don't match `requires-python` and can't be selected for
            // installation.
            if let Some(requires_python) = &requires_python {
                dist.wheels
                    .retain(|wheel| requires_python.matches_wheel_tag(&wheel.filename));
            }
        }
        distributions.sort_by(|dist1, dist2| dist1.id.cmp(&dist2.id));

        // Check for duplicate distribution IDs and also build up the map for
        // distributions keyed by their ID.
        let mut by_id = FxHashMap::default();
        for (i, dist) in distributions.iter().enumerate() {
            if by_id.insert(dist.id.clone(), i).is_some() {
                return Err(LockErrorKind::DuplicateDistribution {
                    id: dist.id.clone(),
                }
                .into());
            }
        }

        // Build up a map from ID to extras.
        let mut extras_by_id = FxHashMap::default();
        for dist in &distributions {
            for extra in dist.optional_dependencies.keys() {
                extras_by_id
                    .entry(dist.id.clone())
                    .or_insert_with(FxHashSet::default)
                    .insert(extra.clone());
            }
        }

        // Remove any non-existent extras (e.g., extras that were requested but don't exist).
        for dist in &mut distributions {
            for dep in dist
                .dependencies
                .iter_mut()
                .chain(dist.optional_dependencies.values_mut().flatten())
                .chain(dist.dev_dependencies.values_mut().flatten())
            {
                dep.extra.retain(|extra| {
                    extras_by_id
                        .get(&dep.distribution_id)
                        .is_some_and(|extras| extras.contains(extra))
                });
            }
        }

        // Check that every dependency has an entry in `by_id`. If any don't,
        // it implies we somehow have a dependency with no corresponding locked
        // distribution.
        for dist in &distributions {
            for dep in &dist.dependencies {
                if !by_id.contains_key(&dep.distribution_id) {
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
                    if !by_id.contains_key(&dep.distribution_id) {
                        return Err(LockErrorKind::UnrecognizedDependency {
                            id: dist.id.clone(),
                            dependency: dep.clone(),
                        }
                        .into());
                    }
                }
            }

            // Perform the same validation for dev dependencies.
            for dependencies in dist.dev_dependencies.values() {
                for dep in dependencies {
                    if !by_id.contains_key(&dep.distribution_id) {
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
        Ok(Self {
            version,
            fork_markers,
            requires_python,
            resolution_mode,
            prerelease_mode,
            exclude_newer,
            distributions,
            by_id,
        })
    }

    /// Returns the [`Distribution`] entries in this lock.
    pub fn distributions(&self) -> &[Distribution] {
        &self.distributions
    }

    /// Returns the owned [`Distribution`] entries in this lock.
    pub fn into_distributions(self) -> Vec<Distribution> {
        self.distributions
    }

    /// Returns the supported Python version range for the lockfile, if present.
    pub fn requires_python(&self) -> Option<&RequiresPython> {
        self.requires_python.as_ref()
    }

    /// Returns the resolution mode used to generate this lock.
    pub fn resolution_mode(&self) -> ResolutionMode {
        self.resolution_mode
    }

    /// Returns the pre-release mode used to generate this lock.
    pub fn prerelease_mode(&self) -> PreReleaseMode {
        self.prerelease_mode
    }

    /// Returns the exclude newer setting used to generate this lock.
    pub fn exclude_newer(&self) -> Option<ExcludeNewer> {
        self.exclude_newer
    }

    /// If this lockfile was built from a forking resolution with non-identical forks, return the
    /// markers of those forks, otherwise `None`.
    pub fn fork_markers(&self) -> &Option<BTreeSet<MarkerTree>> {
        &self.fork_markers
    }

    /// Convert the [`Lock`] to a [`Resolution`] using the given marker environment, tags, and root.
    pub fn to_resolution(
        &self,
        project: &VirtualProject,
        marker_env: &MarkerEnvironment,
        tags: &Tags,
        extras: &ExtrasSpecification,
        dev: &[GroupName],
    ) -> Result<Resolution, LockError> {
        let mut queue: VecDeque<(&Distribution, Option<&ExtraName>)> = VecDeque::new();
        let mut seen = FxHashSet::default();

        // Add the workspace packages to the queue.
        for root_name in project.packages() {
            let root = self
                .find_by_name(root_name)
                .expect("found too many distributions matching root")
                .expect("could not find root");

            // Add the base package.
            queue.push_back((root, None));

            // Add any extras.
            match extras {
                ExtrasSpecification::None => {}
                ExtrasSpecification::All => {
                    for extra in root.optional_dependencies.keys() {
                        queue.push_back((root, Some(extra)));
                    }
                }
                ExtrasSpecification::Some(extras) => {
                    for extra in extras {
                        queue.push_back((root, Some(extra)));
                    }
                }
            }
        }

        let mut map = BTreeMap::default();
        let mut hashes = BTreeMap::default();
        while let Some((dist, extra)) = queue.pop_front() {
            let deps =
                if let Some(extra) = extra {
                    Either::Left(dist.optional_dependencies.get(extra).into_iter().flatten())
                } else {
                    Either::Right(dist.dependencies.iter().chain(
                        dev.iter().flat_map(|group| {
                            dist.dev_dependencies.get(group).into_iter().flatten()
                        }),
                    ))
                };
            for dep in deps {
                if dep
                    .marker
                    .as_ref()
                    .map_or(true, |marker| marker.evaluate(marker_env, &[]))
                {
                    let dep_dist = self.find_by_id(&dep.distribution_id);
                    if seen.insert((&dep.distribution_id, None)) {
                        queue.push_back((dep_dist, None));
                    }
                    for extra in &dep.extra {
                        if seen.insert((&dep.distribution_id, Some(extra))) {
                            queue.push_back((dep_dist, Some(extra)));
                        }
                    }
                }
            }
            map.insert(
                dist.id.name.clone(),
                ResolvedDist::Installable(dist.to_dist(project.workspace().install_path(), tags)?),
            );
            hashes.insert(dist.id.name.clone(), dist.hashes());
        }
        let diagnostics = vec![];
        Ok(Resolution::new(map, hashes, diagnostics))
    }

    /// Returns the TOML representation of this lockfile.
    pub fn to_toml(&self) -> anyhow::Result<String> {
        // We construct a TOML document manually instead of going through Serde to enable
        // the use of inline tables.
        let mut doc = toml_edit::DocumentMut::new();
        doc.insert("version", value(i64::from(self.version)));

        if let Some(ref requires_python) = self.requires_python {
            doc.insert("requires-python", value(requires_python.to_string()));
        }
        if let Some(ref fork_markers) = self.fork_markers {
            let fork_markers =
                each_element_on_its_line_array(fork_markers.iter().map(ToString::to_string));
            doc.insert("environment-markers", value(fork_markers));
        }

        // Write the settings that were used to generate the resolution.
        // This enables us to invalidate the lockfile if the user changes
        // their settings.
        if self.resolution_mode != ResolutionMode::default() {
            doc.insert("resolution-mode", value(self.resolution_mode.to_string()));
        }
        if self.prerelease_mode != PreReleaseMode::default() {
            doc.insert("prerelease-mode", value(self.prerelease_mode.to_string()));
        }
        if let Some(exclude_newer) = self.exclude_newer {
            doc.insert("exclude-newer", value(exclude_newer.to_string()));
        }

        // Count the number of distributions for each package name. When
        // there's only one distribution for a particular package name (the
        // overwhelmingly common case), we can omit some data (like source and
        // version) on dependency edges since it is strictly redundant.
        let mut dist_count_by_name: FxHashMap<PackageName, u64> = FxHashMap::default();
        for dist in &self.distributions {
            *dist_count_by_name.entry(dist.id.name.clone()).or_default() += 1;
        }

        let mut distributions = ArrayOfTables::new();
        for dist in &self.distributions {
            distributions.push(dist.to_toml(&dist_count_by_name)?);
        }

        doc.insert("distribution", Item::ArrayOfTables(distributions));
        Ok(doc.to_string())
    }

    /// Returns the distribution with the given name. If there are multiple
    /// matching distributions, then an error is returned. If there are no
    /// matching distributions, then `Ok(None)` is returned.
    fn find_by_name(&self, name: &PackageName) -> Result<Option<&Distribution>, String> {
        let mut found_dist = None;
        for dist in &self.distributions {
            if &dist.id.name == name {
                if found_dist.is_some() {
                    return Err(format!("found multiple distributions matching `{name}`"));
                }
                found_dist = Some(dist);
            }
        }
        Ok(found_dist)
    }

    fn find_by_id(&self, id: &DistributionId) -> &Distribution {
        let index = *self.by_id.get(id).expect("locked distribution for ID");
        let dist = self
            .distributions
            .get(index)
            .expect("valid index for distribution");
        dist
    }

    /// Convert the [`Lock`] to a [`InMemoryIndex`] that can be used for resolution.
    ///
    /// Any packages specified to be upgraded will be ignored.
    pub fn to_index(
        &self,
        install_path: &Path,
        upgrade: &Upgrade,
    ) -> Result<InMemoryIndex, LockError> {
        let distributions =
            FxOnceMap::with_capacity_and_hasher(self.distributions.len(), Default::default());
        let mut packages: FxHashMap<_, BTreeMap<Version, PrioritizedDist>> =
            FxHashMap::with_capacity_and_hasher(self.distributions.len(), Default::default());

        for distribution in &self.distributions {
            // Skip packages that may be upgraded from their pinned version.
            if upgrade.contains(distribution.name()) {
                continue;
            }

            match distribution.id.source {
                Source::Registry(..) | Source::Git(..) => {}
                // Skip local and direct URL dependencies, as their metadata may have been mutated
                // without a version change.
                Source::Path(..)
                | Source::Directory(..)
                | Source::Editable(..)
                | Source::Direct(..) => continue,
            }

            // Add registry distributions to the package index.
            if let Some(prioritized_dist) = distribution.to_prioritized_dist(install_path)? {
                packages
                    .entry(distribution.name().clone())
                    .or_default()
                    .insert(distribution.id.version.clone(), prioritized_dist);
            }

            // Extract the distribution metadata.
            let version_id = distribution.version_id(install_path)?;
            let hashes = distribution.hashes();
            let metadata = distribution.to_metadata(install_path)?;

            // Add metadata to the distributions index.
            let response = MetadataResponse::Found(ArchiveMetadata::with_hashes(metadata, hashes));
            distributions.done(version_id, Arc::new(response));
        }

        let packages = packages
            .into_iter()
            .map(|(name, versions)| {
                let response = VersionsResponse::Found(vec![VersionMap::from(versions)]);
                (name, Arc::new(response))
            })
            .collect();

        Ok(InMemoryIndex::with(packages, distributions))
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct LockWire {
    version: u32,
    #[serde(default)]
    requires_python: Option<RequiresPython>,
    /// If this lockfile was built from a forking resolution with non-identical forks, store the
    /// forks in the lockfile so we can recreate them in subsequent resolutions.
    #[serde(rename = "environment-markers")]
    fork_markers: Option<BTreeSet<MarkerTree>>,
    #[serde(default)]
    resolution_mode: ResolutionMode,
    #[serde(default)]
    prerelease_mode: PreReleaseMode,
    #[serde(default)]
    exclude_newer: Option<ExcludeNewer>,
    #[serde(rename = "distribution", default)]
    distributions: Vec<DistributionWire>,
}

impl From<Lock> for LockWire {
    fn from(lock: Lock) -> LockWire {
        LockWire {
            version: lock.version,
            requires_python: lock.requires_python,
            fork_markers: lock.fork_markers,
            resolution_mode: lock.resolution_mode,
            prerelease_mode: lock.prerelease_mode,
            exclude_newer: lock.exclude_newer,
            distributions: lock
                .distributions
                .into_iter()
                .map(DistributionWire::from)
                .collect(),
        }
    }
}

impl TryFrom<LockWire> for Lock {
    type Error = LockError;

    fn try_from(wire: LockWire) -> Result<Lock, LockError> {
        // Count the number of distributions for each package name. When
        // there's only one distribution for a particular package name (the
        // overwhelmingly common case), we can omit some data (like source and
        // version) on dependency edges since it is strictly redundant.
        let mut unambiguous_dist_ids: FxHashMap<PackageName, DistributionId> = FxHashMap::default();
        let mut ambiguous = FxHashSet::default();
        for dist in &wire.distributions {
            if ambiguous.contains(&dist.id.name) {
                continue;
            }
            if unambiguous_dist_ids.remove(&dist.id.name).is_some() {
                ambiguous.insert(dist.id.name.clone());
                continue;
            }
            unambiguous_dist_ids.insert(dist.id.name.clone(), dist.id.clone());
        }

        let distributions = wire
            .distributions
            .into_iter()
            .map(|dist| dist.unwire(&unambiguous_dist_ids))
            .collect::<Result<Vec<_>, _>>()?;
        Lock::new(
            wire.version,
            distributions,
            wire.requires_python,
            wire.resolution_mode,
            wire.prerelease_mode,
            wire.exclude_newer,
            wire.fork_markers,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Distribution {
    pub(crate) id: DistributionId,
    sdist: Option<SourceDist>,
    wheels: Vec<Wheel>,
    /// If there are multiple distributions for the same package name, we add the markers of the
    /// fork(s) that contained this distribution, so we can set the correct preferences in the next
    /// resolution.
    ///
    /// Named `environment-markers` in `uv.lock`.
    fork_markers: Option<BTreeSet<MarkerTree>>,
    dependencies: Vec<Dependency>,
    optional_dependencies: BTreeMap<ExtraName, Vec<Dependency>>,
    dev_dependencies: BTreeMap<GroupName, Vec<Dependency>>,
}

impl Distribution {
    fn from_annotated_dist(
        annotated_dist: &AnnotatedDist,
        fork_markers: Option<BTreeSet<MarkerTree>>,
    ) -> Result<Self, LockError> {
        let id = DistributionId::from_annotated_dist(annotated_dist);
        let sdist = SourceDist::from_annotated_dist(&id, annotated_dist)?;
        let wheels = Wheel::from_annotated_dist(annotated_dist)?;
        Ok(Distribution {
            id,
            sdist,
            wheels,
            fork_markers,
            dependencies: vec![],
            optional_dependencies: BTreeMap::default(),
            dev_dependencies: BTreeMap::default(),
        })
    }

    /// Add the [`AnnotatedDist`] as a dependency of the [`Distribution`].
    fn add_dependency(&mut self, annotated_dist: &AnnotatedDist, marker: Option<&MarkerTree>) {
        let new_dep = Dependency::from_annotated_dist(annotated_dist, marker);
        for existing_dep in &mut self.dependencies {
            if existing_dep.distribution_id == new_dep.distribution_id
                && existing_dep.marker == new_dep.marker
            {
                existing_dep.extra.extend(new_dep.extra);
                return;
            }
        }
        self.dependencies.push(new_dep);
    }

    /// Add the [`AnnotatedDist`] as an optional dependency of the [`Distribution`].
    fn add_optional_dependency(
        &mut self,
        extra: ExtraName,
        annotated_dist: &AnnotatedDist,
        marker: Option<&MarkerTree>,
    ) {
        self.optional_dependencies
            .entry(extra)
            .or_default()
            .push(Dependency::from_annotated_dist(annotated_dist, marker));
    }

    /// Add the [`AnnotatedDist`] as a development dependency of the [`Distribution`].
    fn add_dev_dependency(
        &mut self,
        dev: GroupName,
        annotated_dist: &AnnotatedDist,
        marker: Option<&MarkerTree>,
    ) {
        self.dev_dependencies
            .entry(dev)
            .or_default()
            .push(Dependency::from_annotated_dist(annotated_dist, marker));
    }

    /// Convert the [`Distribution`] to a [`Dist`] that can be used in installation.
    fn to_dist(&self, workspace_root: &Path, tags: &Tags) -> Result<Dist, LockError> {
        if let Some(best_wheel_index) = self.find_best_wheel(tags) {
            return match &self.id.source {
                Source::Registry(url) => {
                    let wheels = self
                        .wheels
                        .iter()
                        .map(|wheel| wheel.to_registry_dist(url))
                        .collect();
                    let reg_built_dist = RegistryBuiltDist {
                        wheels,
                        best_wheel_index,
                        sdist: None,
                    };
                    Ok(Dist::Built(BuiltDist::Registry(reg_built_dist)))
                }
                Source::Path(path) => {
                    let filename: WheelFilename = self.wheels[best_wheel_index].filename.clone();
                    let path_dist = PathBuiltDist {
                        filename,
                        url: verbatim_url(workspace_root.join(path), &self.id)?,
                        path: path.clone(),
                    };
                    let built_dist = BuiltDist::Path(path_dist);
                    Ok(Dist::Built(built_dist))
                }
                Source::Direct(url, direct) => {
                    let filename: WheelFilename = self.wheels[best_wheel_index].filename.clone();
                    let url = Url::from(ParsedArchiveUrl {
                        url: url.clone(),
                        subdirectory: direct.subdirectory.as_ref().map(PathBuf::from),
                    });
                    let direct_dist = DirectUrlBuiltDist {
                        filename,
                        location: url.clone(),
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
            };
        }

        if let Some(sdist) = self.to_source_dist(workspace_root)? {
            return Ok(Dist::Source(sdist));
        }

        Err(LockErrorKind::NeitherSourceDistNorWheel {
            id: self.id.clone(),
        }
        .into())
    }

    /// Convert the source of this [`Distribution`] to a [`SourceDist`] that can be used in installation.
    ///
    /// Returns `Ok(None)` if the source cannot be converted because `self.sdist` is `None`. This is required
    /// for registry sources.
    fn to_source_dist(
        &self,
        workspace_root: &Path,
    ) -> Result<Option<distribution_types::SourceDist>, LockError> {
        let sdist = match &self.id.source {
            Source::Path(path) => {
                let path_dist = PathSourceDist {
                    name: self.id.name.clone(),
                    url: verbatim_url(workspace_root.join(path), &self.id)?,
                    install_path: workspace_root.join(path),
                    lock_path: path.clone(),
                };
                distribution_types::SourceDist::Path(path_dist)
            }
            Source::Directory(path) => {
                let dir_dist = DirectorySourceDist {
                    name: self.id.name.clone(),
                    url: verbatim_url(workspace_root.join(path), &self.id)?,
                    install_path: workspace_root.join(path),
                    lock_path: path.clone(),
                    editable: false,
                };
                distribution_types::SourceDist::Directory(dir_dist)
            }
            Source::Editable(path) => {
                let dir_dist = DirectorySourceDist {
                    name: self.id.name.clone(),
                    url: verbatim_url(workspace_root.join(path), &self.id)?,
                    install_path: workspace_root.join(path),
                    lock_path: path.clone(),
                    editable: true,
                };
                distribution_types::SourceDist::Directory(dir_dist)
            }
            Source::Git(url, git) => {
                // Reconstruct the `GitUrl` from the `GitSource`.
                let git_url =
                    uv_git::GitUrl::new(url.clone(), GitReference::from(git.kind.clone()))
                        .with_precise(git.precise);

                // Reconstruct the PEP 508-compatible URL from the `GitSource`.
                let url = Url::from(ParsedGitUrl {
                    url: git_url.clone(),
                    subdirectory: git.subdirectory.as_ref().map(PathBuf::from),
                });

                let git_dist = GitSourceDist {
                    name: self.id.name.clone(),
                    url: VerbatimUrl::from_url(url),
                    git: Box::new(git_url),
                    subdirectory: git.subdirectory.as_ref().map(PathBuf::from),
                };
                distribution_types::SourceDist::Git(git_dist)
            }
            Source::Direct(url, direct) => {
                let url = Url::from(ParsedArchiveUrl {
                    url: url.clone(),
                    subdirectory: direct.subdirectory.as_ref().map(PathBuf::from),
                });
                let direct_dist = DirectUrlSourceDist {
                    name: self.id.name.clone(),
                    location: url.clone(),
                    subdirectory: direct.subdirectory.as_ref().map(PathBuf::from),
                    url: VerbatimUrl::from_url(url),
                };
                distribution_types::SourceDist::DirectUrl(direct_dist)
            }
            Source::Registry(url) => {
                let Some(ref sdist) = self.sdist else {
                    return Ok(None);
                };

                let file_url = sdist.url().ok_or_else(|| LockErrorKind::MissingUrl {
                    id: self.id.clone(),
                })?;
                let filename = sdist
                    .filename()
                    .ok_or_else(|| LockErrorKind::MissingFilename {
                        id: self.id.clone(),
                    })?;
                let file = Box::new(distribution_types::File {
                    dist_info_metadata: false,
                    filename: filename.to_string(),
                    hashes: sdist
                        .hash()
                        .map(|hash| vec![hash.0.clone()])
                        .unwrap_or_default(),
                    requires_python: None,
                    size: sdist.size(),
                    upload_time_utc_ms: None,
                    url: FileLocation::AbsoluteUrl(file_url.clone()),
                    yanked: None,
                });
                let index = IndexUrl::Url(VerbatimUrl::from_url(url.clone()));

                let reg_dist = RegistrySourceDist {
                    name: self.id.name.clone(),
                    version: self.id.version.clone(),
                    file,
                    index,
                    wheels: vec![],
                };
                distribution_types::SourceDist::Registry(reg_dist)
            }
        };

        Ok(Some(sdist))
    }

    /// Convert the [`Distribution`] to a [`PrioritizedDist`] that can be used for resolution, if
    /// it has a registry source.
    fn to_prioritized_dist(
        &self,
        workspace_root: &Path,
    ) -> Result<Option<PrioritizedDist>, LockError> {
        let prioritized_dist = match &self.id.source {
            Source::Registry(url) => {
                let mut prioritized_dist = PrioritizedDist::default();

                // Add the source distribution.
                if let Some(distribution_types::SourceDist::Registry(sdist)) =
                    self.to_source_dist(workspace_root)?
                {
                    // When resolving from a lockfile all sources are equally compatible.
                    let compat = SourceDistCompatibility::Compatible(HashComparison::Matched);
                    let hash = self
                        .sdist
                        .as_ref()
                        .and_then(|sdist| sdist.hash().map(|h| h.0.clone()));
                    prioritized_dist.insert_source(sdist, hash, compat);
                };

                // Add any wheels.
                for wheel in &self.wheels {
                    let hash = wheel.hash.as_ref().map(|h| h.0.clone());
                    let wheel = wheel.to_registry_dist(url);
                    let compat =
                        WheelCompatibility::Compatible(HashComparison::Matched, None, None);
                    prioritized_dist.insert_built(wheel, hash, compat);
                }

                prioritized_dist
            }
            _ => return Ok(None),
        };

        Ok(Some(prioritized_dist))
    }

    /// Convert the [`Distribution`] to [`Metadata`] that can be used for resolution.
    pub fn to_metadata(&self, workspace_root: &Path) -> Result<Metadata, LockError> {
        let name = self.name().clone();
        let version = self.id.version.clone();
        let provides_extras = self.optional_dependencies.keys().cloned().collect();

        let mut dependency_extras = FxHashMap::default();
        let mut requires_dist = self
            .dependencies
            .iter()
            .filter_map(|dep| {
                dep.to_requirement(workspace_root, &mut dependency_extras)
                    .transpose()
            })
            .collect::<Result<Vec<_>, LockError>>()?;

        // Denormalize optional dependencies.
        for (extra, deps) in &self.optional_dependencies {
            for dep in deps {
                if let Some(mut dep) = dep.to_requirement(workspace_root, &mut dependency_extras)? {
                    // Add back the extra marker expression.
                    let marker = MarkerTree::Expression(MarkerExpression::Extra {
                        operator: ExtraOperator::Equal,
                        name: extra.clone(),
                    });
                    match dep.marker {
                        Some(ref mut tree) => tree.and(marker),
                        None => dep.marker = Some(marker),
                    }

                    requires_dist.push(dep);
                }
            }
        }

        // Denormalize extras for each dependency.
        for req in &mut requires_dist {
            if let Some(extras) = dependency_extras.remove(&req.name) {
                req.extras = extras;
            }
        }

        let dev_dependencies = self
            .dev_dependencies
            .iter()
            .map(|(group, deps)| {
                let mut dependency_extras = FxHashMap::default();
                let mut deps = deps
                    .iter()
                    .filter_map(|dep| {
                        dep.to_requirement(workspace_root, &mut dependency_extras)
                            .transpose()
                    })
                    .collect::<Result<Vec<_>, LockError>>()?;

                // Denormalize extras for each development dependency.
                for dep in &mut deps {
                    if let Some(extras) = dependency_extras.remove(&dep.name) {
                        dep.extras = extras;
                    }
                }

                Ok((group.clone(), deps))
            })
            .collect::<Result<_, LockError>>()?;

        Ok(Metadata {
            name,
            version,
            requires_dist,
            dev_dependencies,
            provides_extras,
            requires_python: None,
        })
    }

    fn to_toml(&self, dist_count_by_name: &FxHashMap<PackageName, u64>) -> anyhow::Result<Table> {
        let mut table = Table::new();

        self.id.to_toml(None, &mut table);

        if let Some(ref fork_markers) = self.fork_markers {
            let wheels =
                each_element_on_its_line_array(fork_markers.iter().map(ToString::to_string));
            table.insert("environment-markers", value(wheels));
        }

        if !self.dependencies.is_empty() {
            let deps = each_element_on_its_line_array(
                self.dependencies
                    .iter()
                    .map(|dep| dep.to_toml(dist_count_by_name).into_inline_table()),
            );
            table.insert("dependencies", value(deps));
        }

        if !self.optional_dependencies.is_empty() {
            let mut optional_deps = Table::new();
            for (extra, deps) in &self.optional_dependencies {
                let deps = each_element_on_its_line_array(
                    deps.iter()
                        .map(|dep| dep.to_toml(dist_count_by_name).into_inline_table()),
                );
                optional_deps.insert(extra.as_ref(), value(deps));
            }
            table.insert("optional-dependencies", Item::Table(optional_deps));
        }

        if !self.dev_dependencies.is_empty() {
            let mut dev_dependencies = Table::new();
            for (extra, deps) in &self.dev_dependencies {
                let deps = each_element_on_its_line_array(
                    deps.iter()
                        .map(|dep| dep.to_toml(dist_count_by_name).into_inline_table()),
                );
                dev_dependencies.insert(extra.as_ref(), value(deps));
            }
            table.insert("dev-dependencies", Item::Table(dev_dependencies));
        }

        if let Some(ref sdist) = self.sdist {
            table.insert("sdist", value(sdist.to_toml()?));
        }

        if !self.wheels.is_empty() {
            let wheels = each_element_on_its_line_array(
                self.wheels
                    .iter()
                    .map(Wheel::to_toml)
                    .collect::<anyhow::Result<Vec<_>>>()?
                    .into_iter(),
            );
            table.insert("wheels", value(wheels));
        }

        Ok(table)
    }

    fn find_best_wheel(&self, tags: &Tags) -> Option<usize> {
        let mut best: Option<(TagPriority, usize)> = None;
        for (i, wheel) in self.wheels.iter().enumerate() {
            let TagCompatibility::Compatible(priority) = wheel.filename.compatibility(tags) else {
                continue;
            };
            match best {
                None => {
                    best = Some((priority, i));
                }
                Some((best_priority, _)) => {
                    if priority > best_priority {
                        best = Some((priority, i));
                    }
                }
            }
        }
        best.map(|(_, i)| i)
    }

    /// Returns the [`PackageName`] of the distribution.
    pub fn name(&self) -> &PackageName {
        &self.id.name
    }

    /// Returns the [`Version`] of the distribution.
    pub fn version(&self) -> &Version {
        &self.id.version
    }

    /// Returns a [`VersionId`] for this package that can be used for resolution.
    fn version_id(&self, workspace_root: &Path) -> Result<VersionId, LockError> {
        match &self.id.source {
            Source::Registry(_) => Ok(VersionId::NameVersion(
                self.name().clone(),
                self.id.version.clone(),
            )),
            _ => Ok(self.to_source_dist(workspace_root)?.unwrap().version_id()),
        }
    }

    /// Returns all the hashes associated with this [`Distribution`].
    fn hashes(&self) -> Vec<HashDigest> {
        let mut hashes = Vec::new();
        if let Some(ref sdist) = self.sdist {
            if let Some(hash) = sdist.hash() {
                hashes.push(hash.0.clone());
            }
        }
        for wheel in &self.wheels {
            hashes.extend(wheel.hash.as_ref().map(|h| h.0.clone()));
        }
        hashes
    }

    /// Returns the [`ResolvedRepositoryReference`] for the distribution, if it is a Git source.
    pub fn as_git_ref(&self) -> Option<ResolvedRepositoryReference> {
        match &self.id.source {
            Source::Git(url, git) => Some(ResolvedRepositoryReference {
                reference: RepositoryReference {
                    url: RepositoryUrl::new(url),
                    reference: GitReference::from(git.kind.clone()),
                },
                sha: git.precise,
            }),
            _ => None,
        }
    }
}

/// Attempts to construct a `VerbatimUrl` from the given `Path`.
fn verbatim_url(path: PathBuf, id: &DistributionId) -> Result<VerbatimUrl, LockError> {
    let url = VerbatimUrl::from_path(path).map_err(|err| LockErrorKind::VerbatimUrl {
        id: id.clone(),
        err,
    })?;

    Ok(url)
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct DistributionWire {
    #[serde(flatten)]
    id: DistributionId,
    #[serde(default)]
    sdist: Option<SourceDist>,
    #[serde(default)]
    wheels: Vec<Wheel>,
    #[serde(default, rename = "environment-markers")]
    fork_markers: BTreeSet<MarkerTree>,
    #[serde(default)]
    dependencies: Vec<DependencyWire>,
    #[serde(default)]
    optional_dependencies: BTreeMap<ExtraName, Vec<DependencyWire>>,
    #[serde(default)]
    dev_dependencies: BTreeMap<GroupName, Vec<DependencyWire>>,
}

impl DistributionWire {
    fn unwire(
        self,
        unambiguous_dist_ids: &FxHashMap<PackageName, DistributionId>,
    ) -> Result<Distribution, LockError> {
        let unwire_deps = |deps: Vec<DependencyWire>| -> Result<Vec<Dependency>, LockError> {
            deps.into_iter()
                .map(|dep| dep.unwire(unambiguous_dist_ids))
                .collect()
        };
        Ok(Distribution {
            id: self.id,
            sdist: self.sdist,
            wheels: self.wheels,
            fork_markers: (!self.fork_markers.is_empty()).then_some(self.fork_markers),
            dependencies: unwire_deps(self.dependencies)?,
            optional_dependencies: self
                .optional_dependencies
                .into_iter()
                .map(|(extra, deps)| Ok((extra, unwire_deps(deps)?)))
                .collect::<Result<_, LockError>>()?,
            dev_dependencies: self
                .dev_dependencies
                .into_iter()
                .map(|(group, deps)| Ok((group, unwire_deps(deps)?)))
                .collect::<Result<_, LockError>>()?,
        })
    }
}

impl From<Distribution> for DistributionWire {
    fn from(dist: Distribution) -> DistributionWire {
        let wire_deps = |deps: Vec<Dependency>| -> Vec<DependencyWire> {
            deps.into_iter().map(DependencyWire::from).collect()
        };
        DistributionWire {
            id: dist.id,
            sdist: dist.sdist,
            wheels: dist.wheels,
            fork_markers: dist.fork_markers.unwrap_or_default(),
            dependencies: wire_deps(dist.dependencies),
            optional_dependencies: dist
                .optional_dependencies
                .into_iter()
                .map(|(extra, deps)| (extra, wire_deps(deps)))
                .collect(),
            dev_dependencies: dist
                .dev_dependencies
                .into_iter()
                .map(|(group, deps)| (group, wire_deps(deps)))
                .collect(),
        }
    }
}

/// Inside the lockfile, we match a dependency entry to a distribution entry through a key made up
/// of the name, the version and the source url.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
pub(crate) struct DistributionId {
    pub(crate) name: PackageName,
    pub(crate) version: Version,
    source: Source,
}

impl DistributionId {
    fn from_annotated_dist(annotated_dist: &AnnotatedDist) -> DistributionId {
        let name = annotated_dist.metadata.name.clone();
        let version = annotated_dist.metadata.version.clone();
        let source = Source::from_resolved_dist(&annotated_dist.dist);
        DistributionId {
            name,
            version,
            source,
        }
    }

    /// Writes this distribution ID inline into the table given.
    ///
    /// When a map is given, and if the package name in this ID is unambiguous
    /// (i.e., it has a count of 1 in the map), then the `version` and `source`
    /// fields are omitted. In all other cases, including when a map is not
    /// given, the `version` and `source` fields are written.
    fn to_toml(&self, dist_count_by_name: Option<&FxHashMap<PackageName, u64>>, table: &mut Table) {
        let count = dist_count_by_name.and_then(|map| map.get(&self.name).copied());
        table.insert("name", value(self.name.to_string()));
        if count.map(|count| count > 1).unwrap_or(true) {
            table.insert("version", value(self.version.to_string()));
            self.source.to_toml(table);
        }
    }
}

impl std::fmt::Display for DistributionId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}=={} @ {}", self.name, self.version, self.source)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
struct DistributionIdForDependency {
    name: PackageName,
    version: Option<Version>,
    source: Option<Source>,
}

impl DistributionIdForDependency {
    fn unwire(
        self,
        unambiguous_dist_ids: &FxHashMap<PackageName, DistributionId>,
    ) -> Result<DistributionId, LockError> {
        let unambiguous_dist_id = unambiguous_dist_ids.get(&self.name);
        let version = self.version.map(Ok::<_, LockError>).unwrap_or_else(|| {
            let Some(dist_id) = unambiguous_dist_id else {
                return Err(LockErrorKind::MissingDependencyVersion {
                    name: self.name.clone(),
                }
                .into());
            };
            Ok(dist_id.version.clone())
        })?;
        let source = self.source.map(Ok::<_, LockError>).unwrap_or_else(|| {
            let Some(dist_id) = unambiguous_dist_id else {
                return Err(LockErrorKind::MissingDependencySource {
                    name: self.name.clone(),
                }
                .into());
            };
            Ok(dist_id.source.clone())
        })?;
        Ok(DistributionId {
            name: self.name,
            version,
            source,
        })
    }
}

impl From<DistributionId> for DistributionIdForDependency {
    fn from(id: DistributionId) -> DistributionIdForDependency {
        DistributionIdForDependency {
            name: id.name,
            version: Some(id.version),
            source: Some(id.source),
        }
    }
}

/// A unique identifier to differentiate between different distributions for the same version of a
/// package.
///
/// NOTE: Care should be taken when adding variants to this enum. Namely, new
/// variants should be added without changing the relative ordering of other
/// variants. Otherwise, this could cause the lockfile to have a different
/// canonical ordering of distributions.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
#[serde(try_from = "SourceWire")]
enum Source {
    Registry(Url),
    Git(Url, GitSource),
    Direct(Url, DirectSource),
    Path(PathBuf),
    Directory(PathBuf),
    Editable(PathBuf),
}

/// A [`PathBuf`], but we show `.` instead of an empty path.
///
/// We also normalize backslashes to forward slashes on Windows, to ensure
/// that the lockfile contains portable paths.
fn serialize_path_with_dot(path: &Path) -> Cow<str> {
    let path = path.to_slash_lossy();
    if path.is_empty() {
        Cow::Borrowed(".")
    } else {
        path
    }
}

impl Source {
    fn from_resolved_dist(resolved_dist: &ResolvedDist) -> Source {
        match *resolved_dist {
            // TODO: Do we want to try to lock already-installed distributions?
            // Or should we return an error?
            ResolvedDist::Installed(_) => todo!(),
            ResolvedDist::Installable(ref dist) => Source::from_dist(dist),
        }
    }

    fn from_dist(dist: &Dist) -> Source {
        match *dist {
            Dist::Built(ref built_dist) => Source::from_built_dist(built_dist),
            Dist::Source(ref source_dist) => Source::from_source_dist(source_dist),
        }
    }

    fn from_built_dist(built_dist: &BuiltDist) -> Source {
        match *built_dist {
            BuiltDist::Registry(ref reg_dist) => Source::from_registry_built_dist(reg_dist),
            BuiltDist::DirectUrl(ref direct_dist) => Source::from_direct_built_dist(direct_dist),
            BuiltDist::Path(ref path_dist) => Source::from_path_built_dist(path_dist),
        }
    }

    fn from_source_dist(source_dist: &distribution_types::SourceDist) -> Source {
        match *source_dist {
            distribution_types::SourceDist::Registry(ref reg_dist) => {
                Source::from_registry_source_dist(reg_dist)
            }
            distribution_types::SourceDist::DirectUrl(ref direct_dist) => {
                Source::from_direct_source_dist(direct_dist)
            }
            distribution_types::SourceDist::Git(ref git_dist) => Source::from_git_dist(git_dist),
            distribution_types::SourceDist::Path(ref path_dist) => {
                Source::from_path_source_dist(path_dist)
            }
            distribution_types::SourceDist::Directory(ref directory) => {
                Source::from_directory_source_dist(directory)
            }
        }
    }

    fn from_registry_built_dist(reg_dist: &RegistryBuiltDist) -> Source {
        Source::from_index_url(&reg_dist.best_wheel().index)
    }

    fn from_registry_source_dist(reg_dist: &RegistrySourceDist) -> Source {
        Source::from_index_url(&reg_dist.index)
    }

    fn from_direct_built_dist(direct_dist: &DirectUrlBuiltDist) -> Source {
        Source::Direct(
            direct_dist.url.to_url(),
            DirectSource { subdirectory: None },
        )
    }

    fn from_direct_source_dist(direct_dist: &DirectUrlSourceDist) -> Source {
        Source::Direct(
            direct_dist.url.to_url(),
            DirectSource {
                subdirectory: direct_dist
                    .subdirectory
                    .as_deref()
                    .and_then(Path::to_str)
                    .map(ToString::to_string),
            },
        )
    }

    fn from_path_built_dist(path_dist: &PathBuiltDist) -> Source {
        Source::Path(path_dist.path.clone())
    }

    fn from_path_source_dist(path_dist: &PathSourceDist) -> Source {
        Source::Path(path_dist.install_path.clone())
    }

    fn from_directory_source_dist(directory_dist: &DirectorySourceDist) -> Source {
        if directory_dist.editable {
            Source::Editable(directory_dist.lock_path.clone())
        } else {
            Source::Directory(directory_dist.lock_path.clone())
        }
    }

    fn from_index_url(index_url: &IndexUrl) -> Source {
        match *index_url {
            IndexUrl::Pypi(ref verbatim_url) => Source::Registry(verbatim_url.to_url()),
            IndexUrl::Url(ref verbatim_url) => Source::Registry(verbatim_url.to_url()),
            // TODO(konsti): Retain path on index url without converting to URL.
            IndexUrl::Path(ref verbatim_url) => Source::Path(
                verbatim_url
                    .to_file_path()
                    .expect("Could not convert index url to path"),
            ),
        }
    }

    fn from_git_dist(git_dist: &GitSourceDist) -> Source {
        Source::Git(
            locked_git_url(git_dist),
            GitSource {
                kind: GitSourceKind::from(git_dist.git.reference().clone()),
                precise: git_dist.git.precise().expect("precise commit"),
                subdirectory: git_dist
                    .subdirectory
                    .as_deref()
                    .and_then(Path::to_str)
                    .map(ToString::to_string),
            },
        )
    }

    fn to_toml(&self, table: &mut Table) {
        let mut source_table = InlineTable::new();
        match *self {
            Source::Registry(ref url) => {
                source_table.insert("registry", Value::from(url.as_str()));
            }
            Source::Git(ref url, _) => {
                source_table.insert("git", Value::from(url.as_str()));
            }
            Source::Direct(ref url, DirectSource { ref subdirectory }) => {
                source_table.insert("url", Value::from(url.as_str()));
                if let Some(ref subdirectory) = *subdirectory {
                    source_table.insert("subdirectory", Value::from(subdirectory));
                }
            }
            Source::Path(ref path) => {
                source_table.insert(
                    "path",
                    Value::from(serialize_path_with_dot(path).into_owned()),
                );
            }
            Source::Directory(ref path) => {
                source_table.insert(
                    "directory",
                    Value::from(serialize_path_with_dot(path).into_owned()),
                );
            }
            Source::Editable(ref path) => {
                source_table.insert(
                    "editable",
                    Value::from(serialize_path_with_dot(path).into_owned()),
                );
            }
        }
        table.insert("source", value(source_table));
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Source::Registry(url) | Source::Git(url, _) | Source::Direct(url, _) => {
                write!(f, "{}+{}", self.name(), url)
            }
            Source::Path(path) | Source::Directory(path) | Source::Editable(path) => {
                write!(f, "{}+{}", self.name(), serialize_path_with_dot(path))
            }
        }
    }
}

impl Source {
    fn name(&self) -> &str {
        match *self {
            Self::Registry(..) => "registry",
            Self::Git(..) => "git",
            Self::Direct(..) => "direct",
            Self::Path(..) => "path",
            Self::Directory(..) => "directory",
            Self::Editable(..) => "editable",
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
        match *self {
            Self::Registry(..) => None,
            Self::Direct(..) | Self::Path(..) => Some(true),
            Self::Git(..) | Self::Directory(..) | Self::Editable(..) => Some(false),
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(untagged)]
enum SourceWire {
    Registry {
        registry: Url,
    },
    Git {
        git: String,
    },
    Direct {
        url: Url,
        #[serde(default)]
        subdirectory: Option<String>,
    },
    Path {
        #[serde(deserialize_with = "deserialize_path_with_dot")]
        path: PathBuf,
    },
    Directory {
        #[serde(deserialize_with = "deserialize_path_with_dot")]
        directory: PathBuf,
    },
    Editable {
        #[serde(deserialize_with = "deserialize_path_with_dot")]
        editable: PathBuf,
    },
}

impl TryFrom<SourceWire> for Source {
    type Error = LockError;

    fn try_from(wire: SourceWire) -> Result<Source, LockError> {
        #[allow(clippy::enum_glob_use)]
        use self::SourceWire::*;

        match wire {
            Registry { registry } => Ok(Source::Registry(registry)),
            Git { git } => {
                let mut url = Url::parse(&git)
                    .map_err(|err| SourceParseError::InvalidUrl {
                        given: git.to_string(),
                        err,
                    })
                    .map_err(LockErrorKind::InvalidGitSourceUrl)?;
                let git_source = GitSource::from_url(&mut url)
                    .map_err(|err| match err {
                        GitSourceError::InvalidSha => SourceParseError::InvalidSha {
                            given: git.to_string(),
                        },
                        GitSourceError::MissingSha => SourceParseError::MissingSha {
                            given: git.to_string(),
                        },
                    })
                    .map_err(LockErrorKind::InvalidGitSourceUrl)?;
                Ok(Source::Git(url, git_source))
            }
            Direct { url, subdirectory } => Ok(Source::Direct(url, DirectSource { subdirectory })),
            Path { path } => Ok(Source::Path(path)),
            Directory { directory } => Ok(Source::Directory(directory)),
            Editable { editable } => Ok(Source::Editable(editable)),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
struct DirectSource {
    subdirectory: Option<String>,
}

/// NOTE: Care should be taken when adding variants to this enum. Namely, new
/// variants should be added without changing the relative ordering of other
/// variants. Otherwise, this could cause the lockfile to have a different
/// canonical ordering of distributions.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
struct GitSource {
    precise: GitSha,
    subdirectory: Option<String>,
    kind: GitSourceKind,
}

/// An error that occurs when a source string could not be parsed.
#[derive(Clone, Debug, Eq, PartialEq)]
enum GitSourceError {
    InvalidSha,
    MissingSha,
}

impl GitSource {
    /// Extracts a git source reference from the query pairs and the hash
    /// fragment in the given URL.
    ///
    /// This also removes the query pairs and hash fragment from the given
    /// URL in place.
    fn from_url(url: &mut Url) -> Result<GitSource, GitSourceError> {
        let mut kind = GitSourceKind::DefaultBranch;
        let mut subdirectory = None;
        for (key, val) in url.query_pairs() {
            match &*key {
                "tag" => kind = GitSourceKind::Tag(val.into_owned()),
                "branch" => kind = GitSourceKind::Branch(val.into_owned()),
                "rev" => kind = GitSourceKind::Rev(val.into_owned()),
                "subdirectory" => subdirectory = Some(val.into_owned()),
                _ => continue,
            };
        }
        let precise = GitSha::from_str(url.fragment().ok_or(GitSourceError::MissingSha)?)
            .map_err(|_| GitSourceError::InvalidSha)?;

        url.set_query(None);
        url.set_fragment(None);
        Ok(GitSource {
            precise,
            subdirectory,
            kind,
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
enum GitSourceKind {
    Tag(String),
    Branch(String),
    Rev(String),
    DefaultBranch,
}

/// Inspired by: <https://discuss.python.org/t/lock-files-again-but-this-time-w-sdists/46593>
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
struct SourceDistMetadata {
    /// A hash of the source distribution.
    hash: Option<Hash>,
    /// The size of the source distribution in bytes.
    ///
    /// This is only present for source distributions that come from registries.
    size: Option<u64>,
}

/// A URL or file path where the source dist that was
/// locked against was found. The location does not need to exist in the
/// future, so this should be treated as only a hint to where to look
/// and/or recording where the source dist file originally came from.
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum SourceDist {
    Url {
        url: UrlString,
        #[serde(flatten)]
        metadata: SourceDistMetadata,
    },
    Path {
        #[serde(deserialize_with = "deserialize_path_with_dot")]
        path: PathBuf,
        #[serde(flatten)]
        metadata: SourceDistMetadata,
    },
}

/// A [`PathBuf`], but we show `.` instead of an empty path.
fn deserialize_path_with_dot<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    let path = String::deserialize(deserializer)?;
    if path == "." {
        Ok(PathBuf::new())
    } else {
        Ok(PathBuf::from(path))
    }
}

impl SourceDist {
    fn filename(&self) -> Option<Cow<str>> {
        match self {
            SourceDist::Url { url, .. } => url.filename().ok(),
            SourceDist::Path { path, .. } => {
                path.file_name().map(|filename| filename.to_string_lossy())
            }
        }
    }

    fn url(&self) -> Option<&UrlString> {
        match &self {
            SourceDist::Url { url, .. } => Some(url),
            SourceDist::Path { .. } => None,
        }
    }

    fn hash(&self) -> Option<&Hash> {
        match &self {
            SourceDist::Url { metadata, .. } => metadata.hash.as_ref(),
            SourceDist::Path { metadata, .. } => metadata.hash.as_ref(),
        }
    }

    fn size(&self) -> Option<u64> {
        match &self {
            SourceDist::Url { metadata, .. } => metadata.size,
            SourceDist::Path { metadata, .. } => metadata.size,
        }
    }
}

impl SourceDist {
    /// Returns the TOML representation of this source distribution.
    fn to_toml(&self) -> anyhow::Result<InlineTable> {
        let mut table = InlineTable::new();
        match &self {
            SourceDist::Url { url, .. } => {
                table.insert("url", Value::from(url.as_ref()));
            }
            SourceDist::Path { path, .. } => {
                table.insert("path", Value::from(serialize_path_with_dot(path).as_ref()));
            }
        }
        if let Some(hash) = self.hash() {
            table.insert("hash", Value::from(hash.to_string()));
        }
        if let Some(size) = self.size() {
            table.insert("size", Value::from(i64::try_from(size)?));
        }
        Ok(table)
    }

    fn from_annotated_dist(
        id: &DistributionId,
        annotated_dist: &AnnotatedDist,
    ) -> Result<Option<SourceDist>, LockError> {
        match annotated_dist.dist {
            // TODO: Do we want to try to lock already-installed distributions?
            // Or should we return an error?
            ResolvedDist::Installed(_) => todo!(),
            ResolvedDist::Installable(ref dist) => {
                SourceDist::from_dist(id, dist, &annotated_dist.hashes)
            }
        }
    }

    fn from_dist(
        id: &DistributionId,
        dist: &Dist,
        hashes: &[HashDigest],
    ) -> Result<Option<SourceDist>, LockError> {
        match *dist {
            Dist::Built(BuiltDist::Registry(ref built_dist)) => {
                let Some(sdist) = built_dist.sdist.as_ref() else {
                    return Ok(None);
                };
                SourceDist::from_registry_dist(sdist).map(Some)
            }
            Dist::Built(_) => Ok(None),
            Dist::Source(ref source_dist) => SourceDist::from_source_dist(id, source_dist, hashes),
        }
    }

    fn from_source_dist(
        id: &DistributionId,
        source_dist: &distribution_types::SourceDist,
        hashes: &[HashDigest],
    ) -> Result<Option<SourceDist>, LockError> {
        match *source_dist {
            distribution_types::SourceDist::Registry(ref reg_dist) => {
                SourceDist::from_registry_dist(reg_dist).map(Some)
            }
            distribution_types::SourceDist::DirectUrl(ref direct_dist) => {
                SourceDist::from_direct_dist(id, direct_dist, hashes).map(Some)
            }
            // An actual sdist entry in the lockfile is only required when
            // it's from a registry or a direct URL. Otherwise, it's strictly
            // redundant with the information in all other kinds of `source`.
            distribution_types::SourceDist::Git(_)
            | distribution_types::SourceDist::Path(_)
            | distribution_types::SourceDist::Directory(_) => Ok(None),
        }
    }

    fn from_registry_dist(reg_dist: &RegistrySourceDist) -> Result<SourceDist, LockError> {
        let url = reg_dist
            .file
            .url
            .to_url_string()
            .map_err(LockErrorKind::InvalidFileUrl)
            .map_err(LockError::from)?;
        let hash = reg_dist.file.hashes.iter().max().cloned().map(Hash::from);
        let size = reg_dist.file.size;
        Ok(SourceDist::Url {
            url,
            metadata: SourceDistMetadata { hash, size },
        })
    }

    fn from_direct_dist(
        id: &DistributionId,
        direct_dist: &DirectUrlSourceDist,
        hashes: &[HashDigest],
    ) -> Result<SourceDist, LockError> {
        let Some(hash) = hashes.iter().max().cloned().map(Hash::from) else {
            let kind = LockErrorKind::Hash {
                id: id.clone(),
                artifact_type: "direct URL source distribution",
                expected: true,
            };
            return Err(kind.into());
        };
        Ok(SourceDist::Url {
            url: UrlString::from(direct_dist.url.to_url()),
            metadata: SourceDistMetadata {
                hash: Some(hash),
                size: None,
            },
        })
    }
}

impl From<GitReference> for GitSourceKind {
    fn from(value: GitReference) -> Self {
        match value {
            GitReference::Branch(branch) => GitSourceKind::Branch(branch.to_string()),
            GitReference::Tag(tag) => GitSourceKind::Tag(tag.to_string()),
            GitReference::ShortCommit(rev) => GitSourceKind::Rev(rev.to_string()),
            GitReference::BranchOrTag(rev) => GitSourceKind::Rev(rev.to_string()),
            GitReference::BranchOrTagOrCommit(rev) => GitSourceKind::Rev(rev.to_string()),
            GitReference::NamedRef(rev) => GitSourceKind::Rev(rev.to_string()),
            GitReference::FullCommit(rev) => GitSourceKind::Rev(rev.to_string()),
            GitReference::DefaultBranch => GitSourceKind::DefaultBranch,
        }
    }
}

impl From<GitSourceKind> for GitReference {
    fn from(value: GitSourceKind) -> Self {
        match value {
            GitSourceKind::Branch(branch) => GitReference::Branch(branch),
            GitSourceKind::Tag(tag) => GitReference::Tag(tag),
            GitSourceKind::Rev(rev) => GitReference::from_rev(rev),
            GitSourceKind::DefaultBranch => GitReference::DefaultBranch,
        }
    }
}

/// Construct the lockfile-compatible [`URL`] for a [`GitSourceDist`].
fn locked_git_url(git_dist: &GitSourceDist) -> Url {
    let mut url = git_dist.git.repository().clone();

    // Clear out any existing state.
    url.set_fragment(None);
    url.set_query(None);

    // Put the subdirectory in the query.
    if let Some(subdirectory) = git_dist.subdirectory.as_deref().and_then(Path::to_str) {
        url.query_pairs_mut()
            .append_pair("subdirectory", subdirectory);
    }

    // Put the requested reference in the query.
    match git_dist.git.reference() {
        GitReference::Branch(branch) => {
            url.query_pairs_mut()
                .append_pair("branch", branch.to_string().as_str());
        }
        GitReference::Tag(tag) => {
            url.query_pairs_mut()
                .append_pair("tag", tag.to_string().as_str());
        }
        GitReference::ShortCommit(rev)
        | GitReference::BranchOrTag(rev)
        | GitReference::BranchOrTagOrCommit(rev)
        | GitReference::NamedRef(rev)
        | GitReference::FullCommit(rev) => {
            url.query_pairs_mut()
                .append_pair("rev", rev.to_string().as_str());
        }
        GitReference::DefaultBranch => {}
    }

    // Put the precise commit in the fragment.
    url.set_fragment(
        git_dist
            .git
            .precise()
            .as_ref()
            .map(GitSha::to_string)
            .as_deref(),
    );

    url
}

/// Inspired by: <https://discuss.python.org/t/lock-files-again-but-this-time-w-sdists/46593>
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(try_from = "WheelWire")]
struct Wheel {
    /// A URL or file path (via `file://`) where the wheel that was locked
    /// against was found. The location does not need to exist in the future,
    /// so this should be treated as only a hint to where to look and/or
    /// recording where the wheel file originally came from.
    url: UrlString,
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
    /// The filename of the wheel.
    ///
    /// This isn't part of the wire format since it's redundant with the
    /// URL. But we do use it for various things, and thus compute it at
    /// deserialization time. Not being able to extract a wheel filename from a
    /// wheel URL is thus a deserialization error.
    filename: WheelFilename,
}

impl Wheel {
    fn from_annotated_dist(annotated_dist: &AnnotatedDist) -> Result<Vec<Wheel>, LockError> {
        match annotated_dist.dist {
            // TODO: Do we want to try to lock already-installed distributions?
            // Or should we return an error?
            ResolvedDist::Installed(_) => todo!(),
            ResolvedDist::Installable(ref dist) => Wheel::from_dist(dist, &annotated_dist.hashes),
        }
    }

    fn from_dist(dist: &Dist, hashes: &[HashDigest]) -> Result<Vec<Wheel>, LockError> {
        match *dist {
            Dist::Built(ref built_dist) => Wheel::from_built_dist(built_dist, hashes),
            Dist::Source(distribution_types::SourceDist::Registry(ref source_dist)) => source_dist
                .wheels
                .iter()
                .map(Wheel::from_registry_wheel)
                .collect(),
            Dist::Source(_) => Ok(vec![]),
        }
    }

    fn from_built_dist(
        built_dist: &BuiltDist,
        hashes: &[HashDigest],
    ) -> Result<Vec<Wheel>, LockError> {
        match *built_dist {
            BuiltDist::Registry(ref reg_dist) => Wheel::from_registry_dist(reg_dist),
            BuiltDist::DirectUrl(ref direct_dist) => {
                Ok(vec![Wheel::from_direct_dist(direct_dist, hashes)])
            }
            BuiltDist::Path(ref path_dist) => Ok(vec![Wheel::from_path_dist(path_dist, hashes)]),
        }
    }

    fn from_registry_dist(reg_dist: &RegistryBuiltDist) -> Result<Vec<Wheel>, LockError> {
        reg_dist
            .wheels
            .iter()
            .map(Wheel::from_registry_wheel)
            .collect()
    }

    fn from_registry_wheel(wheel: &RegistryBuiltWheel) -> Result<Wheel, LockError> {
        let filename = wheel.filename.clone();
        let url = wheel
            .file
            .url
            .to_url_string()
            .map_err(LockErrorKind::InvalidFileUrl)
            .map_err(LockError::from)?;
        let hash = wheel.file.hashes.iter().max().cloned().map(Hash::from);
        let size = wheel.file.size;
        Ok(Wheel {
            url,
            hash,
            size,
            filename,
        })
    }

    fn from_direct_dist(direct_dist: &DirectUrlBuiltDist, hashes: &[HashDigest]) -> Wheel {
        Wheel {
            url: direct_dist.url.to_url().into(),
            hash: hashes.iter().max().cloned().map(Hash::from),
            size: None,
            filename: direct_dist.filename.clone(),
        }
    }

    fn from_path_dist(path_dist: &PathBuiltDist, hashes: &[HashDigest]) -> Wheel {
        Wheel {
            url: path_dist.url.to_url().into(),
            hash: hashes.iter().max().cloned().map(Hash::from),
            size: None,
            filename: path_dist.filename.clone(),
        }
    }

    fn to_registry_dist(&self, url: &Url) -> RegistryBuiltWheel {
        let filename: WheelFilename = self.filename.clone();
        let file = Box::new(distribution_types::File {
            dist_info_metadata: false,
            filename: filename.to_string(),
            hashes: self.hash.iter().map(|h| h.0.clone()).collect(),
            requires_python: None,
            size: self.size,
            upload_time_utc_ms: None,
            url: FileLocation::AbsoluteUrl(self.url.clone()),
            yanked: None,
        });
        let index = IndexUrl::Url(VerbatimUrl::from_url(url.clone()));
        RegistryBuiltWheel {
            filename,
            file,
            index,
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
struct WheelWire {
    /// A URL or file path (via `file://`) where the wheel that was locked
    /// against was found. The location does not need to exist in the future,
    /// so this should be treated as only a hint to where to look and/or
    /// recording where the wheel file originally came from.
    url: UrlString,
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
}

impl Wheel {
    /// Returns the TOML representation of this wheel.
    fn to_toml(&self) -> anyhow::Result<InlineTable> {
        let mut table = InlineTable::new();
        table.insert("url", Value::from(self.url.to_string()));
        if let Some(ref hash) = self.hash {
            table.insert("hash", Value::from(hash.to_string()));
        }
        if let Some(size) = self.size {
            table.insert("size", Value::from(i64::try_from(size)?));
        }
        Ok(table)
    }
}

impl TryFrom<WheelWire> for Wheel {
    type Error = String;

    fn try_from(wire: WheelWire) -> Result<Wheel, String> {
        // Extract the filename segment from the URL.
        let filename = wire.url.filename().map_err(|err| err.to_string())?;

        // Parse the filename as a wheel filename.
        let filename = filename
            .parse::<WheelFilename>()
            .map_err(|err| format!("failed to parse `{filename}` as wheel filename: {err}"))?;

        Ok(Wheel {
            url: wire.url,
            hash: wire.hash,
            size: wire.size,
            filename,
        })
    }
}

/// A single dependency of a distribution in a lockfile.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
struct Dependency {
    distribution_id: DistributionId,
    extra: BTreeSet<ExtraName>,
    marker: Option<MarkerTree>,
}

impl Dependency {
    fn from_annotated_dist(
        annotated_dist: &AnnotatedDist,
        marker: Option<&MarkerTree>,
    ) -> Dependency {
        let distribution_id = DistributionId::from_annotated_dist(annotated_dist);
        let extra = annotated_dist.extra.iter().cloned().collect();
        let marker = marker.cloned();
        Dependency {
            distribution_id,
            extra,
            marker,
        }
    }

    /// Convert the [`Dependency`] to a [`Requirement`] that can be used for resolution.
    pub(crate) fn to_requirement(
        &self,
        workspace_root: &Path,
        extras: &mut FxHashMap<PackageName, Vec<ExtraName>>,
    ) -> Result<Option<Requirement>, LockError> {
        // Keep track of extras, these will be denormalized later.
        if !self.extra.is_empty() {
            extras
                .entry(self.distribution_id.name.clone())
                .or_default()
                .extend(self.extra.iter().cloned());
        }

        // Reconstruct the `RequirementSource` from the `Source`.
        let source = match &self.distribution_id.source {
            Source::Registry(_) => RequirementSource::Registry {
                // We don't store the version specifier that was originally used for resolution in
                // the lockfile, so this might be too restrictive. However, this is the only version
                // we have the metadata for, so if resolution fails we will need to fallback to a
                // clean resolve.
                specifier: VersionSpecifier::equals_version(self.distribution_id.version.clone())
                    .into(),
                index: None,
            },
            Source::Git(repository, git) => {
                let git_url =
                    uv_git::GitUrl::new(repository.clone(), GitReference::from(git.kind.clone()))
                        .with_precise(git.precise);

                let parsed_url = ParsedUrl::Git(ParsedGitUrl {
                    url: git_url.clone(),
                    subdirectory: git.subdirectory.as_ref().map(PathBuf::from),
                });
                RequirementSource::from_verbatim_parsed_url(parsed_url)
            }
            Source::Direct(url, direct) => {
                let parsed_url = ParsedUrl::Archive(ParsedArchiveUrl {
                    url: url.clone(),
                    subdirectory: direct.subdirectory.as_ref().map(PathBuf::from),
                });
                RequirementSource::from_verbatim_parsed_url(parsed_url)
            }
            Source::Path(ref path) => RequirementSource::Path {
                lock_path: path.clone(),
                install_path: workspace_root.join(path),
                url: verbatim_url(workspace_root.join(path), &self.distribution_id)?,
            },
            Source::Directory(ref path) => RequirementSource::Directory {
                editable: false,
                lock_path: path.clone(),
                install_path: workspace_root.join(path),
                url: verbatim_url(workspace_root.join(path), &self.distribution_id)?,
            },
            Source::Editable(ref path) => RequirementSource::Directory {
                editable: true,
                lock_path: path.clone(),
                install_path: workspace_root.join(path),
                url: verbatim_url(workspace_root.join(path), &self.distribution_id)?,
            },
        };

        let requirement = Requirement {
            name: self.distribution_id.name.clone(),
            marker: self.marker.clone(),
            origin: None,
            extras: Vec::new(),
            source,
        };

        Ok(Some(requirement))
    }

    /// Returns the TOML representation of this dependency.
    fn to_toml(&self, dist_count_by_name: &FxHashMap<PackageName, u64>) -> Table {
        let mut table = Table::new();
        self.distribution_id
            .to_toml(Some(dist_count_by_name), &mut table);
        if !self.extra.is_empty() {
            let extra_array = self
                .extra
                .iter()
                .map(ToString::to_string)
                .collect::<Array>();
            table.insert("extra", value(extra_array));
        }
        if let Some(ref marker) = self.marker {
            table.insert("marker", value(marker.to_string()));
        }

        table
    }
}

impl std::fmt::Display for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.extra.is_empty() {
            write!(
                f,
                "{}=={} @ {}",
                self.distribution_id.name,
                self.distribution_id.version,
                self.distribution_id.source
            )
        } else {
            write!(
                f,
                "{}[{}]=={} @ {}",
                self.distribution_id.name,
                self.extra.iter().join(","),
                self.distribution_id.version,
                self.distribution_id.source
            )
        }
    }
}

/// A single dependency of a distribution in a lockfile.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, serde::Deserialize)]
struct DependencyWire {
    #[serde(flatten)]
    distribution_id: DistributionIdForDependency,
    #[serde(default)]
    extra: BTreeSet<ExtraName>,
    marker: Option<MarkerTree>,
}

impl DependencyWire {
    fn unwire(
        self,
        unambiguous_dist_ids: &FxHashMap<PackageName, DistributionId>,
    ) -> Result<Dependency, LockError> {
        Ok(Dependency {
            distribution_id: self.distribution_id.unwire(unambiguous_dist_ids)?,
            extra: self.extra,
            marker: self.marker,
        })
    }
}

impl From<Dependency> for DependencyWire {
    fn from(dependency: Dependency) -> DependencyWire {
        DependencyWire {
            distribution_id: DistributionIdForDependency::from(dependency.distribution_id),
            extra: dependency.extra,
            marker: dependency.marker,
        }
    }
}

/// A single hash for a distribution artifact in a lockfile.
///
/// A hash is encoded as a single TOML string in the format
/// `{algorithm}:{digest}`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Hash(HashDigest);

impl From<HashDigest> for Hash {
    fn from(hd: HashDigest) -> Hash {
        Hash(hd)
    }
}

impl std::str::FromStr for Hash {
    type Err = HashParseError;

    fn from_str(s: &str) -> Result<Hash, HashParseError> {
        let (algorithm, digest) = s.split_once(':').ok_or(HashParseError(
            "expected '{algorithm}:{digest}', but found no ':' in hash digest",
        ))?;
        let algorithm = algorithm
            .parse()
            .map_err(|_| HashParseError("unrecognized hash algorithm"))?;
        Ok(Hash(HashDigest {
            algorithm,
            digest: digest.into(),
        }))
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}:{}", self.0.algorithm, self.0.digest)
    }
}

impl<'de> serde::Deserialize<'de> for Hash {
    fn deserialize<D>(d: D) -> Result<Hash, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let string = String::deserialize(d)?;
        string.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct LockError(Box<LockErrorKind>);

impl<E> From<E> for LockError
where
    LockErrorKind: From<E>,
{
    fn from(err: E) -> Self {
        LockError(Box::new(LockErrorKind::from(err)))
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
    /// An error that occurs when multiple distributions with the same
    /// ID were found.
    #[error("found duplicate distribution `{id}`")]
    DuplicateDistribution {
        /// The ID of the conflicting distributions.
        id: DistributionId,
    },
    /// An error that occurs when there are multiple dependencies for the
    /// same distribution that have identical identifiers.
    #[error("for distribution `{id}`, found duplicate dependency `{dependency}`")]
    DuplicateDependency {
        /// The ID of the distribution for which a duplicate dependency was
        /// found.
        id: DistributionId,
        /// The ID of the conflicting dependency.
        dependency: Dependency,
    },
    /// An error that occurs when there are multiple dependencies for the
    /// same distribution that have identical identifiers, as part of the
    /// that distribution's optional dependencies.
    #[error("for distribution `{id}[{extra}]`, found duplicate dependency `{dependency}`")]
    DuplicateOptionalDependency {
        /// The ID of the distribution for which a duplicate dependency was
        /// found.
        id: DistributionId,
        /// The name of the optional dependency group.
        extra: ExtraName,
        /// The ID of the conflicting dependency.
        dependency: Dependency,
    },
    /// An error that occurs when there are multiple dependencies for the
    /// same distribution that have identical identifiers, as part of the
    /// that distribution's development dependencies.
    #[error("for distribution `{id}:{group}`, found duplicate dependency `{dependency}`")]
    DuplicateDevDependency {
        /// The ID of the distribution for which a duplicate dependency was
        /// found.
        id: DistributionId,
        /// The name of the dev dependency group.
        group: GroupName,
        /// The ID of the conflicting dependency.
        dependency: Dependency,
    },
    /// An error that occurs when the URL to a file for a wheel or
    /// source dist could not be converted to a structured `url::Url`.
    #[error("failed to parse wheel or source distribution URL")]
    InvalidFileUrl(
        /// The underlying error that occurred. This includes the
        /// errant URL in its error message.
        #[source]
        ToUrlError,
    ),
    /// Failed to parse a git source URL.
    #[error("failed to parse source git URL")]
    InvalidGitSourceUrl(
        /// The underlying error that occurred. This includes the
        /// errant URL in the message.
        #[source]
        SourceParseError,
    ),
    /// An error that occurs when there's an unrecognized dependency.
    ///
    /// That is, a dependency for a distribution that isn't in the lockfile.
    #[error(
        "for distribution `{id}`, found dependency `{dependency}` with no locked distribution"
    )]
    UnrecognizedDependency {
        /// The ID of the distribution that has an unrecognized dependency.
        id: DistributionId,
        /// The ID of the dependency that doesn't have a corresponding distribution
        /// entry.
        dependency: Dependency,
    },
    /// An error that occurs when a hash is expected (or not) for a particular
    /// artifact, but one was not found (or was).
    #[error("since the distribution `{id}` comes from a {source} dependency, a hash was {expected} but one was not found for {artifact_type}", source = id.source.name(), expected = if *expected { "expected" } else { "not expected" })]
    Hash {
        /// The ID of the distribution that has a missing hash.
        id: DistributionId,
        /// The specific type of artifact, e.g., "source distribution"
        /// or "wheel".
        artifact_type: &'static str,
        /// When true, a hash is expected to be present.
        expected: bool,
    },
    /// An error that occurs when a distribution is included with an extra name,
    /// but no corresponding base distribution (i.e., without the extra) exists.
    #[error("found distribution `{id}` with extra `{extra}` but no base distribution")]
    MissingExtraBase {
        /// The ID of the distribution that has a missing base.
        id: DistributionId,
        /// The extra name that was found.
        extra: ExtraName,
    },
    /// An error that occurs when a distribution is included with a development
    /// dependency group, but no corresponding base distribution (i.e., without
    /// the group) exists.
    #[error("found distribution `{id}` with development dependency group `{group}` but no base distribution")]
    MissingDevBase {
        /// The ID of the distribution that has a missing base.
        id: DistributionId,
        /// The development dependency group that was found.
        group: GroupName,
    },
    /// An error that occurs from an invalid lockfile where a wheel comes from a non-wheel source
    /// such as a directory.
    #[error("wheels cannot come from {source_type} sources")]
    InvalidWheelSource {
        /// The ID of the distribution that has a missing base.
        id: DistributionId,
        /// The kind of the invalid source.
        source_type: &'static str,
    },
    /// An error that occurs when a distribution indicates that it is sourced from a registry, but
    /// is missing a URL.
    #[error("found registry distribution {id} without a valid URL")]
    MissingUrl {
        /// The ID of the distribution that is missing a URL.
        id: DistributionId,
    },
    /// An error that occurs when a distribution indicates that it is sourced from a registry, but
    /// is missing a filename.
    #[error("found registry distribution {id} without a valid filename")]
    MissingFilename {
        /// The ID of the distribution that is missing a filename.
        id: DistributionId,
    },
    /// An error that occurs when a distribution is included with neither wheels nor a source
    /// distribution.
    #[error("distribution {id} can't be installed because it doesn't have a source distribution or wheel for the current platform")]
    NeitherSourceDistNorWheel {
        /// The ID of the distribution that has a missing base.
        id: DistributionId,
    },
    /// An error that occurs when converting between URLs and paths.
    #[error("found dependency `{id}` with no locked distribution")]
    VerbatimUrl {
        /// The ID of the distribution that has a missing base.
        id: DistributionId,
        /// The inner error we forward.
        #[source]
        err: VerbatimUrlError,
    },
    /// An error that occurs when an ambiguous `distribution.dependency` is
    /// missing a `version` field.
    #[error(
        "dependency {name} has missing `version` \
         field but has more than one matching distribution"
    )]
    MissingDependencyVersion {
        /// The name of the dependency that is missing a `version` field.
        name: PackageName,
    },
    /// An error that occurs when an ambiguous `distribution.dependency` is
    /// missing a `source` field.
    #[error(
        "dependency {name} has missing `source` \
         field but has more than one matching distribution"
    )]
    MissingDependencySource {
        /// The name of the dependency that is missing a `source` field.
        name: PackageName,
    },
}

/// An error that occurs when a source string could not be parsed.
#[derive(Clone, Debug, thiserror::Error)]
enum SourceParseError {
    /// An error that occurs when the URL in the source is invalid.
    #[error("invalid URL in source `{given}`")]
    InvalidUrl {
        /// The source string given.
        given: String,
        /// The URL parse error.
        #[source]
        err: url::ParseError,
    },
    /// An error that occurs when a Git URL is missing a precise commit SHA.
    #[error("missing SHA in source `{given}`")]
    MissingSha {
        /// The source string given.
        given: String,
    },
    /// An error that occurs when a Git URL has an invalid SHA.
    #[error("invalid SHA in source `{given}`")]
    InvalidSha {
        /// The source string given.
        given: String,
    },
}

/// An error that occurs when a hash digest could not be parsed.
#[derive(Clone, Debug, Eq, PartialEq)]
struct HashParseError(&'static str);

impl std::error::Error for HashParseError {}

impl std::fmt::Display for HashParseError {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_dependency_source_unambiguous() {
        let data = r#"
version = 1

[[distribution]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution.dependencies]]
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

[[distribution]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution.dependencies]]
name = "a"
source =  { registry = "https://pypi.org/simple" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn missing_dependency_source_version_unambiguous() {
        let data = r#"
version = 1

[[distribution]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution.dependencies]]
name = "a"
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn missing_dependency_source_ambiguous() {
        let data = r#"
version = 1

[[distribution]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "a"
version = "0.1.1"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution.dependencies]]
name = "a"
version = "0.1.0"
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn missing_dependency_version_ambiguous() {
        let data = r#"
version = 1

[[distribution]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "a"
version = "0.1.1"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution.dependencies]]
name = "a"
source =  { registry = "https://pypi.org/simple" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn missing_dependency_source_version_ambiguous() {
        let data = r#"
version = 1

[[distribution]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "a"
version = "0.1.1"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution.dependencies]]
name = "a"
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn hash_optional_missing() {
        let data = r#"
version = 1

[[distribution]]
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

[[distribution]]
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

[[distribution]]
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

[[distribution]]
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

[[distribution]]
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

[[distribution]]
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

[[distribution]]
name = "anyio"
version = "4.3.0"
source = { editable = "path/to/dir" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }
}
