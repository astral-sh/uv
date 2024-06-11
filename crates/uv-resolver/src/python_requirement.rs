use pep440_rs::VersionSpecifiers;
use pep508_rs::StringVersion;
use uv_toolchain::{Interpreter, PythonVersion};

use crate::RequiresPython;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PythonRequirement {
    /// The installed version of Python.
    installed: StringVersion,
    /// The target version of Python; that is, the version of Python for which we are resolving
    /// dependencies. This is typically the same as the installed version, but may be different
    /// when specifying an alternate Python version for the resolution.
    ///
    /// If `None`, the target version is the same as the installed version.
    target: Option<PythonTarget>,
}

impl PythonRequirement {
    /// Create a [`PythonRequirement`] to resolve against both an [`Interpreter`] and a
    /// [`PythonVersion`].
    pub fn from_python_version(interpreter: &Interpreter, python_version: &PythonVersion) -> Self {
        Self {
            installed: interpreter.python_full_version().clone(),
            target: Some(PythonTarget::Version(StringVersion {
                string: python_version.to_string(),
                version: python_version.python_full_version(),
            })),
        }
    }

    /// Create a [`PythonRequirement`] to resolve against both an [`Interpreter`] and a
    /// [`MarkerEnvironment`].
    pub fn from_requires_python(
        interpreter: &Interpreter,
        requires_python: &RequiresPython,
    ) -> Self {
        Self {
            installed: interpreter.python_full_version().clone(),
            target: Some(PythonTarget::RequiresPython(requires_python.clone())),
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
    pub fn target(&self) -> Option<&PythonTarget> {
        self.target.as_ref()
    }

    /// Return the target version of Python as a "requires python" type,
    /// if available.
    pub(crate) fn requires_python(&self) -> Option<&RequiresPython> {
        self.target().and_then(|target| target.as_requires_python())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PythonTarget {
    /// The [`PythonTarget`] specifier is a single version specifier, as provided via
    /// `--python-version` on the command line.
    ///
    /// The use of a separate enum variant allows us to use a verbatim representation when reporting
    /// back to the user.
    Version(StringVersion),
    /// The [`PythonTarget`] specifier is a set of version specifiers, as extracted from the
    /// `Requires-Python` field in a `pyproject.toml` or `METADATA` file.
    RequiresPython(RequiresPython),
}

impl PythonTarget {
    /// Returns `true` if the target Python is compatible with the [`VersionSpecifiers`].
    pub fn is_compatible_with(&self, target: &VersionSpecifiers) -> bool {
        match self {
            PythonTarget::Version(version) => target.contains(version),
            PythonTarget::RequiresPython(requires_python) => {
                requires_python.is_contained_by(target)
            }
        }
    }

    /// Returns the [`RequiresPython`] for the [`PythonTarget`] specifier.
    pub fn as_requires_python(&self) -> Option<&RequiresPython> {
        match self {
            PythonTarget::Version(_) => None,
            PythonTarget::RequiresPython(requires_python) => Some(requires_python),
        }
    }
}

impl std::fmt::Display for PythonTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PythonTarget::Version(specifier) => std::fmt::Display::fmt(specifier, f),
            PythonTarget::RequiresPython(specifiers) => std::fmt::Display::fmt(specifiers, f),
        }
    }
}
