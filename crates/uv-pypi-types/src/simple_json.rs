use std::borrow::Cow;
use std::str::FromStr;
use std::sync::Arc;

use jiff::Timestamp;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Deserializer, Serialize};

use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers, VersionSpecifiersParseError};
use uv_pep508::Requirement;
use uv_small_str::SmallString;

use crate::lenient_requirement::LenientVersionSpecifiers;
use crate::{ProjectStatus, VerbatimParsedUrl};

mod parser;

/// A collection of "files" from `PyPI`'s JSON API for a single package, as served by the
/// `vnd.pypi.simple.v1` media type.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PypiSimpleDetail {
    /// PEP 792 project status information.
    #[serde(default)]
    pub project_status: ProjectStatus,
    /// The list of [`PypiFile`]s available for download.
    #[serde(deserialize_with = "deserialize_pypi_files")]
    pub files: Vec<PypiFile>,
}

impl PypiSimpleDetail {
    /// Parse a PyPI Simple API JSON response directly whenever possible.
    ///
    /// Fall back to Serde for unsupported input, preserving its behavior and errors.
    pub fn from_json(input: &[u8]) -> Result<Self, serde_json::Error> {
        parser::parse(input).map_or_else(|| serde_json::from_slice(input), Ok)
    }
}

/// A single (remote) file belonging to a package, either a wheel or a source distribution, as
/// served by the `vnd.pypi.simple.v1` media type.
///
/// <https://peps.python.org/pep-0691/#project-detail>
#[derive(Debug, Clone)]
pub struct PypiFile {
    pub core_metadata: Option<CoreMetadata>,
    pub filename: SmallString,
    pub hashes: Hashes,
    pub requires_python: Option<Result<Arc<VersionSpecifiers>, VersionSpecifiersParseError>>,
    pub size: Option<u64>,
    pub upload_time: Option<Timestamp>,
    pub url: SmallString,
    pub yanked: Option<Box<Yanked>>,
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "kebab-case")]
enum FileField {
    #[serde(alias = "dist-info-metadata", alias = "data-dist-info-metadata")]
    CoreMetadata,
    Filename,
    Hashes,
    RequiresPython,
    Size,
    UploadTime,
    Url,
    Yanked,
    Zstd,
    #[serde(other)]
    Ignore,
}

type RequiresPythonResult = Result<Arc<VersionSpecifiers>, VersionSpecifiersParseError>;

#[derive(Default)]
struct RequiresPythonInterner {
    values: FxHashMap<SmallString, RequiresPythonResult>,
}

impl RequiresPythonInterner {
    fn parse(&mut self, value: &str) -> RequiresPythonResult {
        if let Some(requires_python) = self.values.get(value) {
            return requires_python.clone();
        }

        let requires_python = LenientVersionSpecifiers::from_str(value)
            .map(VersionSpecifiers::from)
            .map(Arc::new);
        self.values
            .insert(SmallString::from(value), requires_python.clone());
        requires_python
    }
}

struct PypiFileWire<'a> {
    core_metadata: Option<CoreMetadata>,
    filename: SmallString,
    hashes: Hashes,
    requires_python: Option<Cow<'a, str>>,
    size: Option<u64>,
    upload_time: Option<Timestamp>,
    url: SmallString,
    yanked: Option<Box<Yanked>>,
}

impl PypiFileWire<'_> {
    fn into_file(self, interner: &mut RequiresPythonInterner) -> PypiFile {
        PypiFile {
            core_metadata: self.core_metadata,
            filename: self.filename,
            hashes: self.hashes,
            requires_python: self
                .requires_python
                .as_deref()
                .map(|value| interner.parse(value)),
            size: self.size,
            upload_time: self.upload_time,
            url: self.url,
            yanked: self.yanked,
        }
    }
}

