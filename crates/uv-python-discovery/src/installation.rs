use reqwest_retry::policies::ExponentialBackoff;
use tracing::{debug, info};
use uv_fs::Simplified;
use uv_warnings::warn_user;

use uv_cache::Cache;
use uv_client::{BaseClient, BaseClientBuilder};
use uv_pep440::Version;
use uv_platform::{Arch, Libc, Os};

use crate::Error;
use crate::MissingPythonHint;
use crate::discovery::find_best_python_installation;
use crate::discovery::find_python_installation;
use uv_python_interpreter::{EnvironmentNotFound, Interpreter, PythonEnvironment};
use uv_python_managed::downloads::{
    DownloadResult, ManagedPythonDownload, ManagedPythonDownloadList, Reporter,
};
use uv_python_managed::{ManagedPythonInstallation, ManagedPythonInstallations};
use uv_python_types::PythonInstallationKey;
use uv_python_types::{
    EnvironmentPreference, ImplementationName, LenientImplementationName, PythonArchitecture,
    PythonDownloadMirrors, PythonDownloadRequest, PythonDownloads, PythonPreference, PythonRequest,
    PythonSource, PythonVersion, VersionRequest,
};

/// A Python interpreter and accompanying tools.
#[derive(Clone, Debug)]
pub struct PythonInstallation {
    // Public in the crate for test assertions
    pub(crate) source: PythonSource,
    pub(crate) interpreter: Interpreter,
}

impl PythonInstallation {
    /// Create a new [`PythonInstallation`] from a source and interpreter.
    pub(crate) fn new(source: PythonSource, interpreter: Interpreter) -> Self {
        Self {
            source,
            interpreter,
        }
    }

    /// Return a new installation with the given [`PythonSource`].
    #[must_use]
    fn with_source(self, source: PythonSource) -> Self {
        Self { source, ..self }
    }

    /// In test mode, change the source to [`PythonSource::Managed`] if the interpreter was
    /// marked as managed via `TestContext::with_versions_as_managed`.
    #[must_use]
    pub(crate) fn maybe_with_test_source(self) -> Self {
        if std::env::var(uv_static::EnvVars::UV_INTERNAL__TEST_PYTHON_MANAGED).is_ok()
            && self.is_managed()
        {
            self.with_source(PythonSource::Managed)
        } else {
            self
        }
    }

    /// Check whether this installation satisfies the standard post-query discovery filters:
    /// environment preference, version request, and Python preference.
    pub(crate) fn satisfies_preferences(
        &self,
        version: &VersionRequest,
        environments: EnvironmentPreference,
        preference: PythonPreference,
    ) -> bool {
        if !self.satisfies_environment_preference(environments) {
            return false;
        }
        if !self.matches_version_request(version) {
            debug!(
                "Skipping interpreter at `{}` from {}: does not satisfy request `{version}`",
                self.interpreter.sys_executable().user_display(),
                self.source,
            );
            return false;
        }
        if !self.satisfies_preference(&preference) {
            return false;
        }
        true
    }

    /// Find an installed [`PythonInstallation`].
    ///
    /// This is the standard interface for discovering a Python installation for creating
    /// an environment. If interested in finding an existing environment, see
    /// [`find_environment`] instead.
    ///
    /// Note we still require an [`EnvironmentPreference`] as this can either bypass virtual environments
    /// or prefer them. In most cases, this should be [`EnvironmentPreference::OnlySystem`]
    /// but if you want to allow an interpreter from a virtual environment if it satisfies the request,
    /// then use [`EnvironmentPreference::Any`].
    ///
    /// See `find_python_installation` for implementation details.
    pub fn find(
        request: &PythonRequest,
        environments: EnvironmentPreference,
        preference: PythonPreference,
        arch: Option<PythonArchitecture>,
        download_list: &ManagedPythonDownloadList,
        cache: &Cache,
    ) -> Result<Self, Error> {
        let installation = Self::find_existing(request, environments, preference, arch, cache)?;
        installation.warn_if_outdated_prerelease(request, download_list);
        Ok(installation)
    }

    /// Find an existing [`PythonInstallation`].
    pub fn find_existing(
        request: &PythonRequest,
        environments: EnvironmentPreference,
        preference: PythonPreference,
        arch: Option<PythonArchitecture>,
        cache: &Cache,
    ) -> Result<Self, Error> {
        Ok(find_python_installation(
            request,
            environments,
            preference,
            arch,
            cache,
        )??)
    }

