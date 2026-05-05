use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, SystemTimeError};

use anyhow::{Context, Result};
use axoupdater::{
    AxoUpdater, AxoupdateError, ReleaseSource, ReleaseSourceType, UpdateRequest,
    app_name_to_env_var,
};
use owo_colors::OwoColorize;
use serde::Deserialize;
use tempfile::TempDir;
use thiserror::Error;
use tokio::process::Command;
use tracing::{debug, warn};
use url::Url;
use uv_bin_install::{Binary, find_matching_version};
use uv_client::{BaseClientBuilder, RetriableError, WrappedReqwestError, fetch_with_url_fallback};
use uv_fs::Simplified;
use uv_pep440::{Version as Pep440Version, VersionSpecifier, VersionSpecifiers};
use uv_redacted::DisplaySafeUrl;
use uv_static::{
    EnvVars, astral_mirror_base_url, astral_mirror_url_from_env, custom_astral_mirror_url,
};

use crate::commands::ExitStatus;
use crate::printer::Printer;

const UV_GITHUB_RELEASES_DOWNLOAD_PREFIX: &str =
    "https://github.com/astral-sh/uv/releases/download/";

/// The suffix appended to the Astral mirror base for uv release downloads.
const UV_MIRROR_SUFFIX: &str = "/github/uv/releases/download/";

/// Return the effective Astral mirror prefix for uv release downloads.
fn effective_uv_mirror_prefix(astral_mirror_url: Option<&str>) -> String {
    format!(
        "{}{UV_MIRROR_SUFFIX}",
        astral_mirror_base_url(astral_mirror_url)
    )
}

/// Return the `UV_DOWNLOAD_URL` value for the standalone installer when a custom Astral mirror is
/// configured.
fn installer_download_url(
    target_version: &Pep440Version,
    astral_mirror_url: Option<&str>,
) -> Option<String> {
    let mirror = custom_astral_mirror_url(astral_mirror_url)?;
    Some(format!(
        "{}{}{}",
        mirror.trim_end_matches('/'),
        UV_MIRROR_SUFFIX,
        target_version
    ))
}

const AXOUPDATER_CONFIG_PATH: &str = "AXOUPDATER_CONFIG_PATH";
const AXOUPDATER_CONFIG_WORKING_DIR: &str = "AXOUPDATER_CONFIG_WORKING_DIR";