/// Deserialize files while parsing each distinct `requires-python` value only once.
fn deserialize_pypi_files<'de, D>(deserializer: D) -> Result<Vec<PypiFile>, D::Error>
where
    D: Deserializer<'de>,
{
    let files = Vec::<PypiFileWire<'de>>::deserialize(deserializer)?;
    let mut interner = RequiresPythonInterner::default();
    Ok(files
        .into_iter()
        .map(|file| file.into_file(&mut interner))
        .collect())
}

impl<'de> Deserialize<'de> for PypiFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let file = PypiFileWire::deserialize(deserializer)?;
        let mut interner = RequiresPythonInterner::default();
        Ok(file.into_file(&mut interner))
    }
}

impl<'de> Deserialize<'de> for PypiFileWire<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(PypiFileWireVisitor)
    }
}

struct PypiFileWireVisitor;

impl<'de> serde::de::Visitor<'de> for PypiFileWireVisitor {
    type Value = PypiFileWire<'de>;

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

        while let Some(key) = access.next_key::<FileField>()? {
            match key {
                FileField::CoreMetadata if core_metadata.is_none() => {
                    core_metadata = access.next_value()?;
                }
                FileField::Filename => filename = Some(access.next_value()?),
                FileField::Hashes => hashes = Some(access.next_value()?),
                FileField::RequiresPython => {
                    requires_python = access.next_value::<Option<Cow<'de, str>>>()?;
                }
                FileField::Size => size = Some(access.next_value()?),
                FileField::UploadTime => upload_time = Some(access.next_value()?),
                FileField::Url => url = Some(access.next_value()?),
                FileField::Yanked => yanked = Some(access.next_value()?),
                _ => {
                    let _: serde::de::IgnoredAny = access.next_value()?;
                }
            }
        }

        Ok(PypiFileWire {
            core_metadata,
            filename: filename.ok_or_else(|| serde::de::Error::missing_field("filename"))?,
            hashes: hashes.ok_or_else(|| serde::de::Error::missing_field("hashes"))?,
            requires_python,
            size,
            upload_time,
            url: url.ok_or_else(|| serde::de::Error::missing_field("url"))?,
            yanked,
        })
    }
}

/// A collection of "files" from the Simple API.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PyxSimpleDetail {
    /// PEP 792 project status information.
    #[serde(default)]
    pub project_status: ProjectStatus,
    /// The list of [`PyxFile`]s available for download sorted by filename.
    #[serde(deserialize_with = "deserialize_pyx_files")]
    pub files: Vec<PyxFile>,
    /// The core metadata for the project, keyed by version.
    #[serde(default)]
    pub core_metadata: FxHashMap<Version, CoreMetadatum>,
}

/// A single (remote) file belonging to a package, either a wheel or a source distribution,
/// as served by the Simple API.
#[derive(Debug, Clone)]
pub struct PyxFile {
    pub core_metadata: Option<CoreMetadata>,
    pub filename: Option<SmallString>,
    pub hashes: Hashes,
    pub requires_python: Option<Result<Arc<VersionSpecifiers>, VersionSpecifiersParseError>>,
    pub size: Option<u64>,
    pub upload_time: Option<Timestamp>,
    pub url: SmallString,
    pub yanked: Option<Box<Yanked>>,
    pub zstd: Option<Zstd>,
}

struct PyxFileWire<'a> {
    core_metadata: Option<CoreMetadata>,
    filename: Option<SmallString>,
    hashes: Hashes,
    requires_python: Option<Cow<'a, str>>,
    size: Option<u64>,
    upload_time: Option<Timestamp>,
    url: SmallString,
    yanked: Option<Box<Yanked>>,
    zstd: Option<Zstd>,
}

impl PyxFileWire<'_> {
    fn into_file(self, interner: &mut RequiresPythonInterner) -> PyxFile {
        PyxFile {
            core_metadata: self.core_metadata,
            filename: self.filename,
            hashes: self.hashes,
            requires_python: self
                .requires_python
                .as_deref()
                .map(|value| interner.parse(value)),
            size: self.size,
            upload_time: self.upload_time,
            url: self.url,
            yanked: self.yanked,
            zstd: self.zstd,
        }
    }
}

