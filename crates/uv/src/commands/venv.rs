use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;
use std::vec;

use anstream::eprint;
use anyhow::Result;
use itertools::Itertools;
use miette::{Diagnostic, IntoDiagnostic};
use owo_colors::OwoColorize;
use thiserror::Error;

use distribution_types::{DistributionMetadata, IndexLocations, Name, ResolvedDist, UvRequirement};
use install_wheel_rs::linker::LinkMode;
use pep508_rs::Requirement;
use uv_auth::store_credentials_from_url;
use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::KeyringProviderType;
use uv_configuration::{ConfigSettings, IndexStrategy, NoBinary, NoBuild, SetupPyStrategy};
use uv_dispatch::BuildDispatch;
use uv_fs::Simplified;
use uv_interpreter::{find_default_python, find_requested_python, Error};
use uv_resolver::{ExcludeNewer, FlatIndex, InMemoryIndex, OptionsBuilder};
use uv_types::{BuildContext, BuildIsolation, HashStrategy, InFlight};

use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::shell::Shell;

/// Create a virtual environment.
#[allow(clippy::unnecessary_wraps, clippy::too_many_arguments)]
pub(crate) async fn venv(
    path: &Path,
    python_request: Option<&str>,
    link_mode: LinkMode,
    index_locations: &IndexLocations,
    index_strategy: IndexStrategy,
    keyring_provider: KeyringProviderType,
    prompt: uv_virtualenv::Prompt,
    system_site_packages: bool,
    connectivity: Connectivity,
    seed: bool,
    exclude_newer: Option<ExcludeNewer>,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    match venv_impl(
        path,
        python_request,
        link_mode,
        index_locations,
        index_strategy,
        keyring_provider,
        prompt,
        system_site_packages,
        connectivity,
        seed,
        exclude_newer,
        native_tls,
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
    link_mode: LinkMode,
    index_locations: &IndexLocations,
    index_strategy: IndexStrategy,
    keyring_provider: KeyringProviderType,
    prompt: uv_virtualenv::Prompt,
    system_site_packages: bool,
    connectivity: Connectivity,
    seed: bool,
    exclude_newer: Option<ExcludeNewer>,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> miette::Result<ExitStatus> {
    // Locate the Python interpreter.
    let interpreter = if let Some(python_request) = python_request {
        find_requested_python(python_request, cache)
            .into_diagnostic()?
            .ok_or(Error::NoSuchPython(python_request.to_string()))
            .into_diagnostic()?
    } else {
        find_default_python(cache).into_diagnostic()?
    };

    // Add all authenticated sources to the cache.
    for url in index_locations.urls() {
        store_credentials_from_url(url);
    }

    writeln!(
        printer.stderr(),
        "Using Python {} interpreter at: {}",
        interpreter.python_version(),
        interpreter.sys_executable().user_display().cyan()
    )
    .into_diagnostic()?;

    writeln!(
        printer.stderr(),
        "Creating virtualenv at: {}",
        path.user_display().cyan()
    )
    .into_diagnostic()?;

    // Create the virtual environment.
    let venv = uv_virtualenv::create_venv(path, interpreter, prompt, system_site_packages)
        .map_err(VenvError::Creation)?;

    // Install seed packages.
    if seed {
        // Extract the interpreter.
        let interpreter = venv.interpreter();

        // Instantiate a client.
        let client = RegistryClientBuilder::new(cache.clone())
            .native_tls(native_tls)
            .index_urls(index_locations.index_urls())
            .index_strategy(index_strategy)
            .keyring(keyring_provider)
            .connectivity(connectivity)
            .markers(interpreter.markers())
            .platform(interpreter.platform())
            .build();

        // Resolve the flat indexes from `--find-links`.
        let flat_index = {
            let tags = interpreter.tags().map_err(VenvError::Tags)?;
            let client = FlatIndexClient::new(&client, cache);
            let entries = client
                .fetch(index_locations.flat_index())
                .await
                .map_err(VenvError::FlatIndex)?;
            FlatIndex::from_entries(
                entries,
                tags,
                &HashStrategy::None,
                &NoBuild::All,
                &NoBinary::None,
            )
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
            link_mode,
            &NoBuild::All,
            &NoBinary::None,
        )
        .with_options(OptionsBuilder::new().exclude_newer(exclude_newer).build());

        // Resolve the seed packages.
        let mut requirements =
            vec![UvRequirement::from_requirement(Requirement::from_str("pip").unwrap()).unwrap()];

        // Only include `setuptools` and `wheel` on Python <3.12
        if interpreter.python_tuple() < (3, 12) {
            requirements.push(
                UvRequirement::from_requirement(Requirement::from_str("setuptools").unwrap())
                    .unwrap(),
            );
            requirements.push(
                UvRequirement::from_requirement(Requirement::from_str("wheel").unwrap()).unwrap(),
            );
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
            .filter_map(|dist| match dist {
                ResolvedDist::Installable(dist) => Some(dist),
                ResolvedDist::Installed(_) => None,
            })
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
            shlex_posix(venv.scripts().join("activate"))
        )),
        Some(Shell::Fish) => Some(format!(
            "source {}",
            shlex_posix(venv.scripts().join("activate.fish"))
        )),
        Some(Shell::Nushell) => Some(format!(
            "overlay use {}",
            shlex_posix(venv.scripts().join("activate.nu"))
        )),
        Some(Shell::Csh) => Some(format!(
            "source {}",
            shlex_posix(venv.scripts().join("activate.csh"))
        )),
        Some(Shell::Powershell) => Some(shlex_windows(
            venv.scripts().join("activate"),
            Shell::Powershell,
        )),
        Some(Shell::Cmd) => Some(shlex_windows(venv.scripts().join("activate"), Shell::Cmd)),
    };
    if let Some(act) = activation {
        writeln!(printer.stderr(), "Activate with: {}", act.green()).into_diagnostic()?;
    }

    Ok(ExitStatus::Success)
}

/// Quote a path, if necessary, for safe use in a POSIX-compatible shell command.
fn shlex_posix(executable: impl AsRef<Path>) -> String {
    // Convert to a display path.
    let executable = executable.as_ref().user_display().to_string();

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
    let executable = executable.as_ref().user_display().to_string();

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
