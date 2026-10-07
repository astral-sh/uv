//! Compatibility checks for existing project and script environments.

use tracing::{debug, trace};
use uv_cache::Cache;
use uv_distribution_types::RequiresPython;
use uv_pep440::Version;
use uv_python::{
    PythonArchitecture, PythonEnvironment, PythonInstallation, PythonPreference, PythonRequest,
    PythonSource,
};

#[derive(Debug)]
pub(crate) enum EnvironmentKind {
    Script,
    Project,
}

impl std::fmt::Display for EnvironmentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Script => write!(f, "script"),
            Self::Project => write!(f, "project"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum EnvironmentIncompatibilityError {
    #[error("The {0} environment's Python version does not satisfy the request: `{1}`")]
    PythonRequest(EnvironmentKind, PythonRequest),

    #[error("The {0} environment's Python version does not meet the Python requirement: `{1}`")]
    RequiresPython(EnvironmentKind, RequiresPython),

    #[error(
        "The interpreter in the {0} environment has a different version ({1}) than it was created with ({2})"
    )]
    PyenvVersionConflict(EnvironmentKind, Version, Version),

    #[error("The {0} environment's Python interpreter does not meet the Python preference: `{1}`")]
    PythonPreference(EnvironmentKind, PythonPreference),
}

/// Check whether an environment satisfies the requested Python constraints.
pub(crate) fn check_environment_compatibility(
    environment: &PythonEnvironment,
    kind: EnvironmentKind,
    python_request: Option<&PythonRequest>,
    python_preference: PythonPreference,
    python_arch: Option<PythonArchitecture>,
    requires_python: Option<&RequiresPython>,
    cache: &Cache,
) -> Result<(), EnvironmentIncompatibilityError> {
    if let Some((cfg_version, int_version)) = environment.get_pyvenv_version_conflict() {
        return Err(EnvironmentIncompatibilityError::PyenvVersionConflict(
            kind,
            int_version,
            cfg_version,
        ));
    }

    let python_request = python_request
        .or_else(|| python_arch.map(|_| &PythonRequest::Any))
        .map(|request| request.with_default_arch(python_arch.map(PythonArchitecture::into_inner)));
    if let Some(request) = python_request {
        if request.satisfied(environment.interpreter(), cache) {
            debug!("The {kind} environment's Python version satisfies the request: `{request}`");
        } else {
            return Err(EnvironmentIncompatibilityError::PythonRequest(
                kind,
                request.into_owned(),
            ));
        }
    }

    if let Some(requires_python) = requires_python {
        if requires_python.contains(environment.interpreter().python_version()) {
            trace!(
                "The {kind} environment's Python version meets the Python requirement: `{requires_python}`"
            );
        } else {
            return Err(EnvironmentIncompatibilityError::RequiresPython(
                kind,
                requires_python.clone(),
            ));
        }
    }

    if python_preference.allows_installation(&PythonInstallation::new(
        PythonSource::DiscoveredEnvironment,
        environment.interpreter().clone(),
    )) {
        trace!(
            "The virtual environment's Python interpreter meets the Python preference: `{}`",
            python_preference
        );
    } else {
        return Err(EnvironmentIncompatibilityError::PythonPreference(
            kind,
            python_preference,
        ));
    }

    Ok(())
}
