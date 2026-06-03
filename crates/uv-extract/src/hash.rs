use blake2::digest::consts::U32;
use sha2::Digest;
use std::path::{Component, Path};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncReadExt, ReadBuf};

use rustc_hash::FxHashSet;
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

const DIRECTORY_DIGEST_PREFIX: &str = "dirhash-v1-";
const DIRECTORY_DIGEST_CONTEXT: &str = "uv directory digest v1";

const FRAME_EMPTY_DIRECTORY: u8 = 1;
const FRAME_FILE: u8 = 2;
const FRAME_PATH: u8 = 3;
const FRAME_SIZE: u8 = 4;
const FRAME_EXECUTABLE: u8 = 5;
const FRAME_CONTENT_BLAKE3: u8 = 6;

/// A versioned digest of extracted directory contents.
///
/// The digest is formatted as the path-safe `dirhash-v1-` prefix followed by the
/// lowercase hexadecimal BLAKE3 digest.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DirectoryDigest(String);

impl DirectoryDigest {
    /// Return the versioned digest string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) struct DirectoryDigestFile {
    path: String,
    size: u64,
    executable: bool,
    digest: blake3::Hash,
}

impl DirectoryDigestFile {
    pub(crate) fn new(path: &Path, size: u64, executable: bool, digest: blake3::Hash) -> Self {
        Self {
            path: canonical_path(path),
            size,
            executable,
            digest,
        }
    }
}

/// Return canonical paths for explicit directories that are empty in the extracted tree.
pub(crate) fn empty_directory_paths<'a>(
    directories: impl IntoIterator<Item = &'a Path>,
    files: impl IntoIterator<Item = &'a Path>,
) -> Vec<String> {
    let mut candidates = FxHashSet::default();
    let mut non_empty = FxHashSet::default();

    for directory in directories {
        let path = canonical_path(directory);
        if path.is_empty() {
            continue;
        }
        mark_canonical_parent_directories(&mut non_empty, &path);
        candidates.insert(path);
    }

    if candidates.is_empty() {
        return Vec::new();
    }

    for file in files {
        let path = canonical_path(file);
        mark_canonical_parent_directories(&mut non_empty, &path);
    }

    candidates
        .into_iter()
        .filter(|path| !non_empty.contains(path))
        .collect()
}

/// Compute a deterministic digest for extracted file and empty-directory entries.
///
/// The v1 construction uses BLAKE3's derive-key mode for scheme-level domain separation. Its input
/// is a sorted stream of type-length-value frames. Empty directories and files are distinct
/// top-level frame types, while each file contains separate path, size, executable, and content
/// digest frames. Each frame is encoded as a one-byte type, an eight-byte little-endian length,
/// and the frame value.
///
/// Empty leaf directories are included because this digest identifies uv's extracted-wheel cache,
/// where an empty directory can affect Python namespace-package behavior. Non-empty directories are
/// implied by their children. ZIP entries are never followed as symlinks during extraction; all
/// non-directory entries are materialized and hashed as regular files.
pub(crate) fn directory_digest(
    mut files: Vec<DirectoryDigestFile>,
    mut directories: Vec<String>,
) -> DirectoryDigest {
    directories.sort_unstable();
    files.sort_unstable_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.size.cmp(&right.size))
            .then_with(|| left.executable.cmp(&right.executable))
            .then_with(|| left.digest.as_bytes().cmp(right.digest.as_bytes()))
    });

    let mut hasher = blake3::Hasher::new_derive_key(DIRECTORY_DIGEST_CONTEXT);
    let mut entry = Vec::new();
    for directory in directories {
        entry.clear();
        append_frame(&mut entry, FRAME_PATH, directory.as_bytes());
        update_frame(&mut hasher, FRAME_EMPTY_DIRECTORY, &entry);
    }
    for file in files {
        entry.clear();
        append_frame(&mut entry, FRAME_PATH, file.path.as_bytes());
        append_frame(&mut entry, FRAME_SIZE, &file.size.to_le_bytes());
        append_frame(&mut entry, FRAME_EXECUTABLE, &[u8::from(file.executable)]);
        append_frame(&mut entry, FRAME_CONTENT_BLAKE3, file.digest.as_bytes());
        update_frame(&mut hasher, FRAME_FILE, &entry);
    }
    DirectoryDigest(format!(
        "{DIRECTORY_DIGEST_PREFIX}{}",
        hasher.finalize().to_hex()
    ))
}

