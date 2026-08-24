use std::path::{Component, Path, PathBuf};

use crate::validate_archive_member_name;

/// A normalized, relative path that is safe to extract from an archive.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub(crate) struct SanitizedArchivePath(PathBuf);

impl SanitizedArchivePath {
    /// Normalize an archive member name and ensure that it cannot escape the extraction root.
    ///
    /// See: <https://docs.rs/zip/latest/zip/read/struct.ZipFile.html#method.enclosed_name>
    pub(crate) fn from_archive_member(file_name: &str) -> Option<Self> {
        validate_archive_member_name(file_name).ok()?;

        let mut path = PathBuf::new();
        for component in Path::new(file_name).components() {
            match component {
                Component::Prefix(_) | Component::RootDir => return None,
                Component::ParentDir => {
                    if !path.pop() {
                        return None;
                    }
                }
                Component::Normal(component) => path.push(component),
                Component::CurDir => (),
            }
        }
        Some(Self(path))
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

    use super::SanitizedArchivePath;

    #[test]
    fn archive_member_path_normalizes_safe_paths() {
        assert_eq!(
            SanitizedArchivePath::from_archive_member("package/../module.py")
                .as_ref()
                .map(SanitizedArchivePath::as_path),
            Some(Path::new("module.py"))
        );
        assert_eq!(
            SanitizedArchivePath::from_archive_member("package/./subdir//module.py")
                .as_ref()
                .map(SanitizedArchivePath::as_path),
            Some(Path::new("package/subdir/module.py"))
        );
    }

    #[test]
    fn archive_member_path_rejects_paths_outside_root() {
        assert_eq!(
            SanitizedArchivePath::from_archive_member("../module.py"),
            None
        );
        assert_eq!(
            SanitizedArchivePath::from_archive_member("package/../../module.py"),
            None
        );
        assert_eq!(
            SanitizedArchivePath::from_archive_member("/module.py"),
            None
        );
    }

    #[test]
    fn archive_member_path_rejects_invalid_names() {
        for file_name in ["", "module\0.py", "module\n.py", "module\t.py"] {
            assert_eq!(
                SanitizedArchivePath::from_archive_member(file_name),
                None,
                "archive member name should be rejected: {file_name:?}"
            );
        }
    }
}