/// Deserialize files while parsing each distinct `requires-python` value only once.
fn deserialize_pyx_files<'de, D>(deserializer: D) -> Result<Vec<PyxFile>, D::Error>
where
    D: Deserializer<'de>,
{
    let files = Vec::<PyxFileWire<'de>>::deserialize(deserializer)?;
    let mut interner = RequiresPythonInterner::default();
    Ok(files
        .into_iter()
        .map(|file| file.into_file(&mut interner))
        .collect())
}

impl<'de> Deserialize<'de> for PyxFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let file = PyxFileWire::deserialize(deserializer)?;
        let mut interner = RequiresPythonInterner::default();
        Ok(file.into_file(&mut interner))
    }
}

impl<'de> Deserialize<'de> for PyxFileWire<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(PyxFileWireVisitor)
    }
}

struct PyxFileWireVisitor;

impl<'de> serde::de::Visitor<'de> for PyxFileWireVisitor {
    type Value = PyxFileWire<'de>;

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
        let mut zstd = None;

        while let Some(key) = access.next_key::<FileField>()? {
            match key {
                FileField::CoreMetadata if core_metadata.is_none() => {
                    core_metadata = access.next_value()?;
                }
                FileField::Filename => filename = Some(access.next_value()?),
                FileField::Hashes => hashes = Some(access.next_value()?),
                FileField::RequiresPython => {
                    requires_python = access.next_value::<Option<Cow<'de, str>>>()?;
                }
                FileField::Size => size = access.next_value()?,
                FileField::UploadTime => upload_time = Some(access.next_value()?),
                FileField::Url => url = Some(access.next_value()?),
                FileField::Yanked => yanked = Some(access.next_value()?),
                FileField::Zstd => {
                    zstd = Some(access.next_value()?);
                }
                _ => {
                    let _: serde::de::IgnoredAny = access.next_value()?;
                }
            }
        }

        Ok(PyxFileWire {
            core_metadata,
            filename,
            hashes: hashes.ok_or_else(|| serde::de::Error::missing_field("hashes"))?,
            requires_python,
            size,
            upload_time,
            url: url.ok_or_else(|| serde::de::Error::missing_field("url"))?,
            yanked,
            zstd,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CoreMetadatum {
    #[serde(default)]
    pub requires_python: Option<VersionSpecifiers>,
    #[serde(default)]
    pub requires_dist: Box<[Requirement<VerbatimParsedUrl>]>,
    #[serde(default, alias = "provides-extras")]
    pub provides_extra: Box<[ExtraName]>,
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
            .bool(|bool| Ok(Self::Bool(bool)))
            .map(|map| map.deserialize().map(CoreMetadata::Hashes))
            .deserialize(deserializer)
    }
}

impl Serialize for CoreMetadata {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Bool(is_available) => serializer.serialize_bool(*is_available),
            Self::Hashes(hashes) => hashes.serialize(serializer),
        }
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
            .bool(|bool| Ok(Self::Bool(bool)))
            .string(|string| Ok(Self::Reason(SmallString::from(string))))
            .deserialize(deserializer)
    }
}

impl Serialize for Yanked {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Bool(is_yanked) => serializer.serialize_bool(*is_yanked),
            Self::Reason(reason) => serializer.serialize_str(reason.as_ref()),
        }
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

