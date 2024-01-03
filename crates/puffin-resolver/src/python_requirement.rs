use pep440_rs::Version;
use pep508_rs::MarkerEnvironment;
use puffin_interpreter::Interpreter;

#[derive(Debug, Clone)]
pub struct PythonRequirement<'a> {
    /// The installed version of Python.
    installed: &'a Version,
    /// The target version of Python; that is, the version of Python for which we are resolving
    /// dependencies. This is typically the same as the installed version, but may be different
    /// when specifying an alternate Python version for the resolution.
    target: Option<&'a Version>,
}

impl<'a> PythonRequirement<'a> {
    pub fn new(interpreter: &'a Interpreter, markers: &'a MarkerEnvironment) -> Self {
        let installed = interpreter.version();
        let target = &markers.python_version.version;
        Self {
            installed,
            target: if installed == target {
                None
            } else {
                Some(target)
            },
        }
    }

    /// Return a version in the given range.
    pub(crate) fn version(&self) -> &'a Version {
        self.installed
    }

    /// Returns an iterator over the versions of Python to consider when resolving dependencies.
    pub(crate) fn versions(&self) -> impl Iterator<Item = &'a Version> {
        self.target
            .into_iter()
            .chain(std::iter::once(self.installed))
    }
}
