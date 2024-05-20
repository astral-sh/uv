// Temporarily allowed because this module is still in a state of flux
// as we build out universal locking.
#![allow(dead_code, unreachable_code, unused_variables)]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rustc_hash::FxHashMap;
use url::Url;

use distribution_filename::WheelFilename;
use distribution_types::{
    BuiltDist, DirectUrlBuiltDist, DirectUrlSourceDist, DirectorySourceDist, Dist, FileLocation,
    GitSourceDist, IndexUrl, ParsedArchiveUrl, ParsedGitUrl, PathBuiltDist, PathSourceDist,
    RegistryBuiltDist, RegistryBuiltWheel, RegistrySourceDist, RemoteSource, Resolution,
    ResolvedDist, ToUrlError,
};
use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, VerbatimUrl};
use platform_tags::{TagCompatibility, TagPriority, Tags};
use pypi_types::HashDigest;
use uv_git::{GitReference, GitSha};
use uv_normalize::PackageName;

use crate::resolution::AnnotatedDist;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(into = "LockWire", try_from = "LockWire")]
pub struct Lock {
    version: u32,
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
    /// have a corresponding locked distribution entry in the same lock file.
    by_id: FxHashMap<DistributionId, usize>,
}

impl Lock {
    pub(crate) fn new(distributions: Vec<Distribution>) -> Result<Lock, LockError> {
        let wire = LockWire {
            version: 1,
            distributions,
        };
        Lock::try_from(wire)
    }

    pub fn to_resolution(
        &self,
        marker_env: &MarkerEnvironment,
        tags: &Tags,
        root_name: &PackageName,
    ) -> Resolution {
        let root = self
            .find_by_name(root_name)
            // TODO: In the future, we should derive the root distribution
            // from the pyproject.toml, but I don't think the infrastructure
            // for that is in place yet. For now, we ask the caller to specify
            // the root package name explicitly, and we assume here that it is
            // correct.
            .expect("found too many distributions matching root")
            .expect("could not find root");
        let mut queue: VecDeque<&Distribution> = VecDeque::new();
        queue.push_back(root);

        let mut map = FxHashMap::default();
        while let Some(dist) = queue.pop_front() {
            for dep in &dist.dependencies {
                let dep_dist = self.find_by_id(&dep.id);
                queue.push_back(dep_dist);
            }
            let name = dist.id.name.clone();
            let resolved_dist = ResolvedDist::Installable(dist.to_dist(marker_env, tags));
            map.insert(name, resolved_dist);
        }
        Resolution::new(map)
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

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct LockWire {
    version: u32,
    #[serde(rename = "distribution")]
    distributions: Vec<Distribution>,
}

impl From<Lock> for LockWire {
    fn from(lock: Lock) -> LockWire {
        LockWire {
            version: lock.version,
            distributions: lock.distributions,
        }
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
            by_id,
        })
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub(crate) struct Distribution {
    #[serde(flatten)]
    pub(crate) id: DistributionId,
    #[serde(default)]
    pub(crate) marker: Option<String>,
    #[serde(default)]
    pub(crate) sdist: Option<SourceDist>,
    #[serde(default, rename = "wheel", skip_serializing_if = "Vec::is_empty")]
    pub(crate) wheels: Vec<Wheel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) dependencies: Vec<Dependency>,
}

impl Distribution {
    pub(crate) fn from_annotated_dist(
        annotated_dist: &AnnotatedDist,
    ) -> Result<Distribution, LockError> {
        let id = DistributionId::from_annotated_dist(annotated_dist);
        let wheels = Wheel::from_annotated_dist(annotated_dist)?;
        let sdist = SourceDist::from_annotated_dist(annotated_dist)?;
        Ok(Distribution {
            id,
            // TODO: Refactoring is needed to get the marker expressions for a
            // particular resolved dist to this point.
            marker: None,
            sdist,
            wheels,
            dependencies: vec![],
        })
    }

    pub(crate) fn add_dependency(&mut self, annotated_dist: &AnnotatedDist) {
        self.dependencies
            .push(Dependency::from_annotated_dist(annotated_dist));
    }

