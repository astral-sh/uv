use std::{
    collections::{BTreeMap, BTreeSet, Bound},
    ffi::OsString,
    fmt::Write,
    io,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use itertools::Itertools;
use owo_colors::OwoColorize;
use thiserror::Error;
use tracing::{debug, warn};
use uv_cache::{Cache, Refresh};
use uv_client::{BaseClientBuilder, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    BuildOptions, Concurrency, Constraints, DependencyGroupsWithDefaults, ExcludeDependency,
    ExtrasSpecification, GitLfsSetting, InstallOptions, Override, TargetTriple,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::{
    DistributionDatabase, LoweredExtraBuildDependencies, StaticMetadataDatabase,
};
use uv_distribution_types::{
    DependencyMetadata, HashGeneration, Index, InstalledDist, Name, Requirement, RequiresPython,
    Resolution, UnresolvedRequirement,
};
use uv_errors::{ErrorWithHints, Hint, Hints};
#[cfg(unix)]
use uv_fs::replace_symlink;
use uv_fs::{CWD, Simplified};
use uv_git::GitResolver;
use uv_installer::SitePackages;
use uv_normalize::{DefaultExtras, GroupName, PackageName};
use uv_pep440::{Version, VersionSpecifier, VersionSpecifiers};
use uv_preview::Preview;
use uv_pypi_types::Conflicts;
use uv_python::{
    ConfigDiscovery, EnvironmentPreference, Interpreter, PythonDownloads, PythonEnvironment,
    PythonInstallation, PythonPreference, PythonRequest, PythonVariant, PythonVersionFile,
    VersionFileDiscoveryOptions, VersionRequest,
};
use uv_requirements::RequirementsSpecification;
use uv_resolver::{
    FlatIndex, Installable, Lock, OptionsBuilder, Preference, ResolverManifest, ResolverOutput,
};
use uv_settings::{PythonInstallMirrors, ToolOptions};
use uv_shell::Shell;
use uv_tool::{InstalledTools, Tool, ToolEntrypoint, entrypoint_paths};
use uv_types::{BuildIsolation, HashStrategy, SourceTreeEditablePolicy};
use uv_warnings::warn_user_once;
use uv_workspace::WorkspaceCache;

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
use crate::commands::project::{
    EnvironmentSpecification, PlatformState, PreferenceLocation, ProjectError, PythonRequestSource,
    lock::ValidatedLock,
};
use crate::commands::reporters::PythonDownloadReporter;
use crate::printer::Printer;
use crate::settings::ResolverSettings;

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
    remove_entrypoint_paths(
        tool.entrypoints()
            .iter()
            .map(|entrypoint| entrypoint.install_path.as_path()),
    );
}

/// Remove the entrypoints at the given paths.
fn remove_entrypoint_paths<'a>(entrypoints: impl IntoIterator<Item = &'a Path>) {
    for executable in entrypoints {
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
    /// version file, and static `requires-python` metadata from the source requirement.
    pub(crate) python_request: Option<PythonRequest>,
}

impl ToolPython {
    /// Determine the [`ToolPython`] request for a tool invocation.
    pub(crate) async fn from_request(
        python_request: Option<PythonRequest>,
        requirement: Option<&UnresolvedRequirement>,
        config_discovery: ConfigDiscovery,
        lfs: GitLfsSetting,
        git_resolver: &GitResolver,
        client_builder: &BaseClientBuilder<'_>,
        cache: &Cache,
    ) -> Result<Self, ProjectError> {
        let requires_python = if python_request.is_none() {
            match requirement {
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
            }
        } else {
            None
        };

        let (source, python_request) = if let Some(request) = python_request {
            (PythonRequestSource::UserRequest, Some(request))
        } else if let Some(file) = PythonVersionFile::discover(
            &*CWD,
            &VersionFileDiscoveryOptions::default()
                .with_config_discovery(config_discovery)
                .with_no_local(true),
        )
        .await?
        .filter(|file| match (file.version(), requires_python.as_ref()) {
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
                requires_python
                    .as_ref()
                    .and_then(PythonRequest::from_requires_python),
            )
        };

        if let Some(python_request) = python_request.as_ref() {
            debug!(
                "Using Python request `{}` from {source}",
                python_request.to_canonical_string()
            );
        }

        Ok(Self {
            source,
            python_request,
        })
    }

    /// Returns `true` if the selected request was explicitly provided by the user.
    pub(crate) fn is_explicit(&self) -> bool {
        matches!(self.source, PythonRequestSource::UserRequest)
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

/// A universal lock for a tool environment.
pub(crate) struct ToolLock {
    root: PathBuf,
    lock: Lock,
}

/// A tool lock validated against the current resolution inputs.
pub(crate) struct ValidatedToolLock {
    lock: ToolLock,
    satisfied: bool,
    usable: bool,
}

impl ValidatedToolLock {
    /// Return whether the existing lock satisfies the current resolution inputs.
    pub(crate) fn is_satisfied(&self) -> bool {
        self.satisfied
    }

    /// Return the lock as a resolver preference if its versions remain usable.
    pub(crate) fn preference(&self) -> Option<&ToolLock> {
        self.usable.then_some(&self.lock)
    }

    /// Return the validated lock.
    pub(crate) fn into_lock(self) -> ToolLock {
        self.lock
    }
}

impl ToolLock {
    /// Build the lock manifest for a tool environment.
    pub(crate) fn manifest(
        requirements: &[Requirement],
        constraints: &[Requirement],
        overrides: &[Requirement],
        excludes: &[ExcludeDependency],
        build_constraints: &[Requirement],
        dependency_metadata: &DependencyMetadata,
    ) -> ResolverManifest {
        ResolverManifest::new(
            std::iter::empty::<PackageName>(),
            requirements.iter().cloned(),
            constraints.iter().cloned(),
            overrides.iter().cloned().map(Override::Requirement),
            excludes.iter().cloned(),
            build_constraints.iter().cloned(),
            std::iter::empty::<(GroupName, Vec<Requirement>)>(),
            dependency_metadata.values().cloned(),
        )
    }

    /// Build the lock for a tool environment.
    pub(crate) fn from_resolution(
        root: &Path,
        resolution: &ResolverOutput,
        manifest: &ResolverManifest,
    ) -> anyhow::Result<Self> {
        let lock = Lock::from_resolution(resolution, root, Vec::new())?;
        let manifest = manifest.clone().relative_to(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            lock: lock.with_manifest(manifest),
        })
    }

    /// Read the lock for a tool, if one has been generated.
    pub(crate) fn read(directory: &Path) -> Option<Self> {
        let path = directory.join("uv.lock");
        match fs_err::read_to_string(&path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(lock) => Some(Self {
                    root: directory.to_path_buf(),
                    lock,
                }),
                Err(err) => {
                    debug!(
                        "Ignoring invalid tool lock at `{}`: {err}",
                        path.user_display()
                    );
                    None
                }
            },
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => {
                debug!(
                    "Ignoring unreadable tool lock at `{}`: {err}",
                    path.user_display()
                );
                None
            }
        }
    }

    /// Write or remove the lock for a tool.
    pub(crate) fn write(directory: &Path, lock: Option<&Self>) -> anyhow::Result<()> {
        let path = directory.join("uv.lock");
        if let Some(lock) = lock {
            uv_fs::write_atomic_sync(&path, lock.lock.to_toml()?)?;
        } else {
            match fs_err::remove_file(path) {
                Ok(()) => (),
                Err(err) if err.kind() == io::ErrorKind::NotFound => (),
                Err(err) => return Err(err.into()),
            }
        }
        Ok(())
    }

    /// Validate the lock against the current resolution inputs.
    #[expect(clippy::too_many_arguments)]
    pub(crate) async fn validate(
        self,
        requirements: &[Requirement],
        constraints: &[Requirement],
        overrides: &[Requirement],
        excludes: &[ExcludeDependency],
        build_constraints: &[Requirement],
        refresh: &Refresh,
        interpreter: &Interpreter,
        settings: &ResolverSettings,
        client_builder: &BaseClientBuilder<'_>,
        state: &PlatformState,
        concurrency: &Concurrency,
        cache: &Cache,
        workspace_cache: &WorkspaceCache,
        printer: Printer,
        preview: Preview,
    ) -> Result<ValidatedToolLock, ProjectError> {
        let ResolverSettings {
            index_locations,
            index_strategy,
            keyring_provider,
            resolution,
            prerelease,
            fork_strategy,
            dependency_metadata,
            config_setting,
            config_settings_package,
            build_isolation,
            extra_build_dependencies,
            extra_build_variables,
            exclude_newer,
            link_mode,
            upgrade,
            build_options,
            sources,
            torch_backend: _,
            cuda_driver_version: _,
            amd_gpu_architecture: _,
        } = settings;

        let client = RegistryClientBuilder::new(
            client_builder.clone().keyring(*keyring_provider),
            cache.clone(),
        )
        .index_locations(index_locations.clone())
        .index_strategy(*index_strategy)
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build()?;

        let environment;
        let build_isolation = match build_isolation {
            uv_configuration::BuildIsolation::Isolate => BuildIsolation::Isolated,
            uv_configuration::BuildIsolation::Shared => {
                environment = PythonEnvironment::from_interpreter(interpreter.clone());
                BuildIsolation::Shared(&environment)
            }
            uv_configuration::BuildIsolation::SharedPackage(packages) => {
                environment = PythonEnvironment::from_interpreter(interpreter.clone());
                BuildIsolation::SharedPackage(&environment, packages)
            }
        };

        let options = OptionsBuilder::new()
            .resolution_mode(*resolution)
            .prerelease_mode(*prerelease)
            .fork_strategy(*fork_strategy)
            .exclude_newer(exclude_newer.clone())
            .index_strategy(*index_strategy)
            .build_options(build_options.clone())
            .build();
        let hasher = HashStrategy::Generate(HashGeneration::Url);
        let build_hasher = HashStrategy::default();

        let flat_index = {
            let client = FlatIndexClient::new(client.cached_client(), client.connectivity(), cache);
            let entries = client
                .fetch_all(index_locations.flat_indexes().map(Index::url))
                .await?;
            FlatIndex::from_entries(entries, None, &hasher, build_options)
        };

        let extra_build_requires =
            LoweredExtraBuildDependencies::from_non_lowered(extra_build_dependencies.clone())
                .into_inner();
        let dispatch_constraints =
            Constraints::from_requirements(build_constraints.iter().cloned());
        let build_dispatch = BuildDispatch::new(
            &client,
            cache,
            &dispatch_constraints,
            interpreter,
            index_locations,
            &flat_index,
            dependency_metadata,
            state.clone().into_inner(),
            *index_strategy,
            config_setting,
            config_settings_package,
            build_isolation,
            &extra_build_requires,
            extra_build_variables,
            *link_mode,
            build_options,
            &build_hasher,
            exclude_newer.clone(),
            sources.clone(),
            SourceTreeEditablePolicy::Tool,
            workspace_cache.clone(),
            concurrency.clone(),
            preview,
        );
        let database = DistributionDatabase::new(
            &client,
            &build_dispatch,
            concurrency.downloads_semaphore.clone(),
        );

        let requires_python =
            RequiresPython::greater_than_equal_version(&interpreter.python_minor_version());
        let overrides = overrides
            .iter()
            .cloned()
            .map(Override::Requirement)
            .collect::<Vec<_>>();
        let Self { root, lock } = self;
        let validated = ValidatedLock::validate(
            lock,
            &root,
            &BTreeMap::new(),
            &[],
            &BTreeMap::new(),
            requirements,
            &BTreeMap::new(),
            constraints,
            &overrides,
            excludes,
            build_constraints,
            &Conflicts::empty(),
            None,
            None,
            dependency_metadata,
            interpreter,
            &requires_python,
            index_locations,
            upgrade,
            Some(refresh),
            &options,
            &hasher,
            state.index(),
            &database,
            printer,
        )
        .await?;
        let satisfied = validated.is_satisfied();
        let usable = validated.is_usable();

        Ok(ValidatedToolLock {
            lock: Self {
                root,
                lock: validated.into_lock(),
            },
            satisfied,
            usable,
        })
    }

    /// Project the universal lock into a specific environment.
    pub(crate) fn to_resolution(
        &self,
        project_name: Option<&PackageName>,
        interpreter: &Interpreter,
        python_platform: Option<&TargetTriple>,
        build_options: &BuildOptions,
    ) -> anyhow::Result<Resolution> {
        struct ToolLockInstallTarget<'lock> {
            tool_lock: &'lock ToolLock,
            project_name: Option<&'lock PackageName>,
        }

        impl<'lock> Installable<'lock> for ToolLockInstallTarget<'lock> {
            fn install_path(&self) -> &'lock Path {
                &self.tool_lock.root
            }

            fn lock(&self) -> &'lock Lock {
                &self.tool_lock.lock
            }

            fn roots(&self) -> impl Iterator<Item = &PackageName> {
                std::iter::empty()
            }

            fn project_name(&self) -> Option<&PackageName> {
                self.project_name
            }
        }

        let markers = pip::resolution_markers(None, python_platform, interpreter);
        let tags = pip::resolution_tags(None, python_platform, interpreter)?;
        Ok(ToolLockInstallTarget {
            tool_lock: self,
            project_name,
        }
        .to_resolution(
            &markers,
            &tags,
            &ExtrasSpecification::default().with_defaults(DefaultExtras::default()),
            &DependencyGroupsWithDefaults::none(),
            build_options,
            &InstallOptions::default(),
        )?)
    }
}

