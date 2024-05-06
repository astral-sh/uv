// Temporarily allowed because this module is still in a state of flux
// as we build out universal locking.
#![allow(dead_code, unreachable_code, unused_variables)]

use std::collections::VecDeque;

use distribution_filename::WheelFilename;
use distribution_types::{
    BuiltDist, DirectUrlBuiltDist, DirectUrlSourceDist, Dist, DistributionMetadata, FileLocation,
    GitSourceDist, IndexUrl, Name, PathBuiltDist, PathSourceDist, RegistryBuiltDist,
    RegistrySourceDist, Resolution, ResolvedDist, ToUrlError, VersionOrUrlRef,
};
use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, VerbatimUrl};
use platform_tags::{TagCompatibility, TagPriority, Tags};
use pypi_types::HashDigest;
use rustc_hash::FxHashMap;
use url::Url;
use uv_normalize::PackageName;

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
            let name = PackageName::new(dist.id.name.to_string()).unwrap();
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
    pub(crate) sourcedist: Option<SourceDist>,
    #[serde(default, rename = "wheel", skip_serializing_if = "Vec::is_empty")]
    pub(crate) wheels: Vec<Wheel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) dependencies: Vec<Dependency>,
}

impl Distribution {
    pub(crate) fn from_resolved_dist(
        resolved_dist: &ResolvedDist,
    ) -> Result<Distribution, LockError> {
        let id = DistributionId::from_resolved_dist(resolved_dist);
        let mut sourcedist = None;
        let mut wheels = vec![];
        if let Some(wheel) = Wheel::from_resolved_dist(resolved_dist)? {
            wheels.push(wheel);
        } else if let Some(sdist) = SourceDist::from_resolved_dist(resolved_dist)? {
            sourcedist = Some(sdist);
        }
        Ok(Distribution {
            id,
            // TODO: Refactoring is needed to get the marker expressions for a
            // particular resolved dist to this point.
            marker: None,
            sourcedist,
            wheels,
            dependencies: vec![],
        })
    }

    pub(crate) fn add_dependency(&mut self, resolved_dist: &ResolvedDist) {
        self.dependencies
            .push(Dependency::from_resolved_dist(resolved_dist));
    }

    fn to_dist(&self, _marker_env: &MarkerEnvironment, tags: &Tags) -> Dist {
        if let Some(wheel) = self.find_best_wheel(tags) {
            return match self.id.source.kind {
                SourceKind::Registry => {
                    let filename: WheelFilename = wheel.filename.clone();
                    let file = Box::new(distribution_types::File {
                        dist_info_metadata: false,
                        filename: filename.to_string(),
                        hashes: vec![],
                        requires_python: None,
                        size: None,
                        upload_time_utc_ms: None,
                        url: FileLocation::AbsoluteUrl(wheel.url.to_string()),
                        yanked: None,
                    });
                    let index = IndexUrl::Pypi(VerbatimUrl::from_url(self.id.source.url.clone()));
                    let reg_dist = RegistryBuiltDist {
                        filename,
                        file,
                        index,
                    };
                    let built_dist = BuiltDist::Registry(reg_dist);
                    Dist::Built(built_dist)
                }
                // TODO: Handle other kinds of sources.
                _ => todo!(),
            };
        }
        // TODO: Handle source dists.

        // TODO: Convert this to a deserialization error.
        panic!("invalid lock distribution")
    }

    fn find_best_wheel(&self, tags: &Tags) -> Option<&Wheel> {
        let mut best: Option<(TagPriority, &Wheel)> = None;
        for wheel in &self.wheels {
            let TagCompatibility::Compatible(priority) = wheel.filename.compatibility(tags) else {
                continue;
            };
            match best {
                None => {
                    best = Some((priority, wheel));
                }
                Some((best_priority, _)) => {
                    if priority > best_priority {
                        best = Some((priority, wheel));
                    }
                }
            }
        }
        best.map(|(_, wheel)| wheel)
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
    fn from_resolved_dist(resolved_dist: &ResolvedDist) -> DistributionId {
        let name = resolved_dist.name().clone();
        let version = match resolved_dist.version_or_url() {
            VersionOrUrlRef::Version(v) => v.clone(),
            // TODO: We need a way to thread the version number for these
            // cases down into this routine. The version number isn't yet in a
            // `ResolutionGraph`, so this will require a bit of refactoring.
            VersionOrUrlRef::Url(_) => todo!(),
        };
        let source = Source::from_resolved_dist(resolved_dist);
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
        }
    }

    fn from_registry_built_dist(reg_dist: &RegistryBuiltDist) -> Source {
        Source::from_index_url(&reg_dist.index)
    }

    fn from_registry_source_dist(reg_dist: &RegistrySourceDist) -> Source {
        Source::from_index_url(&reg_dist.index)
    }

    fn from_direct_built_dist(direct_dist: &DirectUrlBuiltDist) -> Source {
        Source {
            kind: SourceKind::Direct,
            url: direct_dist.url.to_url(),
        }
    }

