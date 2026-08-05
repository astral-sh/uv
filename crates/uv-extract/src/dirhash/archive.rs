//! Content-addressed identities for extracted wheel archives.

use std::path::{Component, PathBuf};

use super::{DirhashError, DirhashTree};
use crate::archive_path::SanitizedArchivePath;

const DIRECTORY_DIGEST_LENGTH: usize = 24;
const BASE36_ALPHABET: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const BASE36_RADIX: u16 = 36;

/// The platform-independent representation of a sanitized archive path.
#[derive(Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct DigestPath(Box<str>);

impl DigestPath {
    fn as_str(&self) -> &str {
        &self.0
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&SanitizedArchivePath> for DigestPath {
    fn from(path: &SanitizedArchivePath) -> Self {
        let mut canonical = String::new();
        for component in path.as_path().components() {
            let Component::Normal(component) = component else {
                continue;
            };
            if !canonical.is_empty() {
                canonical.push('/');
            }
            canonical.push_str(component.to_string_lossy().as_ref());
        }
        Self(canonical.into_boxed_str())
    }
}

/// A path-safe encoding of the directory hash of an extracted wheel.
///
/// The underlying [`DirhashTree`] includes normalized relative paths, file contents, and empty
/// directories. It intentionally uses the shared dirhash scheme directly, so executable permissions
/// are not currently part of the archive identity.
///
/// The digest is formatted as 24 lowercase base-36 characters, providing approximately 124 bits
/// of output entropy. Its alphabet is safe for case-insensitive filesystems.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DirectoryDigest(String);

impl DirectoryDigest {
    fn from_hash(hash: blake3::Hash) -> Self {
        Self(encode_digest(&hash))
    }

    /// Return the complete path-safe digest string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<DirectoryDigest> for String {
    fn from(digest: DirectoryDigest) -> Self {
        digest.0
    }
}

/// A file extracted from an archive, along with its content-addressing metadata.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ExtractedFile {
    path: SanitizedArchivePath,
    size: u64,
    executable: bool,
    digest: blake3::Hash,
}

impl ExtractedFile {
    pub(crate) fn new(
        path: SanitizedArchivePath,
        size: u64,
        executable: bool,
        digest: blake3::Hash,
    ) -> Self {
        Self {
            path,
            size,
            executable,
            digest,
        }
    }

    /// Return the path of the extracted file within the archive.
    pub(crate) fn path(&self) -> &SanitizedArchivePath {
        &self.path
    }

    /// Convert the extracted file into a `(path, size)` pair.
    pub(crate) fn into_record(self) -> (PathBuf, u64) {
        (self.path.into_path_buf(), self.size)
    }
}

/// Compute the shared directory hash from extracted file and directory entries.
pub(crate) fn directory_digest_from_extracted<'a>(
    files: &[ExtractedFile],
    directories: impl IntoIterator<Item = &'a SanitizedArchivePath>,
) -> Result<DirectoryDigest, DirhashError> {
    let mut tree = DirhashTree::default();

    for directory in directories {
        let path = DigestPath::from(directory);
        if !path.is_empty() {
            tree.add_empty_dir(path.as_str())?;
        }
    }

    for file in files {
        let path = DigestPath::from(file.path());
        tree.add_file(path.as_str(), file.digest)?;
    }

    Ok(DirectoryDigest::from_hash(tree.hash()))
}

fn encode_digest(digest: &blake3::Hash) -> String {
    let mut value = *digest.as_bytes();
    let mut encoded = [b'0'; DIRECTORY_DIGEST_LENGTH];

    for digit in encoded.iter_mut().rev() {
        let mut remainder = 0u16;
        for byte in &mut value {
            let dividend = (remainder << 8) | u16::from(*byte);
            let quotient = dividend / BASE36_RADIX;
            debug_assert!(u8::try_from(quotient).is_ok());
            *byte = quotient.to_le_bytes()[0];
            remainder = dividend % BASE36_RADIX;
        }
        *digit = BASE36_ALPHABET[usize::from(remainder)];
    }

    encoded.into_iter().map(char::from).collect()
}

#[cfg(test)]
mod tests {
    use crate::archive_path::SanitizedArchivePath;

    use super::{
        DIRECTORY_DIGEST_LENGTH, DigestPath, ExtractedFile, directory_digest_from_extracted,
    };

    #[test]
    fn directory_digest_uses_shared_dirhash_scheme() {
        let a = SanitizedArchivePath::from_archive_member("a.txt").expect("valid path");
        let c = SanitizedArchivePath::from_archive_member("b/c.txt").expect("valid path");
        let directory = SanitizedArchivePath::from_archive_member("b/d").expect("valid path");

        let digest = directory_digest_from_extracted(
            &[
                ExtractedFile::new(a, 5, false, blake3::hash(b"hello")),
                ExtractedFile::new(c, 7, false, blake3::hash(b"goodbye")),
            ],
            [&directory],
        )
        .expect("valid directory tree");

        assert_eq!(digest.as_str(), "xhg9bffqlabg1f3sq4i83jfb");
        assert_eq!(digest.as_str().len(), DIRECTORY_DIGEST_LENGTH);
        assert!(
            digest
                .as_str()
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        );
    }

    #[test]
    fn digest_path_uses_normalized_archive_path() {
        let path = SanitizedArchivePath::from_archive_member("example/../package/./data.txt");
        let digest_path = path.as_ref().map(DigestPath::from);

        assert_eq!(
            digest_path.as_ref().map(DigestPath::as_str),
            Some("package/data.txt")
        );
    }
}
