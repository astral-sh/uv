// Temporarily allowed because this module is still in a state of flux
// as we build out universal locking.
#![allow(dead_code, unreachable_code, unused_variables)]

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use either::Either;
use indexmap::IndexMap;
use rustc_hash::FxHashMap;
use toml_edit::{value, Array, ArrayOfTables, InlineTable, Item, Table, Value};
use url::Url;

use cache_key::RepositoryUrl;
use distribution_filename::WheelFilename;
use distribution_types::{
    BuiltDist, DirectUrlBuiltDist, DirectUrlSourceDist, DirectorySourceDist, Dist, FileLocation,
    GitSourceDist, IndexUrl, PathBuiltDist, PathSourceDist, RegistryBuiltDist, RegistryBuiltWheel,
    RegistrySourceDist, RemoteSource, Resolution, ResolvedDist, ToUrlError,
};
use pep440_rs::{Version, VersionSpecifiers};
use pep508_rs::{MarkerEnvironment, MarkerTree, VerbatimUrl};
use platform_tags::{TagCompatibility, TagPriority, Tags};
use pypi_types::{HashDigest, ParsedArchiveUrl, ParsedGitUrl};
use uv_configuration::ExtrasSpecification;
use uv_git::{GitReference, GitSha, RepositoryReference, ResolvedRepositoryReference};
use uv_normalize::{ExtraName, PackageName};

use crate::resolution::AnnotatedDist;
use crate::{lock, ResolutionGraph};

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(try_from = "LockWire")]
pub struct Lock {
    version: u32,
    distributions: Vec<Distribution>,
    /// The range of supported Python versions.
    requires_python: Option<VersionSpecifiers>,
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
        let mut locked_dists = IndexMap::with_capacity(graph.petgraph.node_count());

        // Lock all base packages.
        for node_index in graph.petgraph.node_indices() {
            let dist = &graph.petgraph[node_index];
            if dist.extra.is_some() {
                continue;
            }

            let mut locked_dist = lock::Distribution::from_annotated_dist(dist)?;
            for neighbor in graph.petgraph.neighbors(node_index) {
                let dependency_dist = &graph.petgraph[neighbor];
                locked_dist.add_dependency(dependency_dist);
            }
            if let Some(locked_dist) = locked_dists.insert(locked_dist.id.clone(), locked_dist) {
                return Err(LockError::duplicate_distribution(locked_dist.id));
            }
        }

        // Lock all extras.
        for node_index in graph.petgraph.node_indices() {
            let dist = &graph.petgraph[node_index];
            if let Some(extra) = dist.extra.as_ref() {
                let id = lock::DistributionId::from_annotated_dist(dist);
                let Some(locked_dist) = locked_dists.get_mut(&id) else {
                    return Err(LockError::missing_base(id, extra.clone()));
                };
                for neighbor in graph.petgraph.neighbors(node_index) {
                    let dependency_dist = &graph.petgraph[neighbor];
                    locked_dist.add_optional_dependency(extra.clone(), dependency_dist);
                }
            }
        }

        let distributions = locked_dists.into_values().collect();
        let requires_python = graph.requires_python.clone();
        let lock = Self::new(distributions, requires_python)?;
        Ok(lock)
    }

    /// Initialize a [`Lock`] from a list of [`Distribution`] entries.
    fn new(
        distributions: Vec<Distribution>,
        requires_python: Option<VersionSpecifiers>,
    ) -> Result<Self, LockError> {
        let wire = LockWire {
            version: 1,
            distributions,
            requires_python,
        };
        Self::try_from(wire)
    }

    /// Returns the [`Distribution`] entries in this lock.
    pub fn distributions(&self) -> &[Distribution] {
        &self.distributions
    }

    /// Returns the supported Python version range for the lockfile, if present.
    pub fn requires_python(&self) -> Option<&VersionSpecifiers> {
        self.requires_python.as_ref()
    }

    /// Convert the [`Lock`] to a [`Resolution`] using the given marker environment, tags, and root.
    pub fn to_resolution(
        &self,
        marker_env: &MarkerEnvironment,
        tags: &Tags,
        root_name: &PackageName,
        extras: &ExtrasSpecification,
    ) -> Resolution {
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
            let deps = if let Some(extra) = extra {
                Either::Left(dist.optional_dependencies.get(extra).into_iter().flatten())
            } else {
                Either::Right(dist.dependencies.iter())
            };
            for dep in deps {
                let dep_dist = self.find_by_id(&dep.id);
                if dep_dist
                    .marker
                    .as_ref()
                    .map_or(true, |marker| marker.evaluate(marker_env, &[]))
                {
                    let dep_extra = dep.extra.as_ref();
                    queue.push_back((dep_dist, dep_extra));
                }
            }
            let name = dist.id.name.clone();
            let resolved_dist = ResolvedDist::Installable(dist.to_dist(tags));
            map.insert(name, resolved_dist);
        }
        let diagnostics = vec![];
        Resolution::new(map, diagnostics)
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
    distributions: Vec<Distribution>,
    #[serde(rename = "requires-python")]
    requires_python: Option<VersionSpecifiers>,
}

