use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Error, Result};
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use indexmap::IndexSet;
use itertools::{Either, Itertools};
use owo_colors::OwoColorize;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, trace};

use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_python::downloads::{
    self, ArchRequest, DownloadResult, ManagedPythonDownload, PythonDownloadRequest,
};
use uv_python::managed::{
    ManagedPythonInstallation, ManagedPythonInstallations, PythonMinorVersionLink,
    create_link_to_executable, python_executable_dir,
};
use uv_python::platform::{Arch, Libc};
use uv_python::{
    PythonDownloads, PythonInstallationKey, PythonInstallationMinorVersionKey, PythonRequest,
    PythonVersionFile, VersionFileDiscoveryOptions, VersionFilePreference, VersionRequest,
};
use uv_shell::Shell;
use uv_trampoline_builder::{Launcher, LauncherKind};
use uv_warnings::warn_user;

use crate::commands::python::{ChangeEvent, ChangeEventKind};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{ExitStatus, elapsed};
use crate::printer::Printer;
use crate::settings::NetworkSettings;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct InstallRequest {
    /// The original request from the user
    request: PythonRequest,
    /// A download request corresponding to the `request` with platform information filled
    download_request: PythonDownloadRequest,
    /// A download that satisfies the request
    download: &'static ManagedPythonDownload,
}

impl InstallRequest {
    fn new(request: PythonRequest, python_downloads_json_url: Option<&str>) -> Result<Self> {
        // Make sure the request is a valid download request and fill platform information
        let download_request = PythonDownloadRequest::from_request(&request)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "`{}` is not a valid Python download request; see `uv help python` for supported formats and `uv python list --only-downloads` for available versions",
                    request.to_canonical_string()
                )
            })?
            .fill()?;

        // Find a matching download
        let download =
            match ManagedPythonDownload::from_request(&download_request, python_downloads_json_url)
            {
                Ok(download) => download,
                Err(downloads::Error::NoDownloadFound(request))
                    if request.libc().is_some_and(Libc::is_musl)
                        && request
                            .arch()
                            .is_some_and(|arch| Arch::is_arm(&arch.inner())) =>
                {
                    return Err(anyhow::anyhow!(
                        "uv does not yet provide musl Python distributions on aarch64."
                    ));
                }
                Err(err) => return Err(err.into()),
            };

        Ok(Self {
            request,
            download_request,
            download,
        })
    }

    fn matches_installation(&self, installation: &ManagedPythonInstallation) -> bool {
        self.download_request.satisfied_by_key(installation.key())
    }

    fn python_request(&self) -> &PythonRequest {
        &self.request
    }
}

impl std::fmt::Display for InstallRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.request)
    }
}

#[derive(Debug, Default)]
struct Changelog {
    existing: FxHashSet<PythonInstallationKey>,
    installed: FxHashSet<PythonInstallationKey>,
    uninstalled: FxHashSet<PythonInstallationKey>,
    installed_executables: FxHashMap<PythonInstallationKey, FxHashSet<PathBuf>>,
}

impl Changelog {
    fn events(&self) -> impl Iterator<Item = ChangeEvent> {
        let reinstalled = self
            .uninstalled
            .intersection(&self.installed)
            .cloned()
            .collect::<FxHashSet<_>>();
        let uninstalled = self.uninstalled.difference(&reinstalled).cloned();
        let installed = self.installed.difference(&reinstalled).cloned();

        uninstalled
            .map(|key| ChangeEvent {
                key: key.clone(),
                kind: ChangeEventKind::Removed,
            })
            .chain(installed.map(|key| ChangeEvent {
                key: key.clone(),
                kind: ChangeEventKind::Added,
            }))
            .chain(reinstalled.iter().map(|key| ChangeEvent {
                key: key.clone(),
                kind: ChangeEventKind::Reinstalled,
            }))
            .sorted_unstable_by(|a, b| a.key.cmp(&b.key).then_with(|| a.kind.cmp(&b.kind)))
    }
}

#[derive(Debug, Clone, Copy)]
enum InstallErrorKind {
    DownloadUnpack,
    Bin,
    #[cfg(windows)]
    Registry,
}

