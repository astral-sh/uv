//! Dirhash is a scheme for hashing directory trees, and by extension the contents of archive
//! files.
//!
//! The underlying hash function is BLAKE3, and the dirhash of a file is the regular `blake3::hash`
//! of its content bytes. To compute the dirhash of a directory, we sort and concatenate its
//! entries. Each entry has three components, which are also concatenated:
//!
//! - the UTF-8 filename or subdirectory name
//! - a terminator byte, `0xff`, which cannot occur in UTF-8
//! - the 32-byte dirhash (recursive) of the entry's contents
//!
//! To avoid collisions between files and directories, we compute the hash of those sorted,
//! concatenated directory entries with `blake3::derive_key("directory", ...)`. The implementation
//! checks that directory entries are sorted and that their names are unique and valid UTF-8. It
//! also checks that names don't contain `/` and aren't equal to `.` or `..`. Empty directories are
//! represented as the empty hash rather than omitted (as in Git).
//!
//! Symlinks aren't encoded, and we hash symlinks as the files or directories they points to. That
//! means we can't compute the dirhash of a symlink cycle. When the implementation is reading the
//! filesystem, it detects cycles and reports an error in that case.
//!
//! We don't hash any metadata about a file besides its name. In particular, that means that
//! (unlike Git) we don't hash the Unix executable bit. Two archives that encode the same files
//! with different executable bits could have the same dirhash, and its possible that could cause
//! bugs in some cases. On this other hand, this gives us the property that the dirhash of an
//! archive is the same as the dirhash of its unpacked files, even if the archive was prepared on
//! Unix and unpacked on Windows. Note that Python wheel installers [already include
//! heuristics][heuristics] for these cross-platform problems.
//!
//! [heuristics]: https://packaging.python.org/en/latest/specifications/binary-distribution-format/#recommended-installer-features
//!
//! There are two separate implementations in this module:
//!
//! - `dirhash_path` reads a directory tree from the filesystem and hashes it using Rayon.
//! - `DirhashTree` is an in-memory representation of a directory tree, which accepts entries in
//!   any order. This is intended for unpacking archives, so that we can hash file bytes while
//!   they're in memory instead of writing them to disk and then reading them back again. The
//!   `blake3_copy` function helps with the common case of extracting a `Read` implementation (like
//!   `ZipEntryReader`) to a `Write` implementation (like `std::fs::File`).
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::{Pin, pin};

use rayon::prelude::*;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

mod archive;
mod seek;

pub use archive::DirectoryDigest;
pub(crate) use archive::{ExtractedFile, directory_digest_from_extracted};
pub(crate) use seek::{unzip, unzip_and_hash};

