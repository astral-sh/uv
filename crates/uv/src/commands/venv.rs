use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;
use std::vec;

use anstream::eprint;
use anyhow::Result;
use chrono::{DateTime, Utc};
use itertools::Itertools;
use miette::{Diagnostic, IntoDiagnostic};
use owo_colors::OwoColorize;
use thiserror::Error;

use distribution_types::{DistributionMetadata, IndexLocations, Name};
use pep508_rs::Requirement;
use platform_host::Platform;
use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndex, FlatIndexClient, RegistryClientBuilder};
use uv_dispatch::BuildDispatch;
use uv_fs::Simplified;
use uv_installer::NoBinary;
use uv_interpreter::{find_default_python, find_requested_python, Error};
use uv_resolver::{InMemoryIndex, OptionsBuilder};
use uv_traits::{BuildContext, BuildIsolation, ConfigSettings, InFlight, NoBuild, SetupPyStrategy};

use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::shell::Shell;

/// Create a virtual environment.
#[allow(clippy::unnecessary_wraps, clippy::too_many_arguments)]
pub(crate) async fn venv(
    path: &Path,
    python_request: Option<&str>,
    index_locations: &IndexLocations,
    prompt: uv_virtualenv::Prompt,
    system_site_packages: bool,
    connectivity: Connectivity,
    seed: bool,
    exclude_newer: Option<DateTime<Utc>>,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    match venv_impl(
        path,
        python_request,
        index_locations,
        prompt,
        system_site_packages,
        connectivity,
        seed,
        exclude_newer,
        cache,
        printer,
    )
    .await
    {
        Ok(status) => Ok(status),
        Err(err) => {
            eprint!("{err:?}");
            Ok(ExitStatus::Failure)
        }
    }
}

