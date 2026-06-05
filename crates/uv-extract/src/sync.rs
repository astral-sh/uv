use std::path::{Path, PathBuf};

use crate::Error;
use crate::dirhash::DirectoryDigest;

/// Unzip a `.zip` archive into the target directory.
///
/// Returns the list of unpacked files and their sizes.
pub fn unzip(reader: fs_err::File, target: &Path) -> Result<Vec<(PathBuf, u64)>, Error> {
    crate::dirhash::unzip(reader, target)
}

/// Unzip a `.zip` archive into the target directory while computing a digest of the extracted files.
///
/// The digest includes canonical relative paths, executable bits, sizes, contents, and explicit
/// empty leaf directories. ZIP entries are never followed as symlinks; non-directory entries are
/// materialized and hashed as regular files.
///
/// Returns the list of unpacked files and their sizes, along with the digest.
pub fn unzip_and_hash(
    reader: fs_err::File,
    target: &Path,
) -> Result<(Vec<(PathBuf, u64)>, DirectoryDigest), Error> {
    crate::dirhash::unzip_and_hash(reader, target)
}

/// Extract the top-level directory from an unpacked archive.
///
/// The specification says:
/// > A .tar.gz source distribution (sdist) contains a single top-level directory called
/// > `{name}-{version}` (e.g. foo-1.0), containing the source files of the package.
///
/// This function returns the path to that top-level directory.
pub fn strip_component(source: impl AsRef<Path>) -> Result<PathBuf, Error> {
    // TODO(konstin): Verify the name of the directory.
    let top_level = fs_err::read_dir(source.as_ref())
        .map_err(Error::Io)?
        .collect::<std::io::Result<Vec<fs_err::DirEntry>>>()
        .map_err(Error::Io)?;
    match top_level.as_slice() {
        [root] => Ok(root.path()),
        [] => Err(Error::EmptyArchive),
        _ => Err(Error::NonSingularArchive(
            top_level
                .into_iter()
                .map(|entry| entry.file_name())
                .collect(),
        )),
    }
}
