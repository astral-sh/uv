use std::path::Path;

use tracing::debug;
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::{ActiveEnvironment, DependencyGroupsWithDefaults};
use uv_lock::Installable;
use uv_python::downloads::Reporter;
use uv_python::{
    ConfigDiscovery, EnvironmentPreference, Interpreter, PythonArchitecture, PythonDownloads,
    PythonEnvironment, PythonInstallation, PythonPreference, PythonRequest, PythonVersionFile,
    VersionFileDiscoveryOptions,
};
use uv_settings::PythonInstallMirrors;
use uv_warnings::warn_user;
use uv_workspace::{ProjectEnvironmentSelection, Workspace};

use crate::commands::pip::operations::report_interpreter;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::{
    ProjectEnvironmentPolicy, ProjectEnvironmentTarget, ProjectError, ProjectInterpreter,
    ProjectPythonRequirement, PythonRequestSource, centralized_environment_root,
    centralized_environments_enabled, discover_project_environment, find_lockfile_requires_python,
    find_project_python_requirement, is_centralized_environment_path, read_environment_path_file,
    validate_python_requirement,
};
use crate::commands::reporters::PythonDownloadReporter;
use crate::printer::Printer;

/// Interpreter search settings, independent of whether project compatibility is required.
pub(crate) struct PythonDiscovery<'a> {
    environment_preference: EnvironmentPreference,
    python_preference: PythonPreference,
    python_arch: Option<PythonArchitecture>,
    client_builder: &'a BaseClientBuilder<'a>,
    cache: &'a Cache,
    python_downloads: Option<PythonDownloads>,
    reporter: Option<&'a dyn Reporter>,
    python_install_mirror: Option<&'a str>,
    pypy_install_mirror: Option<&'a str>,
    python_downloads_json_url: Option<&'a str>,
    report: Option<Printer>,
}

impl<'a> PythonDiscovery<'a> {
    /// Search installed interpreters without downloading missing versions.
    pub(crate) fn new(
        environment_preference: EnvironmentPreference,
        python_preference: PythonPreference,
        python_arch: Option<PythonArchitecture>,
        client_builder: &'a BaseClientBuilder<'a>,
        cache: &'a Cache,
    ) -> Self {
        Self {
            environment_preference,
            python_preference,
            python_arch,
            client_builder,
            cache,
            python_downloads: None,
            reporter: None,
            python_install_mirror: None,
            pypy_install_mirror: None,
            python_downloads_json_url: None,
            report: None,
        }
    }

    /// Allow discovery to consider downloads under the given policy.
    pub(crate) fn with_downloads(
        mut self,
        python_downloads: PythonDownloads,
        install_mirrors: &'a PythonInstallMirrors,
        reporter: &'a PythonDownloadReporter,
    ) -> Self {
        self.python_downloads = Some(python_downloads);
        self.reporter = Some(reporter);
        self.python_install_mirror = install_mirrors.python_install_mirror.as_deref();
        self.pypy_install_mirror = install_mirrors.pypy_install_mirror.as_deref();
        self.python_downloads_json_url = install_mirrors.python_downloads_json_url.as_deref();
        self
    }

    /// Use a custom download list when checking for outdated managed prereleases.
    pub(crate) fn with_downloads_json_url(mut self, url: Option<&'a str>) -> Self {
        self.python_downloads_json_url = url;
        self
    }

    /// Report the selected interpreter before any compatibility diagnostic.
    pub(crate) fn with_report(mut self, printer: Printer) -> Self {
        self.report = Some(printer);
        self
    }
}

/// An interpreter that satisfies the project requirement used to select it.
///
/// Only project discovery can construct this type. Warning-only commands and existing
/// environments preserved by `--no-sync` do not use it.
#[derive(Debug)]
pub(crate) struct CompatibleProjectPython(PythonInstallation);

impl CompatibleProjectPython {
    /// Borrow the compatible interpreter.
    pub(super) fn interpreter(&self) -> &Interpreter {
        self.0.interpreter()
    }

    /// Consume the compatible interpreter for use by the environment or resolver APIs.
    pub(super) fn into_interpreter(self) -> Interpreter {
        self.0.into_interpreter()
    }
}

/// A discovered installation together with its compatibility with the project requirement.
enum PythonSelection {
    Compatible(CompatibleProjectPython),
    Incompatible {
        installation: PythonInstallation,
        reason: ProjectError,
    },
}

impl PythonSelection {
    /// Borrow the installation while deciding whether to reuse an existing environment.
    fn installation(&self) -> &PythonInstallation {
        match self {
            Self::Compatible(python) => &python.0,
            Self::Incompatible { installation, .. } => installation,
        }
    }

    /// Finish strict discovery, reporting the selected interpreter if requested.
    fn into_compatible(
        self,
        report: Option<Printer>,
    ) -> Result<CompatibleProjectPython, ProjectError> {
        if let Some(printer) = report {
            report_interpreter(self.installation(), false, printer)?;
        }
        match self {
            Self::Compatible(python) => Ok(python),
            Self::Incompatible { reason, .. } => Err(reason),
        }
    }

