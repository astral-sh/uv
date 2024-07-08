use std::path::{Path, PathBuf};

use pypi_types::Scheme;

/// A `--prefix` directory into which packages can be installed, separate from a virtual environment
/// or system Python interpreter.
#[derive(Debug, Clone)]
pub struct Prefix(PathBuf);

impl Prefix {
    /// Return the [`Scheme`] for the `--prefix` directory.
    pub fn scheme(&self, virtualenv: &Scheme) -> Scheme {
        Scheme {
            purelib: self.0.join(&virtualenv.purelib),
            platlib: self.0.join(&virtualenv.platlib),
            scripts: self.0.join(&virtualenv.scripts),
            data: self.0.join(&virtualenv.data),
            include: self.0.join(&virtualenv.include),
        }
    }

    /// Return an iterator over the `site-packages` directories inside the environment.
    pub fn site_packages(&self, virtualenv: &Scheme) -> impl Iterator<Item = PathBuf> {
        std::iter::once(self.0.join(&virtualenv.purelib))
    }

    /// Initialize the `--prefix` directory.
    pub fn init(&self, virtualenv: &Scheme) -> std::io::Result<()> {
        for site_packages in self.site_packages(virtualenv) {
            fs_err::create_dir_all(site_packages)?;
        }
        Ok(())
    }

    /// Return the path to the `--prefix` directory.
    pub fn root(&self) -> &Path {
        &self.0
    }
}

impl From<PathBuf> for Prefix {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}
