use std::path::{Path, PathBuf};

use pypi_types::Scheme;

/// A `--target` directory into which packages can be installed, separate from a virtual environment
/// or system Python interpreter.
#[derive(Debug, Clone)]
pub struct Target(PathBuf);

impl Target {
    /// Return the [`Scheme`] for the `--target` directory.
    pub fn scheme(&self) -> Scheme {
        Scheme {
            purelib: self.0.clone(),
            platlib: self.0.clone(),
            scripts: self.0.join("bin"),
            data: self.0.clone(),
            include: self.0.join("include"),
        }
    }

    /// Return an iterator over the `site-packages` directories inside the environment.
    pub fn site_packages(&self) -> impl Iterator<Item = &Path> {
        std::iter::once(self.0.as_path())
    }

    /// Initialize the `--target` directory.
    pub fn init(&self) -> std::io::Result<()> {
        fs_err::create_dir_all(&self.0)?;
        Ok(())
    }

    /// Return the path to the `--target` directory.
    pub fn root(&self) -> &Path {
        &self.0
    }
}

impl From<PathBuf> for Target {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}