#[derive(Error, Debug, Diagnostic)]
enum VenvError {
    #[error("Failed to create virtualenv")]
    #[diagnostic(code(uv::venv::creation))]
    Creation(#[source] uv_virtualenv::Error),

    #[error("Failed to install seed packages")]
    #[diagnostic(code(uv::venv::seed))]
    Seed(#[source] anyhow::Error),

    #[error("Failed to extract interpreter tags")]
    #[diagnostic(code(uv::venv::tags))]
    Tags(#[source] platform_tags::TagsError),

    #[error("Failed to resolve `--find-links` entry")]
    #[diagnostic(code(uv::venv::flat_index))]
    FlatIndex(#[source] uv_client::FlatIndexError),
}

/// Create a virtual environment.
#[allow(clippy::too_many_arguments)]
async fn venv_impl(
    path: &Path,
    python_request: Option<&str>,
    index_locations: &IndexLocations,
    prompt: uv_virtualenv::Prompt,
    system_site_packages: bool,
    connectivity: Connectivity,
    seed: bool,
    exclude_newer: Option<DateTime<Utc>>,
    cache: &Cache,
    printer: Printer,
) -> miette::Result<ExitStatus> {
    // Locate the Python interpreter.
    let platform = Platform::current().into_diagnostic()?;
    let interpreter = if let Some(python_request) = python_request {
        find_requested_python(python_request, &platform, cache)
            .into_diagnostic()?
            .ok_or(Error::NoSuchPython(python_request.to_string()))
            .into_diagnostic()?
    } else {
        find_default_python(&platform, cache).into_diagnostic()?
    };

    writeln!(
        printer.stderr(),
        "Using Python {} interpreter at: {}",
        interpreter.python_version(),
        interpreter.sys_executable().simplified_display().cyan()
    )
    .into_diagnostic()?;

    writeln!(
        printer.stderr(),
        "Creating virtualenv at: {}",
        path.simplified_display().cyan()
    )
    .into_diagnostic()?;

    // Extra cfg for pyvenv.cfg to specify uv version
    let extra_cfg = vec![("uv".to_string(), env!("CARGO_PKG_VERSION").to_string())];

    // Create the virtual environment.
    let venv =
        uv_virtualenv::create_venv(path, interpreter, prompt, system_site_packages, extra_cfg)
            .map_err(VenvError::Creation)?;

    // Install seed packages.
    if seed {
        // Extract the interpreter.
        let interpreter = venv.interpreter();

        // Instantiate a client.
        let client = RegistryClientBuilder::new(cache.clone())
            .index_urls(index_locations.index_urls())
            .connectivity(connectivity)
            .build();

        // Resolve the flat indexes from `--find-links`.
        let flat_index = {
            let tags = interpreter.tags().map_err(VenvError::Tags)?;
            let client = FlatIndexClient::new(&client, cache);
            let entries = client
                .fetch(index_locations.flat_index())
                .await
                .map_err(VenvError::FlatIndex)?;
            FlatIndex::from_entries(entries, tags)
        };

        // Create a shared in-memory index.
        let index = InMemoryIndex::default();

        // Track in-flight downloads, builds, etc., across resolutions.
        let in_flight = InFlight::default();

        // For seed packages, assume the default settings are sufficient.
        let config_settings = ConfigSettings::default();

        // Prep the build context.
        let build_dispatch = BuildDispatch::new(
            &client,
            cache,
            interpreter,
            index_locations,
            &flat_index,
            &index,
            &in_flight,
            SetupPyStrategy::default(),
            &config_settings,
            BuildIsolation::Isolated,
            &NoBuild::All,
            &NoBinary::None,
        )
        .with_options(OptionsBuilder::new().exclude_newer(exclude_newer).build());

        // Resolve the seed packages.
        let mut requirements = vec![Requirement::from_str("pip").unwrap()];

        // Only include `setuptools` and `wheel` on Python <3.12
        if interpreter.python_tuple() < (3, 12) {
            requirements.push(Requirement::from_str("setuptools").unwrap());
            requirements.push(Requirement::from_str("wheel").unwrap());
        }
        let resolution = build_dispatch
            .resolve(&requirements)
            .await
            .map_err(VenvError::Seed)?;

        // Install into the environment.
        build_dispatch
            .install(&resolution, &venv)
            .await
            .map_err(VenvError::Seed)?;

        for distribution in resolution
            .distributions()
            .sorted_unstable_by(|a, b| a.name().cmp(b.name()).then(a.version().cmp(&b.version())))
        {
            writeln!(
                printer.stderr(),
                " {} {}{}",
                "+".green(),
                distribution.name().as_ref().bold(),
                distribution.version_or_url().dimmed()
            )
            .into_diagnostic()?;
        }
    }

    // Determine the appropriate activation command.
    let activation = match Shell::from_env() {
        None => None,
        Some(Shell::Bash | Shell::Zsh) => Some(format!(
            "source {}",
            shlex_posix(path.join("bin").join("activate"))
        )),
        Some(Shell::Fish) => Some(format!(
            "source {}",
            shlex_posix(path.join("bin").join("activate.fish"))
        )),
        Some(Shell::Nushell) => Some(format!(
            "overlay use {}",
            shlex_posix(path.join("bin").join("activate.nu"))
        )),
        Some(Shell::Csh) => Some(format!(
            "source {}",
            shlex_posix(path.join("bin").join("activate.csh"))
        )),
        Some(Shell::Powershell) => Some(shlex_windows(
            path.join("Scripts").join("activate"),
            Shell::Powershell,
        )),
        Some(Shell::Cmd) => Some(shlex_windows(
            path.join("Scripts").join("activate"),
            Shell::Cmd,
        )),
    };
    if let Some(act) = activation {
        writeln!(printer.stderr(), "Activate with: {}", act.green()).into_diagnostic()?;
    }

    Ok(ExitStatus::Success)
}

/// Quote a path, if necessary, for safe use in a POSIX-compatible shell command.
fn shlex_posix(executable: impl AsRef<Path>) -> String {
    // Convert to a display path.
    let executable = executable.as_ref().simplified_display().to_string();

    // Like Python's `shlex.quote`:
    // > Use single quotes, and put single quotes into double quotes
    // > The string $'b is then quoted as '$'"'"'b'
    if executable.contains(' ') {
        format!("'{}'", executable.replace('\'', r#"'"'"'"#))
    } else {
        executable
    }
}

/// Quote a path, if necessary, for safe use in `PowerShell` and `cmd`.
fn shlex_windows(executable: impl AsRef<Path>, shell: Shell) -> String {
    // Convert to a display path.
    let executable = executable.as_ref().simplified_display().to_string();

    // Wrap the executable in quotes (and a `&` invocation on PowerShell), if it contains spaces.
    if executable.contains(' ') {
        if shell == Shell::Powershell {
            // For PowerShell, wrap in a `&` invocation.
            format!("& \"{executable}\"")
        } else {
            // Otherwise, assume `cmd`, which doesn't need the `&`.
            format!("\"{executable}\"")
        }
    } else {
        executable
    }
}
