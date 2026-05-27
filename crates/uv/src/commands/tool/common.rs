use std::{
    collections::{BTreeSet, Bound},
    ffi::OsString,
    fmt::Write,
    path::Path,
};

use anyhow::{Context, bail};
use itertools::Itertools;
use owo_colors::OwoColorize;
use thiserror::Error;
use tracing::{debug, warn};
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{Concurrency, GitLfsSetting};
use uv_distribution::StaticMetadataDatabase;
use uv_distribution_types::{
    IndexCapabilities, IndexMetadata, InstalledDist, Name, Requirement, RequirementSource,
    RequiresPython, UnresolvedRequirement,
};
use uv_errors::{ErrorWithHints, Hint, Hints};
#[cfg(unix)]
use uv_fs::replace_symlink;
use uv_fs::{CWD, Simplified};
use uv_git::GitResolver;
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifier, VersionSpecifiers};
use uv_python::{
    EnvironmentPreference, Interpreter, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest, PythonVariant, PythonVersionFile, VersionFileDiscoveryOptions,
    VersionRequest,
};
use uv_settings::{PythonInstallMirrors, ToolOptions};
use uv_shell::Shell;
use uv_tool::{InstalledTools, Tool, ToolEntrypoint, entrypoint_paths};
use uv_warnings::warn_user_once;

use crate::commands::pip;

/// An error raised when a tool package provides no executables.
#[derive(Debug, Error)]
pub(crate) enum NoExecutablesError {
    /// A dependency was requested as a source of tool executables.
    #[error("No executables are provided by package `{package}`")]
    Dependency { package: PackageName },
    /// The root package cannot be installed as a tool.
    #[error("Failed to install entrypoints for `{package}`")]
    Root {
        package: PackageName,
        /// Executables found in dependencies of the package that match the package name.
        matching_dependency_packages: Vec<PackageName>,
    },
}

impl Hint for NoExecutablesError {
    fn hints(&self) -> Hints<'_> {
        let mut hints = Hints::none();
        let (package, matching_dependency_packages) = match self {
            Self::Dependency { package } => {
                hints.push(format!(
                    "Use `--with {}` to include `{}` as a dependency without installing its executables",
                    package.cyan(),
                    package.cyan(),
                ));
                return hints;
            }
            Self::Root {
                package,
                matching_dependency_packages,
            } => (package, matching_dependency_packages),
        };

        match matching_dependency_packages.as_slice() {
            [] => {}
            [dep] => {
                let command = format!("uv tool install {dep}");
                hints.push(format!(
                    "An executable with the name `{}` is available via dependency `{}`.\n      Did you mean `{}`?",
                    package.cyan(),
                    dep.cyan(),
                    command.bold(),
                ));
            }
            deps => {
                let dep_list = deps
                    .iter()
                    .map(|dep| format!("- {}", dep.cyan()))
                    .join("\n");
                hints.push(format!(
                    "An executable with the name `{}` is available via the following dependencies:\n{dep_list}\n      Did you mean to install one of them instead?",
                    package.cyan(),
                ));
            }
        }
        hints
    }
}
use crate::commands::project::{ProjectError, PythonRequestSource};
use crate::commands::reporters::PythonDownloadReporter;
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Return all packages which contain an executable with the given name.
pub(super) fn matching_packages(name: &str, site_packages: &SitePackages) -> Vec<InstalledDist> {
    site_packages
        .iter()
        .filter_map(|package| {
            entrypoint_paths(site_packages, package.name(), package.version())
                .ok()
                .and_then(|entrypoints| {
                    entrypoints
                        .iter()
                        .any(|entrypoint| {
                            entrypoint
                                .0
                                .strip_suffix(std::env::consts::EXE_SUFFIX)
                                .is_some_and(|stripped| stripped == name)
                        })
                        .then(|| package.clone())
                })
        })
        .collect()
}

/// Remove any entrypoints attached to the [`Tool`].
pub(crate) fn remove_entrypoints(tool: &Tool) {
    for executable in tool
        .entrypoints()
        .iter()
        .map(|entrypoint| &entrypoint.install_path)
    {
        debug!("Removing executable: `{}`", executable.simplified_display());
        if let Err(err) = fs_err::remove_file(executable) {
            warn!(
                "Failed to remove executable: `{}`: {err}",
                executable.simplified_display()
            );
        }
    }
}

