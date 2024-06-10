use tracing::{debug, info};
use uv_client::BaseClientBuilder;
use uv_configuration::PreviewMode;

use uv_cache::Cache;

use crate::discovery::{
    find_best_toolchain, find_default_toolchain, find_toolchain, SystemPython, ToolchainRequest,
    ToolchainSources,
};
use crate::downloads::{DownloadResult, PythonDownload, PythonDownloadRequest};
use crate::managed::{InstalledToolchain, InstalledToolchains};
use crate::{Error, Interpreter, ToolchainSource};

/// A Python interpreter and accompanying tools.
#[derive(Clone, Debug)]
pub struct Toolchain {
    // Public in the crate for test assertions
    pub(crate) source: ToolchainSource,
    pub(crate) interpreter: Interpreter,
}

impl Toolchain {
    /// Find an installed [`Toolchain`].
    ///
    /// This is the standard interface for discovering a Python toolchain for use with uv.
    ///
    /// See [`uv-toolchain::discovery`] for implementation details.
    pub fn find(
        python: Option<&str>,
        system: SystemPython,
        preview: PreviewMode,
        cache: &Cache,
    ) -> Result<Self, Error> {
        if let Some(python) = python {
            Self::find_requested(python, system, preview, cache)
        } else if system.is_preferred() {
            Self::find_default(preview, cache)
        } else {
            // First check for a parent interpreter
            // We gate this check to avoid an extra log message when it is not set
            if std::env::var_os("UV_INTERNAL__PARENT_INTERPRETER").is_some() {
                match Self::find_parent_interpreter(system, cache) {
                    Ok(env) => return Ok(env),
                    Err(Error::NotFound(_)) => {}
                    Err(err) => return Err(err),
                }
            }

            // Then a virtual environment
            match Self::find_virtualenv(cache) {
                Ok(venv) => Ok(venv),
                Err(Error::NotFound(_)) if system.is_allowed() => {
                    Self::find_default(preview, cache)
                }
                Err(err) => Err(err),
            }
        }
    }

    /// Find an installed [`Toolchain`] that satisfies a request.
    pub fn find_requested(
        request: &str,
        system: SystemPython,
        preview: PreviewMode,
        cache: &Cache,
    ) -> Result<Self, Error> {
        let sources = ToolchainSources::from_settings(system, preview);
        let request = ToolchainRequest::parse(request);
        let toolchain = find_toolchain(&request, system, &sources, cache)??;

        Ok(toolchain)
    }

    /// Find an installed [`Toolchain`] that satisfies a requested version, if the request cannot
    /// be satisfied, fallback to the best available toolchain.
    pub fn find_best(
        request: &ToolchainRequest,
        system: SystemPython,
        preview: PreviewMode,
        cache: &Cache,
    ) -> Result<Self, Error> {
        Ok(find_best_toolchain(request, system, preview, cache)??)
    }

    /// Find an installed [`Toolchain`] in an existing virtual environment.
    ///
    /// Allows Conda environments (via `CONDA_PREFIX`) though they are not technically virtual environments.
    pub fn find_virtualenv(cache: &Cache) -> Result<Self, Error> {
        let sources = ToolchainSources::VirtualEnv;
        let request = ToolchainRequest::Any;
        let toolchain = find_toolchain(&request, SystemPython::Disallowed, &sources, cache)??;

        debug_assert!(
            toolchain.interpreter().is_virtualenv()
                || matches!(toolchain.source(), ToolchainSource::CondaPrefix),
            "Not a virtualenv (source: {}, prefix: {})",
            toolchain.source(),
            toolchain.interpreter().sys_base_prefix().display()
        );

        Ok(toolchain)
    }

    /// Find the [`Toolchain`] belonging to the parent interpreter i.e. from `python -m uv ...`
    ///
    /// If not spawned by `python -m uv`, the toolchain will not be found.
    pub fn find_parent_interpreter(system: SystemPython, cache: &Cache) -> Result<Self, Error> {
        let sources = ToolchainSources::from_sources([ToolchainSource::ParentInterpreter]);
        let request = ToolchainRequest::Any;
        let toolchain = find_toolchain(&request, system, &sources, cache)??;
        Ok(toolchain)
    }

    /// Find the default installed [`Toolchain`].
    pub fn find_default(preview: PreviewMode, cache: &Cache) -> Result<Self, Error> {
        let toolchain = find_default_toolchain(preview, cache)??;
        Ok(toolchain)
    }

    /// Find or fetch a [`Toolchain`].
    ///
    /// Unlike [`Toolchain::find`], if the toolchain is not installed it will be installed automatically.
    pub async fn find_or_fetch<'a>(
        python: Option<&str>,
        system: SystemPython,
        preview: PreviewMode,
        client_builder: BaseClientBuilder<'a>,
        cache: &Cache,
    ) -> Result<Self, Error> {
        // Perform a find first
        match Self::find(python, system, preview, cache) {
            Ok(venv) => Ok(venv),
            Err(Error::NotFound(_)) if system.is_allowed() && preview.is_enabled() => {
                debug!("Requested Python not found, checking for available download...");
                let request = if let Some(request) = python {
                    ToolchainRequest::parse(request)
                } else {
                    ToolchainRequest::default()
                };
                Self::fetch(request, client_builder, cache).await
            }
            Err(err) => Err(err),
        }
    }

    pub async fn fetch<'a>(
        request: ToolchainRequest,
        client_builder: BaseClientBuilder<'a>,
        cache: &Cache,
    ) -> Result<Self, Error> {
        let toolchains = InstalledToolchains::from_settings()?.init()?;
        let toolchain_dir = toolchains.root();

        let request = PythonDownloadRequest::from_request(request)?.fill()?;
        let download = PythonDownload::from_request(&request)?;
        let client = client_builder.build();

        info!("Fetching requested toolchain...");
        let result = download.fetch(&client, toolchain_dir).await?;

        let path = match result {
            DownloadResult::AlreadyAvailable(path) => path,
            DownloadResult::Fetched(path) => path,
        };

        let installed = InstalledToolchain::new(path)?;

        Ok(Self {
            source: ToolchainSource::Managed,
            interpreter: Interpreter::query(installed.executable(), cache)?,
        })
    }

    /// Create a [`Toolchain`] from an existing [`Interpreter`].
    pub fn from_interpreter(interpreter: Interpreter) -> Self {
        Self {
            source: ToolchainSource::ProvidedPath,
            interpreter,
        }
    }

    pub fn source(&self) -> &ToolchainSource {
        &self.source
    }

    pub fn interpreter(&self) -> &Interpreter {
        &self.interpreter
    }

    pub fn into_interpreter(self) -> Interpreter {
        self.interpreter
    }
}