#[derive(Debug, Clone, Eq, PartialEq, Default, Deserialize, Serialize)]
pub struct Zstd {
    pub hashes: Hashes,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// A dictionary mapping a hash name to a hex encoded digest of the file.
///
/// PEP 691 says multiple hashes can be included and the interpretation is left to the client.
#[derive(Debug, Clone, Eq, PartialEq, Default, Deserialize, Serialize)]
pub struct Hashes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md5: Option<SmallString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<SmallString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha384: Option<SmallString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha512: Option<SmallString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blake2b: Option<SmallString>,
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
            "md5" => Ok(Self {
                md5: Some(SmallString::from(value)),
                sha256: None,
                sha384: None,
                sha512: None,
                blake2b: None,
            }),
            "sha256" => Ok(Self {
                md5: None,
                sha256: Some(SmallString::from(value)),
                sha384: None,
                sha512: None,
                blake2b: None,
            }),
            "sha384" => Ok(Self {
                md5: None,
                sha256: None,
                sha384: Some(SmallString::from(value)),
                sha512: None,
                blake2b: None,
            }),
            "sha512" => Ok(Self {
                md5: None,
                sha256: None,
                sha384: None,
                sha512: Some(SmallString::from(value)),
                blake2b: None,
            }),
            "blake2b" => Ok(Self {
                md5: None,
                sha256: None,
                sha384: None,
                sha512: None,
                blake2b: Some(SmallString::from(value)),
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
            "md5" => Ok(Self {
                md5: Some(SmallString::from(value)),
                sha256: None,
                sha384: None,
                sha512: None,
                blake2b: None,
            }),
            "sha256" => Ok(Self {
                md5: None,
                sha256: Some(SmallString::from(value)),
                sha384: None,
                sha512: None,
                blake2b: None,
            }),
            "sha384" => Ok(Self {
                md5: None,
                sha256: None,
                sha384: Some(SmallString::from(value)),
                sha512: None,
                blake2b: None,
            }),
            "sha512" => Ok(Self {
                md5: None,
                sha256: None,
                sha384: None,
                sha512: Some(SmallString::from(value)),
                blake2b: None,
            }),
            "blake2b" => Ok(Self {
                md5: None,
                sha256: None,
                sha384: None,
                sha512: None,
                blake2b: Some(SmallString::from(value)),
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
    Blake2b,
}

impl HashAlgorithm {
    /// Return the supported [`HashAlgorithm`] variants in order of preference.
    pub(crate) fn preferred() -> impl Iterator<Item = Self> {
        [
            Self::Sha512,
            Self::Sha384,
            Self::Sha256,
            Self::Blake2b,
            Self::Md5,
        ]
        .into_iter()
    }

    /// Return the string representation of the [`HashAlgorithm`].
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha256 => "sha256",
            Self::Sha384 => "sha384",
            Self::Sha512 => "sha512",
            Self::Blake2b => "blake2b",
        }
    }
}

impl FromStr for HashAlgorithm {
    type Err = HashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "md5" => Ok(Self::Md5),
            "sha256" => Ok(Self::Sha256),
            "sha384" => Ok(Self::Sha384),
            "sha512" => Ok(Self::Sha512),
            "blake2b" => Ok(Self::Blake2b),
            _ => Err(HashError::UnsupportedHashAlgorithm(s.to_string())),
        }
    }
}

impl std::fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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
                + usize::from(value.md5.is_some())
                + usize::from(value.blake2b.is_some()),
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
        if let Some(blake2b) = value.blake2b {
            digests.push(HashDigest {
                algorithm: HashAlgorithm::Blake2b,
                digest: blake2b,
            });
        }
        Self::from(digests)
    }
}

