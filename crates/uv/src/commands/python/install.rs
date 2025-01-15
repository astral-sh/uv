use std::fmt::Write;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::Result;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use itertools::{Either, Itertools};
use owo_colors::OwoColorize;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, trace};

use uv_client::Connectivity;
use uv_configuration::PreviewMode;
use uv_configuration::TrustedHost;
use uv_fs::Simplified;
use uv_python::downloads::{self, DownloadResult, ManagedPythonDownload, PythonDownloadRequest};
use uv_python::managed::{
    python_executable_dir, ManagedPythonInstallation, ManagedPythonInstallations,
};
use uv_python::platform::Libc;
use uv_python::{
    PythonDownloads, PythonInstallationKey, PythonRequest, PythonVersionFile,
    VersionFileDiscoveryOptions, VersionFilePreference,
};
use uv_shell::Shell;
use uv_trampoline_builder::{Launcher, LauncherKind};
use uv_warnings::warn_user;

use crate::commands::python::{ChangeEvent, ChangeEventKind};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;

#[derive(Debug, Clone)]
struct InstallRequest {
    /// The original request from the user
    request: PythonRequest,
    /// A download request corresponding to the `request` with platform information filled
    download_request: PythonDownloadRequest,
    /// A download that satisfies the request
    download: &'static ManagedPythonDownload,
}