// Read repeatedly until the whole buffer is full, similar to `read_exact`. But if EOF is
// encountered, return `Ok(n)` with a short length instead of reporting an error.
async fn read_exact_or_eof(
    mut reader: Pin<&mut impl AsyncRead>,
    mut buf: &mut [u8],
) -> io::Result<usize> {
    let mut bytes_read = 0;
    loop {
        match reader.read(buf).await {
            Ok(0) => return Ok(bytes_read),
            Ok(n) => {
                bytes_read += n;
                if n == buf.len() {
                    return Ok(bytes_read);
                }
                buf = &mut buf[n..];
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

/// Copy all the bytes from an async reader to an async writer while computing their BLAKE3 hash.
/// This uses the same buffer for reading, writing, and hashing, to avoid unnecessary re-reads or
/// intermediate copies. Return the number of bytes copied and the resulting hash.
pub async fn blake3_copy<R, W>(reader: R, writer: W) -> io::Result<(u64, blake3::Hash)>
where
    R: AsyncRead,
    W: AsyncWrite,
{
    let mut reader = pin!(reader);
    let mut writer = pin!(writer);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0; 1 << 16]; // 64 KiB
    let mut total = 0u64;
    // BLAKE3 is fastest when hashing power-of-two sized buffers. That maximizes the time we spend
    // in the wide SIMD part of the implementation (which wants between 4 and 16 KiB at a time
    // depending on the platform) and minimizes the time we spend in the slower part that handles
    // short inputs. Hash as many full 64 KiB buffers as we can, and then one possibly-short buffer
    // when we reach EOF.
    loop {
        let bytes_read = read_exact_or_eof(reader.as_mut(), &mut buffer).await?;
        if bytes_read == 0 {
            break; // EOF reached with no bytes. Skip unnecessary calls to `update` and `write_all`.
        }
        total += bytes_read as u64;
        let bytes = &buffer[..bytes_read];
        hasher.update(bytes);
        writer.write_all(bytes).await?;
        if bytes_read < buffer.len() {
            break; // EOF
        }
    }
    writer.flush().await?;
    Ok((total, hasher.finalize()))
}

#[derive(Debug, thiserror::Error)]
pub enum DirhashError {
    #[error("Invalid path for directory hashing: {path:?}")]
    InvalidPath { path: PathBuf },
    #[error("Archive path is missing from the directory hash tree: {path:?}")]
    MissingPath { path: PathBuf },
    #[error("Archive contains duplicate entries for path: {path:?}")]
    DuplicatePath { path: PathBuf },
    #[error("Archive path is used as both a file and a directory: {path:?}")]
    FileDirectoryConflict { path: PathBuf },
    #[error("Encountered a symlink cycle while hashing a directory: {paths:?}")]
    SymlinkCycle { paths: Vec<PathBuf> },
    #[error(transparent)]
    Io(#[from] io::Error),
}

// Seen symlinks form a linked list on the stack as we recurse.
struct SeenSymlinkNode<'a> {
    canonical_path: PathBuf,
    previous: Option<&'a Self>,
}

struct SeenSymlinks<'a> {
    node: Option<SeenSymlinkNode<'a>>,
}

impl<'a> SeenSymlinks<'a> {
    fn new() -> Self {
        Self { node: None }
    }

    fn iter(&self) -> impl Iterator<Item = &Path> {
        let mut node = self.node.as_ref();
        std::iter::from_fn(move || {
            if let Some(next_node) = node {
                let next_path = &next_node.canonical_path;
                node = next_node.previous;
                Some(next_path.as_path())
            } else {
                None
            }
        })
    }

    fn push(&'a self, symlink_path: &Path) -> Result<Self, DirhashError> {
        let canonical_path = canonical_path_to_symlink(symlink_path)?;
        // Walk the seen symlinks list and error out if we've seen this one before.
        for seen in self.iter() {
            if canonical_path == seen {
                let mut paths: Vec<PathBuf> = self.iter().map(Path::to_owned).collect();
                paths.reverse();
                paths.push(canonical_path);
                return Err(DirhashError::SymlinkCycle { paths });
            }
        }
        Ok(Self {
            node: Some(SeenSymlinkNode {
                canonical_path,
                previous: self.node.as_ref(),
            }),
        })
    }
}

// The canonical path *to a link itself*, not the canonical path the link *points to*. For a
// regular file or directory, this is the same as its canonical path.
fn canonical_path_to_symlink(symlink_path: &Path) -> Result<PathBuf, DirhashError> {
    let Some(filename) = symlink_path.file_name() else {
        return Err(DirhashError::InvalidPath {
            path: symlink_path.to_path_buf(),
        });
    };
    let parent = symlink_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(fs_err::canonicalize(parent)?.join(filename))
}

/// Compute the dirhash of a file or directory tree on disk. Note that the names within a directory
/// tree are required to be valid UTF-8, but the path to the root is not.
pub fn dirhash_path(path: &Path) -> Result<blake3::Hash, DirhashError> {
    let seen_symlinks = SeenSymlinks::new();
    dirhash_path_inner(path, &seen_symlinks)
}

// Recurse to compute a dirhash, handling symlink cycles.
fn dirhash_path_inner(
    path: &Path,
    seen_symlinks: &SeenSymlinks,
) -> Result<blake3::Hash, DirhashError> {
    let metadata = fs_err::symlink_metadata(path)?;
    if metadata.is_symlink() {
        let seen_symlinks = seen_symlinks.push(path)?;
        dirhash_path_inner_resolved(path, &fs_err::metadata(path)?, &seen_symlinks)
    } else {
        dirhash_path_inner_resolved(path, &metadata, seen_symlinks)
    }
}

// Recurse to compute a dirhash, after symlinks are resolved.
fn dirhash_path_inner_resolved(
    path: &Path,
    metadata: &std::fs::Metadata,
    seen_symlinks: &SeenSymlinks,
) -> Result<blake3::Hash, DirhashError> {
    if metadata.is_dir() {
        // This is a directory. Recurse over its contents.
        let mut dir_contents = Vec::new();
        for entry in fs_err::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            // Prior components of the `path` can be non-Unicode, but names in the hashed directory
            // tree are required to be Unicode, otherwise we report an error.
            let Ok(name) = entry.file_name().into_string() else {
                return Err(DirhashError::InvalidPath { path });
            };
            dir_contents.push((name, path));
        }
        // Sort the directory contents by name, in lexicographic/UTF-8 order.
        dir_contents.sort_unstable();
        // Iterate over the contents in parallel using Rayon, hashing each one recursively.
        let hashes = dir_contents
            .par_iter()
            // Recurse back to `dirhash_path_inner` for symlink handling.
            .map(|(_, path)| dirhash_path_inner(path, seen_symlinks))
            .collect::<Result<Vec<blake3::Hash>, _>>()?;
        let dirhash_entries = dir_contents
            .iter()
            .zip(hashes)
            .map(|((name, _), hash)| (name.as_str(), hash));
        Ok(hash_dir_entries(dirhash_entries))
    } else {
        // This is not a directory, so treat it like a file and hash it. `update_mmap_rayon` shares
        // the same thread pool as `par_iter` above.
        Ok(blake3::Hasher::new().update_mmap_rayon(path)?.finalize())
    }
}

#[derive(Debug, Clone)]
enum DirhashEntry {
    File(blake3::Hash),
    Directory(DirhashTree),
}

/// An in-memory directory structure for computing a dirhash from an archive as we unpack it, when
/// the entries might come out in any order.
#[derive(Debug, Clone, Default)]
pub struct DirhashTree {
    children: BTreeMap<String, DirhashEntry>,
}

impl DirhashTree {
    fn insertion_entry(
        &mut self,
        normalized_path: &str,
        original_path: &str,
        create_dirs: bool,
    ) -> Result<Entry<'_, String, DirhashEntry>, DirhashError> {
        if let Some((component, rest)) = normalized_path.split_once('/') {
            // There are further path components after this one, so this one is a directory.
            if self.children.contains_key(component) {
                // This entry already exists.
                //
                // We have to do a double lookup here because of borrowck limitations. The
                // alternative is using the `.entry()` API and always allocating a temporary
                // `String` key. Polonius can't come soon enough, but also `BTreeMap` needs a "raw
                // entry" API.
                match self.children.get_mut(component).unwrap() {
                    DirhashEntry::Directory(child) => {
                        child.insertion_entry(rest, original_path, create_dirs)
                    }
                    DirhashEntry::File(_) => Err(DirhashError::FileDirectoryConflict {
                        path: PathBuf::from(original_path),
                    }),
                }
            } else {
                // We need to create this directory, or error if `create_dirs` is false.
                if create_dirs {
                    let child = self
                        .children
                        .entry(String::from(component))
                        .or_insert(DirhashEntry::Directory(Self::default()));
                    let DirhashEntry::Directory(child) = child else {
                        unreachable!()
                    };
                    child.insertion_entry(rest, original_path, create_dirs)
                } else {
                    Err(DirhashError::MissingPath {
                        path: PathBuf::from(original_path),
                    })
                }
            }
        } else {
            // This is the final path component.
            Ok(self.children.entry(String::from(normalized_path)))
        }
    }

    pub fn insert_file(&mut self, path: &str, hash: blake3::Hash) -> Result<(), DirhashError> {
        let normalized_path = normalize_dirhash_path(path)?;
        let entry = self.insertion_entry(&normalized_path, path, true)?;
        match entry {
            Entry::Vacant(vacant) => {
                vacant.insert(DirhashEntry::File(hash));
                Ok(())
            }
            Entry::Occupied(_) => Err(DirhashError::DuplicatePath {
                path: PathBuf::from(path),
            }),
        }
    }

    pub fn update_file(&mut self, path: &str, hash: blake3::Hash) -> Result<(), DirhashError> {
        let normalized_path = normalize_dirhash_path(path)?;
        let entry = self.insertion_entry(&normalized_path, path, true)?;
        match entry {
            Entry::Vacant(_) => Err(DirhashError::MissingPath {
                path: PathBuf::from(path),
            }),
            Entry::Occupied(mut occupied) => match occupied.get_mut() {
                DirhashEntry::File(prev_hash) => {
                    *prev_hash = hash;
                    Ok(())
                }
                DirhashEntry::Directory(_) => Err(DirhashError::FileDirectoryConflict {
                    path: PathBuf::from(path),
                }),
            },
        }
    }

    pub fn insert_empty_dir(&mut self, path: &str) -> Result<(), DirhashError> {
        let normalized_path = normalize_dirhash_path(path)?;
        let entry = self.insertion_entry(&normalized_path, path, true)?;
        match entry {
            Entry::Vacant(vacant) => {
                vacant.insert(DirhashEntry::Directory(Self::default()));
                Ok(())
            }
            Entry::Occupied(occupied) => match occupied.get() {
                DirhashEntry::Directory(_) => Ok(()),
                DirhashEntry::File(_) => Err(DirhashError::FileDirectoryConflict {
                    path: PathBuf::from(path),
                }),
            },
        }
    }

    pub fn hash(&self) -> blake3::Hash {
        hash_dir_entries(self.children.iter().map(|(name, entry)| {
            let hash = match entry {
                DirhashEntry::File(hash) => *hash,
                DirhashEntry::Directory(child) => child.hash(),
            };
            (name.as_str(), hash)
        }))
    }
}

