use std::marker::PhantomData;
use std::mem::size_of;
use std::str::FromStr;
use std::sync::Arc;

use jiff::Timestamp;
use rustc_hash::FxHashMap;
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers, VersionSpecifiersParseError};
use uv_pep508::Requirement;
use uv_small_str::SmallString;

use crate::lenient_requirement::LenientVersionSpecifiers;
use crate::{ProjectStatus, VerbatimParsedUrl};

/// A collection of "files" from `PyPI`'s JSON API for a single package, as served by the
/// `vnd.pypi.simple.v1` media type.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PypiSimpleDetail {
    /// PEP 792 project status information.
    #[serde(default)]
    pub project_status: ProjectStatus,
    /// The list of [`PypiFile`]s available for download.
    #[serde(deserialize_with = "deserialize_files")]
    pub files: Vec<PypiFile>,
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

trait SimpleFile: Sized {
    fn deserialize_with_interner<'de, D>(
        deserializer: D,
        interner: &mut RequiresPythonInterner,
    ) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>;
}

struct SimpleFileSeed<'a, T> {
    interner: &'a mut RequiresPythonInterner,
    marker: PhantomData<T>,
}

impl<'de, T> DeserializeSeed<'de> for SimpleFileSeed<'_, T>
where
    T: SimpleFile,
{
    type Value = T;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize_with_interner(deserializer, self.interner)
    }
}

struct SimpleFilesVisitor<T>(PhantomData<T>);

impl<'de, T> Visitor<'de> for SimpleFilesVisitor<T>
where
    T: SimpleFile,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a sequence of files")
    }

    fn visit_seq<A>(self, mut access: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        // Match Serde's Vec deserializer by limiting untrusted upfront allocations to 1 MiB.
        let capacity = access
            .size_hint()
            .unwrap_or_default()
            .min(1024 * 1024 / size_of::<T>());
        let mut files = Vec::with_capacity(capacity);
        let mut interner = RequiresPythonInterner::default();

        while let Some(file) = access.next_element_seed(SimpleFileSeed {
            interner: &mut interner,
            marker: PhantomData,
        })? {
            files.push(file);
        }

        Ok(files)
    }
}

/// Deserialize files while parsing each distinct `requires-python` value only once.
fn deserialize_files<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: SimpleFile,
{
    deserializer.deserialize_seq(SimpleFilesVisitor(PhantomData))
}

struct RequiresPythonSeed<'a>(&'a mut RequiresPythonInterner);

impl<'de> DeserializeSeed<'de> for RequiresPythonSeed<'_> {
    type Value = Option<RequiresPythonResult>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_option(self)
    }
}

impl<'de> Visitor<'de> for RequiresPythonSeed<'_> {
    type Value = Option<RequiresPythonResult>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("an optional Python version specifier")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(None)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(self)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Some(self.0.parse(value)))
    }
}

impl<'de> Deserialize<'de> for PypiFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut interner = RequiresPythonInterner::default();
        Self::deserialize_with_interner(deserializer, &mut interner)
    }
}

impl SimpleFile for PypiFile {
    fn deserialize_with_interner<'de, D>(
        deserializer: D,
        interner: &mut RequiresPythonInterner,
    ) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(PypiFileVisitor { interner })
    }
}

struct PypiFileVisitor<'a> {
    interner: &'a mut RequiresPythonInterner,
}

impl<'de> Visitor<'de> for PypiFileVisitor<'_> {
    type Value = PypiFile;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a map containing file metadata")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
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
                    requires_python =
                        access.next_value_seed(RequiresPythonSeed(&mut *self.interner))?;
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

        Ok(PypiFile {
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
    #[serde(deserialize_with = "deserialize_files")]
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

impl<'de> Deserialize<'de> for PyxFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut interner = RequiresPythonInterner::default();
        Self::deserialize_with_interner(deserializer, &mut interner)
    }
}

impl SimpleFile for PyxFile {
    fn deserialize_with_interner<'de, D>(
        deserializer: D,
        interner: &mut RequiresPythonInterner,
    ) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(PyxFileVisitor { interner })
    }
}

struct PyxFileVisitor<'a> {
    interner: &'a mut RequiresPythonInterner,
}