/// The resolved Python request for a tool invocation.
#[derive(Debug, Clone)]
pub(crate) struct ToolPython {
    /// The source of the Python request.
    source: PythonRequestSource,
    /// The selected Python request, computed by considering an explicit request, a global
    /// version file, and static `requires-python` metadata from the target requirement.
    pub(crate) python_request: Option<PythonRequest>,
    /// The Python request to apply when considering an existing installed tool environment.
    ///
    /// Registry metadata describes a new resolution of a tool, not an already-installed
    /// version that may still be reusable.
    installed_environment_request: Option<PythonRequest>,
}

impl ToolPython {
    /// Determine the [`ToolPython`] request for a tool invocation.
    pub(crate) async fn from_request(
        python_request: Option<PythonRequest>,
        requirement: Option<&UnresolvedRequirement>,
        registry_requirement: Option<&Requirement>,
        no_config: bool,
        lfs: GitLfsSetting,
        git_resolver: &GitResolver,
        client_builder: &BaseClientBuilder<'_>,
        settings: &ResolverInstallerSettings,
        concurrency: &Concurrency,
        cache: &Cache,
    ) -> Result<Self, ProjectError> {
        let (source_requires_python, registry_requires_python) = if python_request.is_none() {
            let source_requires_python = match requirement {
                Some(requirement) => {
                    infer_requires_python_from_requirement(
                        requirement,
                        lfs,
                        git_resolver,
                        client_builder,
                        cache,
                    )
                    .await
                }
                None => None,
            };

            if source_requires_python.is_some() {
                (source_requires_python, None)
            } else {
                let registry_requirement = registry_requirement.or_else(|| {
                    requirement.and_then(|requirement| match requirement {
                        UnresolvedRequirement::Named(requirement) => Some(requirement),
                        UnresolvedRequirement::Unnamed(_) => None,
                    })
                });

                let registry_requires_python = match registry_requirement {
                    Some(requirement) => {
                        infer_requires_python_from_registry_requirement(
                            requirement,
                            settings,
                            git_resolver,
                            client_builder,
                            concurrency,
                            cache,
                        )
                        .await
                    }
                    None => None,
                };
                (source_requires_python, registry_requires_python)
            }
        } else {
            (None, None)
        };

        let (source, selected_python_request) = Self::select_request(
            python_request.clone(),
            source_requires_python
                .as_ref()
                .or(registry_requires_python.as_ref()),
            no_config,
        )
        .await?;
        let installed_environment_request = if registry_requires_python.is_some() {
            Self::select_request(python_request, source_requires_python.as_ref(), no_config)
                .await?
                .1
        } else {
            selected_python_request.clone()
        };

        if let Some(python_request) = selected_python_request.as_ref() {
            debug!(
                "Using Python request `{}` from {source}",
                python_request.to_canonical_string()
            );
        }

        Ok(Self {
            source,
            python_request: selected_python_request,
            installed_environment_request,
        })
    }

    async fn select_request(
        python_request: Option<PythonRequest>,
        requires_python: Option<&RequiresPython>,
        no_config: bool,
    ) -> Result<(PythonRequestSource, Option<PythonRequest>), ProjectError> {
        let selected = if let Some(request) = python_request {
            (PythonRequestSource::UserRequest, Some(request))
        } else if let Some(file) = PythonVersionFile::discover(
            &*CWD,
            &VersionFileDiscoveryOptions::default()
                .with_no_config(no_config)
                .with_no_local(true),
        )
        .await?
        .filter(|file| match (file.version(), requires_python) {
            (Some(request), Some(requires_python)) => {
                request.intersects_requires_python(requires_python)
            }
            _ => true,
        }) {
            (
                PythonRequestSource::DotPythonVersion(file.clone()),
                file.version().cloned(),
            )
        } else {
            (
                PythonRequestSource::RequiresPython,
                requires_python.and_then(PythonRequest::from_requires_python),
            )
        };
        Ok(selected)
    }

    /// Returns `true` if the selected request was explicitly provided by the user.
    pub(crate) fn is_explicit(&self) -> bool {
        matches!(self.source, PythonRequestSource::UserRequest)
    }

    /// Return the Python request to apply when considering an installed tool environment.
    pub(crate) fn installed_environment_request(&self) -> Option<&PythonRequest> {
        self.installed_environment_request.as_ref()
    }
}

