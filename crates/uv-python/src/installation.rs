use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use tracing::{debug, info};

use pep440_rs::Version;
use uv_cache::Cache;
use uv_client::BaseClientBuilder;

use crate::discovery::{
    find_best_python_installation, find_python_installation, EnvironmentPreference, PythonRequest,
};
use crate::downloads::{DownloadResult, ManagedPythonDownload, PythonDownloadRequest, Reporter};
use crate::implementation::LenientImplementationName;
use crate::managed::{ManagedPythonInstallation, ManagedPythonInstallations};
use crate::platform::{Arch, Libc, Os};
use crate::{
    downloads, Error, Interpreter, PythonDownloads, PythonPreference, PythonSource, PythonVersion,
};

/// A Python interpreter and accompanying tools.
#[derive(Clone, Debug)]
pub struct PythonInstallation {
    // Public in the crate for test assertions
    pub(crate) source: PythonSource,
    pub(crate) interpreter: Interpreter,
}

impl PythonInstallation {
    /// Create a new [`PythonInstallation`] from a source, interpreter tuple.
    pub(crate) fn from_tuple(tuple: (PythonSource, Interpreter)) -> Self {
        let (source, interpreter) = tuple;
        Self {
            source,
            interpreter,
        }
    }

    /// Find an installed [`PythonInstallation`].
    ///
    /// This is the standard interface for discovering a Python installation for creating
    /// an environment. If interested in finding an existing environment, see
    /// [`PythonEnvironment::find`] instead.
    ///
    /// Note we still require an [`EnvironmentPreference`] as this can either bypass virtual environments
    /// or prefer them. In most cases, this should be [`EnvironmentPreference::OnlySystem`]
    /// but if you want to allow an interpreter from a virtual environment if it satisfies the request,
    /// then use [`EnvironmentPreference::Any`].
    ///
    /// See [`find_installation`] for implementation details.
    pub fn find(
        request: &PythonRequest,
        environments: EnvironmentPreference,
        preference: PythonPreference,
        cache: &Cache,
    ) -> Result<Self, Error> {
        let installation = find_python_installation(request, environments, preference, cache)??;
        Ok(installation)
    }

    /// Find an installed [`PythonInstallation`] that satisfies a requested version, if the request cannot
    /// be satisfied, fallback to the best available Python installation.
    pub fn find_best(
        request: &PythonRequest,
        environments: EnvironmentPreference,
        preference: PythonPreference,
        cache: &Cache,
    ) -> Result<Self, Error> {
        Ok(find_best_python_installation(
            request,
            environments,
            preference,
            cache,
        )??)
    }

    /// Find or fetch a [`PythonInstallation`].
    ///
    /// Unlike [`PythonInstallation::find`], if the required Python is not installed it will be installed automatically.
    pub async fn find_or_download<'a>(
        request: Option<&PythonRequest>,
        environments: EnvironmentPreference,
        preference: PythonPreference,
        python_downloads: PythonDownloads,
        client_builder: &BaseClientBuilder<'a>,
        cache: &Cache,
        reporter: Option<&dyn Reporter>,
    ) -> Result<Self, Error> {
        let request = request.unwrap_or_else(|| &PythonRequest::Any);

        // Search for the installation
        match Self::find(request, environments, preference, cache) {
            Ok(venv) => Ok(venv),
            // If missing and allowed, perform a fetch
            Err(Error::MissingPython(err))
                if preference.allows_managed()
                    && python_downloads.is_automatic()
                    && client_builder.connectivity.is_online() =>
            {
                if let Some(request) = PythonDownloadRequest::from_request(request) {
                    debug!("Requested Python not found, checking for available download...");
                    match Self::fetch(request.fill()?, client_builder, cache, reporter).await {
                        Ok(installation) => Ok(installation),
                        Err(Error::Download(downloads::Error::NoDownloadFound(_))) => {
                            Err(Error::MissingPython(err))
                        }
                        Err(err) => Err(err),
                    }
                } else {
                    Err(Error::MissingPython(err))
                }
            }
            Err(err) => Err(err),
        }
    }

    /// Download and install the requested installation.
    pub async fn fetch<'a>(
        request: PythonDownloadRequest,
        client_builder: &BaseClientBuilder<'a>,
        cache: &Cache,
        reporter: Option<&dyn Reporter>,
    ) -> Result<Self, Error> {
        let installations = ManagedPythonInstallations::from_settings()?.init()?;
        let installations_dir = installations.root();
        let cache_dir = installations.cache();
        let _lock = installations.lock().await?;

        let download = ManagedPythonDownload::from_request(&request)?;
        let client = client_builder.build();

        info!("Fetching requested Python...");
        let result = download
            .fetch(&client, installations_dir, &cache_dir, reporter)
            .await?;

        let path = match result {
            DownloadResult::AlreadyAvailable(path) => path,
            DownloadResult::Fetched(path) => path,
        };

        let installed = ManagedPythonInstallation::new(path)?;
        installed.ensure_externally_managed()?;

        Ok(Self {
            source: PythonSource::Managed,
            interpreter: Interpreter::query(installed.executable(), cache)?,
        })
    }

    /// Create a [`PythonInstallation`] from an existing [`Interpreter`].
    pub fn from_interpreter(interpreter: Interpreter) -> Self {
        Self {
            source: PythonSource::ProvidedPath,
            interpreter,
        }
    }

    /// Return the [`PythonSource`] of the Python installation, indicating where it was found.
    pub fn source(&self) -> &PythonSource {
        &self.source
    }

    pub fn key(&self) -> PythonInstallationKey {
        self.interpreter.key()
    }

    /// Return the Python [`Version`] of the Python installation as reported by its interpreter.
    pub fn python_version(&self) -> &Version {
        self.interpreter.python_version()
    }

    /// Return the [`LenientImplementationName`] of the Python installation as reported by its interpreter.
    pub fn implementation(&self) -> LenientImplementationName {
        LenientImplementationName::from(self.interpreter.implementation_name())
    }

    /// Return the [`Arch`] of the Python installation as reported by its interpreter.
    pub fn arch(&self) -> Arch {
        self.interpreter.arch()
    }

    /// Return the [`Libc`] of the Python installation as reported by its interpreter.
    pub fn libc(&self) -> Libc {
        self.interpreter.libc()
    }

    /// Return the [`Os`] of the Python installation as reported by its interpreter.
    pub fn os(&self) -> Os {
        self.interpreter.os()
    }

    /// Return the [`Interpreter`] for the Python installation.
    pub fn interpreter(&self) -> &Interpreter {
        &self.interpreter
    }

    pub fn into_interpreter(self) -> Interpreter {
        self.interpreter
    }
}

