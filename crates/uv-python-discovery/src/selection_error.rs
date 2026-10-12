//! Python request and compatibility failures.

use uv_distribution_types::RequiresPython;
use uv_pep440::Version;
use uv_workspace::{RequiresPythonSources, dependency_groups::DependencyGroupError};

use crate::{PythonRequirementConflicts, format_requires_python_sources};

/// A failure while discovering or validating a project or script interpreter.
#[derive(Debug, thiserror::Error)]
pub enum PythonSelectionError {
    #[error(
        "The requested interpreter resolved to Python {_0}, which is incompatible with the project's Python requirement: `{_1}`{_2}"
    )]
    RequestedPythonProjectIncompatibility(Version, RequiresPython, Box<PythonRequirementConflicts>),

    #[error(
        "The Python request from `{python_request}` resolved to Python {version}, which is incompatible with the project's Python requirement: `{requires_python}`{requires_python_sources}\nUse `uv python pin` to update the `.python-version` file to a compatible version"
    )]
    DotPythonVersionProjectIncompatibility {
        python_request: String,
        version: Version,
        requires_python: RequiresPython,
        requires_python_sources: Box<PythonRequirementConflicts>,
    },

    #[error(
        "The resolved Python interpreter (Python {_0}) is incompatible with the project's Python requirement: `{_1}`{_2}"
    )]
    RequiresPythonProjectIncompatibility(Version, RequiresPython, Box<PythonRequirementConflicts>),

    #[error(
        "The requested interpreter resolved to Python {0}, which is incompatible with the script's Python requirement: `{1}`"
    )]
    RequestedPythonScriptIncompatibility(Version, RequiresPython),

    #[error(
        "The Python request from `{0}` resolved to Python {1}, which is incompatible with the script's Python requirement: `{2}`"
    )]
    DotPythonVersionScriptIncompatibility(String, Version, RequiresPython),

    #[error(
        "The resolved Python interpreter (Python {0}) is incompatible with the script's Python requirement: `{1}`"
    )]
    RequiresPythonScriptIncompatibility(Version, RequiresPython),

    #[error(
        "Found conflicting Python requirements:\n{}",
        format_requires_python_sources(_0)
    )]
    DisjointRequiresPython(RequiresPythonSources),

    #[error(transparent)]
    Python(#[from] crate::Error),

    #[error(transparent)]
    DependencyGroup(#[from] DependencyGroupError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl uv_errors::Hinted for PythonSelectionError {
    fn hints(&self) -> uv_errors::Hints<'_> {
        match self {
            Self::Python(error) => error.hints(),
            Self::RequestedPythonProjectIncompatibility(..)
            | Self::DotPythonVersionProjectIncompatibility { .. }
            | Self::RequiresPythonProjectIncompatibility(..)
            | Self::RequestedPythonScriptIncompatibility(..)
            | Self::DotPythonVersionScriptIncompatibility(..)
            | Self::RequiresPythonScriptIncompatibility(..)
            | Self::DisjointRequiresPython(..)
            | Self::DependencyGroup(..)
            | Self::Io(..) => uv_errors::Hints::none(),
        }
    }
}