/// Infer [`RequiresPython`] from a direct source requirement by reading its `pyproject.toml`.
///
/// Returns `None` when the requirement is not a directory or Git source, its metadata is not
/// statically available, or the Git source cannot be fetched.
async fn infer_requires_python_from_requirement(
    requirement: &UnresolvedRequirement,
    lfs: GitLfsSetting,
    git_resolver: &GitResolver,
    client_builder: &BaseClientBuilder<'_>,
    cache: &Cache,
) -> Option<RequiresPython> {
    let requirement = requirement
        .clone()
        .augment_requirement(None, None, None, lfs.into(), None);
    let source = requirement.source();

    match StaticMetadataDatabase::new(client_builder, git_resolver, cache)
        .requires_python(source.as_ref())
        .await
    {
        Ok(requires_python) => requires_python,
        Err(err) => {
            debug!(
                "Failed to infer `requires-python` from source requirement (`{requirement}`): {err}"
            );
            None
        }
    }
}

/// Infer [`RequiresPython`] from static metadata exposed by a registry for the selected release.
///
/// The registry lookup reads Simple API metadata or wheel metadata; it never builds a source
/// distribution to discover metadata.
async fn infer_requires_python_from_registry_requirement(
    requirement: &Requirement,
    settings: &ResolverInstallerSettings,
    git_resolver: &GitResolver,
    client_builder: &BaseClientBuilder<'_>,
    concurrency: &Concurrency,
    cache: &Cache,
) -> Option<RequiresPython> {
    let RequirementSource::Registry {
        specifier, index, ..
    } = &requirement.source
    else {
        return None;
    };

    let client = match RegistryClientBuilder::new(
        client_builder
            .clone()
            .keyring(settings.resolver.keyring_provider),
        cache.clone(),
    )
    .index_locations(settings.resolver.index_locations.clone())
    .index_strategy(settings.resolver.index_strategy)
    .build()
    {
        Ok(client) => client,
        Err(err) => {
            debug!(
                "Failed to create registry client while inferring `requires-python` (`{requirement}`): {err}"
            );
            return None;
        }
    };
    let capabilities = IndexCapabilities::default();
    let latest_client = pip::latest::LatestClient {
        client: &client,
        capabilities: &capabilities,
        prerelease: settings.resolver.prerelease,
        exclude_newer: &settings.resolver.exclude_newer,
        index_locations: &settings.resolver.index_locations,
        tags: None,
        requires_python: None,
    };

    let distribution = match latest_client
        .find_latest_with_specifier(
            &requirement.name,
            index.as_ref().map(IndexMetadata::url),
            Some(specifier),
            &concurrency.downloads_semaphore,
        )
        .await
    {
        Ok(distribution) => distribution?,
        Err(err) => {
            debug!(
                "Failed to read registry metadata while inferring `requires-python` (`{requirement}`): {err}"
            );
            return None;
        }
    };

    if let Some(requires_python) = distribution.requires_python.as_ref() {
        return Some(RequiresPython::from_specifiers(requires_python));
    }

    let wheel = distribution.wheel?;
    match client
        .wheel_metadata(&wheel, git_resolver, &capabilities, None)
        .await
    {
        Ok(metadata) => metadata
            .requires_python
            .as_ref()
            .map(RequiresPython::from_specifiers),
        Err(err) => {
            debug!(
                "Failed to read wheel metadata while inferring `requires-python` (`{requirement}`): {err}"
            );
            None
        }
    }
}

