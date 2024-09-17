use pep440_rs::Version;
use pep508_rs::MarkerTree;
use uv_python::{Interpreter, PythonVersion};

use crate::{RequiresPython, RequiresPythonRange};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PythonRequirement {
    source: PythonRequirementSource,
    /// The exact installed version of Python.
    exact: Version,
    /// The installed version of Python.
    installed: RequiresPython,
    /// The target version of Python; that is, the version of Python for which we are resolving
    /// dependencies. This is typically the same as the installed version, but may be different
    /// when specifying an alternate Python version for the resolution.
    target: RequiresPython,
}

impl PythonRequirement {
    /// Create a [`PythonRequirement`] to resolve against both an [`Interpreter`] and a
    /// [`PythonVersion`].
    pub fn from_python_version(interpreter: &Interpreter, python_version: &PythonVersion) -> Self {
        let exact = interpreter.python_full_version().version.clone();
        let installed = interpreter.python_full_version().version.only_release();
        let target = python_version.python_full_version().only_release();
        Self {
            exact,
            installed: RequiresPython::greater_than_equal_version(&installed),
            target: RequiresPython::greater_than_equal_version(&target),
            source: PythonRequirementSource::PythonVersion,
        }
    }

    /// Create a [`PythonRequirement`] to resolve against both an [`Interpreter`] and a
    /// [`MarkerEnvironment`].
    pub fn from_requires_python(
        interpreter: &Interpreter,
        requires_python: RequiresPython,
    ) -> Self {
        let exact = interpreter.python_full_version().version.clone();
        let installed = interpreter.python_full_version().version.only_release();
        Self {
            exact,
            installed: RequiresPython::greater_than_equal_version(&installed),
            target: requires_python,
            source: PythonRequirementSource::RequiresPython,
        }
    }

    /// Create a [`PythonRequirement`] to resolve against an [`Interpreter`].
    pub fn from_interpreter(interpreter: &Interpreter) -> Self {
        let exact = interpreter.python_full_version().version.clone();
        let installed = interpreter.python_full_version().version.only_release();
        Self {
            exact,
            installed: RequiresPython::greater_than_equal_version(&installed),
            target: RequiresPython::greater_than_equal_version(&installed),
            source: PythonRequirementSource::Interpreter,
        }
    }

    /// Narrow the [`PythonRequirement`] to the given version, if it's stricter (i.e., greater)
    /// than the current `Requires-Python` minimum.
    pub fn narrow(&self, target: &RequiresPythonRange) -> Option<Self> {
        Some(Self {
            exact: self.exact.clone(),
            installed: self.installed.clone(),
            target: self.target.narrow(target)?,
            source: self.source,
        })
    }

    /// Return the exact version of Python.
    pub fn exact(&self) -> &Version {
        &self.exact
    }

    /// Return the installed version of Python.
    pub fn installed(&self) -> &RequiresPython {
        &self.installed
    }

    /// Return the target version of Python.
    pub fn target(&self) -> &RequiresPython {
        &self.target
    }

    /// Return the source of the [`PythonRequirement`].
    pub fn source(&self) -> PythonRequirementSource {
        self.source
    }

    /// A wrapper around `RequiresPython::simplify_markers`. See its docs for
    /// more info.
    ///
    /// When this `PythonRequirement` isn't `RequiresPython`, the given markers
    /// are returned unchanged.
    pub(crate) fn simplify_markers(&self, marker: MarkerTree) -> MarkerTree {
        self.target.simplify_markers(marker)
    }

    /// Return a [`MarkerTree`] representing the Python requirement.
    ///
    /// See: [`RequiresPython::to_marker_tree`]
    pub fn to_marker_tree(&self) -> MarkerTree {
        self.target.to_marker_tree()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Hash, Ord)]
pub enum PythonRequirementSource {
    /// `--python-version`
    PythonVersion,
    /// `Requires-Python`
    RequiresPython,
    /// The discovered Python interpreter.
    Interpreter,
}