    /// Finish warning-only discovery, reporting any incompatibility before returning.
    fn into_installation(
        self,
        report: Option<Printer>,
    ) -> Result<PythonInstallation, ProjectError> {
        if let Some(printer) = report {
            report_interpreter(self.installation(), false, printer)?;
        }
        match self {
            Self::Compatible(python) => Ok(python.0),
            Self::Incompatible {
                installation,
                reason,
            } => {
                warn_user!("{reason}");
                Ok(installation)
            }
        }
    }
}

/// The resolved Python request, project requirement, and diagnostic sources.
#[derive(Debug, Clone)]
pub(crate) struct ProjectPythonRequest {
    /// The source of the Python request.
    source: PythonRequestSource,
    /// The resolved Python request, computed by considering (1) any explicit request from the user
    /// via `--python`, (2) any implicit request from the user via `.python-version`, and (3) the
    /// workspace or lockfile's `Requires-Python` specifier.
    python_request: Option<PythonRequest>,
    /// The resolved Python requirement for the project and its source.
    requirement: Option<ProjectPythonRequirement>,
}

impl ProjectPythonRequest {
    /// Whether the request permits upgrading to a newer patch release.
    pub(crate) fn upgradeable(&self) -> bool {
        self.python_request
            .as_ref()
            .is_none_or(|request| !request.includes_patch())
    }

    /// Determine the Python request and requirement from a frozen lockfile.
    pub(super) async fn from_lockfile(
        python_request: Option<PythonRequest>,
        target: InstallTarget<'_>,
        groups: &DependencyGroupsWithDefaults,
        project_dir: &Path,
        config_discovery: ConfigDiscovery,
    ) -> Result<Self, ProjectError> {
        Self::from_requirements(
            python_request,
            Some(target.install_path()),
            Some(find_lockfile_requires_python(target, groups)?),
            project_dir,
            config_discovery,
        )
        .await
    }

    /// Determine the [`ProjectPythonRequest`] for the current [`Workspace`].
    pub(crate) async fn from_request(
        python_request: Option<PythonRequest>,
        workspace: Option<&Workspace>,
        groups: &DependencyGroupsWithDefaults,
        project_dir: &Path,
        config_discovery: ConfigDiscovery,
    ) -> Result<Self, ProjectError> {
        let requirement = workspace
            .map(|workspace| find_project_python_requirement(workspace, groups))
            .transpose()?
            .flatten();

        Self::from_requirements(
            python_request,
            workspace.map(|workspace| workspace.install_path().as_path()),
            requirement,
            project_dir,
            config_discovery,
        )
        .await
    }

    /// Select a Python request using a project's root and Python requirement.
    async fn from_requirements(
        python_request: Option<PythonRequest>,
        workspace_root: Option<&Path>,
        requirement: Option<ProjectPythonRequirement>,
        project_dir: &Path,
        config_discovery: ConfigDiscovery,
    ) -> Result<Self, ProjectError> {
        let (source, python_request) = if let Some(request) = python_request {
            // (1) Explicit request from user
            let source = PythonRequestSource::UserRequest;
            let request = Some(request);
            (source, request)
        } else if let Some(file) = PythonVersionFile::discover(
            project_dir,
            &VersionFileDiscoveryOptions::default()
                .with_stop_discovery_at(workspace_root)
                .with_config_discovery(config_discovery),
        )
        .await?
        .filter(|file| {
            // Ignore global version files that are incompatible with requires-python
            if !file.is_global() {
                return true;
            }
            match (file.version(), requirement.as_ref()) {
                (Some(request), Some(requirement)) => request
                    .as_pep440_version()
                    .is_none_or(|version| requirement.requires_python.contains(&version)),
                _ => true,
            }
        }) {
            // (2) Request from `.python-version`
            let source = PythonRequestSource::DotPythonVersion(file.clone());
            let request = file.version().cloned();
            (source, request)
        } else {
            // (3) `requires-python` in `pyproject.toml`
            let request = requirement.as_ref().and_then(|requirement| {
                PythonRequest::from_requires_python(&requirement.requires_python)
            });
            let source = PythonRequestSource::RequiresPython;
            (source, request)
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
            requirement,
        })
    }

    /// Find an installation for the request, warning if it does not satisfy the project requirement.
    pub(crate) async fn find(
        &self,
        discovery: &PythonDiscovery<'_>,
    ) -> Result<PythonInstallation, ProjectError> {
        self.select(discovery)
            .await?
            .into_installation(discovery.report)
    }

    /// Find an interpreter that satisfies both the request and the project requirement.
    pub(super) async fn find_compatible(
        &self,
        discovery: &PythonDiscovery<'_>,
    ) -> Result<CompatibleProjectPython, ProjectError> {
        self.select(discovery)
            .await?
            .into_compatible(discovery.report)
    }

    /// Discover an installation and retain its compatibility before applying command policy.
    async fn select(
        &self,
        discovery: &PythonDiscovery<'_>,
    ) -> Result<PythonSelection, ProjectError> {
        let installation = if let Some(python_downloads) = discovery.python_downloads {
            PythonInstallation::find_or_download(
                self.python_request.as_ref(),
                discovery.environment_preference,
                discovery.python_preference,
                discovery.python_arch,
                python_downloads,
                discovery.client_builder,
                discovery.cache,
                discovery.reporter,
                discovery.python_install_mirror,
                discovery.pypy_install_mirror,
                discovery.python_downloads_json_url,
            )
            .await?
        } else {
            let request = self
                .python_request
                .as_ref()
                .unwrap_or(&PythonRequest::Default);
            let installation = PythonInstallation::find_existing(
                request,
                discovery.environment_preference,
                discovery.python_preference,
                discovery.python_arch,
                discovery.cache,
            )?;
            installation
                .download_and_warn_if_outdated_prerelease(
                    request,
                    discovery.client_builder,
                    discovery.cache,
                    discovery.python_downloads_json_url,
                )
                .await?;
            installation
        };

        let compatibility = self.requirement.as_ref().map_or(Ok(()), |requirement| {
            validate_python_requirement(
                installation.interpreter(),
                &requirement.requires_python,
                &self.source,
                &requirement.source,
            )
        });
        Ok(match compatibility {
            Ok(()) => PythonSelection::Compatible(CompatibleProjectPython(installation)),
            Err(reason) => PythonSelection::Incompatible {
                installation,
                reason,
            },
        })
    }
}

