use std::path::{Component, Path, PathBuf};

use crate::Interpreter;
use uv_pypi_types::Scheme;

/// A `--root` directory into which packages can be installed, separate from a virtual environment
/// or system Python interpreter.
#[derive(Debug, Clone)]
pub struct Root(PathBuf);

impl Root {
    /// Return `path` with `self.0` prepended.
    //
    // See pip's implementation: https://github.com/pypa/pip/blob/65fe65b5f517c289362e1c96fedefc852f111a91/src/pip/_internal/locations/base.py#L28-L53
    fn change_root<T: AsRef<Path>>(&self, path: T) -> PathBuf {
        let mut result = PathBuf::from(&self.0);
        result.extend(
            path.as_ref()
                .components()
                .filter_map(|component| match component {
                    Component::Prefix(_) | Component::RootDir => None,
                    other => Some(other),
                }),
        );
        result
    }

    /// Return the [`Scheme`] for the `--root` directory.
    pub fn scheme(&self, interpreter: &Interpreter) -> Scheme {
        Scheme {
            purelib: self.change_root(interpreter.purelib()),
            platlib: self.change_root(interpreter.platlib()),
            scripts: self.change_root(interpreter.scripts()),
            data: self.change_root(interpreter.data()),
            include: self.change_root(interpreter.include()),
        }
    }

    /// Return an iterator over the `site-packages` directories inside the environment.
    pub fn site_packages(&self, interpreter: &Interpreter) -> impl Iterator<Item = PathBuf> {
        std::iter::once(self.change_root(interpreter.purelib()))
    }

    /// Initialize the `--root` directory.
    pub fn init(&self, interpreter: &Interpreter) -> std::io::Result<()> {
        for site_packages in self.site_packages(interpreter) {
            fs_err::create_dir_all(site_packages)?;
        }
        Ok(())
    }

    /// Return the path to the `--root` directory.
    pub fn root(&self) -> &Path {
        &self.0
    }
}

impl From<PathBuf> for Root {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn test_change_root_posix_style_1() {
        let root = Root(PathBuf::from("/new/root"));
        let path = "/usr/bin/python";
        let result = root.change_root(path);
        assert_eq!(result, PathBuf::from("/new/root/usr/bin/python"));
    }

    #[test]
    #[cfg(unix)]
    fn test_change_root_posix_style_2() {
        let root = Root(PathBuf::from("/new/root"));
        let path = "bin/python";
        let result = root.change_root(path);
        assert_eq!(result, PathBuf::from("/new/root/bin/python"));
    }

    #[test]
    #[cfg(unix)]
    fn test_change_root_posix_style_3() {
        let root = Root(PathBuf::from("/"));
        let path = "/";
        let result = root.change_root(path);
        assert_eq!(result, PathBuf::from("/"));
    }

    #[test]
    #[cfg(unix)]
    fn test_change_root_posix_style_4() {
        let root = Root(PathBuf::from("."));
        let path = "/a";
        let result = root.change_root(path);
        assert_eq!(result, PathBuf::from("./a"));
    }

    #[test]
    #[cfg(unix)]
    fn test_change_root_posix_style_5() {
        let root = Root(PathBuf::from("/root/bin/../etc/."));
        let path = "/root/.././a";
        let result = root.change_root(path);
        assert_eq!(result, PathBuf::from("/root/bin/../etc/./root/.././a"));
    }

    #[test]
    #[cfg(windows)]
    fn test_change_root_nt_style_1() {
        let root = Root(PathBuf::from("C:/a/b"));
        let path = "D:\\e/f";
        let result = root.change_root(path);
        assert_eq!(result, PathBuf::from("C:/a/b/e/f"));
    }

    #[test]
    #[cfg(windows)]
    fn test_change_root_nt_style_2() {
        let root = Root(PathBuf::from("C:/a/b"));
        let path = "c";
        let result = root.change_root(path);
        assert_eq!(result, PathBuf::from("C:\\a\\b\\c"));
    }

    #[test]
    #[cfg(windows)]
    fn test_change_root_nt_style_3() {
        let root = Root(PathBuf::from("a/b"));
        let path = "D:\\c";
        let result = root.change_root(path);
        assert_eq!(result, PathBuf::from("a\\b\\c"));
    }
}
