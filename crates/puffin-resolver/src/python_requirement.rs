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
    target: &'a Version,
}

impl<'a> PythonRequirement<'a> {
    pub fn new(interpreter: &'a Interpreter, markers: &'a MarkerEnvironment) -> Self {
        Self {
            installed: interpreter.version(),
            target: &markers.python_version.version,
        }
    }

    /// Return the installed version of Python.
    pub(crate) fn installed(&self) -> &'a Version {
        self.installed
    }

    /// Return the target version of Python.
    pub(crate) fn target(&self) -> &'a Version {
        self.target
    }

    /// Returns an iterator over the versions of Python to consider when resolving dependencies.
    pub(crate) fn versions(&self) -> impl Iterator<Item = &'a Version> {
        std::iter::once(self.installed).chain(std::iter::once(self.target))
    }
}