impl From<Lock> for LockWire {
    fn from(lock: Lock) -> LockWire {
        LockWire {
            version: lock.version,
            distributions: lock.distributions,
            requires_python: lock.requires_python,
        }
    }
}

impl Lock {
    /// Returns the TOML representation of this lock file.
    pub fn to_toml(&self) -> Result<String> {
        // We construct a TOML document manually instead of going through Serde to enable
        // the use of inline tables.
        let mut doc = toml_edit::DocumentMut::new();
        doc.insert("version", value(i64::from(self.version)));

        if let Some(ref requires_python) = self.requires_python {
            doc.insert("requires-python", value(requires_python.to_string()));
        }

        let mut distributions = ArrayOfTables::new();
        for dist in &self.distributions {
            let mut table = Table::new();

            table.insert("name", value(dist.id.name.to_string()));
            table.insert("version", value(dist.id.version.to_string()));
            table.insert("source", value(dist.id.source.to_string()));

            if let Some(ref marker) = dist.marker {
                table.insert("marker", value(marker.to_string()));
            }

            if let Some(ref sdist) = dist.sdist {
                table.insert("sdist", value(sdist.to_toml()?));
            }

            if !dist.dependencies.is_empty() {
                let deps = dist
                    .dependencies
                    .iter()
                    .map(Dependency::to_toml)
                    .collect::<ArrayOfTables>();
                table.insert("dependencies", Item::ArrayOfTables(deps));
            }

            if !dist.optional_dependencies.is_empty() {
                let mut optional_deps = Table::new();
                for (extra, deps) in &dist.optional_dependencies {
                    let deps = deps
                        .iter()
                        .map(Dependency::to_toml)
                        .collect::<ArrayOfTables>();
                    optional_deps.insert(extra.as_ref(), Item::ArrayOfTables(deps));
                }
                table.insert("optional-dependencies", Item::Table(optional_deps));
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
                    .collect::<Result<Array>>()?;
                table.insert("wheels", value(wheels));
            }

            distributions.push(table);
        }

        doc.insert("distribution", Item::ArrayOfTables(distributions));
        Ok(doc.to_string())
    }
}

impl TryFrom<LockWire> for Lock {
    type Error = LockError;

    fn try_from(mut wire: LockWire) -> Result<Lock, LockError> {
        // Put all dependencies for each distribution in a canonical order and
        // check for duplicates.
        for dist in &mut wire.distributions {
            dist.dependencies.sort();
            for windows in dist.dependencies.windows(2) {
                let (dep1, dep2) = (&windows[0], &windows[1]);
                if dep1.id == dep2.id {
                    return Err(LockError::duplicate_dependency(
                        dist.id.clone(),
                        dep1.id.clone(),
                    ));
                }
            }
        }
        wire.distributions
            .sort_by(|dist1, dist2| dist1.id.cmp(&dist2.id));

        // Check for duplicate distribution IDs and also build up the map for
        // distributions keyed by their ID.
        let mut by_id = FxHashMap::default();
        for (i, dist) in wire.distributions.iter().enumerate() {
            if by_id.insert(dist.id.clone(), i).is_some() {
                return Err(LockError::duplicate_distribution(dist.id.clone()));
            }
        }
        // Check that every dependency has an entry in `by_id`. If any don't,
        // it implies we somehow have a dependency with no corresponding locked
        // distribution.
        for dist in &wire.distributions {
            for dep in &dist.dependencies {
                if !by_id.contains_key(&dep.id) {
                    return Err(LockError::unrecognized_dependency(
                        dist.id.clone(),
                        dep.id.clone(),
                    ));
                }
            }
            // Also check that our sources are consistent with whether we have
            // hashes or not.
            let requires_hash = dist.id.source.kind.requires_hash();
            if let Some(ref sdist) = dist.sdist {
                if requires_hash != sdist.hash.is_some() {
                    return Err(LockError::hash(
                        dist.id.clone(),
                        "source distribution",
                        requires_hash,
                    ));
                }
            }
            for wheel in &dist.wheels {
                if requires_hash != wheel.hash.is_some() {
                    return Err(LockError::hash(dist.id.clone(), "wheel", requires_hash));
                }
            }
        }
        Ok(Lock {
            version: wire.version,
            distributions: wire.distributions,
            requires_python: wire.requires_python,
            by_id,
        })
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Distribution {
    #[serde(flatten)]
    pub(crate) id: DistributionId,
    #[serde(default)]
    marker: Option<MarkerTree>,
    #[serde(default)]
    sdist: Option<SourceDist>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    wheels: Vec<Wheel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dependencies: Vec<Dependency>,
    #[serde(
        default,
        skip_serializing_if = "IndexMap::is_empty",
        rename = "optional-dependencies"
    )]
    optional_dependencies: IndexMap<ExtraName, Vec<Dependency>>,
}

