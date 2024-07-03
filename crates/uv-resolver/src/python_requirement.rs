use pep440_rs::{Version, VersionSpecifiers};
use pep508_rs::MarkerTree;
use uv_python::{Interpreter, PythonVersion};

use crate::{RequiresPython, RequiresPythonBound};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PythonRequirement {
    /// The installed version of Python.
    installed: Version,
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
            installed: interpreter.python_full_version().version.only_release(),
            target: Some(PythonTarget::Version(
                python_version.python_full_version().only_release(),
            )),
        }
    }

    /// Create a [`PythonRequirement`] to resolve against both an [`Interpreter`] and a
    /// [`MarkerEnvironment`].
    pub fn from_requires_python(
        interpreter: &Interpreter,
        requires_python: &RequiresPython,
    ) -> Self {
        Self {
            installed: interpreter.python_full_version().version.only_release(),
            target: Some(PythonTarget::RequiresPython(requires_python.clone())),
        }
    }

    /// Create a [`PythonRequirement`] to resolve against an [`Interpreter`].
    pub fn from_interpreter(interpreter: &Interpreter) -> Self {
        Self {
            installed: interpreter.python_full_version().version.only_release(),
            target: None,
        }
    }

    /// Narrow the [`PythonRequirement`] to the given version, if it's stricter (i.e., greater)
    /// than the current `Requires-Python` minimum.
    pub fn narrow(&self, target: &RequiresPythonBound) -> Option<Self> {
        let Some(PythonTarget::RequiresPython(requires_python)) = self.target.as_ref() else {
            return None;
        };
        let requires_python = requires_python.narrow(target)?;
        Some(Self {
            installed: self.installed.clone(),
            target: Some(PythonTarget::RequiresPython(requires_python)),
        })
    }

    /// Return the installed version of Python.
    pub fn installed(&self) -> &Version {
        &self.installed
    }

    /// Return the target version of Python.
    pub fn target(&self) -> Option<&PythonTarget> {
        self.target.as_ref()
    }

    /// Return a [`MarkerTree`] representing the Python requirement.
    ///
    /// See: [`RequiresPython::to_marker_tree`]
    pub fn to_marker_tree(&self) -> Option<MarkerTree> {
        if let Some(PythonTarget::RequiresPython(requires_python)) = self.target.as_ref() {
            Some(requires_python.to_marker_tree())
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PythonTarget {
    /// The [`PythonTarget`] specifier is a single version specifier, as provided via
    /// `--python-version` on the command line.
    ///
    /// The use of a separate enum variant allows us to use a verbatim representation when reporting
    /// back to the user.
    Version(Version),
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
