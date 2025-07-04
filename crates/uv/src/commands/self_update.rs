use std::fmt::{Display, Write};

use anyhow::Result;
use axoupdater::{AxoUpdater, AxoupdateError, UpdateRequest, Version};
use owo_colors::OwoColorize;
use serde::{Serialize, Serializer};
use tracing::debug;

use uv_cli::SelfUpdateFormat;
use uv_client::WrappedReqwestError;
use uv_fs::Simplified;

use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::NetworkSettings;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
struct VersionWrapper(Version);

impl Display for VersionWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.to_string())
    }
}

impl From<Version> for VersionWrapper {
    fn from(value: Version) -> VersionWrapper {
        VersionWrapper(value)
    }
}

impl Serialize for VersionWrapper {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

#[derive(Serialize)]
#[serde(tag = "result", rename_all = "kebab-case")]
enum SelfUpdateOutput {
    Offline,
    ExternallyInstalled,
    MultipleInstallations {
        current: String,
        other: String,
    },
    #[serde(rename = "github-rate-limit-exceeded")]
    GitHubRateLimitExceeded,
    OnLatest {
        version: String,
        #[serde(skip_serializing)]
        dry_run: bool,
    },
    WouldUpdate {
        from: String,
        to: String,
    },
    Updated {
        from: Option<VersionWrapper>,
        to: VersionWrapper,
        tag: String,
    },
}

impl SelfUpdateOutput {
    fn exit_status(&self) -> ExitStatus {
        match self {
            Self::Offline => ExitStatus::Failure,
            Self::ExternallyInstalled => ExitStatus::Error,
            Self::MultipleInstallations { .. } => ExitStatus::Error,
            Self::GitHubRateLimitExceeded => ExitStatus::Error,
            Self::WouldUpdate { .. } => ExitStatus::Success,
            Self::Updated { .. } => ExitStatus::Success,
            Self::OnLatest { .. } => ExitStatus::Success,
        }
    }
}

/// Attempt to update the uv binary.
pub(crate) async fn self_update(
    version: Option<String>,
    token: Option<String>,
    dry_run: bool,
    output_format: SelfUpdateFormat,
    printer: Printer,
    network_settings: NetworkSettings,
) -> Result<ExitStatus> {
    let output = self_update_impl(
        version,
        token,
        dry_run,
        output_format,
        printer,
        network_settings,
    )
    .await?;
    let exit_status = output.exit_status();

    if matches!(output_format, SelfUpdateFormat::Json) {
        writeln!(printer.stdout(), "{}", serde_json::to_string(&output)?)?;
        return Ok(exit_status);
    }

    let message = match output {
        SelfUpdateOutput::Offline => format!(
            concat!(
                "{}{} Self-update is not possible because network connectivity is disabled (i.e., with `--offline`)"
            ),
            "error".red().bold(),
            ":".bold()
        ),
        SelfUpdateOutput::ExternallyInstalled => format!(
            concat!(
                "{}{} Self-update is only available for uv binaries installed via the standalone installation scripts.",
                "\n",
                "\n",
                "If you installed uv with pip, brew, or another package manager, update uv with `pip install --upgrade`, `brew upgrade`, or similar."
            ),
            "error".red().bold(),
            ":".bold()
        ),
        SelfUpdateOutput::MultipleInstallations { current, other } => format!(
            concat!(
                "{}{} Self-update is only available for uv binaries installed via the standalone installation scripts.",
                "\n",
                "\n",
                "The current executable is at `{}` but the standalone installer was used to install uv to `{}`. Are multiple copies of uv installed?"
            ),
            "error".red().bold(),
            ":".bold(),
            current.bold().cyan(),
            other.bold().cyan()
        ),
        SelfUpdateOutput::GitHubRateLimitExceeded => format!(
            "{}{} GitHub API rate limit exceeded. Please provide a GitHub token via the {} option.",
            "error".red().bold(),
            ":".bold(),
            "`--token`".green().bold()
        ),

        SelfUpdateOutput::OnLatest { version, dry_run } => {
            if dry_run {
                format!(
                    "You're on the latest version of uv ({})",
                    format!("v{}", version).bold().white()
                )
            } else {
                format!(
                    "{}{} You're on the latest version of uv ({})",
                    "success".green().bold(),
                    ":".bold(),
                    format!("v{}", version).bold().cyan()
                )
            }
        }

        SelfUpdateOutput::WouldUpdate { from, to } => {
            let to = if to == "latest" {
                "the latest version".to_string()
            } else {
                format!("v{to}")
            };

            format!(
                "Would update uv from {} to {}",
                format!("v{from}").bold().white(),
                to.bold().white(),
            )
        }

        SelfUpdateOutput::Updated { from, to, tag } => {
            let direction = if from.as_ref().is_some_and(|from| *from > to) {
                "Downgraded"
            } else {
                "Upgraded"
            };

            let version_information = if let Some(from) = from {
                format!(
                    "from {} to {}",
                    format!("v{from}").bold().cyan(),
                    format!("v{to}").bold().cyan(),
                )
            } else {
                format!("to {}", format!("v{to}").bold().cyan())
            };

            format!(
                "{}{} {direction} uv {}! {}",
                "success".green().bold(),
                ":".bold(),
                version_information,
                format!("https://github.com/astral-sh/uv/releases/tag/{}", tag).cyan()
            )
        }
    };

    writeln!(printer.stderr(), "{}", message)?;
    Ok(exit_status)
}

async fn self_update_impl(
    version: Option<String>,
    token: Option<String>,
    dry_run: bool,
    output_format: SelfUpdateFormat,
    printer: Printer,
    network_settings: NetworkSettings,
) -> Result<SelfUpdateOutput> {
    if network_settings.connectivity.is_offline() {
        return Ok(SelfUpdateOutput::Offline);
    }

    let mut updater = AxoUpdater::new_for("uv");
    updater.disable_installer_output();

    if let Some(ref token) = token {
        updater.set_github_token(token);
    }

    // Load the "install receipt" for the current binary. If the receipt is not found, then
    // uv was likely installed via a package manager.
    let Ok(updater) = updater.load_receipt() else {
        debug!("No receipt found; assuming uv was installed via a package manager");
        return Ok(SelfUpdateOutput::ExternallyInstalled);
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

        return Ok(SelfUpdateOutput::MultipleInstallations {
            current: current_exe.simplified_display().to_string(),
            other: receipt_prefix.simplified_display().to_string(),
        });
    }

    if matches!(output_format, SelfUpdateFormat::Text) {
        writeln!(
            printer.stderr(),
            "{}",
            format_args!(
                "{}{} Checking for updates...",
                "info".cyan().bold(),
                ":".bold()
            )
        )?;
    }

    let update_request = if let Some(version) = version {
        UpdateRequest::SpecificTag(version)
    } else {
        UpdateRequest::Latest
    };

    updater.configure_version_specifier(update_request.clone());

    if dry_run {
        // TODO(charlie): `updater.fetch_release` isn't public, so we can't say what the latest
        // version is.
        return if updater.is_update_needed().await? {
            let version = match update_request {
                UpdateRequest::Latest | UpdateRequest::LatestMaybePrerelease => {
                    "latest".to_string()
                }
                UpdateRequest::SpecificTag(version) | UpdateRequest::SpecificVersion(version) => {
                    version
                }
            };

            Ok(SelfUpdateOutput::WouldUpdate {
                from: env!("CARGO_PKG_VERSION").to_string(),
                to: version,
            })
        } else {
            Ok(SelfUpdateOutput::OnLatest {
                version: env!("CARGO_PKG_VERSION").to_string(),
                dry_run: true,
            })
        };
    }

    // Run the updater. This involves a network request, since we need to determine the latest
    // available version of uv.
    match updater.run().await {
        Ok(Some(result)) => Ok(SelfUpdateOutput::Updated {
            from: result.old_version.map(VersionWrapper),
            to: result.new_version.into(),
            tag: result.new_version_tag,
        }),
        Ok(None) => Ok(SelfUpdateOutput::OnLatest {
            version: env!("CARGO_PKG_VERSION").to_string(),
            dry_run: false,
        }),
        Err(err) => match err {
            AxoupdateError::Reqwest(err) => {
                if err.status() == Some(http::StatusCode::FORBIDDEN) && token.is_none() {
                    Ok(SelfUpdateOutput::GitHubRateLimitExceeded)
                } else {
                    Err(WrappedReqwestError::from(err).into())
                }
            }
            _ => Err(err.into()),
        },
    }
}
