use blake2::digest::consts::U32;
use rustc_hash::FxHashMap;
use sha2::Digest;
use std::cmp::Ordering;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncReadExt, ReadBuf};

use crate::Error;
use uv_pypi_types::{HashAlgorithm, HashDigest};

#[derive(Debug)]
pub enum Hasher {
    Md5(md5::Md5),
    Sha256(sha2::Sha256),
    Sha384(sha2::Sha384),
    Sha512(sha2::Sha512),
    Blake2b(blake2::Blake2b<U32>),
}

impl Hasher {
    fn update(&mut self, data: &[u8]) {
        match self {
            Self::Md5(hasher) => hasher.update(data),
            Self::Sha256(hasher) => hasher.update(data),
            Self::Sha384(hasher) => hasher.update(data),
            Self::Sha512(hasher) => hasher.update(data),
            Self::Blake2b(hasher) => hasher.update(data),
        }
    }
}

impl From<HashAlgorithm> for Hasher {
    fn from(algorithm: HashAlgorithm) -> Self {
        match algorithm {
            HashAlgorithm::Md5 => Self::Md5(md5::Md5::new()),
            HashAlgorithm::Sha256 => Self::Sha256(sha2::Sha256::new()),
            HashAlgorithm::Sha384 => Self::Sha384(sha2::Sha384::new()),
            HashAlgorithm::Sha512 => Self::Sha512(sha2::Sha512::new()),
            HashAlgorithm::Blake2b => Self::Blake2b(blake2::Blake2b::new()),
        }
    }
}

impl From<Hasher> for HashDigest {
    fn from(hasher: Hasher) -> Self {
        match hasher {
            Hasher::Md5(hasher) => Self {
                algorithm: HashAlgorithm::Md5,
                digest: format!("{:x}", hasher.finalize()).into(),
            },
            Hasher::Sha256(hasher) => Self {
                algorithm: HashAlgorithm::Sha256,
                digest: format!("{:x}", hasher.finalize()).into(),
            },
            Hasher::Sha384(hasher) => Self {
                algorithm: HashAlgorithm::Sha384,
                digest: format!("{:x}", hasher.finalize()).into(),
            },
            Hasher::Sha512(hasher) => Self {
                algorithm: HashAlgorithm::Sha512,
                digest: format!("{:x}", hasher.finalize()).into(),
            },
            Hasher::Blake2b(hasher) => Self {
                algorithm: HashAlgorithm::Blake2b,
                digest: format!("{:x}", hasher.finalize()).into(),
            },
        }
    }
}

/// A digest of extracted directory contents, hex-encoded.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DirectoryDigest(String);

impl DirectoryDigest {
    /// Create a new [`DirectoryDigest`] from a hex-encoded string.
    pub(crate) fn new(hex: String) -> Self {
        Self(hex)
    }

    /// Return the hex-encoded digest string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DirectoryDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

struct DirectoryDigestFile<'a> {
    path: String,
    size: u64,
    executable: bool,
    digest: DirectoryFileDigest<'a>,
}

impl<'a> DirectoryDigestFile<'a> {
    fn from_blake3(path: String, size: u64, executable: bool, digest: blake3::Hash) -> Self {
        Self {
            path,
            size,
            executable,
            digest: DirectoryFileDigest::Blake3(digest),
        }
    }

    fn from_record(path: String, size: u64, executable: bool, hash: &'a str, crc32: u32) -> Self {
        Self {
            path,
            size,
            executable,
            digest: DirectoryFileDigest::Record { hash, crc32 },
        }
    }
}

enum DirectoryFileDigest<'a> {
    Blake3(blake3::Hash),
    Record { hash: &'a str, crc32: u32 },
}

impl DirectoryFileDigest<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Blake3(left), Self::Blake3(right)) => left.as_bytes().cmp(right.as_bytes()),
            (
                Self::Record {
                    hash: left_hash,
                    crc32: left_crc32,
                },
                Self::Record {
                    hash: right_hash,
                    crc32: right_crc32,
                },
            ) => left_hash
                .cmp(right_hash)
                .then_with(|| left_crc32.cmp(right_crc32)),
            (Self::Blake3(_), Self::Record { .. }) => Ordering::Less,
            (Self::Record { .. }, Self::Blake3(_)) => Ordering::Greater,
        }
    }

    fn update(&self, hasher: &mut blake3::Hasher) {
        match self {
            Self::Blake3(digest) => {
                hasher.update(b"blake3\0");
                hasher.update(digest.as_bytes());
            }
            Self::Record { hash, crc32 } => {
                hasher.update(b"record\0");
                update_bytes(hasher, hash.as_bytes());
                hasher.update(&crc32.to_le_bytes());
            }
        }
    }
}

pub(crate) struct ExtractedDirectoryFile {
    path: PathBuf,
    canonical_path: String,
    size: u64,
    executable: bool,
    crc32: u32,
    digest: Option<blake3::Hash>,
}

impl ExtractedDirectoryFile {
    pub(crate) fn new(
        path: PathBuf,
        size: u64,
        executable: bool,
        crc32: u32,
        digest: Option<blake3::Hash>,
    ) -> Self {
        let canonical_path = canonical_path(&path);
        Self::from_canonical_path(path, canonical_path, size, executable, crc32, digest)
    }

    pub(crate) fn from_canonical_path(
        path: PathBuf,
        canonical_path: String,
        size: u64,
        executable: bool,
        crc32: u32,
        digest: Option<blake3::Hash>,
    ) -> Self {
        Self {
            path,
            canonical_path,
            size,
            executable,
            crc32,
            digest,
        }
    }

    fn is_record(&self) -> bool {
        Self::is_record_path(&self.canonical_path)
    }

