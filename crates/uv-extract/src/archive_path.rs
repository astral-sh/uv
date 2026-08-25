use std::path::{Component, Path, PathBuf};

use crate::{Error, validate_archive_member_name};

/// A normalized, relative path that is safe to extract from an archive.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub(crate) struct SanitizedArchivePath(PathBuf);

impl SanitizedArchivePath {
    /// Normalize an archive member name and ensure that it cannot escape the extraction root.
    ///
    /// Invalid filenames return an error; paths that escape the extraction root return `None`.
    ///
    /// See: <https://docs.rs/zip/latest/zip/read/struct.ZipFile.html#method.enclosed_name>
    pub(crate) fn from_archive_member(file_name: &str) -> Result<Option<Self>, Error> {
        validate_archive_member_name(file_name)?;

        let mut path = PathBuf::new();
        for component in Path::new(file_name).components() {
            match component {
                Component::Prefix(_) | Component::RootDir => return Ok(None),
                Component::ParentDir => {
                    if !path.pop() {
                        return Ok(None);
                    }
                }
                Component::Normal(component) => path.push(component),
                Component::CurDir => (),
            }
        }
        Ok(Some(Self(path)))
    }

    /// Return the normalized path.
    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }

    /// Return the normalized path as a [`PathBuf`].
    pub(crate) fn to_path_buf(&self) -> PathBuf {
        self.0.clone()
    }

    /// Return the normalized path as an owned [`PathBuf`].
    pub(crate) fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::Error;

    use super::SanitizedArchivePath;

    #[test]
    fn archive_member_path_normalizes_safe_paths() -> Result<(), Error> {
        assert_eq!(
            SanitizedArchivePath::from_archive_member("package/../module.py")?
                .as_ref()
                .map(SanitizedArchivePath::as_path),
            Some(Path::new("module.py"))
        );
        assert_eq!(
            SanitizedArchivePath::from_archive_member("package/./subdir//module.py")?
                .as_ref()
                .map(SanitizedArchivePath::as_path),
            Some(Path::new("package/subdir/module.py"))
        );
        Ok(())
    }

    #[test]
    fn archive_member_path_rejects_paths_outside_root() -> Result<(), Error> {
        assert_eq!(
            SanitizedArchivePath::from_archive_member("../module.py")?,
            None
        );
        assert_eq!(
            SanitizedArchivePath::from_archive_member("package/../../module.py")?,
            None
        );
        assert_eq!(
            SanitizedArchivePath::from_archive_member("/module.py")?,
            None
        );
        Ok(())
    }

    #[test]
    fn archive_member_path_rejects_invalid_names() {
        for file_name in ["", "module\0.py", "module\n.py", "module\t.py"] {
            assert!(
                SanitizedArchivePath::from_archive_member(file_name).is_err(),
                "archive member name should be rejected: {file_name:?}"
            );
        }
    }
}
