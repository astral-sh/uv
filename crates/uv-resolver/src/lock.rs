use std::borrow::Cow;
use std::collections::{BTreeMap, VecDeque};
use std::fmt::{Debug, Display};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use either::Either;
use path_slash::PathExt;
use petgraph::visit::EdgeRef;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Deserializer};
use toml_edit::{value, Array, ArrayOfTables, InlineTable, Item, Table, Value};
use url::Url;

use cache_key::RepositoryUrl;
use distribution_filename::WheelFilename;
use distribution_types::{
    BuiltDist, DirectUrlBuiltDist, DirectUrlSourceDist, DirectorySourceDist, Dist, FileLocation,
    GitSourceDist, IndexUrl, PathBuiltDist, PathSourceDist, RegistryBuiltDist, RegistryBuiltWheel,
    RegistrySourceDist, RemoteSource, Resolution, ResolvedDist, ToUrlError,
};
use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, MarkerTree, VerbatimUrl, VerbatimUrlError};
use platform_tags::{TagCompatibility, TagPriority, Tags};
use pypi_types::{HashDigest, ParsedArchiveUrl, ParsedGitUrl};
use uv_configuration::ExtrasSpecification;
use uv_git::{GitReference, GitSha, RepositoryReference, ResolvedRepositoryReference};
use uv_normalize::{ExtraName, GroupName, PackageName};

use crate::resolution::{AnnotatedDist, ResolutionGraphNode};
use crate::{RequiresPython, ResolutionGraph};