/// Attempt to update the uv binary.
pub(crate) async fn self_update(
    version: Option<String>,
    token: Option<String>,
    dry_run: bool,
    printer: Printer,
    client_builder: BaseClientBuilder<'_>,
) -> Result<ExitStatus> {
    if client_builder.is_offline() {
        writeln!(
            printer.stderr_important(),
            "{}",
            format_args!(
                "{}{} Self-update is not possible because network connectivity is disabled (i.e., with `--offline`)",
                "error".red().bold(),
                ":".bold()
            )
        )?;
        return Ok(ExitStatus::Failure);
    }

    let mut updater = AxoUpdater::new_for("uv");
    let updater_client = client_builder.build()?;
    updater
        .set_client(updater_client.raw_client().clone())
        .disable_installer_output();

    if let Some(ref token) = token {
        updater.set_github_token(token);
    }

    // Load the "install receipt" for the current binary. If the receipt is not found, then
    // uv was likely installed via a package manager.
    let Ok(updater) = updater.load_receipt() else {
        debug!("No receipt found; assuming uv was installed via a package manager");
        writeln!(
            printer.stderr_important(),
            "{}",
            format_args!(
                concat!(
                    "{}{} Self-update is only available for uv binaries installed via the standalone installation scripts.",
                    "\n",
                    "\n",
                    "If you installed uv with pip, brew, or another package manager, update uv with `pip install --upgrade`, `brew upgrade`, or similar."
                ),
                "error".red().bold(),
                ":".bold()
            )
        )?;
        return Ok(ExitStatus::Error);
    };

    // If we know what our version is, ignore whatever the receipt thinks it is!
    // This makes us behave better if someone manually installs a random version of uv
    // in a way that doesn't update the receipt.
    if let Ok(version) = env!("CARGO_PKG_VERSION").parse() {
        // This is best-effort, it's fine if it fails (also it can't actually fail)
        let _ = updater.set_current_version(version);
    }

    // Ensure the receipt is for the current binary. If it's not, then the user likely has multiple
    // uv binaries installed, and the current binary was _not_ installed via the standalone
    // installation scripts.
    if !updater.check_receipt_is_for_this_executable()? {
        let current_exe = std::env::current_exe()?;
        let receipt_prefix = updater.install_prefix_root()?;

        writeln!(
            printer.stderr_important(),
            "{}",
            format_args!(
                concat!(
                    "{}{} Self-update is only available for uv binaries installed via the standalone installation scripts.",
                    "\n",
                    "\n",
                    "The current executable is at `{}` but the standalone installer was used to install uv to `{}`. Are multiple copies of uv installed?"
                ),
                "error".red().bold(),
                ":".bold(),
                current_exe.simplified_display().bold().cyan(),
                receipt_prefix.simplified_display().bold().cyan()
            )
        )?;
        return Ok(ExitStatus::Error);
    }

    writeln!(
        printer.stderr(),
        "{}",
        format_args!(
            "{}{} Checking for updates...",
            "info".cyan().bold(),
            ":".bold()
        )
    )?;

    if is_official_public_uv_install(updater.source.as_ref()) {
        debug!("Using official public self-update path");

        let retry_policy = client_builder.retry_policy();
        let client = client_builder.clone().retries(0).build()?;
        let constraints = official_target_version_specifiers(version.as_deref())?;

        let resolved = find_matching_version(
            Binary::Uv,
            constraints.as_ref(),
            None,
            &client,
            &retry_policy,
        )
        .await
        .with_context(|| match version.as_deref() {
            Some(version) => format!("Failed to resolve uv version `{version}`"),
            None => "Failed to resolve the latest uv version".to_string(),
        })?;

        debug!("Resolved self-update target to `uv=={}`", resolved.version);

        let current_version = Pep440Version::from_str(env!("CARGO_PKG_VERSION"))
            .context("Failed to parse the current uv version")?;
        if !is_update_needed(&current_version, &resolved.version, version.is_some()) {
            writeln!(
                printer.stderr(),
                "{}",
                format_args!(
                    "{}{} You're already on version {} of uv{}.",
                    "success".green().bold(),
                    ":".bold(),
                    format!("v{}", env!("CARGO_PKG_VERSION")).bold().cyan(),
                    if version.is_none() {
                        " (the latest version)".to_string()
                    } else {
                        String::new()
                    }
                )
            )?;
            return Ok(ExitStatus::Success);
        }

        if dry_run {
            writeln!(
                printer.stderr_important(),
                "Would update uv from {} to {}",
                format!("v{}", env!("CARGO_PKG_VERSION")).bold().white(),
                format!("v{}", resolved.version).bold().white(),
            )?;
            return Ok(ExitStatus::Success);
        }

        return run_official_updater(
            updater,
            &current_version,
            &resolved.version,
            printer,
            client_builder,
            token.as_deref(),
        )
        .await;
    }

    debug!("Using custom self-update path");

    let update_request = if let Some(version) = version {
        UpdateRequest::SpecificTag(version)
    } else {
        UpdateRequest::Latest
    };

    updater.configure_version_specifier(update_request.clone());

    if dry_run {
        // TODO(charlie): `updater.fetch_release` isn't public, so we can't say what the latest
        // version is.
        if updater.is_update_needed().await? {
            let version = match update_request {
                UpdateRequest::Latest | UpdateRequest::LatestMaybePrerelease => {
                    "the latest version".to_string()
                }
                UpdateRequest::SpecificTag(version) | UpdateRequest::SpecificVersion(version) => {
                    format!("v{version}")
                }
            };
            writeln!(
                printer.stderr_important(),
                "Would update uv from {} to {}",
                format!("v{}", env!("CARGO_PKG_VERSION")).bold().white(),
                version.bold().white(),
            )?;
        } else {
            writeln!(
                printer.stderr(),
                "{}",
                format_args!(
                    "You're on the latest version of uv ({})",
                    format!("v{}", env!("CARGO_PKG_VERSION")).bold().white()
                )
            )?;
        }
        return Ok(ExitStatus::Success);
    }

    run_custom_updater(updater, printer, token.is_some()).await
}

/// Returns `true` if the `source` is the official GitHub repository for uv, or
/// if an installer base url override environment variable is set.
fn is_official_public_uv_install(source: Option<&ReleaseSource>) -> bool {
    is_official_public_uv_install_with_overrides(
        source,
        std::env::var_os(EnvVars::UV_INSTALLER_GITHUB_BASE_URL).is_some(),
        std::env::var_os(EnvVars::UV_INSTALLER_GHE_BASE_URL).is_some(),
    )
}

/// Helper function for [`is_official_public_uv_install`] that allows for easier
/// testing.
fn is_official_public_uv_install_with_overrides(
    source: Option<&ReleaseSource>,
    has_github_base_url_override: bool,
    has_ghe_base_url_override: bool,
) -> bool {
    if has_github_base_url_override || has_ghe_base_url_override {
        return false;
    }

    matches!(
        source,
        Some(ReleaseSource {
            release_type: ReleaseSourceType::GitHub,
            owner,
            name,
            app_name,
        }) if owner == "astral-sh" && name == "uv" && app_name == "uv"
    )
}