fn component_needs_normalization(component: &str) -> bool {
    matches!(component, "" | "." | "..")
}

fn normalize_dirhash_path(mut path: &str) -> Result<Cow<'_, str>, DirhashError> {
    if path.starts_with('/') {
        return Err(DirhashError::InvalidPath {
            path: PathBuf::from(path),
        });
    }
    path = path.trim_start_matches("./");
    path = path.trim_end_matches('/');
    if !path.split('/').any(component_needs_normalization) {
        return Ok(Cow::Borrowed(path));
    }
    let mut components = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if components.pop().is_none() {
                    return Err(DirhashError::InvalidPath {
                        path: PathBuf::from(path),
                    });
                }
            }
            component => components.push(component),
        }
    }
    if components.is_empty() {
        return Err(DirhashError::InvalidPath {
            path: PathBuf::from(path),
        });
    }
    Ok(Cow::Owned(components.join("/")))
}

fn hash_dir_entries<'a, Iter>(entries: Iter) -> blake3::Hash
where
    Iter: IntoIterator<Item = (&'a str, blake3::Hash)>,
{
    // File hashes are the normal BLAKE3 hash of the file's contents. A directory hash shouldn't
    // collide with a file hash, no matter what bytes the file happens contain. BLAKE3's derive-key
    // mode with a context string guarantees that.
    let mut hasher = blake3::Hasher::new_derive_key("directory");
    for (name, hash) in entries {
        hasher.update(name.as_bytes());
        hasher.update(&[0xff]);
        hasher.update(hash.as_bytes());
    }
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::cmp;
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::AsyncRead;

    #[test]
    fn test_normalize() {
        let success_cases = [
            ("foo", Cow::Borrowed("foo")),
            ("foo", Cow::Borrowed("foo")),
            ("foo/", Cow::Borrowed("foo")),
            ("./foo", Cow::Borrowed("foo")),
            ("././foo/bar///", Cow::Borrowed("foo/bar")),
            ("foo//bar", Cow::Owned("foo/bar".to_string())),
            ("foo/./bar", Cow::Owned("foo/bar".to_string())),
            ("foo/.///./bar", Cow::Owned("foo/bar".to_string())),
            ("foo/bar/..", Cow::Owned("foo".to_string())),
            ("foo/bar/../../baz", Cow::Owned("baz".to_string())),
        ];
        for (path, expected) in success_cases {
            let normalized = super::normalize_dirhash_path(path).unwrap();
            assert_eq!(normalized, expected);
        }
        let error_cases = [
            "",
            "/",
            "/foo",
            "///foo",
            "..",
            "foo/..",
            "foo/bar/../../../baz",
        ];
        for path in error_cases {
            super::normalize_dirhash_path(path).unwrap_err();
        }
    }

    #[test]
    fn test_insert_update_and_insert_empty_dir() {
        // Hash the following tree:
        //
        // a.txt      <-- "hello"
        // b
        // ├── c.txt  <-- "goodbye"
        // └── d      <-- empty dir
        //
        // First, assemble the whole hash tree manually.
        let a_hash = blake3::hash(b"hello");
        let c_hash = blake3::hash(b"goodbye");
        let d_hash = blake3::derive_key("directory", b"");
        let mut b_input = Vec::new();
        b_input.extend_from_slice(b"c.txt\xff");
        b_input.extend_from_slice(c_hash.as_bytes());
        b_input.extend_from_slice(b"d\xff");
        b_input.extend_from_slice(&d_hash);
        let b_hash = blake3::derive_key("directory", &b_input);
        let mut root_input = Vec::new();
        root_input.extend_from_slice(b"a.txt\xff");
        root_input.extend_from_slice(a_hash.as_bytes());
        root_input.extend_from_slice(b"b\xff");
        root_input.extend_from_slice(&b_hash);
        let root_hash = blake3::derive_key("directory", &root_input);
        // Pin the specific value of the dirhash. TODO: a full set of test vectors
        assert_eq!(
            blake3::Hash::from_bytes(root_hash).to_hex().as_str(),
            "e508467d129e0d19cefa96527f5f6cb3760530be4d931c527f2818a0dff5d517"
        );

        // Now, confirm that `DirhashTree` gives the same answer.
        let mut tree = super::DirhashTree::default();
        tree.insert_file("a.txt", a_hash).unwrap();
        tree.insert_file("b/c.txt", c_hash).unwrap();
        tree.insert_empty_dir("b/d").unwrap();
        assert_eq!(tree.hash(), root_hash);

        // Changing the hash of a file changes the root hash.
        tree.update_file("b/c.txt", [0; 32].into()).unwrap();
        assert_ne!(tree.hash(), root_hash);
        // But we can change it back and recover the original.
        tree.update_file("b/c.txt", c_hash).unwrap();
        assert_eq!(tree.hash(), root_hash);

        // Reinserting an existing empty directory is a no-op.
        tree.insert_empty_dir("b").unwrap(); // no-op
        assert_eq!(tree.hash(), root_hash);
        // But inserting a new empty directory changes the hash.
        tree.insert_empty_dir("e").unwrap(); // no-op
        assert_ne!(tree.hash(), root_hash);
    }

    #[test]
    fn test_dirhash_path() -> Result<(), super::DirhashError> {
        // Hash the following tree:
        //
        // a.txt      <-- "hello"
        // b
        // ├── c.txt  <-- "goodbye"
        // └── d      <-- empty dir
        //
        // Compare both `DirhashTree` (in memory) and `dirhash_path` (on disk) to make sure we get
        // the same hash from both.
        let temp_dir = tempfile::tempdir()?;
        let root = temp_dir.path();
        fs_err::write(root.join("a.txt"), b"hello")?;
        fs_err::create_dir(root.join("b"))?;
        fs_err::write(root.join("b/c.txt"), b"goodbye")?;
        fs_err::create_dir(root.join("b/d"))?;

        let mut expected = super::DirhashTree::default();
        expected.insert_file("a.txt", blake3::hash(b"hello"))?;
        expected.insert_file("b/c.txt", blake3::hash(b"goodbye"))?;
        expected.insert_empty_dir("b/d")?;

        assert_eq!(super::dirhash_path(root)?, expected.hash());
        // Asking for the dirhash of a file is also valid, and it's equivalent to the regular
        // BLAKE3 hash.
        assert_eq!(
            super::dirhash_path(&root.join("a.txt"))?,
            blake3::hash(b"hello")
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_dirhash_path_symlinks() -> Result<(), super::DirhashError> {
        use fs_err::os::unix::fs::symlink;

        // Start with the following tree, which does not have a cycle:
        //
        // dir1
        // ├── file.txt  <-- "hello"
        // └── dir_link  <-- ../dir2
        // dir2
        // └── file_link <-- ../dir1/file.txt
        //
        // Make sure we get the same answer from both `DirhashTree` (in memory) and `dirhash_path`
        // (on disk).
        let temp_dir = tempfile::tempdir()?;
        let root = temp_dir.path();
        fs_err::create_dir(root.join("dir1"))?;
        fs_err::create_dir(root.join("dir2"))?;
        fs_err::write(root.join("dir1/file.txt"), b"hello")?;
        symlink("../dir2", root.join("dir1/dir_link"))?;
        symlink("../dir1/file.txt", root.join("dir2/file_link"))?;

        let mut in_memory = super::DirhashTree::default();
        in_memory.insert_file("dir1/file.txt", blake3::hash(b"hello"))?;
        in_memory.insert_file("dir1/dir_link/file_link", blake3::hash(b"hello"))?;
        in_memory.insert_file("dir2/file_link", blake3::hash(b"hello"))?;
        let from_disk = super::dirhash_path(root)?;
        assert_eq!(in_memory.hash(), from_disk);

        // Now add another symlink to make a proper cycle. This should error.
        fs_err::create_dir(root.join("dir2/inner"))?;
        symlink("../../dir1", root.join("dir2/inner/dir_link"))?;
        let error = super::dirhash_path(root).unwrap_err();
        std::assert_matches!(error, super::DirhashError::SymlinkCycle { .. });
        Ok(())
    }

    /// Write a test input byte pattern that doesn't repeat at regular power-of-two boundaries.
    /// This is more likely to catch mistakes than hashing a buffer of e.g. all zeros.
    fn paint_input(buf: &mut [u8]) {
        let mut value = 0u8;
        for byte in buf {
            *byte = value;
            value = if value == 250 { 0 } else { value + 1 };
        }
    }

    #[tokio::test]
    async fn test_blake3_copy() -> io::Result<()> {
        let input = b"hello";
        let mut output = Vec::new();
        let (bytes_read, hash) = Box::pin(super::blake3_copy(&input[..], &mut output)).await?;
        assert_eq!(bytes_read, input.len() as u64);
        assert_eq!(input, &output[..]);
        assert_eq!(hash, blake3::hash(input));

        let mut big_input = vec![0; 64_000 * 3];
        paint_input(&mut big_input);
        let mut big_output = Vec::new();
        let (big_bytes_read, big_hash) =
            Box::pin(super::blake3_copy(&big_input[..], &mut big_output)).await?;
        assert_eq!(big_bytes_read, big_input.len() as u64);
        assert_eq!(big_input, big_output);
        assert_eq!(big_hash, blake3::hash(&big_input));
        Ok(())
    }

    /// A reader that always returns short reads, even if it holds lots of input.
    struct ShortReader<'a>(&'a [u8]);

    impl AsyncRead for ShortReader<'_> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            const SHORT_READ_LEN: usize = 251; // any small prime will do
            let want = cmp::min(self.0.len(), buf.remaining());
            let take = cmp::min(want, SHORT_READ_LEN);
            buf.put_slice(&self.0[..take]);
            self.0 = &self.0[take..];
            Poll::Ready(Ok(()))
        }
    }

    /// Exercise the buffer filling logic with a reader that always returns short reads.
    #[tokio::test]
    async fn test_blake3_copy_short_reader() -> io::Result<()> {
        let mut input = vec![0; 64_000 * 3];
        paint_input(&mut input);
        let mut output = Vec::new();
        let (bytes_read, hash) =
            Box::pin(super::blake3_copy(ShortReader(&input), &mut output)).await?;
        assert_eq!(bytes_read, input.len() as u64);
        assert_eq!(input, &output[..]);
        assert_eq!(hash, blake3::hash(&input));
        Ok(())
    }
}