/// Download and install Python versions.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn install(
    project_dir: &Path,
    install_dir: Option<PathBuf>,
    targets: Vec<String>,
    reinstall: bool,
    upgrade: bool,
    bin: Option<bool>,
    registry: Option<bool>,
    force: bool,
    python_install_mirror: Option<String>,
    pypy_install_mirror: Option<String>,
    python_downloads_json_url: Option<String>,
    network_settings: NetworkSettings,
    default: bool,
    python_downloads: PythonDownloads,
    no_config: bool,
    preview: PreviewMode,
    printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // TODO(zanieb): We should consider marking the Python installation as the default when
    // `--default` is used. It's not clear how this overlaps with a global Python pin, but I'd be
    // surprised if `uv python find` returned the "newest" Python version rather than the one I just
    // installed with the `--default` flag.
    if default && !preview.is_enabled() {
        warn_user!(
            "The `--default` option is experimental and may change without warning. Pass `--preview` to disable this warning"
        );
    }

    if upgrade && preview.is_disabled() {
        warn_user!(
            "`uv python upgrade` is experimental and may change without warning. Pass `--preview` to disable this warning"
        );
    }

    if default && targets.len() > 1 {
        anyhow::bail!("The `--default` flag cannot be used with multiple targets");
    }

    // Read the existing installations, lock the directory for the duration
    let installations = ManagedPythonInstallations::from_settings(install_dir.clone())?.init()?;
    let installations_dir = installations.root();
    let scratch_dir = installations.scratch();
    let _lock = installations.lock().await?;
    let existing_installations: Vec<_> = installations
        .find_all()?
        .inspect(|installation| trace!("Found existing installation {}", installation.key()))
        .collect();

    // Resolve the requests
    let mut is_default_install = false;
    let mut is_unspecified_upgrade = false;
    let requests: Vec<_> = if targets.is_empty() {
        if upgrade {
            is_unspecified_upgrade = true;
            let mut minor_version_requests = IndexSet::<InstallRequest>::default();
            for installation in &existing_installations {
                let request = VersionRequest::major_minor_request_from_key(installation.key());
                if let Ok(request) = InstallRequest::new(
                    PythonRequest::Version(request),
                    python_downloads_json_url.as_deref(),
                ) {
                    minor_version_requests.insert(request);
                }
            }
            minor_version_requests.into_iter().collect::<Vec<_>>()
        } else {
            PythonVersionFile::discover(
                project_dir,
                &VersionFileDiscoveryOptions::default()
                    .with_no_config(no_config)
                    .with_preference(VersionFilePreference::Versions),
            )
            .await?
            .map(PythonVersionFile::into_versions)
            .unwrap_or_else(|| {
                // If no version file is found and no requests were made
                // TODO(zanieb): We should consider differentiating between a global Python version
                // file here, allowing a request from there to enable `is_default_install`.
                is_default_install = true;
                vec![if reinstall {
                    // On bare `--reinstall`, reinstall all Python versions
                    PythonRequest::Any
                } else {
                    PythonRequest::Default
                }]
            })
            .into_iter()
            .map(|a| InstallRequest::new(a, python_downloads_json_url.as_deref()))
            .collect::<Result<Vec<_>>>()?
        }
    } else {
        targets
            .iter()
            .map(|target| PythonRequest::parse(target.as_str()))
            .map(|a| InstallRequest::new(a, python_downloads_json_url.as_deref()))
            .collect::<Result<Vec<_>>>()?
    };

    let Some(first_request) = requests.first() else {
        if upgrade {
            writeln!(
                printer.stderr(),
                "There are no installed versions to upgrade"
            )?;
        }
        return Ok(ExitStatus::Success);
    };

    let requested_minor_versions = requests
        .iter()
        .filter_map(|request| {
            if let PythonRequest::Version(VersionRequest::MajorMinor(major, minor, ..)) =
                request.python_request()
            {
                uv_pep440::Version::from_str(&format!("{major}.{minor}")).ok()
            } else {
                None
            }
        })
        .collect::<IndexSet<_>>();

    if upgrade
        && requests
            .iter()
            .any(|request| request.request.includes_patch())
    {
        writeln!(
            printer.stderr(),
            "error: `uv python upgrade` only accepts minor versions"
        )?;
        return Ok(ExitStatus::Failure);
    }

    // Find requests that are already satisfied
    let mut changelog = Changelog::default();
    let (satisfied, unsatisfied): (Vec<_>, Vec<_>) = if reinstall {
        // In the reinstall case, we want to iterate over all matching installations instead of
        // stopping at the first match.

        let mut unsatisfied: Vec<Cow<InstallRequest>> =
            Vec::with_capacity(existing_installations.len() + requests.len());

        for request in &requests {
            let mut matching_installations = existing_installations
                .iter()
                .filter(|installation| request.matches_installation(installation))
                .peekable();

            if matching_installations.peek().is_none() {
                debug!("No installation found for request `{}`", request.cyan());
                unsatisfied.push(Cow::Borrowed(request));
            }

            for installation in matching_installations {
                changelog.existing.insert(installation.key().clone());
                if matches!(&request.request, &PythonRequest::Any) {
                    // Construct an install request matching the existing installation
                    match InstallRequest::new(
                        PythonRequest::Key(installation.into()),
                        python_downloads_json_url.as_deref(),
                    ) {
                        Ok(request) => {
                            debug!("Will reinstall `{}`", installation.key().green());
                            unsatisfied.push(Cow::Owned(request));
                        }
                        Err(err) => {
                            // This shouldn't really happen, but maybe a new version of uv dropped
                            // support for a key we previously supported
                            warn_user!(
                                "Failed to create reinstall request for existing installation `{}`: {err}",
                                installation.key().green()
                            );
                        }
                    }
                } else {
                    // TODO(zanieb): This isn't really right! But we need `--upgrade` or similar
                    // to handle this case correctly without causing a breaking change.

                    // If we have real requests, just ignore the existing installation
                    debug!(
                        "Ignoring match `{}` for request `{}` due to `--reinstall` flag",
                        installation.key().green(),
                        request.cyan()
                    );
                    unsatisfied.push(Cow::Borrowed(request));
                    break;
                }
            }
        }
        (vec![], unsatisfied)
    } else {
        // If we can find one existing installation that matches the request, it is satisfied
        requests.iter().partition_map(|request| {
            if let Some(installation) = existing_installations.iter().find(|installation| {
                if upgrade {
                    // If this is an upgrade, the requested version is a minor version
                    // but the requested download is the highest patch for that minor
                    // version. We need to install it unless an exact match is found.
                    request.download.key() == installation.key()
                } else {
                    request.matches_installation(installation)
                }
            }) {
                debug!(
                    "Found `{}` for request `{}`",
                    installation.key().green(),
                    request.cyan(),
                );
                Either::Left(installation)
            } else {
                debug!("No installation found for request `{}`", request.cyan());
                Either::Right(Cow::Borrowed(request))
            }
        })
    };

    // Check if Python downloads are banned
    if matches!(python_downloads, PythonDownloads::Never) && !unsatisfied.is_empty() {
        writeln!(
            printer.stderr(),
            "Python downloads are not allowed (`python-downloads = \"never\"`). Change to `python-downloads = \"manual\"` to allow explicit installs.",
        )?;
        return Ok(ExitStatus::Failure);
    }

    // Find downloads for the requests
    let downloads = unsatisfied
        .iter()
        .inspect(|request| {
            debug!(
                "Found download `{}` for request `{}`",
                request.download,
                request.cyan(),
            );
        })
        .map(|request| request.download)
        // Ensure we only download each version once
        .unique_by(|download| download.key())
        .collect::<Vec<_>>();

    // Download and unpack the Python versions concurrently
    let client = uv_client::BaseClientBuilder::new()
        .retries_from_env()?
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone())
        .build();
    let reporter = PythonDownloadReporter::new(printer, downloads.len() as u64);
    let mut tasks = FuturesUnordered::new();

    for download in &downloads {
        tasks.push(async {
            (
                *download,
                download
                    .fetch_with_retry(
                        &client,
                        installations_dir,
                        &scratch_dir,
                        reinstall,
                        python_install_mirror.as_deref(),
                        pypy_install_mirror.as_deref(),
                        Some(&reporter),
                    )
                    .await,
            )
        });
    }

    let mut errors = vec![];
    let mut downloaded = Vec::with_capacity(downloads.len());
    let mut requests_by_new_installation = BTreeMap::new();
    while let Some((download, result)) = tasks.next().await {
        match result {
            Ok(download_result) => {
                let path = match download_result {
                    // We should only encounter already-available during concurrent installs
                    DownloadResult::AlreadyAvailable(path) => path,
                    DownloadResult::Fetched(path) => path,
                };

                let installation = ManagedPythonInstallation::new(path, download);
                changelog.installed.insert(installation.key().clone());
                for request in &requests {
                    // Take note of which installations satisfied which requests
                    if request.matches_installation(&installation) {
                        requests_by_new_installation
                            .entry(installation.key().clone())
                            .or_insert(Vec::new())
                            .push(request);
                    }
                }
                if changelog.existing.contains(installation.key()) {
                    changelog.uninstalled.insert(installation.key().clone());
                }
                downloaded.push(installation.clone());
            }
            Err(err) => {
                errors.push((
                    InstallErrorKind::DownloadUnpack,
                    download.key().clone(),
                    anyhow::Error::new(err),
                ));
            }
        }
    }

    let bin_dir = if matches!(bin, Some(false)) {
        None
    } else {
        Some(python_executable_dir()?)
    };

    let installations: Vec<_> = downloaded.iter().chain(satisfied.iter().copied()).collect();

    // Ensure that the installations are _complete_ for both downloaded installations and existing
    // installations that match the request
    for installation in &installations {
        installation.ensure_externally_managed()?;
        installation.ensure_sysconfig_patched()?;
        installation.ensure_canonical_executables()?;
        if let Err(e) = installation.ensure_dylib_patched() {
            e.warn_user(installation);
        }

        let upgradeable = (default || is_default_install)
            || requested_minor_versions.contains(&installation.key().version().python_version());

        if let Some(bin_dir) = bin_dir.as_ref() {
            create_bin_links(
                installation,
                bin_dir,
                reinstall,
                force,
                default,
                upgradeable,
                upgrade,
                is_default_install,
                first_request,
                &existing_installations,
                &installations,
                &mut changelog,
                &mut errors,
                preview,
            );
        }

        if !matches!(registry, Some(false)) {
            #[cfg(windows)]
            {
                match uv_python::windows_registry::create_registry_entry(installation) {
                    Ok(()) => {}
                    Err(err) => {
                        errors.push((
                            InstallErrorKind::Registry,
                            installation.key().clone(),
                            err.into(),
                        ));
                    }
                }
            }
        }
    }

    let minor_versions =
        PythonInstallationMinorVersionKey::highest_installations_by_minor_version_key(
            installations
                .iter()
                .copied()
                .chain(existing_installations.iter()),
        );

    for installation in minor_versions.values() {
        if upgrade {
            // During an upgrade, update existing symlinks but avoid
            // creating new ones.
            installation.update_minor_version_link(preview)?;
        } else {
            installation.ensure_minor_version_link(preview)?;
        }
    }

    if changelog.installed.is_empty() && errors.is_empty() {
        if is_default_install {
            writeln!(
                printer.stderr(),
                "Python is already installed. Use `uv python install <request>` to install another version.",
            )?;
        } else if upgrade && requests.is_empty() {
            writeln!(
                printer.stderr(),
                "There are no installed versions to upgrade"
            )?;
        } else if requests.len() > 1 {
            if upgrade {
                if is_unspecified_upgrade {
                    writeln!(
                        printer.stderr(),
                        "All versions already on latest supported patch release"
                    )?;
                } else {
                    writeln!(
                        printer.stderr(),
                        "All requested versions already on latest supported patch release"
                    )?;
                }
            } else {
                writeln!(printer.stderr(), "All requested versions already installed")?;
            }
        }
        return Ok(ExitStatus::Success);
    }

    if !changelog.installed.is_empty() {
        for install_key in &changelog.installed {
            // Make a note if the selected python is non-native for the architecture,
            // if none of the matching user requests were explicit
            let native_arch = Arch::from_env();
            if install_key.arch().family() != native_arch.family() {
                let not_explicit =
                    requests_by_new_installation
                        .get(install_key)
                        .and_then(|requests| {
                            let all_non_explicit = requests.iter().all(|request| {
                                if let PythonRequest::Key(key) = &request.request {
                                    !matches!(key.arch(), Some(ArchRequest::Explicit(_)))
                                } else {
                                    true
                                }
                            });
                            if all_non_explicit {
                                requests.iter().next()
                            } else {
                                None
                            }
                        });
                if let Some(not_explicit) = not_explicit {
                    let native_request =
                        not_explicit.download_request.clone().with_arch(native_arch);
                    writeln!(
                        printer.stderr(),
                        "{} uv selected a Python distribution with an emulated architecture ({}) for your platform because support for the native architecture ({}) is not yet mature; to override this behaviour, request the native architecture explicitly with: {}",
                        "note:".bold(),
                        install_key.arch(),
                        native_arch,
                        native_request
                    )?;
                }
            }
        }
        if changelog.installed.len() == 1 {
            let installed = changelog.installed.iter().next().unwrap();
            // Ex) "Installed Python 3.9.7 in 1.68s"
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Installed {} {}",
                    format!("Python {}", installed.version()).bold(),
                    format!("in {}", elapsed(start.elapsed())).dimmed()
                )
                .dimmed()
            )?;
        } else {
            // Ex) "Installed 2 versions in 1.68s"
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Installed {} {}",
                    format!("{} versions", changelog.installed.len()).bold(),
                    format!("in {}", elapsed(start.elapsed())).dimmed()
                )
                .dimmed()
            )?;
        }

        for event in changelog.events() {
            let executables = format_executables(&event, &changelog.installed_executables);
            match event.kind {
                ChangeEventKind::Added => {
                    writeln!(
                        printer.stderr(),
                        " {} {}{executables}",
                        "+".green(),
                        event.key.bold()
                    )?;
                }
                ChangeEventKind::Removed => {
                    writeln!(
                        printer.stderr(),
                        " {} {}{executables}",
                        "-".red(),
                        event.key.bold()
                    )?;
                }
                ChangeEventKind::Reinstalled => {
                    writeln!(
                        printer.stderr(),
                        " {} {}{executables}",
                        "~".yellow(),
                        event.key.bold(),
                    )?;
                }
            }
        }

        if let Some(bin_dir) = bin_dir.as_ref() {
            warn_if_not_on_path(bin_dir);
        }
    }

    if !errors.is_empty() {
        // If there are only side-effect install errors and the user didn't opt-in, we're only going
        // to warn
        let fatal = !errors.iter().all(|(kind, _, _)| match kind {
            InstallErrorKind::Bin => bin.is_none(),
            #[cfg(windows)]
            InstallErrorKind::Registry => registry.is_none(),
            InstallErrorKind::DownloadUnpack => false,
        });

        for (kind, key, err) in errors
            .into_iter()
            .sorted_unstable_by(|(_, key_a, _), (_, key_b, _)| key_a.cmp(key_b))
        {
            let (level, verb) = match kind {
                InstallErrorKind::DownloadUnpack => ("error".red().bold().to_string(), "install"),
                InstallErrorKind::Bin => {
                    let level = match bin {
                        None => "warning".yellow().bold().to_string(),
                        Some(false) => continue,
                        Some(true) => "error".red().bold().to_string(),
                    };
                    (level, "install executable for")
                }
                #[cfg(windows)]
                InstallErrorKind::Registry => {
                    let level = match registry {
                        None => "warning".yellow().bold().to_string(),
                        Some(false) => continue,
                        Some(true) => "error".red().bold().to_string(),
                    };
                    (level, "install registry entry for")
                }
            };

            writeln!(
                printer.stderr(),
                "{level}{} Failed to {verb} {}",
                ":".bold(),
                key.green()
            )?;
            for err in err.chain() {
                writeln!(
                    printer.stderr(),
                    "  {}: {}",
                    "Caused by".red().bold(),
                    err.to_string().trim()
                )?;
            }
        }

        if fatal {
            return Ok(ExitStatus::Failure);
        }
    }

    Ok(ExitStatus::Success)
}