    /// Convert the [`Distribution`] to a [`Dist`] that can be used in installation.
    fn to_dist(&self, _marker_env: &MarkerEnvironment, tags: &Tags) -> Dist {
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
                        subdirectory: None,
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
}

#[derive(
    Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
pub(crate) struct DistributionId {
    pub(crate) name: PackageName,
    pub(crate) version: Version,
    pub(crate) source: Source,
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
pub(crate) struct Source {
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
            BuiltDist::Registry(ref reg_dist) => Source::from_registry_built_dist2(reg_dist),
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

    fn from_registry_built_dist(reg_dist: &RegistryBuiltWheel) -> Source {
        Source::from_index_url(&reg_dist.index)
    }

    fn from_registry_built_dist2(reg_dist: &RegistryBuiltDist) -> Source {
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
            kind: SourceKind::Directory,
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
            name => Err(SourceParseError::unrecognized_source_name(s, name)),
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}+{}", self.kind.name(), self.url)
    }
}

impl serde::Serialize for Source {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        s.collect_str(self)
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
pub(crate) enum SourceKind {
    Registry,
    Git(GitSource),
    Direct(DirectSource),
    Path,
    Directory,
}

impl SourceKind {
    fn name(&self) -> &str {
        match *self {
            SourceKind::Registry => "registry",
            SourceKind::Git(_) => "git",
            SourceKind::Direct(_) => "direct",
            SourceKind::Path => "path",
            SourceKind::Directory => "directory",
        }
    }

    /// Returns true when this source kind requires a hash.
    ///
    /// When this returns false, it also implies that a hash should
    /// _not_ be present.
    fn requires_hash(&self) -> bool {
        match *self {
            SourceKind::Registry | SourceKind::Direct(_) | SourceKind::Path => true,
            SourceKind::Git(_) | SourceKind::Directory => false,
        }
    }
}

#[derive(
    Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
pub(crate) struct DirectSource {
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
pub(crate) struct GitSource {
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

#[derive(
    Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
enum GitSourceKind {
    Tag(String),
    Branch(String),
    Rev(String),
    DefaultBranch,
}

/// Inspired by: <https://discuss.python.org/t/lock-files-again-but-this-time-w-sdists/46593>
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub(crate) struct SourceDist {
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
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(into = "WheelWire", try_from = "WheelWire")]
pub(crate) struct Wheel {
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

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
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

impl From<Wheel> for WheelWire {
    fn from(wheel: Wheel) -> WheelWire {
        WheelWire {
            url: wheel.url,
            hash: wheel.hash,
            size: wheel.size,
        }
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
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, serde::Deserialize, serde::Serialize)]
pub(crate) struct Dependency {
    #[serde(flatten)]
    id: DistributionId,
}

impl Dependency {
    fn from_annotated_dist(annotated_dist: &AnnotatedDist) -> Dependency {
        let id = DistributionId::from_annotated_dist(annotated_dist);
        Dependency { id }
    }
}

/// A single hash for a distribution artifact in a lock file.
///
/// A hash is encoded as a single TOML string in the format
/// `{algorithm}:{digest}`.
#[derive(Clone, Debug)]
pub(crate) struct Hash(HashDigest);

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

impl serde::Serialize for Hash {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        s.collect_str(self)
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
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self.kind {
            LockErrorKind::DuplicateDistribution { .. } => None,
            LockErrorKind::DuplicateDependency { .. } => None,
            LockErrorKind::InvalidFileUrl { ref err } => Some(err),
            LockErrorKind::UnrecognizedDependency { ref err } => Some(err),
            LockErrorKind::Hash { .. } => None,
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
}

/// An error that occurs when there's an unrecognized dependency.
///
/// That is, a dependency for a distribution that isn't in the lock file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnrecognizedDependencyError {
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
pub(crate) struct SourceParseError {
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
pub(crate) struct HashParseError(&'static str);

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

[[distribution.wheel]]
url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl"
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

[[distribution.wheel]]
url = "file:///foo/bar/anyio-4.3.0-py3-none-any.whl"
hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8"
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }
}