/// Given a no-solution error and the [`Interpreter`] that was used during the solve, attempt to
/// discover an alternate [`Interpreter`] that satisfies the `requires-python` constraint.
pub(crate) async fn refine_interpreter(
    interpreter: &Interpreter,
    python_request: Option<&PythonRequest>,
    err: &pip::operations::Error,
    client_builder: &BaseClientBuilder<'_>,
    reporter: &PythonDownloadReporter,
    install_mirrors: &PythonInstallMirrors,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    cache: &Cache,
) -> anyhow::Result<Option<Interpreter>, ProjectError> {
    let pip::operations::Error::Resolve(uv_resolver::ResolveError::NoSolution(no_solution_err)) =
        err
    else {
        return Ok(None);
    };

    // Infer the `requires-python` constraint from the error.
    let requires_python = no_solution_err.find_requires_python();

    // If the existing interpreter already satisfies the `requires-python` constraint, we don't need
    // to refine it. We'd expect to fail again anyway.
    if requires_python.contains(interpreter.python_version()) {
        return Ok(None);
    }

    // We want an interpreter that's as close to the required version as possible. If we choose the
    // "latest" Python, we risk choosing a version that lacks wheels for the tool's requirements
    // (assuming those requirements don't publish source distributions).
    //
    // TODO(charlie): Solve for the Python version iteratively (or even, within the resolver
    // itself). The current strategy can also fail if the tool's requirements have greater
    // `requires-python` constraints, and we didn't see them in the initial solve. It can also fail
    // if the tool's requirements don't publish wheels for this interpreter version, though that's
    // rarer.
    let lower_bound = match requires_python.as_ref() {
        Bound::Included(version) => VersionSpecifier::greater_than_equal_version(version.clone()),
        Bound::Excluded(version) => VersionSpecifier::greater_than_version(version.clone()),
        Bound::Unbounded => unreachable!("`requires-python` should never be unbounded"),
    };

    let upper_bound = match requires_python.as_ref() {
        Bound::Included(version) => {
            let major = version.release().first().copied().unwrap_or(0);
            let minor = version.release().get(1).copied().unwrap_or(0);
            VersionSpecifier::less_than_version(Version::new([major, minor + 1]))
        }
        Bound::Excluded(version) => {
            let major = version.release().first().copied().unwrap_or(0);
            let minor = version.release().get(1).copied().unwrap_or(0);
            VersionSpecifier::less_than_version(Version::new([major, minor + 1]))
        }
        Bound::Unbounded => unreachable!("`requires-python` should never be unbounded"),
    };

    let requires_python_request = PythonRequest::Version(VersionRequest::from_specifiers(
        VersionSpecifiers::from_iter([lower_bound, upper_bound]),
        PythonVariant::default(),
    ));

    debug!("Refining interpreter with: {requires_python_request}");

    let interpreter = PythonInstallation::find_or_download(
        Some(&requires_python_request),
        EnvironmentPreference::OnlySystem,
        python_preference,
        python_downloads,
        client_builder,
        cache,
        Some(reporter),
        install_mirrors.python_install_mirror.as_deref(),
        install_mirrors.pypy_install_mirror.as_deref(),
        install_mirrors.python_downloads_json_url.as_deref(),
    )
    .await?
    .into_interpreter();

    // If the user passed a `--python` request, and the refined interpreter is incompatible, we
    // can't use it.
    if let Some(python_request) = python_request {
        if !python_request.satisfied(&interpreter, cache) {
            return Ok(None);
        }
    }

    Ok(Some(interpreter))
}

