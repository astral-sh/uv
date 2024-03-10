use pep440_rs::Version;
use pep508_rs::MarkerEnvironment;
use uv_interpreter::Interpreter;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PythonRequirement {
    /// The installed version of Python.
    installed: Version,
    /// The target version of Python; that is, the version of Python for which we are resolving
    /// dependencies. This is typically the same as the installed version, but may be different
    /// when specifying an alternate Python version for the resolution.
    target: Version,
}

impl PythonRequirement {
    pub fn new(interpreter: &Interpreter, markers: &MarkerEnvironment) -> Self {
        Self {
            installed: interpreter.python_version().clone(),
            target: markers.python_full_version.version.clone(),
        }
    }

    /// Return the installed version of Python.
    pub fn installed(&self) -> &Version {
        &self.installed
    }

    /// Return the target version of Python.
    pub fn target(&self) -> &Version {
        &self.target
    }
}
