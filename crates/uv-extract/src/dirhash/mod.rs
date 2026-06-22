//! Versioned directory hashes for extracted archives.

use std::path::{Component, PathBuf};

use data_encoding::BASE64URL_NOPAD;
use rustc_hash::FxHashSet;

use crate::archive_path::SanitizedArchivePath;

mod seek;

pub(crate) use seek::{unzip, unzip_and_hash};

const DIRECTORY_DIGEST_CONTEXT: &str = "uv directory digest v1";
const DIRECTORY_DIGEST_BYTES: usize = 18;

const FRAME_EMPTY_DIRECTORY: u8 = 1;
const FRAME_FILE: u8 = 2;
const FRAME_PATH: u8 = 3;
const FRAME_SIZE: u8 = 4;
const FRAME_EXECUTABLE: u8 = 5;
const FRAME_CONTENT_BLAKE3: u8 = 6;

/// The platform-independent representation of a sanitized archive path used by the digest.
#[derive(Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct DigestPath(Box<str>);

impl DigestPath {
    fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

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

/// A versioned digest of the filesystem tree produced by extracting a ZIP archive.
///
/// The digest is independent of ZIP entry order and metadata that does not affect the extracted
/// tree, such as archive comments. It includes canonical relative paths, file sizes, executable
/// bits, file contents, and explicit empty leaf directories. Non-empty directories are implied by
/// their children.
///
/// Empty leaf directories are significant because they can affect Python namespace-package
/// imports. For example, an empty `namespace/` directory can affect:
///
/// ```python
/// import namespace
/// ```
///
/// The digest is formatted as 144 bits of the BLAKE3 digest, encoded as unpadded URL-safe base64.
/// The 24-byte representation fits within Minix's 30-byte filesystem component limit.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DirectoryDigest(String);

impl DirectoryDigest {
    /// Compute a deterministic digest for extracted file and empty-directory entries.
    ///
    /// The v1 construction uses BLAKE3's derive-key mode for scheme-level domain separation. Its
    /// input is a sorted stream of type-length-value frames. Empty directories and files are
    /// distinct top-level frame types, while each file contains separate path, size, executable,
    /// and content digest frames. Each frame is encoded as a one-byte type, an eight-byte
    /// little-endian length, and the frame value.
    ///
    /// Empty leaf directories are included because this digest identifies uv's extracted-wheel
    /// cache, where an empty directory can affect Python namespace-package behavior. Non-empty
    /// directories are implied by their children. ZIP entries are never followed as symlinks
    /// during extraction; all non-directory entries are materialized and hashed as regular files.
    pub(crate) fn from_contents(
        mut files: Vec<DirectoryDigestFile>,
        mut directories: Vec<DigestPath>,
    ) -> Self {
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
        let digest = hasher.finalize();
        Self(BASE64URL_NOPAD.encode(&digest.as_bytes()[..DIRECTORY_DIGEST_BYTES]))
    }

    /// Return the complete versioned, path-safe digest string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<DirectoryDigest> for String {
    fn from(d: DirectoryDigest) -> Self {
        d.0
    }
}

/// The digest inputs for a regular file after ZIP extraction semantics have been applied.
pub(crate) struct DirectoryDigestFile {
    path: DigestPath,
    size: u64,
    executable: bool,
    digest: blake3::Hash,
}

impl DirectoryDigestFile {
    fn from_extracted(file: &ExtractedFile) -> Self {
        Self {
            path: DigestPath::from(&file.path),
            size: file.size,
            executable: file.executable,
            digest: file.digest,
        }
    }
}

/// A file extracted from an archive, along with the metadata used by the directory digest.
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

/// Compute a deterministic digest for extracted files and empty-directory paths.
pub(crate) fn directory_digest_from_extracted(
    files: &[ExtractedFile],
    directories: Vec<DigestPath>,
) -> DirectoryDigest {
    DirectoryDigest::from_contents(
        files
            .iter()
            .map(DirectoryDigestFile::from_extracted)
            .collect(),
        directories,
    )
}

/// Return the canonical paths of explicit archive directories that are empty in the extracted tree.
///
/// Parent directories containing an explicit directory or extracted file are omitted because their
/// presence is already implied by that child.
pub(crate) fn empty_directory_paths<'a>(
    directories: impl IntoIterator<Item = &'a SanitizedArchivePath>,
    files: impl IntoIterator<Item = &'a SanitizedArchivePath>,
) -> Vec<DigestPath> {
    let mut candidates = FxHashSet::default();
    let mut non_empty = FxHashSet::default();

    for directory in directories {
        let path = DigestPath::from(directory);
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
        let path = DigestPath::from(file);
        mark_canonical_parent_directories(&mut non_empty, &path);
    }

    candidates
        .into_iter()
        .filter(|path| !non_empty.contains(path))
        .collect()
}

/// Mark all canonical parent directories of a slash-separated path as non-empty.
fn mark_canonical_parent_directories(non_empty: &mut FxHashSet<DigestPath>, path: &DigestPath) {
    let mut path = path.as_str();
    while let Some((parent, _child)) = path.rsplit_once('/') {
        non_empty.insert(DigestPath(parent.into()));
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::archive_path::SanitizedArchivePath;

    use super::{
        DigestPath, DirectoryDigest, DirectoryDigestFile, FRAME_CONTENT_BLAKE3,
        FRAME_EMPTY_DIRECTORY, FRAME_EXECUTABLE, FRAME_FILE, FRAME_PATH, FRAME_SIZE,
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
        let digest = DirectoryDigest::from_contents(
            vec![digest_file("example/data.txt", b"contents", false)],
            vec![DigestPath("example/empty".into())],
        );

        assert_eq!(digest.as_str(), "Y7xwQkyoSQmbHQkyVBHha2v4");
        assert!(digest.as_str().len() <= 30);
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

    #[test]
    fn directory_digest_frames_file_names() {
        let separate_files = DirectoryDigest::from_contents(
            vec![
                digest_file("foo.txt", b"", false),
                digest_file("bar.txt", b"", false),
            ],
            Vec::new(),
        );
        let newline_file = DirectoryDigest::from_contents(
            vec![digest_file("foo.txt\nbar.txt", b"", false)],
            Vec::new(),
        );

        assert_ne!(separate_files, newline_file);
    }

    fn digest_file(path: &str, contents: &[u8], executable: bool) -> DirectoryDigestFile {
        DirectoryDigestFile {
            path: DigestPath(path.into()),
            size: u64::try_from(contents.len()).unwrap_or(u64::MAX),
            executable,
            digest: blake3::hash(contents),
        }
    }
}
