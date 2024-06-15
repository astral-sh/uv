use std::fmt;
use std::str::FromStr;

use pep440_rs::Version;
use tracing::{debug, info};
use uv_client::BaseClientBuilder;
use uv_configuration::PreviewMode;

use uv_cache::Cache;

use crate::discovery::{
    find_best_toolchain, find_default_toolchain, find_toolchain, SystemPython, ToolchainRequest,
    ToolchainSources,
};
use crate::downloads::{DownloadResult, PythonDownload, PythonDownloadRequest};
use crate::implementation::{LenientImplementationName};
use crate::managed::{InstalledToolchain, InstalledToolchains};
use crate::platform::{Arch, Libc, Os};
use crate::{Error, Interpreter, PythonVersion, ToolchainSource};

/// A Python interpreter and accompanying tools.
#[derive(Clone, Debug)]
pub struct Toolchain {
    // Public in the crate for test assertions
    pub(crate) source: ToolchainSource,
    pub(crate) interpreter: Interpreter,
}

impl Toolchain {
    /// Create a new [`Toolchain`] from a source, interpreter tuple.
    pub(crate) fn from_tuple(tuple: (ToolchainSource, Interpreter)) -> Self {
        let (source, interpreter) = tuple;
        Self {
            source,
            interpreter,
        }
    }

    /// Find an installed [`Toolchain`].
    ///
    /// This is the standard interface for discovering a Python toolchain for use with uv.
    ///
    /// See [`uv-toolchain::discovery`] for implementation details.
    pub fn find(
        python: Option<ToolchainRequest>,
        system: SystemPython,
        preview: PreviewMode,
        cache: &Cache,
    ) -> Result<Self, Error> {
        if let Some(request) = python {
            Self::find_requested(&request, system, preview, cache)
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
        request: &ToolchainRequest,
        system: SystemPython,
        preview: PreviewMode,
        cache: &Cache,
    ) -> Result<Self, Error> {
        let sources = ToolchainSources::from_settings(system, preview);
        let toolchain = find_toolchain(request, system, &sources, cache)??;

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
        python: Option<ToolchainRequest>,
        system: SystemPython,
        preview: PreviewMode,
        client_builder: BaseClientBuilder<'a>,
        cache: &Cache,
    ) -> Result<Self, Error> {
        // Perform a find first
        match Self::find(python.clone(), system, preview, cache) {
            Ok(venv) => Ok(venv),
            Err(Error::NotFound(_)) if system.is_allowed() && preview.is_enabled() => {
                debug!("Requested Python not found, checking for available download...");
                let request = if let Some(request) = python {
                    request
                } else {
                    ToolchainRequest::default()
                };
                Self::fetch(request, client_builder, cache).await
            }
            Err(err) => Err(err),
        }
    }

    /// Download and install the requested toolchain.
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
        installed.ensure_externally_managed()?;

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

    /// Return the [`ToolchainSource`] of the toolchain, indicating where it was found.
    pub fn source(&self) -> &ToolchainSource {
        &self.source
    }

    pub fn key(&self) -> ToolchainKey {
        ToolchainKey::new(
            LenientImplementationName::from(self.interpreter.implementation_name()),
            self.interpreter.python_major(),
            self.interpreter.python_minor(),
            self.interpreter.python_patch(),
            self.os(),
            self.arch(),
            self.libc(),
        )
    }

    /// Return the Python [`Version`] of the toolchain as reported by its interpreter.
    pub fn python_version(&self) -> &Version {
        self.interpreter.python_version()
    }

    /// Return the [`LenientImplementationName`] of the toolchain as reported by its interpreter.
    pub fn implementation(&self) -> LenientImplementationName {
        LenientImplementationName::from(self.interpreter.implementation_name())
    }

    /// Return the [`Arch`] of the toolchain as reported by its interpreter.
    pub fn arch(&self) -> Arch {
        Arch::from(&self.interpreter.platform().arch())
    }

    /// Return the [`Libc`] of the toolchain as reported by its interpreter.
    pub fn libc(&self) -> Libc {
        Libc::from(self.interpreter.platform().os())
    }

    /// Return the [`Os`] of the toolchain as reported by its interpreter.
    pub fn os(&self) -> Os {
        Os::from(self.interpreter.platform().os())
    }

    /// Return the [`Interpreter`] for the toolchain.
    pub fn interpreter(&self) -> &Interpreter {
        &self.interpreter
    }

    pub fn into_interpreter(self) -> Interpreter {
        self.interpreter
    }
}

#[derive(Error, Debug)]
pub enum ToolchainKeyError {
    #[error("Failed to parse toolchain key `{0}`: {1}")]
    ParseError(String, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainKey {
    pub(crate) implementation: LenientImplementationName,
    pub(crate) major: u8,
    pub(crate) minor: u8,
    pub(crate) patch: u8,
    pub(crate) os: Os,
    pub(crate) arch: Arch,
    pub(crate) libc: Libc,
}

impl ToolchainKey {
    pub fn new(
        implementation: LenientImplementationName,
        major: u8,
        minor: u8,
        patch: u8,
        os: Os,
        arch: Arch,
        libc: Libc,
    ) -> Self {
        Self {
            implementation,
            major,
            minor,
            patch,
            os,
            arch,
            libc,
        }
    }

    pub fn implementation(&self) -> &LenientImplementationName {
        &self.implementation
    }

    pub fn version(&self) -> PythonVersion {
        PythonVersion::from_str(&format!("{}.{}.{}", self.major, self.minor, self.patch))
            .expect("Toolchain keys must have valid Python versions")
    }

    pub fn arch(&self) -> &Arch {
        &self.arch
    }

    pub fn os(&self) -> &Os {
        &self.os
    }

    pub fn libc(&self) -> &Libc {
        &self.libc
    }
}

impl fmt::Display for ToolchainKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}.{}.{}-{}-{}-{}",
            self.implementation,
            self.major,
            self.minor,
            self.patch,
            self.os,
            self.arch,
            self.libc
        )
    }
}

impl FromStr for ToolchainKey {
    type Err = ToolchainKeyError;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        let parts = key.split('-').collect::<Vec<_>>();
        let [implementation, version, os, arch, libc] = parts.as_slice() else {
            return Err(ToolchainKeyError::ParseError(
                key.to_string(),
                "not enough `-`-separated values".to_string(),
            ));
        };

        let implementation = LenientImplementationName::from(*implementation);

        let os = Os::from_str(os).map_err(|err| {
            ToolchainKeyError::ParseError(key.to_string(), format!("invalid OS: {err}"))
        })?;

        let arch = Arch::from_str(arch).map_err(|err| {
            ToolchainKeyError::ParseError(key.to_string(), format!("invalid architecture: {err}"))
        })?;

        let libc = Libc::from_str(libc).map_err(|err| {
            ToolchainKeyError::ParseError(key.to_string(), format!("invalid libc: {err}"))
        })?;

        let [major, minor, patch] = version
            .splitn(3, '.')
            .map(str::parse::<u8>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| {
                ToolchainKeyError::ParseError(
                    key.to_string(),
                    format!("invalid Python version: {err}"),
                )
            })?[..]
        else {
            return Err(ToolchainKeyError::ParseError(
                key.to_string(),
                "invalid Python version: expected `<major>.<minor>.<patch>`".to_string(),
            ));
        };

        Ok(Self::new(
            implementation,
            major,
            minor,
            patch,
            os,
            arch,
            libc,
        ))
    }
}

impl PartialOrd for ToolchainKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ToolchainKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_string().cmp(&other.to_string())
    }
}