impl<'de> Visitor<'de> for PyxFileVisitor<'_> {
    type Value = PyxFile;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a map containing file metadata")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
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
                    requires_python =
                        access.next_value_seed(RequiresPythonSeed(&mut *self.interner))?;
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

        Ok(PyxFile {
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
    pub md5: Option<Digest<16>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<Digest<32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha384: Option<Digest<48>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha512: Option<Digest<64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blake2b: Option<Digest<32>>,
}

impl Hashes {
    /// Parse the hash from a fragment, as in: `sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61`
    pub fn parse_fragment(fragment: &str) -> Result<Self, HashError> {
        let mut parts = fragment.split('=');

        if let Some(name) = parts.next()
            && let Some(value) = parts.next()
            && let None = parts.next()
        {
            let algorithm = HashAlgorithm::from_str(name)
                .map_err(|_| HashError::UnsupportedHashAlgorithm(fragment.to_string()))?;
            Ok(Self::from(HashDigest::new(algorithm, value)?))
        } else {
            Err(HashError::InvalidFragment(fragment.to_string()))
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

        let algorithm = HashAlgorithm::from_str(name)
            .map_err(|_| HashError::UnsupportedHashAlgorithm(s.to_string()))?;
        Ok(Self::from(HashDigest::new(algorithm, value)?))
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
    Blake2b256,
}

impl HashAlgorithm {
    /// Return the supported [`HashAlgorithm`] variants in order of preference.
    pub(crate) fn preferred() -> impl Iterator<Item = Self> {
        [
            Self::Sha512,
            Self::Sha384,
            Self::Sha256,
            Self::Blake2b256,
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
            Self::Blake2b256 => "blake2b",
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
            "blake2b" => Ok(Self::Blake2b256),
            _ => Err(HashError::UnsupportedHashAlgorithm(s.to_string())),
        }
    }
}

impl std::fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A validated, lowercase hexadecimal digest containing exactly `BYTES` bytes.
#[derive(
    Clone,
    Ord,
    PartialOrd,
    Eq,
    PartialEq,
    Hash,
    Serialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[serde(transparent)]
#[rkyv(derive(Debug))]
pub struct Digest<const BYTES: usize>(SmallString);

impl<const BYTES: usize> Digest<BYTES> {
    /// Validate a hexadecimal digest and normalize it to lowercase.
    pub fn from_hex(digest: impl Into<SmallString>) -> Result<Self, HashError> {
        let digest = digest.into();
        if digest.len() != BYTES * 2 {
            return Err(HashError::InvalidDigestLength {
                expected: BYTES * 2,
                actual: digest.len(),
            });
        }
        if !digest.as_bytes().iter().all(u8::is_ascii_hexdigit) {
            return Err(HashError::InvalidDigestCharacters(digest.to_string()));
        }

        if digest.as_bytes().iter().any(u8::is_ascii_uppercase) {
            Ok(Self(SmallString::from(digest.to_ascii_lowercase())))
        } else {
            Ok(Self(digest))
        }
    }

    /// Encode the exact number of digest bytes as lowercase hexadecimal.
    pub fn from_bytes(bytes: [u8; BYTES]) -> Self {
        const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

        let mut digest = String::with_capacity(BYTES * 2);
        for byte in bytes {
            digest.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            digest.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        Self(digest.into())
    }

    /// Return the lowercase hexadecimal digest.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Decode the validated hexadecimal digest into its fixed-size byte array.
    pub fn decode(&self) -> [u8; BYTES] {
        let mut decoded = [0; BYTES];
        for (index, pair) in self.0.as_bytes().chunks_exact(2).enumerate() {
            let decode_digit = |digit: u8| {
                if digit.is_ascii_digit() {
                    digit - b'0'
                } else {
                    digit - b'a' + 10
                }
            };
            decoded[index] = (decode_digit(pair[0]) << 4) | decode_digit(pair[1]);
        }
        decoded
    }
}

impl<const BYTES: usize> std::fmt::Debug for Digest<BYTES> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

impl<'de, const BYTES: usize> Deserialize<'de> for Digest<BYTES> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let digest = SmallString::deserialize(deserializer)?;
        Self::from_hex(digest).map_err(serde::de::Error::custom)
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
pub enum HashDigest {
    Md5(Digest<16>),
    Sha256(Digest<32>),
    Sha384(Digest<48>),
    Sha512(Digest<64>),
    Blake2b256(Digest<32>),
}

impl HashDigest {
    /// Validate and normalize a digest for the given [`HashAlgorithm`].
    pub fn new(
        algorithm: HashAlgorithm,
        digest: impl Into<SmallString>,
    ) -> Result<Self, HashError> {
        let digest = digest.into();
        match algorithm {
            HashAlgorithm::Md5 => Ok(Self::Md5(Digest::from_hex(digest)?)),
            HashAlgorithm::Sha256 => Ok(Self::Sha256(Digest::from_hex(digest)?)),
            HashAlgorithm::Sha384 => Ok(Self::Sha384(Digest::from_hex(digest)?)),
            HashAlgorithm::Sha512 => Ok(Self::Sha512(Digest::from_hex(digest)?)),
            HashAlgorithm::Blake2b256 => Ok(Self::Blake2b256(Digest::from_hex(digest)?)),
        }
    }

    /// Return the [`HashAlgorithm`] of the digest.
    pub fn algorithm(&self) -> HashAlgorithm {
        match self {
            Self::Md5(_) => HashAlgorithm::Md5,
            Self::Sha256(_) => HashAlgorithm::Sha256,
            Self::Sha384(_) => HashAlgorithm::Sha384,
            Self::Sha512(_) => HashAlgorithm::Sha512,
            Self::Blake2b256(_) => HashAlgorithm::Blake2b256,
        }
    }

    /// Return the hex-encoded digest.
    pub fn digest(&self) -> &str {
        match self {
            Self::Md5(digest) => digest.as_str(),
            Self::Sha256(digest) | Self::Blake2b256(digest) => digest.as_str(),
            Self::Sha384(digest) => digest.as_str(),
            Self::Sha512(digest) => digest.as_str(),
        }
    }
}

impl std::fmt::Display for HashDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.algorithm(), self.digest())
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

        Self::new(HashAlgorithm::from_str(name)?, value)
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
            digests.push(HashDigest::Sha512(sha512));
        }
        if let Some(sha384) = value.sha384 {
            digests.push(HashDigest::Sha384(sha384));
        }
        if let Some(sha256) = value.sha256 {
            digests.push(HashDigest::Sha256(sha256));
        }
        if let Some(md5) = value.md5 {
            digests.push(HashDigest::Md5(md5));
        }
        if let Some(blake2b) = value.blake2b {
            digests.push(HashDigest::Blake2b256(blake2b));
        }
        Self::from(digests)
    }
}