/// Parse an explicit `uv self update` target version for the official public case.
///
/// To preserve existing tag-based behavior, only exact `major.minor.patch` release versions are
/// accepted. Inputs that normalize to a different version string, such as `0.10` or `v0.10.0`,
/// are rejected instead of being silently rewritten.
fn official_target_version_specifiers(
    target_version: Option<&str>,
) -> Result<Option<VersionSpecifiers>> {
    let Some(target_version) = target_version else {
        return Ok(None);
    };

    let pep440_version = Pep440Version::from_str(target_version)
        .with_context(|| format!("Failed to parse version specifier `{target_version}`"))?;
    if pep440_version.to_string() != target_version || pep440_version.release().len() < 3 {
        warn!(
            "Rejecting explicit self-update version specifier `{target_version}` after parsing it as `{pep440_version}`"
        );
        anyhow::bail!(
            "Failed to parse version specifier `{target_version}`: explicit versions must include an exact major.minor.patch release"
        );
    }

    Ok(Some(VersionSpecifiers::from(
        VersionSpecifier::equals_version(pep440_version),
    )))
}

fn is_update_needed(
    current_version: &Pep440Version,
    target_version: &Pep440Version,
    has_target_version: bool,
) -> bool {
    if has_target_version {
        current_version != target_version
    } else {
        current_version < target_version
    }
}

/// Given the current and target versions, fetch the installer from Astral's official release
/// artifacts and run it.
async fn run_official_updater(
    updater: &AxoUpdater,
    current_version: &Pep440Version,
    target_version: &Pep440Version,
    printer: Printer,
    client_builder: BaseClientBuilder<'_>,
    github_token: Option<&str>,
) -> Result<ExitStatus> {
    let custom_astral_mirror = astral_mirror_url_from_env();
    let installer_urls =
        official_installer_urls_with_mirror(target_version, custom_astral_mirror.as_deref())?;
    let temp_dir = TempDir::new()?;
    let installer_path = temp_dir.path().join(installer_filename());
    let install_prefix = PathBuf::from(updater.install_prefix_root()?.as_str());
    // If we can't determine the previous PATH behavior, abort rather than potentially changing the
    // user's shell configuration unexpectedly.
    let modify_path = load_receipt_modify_path("uv")
        .context("Failed to determine whether the existing standalone install modified PATH")?;

    download_installer_from_urls(
        &installer_urls,
        &installer_path,
        client_builder,
        github_token,
    )
    .await?;

    execute_official_installer(
        &installer_path,
        &install_prefix,
        modify_path,
        target_version,
        custom_astral_mirror.as_deref(),
    )
    .await?;

    let direction = if current_version > target_version {
        "Downgraded"
    } else {
        "Upgraded"
    };
    writeln!(
        printer.stderr(),
        "{}",
        format_args!(
            "{}{} {direction} uv from {} to {}! {}",
            "success".green().bold(),
            ":".bold(),
            format!("v{current_version}").bold().cyan(),
            format!("v{target_version}").bold().cyan(),
            format!("https://github.com/astral-sh/uv/releases/tag/{target_version}").cyan(),
        )
    )?;

    Ok(ExitStatus::Success)
}

/// Return the platform-specific standalone installer filename.
fn installer_filename() -> &'static str {
    if cfg!(windows) {
        "uv-installer.ps1"
    } else {
        "uv-installer.sh"
    }
}

/// Build the mirror-first URL list for the official standalone installer.
fn official_installer_urls_with_mirror(
    version: &Pep440Version,
    astral_mirror_url: Option<&str>,
) -> Result<Vec<DisplaySafeUrl>> {
    let astral_mirror_url = custom_astral_mirror_url(astral_mirror_url);
    let filename = installer_filename();
    let mirror_prefix = effective_uv_mirror_prefix(astral_mirror_url);
    let mirror = format!("{mirror_prefix}{version}/{filename}");

    let mut urls = vec![
        DisplaySafeUrl::parse(&mirror).with_context(|| format!("Failed to parse `{mirror}`"))?,
    ];

    // When using the default mirror, also fall back to the canonical GitHub URL.
    if astral_mirror_url.is_none() {
        let canonical = format!("{UV_GITHUB_RELEASES_DOWNLOAD_PREFIX}{version}/{filename}");
        urls.push(
            DisplaySafeUrl::parse(&canonical)
                .with_context(|| format!("Failed to parse `{canonical}`"))?,
        );
    }
    Ok(urls)
}