    fn is_record_path(path: &str) -> bool {
        path.ends_with(".dist-info/RECORD")
    }

    fn into_directory_digest_file<'a>(
        self,
        target: &Path,
        record_hashes: &'a RecordDirectoryHashes,
    ) -> Result<DirectoryDigestFile<'a>, Error> {
        if let Some(hash) = record_hashes.get(&self.canonical_path, self.size) {
            return Ok(DirectoryDigestFile::from_record(
                self.canonical_path,
                self.size,
                self.executable,
                hash,
                self.crc32,
            ));
        }

        let digest = match self.digest {
            Some(digest) => digest,
            None => hash_file(&target.join(&self.path))?,
        };

        Ok(DirectoryDigestFile::from_blake3(
            self.canonical_path,
            self.size,
            self.executable,
            digest,
        ))
    }
}

#[derive(Debug, Default)]
pub(crate) struct RecordDirectoryHashes {
    hashes: FxHashMap<String, RecordHash>,
}

#[derive(Debug)]
struct RecordHash {
    hash: String,
    size: Option<u64>,
}

impl RecordDirectoryHashes {
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    pub(crate) fn from_reader(reader: impl Read) -> Result<Self, Error> {
        let mut hashes = FxHashMap::default();
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(reader);

        for record in reader.records() {
            let record =
                record.map_err(|err| Error::Io(io::Error::new(io::ErrorKind::InvalidData, err)))?;
            let Some(path) = record.get(0).and_then(record_canonical_path) else {
                continue;
            };
            let Some(hash) = record.get(1).filter(is_valid_record_hash) else {
                continue;
            };
            let size = match record.get(2).filter(|size| !size.is_empty()) {
                Some(size) => {
                    let Ok(size) = size.parse::<u64>() else {
                        continue;
                    };
                    Some(size)
                }
                None => None,
            };
            hashes.insert(
                path,
                RecordHash {
                    hash: hash.to_string(),
                    size,
                },
            );
        }

        Ok(Self { hashes })
    }

    pub(crate) fn from_extracted_files(
        target: &Path,
        files: &[ExtractedDirectoryFile],
    ) -> Result<Self, Error> {
        let Some(record_file) = files.iter().find(|file| file.is_record()) else {
            return Ok(Self::empty());
        };
        let file = fs_err::File::open(target.join(&record_file.path)).map_err(Error::Io)?;
        Self::from_reader(file)
    }

    pub(crate) fn has_usable_canonical_hash(&self, path: &str, size: u64) -> bool {
        self.get(path, size).is_some()
    }

    fn get(&self, path: &str, size: u64) -> Option<&str> {
        let hash = self.hashes.get(path)?;
        if hash.size.is_some_and(|expected| expected != size) {
            return None;
        }
        Some(&hash.hash)
    }
}

pub(crate) fn directory_digest(
    target: &Path,
    files: Vec<ExtractedDirectoryFile>,
    record_hashes: &RecordDirectoryHashes,
) -> Result<DirectoryDigest, Error> {
    let mut files = files
        .into_iter()
        .map(|file| file.into_directory_digest_file(target, record_hashes))
        .collect::<Result<Vec<_>, Error>>()?;

    files.sort_unstable_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.size.cmp(&right.size))
            .then_with(|| left.executable.cmp(&right.executable))
            .then_with(|| left.digest.cmp(&right.digest))
    });

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"uv-extract-directory-digest-v2\0");
    for file in files {
        hasher.update(b"file\0");
        update_bytes(&mut hasher, file.path.as_bytes());
        hasher.update(&file.size.to_le_bytes());
        hasher.update(&[u8::from(file.executable)]);
        file.digest.update(&mut hasher);
    }
    Ok(DirectoryDigest::new(hasher.finalize().to_hex().to_string()))
}

fn update_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    hasher.update(&len.to_le_bytes());
    hasher.update(bytes);
}

pub(crate) fn canonical_path(path: &Path) -> String {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(component) => {
                components.push(component.to_string_lossy());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                components.pop();
            }
            Component::Prefix(_) | Component::RootDir => {}
        }
    }
    components.join("/")
}

fn record_canonical_path(path: &str) -> Option<String> {
    if path.contains('\0') {
        return None;
    }
    let path = PathBuf::from(path);
    let mut depth = 0usize;
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => return None,
            Component::ParentDir => depth = depth.checked_sub(1)?,
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
        }
    }
    Some(canonical_path(&path))
}

fn is_valid_record_hash(hash: &&str) -> bool {
    let Some((algorithm, digest)) = hash.split_once('=') else {
        return false;
    };
    !algorithm.is_empty() && !digest.is_empty()
}

fn hash_file(path: &Path) -> Result<blake3::Hash, Error> {
    let mut file = fs_err::File::open(path).map_err(Error::Io)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0; 128 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(Error::Io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize())
}

pub struct HashReader<'a, R> {
    reader: R,
    hashers: &'a mut [Hasher],
}

impl<'a, R> HashReader<'a, R>
where
    R: tokio::io::AsyncRead + Unpin,
{
    pub fn new(reader: R, hashers: &'a mut [Hasher]) -> Self {
        HashReader { reader, hashers }
    }

    /// Exhaust the underlying reader.
    pub async fn finish(&mut self) -> Result<(), std::io::Error> {
        while self.read(&mut vec![0; 8192]).await? > 0 {}

        Ok(())
    }
}

impl<R> tokio::io::AsyncRead for HashReader<'_, R>
where
    R: tokio::io::AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let reader = Pin::new(&mut self.reader);
        match reader.poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                for hasher in self.hashers.iter_mut() {
                    hasher.update(buf.filled());
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}