/// Link the binaries of a managed Python installation to the bin directory.
///
/// This function is fallible, but errors are pushed to `errors` instead of being thrown.
#[allow(clippy::fn_params_excessive_bools)]
fn create_bin_links(
    installation: &ManagedPythonInstallation,
    bin: &Path,
    reinstall: bool,
    force: bool,
    default: bool,
    upgradeable: bool,
    upgrade: bool,
    is_default_install: bool,
    first_request: &InstallRequest,
    existing_installations: &[ManagedPythonInstallation],
    installations: &[&ManagedPythonInstallation],
    changelog: &mut Changelog,
    errors: &mut Vec<(InstallErrorKind, PythonInstallationKey, Error)>,
    preview: PreviewMode,
) {
    // TODO(zanieb): We want more feedback on the `is_default_install` behavior before stabilizing
    // it. In particular, it may be confusing because it does not apply when versions are loaded
    // from a `.python-version` file.
    let targets = if (default || (is_default_install && preview.is_enabled()))
        && first_request.matches_installation(installation)
    {
        vec![
            installation.key().executable_name_minor(),
            installation.key().executable_name_major(),
            installation.key().executable_name(),
        ]
    } else {
        vec![installation.key().executable_name_minor()]
    };

    for target in targets {
        let target = bin.join(target);
        let executable = if upgradeable {
            if let Some(minor_version_link) =
                PythonMinorVersionLink::from_installation(installation, preview)
            {
                minor_version_link.symlink_executable.clone()
            } else {
                installation.executable(false)
            }
        } else {
            installation.executable(false)
        };

        match create_link_to_executable(&target, &executable) {
            Ok(()) => {
                debug!(
                    "Installed executable at `{}` for {}",
                    target.simplified_display(),
                    installation.key(),
                );
                changelog.installed.insert(installation.key().clone());
                changelog
                    .installed_executables
                    .entry(installation.key().clone())
                    .or_default()
                    .insert(target.clone());
            }
            Err(uv_python::managed::Error::LinkExecutable { from: _, to, err })
                if err.kind() == ErrorKind::AlreadyExists =>
            {
                debug!(
                    "Inspecting existing executable at `{}`",
                    target.simplified_display()
                );

                //  Figure out what installation it references, if any
                let existing = find_matching_bin_link(
                    installations
                        .iter()
                        .copied()
                        .chain(existing_installations.iter()),
                    &target,
                );

                match existing {
                    None => {
                        // Determine if the link is valid, i.e., if it points to an existing
                        // Python we don't manage. On Windows, we just assume it is valid because
                        // symlinks are not common for Python interpreters.
                        let valid_link = cfg!(windows)
                            || target
                                .read_link()
                                .and_then(|target| target.try_exists())
                                .inspect_err(|err| {
                                    debug!("Failed to inspect executable with error: {err}");
                                })
                                // If we can't verify the link, assume it is valid.
                                .unwrap_or(true);

                        // There's an existing executable we don't manage, require `--force`
                        if valid_link {
                            if !force {
                                if upgrade {
                                    warn_user!(
                                        "Executable already exists at `{}` but is not managed by uv; use `uv python install {}.{}{} --force` to replace it",
                                        to.simplified_display(),
                                        installation.key().major(),
                                        installation.key().minor(),
                                        installation.key().variant().suffix()
                                    );
                                } else {
                                    errors.push((
                                        InstallErrorKind::Bin,
                                        installation.key().clone(),
                                        anyhow::anyhow!(
                                            "Executable already exists at `{}` but is not managed by uv; use `--force` to replace it",
                                            to.simplified_display()
                                        ),
                                    ));
                                }
                                continue;
                            }
                            debug!(
                                "Replacing existing executable at `{}` due to `--force`",
                                target.simplified_display()
                            );
                        } else {
                            debug!(
                                "Replacing broken symlink at `{}`",
                                target.simplified_display()
                            );
                        }
                    }
                    Some(existing) if existing == installation => {
                        // The existing link points to the same installation, so we're done unless
                        // they requested we reinstall
                        if !(reinstall || force) {
                            debug!(
                                "Executable at `{}` is already for `{}`",
                                target.simplified_display(),
                                installation.key(),
                            );
                            continue;
                        }
                        debug!(
                            "Replacing existing executable for `{}` at `{}`",
                            installation.key(),
                            target.simplified_display(),
                        );
                    }
                    Some(existing) => {
                        // The existing link points to a different installation, check if it
                        // is reasonable to replace
                        if force {
                            debug!(
                                "Replacing existing executable for `{}` at `{}` with executable for `{}` due to `--force` flag",
                                existing.key(),
                                target.simplified_display(),
                                installation.key(),
                            );
                        } else {
                            if installation.is_upgrade_of(existing) {
                                debug!(
                                    "Replacing existing executable for `{}` at `{}` with executable for `{}` since it is an upgrade",
                                    existing.key(),
                                    target.simplified_display(),
                                    installation.key(),
                                );
                            } else if default {
                                debug!(
                                    "Replacing existing executable for `{}` at `{}` with executable for `{}` since `--default` was requested`",
                                    existing.key(),
                                    target.simplified_display(),
                                    installation.key(),
                                );
                            } else {
                                debug!(
                                    "Executable already exists for `{}` at `{}`. Use `--force` to replace it",
                                    existing.key(),
                                    to.simplified_display()
                                );
                                continue;
                            }
                        }
                    }
                }

                // Replace the existing link
                if let Err(err) = fs_err::remove_file(&to) {
                    errors.push((
                        InstallErrorKind::Bin,
                        installation.key().clone(),
                        anyhow::anyhow!(
                            "Executable already exists at `{}` but could not be removed: {err}",
                            to.simplified_display()
                        ),
                    ));
                    continue;
                }

                if let Some(existing) = existing {
                    // Ensure we do not report installation of this executable for an existing
                    // key if we undo it
                    changelog
                        .installed_executables
                        .entry(existing.key().clone())
                        .or_default()
                        .remove(&target);
                }

                if let Err(err) = create_link_to_executable(&target, &executable) {
                    errors.push((
                        InstallErrorKind::Bin,
                        installation.key().clone(),
                        anyhow::anyhow!(
                            "Failed to create link at `{}`: {err}",
                            target.simplified_display()
                        ),
                    ));
                    continue;
                }

                debug!(
                    "Updated executable at `{}` to {}",
                    target.simplified_display(),
                    installation.key(),
                );
                changelog.installed.insert(installation.key().clone());
                changelog
                    .installed_executables
                    .entry(installation.key().clone())
                    .or_default()
                    .insert(target.clone());
            }
            Err(err) => {
                errors.push((
                    InstallErrorKind::Bin,
                    installation.key().clone(),
                    Error::new(err),
                ));
            }
        }
    }
}

