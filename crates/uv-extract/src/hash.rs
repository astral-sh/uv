use std::pin::Pin;
use std::task::{Context, Poll};

use sha2::Digest;
use tokio::io::{AsyncReadExt, ReadBuf};

use pypi_types::{HashAlgorithm, HashDigest};

pub struct Sha256Reader<'a, R> {
    reader: R,
    hasher: &'a mut sha2::Sha256,
}

impl<'a, R> Sha256Reader<'a, R>
where
    R: tokio::io::AsyncRead + Unpin,
{
    pub fn new(reader: R, hasher: &'a mut sha2::Sha256) -> Self {
        Sha256Reader { reader, hasher }
    }
}

impl<'a, R> tokio::io::AsyncRead for Sha256Reader<'a, R>
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
                self.hasher.update(buf.filled());
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

#[derive(Debug)]
pub enum Hasher {
    Md5(md5::Md5),
    Sha256(sha2::Sha256),
    Sha384(sha2::Sha384),
    Sha512(sha2::Sha512),
}

impl Hasher {
    pub fn update(&mut self, data: &[u8]) {
        match self {
            Hasher::Md5(hasher) => hasher.update(data),
            Hasher::Sha256(hasher) => hasher.update(data),
            Hasher::Sha384(hasher) => hasher.update(data),
            Hasher::Sha512(hasher) => hasher.update(data),
        }
    }

    pub fn finalize(self) -> Vec<u8> {
        match self {
            Hasher::Md5(hasher) => hasher.finalize().to_vec(),
            Hasher::Sha256(hasher) => hasher.finalize().to_vec(),
            Hasher::Sha384(hasher) => hasher.finalize().to_vec(),
            Hasher::Sha512(hasher) => hasher.finalize().to_vec(),
        }
    }
}

impl From<HashAlgorithm> for Hasher {
    fn from(algorithm: HashAlgorithm) -> Self {
        match algorithm {
            HashAlgorithm::Md5 => Hasher::Md5(md5::Md5::new()),
            HashAlgorithm::Sha256 => Hasher::Sha256(sha2::Sha256::new()),
            HashAlgorithm::Sha384 => Hasher::Sha384(sha2::Sha384::new()),
            HashAlgorithm::Sha512 => Hasher::Sha512(sha2::Sha512::new()),
        }
    }
}

impl From<Hasher> for HashDigest {
    fn from(hasher: Hasher) -> Self {
        match hasher {
            Hasher::Md5(hasher) => HashDigest {
                algorithm: HashAlgorithm::Md5,
                digest: format!("{:x}", hasher.finalize()).into_boxed_str(),
            },
            Hasher::Sha256(hasher) => HashDigest {
                algorithm: HashAlgorithm::Sha256,
                digest: format!("{:x}", hasher.finalize()).into_boxed_str(),
            },
            Hasher::Sha384(hasher) => HashDigest {
                algorithm: HashAlgorithm::Sha384,
                digest: format!("{:x}", hasher.finalize()).into_boxed_str(),
            },
            Hasher::Sha512(hasher) => HashDigest {
                algorithm: HashAlgorithm::Sha512,
                digest: format!("{:x}", hasher.finalize()).into_boxed_str(),
            },
        }
    }
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

impl<'a, R> tokio::io::AsyncRead for HashReader<'a, R>
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
