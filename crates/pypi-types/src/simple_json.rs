use std::str::FromStr;

use jiff::Timestamp;
use serde::{Deserialize, Deserializer, Serialize};

use pep440_rs::{VersionSpecifiers, VersionSpecifiersParseError};

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
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct File {
    // PEP 714-renamed field, followed by PEP 691-compliant field, followed by non-PEP 691-compliant
    // alias used by PyPI.
    pub core_metadata: Option<CoreMetadata>,
    pub dist_info_metadata: Option<CoreMetadata>,
    pub data_dist_info_metadata: Option<CoreMetadata>,
    pub filename: String,
    pub hashes: Hashes,
    /// There are a number of invalid specifiers on PyPI, so we first try to parse it into a
    /// [`VersionSpecifiers`] according to spec (PEP 440), then a [`LenientVersionSpecifiers`] with
    /// fixup for some common problems and if this still fails, we skip the file when creating a
    /// version map.
    #[serde(default, deserialize_with = "deserialize_version_specifiers_lenient")]
    pub requires_python: Option<Result<VersionSpecifiers, VersionSpecifiersParseError>>,
    pub size: Option<u64>,
    pub upload_time: Option<Timestamp>,
    pub url: String,
    pub yanked: Option<Yanked>,
}

fn deserialize_version_specifiers_lenient<'de, D>(
    deserializer: D,
) -> Result<Option<Result<VersionSpecifiers, VersionSpecifiersParseError>>, D::Error>
where
    D: Deserializer<'de>,
{
    let maybe_string: Option<String> = Option::deserialize(deserializer)?;
    let Some(string) = maybe_string else {
        return Ok(None);
    };
    Ok(Some(
        LenientVersionSpecifiers::from_str(&string).map(VersionSpecifiers::from),
    ))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CoreMetadata {
    Bool(bool),
    Hashes(Hashes),
}

impl CoreMetadata {
    pub fn is_available(&self) -> bool {
        match self {
            Self::Bool(is_available) => *is_available,
            Self::Hashes(_) => true,
        }
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
#[serde(untagged)]
pub enum Yanked {
    Bool(bool),
    Reason(String),
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
    pub md5: Option<Box<str>>,
    pub sha256: Option<Box<str>>,
    pub sha384: Option<Box<str>>,
    pub sha512: Option<Box<str>>,
}

impl Hashes {
    /// Convert a set of [`Hashes`] into a list of [`HashDigest`]s.
    pub fn into_digests(self) -> Vec<HashDigest> {
        let mut digests = Vec::new();
        if let Some(sha512) = self.sha512 {
            digests.push(HashDigest {
                algorithm: HashAlgorithm::Sha512,
                digest: sha512,
            });
        }
        if let Some(sha384) = self.sha384 {
            digests.push(HashDigest {
                algorithm: HashAlgorithm::Sha384,
                digest: sha384,
            });
        }
        if let Some(sha256) = self.sha256 {
            digests.push(HashDigest {
                algorithm: HashAlgorithm::Sha256,
                digest: sha256,
            });
        }
        if let Some(md5) = self.md5 {
            digests.push(HashDigest {
                algorithm: HashAlgorithm::Md5,
                digest: md5,
            });
        }
        digests
    }

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
            "md5" => {
                let md5 = std::str::from_utf8(value.as_bytes())?;
                let md5 = md5.to_owned().into_boxed_str();
                Ok(Hashes {
                    md5: Some(md5),
                    sha256: None,
                    sha384: None,
                    sha512: None,
                })
            }
            "sha256" => {
                let sha256 = std::str::from_utf8(value.as_bytes())?;
                let sha256 = sha256.to_owned().into_boxed_str();
                Ok(Hashes {
                    md5: None,
                    sha256: Some(sha256),
                    sha384: None,
                    sha512: None,
                })
            }
            "sha384" => {
                let sha384 = std::str::from_utf8(value.as_bytes())?;
                let sha384 = sha384.to_owned().into_boxed_str();
                Ok(Hashes {
                    md5: None,
                    sha256: None,
                    sha384: Some(sha384),
                    sha512: None,
                })
            }
            "sha512" => {
                let sha512 = std::str::from_utf8(value.as_bytes())?;
                let sha512 = sha512.to_owned().into_boxed_str();
                Ok(Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: Some(sha512),
                })
            }
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
            "md5" => {
                let md5 = value.to_owned().into_boxed_str();
                Ok(Hashes {
                    md5: Some(md5),
                    sha256: None,
                    sha384: None,
                    sha512: None,
                })
            }
            "sha256" => {
                let sha256 = value.to_owned().into_boxed_str();
                Ok(Hashes {
                    md5: None,
                    sha256: Some(sha256),
                    sha384: None,
                    sha512: None,
                })
            }
            "sha384" => {
                let sha384 = value.to_owned().into_boxed_str();
                Ok(Hashes {
                    md5: None,
                    sha256: None,
                    sha384: Some(sha384),
                    sha512: None,
                })
            }
            "sha512" => {
                let sha512 = value.to_owned().into_boxed_str();
                Ok(Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: Some(sha512),
                })
            }
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
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
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
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct HashDigest {
    pub algorithm: HashAlgorithm,
    pub digest: Box<str>,
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

        Ok(HashDigest {
            algorithm,
            digest: value.to_owned().into_boxed_str(),
        })
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

    #[error("Non-UTF-8 hash digest")]
    NonUtf8(#[from] std::str::Utf8Error),
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