impl Distribution {
    fn from_annotated_dist(annotated_dist: &AnnotatedDist) -> Result<Self, LockError> {
        let id = DistributionId::from_annotated_dist(annotated_dist);
        let mut marker = annotated_dist.marker.clone();
        // Markers can be combined in an unpredictable order, so normalize them
        // such that the lock file output is consistent and deterministic.
        if let Some(ref mut marker) = marker {
            marker.normalize();
        }
        let sdist = SourceDist::from_annotated_dist(annotated_dist)?;
        let wheels = Wheel::from_annotated_dist(annotated_dist)?;
        Ok(Distribution {
            id,
            marker,
            sdist,
            wheels,
            dependencies: vec![],
            optional_dependencies: IndexMap::default(),
        })
    }

    /// Add the [`AnnotatedDist`] as a dependency of the [`Distribution`].
    fn add_dependency(&mut self, annotated_dist: &AnnotatedDist) {
        self.dependencies
            .push(Dependency::from_annotated_dist(annotated_dist));
    }

    /// Add the [`AnnotatedDist`] as an optional dependency of the [`Distribution`].
    fn add_optional_dependency(&mut self, extra: ExtraName, annotated_dist: &AnnotatedDist) {
        let dep = Dependency::from_annotated_dist(annotated_dist);
        self.optional_dependencies
            .entry(extra)
            .or_default()
            .push(dep);
    }