/// Mark all canonical parent directories of a slash-separated path as non-empty.
fn mark_canonical_parent_directories(non_empty: &mut FxHashSet<String>, path: &str) {
    let mut path = path;
    while let Some((parent, _child)) = path.rsplit_once('/') {
        non_empty.insert(parent.to_string());
        path = parent;
    }
}

fn append_frame(output: &mut Vec<u8>, frame_type: u8, value: &[u8]) {
    output.extend_from_slice(&frame_header(frame_type, value.len()));
    output.extend_from_slice(value);
}

fn update_frame(hasher: &mut blake3::Hasher, frame_type: u8, value: &[u8]) {
    hasher.update(&frame_header(frame_type, value.len()));
    hasher.update(value);
}

fn frame_header(frame_type: u8, length: usize) -> [u8; 9] {
    let mut header = [0; 9];
    header[0] = frame_type;
    header[1..].copy_from_slice(&u64::try_from(length).unwrap_or(u64::MAX).to_le_bytes());
    header
}

fn canonical_path(path: &Path) -> String {
    let mut canonical = String::new();
    for component in path.components() {
        let Component::Normal(component) = component else {
            continue;
        };
        if !canonical.is_empty() {
            canonical.push('/');
        }
        canonical.push_str(component.to_string_lossy().as_ref());
    }
    canonical
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

#[cfg(test)]
mod directory_digest_tests {
    use std::collections::HashSet;
    use std::path::Path;

    use super::{
        DirectoryDigestFile, FRAME_CONTENT_BLAKE3, FRAME_EMPTY_DIRECTORY, FRAME_EXECUTABLE,
        FRAME_FILE, FRAME_PATH, FRAME_SIZE, directory_digest,
    };

    #[test]
    fn directory_digest_frame_types_are_distinct() {
        let frame_types = [
            FRAME_EMPTY_DIRECTORY,
            FRAME_FILE,
            FRAME_PATH,
            FRAME_SIZE,
            FRAME_EXECUTABLE,
            FRAME_CONTENT_BLAKE3,
        ];
        assert_eq!(
            frame_types.into_iter().collect::<HashSet<_>>().len(),
            frame_types.len()
        );
    }

    #[test]
    fn directory_digest_is_versioned_and_stable() {
        let digest = directory_digest(
            vec![digest_file("example/data.txt", b"contents", false)],
            vec!["example/empty".to_string()],
        );

        assert_eq!(
            digest.as_str(),
            "dirhash-v1-63bc70424ca849099b1d09325411e16b6bf8fb5f021b324c0ab438426bc3c637"
        );
    }

    #[test]
    fn directory_digest_frames_file_names() {
        let separate_files = directory_digest(
            vec![
                digest_file("foo.txt", b"", false),
                digest_file("bar.txt", b"", false),
            ],
            Vec::new(),
        );
        let newline_file = directory_digest(
            vec![digest_file("foo.txt\nbar.txt", b"", false)],
            Vec::new(),
        );

        assert_ne!(separate_files, newline_file);
    }

    fn digest_file(path: &str, contents: &[u8], executable: bool) -> DirectoryDigestFile {
        DirectoryDigestFile::new(
            Path::new(path),
            u64::try_from(contents.len()).unwrap_or(u64::MAX),
            executable,
            blake3::hash(contents),
        )
    }
}