/// Build an environment specification for a tool, preferring versions from its existing lock when
/// available, then falling back to the installed environment.
pub(crate) fn tool_environment_spec<'lock>(
    requirements: RequirementsSpecification,
    lock: Option<&'lock ToolLock>,
    site_packages: Option<&SitePackages>,
) -> EnvironmentSpecification<'lock> {
    let specification = EnvironmentSpecification::from(requirements);
    if let Some(lock) = lock {
        return specification.with_preferences(PreferenceLocation::Lock {
            lock: &lock.lock,
            install_path: &lock.root,
        });
    }

    let preferences = site_packages
        .into_iter()
        .flat_map(|site_packages| site_packages.iter().filter_map(Preference::from_installed))
        .collect::<Vec<_>>();
    if preferences.is_empty() {
        return specification;
    }

    specification.with_preferences(PreferenceLocation::Entries(preferences))
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
    excludes: Vec<ExcludeDependency>,
    build_constraints: Vec<Requirement>,
    lock: Option<&ToolLock>,
    printer: Printer,
) -> anyhow::Result<()> {
    let executable_directory = uv_tool::tool_executable_dir()?;
    fs_err::create_dir_all(&executable_directory)
        .context("Failed to create executable directory")?;
    debug!(
        "Installing tool executables into: {}",
        executable_directory.user_display()
    );

    let mut installed_entrypoints: Vec<ToolEntrypoint> = Vec::new();
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
        let Some(dist) = installed.first() else {
            if package != name {
                bail!("Expected package `{package}` to be installed");
            }

            writeln!(
                printer.stdout(),
                "No executables are provided by package `{}`; removing tool",
                package.cyan()
            )?;
            remove_entrypoint_paths(
                installed_entrypoints
                    .iter()
                    .map(|entrypoint| entrypoint.install_path.as_path()),
            );
            installed_tools.remove_environment(name)?;

            return Err(NoExecutablesError::Root {
                package: package.clone(),
                matching_dependency_packages: Vec::new(),
            }
            .into());
        };
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
            remove_entrypoint_paths(
                installed_entrypoints
                    .iter()
                    .map(|entrypoint| entrypoint.install_path.as_path()),
            );
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
                remove_entrypoint_paths(
                    installed_entrypoints
                        .iter()
                        .map(|entrypoint| entrypoint.install_path.as_path()),
                );
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
    ToolLock::write(&installed_tools.tool_dir(name), lock)?;
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
