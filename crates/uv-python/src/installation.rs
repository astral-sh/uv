use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use indexmap::IndexMap;
use ref_cast::RefCast;
use tracing::{debug, info};

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::PreviewMode;
use uv_pep440::{Prerelease, Version};

use crate::discovery::{
    EnvironmentPreference, PythonRequest, find_best_python_installation, find_python_installation,
};
use crate::downloads::{DownloadResult, ManagedPythonDownload, PythonDownloadRequest, Reporter};
use crate::implementation::LenientImplementationName;
use crate::managed::{ManagedPythonInstallation, ManagedPythonInstallations};
use crate::platform::{Arch, Libc, Os};
use crate::{
    Error, ImplementationName, Interpreter, PythonDownloads, PythonPreference, PythonSource,
    PythonVariant, PythonVersion, downloads,
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
        preview: PreviewMode,
    ) -> Result<Self, Error> {
        let installation =
            find_python_installation(request, environments, preference, cache, preview)??;
        Ok(installation)
    }

    /// Find an installed [`PythonInstallation`] that satisfies a requested version, if the request cannot
    /// be satisfied, fallback to the best available Python installation.
    pub fn find_best(
        request: &PythonRequest,
        environments: EnvironmentPreference,
        preference: PythonPreference,
        cache: &Cache,
        preview: PreviewMode,
    ) -> Result<Self, Error> {
        Ok(find_best_python_installation(
            request,
            environments,
            preference,
            cache,
            preview,
        )??)
    }

    /// Find or fetch a [`PythonInstallation`].
    ///
    /// Unlike [`PythonInstallation::find`], if the required Python is not installed it will be installed automatically.
    pub async fn find_or_download(
        request: Option<&PythonRequest>,
        environments: EnvironmentPreference,
        preference: PythonPreference,
        python_downloads: PythonDownloads,
        client_builder: &BaseClientBuilder<'_>,
        cache: &Cache,
        reporter: Option<&dyn Reporter>,
        python_install_mirror: Option<&str>,
        pypy_install_mirror: Option<&str>,
        python_downloads_json_url: Option<&str>,
        preview: PreviewMode,
    ) -> Result<Self, Error> {
        let request = request.unwrap_or(&PythonRequest::Default);

        // Search for the installation
        let err = match Self::find(request, environments, preference, cache, preview) {
            Ok(installation) => return Ok(installation),
            Err(err) => err,
        };

        match err {
            // If Python is missing, we should attempt a download
            Error::MissingPython(..) => {}
            // If we raised a non-critical error, we should attempt a download
            Error::Discovery(ref err) if !err.is_critical() => {}
            // Otherwise, this is fatal
            _ => return Err(err),
        }

        // If we can't convert the request to a download, throw the original error
        let Some(download_request) = PythonDownloadRequest::from_request(request) else {
            return Err(err);
        };

        let downloads_enabled = preference.allows_managed()
            && python_downloads.is_automatic()
            && client_builder.connectivity.is_online();

        let download = download_request.clone().fill().map(|request| {
            ManagedPythonDownload::from_request(&request, python_downloads_json_url)
        });

        // Regardless of whether downloads are enabled, we want to determine if the download is
        // available to power error messages. However, if downloads aren't enabled, we don't want to
        // report any errors related to them.
        let download = match download {
            Ok(Ok(download)) => Some(download),
            // If the download cannot be found, return the _original_ discovery error
            Ok(Err(downloads::Error::NoDownloadFound(_))) => {
                if downloads_enabled {
                    debug!("No downloads are available for {request}");
                    return Err(err);
                }
                None
            }
            Err(err) | Ok(Err(err)) => {
                if downloads_enabled {
                    // We failed to determine the platform information
                    return Err(err.into());
                }
                None
            }
        };

        let Some(download) = download else {
            // N.B. We should only be in this case when downloads are disabled; when downloads are
            // enabled, we should fail eagerly when something goes wrong with the download.
            debug_assert!(!downloads_enabled);
            return Err(err);
        };

        // If the download is available, but not usable, we attach a hint to the original error.
        if !downloads_enabled {
            let for_request = match request {
                PythonRequest::Default | PythonRequest::Any => String::new(),
                _ => format!(" for {request}"),
            };

            match python_downloads {
                PythonDownloads::Automatic => {}
                PythonDownloads::Manual => {
                    return Err(err.with_missing_python_hint(format!(
                        "A managed Python download is available{for_request}, but Python downloads are set to 'manual', use `uv python install {}` to install the required version",
                        request.to_canonical_string(),
                    )));
                }
                PythonDownloads::Never => {
                    return Err(err.with_missing_python_hint(format!(
                        "A managed Python download is available{for_request}, but Python downloads are set to 'never'"
                    )));
                }
            }

            match preference {
                PythonPreference::OnlySystem => {
                    return Err(err.with_missing_python_hint(format!(
                        "A managed Python download is available{for_request}, but the Python preference is set to 'only system'"
                    )));
                }
                PythonPreference::Managed
                | PythonPreference::OnlyManaged
                | PythonPreference::System => {}
            }

            if !client_builder.connectivity.is_online() {
                return Err(err.with_missing_python_hint(format!(
                    "A managed Python download is available{for_request}, but uv is set to offline mode"
                )));
            }

            return Err(err);
        }

        Self::fetch(
            download,
            client_builder,
            cache,
            reporter,
            python_install_mirror,
            pypy_install_mirror,
            preview,
        )
        .await
    }

    /// Download and install the requested installation.
    pub async fn fetch(
        download: &'static ManagedPythonDownload,
        client_builder: &BaseClientBuilder<'_>,
        cache: &Cache,
        reporter: Option<&dyn Reporter>,
        python_install_mirror: Option<&str>,
        pypy_install_mirror: Option<&str>,
        preview: PreviewMode,
    ) -> Result<Self, Error> {
        let installations = ManagedPythonInstallations::from_settings(None)?.init()?;
        let installations_dir = installations.root();
        let scratch_dir = installations.scratch();
        let _lock = installations.lock().await?;

        let client = client_builder.build();

        info!("Fetching requested Python...");
        let result = download
            .fetch_with_retry(
                &client,
                installations_dir,
                &scratch_dir,
                false,
                python_install_mirror,
                pypy_install_mirror,
                reporter,
            )
            .await?;

        let path = match result {
            DownloadResult::AlreadyAvailable(path) => path,
            DownloadResult::Fetched(path) => path,
        };

        let installed = ManagedPythonInstallation::new(path, download);
        installed.ensure_externally_managed()?;
        installed.ensure_sysconfig_patched()?;
        installed.ensure_canonical_executables()?;

        let minor_version = installed.minor_version_key();
        let highest_patch = installations
            .find_all()?
            .filter(|installation| installation.minor_version_key() == minor_version)
            .filter_map(|installation| installation.version().patch())
            .fold(0, std::cmp::max);
        if installed
            .version()
            .patch()
            .is_some_and(|p| p >= highest_patch)
        {
            installed.ensure_minor_version_link(preview)?;
        }

        if let Err(e) = installed.ensure_dylib_patched() {
            e.warn_user(&installed);
        }

        Ok(Self {
            source: PythonSource::Managed,
            interpreter: Interpreter::query(installed.executable(false), cache)?,
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

    /// Whether this is a CPython installation.
    ///
    /// Returns false if it is an alternative implementation, e.g., PyPy.
    pub(crate) fn is_alternative_implementation(&self) -> bool {
        !matches!(
            self.implementation(),
            LenientImplementationName::Known(ImplementationName::CPython)
        )
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

    /// Consume the [`PythonInstallation`] and return the [`Interpreter`].
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
    pub(crate) prerelease: Option<Prerelease>,
    pub(crate) os: Os,
    pub(crate) arch: Arch,
    pub(crate) libc: Libc,
    pub(crate) variant: PythonVariant,
}

impl PythonInstallationKey {
    pub fn new(
        implementation: LenientImplementationName,
        major: u8,
        minor: u8,
        patch: u8,
        prerelease: Option<Prerelease>,
        os: Os,
        arch: Arch,
        libc: Libc,
        variant: PythonVariant,
    ) -> Self {
        Self {
            implementation,
            major,
            minor,
            patch,
            prerelease,
            os,
            arch,
            libc,
            variant,
        }
    }

    pub fn new_from_version(
        implementation: LenientImplementationName,
        version: &PythonVersion,
        os: Os,
        arch: Arch,
        libc: Libc,
        variant: PythonVariant,
    ) -> Self {
        Self {
            implementation,
            major: version.major(),
            minor: version.minor(),
            patch: version.patch().unwrap_or_default(),
            prerelease: version.pre(),
            os,
            arch,
            libc,
            variant,
        }
    }

    pub fn implementation(&self) -> &LenientImplementationName {
        &self.implementation
    }

    pub fn version(&self) -> PythonVersion {
        PythonVersion::from_str(&format!(
            "{}.{}.{}{}",
            self.major,
            self.minor,
            self.patch,
            self.prerelease
                .map(|pre| pre.to_string())
                .unwrap_or_default()
        ))
        .expect("Python installation keys must have valid Python versions")
    }

    /// The version in `x.y.z` format.
    pub fn sys_version(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    pub fn major(&self) -> u8 {
        self.major
    }

    pub fn minor(&self) -> u8 {
        self.minor
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

    pub fn variant(&self) -> &PythonVariant {
        &self.variant
    }

    /// Return a canonical name for a minor versioned executable.
    pub fn executable_name_minor(&self) -> String {
        format!(
            "python{maj}.{min}{var}{exe}",
            maj = self.major,
            min = self.minor,
            var = self.variant.suffix(),
            exe = std::env::consts::EXE_SUFFIX
        )
    }

    /// Return a canonical name for a major versioned executable.
    pub fn executable_name_major(&self) -> String {
        format!(
            "python{maj}{var}{exe}",
            maj = self.major,
            var = self.variant.suffix(),
            exe = std::env::consts::EXE_SUFFIX
        )
    }

    /// Return a canonical name for an un-versioned executable.
    pub fn executable_name(&self) -> String {
        format!(
            "python{var}{exe}",
            var = self.variant.suffix(),
            exe = std::env::consts::EXE_SUFFIX
        )
    }
}

impl fmt::Display for PythonInstallationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let variant = match self.variant {
            PythonVariant::Default => String::new(),
            PythonVariant::Freethreaded => format!("+{}", self.variant),
        };
        write!(
            f,
            "{}-{}.{}.{}{}{}-{}-{}-{}",
            self.implementation,
            self.major,
            self.minor,
            self.patch,
            self.prerelease
                .map(|pre| pre.to_string())
                .unwrap_or_default(),
            variant,
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

        let (version, variant) = match version.split_once('+') {
            Some((version, variant)) => {
                let variant = PythonVariant::from_str(variant).map_err(|()| {
                    PythonInstallationKeyError::ParseError(
                        key.to_string(),
                        format!("invalid Python variant: {variant}"),
                    )
                })?;
                (version, variant)
            }
            None => (*version, PythonVariant::Default),
        };

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
            variant,
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
            // Architectures are sorted in preferred order, with native architectures first
            .then_with(|| self.arch.cmp(&other.arch).reverse())
            .then_with(|| self.libc.to_string().cmp(&other.libc.to_string()))
            // Python variants are sorted in preferred order, with `Default` first
            .then_with(|| self.variant.cmp(&other.variant).reverse())
    }
}

/// A view into a [`PythonInstallationKey`] that excludes the patch and prerelease versions.
#[derive(Clone, Eq, Ord, PartialOrd, RefCast)]
#[repr(transparent)]
pub struct PythonInstallationMinorVersionKey(PythonInstallationKey);

impl PythonInstallationMinorVersionKey {
    /// Cast a `&PythonInstallationKey` to a `&PythonInstallationMinorVersionKey` using ref-cast.
    #[inline]
    pub fn ref_cast(key: &PythonInstallationKey) -> &Self {
        RefCast::ref_cast(key)
    }

    /// Takes an [`IntoIterator`] of [`ManagedPythonInstallation`]s and returns an [`FxHashMap`] from
    /// [`PythonInstallationMinorVersionKey`] to the installation with highest [`PythonInstallationKey`]
    /// for that minor version key.
    #[inline]
    pub fn highest_installations_by_minor_version_key<'a, I>(
        installations: I,
    ) -> IndexMap<Self, ManagedPythonInstallation>
    where
        I: IntoIterator<Item = &'a ManagedPythonInstallation>,
    {
        let mut minor_versions = IndexMap::default();
        for installation in installations {
            minor_versions
                .entry(installation.minor_version_key().clone())
                .and_modify(|high_installation: &mut ManagedPythonInstallation| {
                    if installation.key() >= high_installation.key() {
                        *high_installation = installation.clone();
                    }
                })
                .or_insert_with(|| installation.clone());
        }
        minor_versions
    }
}

impl fmt::Display for PythonInstallationMinorVersionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display every field on the wrapped key except the patch
        // and prerelease (with special formatting for the variant).
        let variant = match self.0.variant {
            PythonVariant::Default => String::new(),
            PythonVariant::Freethreaded => format!("+{}", self.0.variant),
        };
        write!(
            f,
            "{}-{}.{}{}-{}-{}-{}",
            self.0.implementation,
            self.0.major,
            self.0.minor,
            variant,
            self.0.os,
            self.0.arch,
            self.0.libc,
        )
    }
}

impl fmt::Debug for PythonInstallationMinorVersionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display every field on the wrapped key except the patch
        // and prerelease.
        f.debug_struct("PythonInstallationMinorVersionKey")
            .field("implementation", &self.0.implementation)
            .field("major", &self.0.major)
            .field("minor", &self.0.minor)
            .field("variant", &self.0.variant)
            .field("os", &self.0.os)
            .field("arch", &self.0.arch)
            .field("libc", &self.0.libc)
            .finish()
    }
}

impl PartialEq for PythonInstallationMinorVersionKey {
    fn eq(&self, other: &Self) -> bool {
        // Compare every field on the wrapped key except the patch
        // and prerelease.
        self.0.implementation == other.0.implementation
            && self.0.major == other.0.major
            && self.0.minor == other.0.minor
            && self.0.os == other.0.os
            && self.0.arch == other.0.arch
            && self.0.libc == other.0.libc
            && self.0.variant == other.0.variant
    }
}

impl Hash for PythonInstallationMinorVersionKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash every field on the wrapped key except the patch
        // and prerelease.
        self.0.implementation.hash(state);
        self.0.major.hash(state);
        self.0.minor.hash(state);
        self.0.os.hash(state);
        self.0.arch.hash(state);
        self.0.libc.hash(state);
        self.0.variant.hash(state);
    }
}

impl From<PythonInstallationKey> for PythonInstallationMinorVersionKey {
    fn from(key: PythonInstallationKey) -> Self {
        PythonInstallationMinorVersionKey(key)
    }
}