impl From<HashDigests> for Hashes {
    fn from(value: HashDigests) -> Self {
        let mut hashes = Self::default();
        for digest in value {
            match digest.algorithm() {
                HashAlgorithm::Md5 => hashes.md5 = Some(digest.digest),
                HashAlgorithm::Sha256 => hashes.sha256 = Some(digest.digest),
                HashAlgorithm::Sha384 => hashes.sha384 = Some(digest.digest),
                HashAlgorithm::Sha512 => hashes.sha512 = Some(digest.digest),
                HashAlgorithm::Blake2b => hashes.blake2b = Some(digest.digest),
            }
        }
        hashes
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
        "Unsupported hash algorithm (expected one of: `md5`, `sha256`, `sha384`, `sha512`, or `blake2b`) on: `{0}`"
    )]
    UnsupportedHashAlgorithm(String),
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{CoreMetadata, HashError, Hashes, PypiSimpleDetail, Status};

    fn parse_pypi_simple_json(json: &str) -> Result<PypiSimpleDetail, serde_json::Error> {
        let expected: PypiSimpleDetail = serde_json::from_str(json)?;
        let actual = PypiSimpleDetail::from_json(json.as_bytes())?;

        assert_eq!(format!("{actual:#?}"), format!("{expected:#?}"));

        Ok(actual)
    }

    #[test]
    fn parse_hashes() -> Result<(), HashError> {
        let hashes: Hashes =
            "blake2b:af4793213ee66ef8fae3b93b3e29206f6b251e65c97bd91d8e1c5596ef15af0a".parse()?;
        assert_eq!(
            hashes,
            Hashes {
                md5: None,
                sha256: None,
                sha384: None,
                sha512: None,
                blake2b: Some(
                    "af4793213ee66ef8fae3b93b3e29206f6b251e65c97bd91d8e1c5596ef15af0a".into()
                ),
            }
        );

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
                blake2b: None,
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
                sha512: None,
                blake2b: None,
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
                sha512: None,
                blake2b: None,
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
                sha512: None,
                blake2b: None,
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

    #[test]
    fn parse_pypi_simple_json_response() -> Result<(), serde_json::Error> {
        let json = r#"
        {
            "ignored-before-files": {"nested": [true, null, 42, {"value": "ignored"}]},
            "project-status": {
                "reason": "This project is no longer maintained.",
                "status": "archived",
                "ignored": false
            },
            "files": [
                {
                    "requires-python": ">=3.8",
                    "unknown": {"nested": [false, null, "ignored"]},
                    "data-dist-info-metadata": null,
                    "core-metadata": {"sha256": "metadata-sha256"},
                    "dist-info-metadata": false,
                    "filename": "example-1.0.0.tar.gz",
                    "hashes": {
                        "unknown": {"nested": [true, null]},
                        "sha512": "file-sha512",
                        "blake2b": "file-blake2b",
                        "sha384": "file-sha384",
                        "md5": "file-md5",
                        "sha256": "file-sha256"
                    },
                    "upload-time": "2022-08-04T10:42:02.190074Z",
                    "size": 424460,
                    "url": "https://example.com/example-1.0.0.tar.gz",
                    "yanked": "Withdrawn for testing"
                },
                {
                    "hashes": {"sha256": "wheel-sha256"},
                    "url": "https://example.com/example-1.0.0-py3-none-any.whl",
                    "yanked": false,
                    "requires-python": ">=3.8",
                    "dist-info-metadata": true,
                    "filename": "example-1.0.0-py3-none-any.whl"
                }
            ],
            "meta": {"api-version": "1.4", "_last-serial": 15765070},
            "name": "example",
            "versions": ["1.0.0"]
        }
        "#;

        let detail = parse_pypi_simple_json(json)?;
        let [source, wheel] = detail.files.as_slice() else {
            return Err(serde::de::Error::custom("expected two files"));
        };
        let (Some(Ok(source_requires_python)), Some(Ok(wheel_requires_python))) = (
            source.requires_python.as_ref(),
            wheel.requires_python.as_ref(),
        ) else {
            return Err(serde::de::Error::custom("missing requires-python"));
        };
        assert!(Arc::ptr_eq(source_requires_python, wheel_requires_python));

        Ok(())
    }

    #[test]
    fn parse_pypi_simple_json_metadata_aliases() -> Result<(), serde_json::Error> {
        let cases = [
            (
                r#""core-metadata": null, "dist-info-metadata": true, "data-dist-info-metadata": false"#,
                Some(true),
            ),
            (
                r#""data-dist-info-metadata": false, "core-metadata": "ignored""#,
                Some(false),
            ),
            (
                r#""dist-info-metadata": {"sha256": "metadata"}, "core-metadata": true"#,
                Some(true),
            ),
            (r#""core-metadata": null, "dist-info-metadata": null"#, None),
        ];

        for (metadata, expected) in cases {
            let json = format!(
                r#"{{"files":[{{{metadata},"filename":"example-1.0.tar.gz","hashes":{{}},"url":"https://example.com/example-1.0.tar.gz"}}]}}"#
            );
            let detail = parse_pypi_simple_json(&json)?;

            assert_eq!(
                detail.files[0]
                    .core_metadata
                    .as_ref()
                    .map(CoreMetadata::is_available),
                expected,
                "unexpected metadata for {metadata}"
            );
        }

        Ok(())
    }

    #[test]
    fn parse_pypi_simple_json_project_status() -> Result<(), serde_json::Error> {
        let cases = [
            (r#"{"files":[]}"#, Status::Active, None),
            (r#"{"files":[],"project-status":{}}"#, Status::Active, None),
            (
                r#"{"files":[],"project-status":{"status":"unknown","reason":null}}"#,
                Status::Active,
                None,
            ),
            (
                r#"{"project-status":{"status":"quarantined","reason":"unsafe"},"files":[]}"#,
                Status::Quarantined,
                Some("unsafe"),
            ),
            (
                r#"{"files":[],"project-status":{"status":"deprecated"}}"#,
                Status::Deprecated,
                None,
            ),
        ];

        for (json, expected_status, expected_reason) in cases {
            let detail = parse_pypi_simple_json(json)?;

            assert_eq!(detail.project_status.status, expected_status);
            assert_eq!(detail.project_status.reason.as_deref(), expected_reason);
        }

        Ok(())
    }

    #[test]
    fn parse_pypi_simple_json_escaped_strings() -> Result<(), serde_json::Error> {
        let json = r#"
        {
            "igno\u0072ed": {"escaped": "\uD83D\uDE80 \"quoted\"\n"},
            "project-status": {
                "status": "archived",
                "reason": "Archived \uD83D\uDE80"
            },
            "fi\u006ces": [
                {
                    "filename": "example-\u0031.0.tar.gz",
                    "hashes": {"sha\u003256": "escaped-hash"},
                    "requires-python": "\u003e=3.8",
                    "url": "https:\/\/example.com\/example-1.0.tar.gz",
                    "yanked": "Quoted \"reason\"\nsecond line"
                }
            ]
        }
        "#;

        let detail = parse_pypi_simple_json(json)?;

        assert_eq!(detail.project_status.reason.as_deref(), Some("Archived 🚀"));
        assert_eq!(detail.files[0].filename.as_ref(), "example-1.0.tar.gz");

        Ok(())
    }

    #[test]
    fn parse_pypi_simple_json_optional_fields() -> Result<(), serde_json::Error> {
        let json = r#"
        {
            "files": [
                {
                    "filename": "overwritten.tar.gz",
                    "filename": "example-1.0.tar.gz",
                    "hashes": {"sha256": null, "unsupported": [true, null, {}]},
                    "requires-python": null,
                    "core-metadata": null,
                    "url": "https://example.com/example-1.0.tar.gz"
                },
                {
                    "filename": "example-2.0.tar.gz",
                    "hashes": {},
                    "requires-python": ">=3.8,,<4",
                    "url": "https://example.com/example-2.0.tar.gz"
                }
            ]
        }
        "#;

        let detail = parse_pypi_simple_json(json)?;
        assert!(
            detail.files[1]
                .requires_python
                .as_ref()
                .is_some_and(Result::is_err)
        );

        Ok(())
    }

    #[test]
    fn parse_pypi_simple_json_rejects_invalid_input() {
        let invalid = [
            "",
            "null",
            "{}",
            r#"{"files":null}"#,
            r#"{"files":[null]}"#,
            r#"{"files":[],}"#,
            r#"{"files":[]} trailing"#,
            r#"{"files":[],"files":[]}"#,
            r#"{"files":[],"ignored":01}"#,
            r#"{"files":[],"ignored":1.}"#,
            r#"{"files":[],"ignored":1e+}"#,
            r#"{"files":[],"ignored":+1}"#,
            r#"{"files":[],"ignored":tru}"#,
            r#"{"files":[],"ignored":{"nested":[true,]}}"#,
            "{\"files\":[],\"ignored\":\"line\nbreak\"}",
            r#"{"files":[],"project-status":null}"#,
            r#"{"files":[],"project-status":{"status":null}}"#,
            r#"{"files":[],"project-status":{"reason":42}}"#,
            r#"{"files":[],"project-status":{},"project-status":{}}"#,
            r#"{"files":[{"hashes":{},"url":"file"}]}"#,
            r#"{"files":[{"filename":"file","url":"file"}]}"#,
            r#"{"files":[{"filename":"file","hashes":{}}]}"#,
            r#"{"files":[{"filename":null,"hashes":{},"url":"file"}]}"#,
            r#"{"files":[{"filename":"file","hashes":null,"url":"file"}]}"#,
            r#"{"files":[{"filename":"file","hashes":{},"url":null}]}"#,
            r#"{"files":[{"filename":"file","hashes":{"sha256":42},"url":"file"}]}"#,
            r#"{"files":[{"filename":"file","hashes":{"sha256":"one","sha256":"two"},"url":"file"}]}"#,
            r#"{"files":[{"filename":"file","hashes":{},"url":"file","size":null}]}"#,
            r#"{"files":[{"filename":"file","hashes":{},"url":"file","size":-1}]}"#,
            r#"{"files":[{"filename":"file","hashes":{},"url":"file","size":1.5}]}"#,
            r#"{"files":[{"filename":"file","hashes":{},"url":"file","size":18446744073709551616}]}"#,
            r#"{"files":[{"filename":"file","hashes":{},"url":"file","upload-time":null}]}"#,
            r#"{"files":[{"filename":"file","hashes":{},"url":"file","upload-time":"invalid"}]}"#,
            r#"{"files":[{"filename":"file","hashes":{},"url":"file","requires-python":false}]}"#,
            r#"{"files":[{"filename":"file","hashes":{},"url":"file","core-metadata":42}]}"#,
            r#"{"files":[{"filename":"file","hashes":{},"url":"file","yanked":null}]}"#,
            r#"{"files":[{"filename":"\uD800","hashes":{},"url":"file"}]}"#,
        ];

        for json in invalid {
            let expected = serde_json::from_str::<PypiSimpleDetail>(json);
            let actual = PypiSimpleDetail::from_json(json.as_bytes());

            assert!(expected.is_err(), "Serde unexpectedly accepted {json}");
            assert!(
                actual.is_err(),
                "the dedicated parser unexpectedly accepted {json}"
            );
        }

        let invalid_utf8 = b"{\"files\":[{\"filename\":\"\xff\",\"hashes\":{},\"url\":\"file\"}]}";
        assert!(serde_json::from_slice::<PypiSimpleDetail>(invalid_utf8).is_err());
        assert!(PypiSimpleDetail::from_json(invalid_utf8).is_err());

        let excessive_nesting = format!(
            "{{\"files\":[],\"ignored\":{}null{}}}",
            "[".repeat(128),
            "]".repeat(128)
        );
        assert!(parse_pypi_simple_json(&excessive_nesting).is_ok());
    }
}

/// Response from the Simple API root endpoint (index) listing all available projects,
/// as served by the `vnd.pypi.simple.v1` media type.
///
/// <https://peps.python.org/pep-0691/#specification>
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PypiSimpleIndex {
    /// The list of projects available in the index.
    projects: Vec<ProjectEntry>,
}

/// Response from the Pyx Simple API root endpoint listing all available projects,
/// as served by the `vnd.pyx.simple.v1` media types.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PyxSimpleIndex {
    /// The list of projects available in the index.
    projects: Vec<ProjectEntry>,
}

/// A single project entry in the Simple API index.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct ProjectEntry {
    /// The name of the project.
    name: PackageName,
}

impl PypiSimpleIndex {
    /// Return the project names in the index.
    pub fn into_project_names(self) -> Vec<PackageName> {
        self.projects.into_iter().map(|entry| entry.name).collect()
    }
}

impl PyxSimpleIndex {
    /// Return the project names in the index.
    pub fn into_project_names(self) -> Vec<PackageName> {
        self.projects.into_iter().map(|entry| entry.name).collect()
    }
}