    /// Convert the [`Distribution`] to a [`Dist`] that can be used in installation.
    fn to_dist(&self, tags: &Tags) -> Dist {
        if let Some(best_wheel_index) = self.find_best_wheel(tags) {
            return match &self.id.source.kind {
                SourceKind::Registry => {
                    let wheels = self
                        .wheels
                        .iter()
                        .map(|wheel| wheel.to_registry_dist(&self.id.source))
                        .collect();
                    let reg_built_dist = RegistryBuiltDist {
                        wheels,
                        best_wheel_index,
                        sdist: None,
                    };
                    Dist::Built(BuiltDist::Registry(reg_built_dist))
                }
                SourceKind::Path => {
                    let filename: WheelFilename = self.wheels[best_wheel_index].filename.clone();
                    let path_dist = PathBuiltDist {
                        filename,
                        url: VerbatimUrl::from_url(self.id.source.url.clone()),
                        path: self.id.source.url.to_file_path().unwrap(),
                    };
                    let built_dist = BuiltDist::Path(path_dist);
                    Dist::Built(built_dist)
                }
                SourceKind::Direct(direct) => {
                    let filename: WheelFilename = self.wheels[best_wheel_index].filename.clone();
                    let url = Url::from(ParsedArchiveUrl {
                        url: self.id.source.url.clone(),
                        subdirectory: direct.subdirectory.as_ref().map(PathBuf::from),
                    });
                    let direct_dist = DirectUrlBuiltDist {
                        filename,
                        location: self.id.source.url.clone(),
                        url: VerbatimUrl::from_url(url),
                    };
                    let built_dist = BuiltDist::DirectUrl(direct_dist);
                    Dist::Built(built_dist)
                }
                SourceKind::Git(_) => {
                    unreachable!("Wheels cannot come from Git sources")
                }
                SourceKind::Directory => {
                    unreachable!("Wheels cannot come from directory sources")
                }
                SourceKind::Editable => {
                    unreachable!("Wheels cannot come from editable sources")
                }
            };
        }

        if let Some(sdist) = &self.sdist {
            return match &self.id.source.kind {
                SourceKind::Path => {
                    let path_dist = PathSourceDist {
                        name: self.id.name.clone(),
                        url: VerbatimUrl::from_url(self.id.source.url.clone()),
                        path: self.id.source.url.to_file_path().unwrap(),
                    };
                    let source_dist = distribution_types::SourceDist::Path(path_dist);
                    Dist::Source(source_dist)
                }
                SourceKind::Directory => {
                    let dir_dist = DirectorySourceDist {
                        name: self.id.name.clone(),
                        url: VerbatimUrl::from_url(self.id.source.url.clone()),
                        path: self.id.source.url.to_file_path().unwrap(),
                        editable: false,
                    };
                    let source_dist = distribution_types::SourceDist::Directory(dir_dist);
                    Dist::Source(source_dist)
                }
                SourceKind::Editable => {
                    let dir_dist = DirectorySourceDist {
                        name: self.id.name.clone(),
                        url: VerbatimUrl::from_url(self.id.source.url.clone()),
                        path: self.id.source.url.to_file_path().unwrap(),
                        editable: true,
                    };
                    let source_dist = distribution_types::SourceDist::Directory(dir_dist);
                    Dist::Source(source_dist)
                }
                SourceKind::Git(git) => {
                    // Reconstruct the `GitUrl` from the `GitSource`.
                    let git_url = uv_git::GitUrl::new(
                        self.id.source.url.clone(),
                        GitReference::from(git.kind.clone()),
                    )
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
                    Dist::Source(source_dist)
                }
                SourceKind::Direct(direct) => {
                    let url = Url::from(ParsedArchiveUrl {
                        url: self.id.source.url.clone(),
                        subdirectory: direct.subdirectory.as_ref().map(PathBuf::from),
                    });
                    let direct_dist = DirectUrlSourceDist {
                        name: self.id.name.clone(),
                        location: self.id.source.url.clone(),
                        subdirectory: direct.subdirectory.as_ref().map(PathBuf::from),
                        url: VerbatimUrl::from_url(url),
                    };
                    let source_dist = distribution_types::SourceDist::DirectUrl(direct_dist);
                    Dist::Source(source_dist)
                }
                SourceKind::Registry => {
                    let file = Box::new(distribution_types::File {
                        dist_info_metadata: false,
                        filename: sdist.url.filename().unwrap().to_string(),
                        hashes: vec![],
                        requires_python: None,
                        size: sdist.size,
                        upload_time_utc_ms: None,
                        url: FileLocation::AbsoluteUrl(sdist.url.to_string()),
                        yanked: None,
                    });
                    let index = IndexUrl::Url(VerbatimUrl::from_url(self.id.source.url.clone()));
                    let reg_dist = RegistrySourceDist {
                        name: self.id.name.clone(),
                        version: self.id.version.clone(),
                        file,
                        index,
                        wheels: vec![],
                    };
                    let source_dist = distribution_types::SourceDist::Registry(reg_dist);
                    Dist::Source(source_dist)
                }
            };
        }

        // TODO: Convert this to a deserialization error.
        panic!("invalid lock distribution")
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
        match &self.id.source.kind {
            SourceKind::Git(git) => Some(ResolvedRepositoryReference {
                reference: RepositoryReference {
                    url: RepositoryUrl::new(&self.id.source.url),
                    reference: GitReference::from(git.kind.clone()),
                },
                sha: git.precise,
            }),
            _ => None,
        }
    }
}

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
        write!(f, "{} {} {}", self.name, self.version, self.source)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
