use std::collections::hash_map::Entry;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::io;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::{FxBuildHasher, FxHashMap};
use tracing::debug;
use url::Url;

use uv_cache::Cache;
use uv_cache_key::RepositoryUrl;
use uv_client::{BaseClientBuilder, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, Constraints, DependencyGroups, DevMode, DryRun, EditableMode, ExtrasSpecification,
    InstallOptions, PreviewMode, SourceStrategy,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_distribution_types::{
    Index, IndexName, IndexUrls, NameRequirementSpecification, UnresolvedRequirement, VersionId,
};
use uv_fs::Simplified;
use uv_git::GIT_STORE;
use uv_git_types::GitReference;
use uv_normalize::{PackageName, DEV_DEPENDENCIES};
use uv_pep508::{ExtraName, MarkerTree, Requirement, UnnamedRequirement, VersionOrUrl};
use uv_pypi_types::{redact_credentials, ParsedUrl, RequirementSource, VerbatimParsedUrl};
use uv_python::{Interpreter, PythonDownloads, PythonEnvironment, PythonPreference, PythonRequest};
use uv_requirements::{NamedRequirementsResolver, RequirementsSource, RequirementsSpecification};
use uv_resolver::FlatIndex;
use uv_scripts::{Pep723ItemRef, Pep723Metadata, Pep723Script};
use uv_settings::PythonInstallMirrors;
use uv_types::{BuildIsolation, HashStrategy};
use uv_warnings::warn_user_once;
use uv_workspace::pyproject::{DependencyType, Source, SourceError};
use uv_workspace::pyproject_mut::{ArrayEdit, DependencyTarget, PyProjectTomlMut};
use uv_workspace::{DiscoveryOptions, VirtualProject, Workspace, WorkspaceCache};

use crate::commands::pip::loggers::{
    DefaultInstallLogger, DefaultResolveLogger, SummaryResolveLogger,
};
use crate::commands::pip::operations::Modifications;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::LockMode;
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    init_script_python_requirement, PlatformState, ProjectEnvironment, ProjectError,
    ProjectInterpreter, ScriptInterpreter, UniversalState,
};
use crate::commands::reporters::{PythonDownloadReporter, ResolverReporter};
use crate::commands::{diagnostics, project, ExitStatus, ScriptPath};
use crate::printer::Printer;
use crate::settings::{NetworkSettings, ResolverInstallerSettings};

