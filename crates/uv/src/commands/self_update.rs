use std::fmt::Write;

use anyhow::{anyhow, Result};
use axoupdater::{AxoUpdater, AxoupdateError};
use distribution_types::{IndexLocations, Resolution};
use install_wheel_rs::linker::LinkMode;
use owo_colors::OwoColorize;
use pep508_rs::Requirement;
use std::env::current_exe;
use std::str::FromStr;

use tracing::debug;
use uv_cache::Cache;
use uv_client::{BetterReqwestError, FlatIndex, RegistryClientBuilder};
use uv_dispatch::BuildDispatch;
#[cfg(windows)]
use uv_fs::{force_remove_all, rename_with_retry};
use uv_installer::{Reinstall, SitePackages};
use uv_interpreter::{find_default_python, Interpreter, PythonEnvironment};
use uv_normalize::PackageName;
use uv_resolver::{InMemoryIndex, Options};
use uv_types::{
    BuildIsolation, ConfigSettings, InFlight, NoBinary, NoBuild, SetupPyStrategy, Upgrade,
};

use crate::commands::pip_install::{install, resolve};
use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Attempt to update the `uv` binary.
pub(crate) async fn self_update(printer: Printer, cache: Cache) -> Result<ExitStatus> {
    let mut updater = AxoUpdater::new_for("uv");
    updater.disable_installer_output();

    // Load the "install receipt" for the current binary. If the receipt is not found, then
    // `uv` was likely installed via a package manager.
    let Ok(updater) = updater.load_receipt() else {
        debug!("no receipt found; assuming `uv` was installed via a package manager");

        if let Some(python) = find_default_python(&cache)
            .ok()
            .filter(|py| is_installed_on_python(py).unwrap_or_default())
        {
            return self_update_pip(python, printer, cache).await;
        };

        writeln!(
            printer.stderr(),
            "{}",
            format_args!(
                concat!(
                    "{}{} Self-update is only available for `uv` binaries installed via the standalone installation scripts.",
                    "\n",
                    "\n",
                    "If you installed `uv` with `pip`, `brew`, or another package manager, update `uv` with `pip install --upgrade`, `brew upgrade`, or similar."
                ),
                "warning".yellow().bold(),
                ":".bold()
            )
        )?;
        return Ok(ExitStatus::Error);
    };

    // Ensure the receipt is for the current binary. If it's not, then the user likely has multiple
    // `uv` binaries installed, and the current binary was _not_ installed via the standalone
    // installation scripts.
    if !updater.check_receipt_is_for_this_executable()? {
        debug!(
            "receipt is not for this executable; assuming `uv` was installed via a package manager"
        );
        writeln!(
            printer.stderr(),
            "{}",
            format_args!(
                concat!(
                    "{}{} Self-update is only available for `uv` binaries installed via the standalone installation scripts.",
                    "\n",
                    "\n",
                    "If you installed `uv` with `pip`, `brew`, or another package manager, update `uv` with `pip install --upgrade`, `brew upgrade`, or similar."
                ),
                "warning".yellow().bold(),
                ":".bold()
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

    // Run the updater. This involves a network request, since we need to determine the latest
    // available version of `uv`.
    match updater.run().await {
        Ok(Some(result)) => {
            writeln!(
                printer.stderr(),
                "{}",
                format_args!(
                    "{}{} Upgraded `uv` to {}! {}",
                    "success".green().bold(),
                    ":".bold(),
                    format!("v{}", result.new_version).bold().white(),
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
                    "{}{} You're on the latest version of `uv` ({}).",
                    "success".green().bold(),
                    ":".bold(),
                    format!("v{}", env!("CARGO_PKG_VERSION")).bold().white()
                )
            )?;
        }
        Err(err) => {
            return Err(if let AxoupdateError::Reqwest(err) = err {
                BetterReqwestError::from(err).into()
            } else {
                err.into()
            });
        }
    }

    Ok(ExitStatus::Success)
}

fn is_installed_on_python(python: &Interpreter) -> Result<bool> {
    let python_scripts_location = python.scripts();
    let current_exe = current_exe()?;
    let current_install_location = current_exe.parent().ok_or(anyhow!(
        "Cannot determine the parent directory of the current executable."
    ))?;
    Ok(current_install_location == python_scripts_location)
}

async fn self_update_pip(
    python: Interpreter,
    printer: Printer,
    cache: Cache,
) -> Result<ExitStatus> {
    #[cfg(windows)]
    let (orig, temp) = {
        let orig = current_exe()?;
        let temp = orig.with_extension("old");
        force_remove_all(&temp)?;
        rename_with_retry(&orig, &temp).await?;
        (orig, temp)
    };

    let result = {
        let venv = PythonEnvironment::from_interpreter(python);
        let site_packages = SitePackages::from_executable(&venv)?;
        let tags = venv.interpreter().tags()?;
        let markers = venv.interpreter().markers();
        let client = RegistryClientBuilder::new(cache.clone())
            // .native_tls(native_tls)
            .markers(markers)
            .platform(venv.interpreter().platform())
            .build();

        let flat_index = FlatIndex::default();
        let index = InMemoryIndex::default();
        let index_locations = IndexLocations::default();
        let settings = ConfigSettings::default();
        let in_flight = InFlight::default();

        let dispatch = BuildDispatch::new(
            &client,
            &cache,
            venv.interpreter(),
            &index_locations,
            &flat_index,
            &index,
            &in_flight,
            SetupPyStrategy::default(),
            &settings,
            BuildIsolation::Isolated,
            &NoBuild::None,
            &NoBinary::None,
        );

        let resolution: Resolution = resolve(
            vec![Requirement::from_str("uv")?],
            vec![],
            vec![],
            None,
            &[],
            &site_packages,
            &Reinstall::None,
            &Upgrade::All,
            venv.interpreter(),
            tags,
            markers,
            &client,
            &flat_index,
            &index,
            &dispatch,
            Options::default(),
            printer,
        )
        .await?
        .into();

        let package_name = PackageName::from_str("uv")?;

        let latest_version = resolution
            .get(&package_name)
            .and_then(|dist| dist.version())
            .ok_or(anyhow!("Cannot find new version on pypi."))?
            .clone();

        let newer_version_available = site_packages
            .get_packages(&package_name)
            .iter()
            .all(|installed| installed.version() < &latest_version);

        if !newer_version_available {
            writeln!(
                printer.stderr(),
                "{}",
                format_args!(
                    "{}{} You're on the latest version of `uv` ({}).",
                    "success".green().bold(),
                    ":".bold(),
                    format!("v{}", env!("CARGO_PKG_VERSION")).bold().white()
                )
            )?;
            return Ok(ExitStatus::Success);
        }

        let in_flight = InFlight::default();
        install(
            &resolution,
            vec![],
            site_packages,
            &Reinstall::None,
            &NoBinary::None,
            LinkMode::default(),
            false,
            &index_locations,
            tags,
            &client,
            &in_flight,
            &dispatch,
            &cache,
            &venv,
            false,
            printer,
        )
        .await?;

        writeln!(
            printer.stderr(),
            "{}",
            format_args!(
                "{}{} Upgraded `uv` to {}! {}",
                "success".green().bold(),
                ":".bold(),
                format!("v{latest_version}").bold().white(),
                format!("https://github.com/astral-sh/uv/releases/tag/{latest_version}").cyan()
            )
        )?;

        Ok(ExitStatus::Success)
    };

    #[cfg(windows)]
    if result.is_ok() {
        // TODO remove old binary
    } else {
        rename_with_retry(&temp, &orig).await?;
    };

    result
}
