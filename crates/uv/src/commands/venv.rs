use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;
use std::vec;

use anstream::eprint;
use anyhow::Result;
use miette::{Diagnostic, IntoDiagnostic};
use owo_colors::OwoColorize;
use thiserror::Error;

use distribution_types::IndexLocations;
use install_wheel_rs::linker::LinkMode;
use pypi_types::Requirement;
use uv_auth::store_credentials_from_url;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    BuildOptions, Concurrency, ConfigSettings, IndexStrategy, KeyringProviderType, NoBinary,
    NoBuild, PreviewMode, SetupPyStrategy,
};
use uv_dispatch::BuildDispatch;
use uv_fs::Simplified;
use uv_git::GitResolver;
use uv_resolver::{ExcludeNewer, FlatIndex, InMemoryIndex, OptionsBuilder};
use uv_toolchain::{request_from_version_file, SystemPython, Toolchain, ToolchainRequest};
use uv_types::{BuildContext, BuildIsolation, HashStrategy, InFlight};

use crate::commands::{pip, ExitStatus};
use crate::printer::Printer;
use crate::shell::Shell;

/// Create a virtual environment.
#[allow(
    clippy::unnecessary_wraps,
    clippy::too_many_arguments,
    clippy::fn_params_excessive_bools
)]
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
    allow_existing: bool,
    exclude_newer: Option<ExcludeNewer>,
    native_tls: bool,
    preview: PreviewMode,
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
        preview,
        allow_existing,
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
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
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
    preview: PreviewMode,
    allow_existing: bool,
    exclude_newer: Option<ExcludeNewer>,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> miette::Result<ExitStatus> {
    let client_builder = BaseClientBuilder::default()
        .connectivity(connectivity)
        .native_tls(native_tls);

    let mut interpreter_request = python_request.map(ToolchainRequest::parse);
    if preview.is_enabled() && interpreter_request.is_none() {
        interpreter_request = request_from_version_file().await.into_diagnostic()?;
    }

    // Locate the Python interpreter to use in the environment
    let interpreter = Toolchain::find_or_fetch(
        interpreter_request,
        SystemPython::Required,
        preview,
        client_builder,
        cache,
    )
    .await
    .into_diagnostic()?
    .into_interpreter();

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
    let venv = uv_virtualenv::create_venv(
        path,
        interpreter,
        prompt,
        system_site_packages,
        allow_existing,
    )
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
                Some(tags),
                &HashStrategy::None,
                &BuildOptions::new(NoBinary::None, NoBuild::All),
            )
        };

        // Create a shared in-memory index.
        let index = InMemoryIndex::default();
        let git = GitResolver::default();

        // Track in-flight downloads, builds, etc., across resolutions.
        let in_flight = InFlight::default();

        // For seed packages, assume the default settings and concurrency is sufficient.
        let config_settings = ConfigSettings::default();
        let concurrency = Concurrency::default();

        // Do not allow builds
        let build_options = BuildOptions::new(NoBinary::None, NoBuild::All);

        // Prep the build context.
        let build_dispatch = BuildDispatch::new(
            &client,
            cache,
            interpreter,
            index_locations,
            &flat_index,
            &index,
            &git,
            &in_flight,
            SetupPyStrategy::default(),
            &config_settings,
            BuildIsolation::Isolated,
            link_mode,
            &build_options,
            concurrency,
            preview,
        )
        .with_options(OptionsBuilder::new().exclude_newer(exclude_newer).build());

        // Resolve the seed packages.
        let requirements = if interpreter.python_tuple() < (3, 12) {
            // Only include `setuptools` and `wheel` on Python <3.12
            vec![
                Requirement::from(pep508_rs::Requirement::from_str("pip").unwrap()),
                Requirement::from(pep508_rs::Requirement::from_str("setuptools").unwrap()),
                Requirement::from(pep508_rs::Requirement::from_str("wheel").unwrap()),
            ]
        } else {
            vec![Requirement::from(
                pep508_rs::Requirement::from_str("pip").unwrap(),
            )]
        };

        // Resolve and install the requirements.
        //
        // Since the virtual environment is empty, and the set of requirements is trivial (no
        // constraints, no editables, etc.), we can use the build dispatch APIs directly.
        let resolution = build_dispatch
            .resolve(&requirements)
            .await
            .map_err(VenvError::Seed)?;
        let installed = build_dispatch
            .install(&resolution, &venv)
            .await
            .map_err(VenvError::Seed)?;

        pip::operations::report_modifications(installed, Vec::new(), Vec::new(), printer)
            .into_diagnostic()?;
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