/// Add one or more packages to the project requirements.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn add(
    project_dir: &Path,
    locked: bool,
    frozen: bool,
    active: Option<bool>,
    no_sync: bool,
    requirements: Vec<RequirementsSource>,
    constraints: Vec<RequirementsSource>,
    marker: Option<MarkerTree>,
    editable: Option<bool>,
    dependency_type: DependencyType,
    raw_sources: bool,
    indexes: Vec<Index>,
    rev: Option<String>,
    tag: Option<String>,
    branch: Option<String>,
    extras: Vec<ExtraName>,
    package: Option<PackageName>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverInstallerSettings,
    network_settings: NetworkSettings,
    script: Option<ScriptPath>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    no_config: bool,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    for source in &requirements {
        match source {
            RequirementsSource::PyprojectToml(_) => {
                bail!("Adding requirements from a `pyproject.toml` is not supported in `uv add`");
            }
            RequirementsSource::SetupPy(_) => {
                bail!("Adding requirements from a `setup.py` is not supported in `uv add`");
            }
            RequirementsSource::SetupCfg(_) => {
                bail!("Adding requirements from a `setup.cfg` is not supported in `uv add`");
            }
            _ => {}
        }
    }

    let reporter = PythonDownloadReporter::single(printer);

    let target = if let Some(script) = script {
        // If we found a PEP 723 script and the user provided a project-only setting, warn.
        if package.is_some() {
            warn_user_once!(
                "`--package` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if locked {
            warn_user_once!(
                "`--locked` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if frozen {
            warn_user_once!(
                "`--frozen` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if no_sync {
            warn_user_once!(
                "`--no-sync` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }

        let client_builder = BaseClientBuilder::new()
            .connectivity(network_settings.connectivity)
            .native_tls(network_settings.native_tls)
            .allow_insecure_host(network_settings.allow_insecure_host.clone());

        // If we found a script, add to the existing metadata. Otherwise, create a new inline
        // metadata tag.
        let script = match script {
            ScriptPath::Script(script) => script,
            ScriptPath::Path(path) => {
                let requires_python = init_script_python_requirement(
                    python.as_deref(),
                    &install_mirrors,
                    project_dir,
                    false,
                    python_preference,
                    python_downloads,
                    no_config,
                    &client_builder,
                    cache,
                    &reporter,
                )
                .await?;
                Pep723Script::init(&path, requires_python.specifiers()).await?
            }
        };

        // Discover the interpreter.
        let interpreter = ScriptInterpreter::discover(
            Pep723ItemRef::Script(&script),
            python.as_deref().map(PythonRequest::parse),
            &network_settings,
            python_preference,
            python_downloads,
            &install_mirrors,
            no_config,
            active,
            cache,
            printer,
        )
        .await?
        .into_interpreter();

        AddTarget::Script(script, Box::new(interpreter))
    } else {
        // Find the project in the workspace.
        // No workspace caching since `uv add` changes the workspace definition.
        let project = if let Some(package) = package {
            VirtualProject::Project(
                Workspace::discover(
                    project_dir,
                    &DiscoveryOptions::default(),
                    &WorkspaceCache::default(),
                )
                .await?
                .with_current_project(package.clone())
                .with_context(|| format!("Package `{package}` not found in workspace"))?,
            )
        } else {
            VirtualProject::discover(
                project_dir,
                &DiscoveryOptions::default(),
                &WorkspaceCache::default(),
            )
            .await?
        };

        // For non-project workspace roots, allow dev dependencies, but nothing else.
        // TODO(charlie): Automatically "upgrade" the project by adding a `[project]` table.
        if project.is_non_project() {
            match dependency_type {
                DependencyType::Production => {
                    bail!("Project is missing a `[project]` table; add a `[project]` table to use production dependencies, or run `{}` instead", "uv add --dev".green())
                }
                DependencyType::Optional(_) => {
                    bail!("Project is missing a `[project]` table; add a `[project]` table to use optional dependencies, or run `{}` instead", "uv add --dev".green())
                }
                DependencyType::Group(_) => {}
                DependencyType::Dev => (),
            }
        }

        if frozen || no_sync {
            // Discover the interpreter.
            let interpreter = ProjectInterpreter::discover(
                project.workspace(),
                project_dir,
                python.as_deref().map(PythonRequest::parse),
                &network_settings,
                python_preference,
                python_downloads,
                &install_mirrors,
                no_config,
                active,
                cache,
                printer,
            )
            .await?
            .into_interpreter();

            AddTarget::Project(project, Box::new(PythonTarget::Interpreter(interpreter)))
        } else {
            // Discover or create the virtual environment.
            let environment = ProjectEnvironment::get_or_init(
                project.workspace(),
                python.as_deref().map(PythonRequest::parse),
                &install_mirrors,
                &network_settings,
                python_preference,
                python_downloads,
                no_config,
                active,
                cache,
                DryRun::Disabled,
                printer,
            )
            .await?
            .into_environment()?;

            AddTarget::Project(project, Box::new(PythonTarget::Environment(environment)))
        }
    };

    let client_builder = BaseClientBuilder::new()
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .keyring(settings.resolver.keyring_provider)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());

    // Read the requirements.
    let RequirementsSpecification {
        requirements,
        constraints,
        ..
    } = RequirementsSpecification::from_sources(
        &requirements,
        &constraints,
        &[],
        BTreeMap::default(),
        &client_builder,
    )
    .await?;

    // Initialize any shared state.
    let state = PlatformState::default();

    // Resolve any unnamed requirements.
    let requirements = {
        // Partition the requirements into named and unnamed requirements.
        let (mut requirements, unnamed): (Vec<_>, Vec<_>) = requirements
            .into_iter()
            .map(|spec| {
                augment_requirement(
                    spec.requirement,
                    rev.as_deref(),
                    tag.as_deref(),
                    branch.as_deref(),
                    marker,
                )
            })
            .partition_map(|requirement| match requirement {
                UnresolvedRequirement::Named(requirement) => itertools::Either::Left(requirement),
                UnresolvedRequirement::Unnamed(requirement) => {
                    itertools::Either::Right(requirement)
                }
            });

        // Resolve any unnamed requirements.
        if !unnamed.is_empty() {
            // TODO(charlie): These are all default values. We should consider whether we want to
            // make them optional on the downstream APIs.
            let build_constraints = Constraints::default();
            let build_hasher = HashStrategy::default();
            let hasher = HashStrategy::default();
            let sources = SourceStrategy::Enabled;

            // Add all authenticated sources to the cache.
            for index in settings.resolver.index_locations.allowed_indexes() {
                if let Some(credentials) = index.credentials() {
                    let credentials = Arc::new(credentials);
                    uv_auth::store_credentials(index.raw_url(), credentials.clone());
                    if let Some(root_url) = index.root_url() {
                        uv_auth::store_credentials(&root_url, credentials.clone());
                    }
                }
            }

            // Initialize the registry client.
            let client = RegistryClientBuilder::try_from(client_builder)?
                .index_urls(settings.resolver.index_locations.index_urls())
                .index_strategy(settings.resolver.index_strategy)
                .markers(target.interpreter().markers())
                .platform(target.interpreter().platform())
                .build();

            // Determine whether to enable build isolation.
            let environment;
            let build_isolation = if settings.resolver.no_build_isolation {
                environment = PythonEnvironment::from_interpreter(target.interpreter().clone());
                BuildIsolation::Shared(&environment)
            } else if settings.resolver.no_build_isolation_package.is_empty() {
                BuildIsolation::Isolated
            } else {
                environment = PythonEnvironment::from_interpreter(target.interpreter().clone());
                BuildIsolation::SharedPackage(
                    &environment,
                    &settings.resolver.no_build_isolation_package,
                )
            };

            // Resolve the flat indexes from `--find-links`.
            let flat_index = {
                let client = FlatIndexClient::new(&client, cache);
                let entries = client
                    .fetch(
                        settings
                            .resolver
                            .index_locations
                            .flat_indexes()
                            .map(Index::url),
                    )
                    .await?;
                FlatIndex::from_entries(entries, None, &hasher, &settings.resolver.build_options)
            };

            // Create a build dispatch.
            let build_dispatch = BuildDispatch::new(
                &client,
                cache,
                build_constraints,
                target.interpreter(),
                &settings.resolver.index_locations,
                &flat_index,
                &settings.resolver.dependency_metadata,
                state.clone().into_inner(),
                settings.resolver.index_strategy,
                &settings.resolver.config_setting,
                build_isolation,
                settings.resolver.link_mode,
                &settings.resolver.build_options,
                &build_hasher,
                settings.resolver.exclude_newer,
                sources,
                // No workspace caching since `uv add` changes the workspace definition.
                WorkspaceCache::default(),
                concurrency,
                preview,
            );

            requirements.extend(
                NamedRequirementsResolver::new(
                    &hasher,
                    state.index(),
                    DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads),
                )
                .with_reporter(Arc::new(ResolverReporter::from(printer)))
                .resolve(unnamed.into_iter())
                .await?,
            );
        }

        requirements
    };

    // If any of the requirements are self-dependencies, bail.
    if matches!(dependency_type, DependencyType::Production) {
        if let AddTarget::Project(project, _) = &target {
            if let Some(project_name) = project.project_name() {
                for requirement in &requirements {
                    if requirement.name == *project_name {
                        bail!(
                            "Requirement name `{}` matches project name `{}`, but self-dependencies are not permitted without the `--dev` or `--optional` flags. If your project name (`{}`) is shadowing that of a third-party dependency, consider renaming the project.",
                            requirement.name.cyan(),
                            project_name.cyan(),
                            project_name.cyan(),
                        );
                    }
                }
            }
        }
    }

    // If the user provides a single, named index, pin all requirements to that index.
    let index = indexes
        .first()
        .as_ref()
        .and_then(|index| index.name.as_ref())
        .filter(|_| indexes.len() == 1)
        .inspect(|index| {
            debug!("Pinning all requirements to index: `{index}`");
        });

    // Add the requirements to the `pyproject.toml` or script.
    let mut toml = match &target {
        AddTarget::Script(script, _) => {
            PyProjectTomlMut::from_toml(&script.metadata.raw, DependencyTarget::Script)
        }
        AddTarget::Project(project, _) => PyProjectTomlMut::from_toml(
            &project.pyproject_toml().raw,
            DependencyTarget::PyProjectToml,
        ),
    }?;
    let mut edits = Vec::<DependencyEdit>::with_capacity(requirements.len());
    for mut requirement in requirements {
        // Add the specified extras.
        requirement.extras.extend(extras.iter().cloned());
        requirement.extras.sort_unstable();
        requirement.extras.dedup();

        let (requirement, source) = match target {
            AddTarget::Script(_, _) | AddTarget::Project(_, _) if raw_sources => {
                (uv_pep508::Requirement::from(requirement), None)
            }
            AddTarget::Script(ref script, _) => {
                let script_path = std::path::absolute(&script.path)?;
                let script_dir = script_path.parent().expect("script path has no parent");
                resolve_requirement(
                    requirement,
                    false,
                    editable,
                    index.cloned(),
                    rev.clone(),
                    tag.clone(),
                    branch.clone(),
                    script_dir,
                )?
            }
            AddTarget::Project(ref project, _) => {
                let workspace = project
                    .workspace()
                    .packages()
                    .contains_key(&requirement.name);
                resolve_requirement(
                    requirement,
                    workspace,
                    editable,
                    index.cloned(),
                    rev.clone(),
                    tag.clone(),
                    branch.clone(),
                    project.root(),
                )?
            }
        };

        // Redact any credentials. By default, we avoid writing sensitive credentials to files that
        // will be checked into version control (e.g., `pyproject.toml` and `uv.lock`). Instead,
        // we store the credentials in a global store, and reuse them during resolution. The
        // expectation is that subsequent resolutions steps will succeed by reading from (e.g.) the
        // user's credentials store, rather than by reading from the `pyproject.toml` file.
        let source = match source {
            Some(Source::Git {
                mut git,
                subdirectory,
                rev,
                tag,
                branch,
                marker,
                extra,
                group,
            }) => {
                let credentials = uv_auth::Credentials::from_url(&git);
                if let Some(credentials) = credentials {
                    debug!("Caching credentials for: {git}");
                    GIT_STORE.insert(RepositoryUrl::new(&git), credentials);

                    // Redact the credentials.
                    redact_credentials(&mut git);
                };
                Some(Source::Git {
                    git,
                    subdirectory,
                    rev,
                    tag,
                    branch,
                    marker,
                    extra,
                    group,
                })
            }
            _ => source,
        };

        // Determine the dependency type.
        let dependency_type = match &dependency_type {
            DependencyType::Dev => {
                let existing = toml.find_dependency(&requirement.name, None);
                if existing.iter().any(|dependency_type| matches!(dependency_type, DependencyType::Group(group) if group == &*DEV_DEPENDENCIES)) {
                    // If the dependency already exists in `dependency-groups.dev`, use that.
                    DependencyType::Group(DEV_DEPENDENCIES.clone())
                } else if existing.iter().any(|dependency_type| matches!(dependency_type, DependencyType::Dev)) {
                    // If the dependency already exists in `dev-dependencies`, use that.
                    DependencyType::Dev
                } else {
                    // Otherwise, use `dependency-groups.dev`, unless it would introduce a separate table.
                    match (toml.has_dev_dependencies(), toml.has_dependency_group(&DEV_DEPENDENCIES)) {
                        (true, false) => DependencyType::Dev,
                        (false, true) => DependencyType::Group(DEV_DEPENDENCIES.clone()),
                        (true, true) => DependencyType::Group(DEV_DEPENDENCIES.clone()),
                        (false, false) => DependencyType::Group(DEV_DEPENDENCIES.clone()),
                    }
                }
            }
            DependencyType::Group(group) if group == &*DEV_DEPENDENCIES => {
                let existing = toml.find_dependency(&requirement.name, None);
                if existing.iter().any(|dependency_type| matches!(dependency_type, DependencyType::Group(group) if group == &*DEV_DEPENDENCIES)) {
                    // If the dependency already exists in `dependency-groups.dev`, use that.
                    DependencyType::Group(DEV_DEPENDENCIES.clone())
                } else if existing.iter().any(|dependency_type| matches!(dependency_type, DependencyType::Dev)) {
                    // If the dependency already exists in `dev-dependencies`, use that.
                    DependencyType::Dev
                } else {
                    // Otherwise, use `dependency-groups.dev`.
                    DependencyType::Group(DEV_DEPENDENCIES.clone())
                }
            }
            DependencyType::Production => DependencyType::Production,
            DependencyType::Optional(extra) => DependencyType::Optional(extra.clone()),
            DependencyType::Group(group) => DependencyType::Group(group.clone()),
        };

        // Update the `pyproject.toml`.
        let edit = match &dependency_type {
            DependencyType::Production => toml.add_dependency(&requirement, source.as_ref())?,
            DependencyType::Dev => toml.add_dev_dependency(&requirement, source.as_ref())?,
            DependencyType::Optional(ref extra) => {
                toml.add_optional_dependency(extra, &requirement, source.as_ref())?
            }
            DependencyType::Group(ref group) => {
                toml.add_dependency_group_requirement(group, &requirement, source.as_ref())?
            }
        };

        // If the edit was inserted before the end of the list, update the existing edits.
        if let ArrayEdit::Add(index) = &edit {
            for edit in &mut edits {
                if edit.dependency_type == dependency_type {
                    match &mut edit.edit {
                        ArrayEdit::Add(existing) => {
                            if *existing >= *index {
                                *existing += 1;
                            }
                        }
                        ArrayEdit::Update(existing) => {
                            if *existing >= *index {
                                *existing += 1;
                            }
                        }
                    }
                }
            }
        }

        edits.push(DependencyEdit {
            dependency_type,
            requirement,
            source,
            edit,
        });
    }

    // Add any indexes that were provided on the command-line, in priority order.
    if !raw_sources {
        let urls = IndexUrls::from_indexes(indexes);
        for index in urls.defined_indexes() {
            toml.add_index(index)?;
        }
    }

    let content = toml.to_string();

    // Save the modified `pyproject.toml` or script.
    let modified = target.write(&content)?;

    // If `--frozen`, exit early. There's no reason to lock and sync, since we don't need a `uv.lock`
    // to exist at all.
    if frozen {
        return Ok(ExitStatus::Success);
    }

    // If we're modifying a script, and lockfile doesn't exist, don't create it.
    if let AddTarget::Script(ref script, _) = target {
        if !LockTarget::from(script).lock_path().is_file() {
            writeln!(
                printer.stderr(),
                "Updated `{}`",
                script.path.user_display().cyan()
            )?;
            return Ok(ExitStatus::Success);
        }
    }

    // Store the content prior to any modifications.
    let snapshot = target.snapshot().await?;

    // Update the `pypackage.toml` in-memory.
    let target = target.update(&content)?;

    // Set the Ctrl-C handler to revert changes on exit.
    let _ = ctrlc::set_handler({
        let snapshot = snapshot.clone();
        move || {
            if modified {
                let _ = snapshot.revert();
            }

            #[allow(clippy::exit, clippy::cast_possible_wrap)]
            std::process::exit(if cfg!(windows) {
                0xC000_013A_u32 as i32
            } else {
                130
            });
        }
    });

    // Use separate state for locking and syncing.
    let lock_state = state.fork();
    let sync_state = state;

    match Box::pin(lock_and_sync(
        target,
        &mut toml,
        &edits,
        lock_state,
        sync_state,
        locked,
        &dependency_type,
        raw_sources,
        constraints,
        &settings,
        &network_settings,
        installer_metadata,
        concurrency,
        cache,
        printer,
        preview,
    ))
    .await
    {
        Ok(()) => Ok(ExitStatus::Success),
        Err(err) => {
            if modified {
                let _ = snapshot.revert();
            }
            match err {
                ProjectError::Operation(err) => diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls).with_hint(format!("If you want to add the package regardless of the failed resolution, provide the `{}` flag to skip locking and syncing.", "--frozen".green()))
                    .report(err)
                    .map_or(Ok(ExitStatus::Failure), |err| Err(err.into())),
                err => Err(err.into()),
            }
        }
    }
}

/// Re-lock and re-sync the project after a series of edits.
#[allow(clippy::fn_params_excessive_bools)]
async fn lock_and_sync(
    mut target: AddTarget,
    toml: &mut PyProjectTomlMut,
    edits: &[DependencyEdit],
    lock_state: UniversalState,
    sync_state: PlatformState,
    locked: bool,
    dependency_type: &DependencyType,
    raw_sources: bool,
    constraints: Vec<NameRequirementSpecification>,
    settings: &ResolverInstallerSettings,
    network_settings: &NetworkSettings,
    installer_metadata: bool,
    concurrency: Concurrency,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<(), ProjectError> {
    let mut lock = project::lock::LockOperation::new(
        if locked {
            LockMode::Locked(target.interpreter())
        } else {
            LockMode::Write(target.interpreter())
        },
        &settings.resolver,
        network_settings,
        &lock_state,
        Box::new(DefaultResolveLogger),
        concurrency,
        cache,
        printer,
        preview,
    )
    .with_constraints(constraints)
    .execute((&target).into())
    .await?
    .into_lock();

    // Avoid modifying the user request further if `--raw-sources` is set.
    if !raw_sources {
        // Extract the minimum-supported version for each dependency.
        let mut minimum_version =
            FxHashMap::with_capacity_and_hasher(lock.packages().len(), FxBuildHasher);
        for dist in lock.packages() {
            let name = dist.name();
            let Some(version) = dist.version() else {
                continue;
            };
            match minimum_version.entry(name) {
                Entry::Vacant(entry) => {
                    entry.insert(version);
                }
                Entry::Occupied(mut entry) => {
                    if version < *entry.get() {
                        entry.insert(version);
                    }
                }
            }
        }

        // If any of the requirements were added without version specifiers, add a lower bound.
        let mut modified = false;
        for edit in edits {
            // Only set a minimum version for newly-added dependencies (as opposed to updates).
            let ArrayEdit::Add(index) = &edit.edit else {
                continue;
            };

            // Only set a minimum version for registry requirements.
            if edit
                .source
                .as_ref()
                .is_some_and(|source| !matches!(source, Source::Registry { .. }))
            {
                continue;
            }

            // Only set a minimum version for registry requirements.
            let is_empty = match edit.requirement.version_or_url.as_ref() {
                Some(VersionOrUrl::VersionSpecifier(version)) => version.is_empty(),
                Some(VersionOrUrl::Url(_)) => false,
                None => true,
            };
            if !is_empty {
                continue;
            }

            // Set the minimum version.
            let Some(minimum) = minimum_version.get(&edit.requirement.name) else {
                continue;
            };

            // Drop the local version identifier, which isn't permitted in `>=` constraints.
            // For example, convert `1.2.3+local` to `1.2.3`.
            let minimum = (*minimum).clone().without_local();

            match edit.dependency_type {
                DependencyType::Production => {
                    toml.set_dependency_minimum_version(*index, minimum)?;
                }
                DependencyType::Dev => {
                    toml.set_dev_dependency_minimum_version(*index, minimum)?;
                }
                DependencyType::Optional(ref extra) => {
                    toml.set_optional_dependency_minimum_version(extra, *index, minimum)?;
                }
                DependencyType::Group(ref group) => {
                    toml.set_dependency_group_requirement_minimum_version(group, *index, minimum)?;
                }
            }

            modified = true;
        }

        // Save the modified `pyproject.toml`. No need to check for changes in the underlying
        // string content, since the above loop _must_ change an empty specifier to a non-empty
        // specifier.
        if modified {
            let content = toml.to_string();

            // Write the updated `pyproject.toml` to disk.
            target.write(&content)?;

            // Update the `pypackage.toml` in-memory.
            target = target.update(&content)?;

            // Invalidate the project metadata.
            if let AddTarget::Project(VirtualProject::Project(ref project), _) = target {
                let url = Url::from_file_path(project.project_root())
                    .expect("project root is a valid URL");
                let version_id = VersionId::from_url(&url);
                let existing = lock_state.index().distributions().remove(&version_id);
                debug_assert!(existing.is_some(), "distribution should exist");
            }

            // If the file was modified, we have to lock again, though the only expected change is
            // the addition of the minimum version specifiers.
            lock = project::lock::LockOperation::new(
                if locked {
                    LockMode::Locked(target.interpreter())
                } else {
                    LockMode::Write(target.interpreter())
                },
                &settings.resolver,
                network_settings,
                &lock_state,
                Box::new(SummaryResolveLogger),
                concurrency,
                cache,
                printer,
                preview,
            )
            .execute((&target).into())
            .await?
            .into_lock();
        }
    }

    let AddTarget::Project(project, environment) = target else {
        // If we're not adding to a project, exit early.
        return Ok(());
    };

    let PythonTarget::Environment(venv) = &*environment else {
        // If we're not syncing, exit early.
        return Ok(());
    };

    // Sync the environment.
    let (extras, dev) = match dependency_type {
        DependencyType::Production => {
            let extras = ExtrasSpecification::None;
            let dev = DependencyGroups::from_dev_mode(DevMode::Exclude);
            (extras, dev)
        }
        DependencyType::Dev => {
            let extras = ExtrasSpecification::None;
            let dev = DependencyGroups::from_dev_mode(DevMode::Include);
            (extras, dev)
        }
        DependencyType::Optional(ref extra_name) => {
            let extras = ExtrasSpecification::Some(vec![extra_name.clone()]);
            let dev = DependencyGroups::from_dev_mode(DevMode::Exclude);
            (extras, dev)
        }
        DependencyType::Group(ref group_name) => {
            let extras = ExtrasSpecification::None;
            let dev = DependencyGroups::from_group(group_name.clone());
            (extras, dev)
        }
    };

    // Identify the installation target.
    let target = match &project {
        VirtualProject::Project(project) => InstallTarget::Project {
            workspace: project.workspace(),
            name: project.project_name(),
            lock: &lock,
        },
        VirtualProject::NonProject(workspace) => InstallTarget::NonProjectWorkspace {
            workspace,
            lock: &lock,
        },
    };

    project::sync::do_sync(
        target,
        venv,
        &extras,
        &dev.with_defaults(Vec::new()),
        EditableMode::Editable,
        InstallOptions::default(),
        Modifications::Sufficient,
        settings.into(),
        network_settings,
        &sync_state,
        Box::new(DefaultInstallLogger),
        installer_metadata,
        concurrency,
        cache,
        DryRun::Disabled,
        printer,
        preview,
    )
    .await?;

    Ok(())
}

/// Augment a user-provided requirement by attaching any specification data that was provided
/// separately from the requirement itself (e.g., `--branch main`).
fn augment_requirement(
    requirement: UnresolvedRequirement,
    rev: Option<&str>,
    tag: Option<&str>,
    branch: Option<&str>,
    marker: Option<MarkerTree>,
) -> UnresolvedRequirement {
    match requirement {
        UnresolvedRequirement::Named(mut requirement) => {
            UnresolvedRequirement::Named(uv_pypi_types::Requirement {
                marker: marker
                    .map(|marker| {
                        requirement.marker.and(marker);
                        requirement.marker
                    })
                    .unwrap_or(requirement.marker),
                source: match requirement.source {
                    RequirementSource::Git {
                        git,
                        subdirectory,
                        url,
                    } => {
                        let git = if let Some(rev) = rev {
                            git.with_reference(GitReference::from_rev(rev.to_string()))
                        } else if let Some(tag) = tag {
                            git.with_reference(GitReference::Tag(tag.to_string()))
                        } else if let Some(branch) = branch {
                            git.with_reference(GitReference::Branch(branch.to_string()))
                        } else {
                            git
                        };
                        RequirementSource::Git {
                            git,
                            subdirectory,
                            url,
                        }
                    }
                    _ => requirement.source,
                },
                ..requirement
            })
        }
        UnresolvedRequirement::Unnamed(mut requirement) => {
            UnresolvedRequirement::Unnamed(UnnamedRequirement {
                marker: marker
                    .map(|marker| {
                        requirement.marker.and(marker);
                        requirement.marker
                    })
                    .unwrap_or(requirement.marker),
                url: match requirement.url.parsed_url {
                    ParsedUrl::Git(mut git) => {
                        let reference = if let Some(rev) = rev {
                            Some(GitReference::from_rev(rev.to_string()))
                        } else if let Some(tag) = tag {
                            Some(GitReference::Tag(tag.to_string()))
                        } else {
                            branch.map(|branch| GitReference::Branch(branch.to_string()))
                        };
                        if let Some(reference) = reference {
                            git.url = git.url.with_reference(reference);
                        }
                        VerbatimParsedUrl {
                            parsed_url: ParsedUrl::Git(git),
                            verbatim: requirement.url.verbatim,
                        }
                    }
                    _ => requirement.url,
                },
                ..requirement
            })
        }
    }
}

/// Resolves the source for a requirement and processes it into a PEP 508 compliant format.
fn resolve_requirement(
    requirement: uv_pypi_types::Requirement,
    workspace: bool,
    editable: Option<bool>,
    index: Option<IndexName>,
    rev: Option<String>,
    tag: Option<String>,
    branch: Option<String>,
    root: &Path,
) -> Result<(Requirement, Option<Source>), anyhow::Error> {
    let result = Source::from_requirement(
        &requirement.name,
        requirement.source.clone(),
        workspace,
        editable,
        index,
        rev,
        tag,
        branch,
        root,
    );

    let source = match result {
        Ok(source) => source,
        Err(SourceError::UnresolvedReference(rev)) => {
            bail!(
                "Cannot resolve Git reference `{rev}` for requirement `{name}`. Specify the reference with one of `--tag`, `--branch`, or `--rev`, or use the `--raw-sources` flag.",
                name = requirement.name
            )
        }
        Err(err) => return Err(err.into()),
    };

    // Ignore the PEP 508 source by clearing the URL.
    let mut processed_requirement = uv_pep508::Requirement::from(requirement);
    processed_requirement.clear_url();

    Ok((processed_requirement, source))
}

/// A Python [`Interpreter`] or [`PythonEnvironment`] for a project.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub(super) enum PythonTarget {
    Interpreter(Interpreter),
    Environment(PythonEnvironment),
}

impl PythonTarget {
    /// Return the [`Interpreter`] for the project.
    fn interpreter(&self) -> &Interpreter {
        match self {
            Self::Interpreter(interpreter) => interpreter,
            Self::Environment(venv) => venv.interpreter(),
        }
    }
}

/// Represents the destination where dependencies are added, either to a project or a script.
#[derive(Debug, Clone)]
pub(super) enum AddTarget {
    /// A PEP 723 script, with inline metadata.
    Script(Pep723Script, Box<Interpreter>),

    /// A project with a `pyproject.toml`.
    Project(VirtualProject, Box<PythonTarget>),
}

impl<'lock> From<&'lock AddTarget> for LockTarget<'lock> {
    fn from(value: &'lock AddTarget) -> Self {
        match value {
            AddTarget::Script(script, _) => Self::Script(script),
            AddTarget::Project(project, _) => Self::Workspace(project.workspace()),
        }
    }
}