    /// Find or download a [`PythonInstallation`] that satisfies a requested version, if the request
    /// cannot be satisfied, fallback to the best available Python installation.
    pub async fn find_best(
        request: &PythonRequest,
        environments: EnvironmentPreference,
        preference: PythonPreference,
        arch: Option<PythonArchitecture>,
        python_downloads: PythonDownloads,
        client_builder: &BaseClientBuilder<'_>,
        cache: &Cache,
        reporter: Option<&dyn Reporter>,
        mirrors: PythonDownloadMirrors<'_>,
        python_downloads_json_url: Option<&str>,
    ) -> Result<Self, Error> {
        let downloads_enabled = preference.allows_managed()
            && python_downloads.is_automatic()
            && client_builder.connectivity.is_online();
        let installation = find_best_python_installation(
            request,
            environments,
            preference,
            arch,
            downloads_enabled,
            client_builder,
            cache,
            reporter,
            mirrors,
            python_downloads_json_url,
        )
        .await?;
        installation
            .download_and_warn_if_outdated_prerelease(
                request,
                client_builder,
                cache,
                python_downloads_json_url,
            )
            .await?;
        Ok(installation)
    }

    /// Find or fetch a [`PythonInstallation`].
    ///
    /// Unlike [`PythonInstallation::find`], if the required Python is not installed it will be installed automatically.
    pub async fn find_or_download(
        request: Option<&PythonRequest>,
        environments: EnvironmentPreference,
        preference: PythonPreference,
        arch: Option<PythonArchitecture>,
        python_downloads: PythonDownloads,
        client_builder: &BaseClientBuilder<'_>,
        cache: &Cache,
        reporter: Option<&dyn Reporter>,
        mirrors: PythonDownloadMirrors<'_>,
        python_downloads_json_url: Option<&str>,
    ) -> Result<Self, Error> {
        let request = request.unwrap_or(&PythonRequest::Default);

        let err = match Self::find_existing(request, environments, preference, arch, cache) {
            Ok(installation) => {
                installation
                    .download_and_warn_if_outdated_prerelease(
                        request,
                        client_builder,
                        cache,
                        python_downloads_json_url,
                    )
                    .await?;
                return Ok(installation);
            }
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

        let download_list =
            ManagedPythonDownloadList::new(client_builder, cache, python_downloads_json_url)
                .await?;

        let downloads_enabled = preference.allows_managed()
            && python_downloads.is_automatic()
            && client_builder.connectivity.is_online();

        let download = download_request
            .clone()
            .with_default_arch(arch.map(PythonArchitecture::into_inner))
            .fill()
            .map_err(uv_python_managed::downloads::Error::from)
            .map(|request| download_list.find(&request));

        // Regardless of whether downloads are enabled, we want to determine if the download is
        // available to power error messages. However, if downloads aren't enabled, we don't want to
        // report any errors related to them.
        let download = match download {
            Ok(Ok(download)) => Some(download),
            // If the download cannot be found, return the _original_ discovery error
            Ok(Err(uv_python_managed::downloads::Error::NoDownloadFound(_))) => {
                if downloads_enabled {
                    debug!("No downloads are available for {request}");
                    if matches!(request, PythonRequest::Default | PythonRequest::Any) {
                        return Err(err);
                    }
                    return Err(err.with_hint(MissingPythonHint::RequiresUpdate));
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
            match python_downloads {
                PythonDownloads::Automatic => {}
                PythonDownloads::Manual => {
                    return Err(err.with_hint(MissingPythonHint::DownloadsManual(request.clone())));
                }
                PythonDownloads::Never => {
                    return Err(err.with_hint(MissingPythonHint::DownloadsNever(request.clone())));
                }
            }

            match preference {
                PythonPreference::OnlySystem => {
                    return Err(
                        err.with_hint(MissingPythonHint::PreferenceOnlySystem(request.clone()))
                    );
                }
                PythonPreference::Managed
                | PythonPreference::OnlyManaged
                | PythonPreference::System => {}
            }

            if !client_builder.connectivity.is_online() {
                return Err(err.with_hint(MissingPythonHint::Offline(request.clone())));
            }

            return Err(err);
        }

        // Python downloads are performing their own retries to catch stream errors, disable the
        // default retries to avoid the middleware performing uncontrolled retries.
        let retry_policy = client_builder.retry_policy();
        let download_client = client_builder.clone().retries(0).build()?;

        let installation = Self::fetch(
            download,
            &download_client,
            &retry_policy,
            cache,
            reporter,
            mirrors,
        )
        .await?;

        installation.warn_if_outdated_prerelease(request, &download_list);

        Ok(installation)
    }

    /// Download and install the requested installation.
    pub(crate) async fn fetch(
        download: &ManagedPythonDownload,
        client: &BaseClient,
        retry_policy: &ExponentialBackoff,
        cache: &Cache,
        reporter: Option<&dyn Reporter>,
        mirrors: PythonDownloadMirrors<'_>,
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
                mirrors,
                reporter,
            )
            .await?;

        let path = match result {
            DownloadResult::AlreadyAvailable(path) => path,
            DownloadResult::Fetched(path) => path,
        };

        let installed = ManagedPythonInstallation::new(path, download)?;
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
            installed.ensure_minor_version_link()?;
        }

        if let Err(e) = installed.ensure_dylib_patched() {
            e.warn_user(&installed);
        }

        Ok(Self {
            source: PythonSource::Managed,
            interpreter: Interpreter::query(installed.executable(false), cache)?,
        })
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

    /// Returns `true` if this is a managed (uv-installed) Python installation.
    ///
    /// Uses the source as a fast path, then falls back to checking the interpreter's base prefix.
    pub(crate) fn is_managed(&self) -> bool {
        if self.source.is_managed() {
            return true;
        }

        if let Ok(test_managed) =
            std::env::var(uv_static::EnvVars::UV_INTERNAL__TEST_PYTHON_MANAGED)
        {
            // During testing, we collect interpreters into an artificial search path and need to
            // be able to mock whether an interpreter is managed or not.
            return test_managed.split_ascii_whitespace().any(|item| {
                let version = <PythonVersion as std::str::FromStr>::from_str(item).expect(
                    "`UV_INTERNAL__TEST_PYTHON_MANAGED` items should be valid Python versions",
                );
                if version.patch().is_some() {
                    version.version() == self.interpreter.python_version()
                } else {
                    (version.major(), version.minor()) == self.interpreter.python_tuple()
                }
            });
        }

        ManagedPythonInstallations::from_settings(None)
            .is_ok_and(|installations| installations.contains(&self.interpreter))
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

    /// Return `true` when checking for an outdated managed prerelease warning may be necessary.
    fn should_check_outdated_prerelease_warning(&self, request: &PythonRequest) -> bool {
        if request.allows_prereleases() {
            return false;
        }

        let interpreter = self.interpreter();

        if interpreter.python_version().pre().is_none() {
            return false;
        }

        if !self.is_managed() {
            return false;
        }

        // Transparent upgrades only exist for CPython, so skip the warning for other
        // managed implementations.
        //
        // See: https://github.com/astral-sh/uv/issues/16675
        if !interpreter
            .implementation_name()
            .eq_ignore_ascii_case("cpython")
        {
            return false;
        }

        true
    }

    /// Emit a warning when the interpreter is a managed prerelease and a matching stable
    /// build can be installed via `uv python upgrade`.
    fn warn_if_outdated_prerelease(
        &self,
        request: &PythonRequest,
        download_list: &ManagedPythonDownloadList,
    ) {
        if !self.should_check_outdated_prerelease_warning(request) {
            return;
        }

        let interpreter = self.interpreter();
        let version = interpreter.python_version();

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

    /// Emit a warning when the interpreter is a managed prerelease and a matching stable
    /// build can be installed via `uv python upgrade`.
    ///
    /// Avoids loading the Python download list unless the discovered interpreter could require
    /// the warning.
    pub async fn download_and_warn_if_outdated_prerelease(
        &self,
        request: &PythonRequest,
        client_builder: &BaseClientBuilder<'_>,
        cache: &Cache,
        python_downloads_json_url: Option<&str>,
    ) -> Result<(), Error> {
        if !self.should_check_outdated_prerelease_warning(request) {
            return Ok(());
        }

        let download_list =
            ManagedPythonDownloadList::new(client_builder, cache, python_downloads_json_url)
                .await?;
        self.warn_if_outdated_prerelease(request, &download_list);

        Ok(())
    }

    /// Check whether this installation satisfies the Python preference.
    ///
    /// Explicit sources, including provided paths and active environments, are accepted even
    /// when their managed status conflicts with the preference.
    pub fn satisfies_preference(&self, preference: &PythonPreference) -> bool {
        let source = self.source;
        let interpreter = &self.interpreter;

        match preference {
            PythonPreference::OnlyManaged => {
                if self.is_managed() {
                    true
                } else if source.is_explicit() {
                    debug!(
                        "Allowing unmanaged Python interpreter at `{}` (in conflict with the `python-preference`) since it is from source: {source}",
                        interpreter.sys_executable().display()
                    );
                    true
                } else {
                    debug!(
                        "Ignoring Python interpreter at `{}`: only managed interpreters allowed",
                        interpreter.sys_executable().display()
                    );
                    false
                }
            }
            // If not "only" a kind, any interpreter is okay
            PythonPreference::Managed | PythonPreference::System => true,
            PythonPreference::OnlySystem => {
                if !self.is_managed() {
                    true
                } else if source.is_explicit() {
                    debug!(
                        "Allowing managed Python interpreter at `{}` (in conflict with the `python-preference`) since it is from source: {source}",
                        interpreter.sys_executable().display()
                    );
                    true
                } else {
                    debug!(
                        "Ignoring Python interpreter at `{}`: only system interpreters allowed",
                        interpreter.sys_executable().display()
                    );
                    false
                }
            }
        }
    }

    /// Check the environment preference using both the discovery source and queried interpreter.
    ///
    /// Source filtering alone cannot determine whether an interpreter is in a virtual environment.
    pub(crate) fn satisfies_environment_preference(
        &self,
        preference: EnvironmentPreference,
    ) -> bool {
        match (
            preference,
            // Conda environments are not conformant virtual environments but we treat them as such.
            self.interpreter.is_virtualenv() || (matches!(self.source, PythonSource::CondaPrefix)),
        ) {
            (EnvironmentPreference::Any, _) => true,
            (EnvironmentPreference::OnlyVirtual, true) => true,
            (EnvironmentPreference::OnlyVirtual, false) => {
                debug!(
                    "Ignoring Python interpreter at `{}`: only virtual environments allowed",
                    self.interpreter.sys_executable().display()
                );
                false
            }
            (EnvironmentPreference::ExplicitSystem, true) => true,
            (EnvironmentPreference::ExplicitSystem, false) => {
                if matches!(
                    self.source,
                    PythonSource::ProvidedPath | PythonSource::ParentInterpreter
                ) {
                    debug!(
                        "Allowing explicitly requested system Python interpreter at `{}`",
                        self.interpreter.sys_executable().display()
                    );
                    true
                } else {
                    debug!(
                        "Ignoring Python interpreter at `{}`: system interpreter not explicitly requested",
                        self.interpreter.sys_executable().display()
                    );
                    false
                }
            }
            (EnvironmentPreference::OnlySystem, true) => {
                debug!(
                    "Ignoring Python interpreter at `{}`: system interpreter required",
                    self.interpreter.sys_executable().display()
                );
                false
            }
            (EnvironmentPreference::OnlySystem, false) => true,
        }
    }

    /// Check the version request after adjusting its defaults for this installation's source.
    fn matches_version_request(&self, request: &VersionRequest) -> bool {
        let request = request.clone().into_request_for_source(self.source);
        self.interpreter.matches_version_request(&request)
    }
}

/// Find a [`PythonEnvironment`] matching the given request and preference.
///
/// If looking for a Python interpreter to create a new environment, use [`PythonInstallation::find`]
/// instead.
pub fn find_environment(
    request: &PythonRequest,
    preference: EnvironmentPreference,
    python_preference: PythonPreference,
    python_arch: Option<PythonArchitecture>,
    cache: &Cache,
) -> Result<PythonEnvironment, Error> {
    let installation =
        match find_python_installation(request, preference, python_preference, python_arch, cache)?
        {
            Ok(installation) => installation,
            Err(err) => {
                return Err(
                    EnvironmentNotFound::new(err.request, err.environment_preference).into(),
                );
            }
        };
    Ok(PythonEnvironment::from_interpreter(
        installation.into_interpreter(),
    ))
}