impl InstallRequest {
    fn new(request: PythonRequest) -> Result<Self> {
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
        let download = match ManagedPythonDownload::from_request(&download_request) {
            Ok(download) => download,
            Err(downloads::Error::NoDownloadFound(request))
                if request.libc().is_some_and(Libc::is_musl) =>
            {
                return Err(anyhow::anyhow!(
                    "uv does not yet provide musl Python distributions. See https://github.com/astral-sh/uv/issues/6890 to track support."
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

/// Download and install Python versions.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn install(
    project_dir: &Path,
    install_dir: Option<PathBuf>,
    targets: Vec<String>,
    reinstall: bool,
    force: bool,
    python_install_mirror: Option<String>,
    pypy_install_mirror: Option<String>,
    default: bool,
    python_downloads: PythonDownloads,
    native_tls: bool,
    connectivity: Connectivity,
    allow_insecure_host: &[TrustedHost],
    no_config: bool,
    preview: PreviewMode,
    printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    if default && !preview.is_enabled() {
        writeln!(printer.stderr(), "The `--default` flag is only available in preview mode; add the `--preview` flag to use `--default`")?;
        return Ok(ExitStatus::Failure);
    }

    if default && targets.len() > 1 {
        anyhow::bail!("The `--default` flag cannot be used with multiple targets");
    }

    // Resolve the requests
    let mut is_default_install = false;
    let requests: Vec<_> = if targets.is_empty() {
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
            is_default_install = true;
            vec![PythonRequest::Default]
        })
        .into_iter()
        .map(InstallRequest::new)
        .collect::<Result<Vec<_>>>()?
    } else {
        targets
            .iter()
            .map(|target| PythonRequest::parse(target.as_str()))
            .map(InstallRequest::new)
            .collect::<Result<Vec<_>>>()?
    };

    let Some(first_request) = requests.first() else {
        return Ok(ExitStatus::Success);
    };

    // Read the existing installations, lock the directory for the duration
    let installations = ManagedPythonInstallations::from_settings(install_dir)?.init()?;
    let installations_dir = installations.root();
    let scratch_dir = installations.scratch();
    let _lock = installations.lock().await?;
    let existing_installations: Vec<_> = installations
        .find_all()?
        .inspect(|installation| trace!("Found existing installation {}", installation.key()))
        .collect();

    // Find requests that are already satisfied
    let mut changelog = Changelog::default();
    let (satisfied, unsatisfied): (Vec<_>, Vec<_>) = requests.iter().partition_map(|request| {
        if let Some(installation) = existing_installations
            .iter()
            .find(|installation| request.matches_installation(installation))
        {
            changelog.existing.insert(installation.key().clone());
            if reinstall {
                debug!(
                    "Ignoring match `{}` for request `{}` due to `--reinstall` flag",
                    installation.key().green(),
                    request.cyan()
                );

                Either::Right(request)
            } else {
                debug!(
                    "Found `{}` for request `{}`",
                    installation.key().green(),
                    request.cyan(),
                );

                Either::Left(installation)
            }
        } else {
            debug!("No installation found for request `{}`", request.cyan(),);

            Either::Right(request)
        }
    });

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
        .connectivity(connectivity)
        .native_tls(native_tls)
        .allow_insecure_host(allow_insecure_host.to_vec())
        .build();
    let reporter = PythonDownloadReporter::new(printer, downloads.len() as u64);
    let mut tasks = FuturesUnordered::new();
    for download in &downloads {
        tasks.push(async {
            (
                download.key(),
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
    while let Some((key, result)) = tasks.next().await {
        match result {
            Ok(download) => {
                let path = match download {
                    // We should only encounter already-available during concurrent installs
                    DownloadResult::AlreadyAvailable(path) => path,
                    DownloadResult::Fetched(path) => path,
                };

                let installation = ManagedPythonInstallation::new(path)?;
                changelog.installed.insert(installation.key().clone());
                if changelog.existing.contains(installation.key()) {
                    changelog.uninstalled.insert(installation.key().clone());
                }
                downloaded.push(installation);
            }
            Err(err) => {
                errors.push((key, anyhow::Error::new(err)));
            }
        }
    }

    let bin = if preview.is_enabled() {
        Some(python_executable_dir()?)
    } else {
        None
    };

    let installations: Vec<_> = downloaded.iter().chain(satisfied.iter().copied()).collect();

    // Ensure that the installations are _complete_ for both downloaded installations and existing
    // installations that match the request
    for installation in &installations {
        installation.ensure_externally_managed()?;
        installation.ensure_sysconfig_patched()?;
        installation.ensure_canonical_executables()?;
        installation.ensure_dylib_patched()?;

        if preview.is_disabled() {
            debug!("Skipping installation of Python executables, use `--preview` to enable.");
            continue;
        }

        let bin = bin
            .as_ref()
            .expect("We should have a bin directory with preview enabled")
            .as_path();

        let targets = if (default || is_default_install)
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
            match installation.create_bin_link(&target) {
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

                    // Check if the existing link is valid
                    let valid_link = target
                        .read_link()
                        .and_then(|target| target.try_exists())
                        .inspect_err(|err| debug!("Failed to inspect executable with error: {err}"))
                        .unwrap_or(true);

                    // Figure out what installation it references, if any
                    let existing = valid_link
                        .then(|| {
                            find_matching_bin_link(
                                installations
                                    .iter()
                                    .copied()
                                    .chain(existing_installations.iter()),
                                &target,
                            )
                        })
                        .flatten();

                    match existing {
                        None => {
                            // There's an existing executable we don't manage, require `--force`
                            if valid_link {
                                if !force {
                                    errors.push((
                                        installation.key(),
                                        anyhow::anyhow!(
                                            "Executable already exists at `{}` but is not managed by uv; use `--force` to replace it",
                                            to.simplified_display()
                                        ),
                                    ));
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
                        Some(existing) if existing == *installation => {
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
                    fs_err::remove_file(&to)?;

                    if let Some(existing) = existing {
                        // Ensure we do not report installation of this executable for an existing
                        // key if we undo it
                        changelog
                            .installed_executables
                            .entry(existing.key().clone())
                            .or_default()
                            .remove(&target);
                    }

                    installation.create_bin_link(&target)?;
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
                    errors.push((installation.key(), anyhow::Error::new(err)));
                }
            }
        }
    }

    if changelog.installed.is_empty() && errors.is_empty() {
        if is_default_install {
            writeln!(
                printer.stderr(),
                "Python is already installed. Use `uv python install <request>` to install another version.",
            )?;
        } else if requests.len() > 1 {
            writeln!(printer.stderr(), "All requested versions already installed")?;
        }
        return Ok(ExitStatus::Success);
    }

    if !changelog.installed.is_empty() {
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
            };
        }

        if preview.is_enabled() {
            let bin = bin
                .as_ref()
                .expect("We should have a bin directory with preview enabled")
                .as_path();
            warn_if_not_on_path(bin);
        }
    }

    if !errors.is_empty() {
        for (key, err) in errors
            .into_iter()
            .sorted_unstable_by(|(key_a, _), (key_b, _)| key_a.cmp(key_b))
        {
            writeln!(
                printer.stderr(),
                "{}: Failed to install {}",
                "error".red().bold(),
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
        return Ok(ExitStatus::Failure);
    }

    Ok(ExitStatus::Success)
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
                if shell.configuration_files().is_empty() {
                    warn_user!(
                        "`{}` is not on your PATH. To use the installed Python executable, run `{}`.",
                        bin.simplified_display().cyan(),
                        command.green()
                    );
                } else {
                    // TODO(zanieb): Update when we add `uv python update-shell` to match `uv tool`
                    warn_user!(
                        "`{}` is not on your PATH. To use the installed Python executable, run `{}`.",
                        bin.simplified_display().cyan(),
                        command.green(),
                    );
                }
            } else {
                warn_user!(
                    "`{}` is not on your PATH. To use the installed Python executable, add the directory to your PATH.",
                    bin.simplified_display().cyan(),
                );
            }
        } else {
            warn_user!(
                "`{}` is not on your PATH. To use the installed Python executable, add the directory to your PATH.",
                bin.simplified_display().cyan(),
            );
        }
    }
}

/// Find the [`ManagedPythonInstallation`] corresponding to an executable link installed at the
/// given path, if any.
///
/// Like [`ManagedPythonInstallation::is_bin_link`], but this method will only resolve the
/// given path one time.
fn find_matching_bin_link<'a>(
    mut installations: impl Iterator<Item = &'a ManagedPythonInstallation>,
    path: &Path,
) -> Option<&'a ManagedPythonInstallation> {
    let target = if cfg!(unix) {
        if !path.is_symlink() {
            return None;
        }
        path.read_link().ok()?
    } else if cfg!(windows) {
        let launcher = Launcher::try_from_path(path).ok()??;
        if !matches!(launcher.kind, LauncherKind::Python) {
            return None;
        }
        launcher.python_path
    } else {
        unreachable!("Only Windows and Unix are supported")
    };

    installations.find(|installation| installation.executable() == target)
}