/// Download the official installer from the provided mirror/canonical URL list.
async fn download_installer_from_urls(
    urls: &[DisplaySafeUrl],
    installer_path: &Path,
    client_builder: BaseClientBuilder<'_>,
    github_token: Option<&str>,
) -> Result<()> {
    let retry_policy = client_builder.retry_policy();
    // Disable the client's built-in retries here because `fetch_with_url_fallback` already owns
    // the retry budget across the mirror-first URL list.
    let client = client_builder
        .retries(0)
        .build()
        .context("Failed to build HTTP client for self-update")?;

    fetch_with_url_fallback(urls, retry_policy, "official uv installer", |url| async {
        let mut request = client.for_host(&url).get(Url::from(url.clone()));
        if let Some(github_token) = installer_download_github_token(&url, github_token) {
            request = request.header("Authorization", format!("Bearer {github_token}"));
        }

        let response = request
            .send()
            .await
            .map_err(|source| InstallerDownloadError::Download {
                url: url.clone(),
                source: source.into(),
            })?;

        let response =
            response
                .error_for_status()
                .map_err(|source| InstallerDownloadError::Download {
                    url: url.clone(),
                    source: source.into(),
                })?;

        let bytes = response
            .bytes()
            .await
            .map_err(|source| InstallerDownloadError::Download {
                url,
                source: source.into(),
            })?;

        fs_err::tokio::write(installer_path, &bytes)
            .await
            .map_err(|source| InstallerDownloadError::Write {
                path: installer_path.to_path_buf(),
                source,
            })?;

        Ok::<(), InstallerDownloadError>(())
    })
    .await?;

    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;

        fs_err::tokio::set_permissions(installer_path, Permissions::from_mode(0o744)).await?;
    }

    Ok(())
}

fn installer_download_github_token<'a>(
    url: &DisplaySafeUrl,
    github_token: Option<&'a str>,
) -> Option<&'a str> {
    match url.host_str() {
        Some("github.com") => github_token,
        _ => None,
    }
}

/// Execute the standalone installer while preserving the existing install location and PATH
/// behavior.
///
/// When [`UV_ASTRAL_MIRROR_URL`](EnvVars::UV_ASTRAL_MIRROR_URL) is set, the installer is also
/// given `UV_DOWNLOAD_URL` pointing at the mirror's uv release directory so the installer itself
/// fetches the uv archive from the mirror.
async fn execute_official_installer(
    installer_path: &Path,
    install_prefix: &Path,
    modify_path: bool,
    target_version: &Pep440Version,
    astral_mirror_url: Option<&str>,
) -> Result<(), AxoupdateError> {
    let mut command = if cfg!(windows) {
        let mut command = Command::new("powershell");
        command.arg("-ExecutionPolicy").arg("ByPass");
        command.arg(installer_path);
        command
    } else {
        Command::new(installer_path)
    };

    let to_restore = if cfg!(windows) {
        let old_path = std::env::current_exe()?;
        let mut previous_path = old_path.as_os_str().to_os_string();
        previous_path.push(".previous.exe");
        let previous_path = PathBuf::from(previous_path);
        fs_err::rename(&old_path, &previous_path)?;
        Some((previous_path, old_path))
    } else {
        None
    };

    command.env_remove(EnvVars::PS_MODULE_PATH);
    command.env("CARGO_DIST_FORCE_INSTALL_DIR", install_prefix);
    command.env(EnvVars::UV_INSTALL_DIR, install_prefix);
    // When a custom Astral mirror is configured, point the installer at the mirrored
    // uv release directory so it downloads the archive from the mirror too.
    if let Some(download_url) = installer_download_url(target_version, astral_mirror_url) {
        command.env(EnvVars::UV_DOWNLOAD_URL, download_url);
    }
    if !modify_path {
        let app_name_env_var = app_name_to_env_var("uv");
        command.env(format!("{app_name_env_var}_NO_MODIFY_PATH"), "1");
    }

    let result = command.output().await;
    let failed = result
        .as_ref()
        .map(|output| !output.status.success())
        .unwrap_or(true);

    if let Some((previous_path, old_path)) = to_restore.as_ref() {
        if failed {
            fs_err::rename(previous_path, old_path)?;
        } else {
            #[cfg(windows)]
            self_replace::self_delete_at(previous_path)
                .map_err(|_| AxoupdateError::CleanupFailed {})?;
        }
    }

    let output = result?;
    if output.status.success() {
        return Ok(());
    }

    let stdout =
        (!output.stdout.is_empty()).then(|| String::from_utf8_lossy(&output.stdout).to_string());
    let stderr =
        (!output.stderr.is_empty()).then(|| String::from_utf8_lossy(&output.stderr).to_string());
    Err(AxoupdateError::InstallFailed {
        status: output.status.code(),
        stdout,
        stderr,
    })
}