impl AddTarget {
    /// Returns the [`Interpreter`] for the target.
    pub(super) fn interpreter(&self) -> &Interpreter {
        match self {
            Self::Script(_, interpreter) => interpreter,
            Self::Project(_, venv) => venv.interpreter(),
        }
    }

    /// Write the updated content to the target.
    ///
    /// Returns `true` if the content was modified.
    fn write(&self, content: &str) -> Result<bool, io::Error> {
        match self {
            Self::Script(script, _) => {
                if content == script.metadata.raw {
                    debug!("No changes to dependencies; skipping update");
                    Ok(false)
                } else {
                    script.write(content)?;
                    Ok(true)
                }
            }
            Self::Project(project, _) => {
                if content == project.pyproject_toml().raw {
                    debug!("No changes to dependencies; skipping update");
                    Ok(false)
                } else {
                    let pyproject_path = project.root().join("pyproject.toml");
                    fs_err::write(pyproject_path, content)?;
                    Ok(true)
                }
            }
        }
    }

    /// Update the target in-memory to incorporate the new content.
    #[allow(clippy::result_large_err)]
    fn update(self, content: &str) -> Result<Self, ProjectError> {
        match self {
            Self::Script(mut script, interpreter) => {
                script.metadata = Pep723Metadata::from_str(content)
                    .map_err(ProjectError::Pep723ScriptTomlParse)?;
                Ok(Self::Script(script, interpreter))
            }
            Self::Project(project, venv) => {
                let project = project
                    .with_pyproject_toml(
                        toml::from_str(content).map_err(ProjectError::PyprojectTomlParse)?,
                    )
                    .ok_or(ProjectError::PyprojectTomlUpdate)?;
                Ok(Self::Project(project, venv))
            }
        }
    }