impl From<HashDigest> for Hashes {
    fn from(value: HashDigest) -> Self {
        let mut hashes = Self::default();
        match value {
            HashDigest::Md5(digest) => hashes.md5 = Some(digest),
            HashDigest::Sha256(digest) => hashes.sha256 = Some(digest),
            HashDigest::Sha384(digest) => hashes.sha384 = Some(digest),
            HashDigest::Sha512(digest) => hashes.sha512 = Some(digest),
            HashDigest::Blake2b256(digest) => hashes.blake2b = Some(digest),
        }
        hashes
    }
}

impl From<HashDigests> for Hashes {
    fn from(value: HashDigests) -> Self {
        let mut hashes = Self::default();
        for digest in value {
            match digest {
                HashDigest::Md5(digest) => hashes.md5 = Some(digest),
                HashDigest::Sha256(digest) => hashes.sha256 = Some(digest),
                HashDigest::Sha384(digest) => hashes.sha384 = Some(digest),
                HashDigest::Sha512(digest) => hashes.sha512 = Some(digest),
                HashDigest::Blake2b256(digest) => hashes.blake2b = Some(digest),
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
        "Invalid hash digest length (expected {expected} hexadecimal characters, found {actual})"
    )]
    InvalidDigestLength { expected: usize, actual: usize },

    #[error("Invalid hash digest (expected only hexadecimal characters): `{0}`")]
    InvalidDigestCharacters(String),

    #[error(
        "Unsupported hash algorithm (expected one of: `md5`, `sha256`, `sha384`, `sha512`, or `blake2b`) on: `{0}`"
    )]
    UnsupportedHashAlgorithm(String),
}

#[cfg(test)]
mod tests {
    use crate::{Digest, HashAlgorithm, HashDigest, HashDigests, HashError, Hashes};