struct Source {
    kind: SourceKind,
    url: Url,
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
        Source {
            kind: SourceKind::Direct(DirectSource { subdirectory: None }),
            url: direct_dist.url.to_url(),
        }
    }

    fn from_direct_source_dist(direct_dist: &DirectUrlSourceDist) -> Source {
        Source {
            kind: SourceKind::Direct(DirectSource {
                subdirectory: direct_dist
                    .subdirectory
                    .as_deref()
                    .and_then(Path::to_str)
                    .map(ToString::to_string),
            }),
            url: direct_dist.url.to_url(),
        }
    }

    fn from_path_built_dist(path_dist: &PathBuiltDist) -> Source {
        Source {
            kind: SourceKind::Path,
            url: path_dist.url.to_url(),
        }
    }

    fn from_path_source_dist(path_dist: &PathSourceDist) -> Source {
        Source {
            kind: SourceKind::Path,
            url: path_dist.url.to_url(),
        }
    }

    fn from_directory_source_dist(directory_dist: &DirectorySourceDist) -> Source {
        Source {
            kind: if directory_dist.editable {
                SourceKind::Editable
            } else {
                SourceKind::Directory
            },
            url: directory_dist.url.to_url(),
        }
    }

    fn from_index_url(index_url: &IndexUrl) -> Source {
        match *index_url {
            IndexUrl::Pypi(ref verbatim_url) => Source {
                kind: SourceKind::Registry,
                url: verbatim_url.to_url(),
            },
            IndexUrl::Url(ref verbatim_url) => Source {
                kind: SourceKind::Registry,
                url: verbatim_url.to_url(),
            },
            IndexUrl::Path(ref verbatim_url) => Source {
                kind: SourceKind::Path,
                url: verbatim_url.to_url(),
            },
        }
    }

    fn from_git_dist(git_dist: &GitSourceDist) -> Source {
        Source {
            kind: SourceKind::Git(GitSource {
                kind: GitSourceKind::from(git_dist.git.reference().clone()),
                precise: git_dist.git.precise().expect("precise commit"),
                subdirectory: git_dist
                    .subdirectory
                    .as_deref()
                    .and_then(Path::to_str)
                    .map(ToString::to_string),
            }),
            url: locked_git_url(git_dist),
        }
    }
}

impl std::str::FromStr for Source {
    type Err = SourceParseError;