/// Read whether the existing standalone install opted out of PATH modification.
///
/// Older receipts that lack this field default to `true` for modifying PATH.
fn load_receipt_modify_path(app_name: &str) -> Result<bool> {
    let Some(receipt_path) = find_receipt_path(app_name)? else {
        anyhow::bail!("Failed to locate the standalone install receipt for `{app_name}`");
    };

    // Axoupdater does not expose `modify_path`, so we re-read the already-validated receipt.
    let receipt = fs_err::read(&receipt_path).with_context(|| {
        format!(
            "Failed to read install receipt at `{}`",
            receipt_path.display()
        )
    })?;
    let receipt: StandaloneInstallReceipt =
        serde_json::from_slice(&receipt).with_context(|| {
            format!(
                "Failed to parse install receipt at `{}`",
                receipt_path.display()
            )
        })?;
    Ok(receipt.modify_path)
}

/// Find the receipt path for the given app name. Returns `Ok(None)` if the receipt
/// definitely doesn't exist.
fn find_receipt_path(app_name: &str) -> Result<Option<PathBuf>> {
    for prefix in receipt_prefixes(app_name)? {
        let receipt_path = prefix.join(format!("{app_name}-receipt.json"));
        if receipt_path.exists() {
            return Ok(Some(receipt_path));
        }
    }
    Ok(None)
}

/// List all possible locations for the receipt file for a given app name,
/// taking into account axoupdater-specific environment variable overrides.
fn receipt_prefixes(app_name: &str) -> Result<Vec<PathBuf>> {
    if std::env::var_os(AXOUPDATER_CONFIG_WORKING_DIR).is_some() {
        return Ok(vec![std::env::current_dir()?]);
    }

    if let Some(path) = std::env::var_os(AXOUPDATER_CONFIG_PATH) {
        return Ok(vec![PathBuf::from(path)]);
    }

    let mut prefixes = Vec::new();

    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
        let path = PathBuf::from(path).join(app_name);
        if path.exists() {
            prefixes.push(path);
        }
    }

    #[cfg(windows)]
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        prefixes.push(PathBuf::from(path).join(app_name));
    }

    #[cfg(not(windows))]
    if let Ok(path) = etcetera::home_dir() {
        prefixes.push(path.join(".config").join(app_name));
    }

    Ok(prefixes)
}

/// Runs the regular axoupdater-based update flow, printing the results to the console.
///
/// This is used when the Astral-provided official releases are disabled by the user.
/// See [`is_official_public_uv_install`] for the condition that enables this.
async fn run_custom_updater(
    updater: &mut AxoUpdater,
    printer: Printer,
    has_token: bool,
) -> Result<ExitStatus> {
    match updater.run().await {
        Ok(Some(result)) => {
            let direction = if result
                .old_version
                .as_ref()
                .is_some_and(|old_version| *old_version > result.new_version)
            {
                "Downgraded"
            } else {
                "Upgraded"
            };

            let version_information = if let Some(old_version) = result.old_version {
                format!(
                    "from {} to {}",
                    format!("v{old_version}").bold().cyan(),
                    format!("v{}", result.new_version).bold().cyan(),
                )
            } else {
                format!("to {}", format!("v{}", result.new_version).bold().cyan())
            };

            writeln!(
                printer.stderr_important(),
                "{}",
                format_args!(
                    "{}{} {direction} uv {}! {}",
                    "success".green().bold(),
                    ":".bold(),
                    version_information,
                    format!(
                        "https://github.com/astral-sh/uv/releases/tag/{}",
                        result.new_version_tag
                    )
                    .cyan()
                )
            )?;
        }
        Ok(None) => {
            writeln!(
                printer.stderr(),
                "{}",
                format_args!(
                    "{}{} You're on the latest version of uv ({})",
                    "success".green().bold(),
                    ":".bold(),
                    format!("v{}", env!("CARGO_PKG_VERSION")).bold().cyan()
                )
            )?;
        }
        Err(err) => {
            return if let AxoupdateError::Reqwest(err) = err {
                if err.status() == Some(http::StatusCode::FORBIDDEN) && !has_token {
                    writeln!(
                        printer.stderr_important(),
                        "{}",
                        format_args!(
                            "{}{} GitHub API rate limit exceeded. Please provide a GitHub token via the {} option.",
                            "error".red().bold(),
                            ":".bold(),
                            "`--token`".green().bold()
                        )
                    )?;
                    Ok(ExitStatus::Error)
                } else {
                    Err(err.into())
                }
            } else {
                Err(err.into())
            };
        }
    }

    Ok(ExitStatus::Success)
}

#[derive(Debug, Deserialize)]
struct StandaloneInstallReceipt {
    #[serde(default = "default_modify_path")]
    modify_path: bool,
}

const fn default_modify_path() -> bool {
    true
}

#[derive(Debug, Error)]
enum InstallerDownloadError {
    #[error("Failed to download installer from: {url}")]
    Download {
        url: DisplaySafeUrl,
        #[source]
        source: WrappedReqwestError,
    },