    fn from_direct_source_dist(direct_dist: &DirectUrlSourceDist) -> Source {
        Source {
            kind: SourceKind::Direct,
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
        // FIXME: Fill in the git revision details here. They aren't in
        // `GitSourceDist`, so this will likely need some refactoring.
        Source {
            kind: SourceKind::Git(GitSource {
                precise: None,
                kind: GitSourceKind::DefaultBranch,
            }),
            url: git_dist.url.to_url(),
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
                kind: SourceKind::Git(GitSource::from_url(&mut url)),
                url,
            }),
            "direct" => Ok(Source {
                kind: SourceKind::Direct,
                url,
            }),
            "path" => Ok(Source {
                kind: SourceKind::Path,
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
#[derive(
    Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SourceKind {
    Registry,
    Git(GitSource),
    Direct,
    Path,
}

impl SourceKind {
    fn name(&self) -> &str {
        match *self {
            SourceKind::Registry => "registry",
            SourceKind::Git(_) => "git",
            SourceKind::Direct => "direct",
            SourceKind::Path => "path",
        }
    }
}

/// NOTE: Care should be taken when adding variants to this enum. Namely, new
/// variants should be added without changing the relative ordering of other
/// variants. Otherwise, this could cause the lock file to have a different
/// canonical ordering of distributions.
#[derive(
    Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
pub(crate) struct GitSource {
    precise: Option<String>,
    kind: GitSourceKind,
}

impl GitSource {
    /// Extracts a git source reference from the query pairs and the hash
    /// fragment in the given URL.
    ///
    /// This also removes the query pairs and hash fragment from the given
    /// URL in place.
    fn from_url(url: &mut Url) -> GitSource {
        let mut kind = GitSourceKind::DefaultBranch;
        for (key, val) in url.query_pairs() {
            kind = match &*key {
                "tag" => GitSourceKind::Tag(val.into_owned()),
                "branch" => GitSourceKind::Branch(val.into_owned()),
                "rev" => GitSourceKind::Rev(val.into_owned()),
                _ => continue,
            };
        }
        let precise = url.fragment().map(ToString::to_string);
        url.set_query(None);
        url.set_fragment(None);
        GitSource { precise, kind }
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
    hash: Hash,
}

impl SourceDist {
    fn from_resolved_dist(resolved_dist: &ResolvedDist) -> Result<Option<SourceDist>, LockError> {
        match *resolved_dist {
            // TODO: Do we want to try to lock already-installed distributions?
            // Or should we return an error?
            ResolvedDist::Installed(_) => todo!(),
            ResolvedDist::Installable(ref dist) => SourceDist::from_dist(dist),
        }
    }

    fn from_dist(dist: &Dist) -> Result<Option<SourceDist>, LockError> {
        match *dist {
            Dist::Built(_) => Ok(None),
            Dist::Source(ref source_dist) => SourceDist::from_source_dist(source_dist).map(Some),
        }
    }

    fn from_source_dist(
        source_dist: &distribution_types::SourceDist,
    ) -> Result<SourceDist, LockError> {
        match *source_dist {
            distribution_types::SourceDist::Registry(ref reg_dist) => {
                SourceDist::from_registry_dist(reg_dist)
            }
            distribution_types::SourceDist::DirectUrl(ref direct_dist) => {
                Ok(SourceDist::from_direct_dist(direct_dist))
            }
            distribution_types::SourceDist::Git(ref git_dist) => {
                Ok(SourceDist::from_git_dist(git_dist))
            }
            distribution_types::SourceDist::Path(ref path_dist) => {
                Ok(SourceDist::from_path_dist(path_dist))
            }
        }
    }

    fn from_registry_dist(reg_dist: &RegistrySourceDist) -> Result<SourceDist, LockError> {
        // FIXME: Is it guaranteed that there is at least one hash?
        // If not, we probably need to make this fallible.
        let url = reg_dist
            .file
            .url
            .to_url()
            .map_err(LockError::invalid_file_url)?;
        let hash = Hash::from(reg_dist.file.hashes[0].clone());
        Ok(SourceDist { url, hash })
    }

    fn from_direct_dist(direct_dist: &DirectUrlSourceDist) -> SourceDist {
        SourceDist {
            url: direct_dist.url.to_url(),
            // TODO: We want a hash for the artifact at the URL.
            hash: todo!(),
        }
    }

    fn from_git_dist(git_dist: &GitSourceDist) -> SourceDist {
        SourceDist {
            url: git_dist.url.to_url(),
            // TODO: We want a hash for the artifact at the URL.
            hash: todo!(),
        }
    }

    fn from_path_dist(path_dist: &PathSourceDist) -> SourceDist {
        SourceDist {
            url: path_dist.url.to_url(),
            // TODO: We want a hash for the artifact at the URL.
            hash: todo!(),
        }
    }
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
    /// A hash of the source distribution.
    hash: Hash,
    /// The filename of the wheel.
    ///
    /// This isn't part of the wire format since it's redundant with the
    /// URL. But we do use it for various things, and thus compute it at
    /// deserialization time. Not being able to extract a wheel filename from a
    /// wheel URL is thus a deserialization error.
    filename: WheelFilename,
}

impl Wheel {
    fn from_resolved_dist(resolved_dist: &ResolvedDist) -> Result<Option<Wheel>, LockError> {
        match *resolved_dist {
            // TODO: Do we want to try to lock already-installed distributions?
            // Or should we return an error?
            ResolvedDist::Installed(_) => todo!(),
            ResolvedDist::Installable(ref dist) => Wheel::from_dist(dist),
        }
    }

    fn from_dist(dist: &Dist) -> Result<Option<Wheel>, LockError> {
        match *dist {
            Dist::Built(ref built_dist) => Wheel::from_built_dist(built_dist).map(Some),
            Dist::Source(_) => Ok(None),
        }
    }

    fn from_built_dist(built_dist: &BuiltDist) -> Result<Wheel, LockError> {
        match *built_dist {
            BuiltDist::Registry(ref reg_dist) => Wheel::from_registry_dist(reg_dist),
            BuiltDist::DirectUrl(ref direct_dist) => Ok(Wheel::from_direct_dist(direct_dist)),
            BuiltDist::Path(ref path_dist) => Ok(Wheel::from_path_dist(path_dist)),
        }
    }

    fn from_registry_dist(reg_dist: &RegistryBuiltDist) -> Result<Wheel, LockError> {
        // FIXME: Is it guaranteed that there is at least one hash?
        // If not, we probably need to make this fallible.
        let filename = reg_dist.filename.clone();
        let url = reg_dist
            .file
            .url
            .to_url()
            .map_err(LockError::invalid_file_url)?;
        let hash = Hash::from(reg_dist.file.hashes[0].clone());
        Ok(Wheel {
            url,
            hash,
            filename,
        })
    }

    fn from_direct_dist(direct_dist: &DirectUrlBuiltDist) -> Wheel {
        Wheel {
            url: direct_dist.url.to_url(),
            // TODO: We want a hash for the artifact at the URL.
            hash: todo!(),
            filename: direct_dist.filename.clone(),
        }
    }

    fn from_path_dist(path_dist: &PathBuiltDist) -> Wheel {
        Wheel {
            url: path_dist.url.to_url(),
            // TODO: We want a hash for the artifact at the URL.
            hash: todo!(),
            filename: path_dist.filename.clone(),
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
    /// A hash of the source distribution.
    hash: Hash,
}

impl From<Wheel> for WheelWire {
    fn from(wheel: Wheel) -> WheelWire {
        WheelWire {
            url: wheel.url,
            hash: wheel.hash,
        }
    }
}

impl TryFrom<WheelWire> for Wheel {
    type Error = String;

    fn try_from(wire: WheelWire) -> Result<Wheel, String> {
        let path_segments = wire
            .url
            .path_segments()
            .ok_or_else(|| format!("could not extract path from URL `{}`", wire.url))?;
        // This is guaranteed by the contract of Url::path_segments.
        let last = path_segments.last().expect("path segments is non-empty");
        let filename = last
            .parse::<WheelFilename>()
            .map_err(|err| format!("failed to parse `{last}` as wheel filename: {err}"))?;
        Ok(Wheel {
            url: wire.url,
            hash: wire.hash,
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
    fn from_resolved_dist(resolved_dist: &ResolvedDist) -> Dependency {
        let id = DistributionId::from_resolved_dist(resolved_dist);
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
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self.kind {
            LockErrorKind::DuplicateDistribution { .. } => None,
            LockErrorKind::DuplicateDependency { .. } => None,
            LockErrorKind::InvalidFileUrl { ref err } => Some(err),
            LockErrorKind::UnrecognizedDependency { ref err } => Some(err),
        }
    }
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self.kind {
            LockErrorKind::DuplicateDistribution { ref id } => {
                write!(f, "found duplicate distribution {id}")
            }
            LockErrorKind::DuplicateDependency {
                ref id,
                ref dependency_id,
            } => {
                write!(
                    f,
                    "for distribution {id}, found duplicate dependency {dependency_id}"
                )
            }
            LockErrorKind::InvalidFileUrl { .. } => {
                write!(f, "failed to parse wheel or source dist URL")
            }
            LockErrorKind::UnrecognizedDependency { .. } => {
                write!(f, "found unrecognized dependency")
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
}

impl std::error::Error for SourceParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind {
            SourceParseErrorKind::NoPlus | SourceParseErrorKind::UnrecognizedSourceName { .. } => {
                None
            }
            SourceParseErrorKind::InvalidUrl { ref err } => Some(err),
        }
    }
}

impl std::fmt::Display for SourceParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let given = &self.given;
        match self.kind {
            SourceParseErrorKind::NoPlus => write!(f, "could not find '+' in source `{given}`"),
            SourceParseErrorKind::UnrecognizedSourceName { ref name } => {
                write!(f, "unrecognized name `{name}` in source `{given}`")
            }
            SourceParseErrorKind::InvalidUrl { .. } => write!(f, "invalid URL in source `{given}`"),
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