#[derive(Error, Debug)]
pub enum PythonInstallationKeyError {
    #[error("Failed to parse Python installation key `{0}`: {1}")]
    ParseError(String, String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PythonInstallationKey {
    pub(crate) implementation: LenientImplementationName,
    pub(crate) major: u8,
    pub(crate) minor: u8,
    pub(crate) patch: u8,
    pub(crate) prerelease: Cow<'static, str>,
    pub(crate) os: Os,
    pub(crate) arch: Arch,
    pub(crate) libc: Libc,
}

impl PythonInstallationKey {
    pub fn new(
        implementation: LenientImplementationName,
        major: u8,
        minor: u8,
        patch: u8,
        prerelease: String,
        os: Os,
        arch: Arch,
        libc: Libc,
    ) -> Self {
        Self {
            implementation,
            major,
            minor,
            patch,
            prerelease: Cow::Owned(prerelease),
            os,
            arch,
            libc,
        }
    }

    pub fn new_from_version(
        implementation: LenientImplementationName,
        version: &PythonVersion,
        os: Os,
        arch: Arch,
        libc: Libc,
    ) -> Self {
        Self {
            implementation,
            major: version.major(),
            minor: version.minor(),
            patch: version.patch().unwrap_or_default(),
            prerelease: Cow::Owned(version.pre().map(|pre| pre.to_string()).unwrap_or_default()),
            os,
            arch,
            libc,
        }
    }

    pub fn implementation(&self) -> &LenientImplementationName {
        &self.implementation
    }

    pub fn version(&self) -> PythonVersion {
        PythonVersion::from_str(&format!(
            "{}.{}.{}{}",
            self.major, self.minor, self.patch, self.prerelease
        ))
        .expect("Python installation keys must have valid Python versions")
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

impl fmt::Display for PythonInstallationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}.{}.{}{}-{}-{}-{}",
            self.implementation,
            self.major,
            self.minor,
            self.patch,
            self.prerelease,
            self.os,
            self.arch,
            self.libc
        )
    }
}

impl FromStr for PythonInstallationKey {
    type Err = PythonInstallationKeyError;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        let parts = key.split('-').collect::<Vec<_>>();
        let [implementation, version, os, arch, libc] = parts.as_slice() else {
            return Err(PythonInstallationKeyError::ParseError(
                key.to_string(),
                "not enough `-`-separated values".to_string(),
            ));
        };

        let implementation = LenientImplementationName::from(*implementation);

        let os = Os::from_str(os).map_err(|err| {
            PythonInstallationKeyError::ParseError(key.to_string(), format!("invalid OS: {err}"))
        })?;

        let arch = Arch::from_str(arch).map_err(|err| {
            PythonInstallationKeyError::ParseError(
                key.to_string(),
                format!("invalid architecture: {err}"),
            )
        })?;

        let libc = Libc::from_str(libc).map_err(|err| {
            PythonInstallationKeyError::ParseError(key.to_string(), format!("invalid libc: {err}"))
        })?;

        let version = PythonVersion::from_str(version).map_err(|err| {
            PythonInstallationKeyError::ParseError(
                key.to_string(),
                format!("invalid Python version: {err}"),
            )
        })?;

        Ok(Self::new_from_version(
            implementation,
            &version,
            os,
            arch,
            libc,
        ))
    }
}

impl PartialOrd for PythonInstallationKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PythonInstallationKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.implementation
            .cmp(&other.implementation)
            .then_with(|| self.version().cmp(&other.version()))
            .then_with(|| self.os.to_string().cmp(&other.os.to_string()))
            .then_with(|| self.arch.to_string().cmp(&other.arch.to_string()))
            .then_with(|| self.libc.to_string().cmp(&other.libc.to_string()))
    }
}
