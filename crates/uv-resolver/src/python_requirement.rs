use std::collections::Bound;

use uv_distribution_types::{RequiresPython, RequiresPythonRange};
use uv_pep440::Version;
use uv_pep508::{MarkerEnvironment, MarkerTree};
use uv_python::{Interpreter, PythonVersion};

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
    /// The canonical marker representation of the target Python requirement.
    target_marker: MarkerTree,
}

impl PythonRequirement {
    fn new(
        source: PythonRequirementSource,
        exact: Version,
        installed: RequiresPython,
        target: RequiresPython,
    ) -> Self {
        let target_marker = target.to_marker_tree();
        Self {
            source,
            exact,
            installed,
            target,
            target_marker,
        }
    }

    /// Create a [`PythonRequirement`] to resolve against both an [`Interpreter`] and a
    /// [`PythonVersion`].
    pub fn from_python_version(interpreter: &Interpreter, python_version: &PythonVersion) -> Self {
        let exact = interpreter.python_full_version().version.clone();
        let installed = interpreter
            .python_full_version()
            .version
            .only_release()
            .without_trailing_zeros();
        let target = python_version
            .python_full_version()
            .only_release()
            .without_trailing_zeros();
        Self::new(
            PythonRequirementSource::PythonVersion,
            exact,
            RequiresPython::greater_than_equal_version(&installed),
            RequiresPython::greater_than_equal_version(&target),
        )
    }

    /// Create a [`PythonRequirement`] to resolve against both an [`Interpreter`] and a
    /// [`MarkerEnvironment`].
    pub fn from_requires_python(
        interpreter: &Interpreter,
        requires_python: RequiresPython,
    ) -> Self {
        Self::from_marker_environment(interpreter.markers(), requires_python)
    }

    /// Create a [`PythonRequirement`] to resolve against an [`Interpreter`].
    pub fn from_interpreter(interpreter: &Interpreter) -> Self {
        let exact = interpreter
            .python_full_version()
            .version
            .clone()
            .without_trailing_zeros();
        let installed = interpreter
            .python_full_version()
            .version
            .only_release()
            .without_trailing_zeros();
        Self::new(
            PythonRequirementSource::Interpreter,
            exact,
            RequiresPython::greater_than_equal_version(&installed),
            RequiresPython::greater_than_equal_version(&installed),
        )
    }

    /// Create a [`PythonRequirement`] from a [`MarkerEnvironment`] and a
    /// specific `Requires-Python` directive.
    ///
    /// This has the same "source" as
    /// [`PythonRequirement::from_requires_python`], but is useful for
    /// constructing a `PythonRequirement` without an [`Interpreter`].
    pub fn from_marker_environment(
        marker_env: &MarkerEnvironment,
        requires_python: RequiresPython,
    ) -> Self {
        let exact = marker_env
            .python_full_version()
            .version
            .clone()
            .without_trailing_zeros();
        let installed = marker_env
            .python_full_version()
            .version
            .only_release()
            .without_trailing_zeros();
        Self::new(
            PythonRequirementSource::RequiresPython,
            exact,
            RequiresPython::greater_than_equal_version(&installed),
            requires_python,
        )
    }

    /// Narrow the [`PythonRequirement`] to the given version, if it's stricter (i.e., greater)
    /// than the current `Requires-Python` minimum.
    ///
    /// Returns `None` if the given range is not narrower than the current range.
    pub(crate) fn narrow(&self, target: &RequiresPythonRange) -> Option<Self> {
        Some(Self::new(
            self.source,
            self.exact.clone(),
            self.installed.clone(),
            self.target.narrow(target)?,
        ))
    }

    /// Split the [`PythonRequirement`] at the given version.
    ///
    /// For example, if the current requirement is `>=3.10`, and the split point is `3.11`, then
    /// the result will be `>=3.10 and <3.11` and `>=3.11`.
    pub(crate) fn split(&self, at: Bound<Version>) -> Option<(Self, Self)> {
        let (lower, upper) = self.target.split(at)?;
        Some((
            Self::new(
                self.source,
                self.exact.clone(),
                self.installed.clone(),
                lower,
            ),
            Self::new(
                self.source,
                self.exact.clone(),
                self.installed.clone(),
                upper,
            ),
        ))
    }

    /// Returns `true` if the minimum version of Python required by the target is greater than the
    /// installed version.
    pub(crate) fn raises(&self, target: &RequiresPythonRange) -> bool {
        target.lower() > self.target.range().lower()
    }

    /// Return the exact version of Python.
    pub(crate) fn exact(&self) -> &Version {
        &self.exact
    }

    /// Return the installed version of Python.
    pub(crate) fn installed(&self) -> &RequiresPython {
        &self.installed
    }

    /// Return the target version of Python.
    pub(crate) fn target(&self) -> &RequiresPython {
        &self.target
    }

    /// Return the source of the [`PythonRequirement`].
    pub(crate) fn source(&self) -> PythonRequirementSource {
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
    pub(crate) fn to_marker_tree(&self) -> MarkerTree {
        self.target_marker
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
