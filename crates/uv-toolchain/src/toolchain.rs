use uv_configuration::PreviewMode;

use uv_cache::Cache;

use crate::discovery::{SystemPython, ToolchainRequest, ToolchainSources};
use crate::{find_default_toolchain, find_toolchain, Error, Interpreter, ToolchainSource};

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
    pub fn find(
        python: Option<&str>,
        system: SystemPython,
        preview: PreviewMode,
        cache: &Cache,
    ) -> Result<Self, Error> {
        if let Some(python) = python {
            Self::from_requested_python(python, system, preview, cache)
        } else if system.is_preferred() {
            Self::from_default_python(preview, cache)
        } else {
            // First check for a parent intepreter
            // We gate this check to avoid an extra log message when it is not set
            if std::env::var_os("UV_INTERNAL__PARENT_INTERPRETER").is_some() {
                match Self::from_parent_interpreter(system, cache) {
                    Ok(env) => return Ok(env),
                    Err(Error::NotFound(_)) => {}
                    Err(err) => return Err(err),
                }
            }

            // Then a virtual environment
            match Self::from_virtualenv(cache) {
                Ok(venv) => Ok(venv),
                Err(Error::NotFound(_)) if system.is_allowed() => {
                    Self::from_default_python(preview, cache)
                }
                Err(err) => Err(err),
            }
        }
    }

    /// Find an installed [`Toolchain`] in an existing virtual environment.
    ///
    /// Allows Conda environments (via `CONDA_PREFIX`) though they are not technically virtual environments.
    pub fn from_virtualenv(cache: &Cache) -> Result<Self, Error> {
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

    /// Find an installed [`Toolchain`] belonging to the parent interpreter i.e. the executable in `python -m uv ...`
    pub fn from_parent_interpreter(system: SystemPython, cache: &Cache) -> Result<Self, Error> {
        let sources = ToolchainSources::from_sources([ToolchainSource::ParentInterpreter]);
        let request = ToolchainRequest::Any;
        let toolchain = find_toolchain(&request, system, &sources, cache)??;
        Ok(toolchain)
    }

    /// Find an installed [`Toolchain`] that satisfies a request.
    pub fn from_requested_python(
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

    /// Find an installed [`Toolchain`] for the default Python interpreter.
    pub fn from_default_python(preview: PreviewMode, cache: &Cache) -> Result<Self, Error> {
        let toolchain = find_default_toolchain(preview, cache)??;
        Ok(toolchain)
    }

    /// Find an installed [`Toolchain`] from an existing [`Interpreter`].
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
