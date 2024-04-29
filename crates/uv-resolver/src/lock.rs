// Temporarily allowed because this module is still in a state of flux
// as we build out universal locking.
#![allow(dead_code, unreachable_code)]

use distribution_types::{
    BuiltDist, DirectUrlBuiltDist, DirectUrlSourceDist, Dist, DistributionMetadata, GitSourceDist,
    IndexUrl, Name, PathBuiltDist, PathSourceDist, RegistryBuiltDist, RegistrySourceDist,
    ResolvedDist, ToUrlError, VersionOrUrl,
};
use pep440_rs::Version;
use pypi_types::HashDigest;
use url::Url;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct Lock {
    version: u32,
    #[cfg_attr(feature = "serde", serde(rename = "distribution"))]
    distributions: Vec<Distribution>,
}

impl Lock {
    pub(crate) fn new(mut distributions: Vec<Distribution>) -> Result<Lock, LockError> {
        for dist in &mut distributions {
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
        distributions.sort_by(|dist1, dist2| dist1.id.cmp(&dist2.id));
        for window in distributions.windows(2) {
            let (dist1, dist2) = (&window[0], &window[1]);
            if dist1.id == dist2.id {
                return Err(LockError::duplicate_distribution(dist1.id.clone()));
            }
        }
        Ok(Lock {
            version: 1,
            distributions,
        })
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub(crate) struct Distribution {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub(crate) id: DistributionId,
    pub(crate) marker: Option<String>,
    pub(crate) sourcedist: Option<SourceDist>,
    #[cfg_attr(
        feature = "serde",
        serde(rename = "wheel", skip_serializing_if = "Vec::is_empty")
    )]
    pub(crate) wheels: Vec<Wheel>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Vec::is_empty"))]
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
}

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub(crate) struct DistributionId {
    pub(crate) name: String,
    pub(crate) version: Version,
    pub(crate) source: Source,
}

impl DistributionId {
    fn from_resolved_dist(resolved_dist: &ResolvedDist) -> DistributionId {
        let name = resolved_dist.name().to_string();
        let version = match resolved_dist.version_or_url() {
            VersionOrUrl::Version(v) => v.clone(),
            // TODO: We need a way to thread the version number for these
            // cases down into this routine. The version number isn't yet in a
            // `ResolutionGraph`, so this will require a bit of refactoring.
            VersionOrUrl::Url(_) => todo!(),
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

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
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

#[cfg(feature = "serde")]
impl serde::Serialize for Source {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        s.collect_str(self)
    }
}

#[cfg(feature = "serde")]
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
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
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
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
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

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
enum GitSourceKind {
    Tag(String),
    Branch(String),
    Rev(String),
    DefaultBranch,
}

/// Inspired by: <https://discuss.python.org/t/lock-files-again-but-this-time-w-sdists/46593>
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
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
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub(crate) struct Wheel {
    /// A URL or file path (via `file://`) where the wheel that was locked
    /// against was found. The location does not need to exist in the future,
    /// so this should be treated as only a hint to where to look and/or
    /// recording where the wheel file originally came from.
    url: Url,
    /// A hash of the source distribution.
    hash: Hash,
    // THOUGHT: Would it be better to include a more structured representation
    // of the wheel's filename in the lock file itself? e.g., All of the wheel
    // tags. This would avoid needing to parse the wheel tags out of the URL,
    // which is a potentially fallible operation. But, I think it is nice to
    // have just the URL which is more succinct and doesn't result in encoding
    // the same information twice. Probably the best thing to do here is to add
    // the wheel tags fields here, but don't serialize them.
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
        let url = reg_dist
            .file
            .url
            .to_url()
            .map_err(LockError::invalid_file_url)?;
        let hash = Hash::from(reg_dist.file.hashes[0].clone());
        Ok(Wheel { url, hash })
    }

    fn from_direct_dist(direct_dist: &DirectUrlBuiltDist) -> Wheel {
        Wheel {
            url: direct_dist.url.to_url(),
            // TODO: We want a hash for the artifact at the URL.
            hash: todo!(),
        }
    }

    fn from_path_dist(path_dist: &PathBuiltDist) -> Wheel {
        Wheel {
            url: path_dist.url.to_url(),
            // TODO: We want a hash for the artifact at the URL.
            hash: todo!(),
        }
    }
}

/// A single dependency of a distribution in a lock file.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub(crate) struct Dependency {
    #[cfg_attr(feature = "serde", serde(flatten))]
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

#[cfg(feature = "serde")]
impl serde::Serialize for Hash {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        s.collect_str(self)
    }
}

#[cfg(feature = "serde")]
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
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self.kind {
            LockErrorKind::DuplicateDistribution { .. } => None,
            LockErrorKind::DuplicateDependency { .. } => None,
            LockErrorKind::InvalidFileUrl { ref err } => Some(err),
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
