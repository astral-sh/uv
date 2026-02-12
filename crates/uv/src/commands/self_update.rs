use std::fmt::Write;
use std::str::FromStr;

use anyhow::{Context, Result};
use axoupdater::{AxoUpdater, AxoupdateError, ReleaseSource, ReleaseSourceType, UpdateRequest};
use owo_colors::OwoColorize;
use tracing::{debug, warn};
use uv_bin_install::{Binary, find_matching_version};
use uv_client::BaseClientBuilder;
use uv_fs::Simplified;
use uv_pep440::{Version as Pep440Version, VersionSpecifier, VersionSpecifiers};
use uv_static::EnvVars;

use crate::commands::ExitStatus;
use crate::printer::Printer;

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
    updater
        .set_client(client_builder.build().raw_client().clone())
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
        let client = client_builder.retries(0).build();
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

        updater
            .configure_version_specifier(UpdateRequest::SpecificTag(resolved.version.to_string()));
        return run_updater(updater, printer, token.is_some()).await;
    }

    debug!("Using legacy self-update path");

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

    run_updater(updater, printer, token.is_some()).await
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
/// To preserve legacy tag-based behavior, only exact `major.minor.patch` release versions are
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

async fn run_updater(
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