    #[error("Failed to write installer to: {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "Request failed after {retries} {subject} in {duration:.1}s",
        subject = if *retries > 1 { "retries" } else { "retry" },
        duration = duration.as_secs_f32()
    )]
    RetriedError {
        #[source]
        err: Box<Self>,
        retries: u32,
        duration: Duration,
    },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    SystemTime(#[from] SystemTimeError),
}

impl RetriableError for InstallerDownloadError {
    fn should_try_next_url(&self) -> bool {
        match self {
            Self::Download { source, .. } => should_try_next_installer_url(source),
            Self::RetriedError { err, .. } => err.should_try_next_url(),
            Self::Write { .. } | Self::Io(..) | Self::SystemTime(..) => false,
        }
    }

    fn retries(&self) -> u32 {
        if let Self::RetriedError { retries, .. } = self {
            *retries
        } else {
            0
        }
    }

    fn into_retried(self, retries: u32, duration: Duration) -> Self {
        Self::RetriedError {
            err: Box::new(self),
            retries,
            duration,
        }
    }
}

fn should_try_next_installer_url(error: &WrappedReqwestError) -> bool {
    if let Some(error) = error.inner()
        && (error.status().is_some()
            || error.is_timeout()
            || error.is_connect()
            || error.is_request()
            || error.is_body()
            || error.is_decode())
    {
        return true;
    }

    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(error) = source {
        if let Some(io_error) = error.downcast_ref::<std::io::Error>()
            && matches!(
                io_error.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::InvalidData
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::UnexpectedEof
            )
        {
            return true;
        }
        source = error.source();
    }

    false
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Sender};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use super::*;

    fn spawn_http_server(
        response: String,
    ) -> (DisplaySafeUrl, Arc<AtomicUsize>, Sender<()>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let requests_clone = Arc::clone(&requests);
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            loop {
                if shutdown_rx.try_recv().is_ok() {
                    return;
                }

                match listener.accept() {
                    Ok((mut stream, _)) => {
                        requests_clone.fetch_add(1, Ordering::SeqCst);
                        let mut buf = [0u8; 4096];
                        let _ = stream.read(&mut buf);
                        stream.write_all(response.as_bytes()).unwrap();
                        return;
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(err) => panic!("failed to accept connection: {err}"),
                }
            }
        });
        (
            DisplaySafeUrl::parse(&format!("http://{addr}/uv-installer.sh")).unwrap(),
            requests,
            shutdown_tx,
            handle,
        )
    }

    fn installer_response(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n{body}",
            body.len()
        )
    }

    fn not_found_response() -> String {
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string()
    }

    #[test]
    fn test_is_official_public_uv_install() {
        let source = ReleaseSource {
            release_type: ReleaseSourceType::GitHub,
            owner: "astral-sh".to_string(),
            name: "uv".to_string(),
            app_name: "uv".to_string(),
        };

        assert!(!is_official_public_uv_install_with_overrides(
            None, false, false,
        ));
        assert!(is_official_public_uv_install_with_overrides(
            Some(&source),
            false,
            false,
        ));
        assert!(!is_official_public_uv_install_with_overrides(
            Some(&source),
            true,
            false,
        ));
        assert!(!is_official_public_uv_install_with_overrides(
            Some(&source),
            false,
            true,
        ));

        let source = ReleaseSource {
            owner: "astral-sh".to_string(),
            name: "ruff".to_string(),
            app_name: "uv".to_string(),
            ..source
        };
        assert!(!is_official_public_uv_install_with_overrides(
            Some(&source),
            false,
            false,
        ));
    }

    #[test]
    fn test_official_target_version_specifiers() {
        assert_eq!(official_target_version_specifiers(None).unwrap(), None);
        assert_eq!(
            official_target_version_specifiers(Some("1.2.3")).unwrap(),
            Some(VersionSpecifiers::from(VersionSpecifier::equals_version(
                Pep440Version::new([1, 2, 3]),
            )))
        );
        assert!(official_target_version_specifiers(Some("0.10")).is_err());
        assert!(official_target_version_specifiers(Some("v1.2.3")).is_err());
    }

    #[test]
    fn test_official_update_needed() {
        assert!(!is_update_needed(
            &Pep440Version::new([1, 2, 3]),
            &Pep440Version::new([1, 2, 3]),
            false,
        ));
        assert!(is_update_needed(
            &Pep440Version::new([1, 2, 3]),
            &Pep440Version::new([1, 2, 4]),
            false,
        ));
        assert!(!is_update_needed(
            &Pep440Version::new([1, 2, 4]),
            &Pep440Version::new([1, 2, 3]),
            false,
        ));
        assert!(!is_update_needed(
            &Pep440Version::new([1, 2, 3]),
            &Pep440Version::new([1, 2, 3]),
            true,
        ));
        assert!(is_update_needed(
            &Pep440Version::new([1, 2, 4]),
            &Pep440Version::new([1, 2, 3]),
            true,
        ));
    }

    #[test]
    fn test_official_installer_urls() {
        let urls = official_installer_urls_with_mirror(&Pep440Version::new([1, 2, 3]), None)
            .unwrap()
            .into_iter()
            .map(|url| url.to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            urls,
            vec![
                format!(
                    "https://releases.astral.sh/github/uv/releases/download/1.2.3/{}",
                    installer_filename()
                ),
                format!(
                    "https://github.com/astral-sh/uv/releases/download/1.2.3/{}",
                    installer_filename()
                ),
            ]
        );
    }

    #[test]
    fn test_official_installer_urls_custom_astral_mirror() {
        let urls = official_installer_urls_with_mirror(
            &Pep440Version::new([1, 2, 3]),
            Some("https://nexus.example.com/repository/releases.astral.sh/"),
        )
        .unwrap()
        .into_iter()
        .map(|url| url.to_string())
        .collect::<Vec<_>>();
        assert_eq!(
            urls,
            vec![format!(
                "https://nexus.example.com/repository/releases.astral.sh/github/uv/releases/download/1.2.3/{}",
                installer_filename()
            )]
        );
    }

    #[test]
    fn test_official_installer_urls_empty_astral_mirror_uses_default() {
        let default_urls =
            official_installer_urls_with_mirror(&Pep440Version::new([1, 2, 3]), None).unwrap();
        let empty_urls =
            official_installer_urls_with_mirror(&Pep440Version::new([1, 2, 3]), Some("")).unwrap();
        assert_eq!(default_urls, empty_urls);
    }

    #[test]
    fn test_installer_download_url_custom_astral_mirror() {
        assert_eq!(
            installer_download_url(
                &Pep440Version::new([1, 2, 3]),
                Some("https://nexus.example.com/repository/releases.astral.sh/")
            )
            .as_deref(),
            Some(
                "https://nexus.example.com/repository/releases.astral.sh/github/uv/releases/download/1.2.3"
            )
        );
    }

    #[test]
    fn test_installer_download_url_empty_astral_mirror_uses_default() {
        assert_eq!(
            installer_download_url(&Pep440Version::new([1, 2, 3]), Some("")),
            None
        );
    }

    #[test]
    fn test_installer_download_github_token() {
        let mirror = DisplaySafeUrl::parse(
            "https://releases.astral.sh/github/uv/releases/download/1.2.3/uv-installer.sh",
        )
        .unwrap();
        let github = DisplaySafeUrl::parse(
            "https://github.com/astral-sh/uv/releases/download/1.2.3/uv-installer.sh",
        )
        .unwrap();

        assert_eq!(
            installer_download_github_token(&mirror, Some("token")),
            None
        );
        assert_eq!(
            installer_download_github_token(&github, Some("token")),
            Some("token")
        );
        assert_eq!(installer_download_github_token(&github, None), None);
    }

    #[tokio::test]
    async fn test_download_installer_falls_back_to_canonical_url() {
        let (mirror_url, mirror_requests, mirror_shutdown, mirror_handle) =
            spawn_http_server(not_found_response());
        let (canonical_url, canonical_requests, canonical_shutdown, canonical_handle) =
            spawn_http_server(installer_response("echo canonical installer\n"));
        let temp_dir = TempDir::new().unwrap();
        let installer_path = temp_dir.path().join("installer.sh");

        download_installer_from_urls(
            &[mirror_url, canonical_url],
            &installer_path,
            BaseClientBuilder::default(),
            None,
        )
        .await
        .expect("404 from mirror should fall back to canonical installer URL");

        let _ = mirror_shutdown.send(());
        let _ = canonical_shutdown.send(());
        mirror_handle.join().unwrap();
        canonical_handle.join().unwrap();

        assert_eq!(mirror_requests.load(Ordering::SeqCst), 1);
        assert_eq!(canonical_requests.load(Ordering::SeqCst), 1);
        assert_eq!(
            fs_err::read_to_string(&installer_path).unwrap(),
            "echo canonical installer\n"
        );
    }

    #[test]
    fn test_standalone_install_receipt_defaults_modify_path_to_true() {
        let receipt: StandaloneInstallReceipt =
            serde_json::from_str("{}\n").expect("receipt without modify_path should parse");
        assert!(receipt.modify_path);

        let receipt: StandaloneInstallReceipt = serde_json::from_str("{\"modify_path\":false}\n")
            .expect("receipt with explicit modify_path should parse");
        assert!(!receipt.modify_path);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_download_installer_sets_executable_bit() {
        use std::os::unix::fs::PermissionsExt;

        let (url, _requests, shutdown_tx, handle) =
            spawn_http_server(installer_response("echo installer\n"));
        let temp_dir = TempDir::new().unwrap();
        let installer_path = temp_dir.path().join("installer.sh");

        download_installer_from_urls(&[url], &installer_path, BaseClientBuilder::default(), None)
            .await
            .expect("installer download should succeed");

        let _ = shutdown_tx.send(());
        handle.join().unwrap();

        let mode = fs_err::metadata(&installer_path)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o100, 0o100, "installer should be owner-executable");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_execute_official_installer_reports_failure() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let installer_path = temp_dir.path().join("installer.sh");
        let install_prefix = temp_dir.path().join("install-prefix");

        fs_err::write(
            &installer_path,
            "#!/bin/sh\nprintf 'hello from stdout\\n'\nprintf 'hello from stderr\\n' >&2\nexit 23\n",
        )
        .unwrap();
        fs_err::set_permissions(&installer_path, std::fs::Permissions::from_mode(0o744)).unwrap();

        let err = execute_official_installer(
            &installer_path,
            &install_prefix,
            true,
            &Pep440Version::new([1, 2, 3]),
            None,
        )
        .await
        .expect_err("failing installer should return an error");
        let AxoupdateError::InstallFailed {
            status,
            stdout,
            stderr,
        } = err
        else {
            panic!("expected InstallFailed error");
        };

        assert_eq!(status, Some(23));
        assert_eq!(stdout.as_deref(), Some("hello from stdout\n"));
        assert_eq!(stderr.as_deref(), Some("hello from stderr\n"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_execute_official_installer_sets_install_env_vars() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("env.txt");
        let installer_path = temp_dir.path().join("installer.sh");
        let install_prefix = temp_dir.path().join("install-prefix");

        fs_err::write(
            &installer_path,
            format!(
                "#!/bin/sh\nset -eu\n{{\nprintf 'CARGO_DIST_FORCE_INSTALL_DIR=%s\\n' \"$CARGO_DIST_FORCE_INSTALL_DIR\"\nprintf 'UV_INSTALL_DIR=%s\\n' \"$UV_INSTALL_DIR\"\nprintf 'UV_NO_MODIFY_PATH=%s\\n' \"${{UV_NO_MODIFY_PATH-}}\"\n}} > \"{}\"\n",
                output_path.display()
            ),
        )
        .unwrap();
        fs_err::set_permissions(&installer_path, std::fs::Permissions::from_mode(0o744)).unwrap();

        execute_official_installer(
            &installer_path,
            &install_prefix,
            false,
            &Pep440Version::new([1, 2, 3]),
            None,
        )
        .await
        .unwrap();

        assert_eq!(
            fs_err::read_to_string(&output_path).unwrap(),
            format!(
                "CARGO_DIST_FORCE_INSTALL_DIR={}\nUV_INSTALL_DIR={}\nUV_NO_MODIFY_PATH=1\n",
                install_prefix.display(),
                install_prefix.display(),
            )
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_execute_official_installer_sets_download_url_for_astral_mirror() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("env.txt");
        let installer_path = temp_dir.path().join("installer.sh");
        let install_prefix = temp_dir.path().join("install-prefix");

        fs_err::write(
            &installer_path,
            format!(
                "#!/bin/sh\nset -eu\n{{\nprintf 'UV_DOWNLOAD_URL=%s\\n' \"${{UV_DOWNLOAD_URL-}}\"\n}} > \"{}\"\n",
                output_path.display()
            ),
        )
        .unwrap();
        fs_err::set_permissions(&installer_path, std::fs::Permissions::from_mode(0o744)).unwrap();

        execute_official_installer(
            &installer_path,
            &install_prefix,
            true,
            &Pep440Version::new([1, 2, 3]),
            Some("https://nexus.example.com/repository/releases.astral.sh/"),
        )
        .await
        .unwrap();

        assert_eq!(
            fs_err::read_to_string(&output_path).unwrap(),
            "UV_DOWNLOAD_URL=https://nexus.example.com/repository/releases.astral.sh/github/uv/releases/download/1.2.3\n"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_execute_official_installer_preserves_modify_path_default() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("env.txt");
        let installer_path = temp_dir.path().join("installer.sh");
        let install_prefix = temp_dir.path().join("install-prefix");

        fs_err::write(
            &installer_path,
            format!(
                "#!/bin/sh\nset -eu\n{{\nprintf 'UV_NO_MODIFY_PATH=%s\\n' \"${{UV_NO_MODIFY_PATH-}}\"\n}} > \"{}\"\n",
                output_path.display()
            ),
        )
        .unwrap();
        fs_err::set_permissions(&installer_path, std::fs::Permissions::from_mode(0o744)).unwrap();

        execute_official_installer(
            &installer_path,
            &install_prefix,
            true,
            &Pep440Version::new([1, 2, 3]),
            None,
        )
        .await
        .unwrap();

        assert_eq!(
            fs_err::read_to_string(&output_path).unwrap(),
            "UV_NO_MODIFY_PATH=\n"
        );
    }
}