impl ProjectInterpreter {
    /// Discover an interpreter for a workspace or frozen lockfile.
    pub(crate) async fn discover(
        target: ProjectEnvironmentTarget<'_>,
        project_python: ProjectPythonRequest,
        client_builder: &BaseClientBuilder<'_>,
        python_preference: PythonPreference,
        python_arch: Option<PythonArchitecture>,
        python_downloads: PythonDownloads,
        install_mirrors: &PythonInstallMirrors,
        policy: ProjectEnvironmentPolicy,
        active: ActiveEnvironment,
        cache: &Cache,
        printer: Printer,
    ) -> Result<Self, ProjectError> {
        let python_request = project_python.python_request.as_ref();
        let requires_python = project_python
            .requirement
            .as_ref()
            .map(|requirement| &requirement.requires_python);

        let environment_selection =
            ProjectEnvironmentSelection::from_install_path(target.install_path(), active);
        let centralized = centralized_environments_enabled(&environment_selection, cache);
        let upgradeable = python_request.is_none_or(|request| !request.includes_patch());

        // Prefer `.venv`'s interpreter to keep its compatible cached environment selected; derive
        // the cache root instead of trusting the link target.
        if centralized {
            let project_environment_path = target.install_path().join(".venv");
            if let Ok(candidate) = PythonEnvironment::from_root(
                read_environment_path_file(&project_environment_path)
                    .ok()
                    .as_deref()
                    .unwrap_or(&project_environment_path),
                cache,
            ) {
                let root = centralized_environment_root(
                    target,
                    candidate.interpreter(),
                    upgradeable,
                    cache,
                );
                if let Some(environment) = discover_project_environment(
                    &root,
                    python_request,
                    python_preference,
                    python_arch,
                    requires_python,
                    policy,
                    centralized,
                    cache,
                )? {
                    return Ok(Self::Environment(environment));
                }
            }
        } else {
            let project_environment_path = environment_selection
                .explicit_path()
                .map_or_else(|| target.install_path().join(".venv"), Path::to_path_buf);
            // TODO(tk): Revisit after PEP 832.
            // A centralized path file is not a local environment; let initialization replace it.
            if !(environment_selection.is_default()
                && read_environment_path_file(&project_environment_path)
                    .is_ok_and(|target| is_centralized_environment_path(&target, cache)))
                && let Some(environment) = discover_project_environment(
                    &project_environment_path,
                    python_request,
                    python_preference,
                    python_arch,
                    requires_python,
                    policy,
                    centralized,
                    cache,
                )?
            {
                return Ok(Self::Environment(environment));
            }
        }

        let reporter = PythonDownloadReporter::single(printer);

        let discovery = PythonDiscovery::new(
            EnvironmentPreference::OnlySystem,
            python_preference,
            python_arch,
            client_builder,
            cache,
        )
        .with_downloads(python_downloads, install_mirrors, &reporter)
        .with_report(printer);
        let selection = project_python.select(&discovery).await?;

        if centralized {
            let root = centralized_environment_root(
                target,
                selection.installation().interpreter(),
                upgradeable,
                cache,
            );
            if let Some(environment) = discover_project_environment(
                &root,
                python_request,
                python_preference,
                python_arch,
                requires_python,
                policy,
                centralized,
                cache,
            )? {
                return Ok(Self::Environment(environment));
            }
        }

        Ok(Self::Interpreter(
            selection.into_compatible(discovery.report)?,
        ))
    }
}
