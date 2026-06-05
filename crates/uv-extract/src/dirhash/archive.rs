//! Content-addressed identities for extracted wheel archives.

use std::path::{Path, PathBuf};

use super::{DirhashError, DirhashTree};
use crate::archive_path::SanitizedArchivePath;

const DIRECTORY_DIGEST_LENGTH: usize = 24;
const BASE36_ALPHABET: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const BASE36_RADIX: u16 = 36;

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
    /// Return the complete path-safe digest string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<blake3::Hash> for DirectoryDigest {
    fn from(hash: blake3::Hash) -> Self {
        Self(encode_digest(&hash))
    }
}

impl From<DirectoryDigest> for String {
    fn from(digest: DirectoryDigest) -> Self {
        digest.0
    }
}

/// A file extracted from an archive, along with its content-addressing metadata.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ExtractedFile {
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
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Return the sanitized archive path used internally during extraction.
    pub(crate) fn sanitized_path(&self) -> &SanitizedArchivePath {
        &self.path
    }

    /// Return whether the extracted file should be executable.
    pub fn executable(&self) -> bool {
        self.executable
    }

    /// Return the hex-encoded content digest of the extracted file.
    pub fn digest_hex(&self) -> String {
        self.digest.to_hex().to_string()
    }

    /// Convert the extracted file into a `(path, size)` pair.
    pub fn into_record(self) -> (PathBuf, u64) {
        (self.path.into_path_buf(), self.size)
    }

    /// Return the extracted file as a `(path, size)` pair.
    pub fn to_record(&self) -> (PathBuf, u64) {
        (self.path.to_path_buf(), self.size)
    }
}

/// Build the shared directory hash tree from extracted file and directory entries.
pub(crate) fn directory_tree_from_extracted<'a>(
    files: &[ExtractedFile],
    directories: impl IntoIterator<Item = &'a SanitizedArchivePath>,
) -> Result<DirhashTree, DirhashError> {
    let mut tree = DirhashTree::default();

    for directory in directories {
        let path = digest_path(directory);
        if !path.is_empty() {
            tree.add_empty_dir(&path)?;
        }
    }

    for file in files {
        tree.add_file(&digest_path(file.sanitized_path()), file.digest)?;
    }

    Ok(tree)
}

/// Format a sanitized archive path with platform-independent separators.
fn digest_path(path: &SanitizedArchivePath) -> String {
    let mut normalized = String::new();
    for component in path.as_path() {
        if !normalized.is_empty() {
            normalized.push('/');
        }
        normalized.push_str(&component.to_string_lossy());
    }
    normalized
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
        DIRECTORY_DIGEST_LENGTH, DirectoryDigest, ExtractedFile, digest_path,
        directory_tree_from_extracted,
    };

    #[test]
    fn directory_digest_uses_shared_dirhash_scheme() {
        let a = SanitizedArchivePath::from_archive_member("a.txt").expect("valid path");
        let c = SanitizedArchivePath::from_archive_member("b/c.txt").expect("valid path");
        let directory = SanitizedArchivePath::from_archive_member("b/d").expect("valid path");

        let tree = directory_tree_from_extracted(
            &[
                ExtractedFile::new(a, 5, false, blake3::hash(b"hello")),
                ExtractedFile::new(c, 7, false, blake3::hash(b"goodbye")),
            ],
            [&directory],
        )
        .expect("valid directory tree");
        let digest = DirectoryDigest::from(tree.hash());

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
        assert_eq!(
            path.as_ref().map(digest_path).as_deref(),
            Some("package/data.txt")
        );
    }
}