    /// Take a snapshot of the target.
    async fn snapshot(&self) -> Result<AddTargetSnapshot, io::Error> {
        // Read the lockfile into memory.
        let target = match self {
            Self::Script(script, _) => LockTarget::from(script),
            Self::Project(project, _) => LockTarget::Workspace(project.workspace()),
        };
        let lock = target.read_bytes().await?;

        // Clone the target.
        match self {
            Self::Script(script, _) => Ok(AddTargetSnapshot::Script(script.clone(), lock)),
            Self::Project(project, _) => Ok(AddTargetSnapshot::Project(project.clone(), lock)),
        }
    }
}

#[derive(Debug, Clone)]
enum AddTargetSnapshot {
    Script(Pep723Script, Option<Vec<u8>>),
    Project(VirtualProject, Option<Vec<u8>>),
}

impl AddTargetSnapshot {
    /// Write the snapshot back to disk (e.g., to a `pyproject.toml` and `uv.lock`).
    fn revert(&self) -> Result<(), io::Error> {
        match self {
            Self::Script(script, lock) => {
                // Write the PEP 723 script back to disk.
                debug!("Reverting changes to PEP 723 script block");
                script.write(&script.metadata.raw)?;

                // Write the lockfile back to disk.
                let target = LockTarget::from(script);
                if let Some(lock) = lock {
                    debug!("Reverting changes to `uv.lock`");
                    fs_err::write(target.lock_path(), lock)?;
                } else {
                    debug!("Removing `uv.lock`");
                    fs_err::remove_file(target.lock_path())?;
                }
                Ok(())
            }
            Self::Project(project, lock) => {
                // Write the `pyproject.toml` back to disk.
                debug!("Reverting changes to `pyproject.toml`");
                fs_err::write(
                    project.root().join("pyproject.toml"),
                    project.pyproject_toml().as_ref(),
                )?;

                // Write the lockfile back to disk.
                let target = LockTarget::from(project.workspace());
                if let Some(lock) = lock {
                    debug!("Reverting changes to `uv.lock`");
                    fs_err::write(target.lock_path(), lock)?;
                } else {
                    debug!("Removing `uv.lock`");
                    fs_err::remove_file(target.lock_path())?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone)]
struct DependencyEdit {
    dependency_type: DependencyType,
    requirement: Requirement,
    source: Option<Source>,
    edit: ArrayEdit,
}
