use std::borrow::Cow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use indexmap::IndexMap;
use ref_cast::RefCast;
use reqwest_retry::policies::ExponentialBackoff;
use tracing::{debug, info};
use uv_warnings::warn_user;

use uv_cache::Cache;
use uv_client::{BaseClient, BaseClientBuilder};
use uv_pep440::{Prerelease, Version};
use uv_platform::{Arch, Libc, Os, Platform};
use uv_preview::Preview;

use crate::discovery::{
    EnvironmentPreference, PythonRequest, find_best_python_installation, find_python_installation,
};
use crate::downloads::{
    DownloadResult, ManagedPythonDownload, ManagedPythonDownloadList, PythonDownloadRequest,
    Reporter,
};
use crate::implementation::LenientImplementationName;
use crate::managed::{ManagedPythonInstallation, ManagedPythonInstallations};
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
        download_list: &ManagedPythonDownloadList,
        cache: &Cache,
        preview: Preview,
    ) -> Result<Self, Error> {
        let installation =
            find_python_installation(request, environments, preference, cache, preview)??;
        installation.warn_if_outdated_prerelease(request, download_list);
        Ok(installation)
    }

    /// Find an installed [`PythonInstallation`] that satisfies a requested version, if the request cannot
    /// be satisfied, fallback to the best available Python installation.
    pub fn find_best(
        request: &PythonRequest,
        environments: EnvironmentPreference,
        preference: PythonPreference,
        download_list: &ManagedPythonDownloadList,
        cache: &Cache,
        preview: Preview,
    ) -> Result<Self, Error> {
        let installation =
            find_best_python_installation(request, environments, preference, cache, preview)??;
        installation.warn_if_outdated_prerelease(request, download_list);
        Ok(installation)
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
        preview: Preview,
    ) -> Result<Self, Error> {
        let request = request.unwrap_or(&PythonRequest::Default);

        // Python downloads are performing their own retries to catch stream errors, disable the
        // default retries to avoid the middleware performing uncontrolled retries.
        let retry_policy = client_builder.retry_policy();
        let client = client_builder.clone().retries(0).build();
        let download_list =
            ManagedPythonDownloadList::new(&client, python_downloads_json_url).await?;

        // Search for the installation
        let err = match Self::find(
            request,
            environments,
            preference,
            &download_list,
            cache,
            preview,
        ) {
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

        let download = download_request
            .clone()
            .fill()
            .map(|request| download_list.find(&request));

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

        let installation = Self::fetch(
            download,
            &client,
            &retry_policy,
            cache,
            reporter,
            python_install_mirror,
            pypy_install_mirror,
            preview,
        )
        .await?;

        installation.warn_if_outdated_prerelease(request, &download_list);

        Ok(installation)
    }

    /// Download and install the requested installation.
    pub async fn fetch(
        download: &ManagedPythonDownload,
        client: &BaseClient,
        retry_policy: &ExponentialBackoff,
        cache: &Cache,
        reporter: Option<&dyn Reporter>,
        python_install_mirror: Option<&str>,
        pypy_install_mirror: Option<&str>,
        preview: Preview,
    ) -> Result<Self, Error> {
        let installations = ManagedPythonInstallations::from_settings(None)?.init()?;
        let installations_dir = installations.root();
        let scratch_dir = installations.scratch();
        let _lock = installations.lock().await?;

        info!("Fetching requested Python...");
        let result = download
            .fetch_with_retry(
                client,
                retry_policy,
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
        installed.ensure_build_file()?;

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
        ) || self.os().is_emscripten()
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

    /// Emit a warning when the interpreter is a managed prerelease and a matching stable
    /// build can be installed via `uv python upgrade`.
    pub(crate) fn warn_if_outdated_prerelease(
        &self,
        request: &PythonRequest,
        download_list: &ManagedPythonDownloadList,
    ) {
        if request.allows_prereleases() {
            return;
        }

        let interpreter = self.interpreter();
        let version = interpreter.python_version();

        if version.pre().is_none() {
            return;
        }

        if !interpreter.is_managed() {
            return;
        }

        // Transparent upgrades only exist for CPython, so skip the warning for other
        // managed implementations.
        //
        // See: https://github.com/astral-sh/uv/issues/16675
        if !interpreter
            .implementation_name()
            .eq_ignore_ascii_case("cpython")
        {
            return;
        }

        let release = version.only_release();

        let Ok(download_request) = PythonDownloadRequest::try_from(&interpreter.key()) else {
            return;
        };

        let download_request = download_request.with_prereleases(false);

        let has_stable_download = {
            let mut downloads = download_list.iter_matching(&download_request);

            downloads.any(|download| {
                let download_version = download.key().version().into_version();
                download_version.pre().is_none() && download_version.only_release() >= release
            })
        };

        if !has_stable_download {
            return;
        }

        if let Some(upgrade_request) = download_request
            .unset_defaults()
            .without_patch()
            .simplified_display()
        {
            warn_user!(
                "You're using a pre-release version of Python ({}) but a stable version is available. Use `uv python upgrade {}` to upgrade.",
                version,
                upgrade_request
            );
        } else {
            warn_user!(
                "You're using a pre-release version of Python ({}) but a stable version is available. Run `uv python upgrade` to update your managed interpreters.",
                version,
            );
        }
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
    pub(crate) platform: Platform,
    pub(crate) variant: PythonVariant,
}

impl PythonInstallationKey {
    pub fn new(
        implementation: LenientImplementationName,
        major: u8,
        minor: u8,
        patch: u8,
        prerelease: Option<Prerelease>,
        platform: Platform,
        variant: PythonVariant,
    ) -> Self {
        Self {
            implementation,
            major,
            minor,
            patch,
            prerelease,
            platform,
            variant,
        }
    }

    pub fn new_from_version(
        implementation: LenientImplementationName,
        version: &PythonVersion,
        platform: Platform,
        variant: PythonVariant,
    ) -> Self {
        Self {
            implementation,
            major: version.major(),
            minor: version.minor(),
            patch: version.patch().unwrap_or_default(),
            prerelease: version.pre(),
            platform,
            variant,
        }
    }

    pub fn implementation(&self) -> Cow<'_, LenientImplementationName> {
        if self.os().is_emscripten() {
            Cow::Owned(LenientImplementationName::from(ImplementationName::Pyodide))
        } else {
            Cow::Borrowed(&self.implementation)
        }
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

    pub fn prerelease(&self) -> Option<Prerelease> {
        self.prerelease
    }

    pub fn platform(&self) -> &Platform {
        &self.platform
    }

    pub fn arch(&self) -> &Arch {
        &self.platform.arch
    }

    pub fn os(&self) -> &Os {
        &self.platform.os
    }

    pub fn libc(&self) -> &Libc {
        &self.platform.libc
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
            var = self.variant.executable_suffix(),
            exe = std::env::consts::EXE_SUFFIX
        )
    }

    /// Return a canonical name for a major versioned executable.
    pub fn executable_name_major(&self) -> String {
        format!(
            "python{maj}{var}{exe}",
            maj = self.major,
            var = self.variant.executable_suffix(),
            exe = std::env::consts::EXE_SUFFIX
        )
    }

    /// Return a canonical name for an un-versioned executable.
    pub fn executable_name(&self) -> String {
        format!(
            "python{var}{exe}",
            var = self.variant.executable_suffix(),
            exe = std::env::consts::EXE_SUFFIX
        )
    }
}

impl fmt::Display for PythonInstallationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let variant = match self.variant {
            PythonVariant::Default => String::new(),
            _ => format!("+{}", self.variant),
        };
        write!(
            f,
            "{}-{}.{}.{}{}{}-{}",
            self.implementation(),
            self.major,
            self.minor,
            self.patch,
            self.prerelease
                .map(|pre| pre.to_string())
                .unwrap_or_default(),
            variant,
            self.platform
        )
    }
}

