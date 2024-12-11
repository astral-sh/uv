use std::fmt::Write;

use anyhow::Result;
use futures::StreamExt;
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use tracing::warn;

use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_client::{Connectivity, RegistryClientBuilder};
use uv_configuration::{Concurrency, TrustedHost};
use uv_distribution_filename::DistFilename;
use uv_distribution_types::IndexCapabilities;
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_python::{
    EnvironmentPreference, PythonEnvironment, PythonInstallation, PythonPreference, PythonRequest,
    PythonVariant, VersionRequest,
};
use uv_resolver::RequiresPython;
use uv_settings::ResolverInstallerOptions;
use uv_tool::{InstalledTools, Tool};
use uv_warnings::warn_user;

use crate::commands::pip::latest::LatestClient;
use crate::commands::reporters::LatestVersionReporter;
use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// List installed tools.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn list(
    show_paths: bool,
    show_version_specifiers: bool,
    outdated: bool,
    python_preference: PythonPreference,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let installed_tools = InstalledTools::from_settings()?;
    let _lock = match installed_tools.lock().await {
        Ok(lock) => lock,
        Err(uv_tool::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
            writeln!(printer.stderr(), "No tools installed")?;
            return Ok(ExitStatus::Success);
        }
        Err(err) => return Err(err.into()),
    };

    let mut tools = installed_tools.tools()?.into_iter().collect::<Vec<_>>();
    tools.sort_by_key(|(name, _)| name.clone());

    if tools.is_empty() {
        writeln!(printer.stderr(), "No tools installed")?;
        return Ok(ExitStatus::Success);
    }

    // Get the versions of the installed tools.
    let versions = tools
        .iter()
        .map(|(name, _)| {
            let version = installed_tools.version(name, cache);
            (name.clone(), version)
        })
        .collect::<FxHashMap<_, _>>();

    let latest = if outdated {
        let reporter = LatestVersionReporter::from(printer).with_length(tools.len() as u64);

        // Filter out malformed tools.
        let tools = tools
            .iter()
            .filter_map(|(name, tool)| {
                if let Ok(ref tool) = tool {
                    if versions[name].is_ok() {
                        return Some((name, tool));
                    }
                };
                None
            })
            .collect_vec();

        // Fetch the latest version for each tool.
        let mut fetches = futures::stream::iter(tools)
            .map(|(name, tool)| {
                // SAFETY: The tool is known to be well-formed, get_environment will not fail.
                let environment = installed_tools
                    .get_environment(name, cache)
                    .unwrap()
                    .unwrap();
                async move {
                    let latest = find_latest(
                        name,
                        tool,
                        &environment,
                        python_preference,
                        connectivity,
                        native_tls,
                        allow_insecure_host,
                        cache,
                    )
                    .await?;
                    anyhow::Ok((name, latest))
                }
            })
            .buffer_unordered(concurrency.downloads);

        let mut map = FxHashMap::default();
        while let Some((package, version)) = fetches.next().await.transpose()? {
            if let Some(version) = version.as_ref() {
                reporter.on_fetch_version(package, version.version());
                map.insert(package.clone(), version.clone());
            } else {
                reporter.on_fetch_progress();
            }
        }
        reporter.on_fetch_complete();
        map
    } else {
        FxHashMap::default()
    };

    // Remove tools that are up-to-date.
    let tools = if outdated {
        tools
            .into_iter()
            .filter(|(name, tool)| {
                if tool.is_err() {
                    return true;
                }
                let Ok(version) = versions[name].as_ref() else {
                    return true;
                };
                latest
                    .get(name)
                    .map_or(true, |filename| filename.version() > version)
            })
            .collect_vec()
    } else {
        tools
    };

    for (name, tool) in tools {
        // Skip invalid tools
        let Ok(tool) = tool else {
            warn_user!(
                "Ignoring malformed tool `{name}` (run `{}` to remove)",
                format!("uv tool uninstall {name}").green()
            );
            continue;
        };

        // Output tool name and version
        let version = match &versions[&name] {
            Ok(version) => version,
            Err(e) => {
                writeln!(printer.stderr(), "{e}")?;
                continue;
            }
        };

        let version_specifier = if show_version_specifiers {
            let specifiers = tool
                .requirements()
                .iter()
                .filter(|req| req.name == name)
                .map(|req| req.source.to_string())
                .filter(|s| !s.is_empty())
                .join(", ");
            if specifiers.is_empty() {
                String::new()
            } else {
                format!(" [required: {specifiers}]")
            }
        } else {
            String::new()
        };

        let latest = if outdated {
            let latest = latest.get(&name).map(DistFilename::version);
            if let Some(latest) = latest {
                format!(" (latest: v{latest})")
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        if show_paths {
            writeln!(
                printer.stdout(),
                "{}{}{}",
                format!("{name} v{version}{version_specifier}").bold(),
                latest.bold().cyan(),
                format!(
                    " ({})",
                    installed_tools.tool_dir(&name).simplified_display()
                )
                .cyan(),
            )?;
        } else {
            writeln!(
                printer.stdout(),
                "{}{}",
                format!("{name} v{version}{version_specifier}").bold(),
                latest.bold().cyan(),
            )?;
        }

        // Output tool entrypoints
        for entrypoint in tool.entrypoints() {
            if show_paths {
                writeln!(
                    printer.stdout(),
                    "- {} ({})",
                    entrypoint.name,
                    entrypoint.install_path.simplified_display().cyan()
                )?;
            } else {
                writeln!(printer.stdout(), "- {}", entrypoint.name)?;
            }
        }
    }

    Ok(ExitStatus::Success)
}

/// Find the latest version of a tool.
async fn find_latest(
    name: &PackageName,
    tool: &Tool,
    environment: &PythonEnvironment,
    python_preference: PythonPreference,
    connectivity: Connectivity,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
) -> Result<Option<DistFilename>> {
    let capabilities = IndexCapabilities::default();

    let ResolverInstallerSettings {
        index_locations,
        index_strategy,
        keyring_provider,
        prerelease,
        exclude_newer,
        ..
    } = ResolverInstallerOptions::from(tool.options().clone()).into();

    // Initialize the registry client.
    let client =
        RegistryClientBuilder::new(cache.clone().with_refresh(Refresh::All(Timestamp::now())))
            .native_tls(native_tls)
            .connectivity(connectivity)
            .index_urls(index_locations.index_urls())
            .index_strategy(index_strategy)
            .keyring(keyring_provider)
            .allow_insecure_host(allow_insecure_host.to_vec())
            .markers(environment.interpreter().markers())
            .platform(environment.interpreter().platform())
            .build();

    // Determine the platform tags.
    let interpreter = environment.interpreter();
    let tags = interpreter.tags()?;

    // Determine the `requires-python` specifier.
    let python_request = tool.python().as_deref().map(PythonRequest::parse);
    let requires_python = if let Some(python_request) = python_request {
        match python_request {
            PythonRequest::Version(VersionRequest::MajorMinor(
                major,
                minor,
                PythonVariant::Default,
            )) => RequiresPython::greater_than_equal_version(&Version::new([
                u64::from(major),
                u64::from(minor),
            ])),
            PythonRequest::Version(VersionRequest::MajorMinorPatch(
                major,
                minor,
                patch,
                PythonVariant::Default,
            )) => RequiresPython::greater_than_equal_version(&Version::new([
                u64::from(major),
                u64::from(minor),
                u64::from(patch),
            ])),
            PythonRequest::Version(VersionRequest::Range(ref specifiers, _)) => {
                RequiresPython::from_specifiers(specifiers)
            }
            python_request => {
                match PythonInstallation::find(
                    &python_request,
                    EnvironmentPreference::OnlySystem,
                    python_preference,
                    cache,
                ) {
                    Ok(installation) => {
                        let interpreter = installation.into_interpreter();
                        RequiresPython::greater_than_equal_version(
                            &interpreter.python_minor_version(),
                        )
                    }
                    Err(err) => {
                        warn!(
                            "Failed to find a Python interpreter for tool `{name}` by request `{python_request}`, use current tool interpreter instead: {err}",
                        );
                        RequiresPython::greater_than_equal_version(
                            &interpreter.python_minor_version(),
                        )
                    }
                }
            }
        }
    } else {
        RequiresPython::greater_than_equal_version(&interpreter.python_minor_version())
    };

    let client = LatestClient {
        client: &client,
        capabilities: &capabilities,
        prerelease,
        exclude_newer,
        tags: Some(tags),
        requires_python: &requires_python,
    };

    Ok(client.find_latest(name, None).await?)
}
