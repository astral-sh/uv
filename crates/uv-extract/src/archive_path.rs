use std::path::{Component, Path, PathBuf};

/// A normalized, relative path that is safe to extract from an archive.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub(crate) struct SanitizedArchivePath(PathBuf);

impl SanitizedArchivePath {
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

impl AsRef<Path> for SanitizedArchivePath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

/// Normalize an archive member name and ensure that it cannot escape the extraction root.
///
/// See: <https://docs.rs/zip/latest/zip/read/struct.ZipFile.html#method.enclosed_name>
pub(crate) fn enclosed_name(file_name: &str) -> Option<SanitizedArchivePath> {
    if file_name.contains('\0') {
        return None;
    }
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
    Some(SanitizedArchivePath(path))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{SanitizedArchivePath, enclosed_name};

    #[test]
    fn enclosed_name_normalizes_safe_paths() {
        assert_eq!(
            enclosed_name("package/../module.py")
                .as_ref()
                .map(SanitizedArchivePath::as_path),
            Some(Path::new("module.py"))
        );
        assert_eq!(
            enclosed_name("package/./subdir//module.py")
                .as_ref()
                .map(SanitizedArchivePath::as_path),
            Some(Path::new("package/subdir/module.py"))
        );
    }

    #[test]
    fn enclosed_name_rejects_paths_outside_root() {
        assert_eq!(enclosed_name("../module.py"), None);
        assert_eq!(enclosed_name("package/../../module.py"), None);
        assert_eq!(enclosed_name("/module.py"), None);
        assert_eq!(enclosed_name("module\0.py"), None);
    }
}