impl FromStr for PythonInstallationKey {
    type Err = PythonInstallationKeyError;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        let parts = key.split('-').collect::<Vec<_>>();

        // We need exactly implementation-version-os-arch-libc
        if parts.len() != 5 {
            return Err(PythonInstallationKeyError::ParseError(
                key.to_string(),
                format!(
                    "expected exactly 5 `-`-separated values, got {}",
                    parts.len()
                ),
            ));
        }

        let [implementation_str, version_str, os, arch, libc] = parts.as_slice() else {
            unreachable!()
        };

        let implementation = LenientImplementationName::from(*implementation_str);

        let (version, variant) = match version_str.split_once('+') {
            Some((version, variant)) => {
                let variant = PythonVariant::from_str(variant).map_err(|()| {
                    PythonInstallationKeyError::ParseError(
                        key.to_string(),
                        format!("invalid Python variant: {variant}"),
                    )
                })?;
                (version, variant)
            }
            None => (*version_str, PythonVariant::Default),
        };

        let version = PythonVersion::from_str(version).map_err(|err| {
            PythonInstallationKeyError::ParseError(
                key.to_string(),
                format!("invalid Python version: {err}"),
            )
        })?;

        let platform = Platform::from_parts(os, arch, libc).map_err(|err| {
            PythonInstallationKeyError::ParseError(
                key.to_string(),
                format!("invalid platform: {err}"),
            )
        })?;

