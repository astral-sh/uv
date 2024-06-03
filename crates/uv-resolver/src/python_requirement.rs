use pep508_rs::StringVersion;
use uv_interpreter::{Interpreter, PythonVersion};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PythonRequirement {
    /// The installed version of Python.
    installed: StringVersion,
    /// The target version of Python; that is, the version of Python for which we are resolving
    /// dependencies. This is typically the same as the installed version, but may be different
    /// when specifying an alternate Python version for the resolution.
    ///
    /// If `None`, the target version is the same as the installed version.
    target: Option<StringVersion>,
}

impl PythonRequirement {
    /// Create a [`PythonRequirement`] to resolve against both an [`Interpreter`] and a
    /// [`PythonVersion`].
    pub fn from_python_version(interpreter: &Interpreter, python_version: &PythonVersion) -> Self {
        Self {
            installed: interpreter.python_full_version().clone(),
            target: Some(StringVersion {
                string: python_version.to_string(),
                version: python_version.python_full_version(),
            }),
        }
    }

    /// Create a [`PythonRequirement`] to resolve against an [`Interpreter`].
    pub fn from_interpreter(interpreter: &Interpreter) -> Self {
        Self {
            installed: interpreter.python_full_version().clone(),
            target: None,
        }
    }

    /// Return the installed version of Python.
    pub fn installed(&self) -> &StringVersion {
        &self.installed
    }

    /// Return the target version of Python.
    pub fn target(&self) -> Option<&StringVersion> {
        self.target.as_ref()
    }
}
