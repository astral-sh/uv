use pep508_rs::PackageName;
use uv_python::PythonEnvironment;

/// Whether to enforce build isolation when building source distributions.
#[derive(Debug, Default, Copy, Clone)]
pub enum BuildIsolation<'a> {
    #[default]
    Isolated,
    Shared(&'a PythonEnvironment),
    SharedPackage(&'a PythonEnvironment, &'a [PackageName]),
}

impl<'a> BuildIsolation<'a> {
    /// Returns `true` if build isolation is enforced.
    pub fn is_isolated(&self, package_name: &Option<PackageName>) -> bool {
        match self {
            Self::Isolated => true,
            Self::Shared(_) => false,
            Self::SharedPackage(_, packages) => match package_name {
                Some(package_name) => !packages.iter().any(|p| p == package_name),
                None => true,
            },
        }
    }
}