        Ok(Self {
            implementation,
            major: version.major(),
            minor: version.minor(),
            patch: version.patch().unwrap_or_default(),
            prerelease: version.pre(),
            platform,
            variant,
        })
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
            // Platforms are sorted in preferred order for the target
            .then_with(|| self.platform.cmp(&other.platform).reverse())
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
            _ => format!("+{}", self.0.variant),
        };
        write!(
            f,
            "{}-{}.{}{}-{}",
            self.0.implementation, self.0.major, self.0.minor, variant, self.0.platform,
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
            .field("os", &self.0.platform.os)
            .field("arch", &self.0.platform.arch)
            .field("libc", &self.0.platform.libc)
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
            && self.0.platform == other.0.platform
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
        self.0.platform.hash(state);
        self.0.variant.hash(state);
    }
}

impl From<PythonInstallationKey> for PythonInstallationMinorVersionKey {
    fn from(key: PythonInstallationKey) -> Self {
        Self(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uv_platform::ArchVariant;

    #[test]
    fn test_python_installation_key_from_str() {
        // Test basic parsing
        let key = PythonInstallationKey::from_str("cpython-3.12.0-linux-x86_64-gnu").unwrap();
        assert_eq!(
            key.implementation,
            LenientImplementationName::Known(ImplementationName::CPython)
        );
        assert_eq!(key.major, 3);
        assert_eq!(key.minor, 12);
        assert_eq!(key.patch, 0);
        assert_eq!(
            key.platform.os,
            Os::new(target_lexicon::OperatingSystem::Linux)
        );
        assert_eq!(
            key.platform.arch,
            Arch::new(target_lexicon::Architecture::X86_64, None)
        );
        assert_eq!(
            key.platform.libc,
            Libc::Some(target_lexicon::Environment::Gnu)
        );

        // Test with architecture variant
        let key = PythonInstallationKey::from_str("cpython-3.11.2-linux-x86_64_v3-musl").unwrap();
        assert_eq!(
            key.implementation,
            LenientImplementationName::Known(ImplementationName::CPython)
        );
        assert_eq!(key.major, 3);
        assert_eq!(key.minor, 11);
        assert_eq!(key.patch, 2);
        assert_eq!(
            key.platform.os,
            Os::new(target_lexicon::OperatingSystem::Linux)
        );
        assert_eq!(
            key.platform.arch,
            Arch::new(target_lexicon::Architecture::X86_64, Some(ArchVariant::V3))
        );
        assert_eq!(
            key.platform.libc,
            Libc::Some(target_lexicon::Environment::Musl)
        );

        // Test with Python variant (freethreaded)
        let key = PythonInstallationKey::from_str("cpython-3.13.0+freethreaded-macos-aarch64-none")
            .unwrap();
        assert_eq!(
            key.implementation,
            LenientImplementationName::Known(ImplementationName::CPython)
        );
        assert_eq!(key.major, 3);
        assert_eq!(key.minor, 13);
        assert_eq!(key.patch, 0);
        assert_eq!(key.variant, PythonVariant::Freethreaded);
        assert_eq!(
            key.platform.os,
            Os::new(target_lexicon::OperatingSystem::Darwin(None))
        );
        assert_eq!(
            key.platform.arch,
            Arch::new(
                target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                None
            )
        );
        assert_eq!(key.platform.libc, Libc::None);

        // Test error cases
        assert!(PythonInstallationKey::from_str("cpython-3.12.0-linux-x86_64").is_err());
        assert!(PythonInstallationKey::from_str("cpython-3.12.0").is_err());
        assert!(PythonInstallationKey::from_str("cpython").is_err());
    }

    #[test]
    fn test_python_installation_key_display() {
        let key = PythonInstallationKey {
            implementation: LenientImplementationName::from("cpython"),
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            platform: Platform::from_str("linux-x86_64-gnu").unwrap(),
            variant: PythonVariant::Default,
        };
        assert_eq!(key.to_string(), "cpython-3.12.0-linux-x86_64-gnu");

        let key_with_variant = PythonInstallationKey {
            implementation: LenientImplementationName::from("cpython"),
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            platform: Platform::from_str("macos-aarch64-none").unwrap(),
            variant: PythonVariant::Freethreaded,
        };
        assert_eq!(
            key_with_variant.to_string(),
            "cpython-3.13.0+freethreaded-macos-aarch64-none"
        );
    }
}
