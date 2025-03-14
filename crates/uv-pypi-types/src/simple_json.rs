use std::str::FromStr;

use jiff::Timestamp;
use serde::{Deserialize, Deserializer, Serialize};

use uv_pep440::{VersionSpecifiers, VersionSpecifiersParseError};
use uv_small_str::SmallString;

use crate::lenient_requirement::LenientVersionSpecifiers;

/// A collection of "files" from `PyPI`'s JSON API for a single package.
#[derive(Debug, Clone, Deserialize)]
pub struct SimpleJson {
    /// The list of [`File`]s available for download sorted by filename.
    #[serde(deserialize_with = "sorted_simple_json_files")]
    pub files: Vec<File>,
}

/// Deserializes a sequence of "simple" files from `PyPI` and ensures that they
/// are sorted in a stable order.
fn sorted_simple_json_files<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<File>, D::Error> {
    let mut files = <Vec<File>>::deserialize(d)?;
    // While it has not been positively observed, we sort the files
    // to ensure we have a defined ordering. Otherwise, if we rely on
    // the API to provide a stable ordering and doesn't, it can lead
    // non-deterministic behavior elsewhere. (This is somewhat hand-wavy
    // and a bit of a band-aide, since arguably, the order of this API
    // response probably shouldn't have an impact on things downstream from
    // this. That is, if something depends on ordering, then it should
    // probably be the thing that does the sorting.)
    files.sort_unstable_by(|f1, f2| f1.filename.cmp(&f2.filename));
    Ok(files)
}

/// A single (remote) file belonging to a package, either a wheel or a source distribution.
///
/// <https://peps.python.org/pep-0691/#project-detail>
#[derive(Debug, Clone)]
pub struct File {
    pub core_metadata: Option<CoreMetadata>,
    pub filename: SmallString,
    pub hashes: Hashes,
    pub requires_python: Option<Result<VersionSpecifiers, VersionSpecifiersParseError>>,
    pub size: Option<u64>,
    pub upload_time: Option<Timestamp>,
    pub url: SmallString,
    pub yanked: Option<Box<Yanked>>,
}

impl<'de> Deserialize<'de> for File {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FileVisitor;

        impl<'de> serde::de::Visitor<'de> for FileVisitor {
            type Value = File;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map containing file metadata")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut core_metadata = None;
                let mut filename = None;
                let mut hashes = None;
                let mut requires_python = None;
                let mut size = None;
                let mut upload_time = None;
                let mut url = None;
                let mut yanked = None;

                while let Some(key) = access.next_key::<String>()? {
                    match key.as_str() {
                        "core-metadata" | "dist-info-metadata" | "data-dist-info-metadata" => {
                            if core_metadata.is_none() {
                                core_metadata = access.next_value()?;
                            } else {
                                let _: serde::de::IgnoredAny = access.next_value()?;
                            }
                        }
                        "filename" => filename = Some(access.next_value()?),
                        "hashes" => hashes = Some(access.next_value()?),
                        "requires-python" => {
                            requires_python = access.next_value::<Option<&str>>()?.map(|s| {
                                LenientVersionSpecifiers::from_str(s).map(VersionSpecifiers::from)
                            });
                        }
                        "size" => size = Some(access.next_value()?),
                        "upload-time" => upload_time = Some(access.next_value()?),
                        "url" => url = Some(access.next_value()?),
                        "yanked" => yanked = Some(access.next_value()?),
                        _ => {
                            let _: serde::de::IgnoredAny = access.next_value()?;
                        }
                    }
                }

                Ok(File {
                    core_metadata,
                    filename: filename
                        .ok_or_else(|| serde::de::Error::missing_field("filename"))?,
                    hashes: hashes.ok_or_else(|| serde::de::Error::missing_field("hashes"))?,
                    requires_python,
                    size,
                    upload_time,
                    url: url.ok_or_else(|| serde::de::Error::missing_field("url"))?,
                    yanked,
                })
            }
        }

        deserializer.deserialize_map(FileVisitor)
    }
}

#[derive(Debug, Clone)]
pub enum CoreMetadata {
    Bool(bool),
    Hashes(Hashes),
}

impl<'de> Deserialize<'de> for CoreMetadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        serde_untagged::UntaggedEnumVisitor::new()
            .bool(|bool| Ok(CoreMetadata::Bool(bool)))
            .map(|map| map.deserialize().map(CoreMetadata::Hashes))
            .deserialize(deserializer)
    }
}