/// The current version of the lock file format.
const VERSION: u32 = 1;

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(try_from = "LockWire")]
pub struct Lock {
    version: u32,
    distributions: Vec<Distribution>,
    /// The range of supported Python versions.
    requires_python: Option<RequiresPython>,
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
    /// have a corresponding locked distribution entry in the same lock file.
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
                let mut locked_dist = Distribution::from_annotated_dist(dist)?;
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
        let lock = Self::new(VERSION, distributions, requires_python)?;
        Ok(lock)
    }

    /// Initialize a [`Lock`] from a list of [`Distribution`] entries.
    fn new(
        version: u32,
        mut distributions: Vec<Distribution>,
        requires_python: Option<RequiresPython>,
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
            dist.dependencies.retain(|dep| {
                dep.extra.as_ref().map_or(true, |extra| {
                    extras_by_id
                        .get(&dep.distribution_id)
                        .is_some_and(|extras| extras.contains(extra))
                })
            });

            for dependencies in dist.optional_dependencies.values_mut() {
                dependencies.retain(|dep| {
                    dep.extra.as_ref().map_or(true, |extra| {
                        extras_by_id
                            .get(&dep.distribution_id)
                            .is_some_and(|extras| extras.contains(extra))
                    })
                });
            }

            for dependencies in dist.dev_dependencies.values_mut() {
                dependencies.retain(|dep| {
                    dep.extra.as_ref().map_or(true, |extra| {
                        extras_by_id
                            .get(&dep.distribution_id)
                            .is_some_and(|extras| extras.contains(extra))
                    })
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
            let requires_hash = dist.id.source.requires_hash();
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
        Ok(Lock {
            version,
            distributions,
            requires_python,
            by_id,
        })
    }

    /// Returns the [`Distribution`] entries in this lock.
    pub fn distributions(&self) -> &[Distribution] {
        &self.distributions
    }

    /// Returns the supported Python version range for the lockfile, if present.
    pub fn requires_python(&self) -> Option<&RequiresPython> {
        self.requires_python.as_ref()
    }

    /// Convert the [`Lock`] to a [`Resolution`] using the given marker environment, tags, and root.
    pub fn to_resolution(
        &self,
        workspace_root: &Path,
        marker_env: &MarkerEnvironment,
        tags: &Tags,
        root_name: &PackageName,
        extras: &ExtrasSpecification,
        dev: &[GroupName],
    ) -> Result<Resolution, LockError> {
        let mut queue: VecDeque<(&Distribution, Option<&ExtraName>)> = VecDeque::new();

        // Add the root distribution to the queue.
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

        let mut map = BTreeMap::default();
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
                    let dep_extra = dep.extra.as_ref();
                    queue.push_back((dep_dist, dep_extra));
                }
            }
            let name = dist.id.name.clone();
            let resolved_dist = ResolvedDist::Installable(dist.to_dist(workspace_root, tags)?);
            map.insert(name, resolved_dist);
        }
        let diagnostics = vec![];
        Ok(Resolution::new(map, diagnostics))
    }

    /// Returns the TOML representation of this lock file.
    pub fn to_toml(&self) -> anyhow::Result<String> {
        // We construct a TOML document manually instead of going through Serde to enable
        // the use of inline tables.
        let mut doc = toml_edit::DocumentMut::new();
        doc.insert("version", value(i64::from(self.version)));

        if let Some(ref requires_python) = self.requires_python {
            doc.insert("requires-python", value(requires_python.to_string()));
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
            let mut table = Table::new();

            table.insert("name", value(dist.id.name.to_string()));
            table.insert("version", value(dist.id.version.to_string()));
            table.insert("source", value(dist.id.source.to_string()));

            if let Some(ref sdist) = dist.sdist {
                table.insert("sdist", value(sdist.to_toml()?));
            }

            if !dist.dependencies.is_empty() {
                let deps = dist
                    .dependencies
                    .iter()
                    .map(|dep| dep.to_toml(&dist_count_by_name))
                    .collect::<ArrayOfTables>();
                table.insert("dependencies", Item::ArrayOfTables(deps));
            }

            if !dist.optional_dependencies.is_empty() {
                let mut optional_deps = Table::new();
                for (extra, deps) in &dist.optional_dependencies {
                    let deps = deps
                        .iter()
                        .map(|dep| dep.to_toml(&dist_count_by_name))
                        .collect::<ArrayOfTables>();
                    optional_deps.insert(extra.as_ref(), Item::ArrayOfTables(deps));
                }
                table.insert("optional-dependencies", Item::Table(optional_deps));
            }

            if !dist.dev_dependencies.is_empty() {
                let mut dev_dependencies = Table::new();
                for (extra, deps) in &dist.dev_dependencies {
                    let deps = deps
                        .iter()
                        .map(|dep| dep.to_toml(&dist_count_by_name))
                        .collect::<ArrayOfTables>();
                    dev_dependencies.insert(extra.as_ref(), Item::ArrayOfTables(deps));
                }
                table.insert("dev-dependencies", Item::Table(dev_dependencies));
            }

            if !dist.wheels.is_empty() {
                let wheels = dist
                    .wheels
                    .iter()
                    .enumerate()
                    .map(|(i, wheel)| {
                        let mut table = wheel.to_toml()?;

                        if dist.wheels.len() > 1 {
                            // Indent each wheel on a new line.
                            table.decor_mut().set_prefix("\n\t");
                            if i == dist.wheels.len() - 1 {
                                table.decor_mut().set_suffix("\n");
                            }
                        }

                        Ok(table)
                    })
                    .collect::<anyhow::Result<Array>>()?;
                table.insert("wheels", value(wheels));
            }

            distributions.push(table);
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
}

#[derive(Clone, Debug, serde::Deserialize)]
struct LockWire {
    version: u32,
    #[serde(rename = "distribution")]
    distributions: Vec<DistributionWire>,
    #[serde(rename = "requires-python")]
    requires_python: Option<RequiresPython>,
}

impl From<Lock> for LockWire {
    fn from(lock: Lock) -> LockWire {
        LockWire {
            version: lock.version,
            distributions: lock
                .distributions
                .into_iter()
                .map(DistributionWire::from)
                .collect(),
            requires_python: lock.requires_python,
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
        Lock::new(wire.version, distributions, wire.requires_python)
    }
}

#[derive(Clone, Debug)]
pub struct Distribution {
    pub(crate) id: DistributionId,
    sdist: Option<SourceDist>,
    wheels: Vec<Wheel>,
    dependencies: Vec<Dependency>,
    optional_dependencies: BTreeMap<ExtraName, Vec<Dependency>>,
    dev_dependencies: BTreeMap<GroupName, Vec<Dependency>>,
}

impl Distribution {
    fn from_annotated_dist(annotated_dist: &AnnotatedDist) -> Result<Self, LockError> {
        let id = DistributionId::from_annotated_dist(annotated_dist);
        let sdist = SourceDist::from_annotated_dist(&id, annotated_dist)?;
        let wheels = Wheel::from_annotated_dist(annotated_dist)?;
        Ok(Distribution {
            id,
            sdist,
            wheels,
            dependencies: vec![],
            optional_dependencies: BTreeMap::default(),
            dev_dependencies: BTreeMap::default(),
        })
    }

    /// Add the [`AnnotatedDist`] as a dependency of the [`Distribution`].
    fn add_dependency(&mut self, annotated_dist: &AnnotatedDist, marker: Option<&MarkerTree>) {
        self.dependencies
            .push(Dependency::from_annotated_dist(annotated_dist, marker));
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
                        url: VerbatimUrl::from_path(workspace_root.join(path)).map_err(|err| {
                            LockErrorKind::VerbatimUrl {
                                id: self.id.clone(),
                                err,
                            }
                        })?,
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

        match &self.id.source {
            Source::Path(path) => {
                let path_dist = PathSourceDist {
                    name: self.id.name.clone(),
                    url: VerbatimUrl::from_path(workspace_root.join(path)).map_err(|err| {
                        LockErrorKind::VerbatimUrl {
                            id: self.id.clone(),
                            err,
                        }
                    })?,
                    install_path: workspace_root.join(path),
                    lock_path: path.clone(),
                };
                let source_dist = distribution_types::SourceDist::Path(path_dist);
                return Ok(Dist::Source(source_dist));
            }
            Source::Directory(path) => {
                let dir_dist = DirectorySourceDist {
                    name: self.id.name.clone(),
                    url: VerbatimUrl::from_path(workspace_root.join(path)).map_err(|err| {
                        LockErrorKind::VerbatimUrl {
                            id: self.id.clone(),
                            err,
                        }
                    })?,
                    install_path: workspace_root.join(path),
                    lock_path: path.clone(),
                    editable: false,
                };
                let source_dist = distribution_types::SourceDist::Directory(dir_dist);
                return Ok(Dist::Source(source_dist));
            }
            Source::Editable(path) => {
                let dir_dist = DirectorySourceDist {
                    name: self.id.name.clone(),
                    url: VerbatimUrl::from_path(workspace_root.join(path)).map_err(|err| {
                        LockErrorKind::VerbatimUrl {
                            id: self.id.clone(),
                            err,
                        }
                    })?,
                    install_path: workspace_root.join(path),
                    lock_path: path.clone(),
                    editable: true,
                };
                let source_dist = distribution_types::SourceDist::Directory(dir_dist);
                return Ok(Dist::Source(source_dist));
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
                let source_dist = distribution_types::SourceDist::Git(git_dist);
                return Ok(Dist::Source(source_dist));
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
                let source_dist = distribution_types::SourceDist::DirectUrl(direct_dist);
                return Ok(Dist::Source(source_dist));
            }
            Source::Registry(url) => {
                if let Some(ref sdist) = self.sdist {
                    let file_url = sdist.url().ok_or_else(|| LockErrorKind::MissingUrl {
                        id: self.id.clone(),
                    })?;
                    let filename =
                        sdist
                            .filename()
                            .ok_or_else(|| LockErrorKind::MissingFilename {
                                id: self.id.clone(),
                            })?;
                    let file = Box::new(distribution_types::File {
                        dist_info_metadata: false,
                        filename: filename.to_string(),
                        hashes: vec![],
                        requires_python: None,
                        size: sdist.size(),
                        upload_time_utc_ms: None,
                        url: FileLocation::AbsoluteUrl(file_url.to_string()),
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
                    let source_dist = distribution_types::SourceDist::Registry(reg_dist);
                    return Ok(Dist::Source(source_dist));
                }
            }
        }

        Err(LockErrorKind::NeitherSourceDistNorWheel {
            id: self.id.clone(),
        }
        .into())
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

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct DistributionWire {
    #[serde(flatten)]
    id: DistributionId,
    #[serde(default)]
    sdist: Option<SourceDist>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    wheels: Vec<Wheel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dependencies: Vec<DependencyWire>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    optional_dependencies: BTreeMap<ExtraName, Vec<DependencyWire>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
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
/// variants. Otherwise, this could cause the lock file to have a different
/// canonical ordering of distributions.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
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
/// that the lock file contains portable paths.
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
}

impl std::str::FromStr for Source {
    type Err = SourceParseError;

    fn from_str(s: &str) -> Result<Source, SourceParseError> {
        let (kind, url_or_path) = s.split_once('+').ok_or_else(|| SourceParseError::NoPlus {
            given: s.to_string(),
        })?;
        match kind {
            "registry" => {
                let url = Url::parse(url_or_path).map_err(|err| SourceParseError::InvalidUrl {
                    given: s.to_string(),
                    err,
                })?;
                Ok(Source::Registry(url))
            }
            "git" => {
                let mut url =
                    Url::parse(url_or_path).map_err(|err| SourceParseError::InvalidUrl {
                        given: s.to_string(),
                        err,
                    })?;
                let git_source = GitSource::from_url(&mut url).map_err(|err| match err {
                    GitSourceError::InvalidSha => SourceParseError::InvalidSha {
                        given: s.to_string(),
                    },
                    GitSourceError::MissingSha => SourceParseError::MissingSha {
                        given: s.to_string(),
                    },
                })?;
                Ok(Source::Git(url, git_source))
            }
            "direct" => {
                let mut url =
                    Url::parse(url_or_path).map_err(|err| SourceParseError::InvalidUrl {
                        given: s.to_string(),
                        err,
                    })?;
                let direct_source = DirectSource::from_url(&mut url);
                Ok(Source::Direct(url, direct_source))
            }
            "path" => Ok(Source::Path(PathBuf::from(url_or_path))),
            "directory" => Ok(Source::Directory(PathBuf::from(url_or_path))),
            "editable" => Ok(Source::Editable(PathBuf::from(url_or_path))),
            name => Err(SourceParseError::UnrecognizedSourceName {
                given: s.to_string(),
                name: name.to_string(),
            }),
        }
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

impl<'de> serde::Deserialize<'de> for Source {
    fn deserialize<D>(d: D) -> Result<Source, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let string = String::deserialize(d)?;
        string.parse().map_err(serde::de::Error::custom)
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

    /// Returns true when this source kind requires a hash.
    ///
    /// When this returns false, it also implies that a hash should
    /// _not_ be present.
    fn requires_hash(&self) -> bool {
        match *self {
            Self::Registry(..) | Self::Direct(..) | Self::Path(..) => true,
            Self::Git(..) | Self::Directory(..) | Self::Editable(..) => false,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
struct DirectSource {
    subdirectory: Option<String>,
}

impl DirectSource {
    /// Extracts a direct source reference from the query pairs in the given URL.
    ///
    /// This also removes the query pairs and hash fragment from the given
    /// URL in place.
    fn from_url(url: &mut Url) -> DirectSource {
        let subdirectory = url.query_pairs().find_map(|(key, val)| {
            if key == "subdirectory" {
                Some(val.into_owned())
            } else {
                None
            }
        });
        url.set_query(None);
        url.set_fragment(None);
        DirectSource { subdirectory }
    }
}

/// NOTE: Care should be taken when adding variants to this enum. Namely, new
/// variants should be added without changing the relative ordering of other
/// variants. Otherwise, this could cause the lock file to have a different
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
#[derive(Clone, Debug, serde::Deserialize)]
struct SourceDistMetadata {
    /// A hash of the source distribution.
    hash: Hash,
    /// The size of the source distribution in bytes.
    ///
    /// This is only present for source distributions that come from registries.
    size: Option<u64>,
}

/// A URL or file path where the source dist that was
/// locked against was found. The location does not need to exist in the
/// future, so this should be treated as only a hint to where to look
/// and/or recording where the source dist file originally came from.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(untagged)]
enum SourceDist {
    Url {
        url: Url,
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

    fn url(&self) -> Option<&Url> {
        match &self {
            SourceDist::Url { url, .. } => Some(url),
            SourceDist::Path { .. } => None,
        }
    }

    fn hash(&self) -> &Hash {
        match &self {
            SourceDist::Url { metadata, .. } => &metadata.hash,
            SourceDist::Path { metadata, .. } => &metadata.hash,
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
                table.insert("url", Value::from(url.as_str()));
            }
            SourceDist::Path { path, .. } => {
                table.insert("path", Value::from(serialize_path_with_dot(path).as_ref()));
            }
        }
        table.insert("hash", Value::from(self.hash().to_string()));
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
                SourceDist::from_registry_dist(id, sdist).map(Some)
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
                SourceDist::from_registry_dist(id, reg_dist).map(Some)
            }
            distribution_types::SourceDist::DirectUrl(ref direct_dist) => {
                SourceDist::from_direct_dist(id, direct_dist, hashes).map(Some)
            }
            // An actual sdist entry in the lock file is only required when
            // it's from a registry or a direct URL. Otherwise, it's strictly
            // redundant with the information in all other kinds of `source`.
            distribution_types::SourceDist::Git(_)
            | distribution_types::SourceDist::Path(_)
            | distribution_types::SourceDist::Directory(_) => Ok(None),
        }
    }

    fn from_registry_dist(
        id: &DistributionId,
        reg_dist: &RegistrySourceDist,
    ) -> Result<SourceDist, LockError> {
        let url = reg_dist
            .file
            .url
            .to_url()
            .map_err(LockErrorKind::InvalidFileUrl)
            .map_err(LockError::from)?;
        let Some(hash) = reg_dist.file.hashes.first().cloned().map(Hash::from) else {
            let kind = LockErrorKind::Hash {
                id: id.clone(),
                artifact_type: "registry source distribution",
                expected: true,
            };
            return Err(kind.into());
        };
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
        let Some(hash) = hashes.first().cloned().map(Hash::from) else {
            let kind = LockErrorKind::Hash {
                id: id.clone(),
                artifact_type: "direct URL source distribution",
                expected: true,
            };
            return Err(kind.into());
        };
        Ok(SourceDist::Url {
            url: direct_dist.url.to_url(),
            metadata: SourceDistMetadata { hash, size: None },
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
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(try_from = "WheelWire")]
struct Wheel {
    /// A URL or file path (via `file://`) where the wheel that was locked
    /// against was found. The location does not need to exist in the future,
    /// so this should be treated as only a hint to where to look and/or
    /// recording where the wheel file originally came from.
    url: Url,
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
            .to_url()
            .map_err(LockErrorKind::InvalidFileUrl)
            .map_err(LockError::from)?;
        let hash = wheel.file.hashes.first().cloned().map(Hash::from);
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
            url: direct_dist.url.to_url(),
            hash: hashes.first().cloned().map(Hash::from),
            size: None,
            filename: direct_dist.filename.clone(),
        }
    }

    fn from_path_dist(path_dist: &PathBuiltDist, hashes: &[HashDigest]) -> Wheel {
        Wheel {
            url: path_dist.url.to_url(),
            hash: hashes.first().cloned().map(Hash::from),
            size: None,
            filename: path_dist.filename.clone(),
        }
    }

    fn to_registry_dist(&self, url: &Url) -> RegistryBuiltWheel {
        let filename: WheelFilename = self.filename.clone();
        let file = Box::new(distribution_types::File {
            dist_info_metadata: false,
            filename: filename.to_string(),
            hashes: vec![],
            requires_python: None,
            size: self.size,
            upload_time_utc_ms: None,
            url: FileLocation::AbsoluteUrl(self.url.to_string()),
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
    url: Url,
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

/// A single dependency of a distribution in a lock file.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
struct Dependency {
    distribution_id: DistributionId,
    extra: Option<ExtraName>,
    marker: Option<MarkerTree>,
}

impl Dependency {
    fn from_annotated_dist(
        annotated_dist: &AnnotatedDist,
        marker: Option<&MarkerTree>,
    ) -> Dependency {
        let distribution_id = DistributionId::from_annotated_dist(annotated_dist);
        let extra = annotated_dist.extra.clone();
        let marker = marker.cloned().and_then(crate::marker::normalize);
        Dependency {
            distribution_id,
            extra,
            marker,
        }
    }

    /// Returns the TOML representation of this dependency.
    fn to_toml(&self, dist_count_by_name: &FxHashMap<PackageName, u64>) -> Table {
        let count = dist_count_by_name
            .get(&self.distribution_id.name)
            .copied()
            .expect("all dependencies have a corresponding distribution");
        let mut table = Table::new();
        table.insert("name", value(self.distribution_id.name.to_string()));
        if count > 1 {
            table.insert("version", value(self.distribution_id.version.to_string()));
            table.insert("source", value(self.distribution_id.source.to_string()));
        }
        if let Some(ref extra) = self.extra {
            table.insert("extra", value(extra.to_string()));
        }
        if let Some(ref marker) = self.marker {
            table.insert("marker", value(marker.to_string()));
        }

        table
    }
}

impl std::fmt::Display for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(ref extra) = self.extra {
            write!(
                f,
                "{}[{}]=={} @ {}",
                self.distribution_id.name,
                extra,
                self.distribution_id.version,
                self.distribution_id.source
            )
        } else {
            write!(
                f,
                "{}=={} @ {}",
                self.distribution_id.name,
                self.distribution_id.version,
                self.distribution_id.source
            )
        }
    }
}

/// A single dependency of a distribution in a lock file.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, serde::Deserialize)]
struct DependencyWire {
    #[serde(flatten)]
    distribution_id: DistributionIdForDependency,
    #[serde(skip_serializing_if = "Option::is_none")]
    extra: Option<ExtraName>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

/// A single hash for a distribution artifact in a lock file.
///
/// A hash is encoded as a single TOML string in the format
/// `{algorithm}:{digest}`.
#[derive(Clone, Debug)]
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
    /// An error that occurs when there's an unrecognized dependency.
    ///
    /// That is, a dependency for a distribution that isn't in the lock file.
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
    #[error("found distribution {id} with neither wheels nor source distribution")]
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
    /// An error that occurs when no '+' could be found.
    #[error("could not find `+` in source `{given}`")]
    NoPlus {
        /// The source string given.
        given: String,
    },
    /// An error that occurs when the source name was unrecognized.
    #[error("unrecognized name `{name}` in source `{given}`")]
    UnrecognizedSourceName {
        /// The source string given.
        given: String,
        /// The unrecognized name.
        name: String,
    },
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
source = "registry+https://pypi.org/simple"
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "b"
version = "0.1.0"
source = "registry+https://pypi.org/simple"
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
source = "registry+https://pypi.org/simple"
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "b"
version = "0.1.0"
source = "registry+https://pypi.org/simple"
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution.dependencies]]
name = "a"
source = "registry+https://pypi.org/simple"
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
source = "registry+https://pypi.org/simple"
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "b"
version = "0.1.0"
source = "registry+https://pypi.org/simple"
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
source = "registry+https://pypi.org/simple"
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "a"
version = "0.1.1"
source = "registry+https://pypi.org/simple"
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "b"
version = "0.1.0"
source = "registry+https://pypi.org/simple"
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
source = "registry+https://pypi.org/simple"
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "a"
version = "0.1.1"
source = "registry+https://pypi.org/simple"
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "b"
version = "0.1.0"
source = "registry+https://pypi.org/simple"
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution.dependencies]]
name = "a"
source = "registry+https://pypi.org/simple"
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
source = "registry+https://pypi.org/simple"
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "a"
version = "0.1.1"
source = "registry+https://pypi.org/simple"
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution]]
name = "b"
version = "0.1.0"
source = "registry+https://pypi.org/simple"
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[distribution.dependencies]]
name = "a"
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
source = "registry+https://pypi.org/simple"
wheels = [{ url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl" }]
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
source = "path+file:///foo/bar"
wheels = [{ url = "file:///foo/bar/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8" }]
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }
}
