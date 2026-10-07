//! Python requests and compatibility validation for PEP 723 scripts.

use std::path::Path;

use tracing::debug;
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_distribution_types::RequiresPython;
use uv_fs::CWD;
use uv_python::{
    ConfigDiscovery, EnvironmentPreference, Interpreter, PythonArchitecture, PythonDownloads,
    PythonInstallation, PythonPreference, PythonRequest, PythonVariant, PythonVersionFile,
    VersionFileDiscoveryOptions, VersionRequest,
};
use uv_scripts::Pep723ItemRef;
use uv_settings::PythonInstallMirrors;

use crate::{PythonContextError, PythonDownloadReporter, PythonRequestSource};

/// Determine the [`RequiresPython`] requirement for a new PEP 723 script.
pub async fn init_script_python_requirement(
    python: Option<&str>,
    install_mirrors: &PythonInstallMirrors,
    directory: &Path,
    no_pin_python: bool,
    python_preference: PythonPreference,
    python_arch: Option<PythonArchitecture>,
    python_downloads: PythonDownloads,
    config_discovery: ConfigDiscovery,
    client_builder: &BaseClientBuilder<'_>,
    cache: &Cache,
    reporter: &PythonDownloadReporter,
) -> Result<RequiresPython, PythonContextError> {
    let python_request = if let Some(request) = python {
        // (1) Explicit request from user
        Some(PythonRequest::parse(request))
    } else if let (false, Some(request)) = (
        no_pin_python,
        PythonVersionFile::discover(
            directory,
            &VersionFileDiscoveryOptions::default().with_config_discovery(config_discovery),
        )
        .await?
        .and_then(PythonVersionFile::into_version),
    ) {
        // (2) Request from `.python-version`
        Some(request)
    } else {
        // (3) No explicit request
        None
    };

    let interpreter = PythonInstallation::find_or_download(
        python_request.as_ref(),
        EnvironmentPreference::Any,
        python_preference,
        python_arch,
        python_downloads,
        client_builder,
        cache,
        Some(reporter),
        install_mirrors.mirrors(),
        install_mirrors.python_downloads_json_url.as_deref(),
    )
    .await?
    .into_interpreter();

    Ok(RequiresPython::greater_than_equal_version(
        &interpreter.python_minor_version(),
    ))
}

/// Returns an error if the [`Interpreter`] does not satisfy the script's `requires-python`.
fn validate_script_requires_python(
    interpreter: &Interpreter,
    requires_python: &RequiresPython,
    source: &PythonRequestSource,
) -> Result<(), PythonContextError> {
    if requires_python.contains(interpreter.python_version()) {
        return Ok(());
    }
    match source {
        PythonRequestSource::UserRequest => {
            Err(PythonContextError::RequestedPythonScriptIncompatibility(
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
        PythonRequestSource::DotPythonVersion(file) => {
            Err(PythonContextError::DotPythonVersionScriptIncompatibility(
                file.file_name().to_string(),
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
        PythonRequestSource::RequiresPython => {
            Err(PythonContextError::RequiresPythonScriptIncompatibility(
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
    }
}

/// The resolved Python request and requirement for a [`Pep723Script`]
#[derive(Debug, Clone)]
pub struct ScriptPythonRequest {
    /// The source of the Python request.
    source: PythonRequestSource,
    /// The resolved Python request, computed by considering (1) any explicit request from the user
    /// via `--python`, (2) any implicit request from the user via `.python-version`, (3) any
    /// `Requires-Python` specifier in the script metadata.
    pub python_request: Option<PythonRequest>,
    /// The resolved Python requirement for the script.
    requires_python: Option<RequiresPython>,
}

impl ScriptPythonRequest {
    /// Determine the [`ScriptPythonRequest`] for the current [`Pep723Script`].
    pub async fn from_request(
        python_request: Option<PythonRequest>,
        script: Pep723ItemRef<'_>,
        config_discovery: ConfigDiscovery,
    ) -> Result<Self, PythonContextError> {
        let requires_python = script
            .metadata()
            .requires_python
            .as_ref()
            .map(|specifiers| RequiresPython::from_specifiers(specifiers.clone()));

        let project_dir = script.path().and_then(Path::parent).unwrap_or(&**CWD);

        let (source, python_request) = if let Some(request) = python_request {
            // (1) Explicit request from user
            (PythonRequestSource::UserRequest, Some(request))
        } else if let Some(file) = PythonVersionFile::discover(
            project_dir,
            &VersionFileDiscoveryOptions::default().with_config_discovery(config_discovery),
        )
        .await?
        .filter(|file| {
            // Ignore version files that are incompatible with the script's `requires-python`
            match (file.version(), requires_python.as_ref()) {
                (Some(request), Some(requires_python)) => {
                    request.intersects_requires_python(requires_python)
                }
                _ => true,
            }
        }) {
            // (2) Request from `.python-version`
            (
                PythonRequestSource::DotPythonVersion(file.clone()),
                file.version().cloned(),
            )
        } else if let Some(specifiers) = script.metadata().requires_python.as_ref() {
            // (3) `requires-python` from script metadata
            let request = PythonRequest::Version(VersionRequest::from_specifiers(
                specifiers.clone(),
                PythonVariant::Default,
            ));
            (PythonRequestSource::RequiresPython, Some(request))
        } else {
            // (4) No Python request or requirement.
            (PythonRequestSource::RequiresPython, None)
        };

        if let Some(python_request) = python_request.as_ref() {
            debug!(
                "Using Python request `{}` from {source}",
                python_request.to_canonical_string()
            );
        }

        Ok(Self {
            source,
            python_request,
            requires_python,
        })
    }

    /// Return the script's Python requirement, if it declares one.
    pub fn requires_python(&self) -> Option<&RequiresPython> {
        self.requires_python.as_ref()
    }

    /// Check whether an interpreter satisfies the script's Python requirement.
    pub fn check(&self, interpreter: &Interpreter) -> Result<(), PythonContextError> {
        if let Some(requires_python) = &self.requires_python {
            validate_script_requires_python(interpreter, requires_python, &self.source)?;
        }
        Ok(())
    }
}
