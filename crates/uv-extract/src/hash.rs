use blake2::digest::consts::U32;
use sha2::Digest;
use std::path::{Component, Path};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncReadExt, ReadBuf};

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

pub(crate) fn directory_digest(mut files: Vec<DirectoryDigestFile>) -> DirectoryDigest {
    files.sort_unstable_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.size.cmp(&right.size))
            .then_with(|| left.executable.cmp(&right.executable))
            .then_with(|| left.digest.as_bytes().cmp(right.digest.as_bytes()))
    });

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"uv-extract-directory-digest-v1\0");
    for file in files {
        hasher.update(b"file\0");
        update_bytes(&mut hasher, file.path.as_bytes());
        hasher.update(&file.size.to_le_bytes());
        hasher.update(&[u8::from(file.executable)]);
        hasher.update(file.digest.as_bytes());
    }
    DirectoryDigest::new(hasher.finalize().to_hex().to_string())
}

fn update_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    hasher.update(&len.to_le_bytes());
    hasher.update(bytes);
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
