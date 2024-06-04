use pep440_rs::VersionSpecifiers;
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
    target: Option<RequiresPython>,
}

impl PythonRequirement {
    /// Create a [`PythonRequirement`] to resolve against both an [`Interpreter`] and a
    /// [`PythonVersion`].
    pub fn from_python_version(interpreter: &Interpreter, python_version: &PythonVersion) -> Self {
        Self {
            installed: interpreter.python_full_version().clone(),
            target: Some(RequiresPython::Specifier(StringVersion {
                string: python_version.to_string(),
                version: python_version.python_full_version(),
            })),
        }
    }

    /// Create a [`PythonRequirement`] to resolve against both an [`Interpreter`] and a
    /// [`MarkerEnvironment`].
    pub fn from_requires_python(
        interpreter: &Interpreter,
        requires_python: &VersionSpecifiers,
    ) -> Self {
        Self {
            installed: interpreter.python_full_version().clone(),
            target: Some(RequiresPython::Specifiers(requires_python.clone())),
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
    pub fn target(&self) -> Option<&RequiresPython> {
        self.target.as_ref()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RequiresPython {
    /// The `RequiresPython` specifier is a single version specifier, as provided via
    /// `--python-version` on the command line.
    ///
    /// The use of a separate enum variant allows us to use a verbatim representation when reporting
    /// back to the user.
    Specifier(StringVersion),
    /// The `RequiresPython` specifier is a set of version specifiers.
    Specifiers(VersionSpecifiers),
}

impl RequiresPython {
    /// Returns `true` if the target Python is covered by the [`VersionSpecifiers`].
    ///
    /// For example, if the target Python is `>=3.8`, then `>=3.7` would cover it. However, `>=3.9`
    /// would not.
    pub fn subset_of(&self, requires_python: &VersionSpecifiers) -> bool {
        match self {
            RequiresPython::Specifier(specifier) => requires_python.contains(specifier),
            RequiresPython::Specifiers(specifiers) => {
                let Ok(target) = crate::pubgrub::PubGrubSpecifier::try_from(specifiers) else {
                    return false;
                };

                let Ok(requires_python) =
                    crate::pubgrub::PubGrubSpecifier::try_from(requires_python)
                else {
                    return false;
                };

                target.subset_of(&requires_python)
            }
        }
    }
}

impl std::fmt::Display for RequiresPython {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequiresPython::Specifier(specifier) => std::fmt::Display::fmt(specifier, f),
            RequiresPython::Specifiers(specifiers) => std::fmt::Display::fmt(specifiers, f),
        }
    }
}