    #[test]
    fn hash_digest_variants() -> Result<(), HashError> {
        let variants = [
            (HashAlgorithm::Md5, "md5", "Md5", 16),
            (HashAlgorithm::Sha256, "sha256", "Sha256", 32),
            (HashAlgorithm::Sha384, "sha384", "Sha384", 48),
            (HashAlgorithm::Sha512, "sha512", "Sha512", 64),
            (HashAlgorithm::Blake2b256, "blake2b", "Blake2b256", 32),
        ];

        for (algorithm, name, variant, bytes) in variants {
            let digest = "ab".repeat(bytes);
            let parsed = format!("{name}:{}", "aB".repeat(bytes)).parse::<HashDigest>()?;
            let expected = HashDigest::new(algorithm, digest.clone())?;

            assert_eq!(parsed, expected);
            assert_eq!(parsed.algorithm(), algorithm);
            assert_eq!(parsed.digest(), digest);
            assert_eq!(parsed.to_string(), format!("{name}:{digest}"));
            let serialized = serde_json::to_string(&parsed).expect("serialize hash digest");
            assert_eq!(serialized, format!(r#"{{"{variant}":"{digest}"}}"#));
            assert_eq!(
                serde_json::from_str::<HashDigest>(&serialized).expect("deserialize hash digest"),
                parsed
            );

            let uppercase = serde_json::from_str::<HashDigest>(&format!(
                r#"{{"{variant}":"{}"}}"#,
                digest.to_ascii_uppercase()
            ))
            .expect("deserialize uppercase hash digest");
            assert_eq!(uppercase, parsed);

            let hashes = serde_json::from_str::<Hashes>(&format!(
                r#"{{"{name}":"{}"}}"#,
                digest.to_ascii_uppercase()
            ))
            .expect("deserialize uppercase hash map");
            assert_eq!(hashes, Hashes::from(parsed));
        }

        Ok(())
    }

    #[test]
    fn hash_digest_rejects_invalid_digests() {
        for (algorithm, bytes) in [
            (HashAlgorithm::Md5, 16),
            (HashAlgorithm::Sha256, 32),
            (HashAlgorithm::Sha384, 48),
            (HashAlgorithm::Sha512, 64),
            (HashAlgorithm::Blake2b256, 32),
        ] {
            for digest in [
                String::new(),
                "a".repeat(bytes * 2 - 1),
                "a".repeat(bytes * 2 + 1),
            ] {
                assert!(matches!(
                    HashDigest::new(algorithm, digest),
                    Err(HashError::InvalidDigestLength { .. })
                ));
            }
            assert!(matches!(
                HashDigest::new(algorithm, "g".repeat(bytes * 2)),
                Err(HashError::InvalidDigestCharacters(_))
            ));
        }

        assert!(matches!(
            "sha256:digest:extra".parse::<HashDigest>(),
            Err(HashError::InvalidStructure(_))
        ));
        assert!(matches!(
            "sha1:digest".parse::<HashDigest>(),
            Err(HashError::UnsupportedHashAlgorithm(_))
        ));

        assert!(serde_json::from_str::<HashDigest>(r#"{"Sha256":"short"}"#).is_err());
        assert!(serde_json::from_str::<Hashes>(r#"{"sha256":"short"}"#).is_err());
        assert!(serde_json::from_str::<Digest<32>>(&format!(r#""{}""#, "g".repeat(64))).is_err());
    }

    #[test]
    fn hash_digests_round_trip_hashes() {
        let hashes = Hashes {
            md5: Some(Digest::from_bytes([0x11; 16])),
            sha256: Some(Digest::from_bytes([0x22; 32])),
            sha384: Some(Digest::from_bytes([0x33; 48])),
            sha512: Some(Digest::from_bytes([0x44; 64])),
            blake2b: Some(Digest::from_bytes([0x55; 32])),
        };
        let digests = HashDigests::from(hashes.clone());

        assert_eq!(
            digests.as_slice(),
            [
                HashDigest::Sha512(Digest::from_bytes([0x44; 64])),
                HashDigest::Sha384(Digest::from_bytes([0x33; 48])),
                HashDigest::Sha256(Digest::from_bytes([0x22; 32])),
                HashDigest::Md5(Digest::from_bytes([0x11; 16])),
                HashDigest::Blake2b256(Digest::from_bytes([0x55; 32])),
            ]
        );
        let mut sorted = digests.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted.iter().map(HashDigest::algorithm).collect::<Vec<_>>(),
            [
                HashAlgorithm::Md5,
                HashAlgorithm::Sha256,
                HashAlgorithm::Sha384,
                HashAlgorithm::Sha512,
                HashAlgorithm::Blake2b256,
            ]
        );
        assert_eq!(Hashes::from(digests), hashes);
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
                blake2b: Some(Digest::from_hex(
                    "af4793213ee66ef8fae3b93b3e29206f6b251e65c97bd91d8e1c5596ef15af0a"
                )?),
            }
        );

        let sha512 = "40".repeat(64);
        let hashes: Hashes = format!("sha512:{sha512}").parse()?;
        assert_eq!(
            hashes,
            Hashes {
                md5: None,
                sha256: None,
                sha384: None,
                sha512: Some(Digest::from_hex(sha512)?),
                blake2b: None,
            }
        );

        let sha384 = "40".repeat(48);
        let hashes: Hashes = format!("sha384:{sha384}").parse()?;
        assert_eq!(
            hashes,
            Hashes {
                md5: None,
                sha256: None,
                sha384: Some(Digest::from_hex(sha384)?),
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
                sha256: Some(Digest::from_hex(
                    "40627dcf047dadb22cd25ea7ecfe9cbf3bbbad0482ee5920b582f3809c97654f"
                )?),
                sha384: None,
                sha512: None,
                blake2b: None,
            }
        );

        let hashes: Hashes = "md5:090376d812fb6ac5f171e5938e82e7f2".parse()?;
        assert_eq!(
            hashes,
            Hashes {
                md5: Some(Digest::from_hex("090376d812fb6ac5f171e5938e82e7f2")?),
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