impl CoreMetadata {
    pub fn is_available(&self) -> bool {
        match self {
            Self::Bool(is_available) => *is_available,
            Self::Hashes(_) => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub enum Yanked {
    Bool(bool),
    Reason(SmallString),
}

impl<'de> Deserialize<'de> for Yanked {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        serde_untagged::UntaggedEnumVisitor::new()
            .bool(|bool| Ok(Yanked::Bool(bool)))
            .string(|string| Ok(Yanked::Reason(SmallString::from(string))))
            .deserialize(deserializer)
    }
}

impl Yanked {
    pub fn is_yanked(&self) -> bool {
        match self {
            Self::Bool(is_yanked) => *is_yanked,
            Self::Reason(_) => true,
        }
    }
}

impl Default for Yanked {
    fn default() -> Self {
        Self::Bool(false)
    }
}

/// A dictionary mapping a hash name to a hex encoded digest of the file.
///
/// PEP 691 says multiple hashes can be included and the interpretation is left to the client.
#[derive(Debug, Clone, Eq, PartialEq, Default, Deserialize)]
pub struct Hashes {
    pub md5: Option<SmallString>,
    pub sha256: Option<SmallString>,
    pub sha384: Option<SmallString>,
    pub sha512: Option<SmallString>,
}

impl Hashes {
    /// Parse the hash from a fragment, as in: `sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61`
    pub fn parse_fragment(fragment: &str) -> Result<Self, HashError> {
        let mut parts = fragment.split('=');

        // Extract the key and value.
        let name = parts
            .next()
            .ok_or_else(|| HashError::InvalidFragment(fragment.to_string()))?;
        let value = parts
            .next()
            .ok_or_else(|| HashError::InvalidFragment(fragment.to_string()))?;

        // Ensure there are no more parts.
        if parts.next().is_some() {
            return Err(HashError::InvalidFragment(fragment.to_string()));
        }

        match name {
            "md5" => Ok(Hashes {
                md5: Some(SmallString::from(value)),
                sha256: None,
                sha384: None,
                sha512: None,
            }),
            "sha256" => Ok(Hashes {
                md5: None,
                sha256: Some(SmallString::from(value)),
                sha384: None,
                sha512: None,
            }),
            "sha384" => Ok(Hashes {
                md5: None,
                sha256: None,
                sha384: Some(SmallString::from(value)),
                sha512: None,
            }),
            "sha512" => Ok(Hashes {
                md5: None,
                sha256: None,
                sha384: None,
                sha512: Some(SmallString::from(value)),
            }),
            _ => Err(HashError::UnsupportedHashAlgorithm(fragment.to_string())),
        }
    }
}

impl FromStr for Hashes {
    type Err = HashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split(':');

        // Extract the key and value.
        let name = parts
            .next()
            .ok_or_else(|| HashError::InvalidStructure(s.to_string()))?;
        let value = parts
            .next()
            .ok_or_else(|| HashError::InvalidStructure(s.to_string()))?;

        // Ensure there are no more parts.
        if parts.next().is_some() {
            return Err(HashError::InvalidStructure(s.to_string()));
        }

        match name {
            "md5" => Ok(Hashes {
                md5: Some(SmallString::from(value)),
                sha256: None,
                sha384: None,
                sha512: None,
            }),
            "sha256" => Ok(Hashes {
                md5: None,
                sha256: Some(SmallString::from(value)),
                sha384: None,
                sha512: None,
            }),
            "sha384" => Ok(Hashes {
                md5: None,
                sha256: None,
                sha384: Some(SmallString::from(value)),
                sha512: None,
            }),
            "sha512" => Ok(Hashes {
                md5: None,
                sha256: None,
                sha384: None,
                sha512: Some(SmallString::from(value)),
            }),
            _ => Err(HashError::UnsupportedHashAlgorithm(s.to_string())),
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Ord,
    PartialOrd,
    Eq,
    PartialEq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
pub enum HashAlgorithm {
    Md5,
    Sha256,
    Sha384,
    Sha512,
}

impl FromStr for HashAlgorithm {
    type Err = HashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "md5" => Ok(Self::Md5),
            "sha256" => Ok(Self::Sha256),
            "sha384" => Ok(Self::Sha384),
            "sha512" => Ok(Self::Sha512),
            _ => Err(HashError::UnsupportedHashAlgorithm(s.to_string())),
        }
    }
}

impl std::fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Md5 => write!(f, "md5"),
            Self::Sha256 => write!(f, "sha256"),
            Self::Sha384 => write!(f, "sha384"),
            Self::Sha512 => write!(f, "sha512"),
        }
    }
}

/// A hash name and hex encoded digest of the file.
#[derive(
    Debug,
    Clone,
    Ord,
    PartialOrd,
    Eq,
    PartialEq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
pub struct HashDigest {
    pub algorithm: HashAlgorithm,
    pub digest: SmallString,
}

impl HashDigest {
    /// Return the [`HashAlgorithm`] of the digest.
    pub fn algorithm(&self) -> HashAlgorithm {
        self.algorithm
    }
}

impl std::fmt::Display for HashDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.algorithm, self.digest)
    }
}

impl FromStr for HashDigest {
    type Err = HashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split(':');

        // Extract the key and value.
        let name = parts
            .next()
            .ok_or_else(|| HashError::InvalidStructure(s.to_string()))?;
        let value = parts
            .next()
            .ok_or_else(|| HashError::InvalidStructure(s.to_string()))?;

        // Ensure there are no more parts.
        if parts.next().is_some() {
            return Err(HashError::InvalidStructure(s.to_string()));
        }

        let algorithm = HashAlgorithm::from_str(name)?;
        let digest = SmallString::from(value);

        Ok(Self { algorithm, digest })
    }
}