pub(crate) fn format_executables(
    event: &ChangeEvent,
    executables: &FxHashMap<PythonInstallationKey, FxHashSet<PathBuf>>,
) -> String {
    let Some(installed) = executables.get(&event.key) else {
        return String::new();
    };

    if installed.is_empty() {
        return String::new();
    }

    let names = installed
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy())
        // Do not include the `.exe` during comparisons, it can change the ordering
        .sorted_unstable_by(|a, b| a.trim_end_matches(".exe").cmp(b.trim_end_matches(".exe")))
        .join(", ");

    format!(" ({names})")
}

fn warn_if_not_on_path(bin: &Path) {
    if !Shell::contains_path(bin) {
        if let Some(shell) = Shell::from_env() {
            if let Some(command) = shell.prepend_path(bin) {
                if shell.supports_update() {
                    warn_user!(
                        "`{}` is not on your PATH. To use installed Python executables, run `{}` or `{}`.",
                        bin.simplified_display().cyan(),
                        command.green(),
                        "uv python update-shell".green()
                    );
                } else {
                    warn_user!(
                        "`{}` is not on your PATH. To use installed Python executables, run `{}`.",
                        bin.simplified_display().cyan(),
                        command.green()
                    );
                }
            } else {
                warn_user!(
                    "`{}` is not on your PATH. To use installed Python executables, add the directory to your PATH.",
                    bin.simplified_display().cyan(),
                );
            }
        } else {
            warn_user!(
                "`{}` is not on your PATH. To use installed Python executables, add the directory to your PATH.",
                bin.simplified_display().cyan(),
            );
        }
    }
}

/// Find the [`ManagedPythonInstallation`] corresponding to an executable link installed at the
/// given path, if any.
///
/// Will resolve symlinks on Unix. On Windows, will resolve the target link for a trampoline.
fn find_matching_bin_link<'a>(
    mut installations: impl Iterator<Item = &'a ManagedPythonInstallation>,
    path: &Path,
) -> Option<&'a ManagedPythonInstallation> {
    let target = if cfg!(unix) {
        if !path.is_symlink() {
            return None;
        }
        fs_err::canonicalize(path).ok()?
    } else if cfg!(windows) {
        let launcher = Launcher::try_from_path(path).ok()??;
        if !matches!(launcher.kind, LauncherKind::Python) {
            return None;
        }
        dunce::canonicalize(launcher.python_path).ok()?
    } else {
        unreachable!("Only Windows and Unix are supported")
    };

    installations.find(|installation| installation.executable(false) == target)
}
