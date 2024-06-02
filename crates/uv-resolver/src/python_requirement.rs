use pep508_rs::{MarkerEnvironment, StringVersion};
use uv_interpreter::Interpreter;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PythonRequirement {
    /// The installed version of Python.
    installed: StringVersion,
    /// The target version of Python; that is, the version of Python for which we are resolving
    /// dependencies. This is typically the same as the installed version, but may be different
    /// when specifying an alternate Python version for the resolution.
    target: StringVersion,
}

impl PythonRequirement {
    pub fn from_marker_environment(interpreter: &Interpreter, env: &MarkerEnvironment) -> Self {
        Self {
            installed: interpreter.python_full_version().clone(),
            target: env.python_full_version().clone(),
        }
    }

    pub fn from_interpreter(interpreter: &Interpreter) -> Self {
        Self {
            installed: interpreter.python_full_version().clone(),
            target: interpreter.python_full_version().clone(),
        }
    }

    /// Return the installed version of Python.
    pub fn installed(&self) -> &StringVersion {
        &self.installed
    }

    /// Return the target version of Python.
    pub fn target(&self) -> &StringVersion {
        &self.target
    }
}
