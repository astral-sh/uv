use blake2::digest::consts::U32;
use sha2::Digest;
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
    pub fn update(&mut self, data: &[u8]) {
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

/// A blake3 hash digest, hex-encoded.
#[derive(Debug, Clone)]
pub struct Blake3Digest(String);

impl Blake3Digest {
    /// Create a new [`Blake3Digest`] from a hex-encoded string.
    pub fn new(hex: String) -> Self {
        Self(hex)
    }

    /// Return the hex-encoded digest string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Blake3Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

pub struct HashReader<'a, R> {
    reader: R,
    hashers: &'a mut [Hasher],
    blake3: blake3::Hasher,
}

impl<'a, R> HashReader<'a, R>
where
    R: tokio::io::AsyncRead + Unpin,
{
    pub fn new(reader: R, hashers: &'a mut [Hasher]) -> Self {
        HashReader {
            reader,
            hashers,
            blake3: blake3::Hasher::new(),
        }
    }

    /// Exhaust the underlying reader.
    pub async fn finish(&mut self) -> Result<(), std::io::Error> {
        while self.read(&mut vec![0; 8192]).await? > 0 {}

        Ok(())
    }

    /// Finalize and return the blake3 digest.
    pub fn blake3_digest(&self) -> Blake3Digest {
        Blake3Digest(self.blake3.finalize().to_hex().to_string())
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
                let filled = buf.filled();
                for hasher in self.hashers.iter_mut() {
                    hasher.update(filled);
                }
                self.blake3.update(filled);
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}