    fn from_str(s: &str) -> Result<Source, SourceParseError> {
        let (kind, url) = s
            .split_once('+')
            .ok_or_else(|| SourceParseError::no_plus(s))?;
        let mut url = Url::parse(url).map_err(|err| SourceParseError::invalid_url(s, err))?;
        match kind {
            "registry" => Ok(Source {
                kind: SourceKind::Registry,
                url,
            }),
            "git" => Ok(Source {
                kind: SourceKind::Git(GitSource::from_url(&mut url).map_err(|err| match err {
                    GitSourceError::InvalidSha => SourceParseError::invalid_sha(s),
                    GitSourceError::MissingSha => SourceParseError::missing_sha(s),
                })?),
                url,
            }),
            "direct" => Ok(Source {
                kind: SourceKind::Direct(DirectSource::from_url(&mut url)),
                url,
            }),
            "path" => Ok(Source {
                kind: SourceKind::Path,
                url,
            }),
            "directory" => Ok(Source {
                kind: SourceKind::Directory,
                url,
            }),
            "editable" => Ok(Source {
                kind: SourceKind::Editable,
                url,
            }),
            name => Err(SourceParseError::unrecognized_source_name(s, name)),
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}+{}", self.kind.name(), self.url)
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

/// NOTE: Care should be taken when adding variants to this enum. Namely, new
/// variants should be added without changing the relative ordering of other
/// variants. Otherwise, this could cause the lock file to have a different
/// canonical ordering of distributions.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
enum SourceKind {
    Registry,
    Git(GitSource),
    Direct(DirectSource),
    Path,
    Directory,
    Editable,
}

impl SourceKind {
    fn name(&self) -> &str {
        match *self {
            SourceKind::Registry => "registry",
            SourceKind::Git(_) => "git",
            SourceKind::Direct(_) => "direct",
            SourceKind::Path => "path",
            SourceKind::Directory => "directory",
            SourceKind::Editable => "editable",
        }
    }

    /// Returns true when this source kind requires a hash.
    ///
    /// When this returns false, it also implies that a hash should
    /// _not_ be present.
    fn requires_hash(&self) -> bool {
        match *self {
            SourceKind::Registry | SourceKind::Direct(_) | SourceKind::Path => true,
            SourceKind::Git(_) | SourceKind::Directory | SourceKind::Editable => false,
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
struct SourceDist {
    /// A URL or file path (via `file://`) where the source dist that was
    /// locked against was found. The location does not need to exist in the
    /// future, so this should be treated as only a hint to where to look
    /// and/or recording where the source dist file originally came from.
    url: Url,
    /// A hash of the source distribution.
    ///
    /// This is only present for source distributions that come from registries
    /// and direct URLs. Source distributions from git or path dependencies do
    /// not have hashes associated with them.
    hash: Option<Hash>,
    /// The size of the source distribution in bytes.
    ///
    /// This is only present for source distributions that come from registries.
    size: Option<u64>,
}

impl SourceDist {
    /// Returns the TOML representation of this source distribution.
    fn to_toml(&self) -> Result<InlineTable> {
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

    fn from_annotated_dist(
        annotated_dist: &AnnotatedDist,
    ) -> Result<Option<SourceDist>, LockError> {
        match annotated_dist.dist {
            // TODO: Do we want to try to lock already-installed distributions?
            // Or should we return an error?
            ResolvedDist::Installed(_) => todo!(),
            ResolvedDist::Installable(ref dist) => {
                SourceDist::from_dist(dist, &annotated_dist.hashes)
            }
        }
    }

    fn from_dist(dist: &Dist, hashes: &[HashDigest]) -> Result<Option<SourceDist>, LockError> {
        match *dist {
            Dist::Built(BuiltDist::Registry(ref built_dist)) => {
                let Some(sdist) = built_dist.sdist.as_ref() else {
                    return Ok(None);
                };
                SourceDist::from_registry_dist(sdist).map(Some)
            }
            Dist::Built(_) => Ok(None),
            Dist::Source(ref source_dist) => {
                SourceDist::from_source_dist(source_dist, hashes).map(Some)
            }
        }
    }

    fn from_source_dist(
        source_dist: &distribution_types::SourceDist,
        hashes: &[HashDigest],
    ) -> Result<SourceDist, LockError> {
        match *source_dist {
            distribution_types::SourceDist::Registry(ref reg_dist) => {
                SourceDist::from_registry_dist(reg_dist)
            }
            distribution_types::SourceDist::DirectUrl(ref direct_dist) => {
                Ok(SourceDist::from_direct_dist(direct_dist, hashes))
            }
            distribution_types::SourceDist::Git(ref git_dist) => {
                Ok(SourceDist::from_git_dist(git_dist, hashes))
            }
            distribution_types::SourceDist::Path(ref path_dist) => {
                Ok(SourceDist::from_path_dist(path_dist, hashes))
            }
            distribution_types::SourceDist::Directory(ref directory_dist) => {
                Ok(SourceDist::from_directory_dist(directory_dist, hashes))
            }
        }
    }

    fn from_registry_dist(reg_dist: &RegistrySourceDist) -> Result<SourceDist, LockError> {
        let url = reg_dist
            .file
            .url
            .to_url()
            .map_err(LockError::invalid_file_url)?;
        let hash = reg_dist.file.hashes.first().cloned().map(Hash::from);
        let size = reg_dist.file.size;
        Ok(SourceDist { url, hash, size })
    }

    fn from_direct_dist(direct_dist: &DirectUrlSourceDist, hashes: &[HashDigest]) -> SourceDist {
        SourceDist {
            url: direct_dist.url.to_url(),
            hash: hashes.first().cloned().map(Hash::from),
            size: None,
        }
    }

    fn from_git_dist(git_dist: &GitSourceDist, hashes: &[HashDigest]) -> SourceDist {
        SourceDist {
            url: locked_git_url(git_dist),
            hash: hashes.first().cloned().map(Hash::from),
            size: None,
        }
    }

    fn from_path_dist(path_dist: &PathSourceDist, hashes: &[HashDigest]) -> SourceDist {
        SourceDist {
            url: path_dist.url.to_url(),
            hash: hashes.first().cloned().map(Hash::from),
            size: None,
        }
    }

    fn from_directory_dist(
        directory_dist: &DirectorySourceDist,
        hashes: &[HashDigest],
    ) -> SourceDist {
        SourceDist {
            url: directory_dist.url.to_url(),
            hash: hashes.first().cloned().map(Hash::from),
            size: None,
        }
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
            .map_err(LockError::invalid_file_url)?;
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

    fn to_registry_dist(&self, source: &Source) -> RegistryBuiltWheel {
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
        let index = IndexUrl::Url(VerbatimUrl::from_url(source.url.clone()));
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
    fn to_toml(&self) -> Result<InlineTable> {
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
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, serde::Deserialize)]
struct Dependency {
    #[serde(flatten)]
    id: DistributionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    extra: Option<ExtraName>,
}

impl Dependency {
    fn from_annotated_dist(annotated_dist: &AnnotatedDist) -> Dependency {
        let id = DistributionId::from_annotated_dist(annotated_dist);
        let extra = annotated_dist.extra.clone();
        Dependency { id, extra }
    }

    /// Returns the TOML representation of this dependency.
    fn to_toml(&self) -> Table {
        let mut table = Table::new();
        table.insert("name", value(self.id.name.to_string()));
        table.insert("version", value(self.id.version.to_string()));
        table.insert("source", value(self.id.source.to_string()));
        if let Some(ref extra) = self.extra {
            table.insert("extra", value(extra.to_string()));
        }

        table
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

/// An error that occurs when generating a `Lock` data structure.
///
/// These errors are sometimes the result of possible programming bugs.
/// For example, if there are two or more duplicative distributions given
/// to `Lock::new`, then an error is returned. It's likely that the fault
/// is with the caller somewhere in such cases.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockError {
    kind: Box<LockErrorKind>,
}

impl LockError {
    fn duplicate_distribution(id: DistributionId) -> LockError {
        let kind = LockErrorKind::DuplicateDistribution { id };
        LockError {
            kind: Box::new(kind),
        }
    }

    fn duplicate_dependency(id: DistributionId, dependency_id: DistributionId) -> LockError {
        let kind = LockErrorKind::DuplicateDependency { id, dependency_id };
        LockError {
            kind: Box::new(kind),
        }
    }

    fn invalid_file_url(err: ToUrlError) -> LockError {
        let kind = LockErrorKind::InvalidFileUrl { err };
        LockError {
            kind: Box::new(kind),
        }
    }

    fn unrecognized_dependency(id: DistributionId, dependency_id: DistributionId) -> LockError {
        let err = UnrecognizedDependencyError { id, dependency_id };
        let kind = LockErrorKind::UnrecognizedDependency { err };
        LockError {
            kind: Box::new(kind),
        }
    }

    fn hash(id: DistributionId, artifact_type: &'static str, expected: bool) -> LockError {
        let kind = LockErrorKind::Hash {
            id,
            artifact_type,
            expected,
        };
        LockError {
            kind: Box::new(kind),
        }
    }

    fn missing_base(id: DistributionId, extra: ExtraName) -> LockError {
        let kind = LockErrorKind::MissingBase { id, extra };
        LockError {
            kind: Box::new(kind),
        }
    }
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self.kind {
            LockErrorKind::DuplicateDistribution { .. } => None,
            LockErrorKind::DuplicateDependency { .. } => None,
            LockErrorKind::InvalidFileUrl { ref err } => Some(err),
            LockErrorKind::UnrecognizedDependency { ref err } => Some(err),
            LockErrorKind::Hash { .. } => None,
            LockErrorKind::MissingBase { .. } => None,
        }
    }
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self.kind {
            LockErrorKind::DuplicateDistribution { ref id } => {
                write!(f, "found duplicate distribution `{id}`")
            }
            LockErrorKind::DuplicateDependency {
                ref id,
                ref dependency_id,
            } => {
                write!(
                    f,
                    "for distribution `{id}`, found duplicate dependency `{dependency_id}`"
                )
            }
            LockErrorKind::InvalidFileUrl { .. } => {
                write!(f, "failed to parse wheel or source dist URL")
            }
            LockErrorKind::UnrecognizedDependency { .. } => {
                write!(f, "found unrecognized dependency")
            }
            LockErrorKind::Hash {
                ref id,
                artifact_type,
                expected: true,
            } => {
                write!(
                    f,
                    "since the distribution `{id}` comes from a {source} dependency, \
                     a hash was expected but one was not found for {artifact_type}",
                    source = id.source.kind.name(),
                )
            }
            LockErrorKind::Hash {
                ref id,
                artifact_type,
                expected: false,
            } => {
                write!(
                    f,
                    "since the distribution `{id}` comes from a {source} dependency, \
                     a hash was not expected but one was found for {artifact_type}",
                    source = id.source.kind.name(),
                )
            }
            LockErrorKind::MissingBase { ref id, ref extra } => {
                write!(
                    f,
                    "found distribution `{id}` with extra `{extra}` but no base distribution",
                )
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LockErrorKind {
    /// An error that occurs when multiple distributions with the same
    /// ID were found.
    DuplicateDistribution {
        /// The ID of the conflicting distributions.
        id: DistributionId,
    },
    /// An error that occurs when there are multiple dependencies for the
    /// same distribution that have identical identifiers.
    DuplicateDependency {
        /// The ID of the distribution for which a duplicate dependency was
        /// found.
        id: DistributionId,
        /// The ID of the conflicting dependency.
        dependency_id: DistributionId,
    },
    /// An error that occurs when the URL to a file for a wheel or
    /// source dist could not be converted to a structured `url::Url`.
    InvalidFileUrl {
        /// The underlying error that occurred. This includes the
        /// errant URL in its error message.
        err: ToUrlError,
    },
    /// An error that occurs when the caller provides a distribution with a
    /// dependency that doesn't correspond to any other distribution in the
    /// lock file.
    UnrecognizedDependency {
        /// The actual error.
        err: UnrecognizedDependencyError,
    },
    /// An error that occurs when a hash is expected (or not) for a particular
    /// artifact, but one was not found (or was).
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
    MissingBase {
        /// The ID of the distribution that has a missing base.
        id: DistributionId,
        /// The extra name that was found.
        extra: ExtraName,
    },
}

/// An error that occurs when there's an unrecognized dependency.
///
/// That is, a dependency for a distribution that isn't in the lock file.
#[derive(Clone, Debug, Eq, PartialEq)]
struct UnrecognizedDependencyError {
    /// The ID of the distribution that has an unrecognized dependency.
    id: DistributionId,
    /// The ID of the dependency that doesn't have a corresponding distribution
    /// entry.
    dependency_id: DistributionId,
}

impl std::error::Error for UnrecognizedDependencyError {}

impl std::fmt::Display for UnrecognizedDependencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let UnrecognizedDependencyError {
            ref id,
            ref dependency_id,
        } = *self;
        write!(
            f,
            "found dependency `{dependency_id}` for `{id}` with no locked distribution"
        )
    }
}

/// An error that occurs when a source string could not be parsed.
#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceParseError {
    given: String,
    kind: SourceParseErrorKind,
}

impl SourceParseError {
    fn no_plus(given: &str) -> SourceParseError {
        let given = given.to_string();
        let kind = SourceParseErrorKind::NoPlus;
        SourceParseError { given, kind }
    }

    fn unrecognized_source_name(given: &str, name: &str) -> SourceParseError {
        let given = given.to_string();
        let kind = SourceParseErrorKind::UnrecognizedSourceName {
            name: name.to_string(),
        };
        SourceParseError { given, kind }
    }

    fn invalid_url(given: &str, err: url::ParseError) -> SourceParseError {
        let given = given.to_string();
        let kind = SourceParseErrorKind::InvalidUrl { err };
        SourceParseError { given, kind }
    }

    fn missing_sha(given: &str) -> SourceParseError {
        let given = given.to_string();
        let kind = SourceParseErrorKind::MissingSha;
        SourceParseError { given, kind }
    }

    fn invalid_sha(given: &str) -> SourceParseError {
        let given = given.to_string();
        let kind = SourceParseErrorKind::InvalidSha;
        SourceParseError { given, kind }
    }
}

impl std::error::Error for SourceParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind {
            SourceParseErrorKind::NoPlus
            | SourceParseErrorKind::UnrecognizedSourceName { .. }
            | SourceParseErrorKind::MissingSha
            | SourceParseErrorKind::InvalidSha => None,
            SourceParseErrorKind::InvalidUrl { ref err } => Some(err),
        }
    }
}

impl std::fmt::Display for SourceParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let given = &self.given;
        match self.kind {
            SourceParseErrorKind::NoPlus => write!(f, "could not find `+` in source `{given}`"),
            SourceParseErrorKind::UnrecognizedSourceName { ref name } => {
                write!(f, "unrecognized name `{name}` in source `{given}`")
            }
            SourceParseErrorKind::InvalidUrl { .. } => write!(f, "invalid URL in source `{given}`"),
            SourceParseErrorKind::MissingSha => write!(f, "missing SHA in source `{given}`"),
            SourceParseErrorKind::InvalidSha => write!(f, "invalid SHA in source `{given}`"),
        }
    }
}

/// The kind of error that can occur when parsing a source string.
#[derive(Clone, Debug, Eq, PartialEq)]
enum SourceParseErrorKind {
    /// An error that occurs when no '+' could be found.
    NoPlus,
    /// An error that occurs when the source name was unrecognized.
    UnrecognizedSourceName {
        /// The unrecognized name.
        name: String,
    },
    /// An error that occurs when the URL in the source is invalid.
    InvalidUrl {
        /// The URL parse error.
        err: url::ParseError,
    },
    /// An error that occurs when a Git URL is missing a precise commit SHA.
    MissingSha,
    /// An error that occurs when a Git URL has an invalid SHA.
    InvalidSha,
}

/// An error that occurs when a hash digest could not be parsed.
#[derive(Clone, Debug, Eq, PartialEq)]
struct HashParseError(&'static str);

impl std::error::Error for HashParseError {}

impl std::fmt::Display for HashParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