/// A collection of [`HashDigest`] entities.
#[derive(
    Debug,
    Clone,
    Ord,
    PartialOrd,
    Eq,
    PartialEq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
pub struct HashDigests(Box<[HashDigest]>);

impl HashDigests {
    /// Initialize an empty collection of [`HashDigest`] entities.
    pub fn empty() -> Self {
        Self(Box::new([]))
    }

    /// Return the [`HashDigest`] entities as a slice.
    pub fn as_slice(&self) -> &[HashDigest] {
        self.0.as_ref()
    }

    /// Returns `true` if the [`HashDigests`] are empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the first [`HashDigest`] entity.
    pub fn first(&self) -> Option<&HashDigest> {
        self.0.first()
    }

    /// Return the [`HashDigest`] entities as a vector.
    pub fn to_vec(&self) -> Vec<HashDigest> {
        self.0.to_vec()
    }

    /// Returns an [`Iterator`] over the [`HashDigest`] entities.
    pub fn iter(&self) -> impl Iterator<Item = &HashDigest> {
        self.0.iter()
    }

    /// Sort the underlying [`HashDigest`] entities.
    pub fn sort_unstable(&mut self) {
        self.0.sort_unstable();
    }
}

/// Convert a set of [`Hashes`] into a list of [`HashDigest`]s.
impl From<Hashes> for HashDigests {
    fn from(value: Hashes) -> Self {
        let mut digests = Vec::with_capacity(
            usize::from(value.sha512.is_some())
                + usize::from(value.sha384.is_some())
                + usize::from(value.sha256.is_some())
                + usize::from(value.md5.is_some()),
        );
        if let Some(sha512) = value.sha512 {
            digests.push(HashDigest {
                algorithm: HashAlgorithm::Sha512,
                digest: sha512,
            });
        }
        if let Some(sha384) = value.sha384 {
            digests.push(HashDigest {
                algorithm: HashAlgorithm::Sha384,
                digest: sha384,
            });
        }
        if let Some(sha256) = value.sha256 {
            digests.push(HashDigest {
                algorithm: HashAlgorithm::Sha256,
                digest: sha256,
            });
        }
        if let Some(md5) = value.md5 {
            digests.push(HashDigest {
                algorithm: HashAlgorithm::Md5,
                digest: md5,
            });
        }
        Self::from(digests)
    }
}

impl From<HashDigest> for HashDigests {
    fn from(value: HashDigest) -> Self {
        Self(Box::new([value]))
    }
}

impl From<&[HashDigest]> for HashDigests {
    fn from(value: &[HashDigest]) -> Self {
        Self(Box::from(value))
    }
}

impl From<Vec<HashDigest>> for HashDigests {
    fn from(value: Vec<HashDigest>) -> Self {
        Self(value.into_boxed_slice())
    }
}

impl FromIterator<HashDigest> for HashDigests {
    fn from_iter<T: IntoIterator<Item = HashDigest>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl IntoIterator for HashDigests {
    type Item = HashDigest;
    type IntoIter = std::vec::IntoIter<HashDigest>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_vec().into_iter()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum HashError {
    #[error("Unexpected hash (expected `<algorithm>:<hash>`): {0}")]
    InvalidStructure(String),

    #[error("Unexpected fragment (expected `#sha256=...` or similar) on URL: {0}")]
    InvalidFragment(String),

    #[error(
        "Unsupported hash algorithm (expected one of: `md5`, `sha256`, `sha384`, or `sha512`) on: `{0}`"
    )]
    UnsupportedHashAlgorithm(String),
}

#[cfg(test)]
mod tests {
    use crate::{HashError, Hashes};

    #[test]
    fn parse_hashes() -> Result<(), HashError> {
        let hashes: Hashes =
            "sha512:40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f".parse()?;
        assert_eq!(
            hashes,
            Hashes {
                md5: None,
                sha256: None,
                sha384: None,
                sha512: Some(
                    "40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f".into()
                ),
            }
        );

        let hashes: Hashes =
            "sha384:40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f".parse()?;
        assert_eq!(
            hashes,
            Hashes {
                md5: None,
                sha256: None,
                sha384: Some(
                    "40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f".into()
                ),
                sha512: None
            }
        );

        let hashes: Hashes =
            "sha256:40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f".parse()?;
        assert_eq!(
            hashes,
            Hashes {
                md5: None,
                sha256: Some(
                    "40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f".into()
                ),
                sha384: None,
                sha512: None
            }
        );

        let hashes: Hashes =
            "md5:090376d812fb6ac5f171e5938e82e7f2d7adc2b629101cec0db8b267815c85e2".parse()?;
        assert_eq!(
            hashes,
            Hashes {
                md5: Some(
                    "090376d812fb6ac5f171e5938e82e7f2d7adc2b629101cec0db8b267815c85e2".into()
                ),
                sha256: None,
                sha384: None,
                sha512: None
            }
        );

        let result = "sha256=40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f"
            .parse::<Hashes>();
        assert!(result.is_err());

        let result = "blake2:55f44b440d491028addb3b88f72207d71eeebfb7b5dbf0643f7c023ae1fba619"
            .parse::<Hashes>();
        assert!(result.is_err());

        Ok(())
    }
}