/// Finalizes a tool installation, after creation of an environment.
///
/// Installs tool executables for a given package, handling any conflicts.
///
/// Adds a receipt for the tool.
pub(crate) fn finalize_tool_install(
    environment: &PythonEnvironment,
    name: &PackageName,
    entrypoints: &[PackageName],
    installed_tools: &InstalledTools,
    options: &ToolOptions,
    force: bool,
    python: Option<PythonRequest>,
    requirements: Vec<Requirement>,
    constraints: Vec<Requirement>,
    overrides: Vec<Requirement>,
    excludes: Vec<PackageName>,
    build_constraints: Vec<Requirement>,
    printer: Printer,
) -> anyhow::Result<()> {
    let executable_directory = uv_tool::tool_executable_dir()?;
    fs_err::create_dir_all(&executable_directory)
        .context("Failed to create executable directory")?;
    debug!(
        "Installing tool executables into: {}",
        executable_directory.user_display()
    );

    let mut installed_entrypoints = Vec::new();
    let site_packages = SitePackages::from_environment(environment)?;
    let ordered_packages = entrypoints
        // Install dependencies first
        .iter()
        .filter(|pkg| *pkg != name)
        .collect::<BTreeSet<_>>()
        // Then install the root package last
        .into_iter()
        .chain(std::iter::once(name));

    for package in ordered_packages {
        if package == name {
            debug!("Installing entrypoints for tool `{package}`");
        } else {
            debug!("Installing entrypoints for `{package}` as part of tool `{name}`");
        }

        let installed = site_packages.get_packages(package);
        let dist = installed
            .first()
            .context("Expected at least one requirement")?;
        let dist_entrypoints = entrypoint_paths(&site_packages, dist.name(), dist.version())?;

        // Determine the entry points targets. Use a sorted collection for deterministic output.
        let target_entrypoints = dist_entrypoints
            .into_iter()
            .map(|(name, source_path)| {
                let target_path = executable_directory.join(
                    source_path
                        .file_name()
                        .map(std::borrow::ToOwned::to_owned)
                        .unwrap_or_else(|| OsString::from(name.clone())),
                );
                (name, source_path, target_path)
            })
            .collect::<BTreeSet<_>>();

        if target_entrypoints.is_empty() {
            let err = if package != name {
                NoExecutablesError::Dependency {
                    package: package.clone(),
                }
            } else {
                NoExecutablesError::Root {
                    package: package.clone(),
                    matching_dependency_packages: matching_packages(
                        package.as_ref(),
                        &site_packages,
                    )
                    .into_iter()
                    .map(|dist| dist.name().clone())
                    .collect(),
                }
            };

            if package != name {
                // Non-root package: display the error with hints and continue.
                writeln!(
                    printer.stdout(),
                    "{}",
                    ErrorWithHints::new(&err, err.hints())
                )?;
                continue;
            }

            // For the root package, this is a fatal error.
            writeln!(
                printer.stdout(),
                "No executables are provided by package `{}`; removing tool",
                package.cyan()
            )?;

            // Clean up the environment we just created.
            installed_tools.remove_environment(name)?;

            return Err(err.into());
        }

        // Error if we're overwriting an existing entrypoint, unless the user passed `--force`.
        if !force {
            let mut existing_entrypoints = target_entrypoints
                .iter()
                .filter(|(_, _, target_path)| target_path.exists())
                .peekable();
            if existing_entrypoints.peek().is_some() {
                // Clean up the environment we just created
                installed_tools.remove_environment(name)?;

                let existing_entrypoints = existing_entrypoints
                    // SAFETY: We know the target has a filename because we just constructed it above
                    .map(|(_, _, target)| target.file_name().unwrap().to_string_lossy())
                    .collect::<Vec<_>>();
                let (s, exists) = if existing_entrypoints.len() == 1 {
                    ("", "exists")
                } else {
                    ("s", "exist")
                };
                bail!(
                    "Executable{s} already {exists}: {} (use `--force` to overwrite)",
                    existing_entrypoints
                        .iter()
                        .map(|name| name.bold())
                        .join(", ")
                )
            }
        }

        #[cfg(windows)]
        let itself = std::env::current_exe().ok();

        let mut names = BTreeSet::new();
        for (name, src, target) in target_entrypoints {
            debug!("Installing executable: `{name}`");

            #[cfg(unix)]
            replace_symlink(src, &target).context("Failed to install executable")?;

            #[cfg(windows)]
            if itself.as_ref().is_some_and(|itself| {
                std::path::absolute(&target).is_ok_and(|target| *itself == target)
            }) {
                self_replace::self_replace(src).context("Failed to install entrypoint")?;
            } else {
                fs_err::copy(src, &target).context("Failed to install entrypoint")?;
            }

            let tool_entry = ToolEntrypoint::new(&name, target, package.to_string());
            names.insert(tool_entry.name.clone());
            installed_entrypoints.push(tool_entry);
        }

        let s = if names.len() == 1 { "" } else { "s" };
        let from_pkg = if name == package {
            String::new()
        } else {
            format!(" from `{package}`")
        };
        writeln!(
            printer.stderr(),
            "Installed {} executable{s}{from_pkg}: {}",
            names.len(),
            names.iter().map(|name| name.bold()).join(", ")
        )?;
    }

    debug!("Adding receipt for tool `{name}`");
    let tool = Tool::new(
        requirements,
        constraints,
        overrides,
        excludes,
        build_constraints,
        python,
        installed_entrypoints,
        options.clone(),
    );
    installed_tools.add_tool_receipt(name, tool)?;

    warn_out_of_path(&executable_directory);

    Ok(())
}

fn warn_out_of_path(executable_directory: &Path) {
    // If the executable directory isn't on the user's PATH, warn.
    if !Shell::contains_path(executable_directory) {
        if let Some(shell) = Shell::from_env() {
            if let Some(command) = shell.prepend_path(executable_directory) {
                if shell.supports_update() {
                    warn_user_once!(
                        "`{}` is not on your PATH. To use installed tools, run `{}` or `{}`.",
                        executable_directory.simplified_display().cyan(),
                        command.green(),
                        "uv tool update-shell".green()
                    );
                } else {
                    warn_user_once!(
                        "`{}` is not on your PATH. To use installed tools, run `{}`.",
                        executable_directory.simplified_display().cyan(),
                        command.green()
                    );
                }
            } else {
                warn_user_once!(
                    "`{}` is not on your PATH. To use installed tools, add the directory to your PATH.",
                    executable_directory.simplified_display().cyan(),
                );
            }
        } else {
            warn_user_once!(
                "`{}` is not on your PATH. To use installed tools, add the directory to your PATH.",
                executable_directory.simplified_display().cyan(),
            );
        }
    }
}
