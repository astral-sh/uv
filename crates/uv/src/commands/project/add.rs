use std::collections::hash_map::Entry;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::{FxBuildHasher, FxHashMap};
use tracing::debug;
use url::Url;

use uv_cache::Cache;
use uv_cache_key::RepositoryUrl;
use uv_client::{BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, Constraints, DevMode, EditableMode, ExtrasSpecification, InstallOptions,
    LowerBound, SourceStrategy, TrustedHost,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_distribution_types::{Index, IndexName, UnresolvedRequirement, VersionId};
use uv_fs::Simplified;
use uv_git::{GitReference, GIT_STORE};
use uv_normalize::PackageName;
use uv_pep508::{ExtraName, Requirement, UnnamedRequirement, VersionOrUrl};
use uv_pypi_types::{redact_credentials, ParsedUrl, RequirementSource, VerbatimParsedUrl};
use uv_python::{
    EnvironmentPreference, Interpreter, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest, PythonVariant, PythonVersionFile, VersionRequest,
};
use uv_requirements::{NamedRequirementsResolver, RequirementsSource, RequirementsSpecification};
use uv_resolver::FlatIndex;
use uv_scripts::Pep723Script;
use uv_types::{BuildIsolation, HashStrategy};
use uv_warnings::warn_user_once;
use uv_workspace::pyproject::{DependencyType, Source, SourceError};
use uv_workspace::pyproject_mut::{ArrayEdit, DependencyTarget, PyProjectTomlMut};
use uv_workspace::{DiscoveryOptions, InstallTarget, VirtualProject, Workspace};

use crate::commands::pip::loggers::{
    DefaultInstallLogger, DefaultResolveLogger, SummaryResolveLogger,
};
use crate::commands::pip::operations::Modifications;
use crate::commands::pip::resolution_environment;
use crate::commands::project::{script_python_requirement, ProjectError};
use crate::commands::reporters::{PythonDownloadReporter, ResolverReporter};
use crate::commands::{diagnostics, pip, project, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::{ResolverInstallerSettings, ResolverInstallerSettingsRef};

/// Add one or more packages to the project requirements.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn add(
    project_dir: &Path,
    locked: bool,
    frozen: bool,
    no_sync: bool,
    requirements: Vec<RequirementsSource>,
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
    settings: ResolverInstallerSettings,
    script: Option<PathBuf>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
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
            RequirementsSource::RequirementsTxt(path) => {
                if path == Path::new("-") {
                    bail!("Reading requirements from stdin is not supported in `uv add`");
                }
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
            .connectivity(connectivity)
            .native_tls(native_tls)
            .allow_insecure_host(allow_insecure_host.to_vec());

        // If we found a script, add to the existing metadata. Otherwise, create a new inline
        // metadata tag.
        let script = if let Some(script) = Pep723Script::read(&script).await? {
            script
        } else {
            let requires_python = script_python_requirement(
                python.as_deref(),
                project_dir,
                false,
                python_preference,
                python_downloads,
                &client_builder,
                cache,
                &reporter,
            )
            .await?;
            Pep723Script::init(&script, requires_python.specifiers()).await?
        };

        let python_request = if let Some(request) = python.as_deref() {
            // (1) Explicit request from user
            Some(PythonRequest::parse(request))
        } else if let Some(request) = PythonVersionFile::discover(project_dir, false, false)
            .await?
            .and_then(PythonVersionFile::into_version)
        {
            // (2) Request from `.python-version`
            Some(request)
        } else {
            // (3) `Requires-Python` in `pyproject.toml`
            script
                .metadata
                .requires_python
                .clone()
                .map(|requires_python| {
                    PythonRequest::Version(VersionRequest::Range(
                        requires_python,
                        PythonVariant::Default,
                    ))
                })
        };

        let interpreter = PythonInstallation::find_or_download(
            python_request.as_ref(),
            EnvironmentPreference::Any,
            python_preference,
            python_downloads,
            &client_builder,
            cache,
            Some(&reporter),
        )
        .await?
        .into_interpreter();

        Target::Script(script, Box::new(interpreter))
    } else {
        // Find the project in the workspace.
        let project = if let Some(package) = package {
            VirtualProject::Project(
                Workspace::discover(project_dir, &DiscoveryOptions::default())
                    .await?
                    .with_current_project(package.clone())
                    .with_context(|| format!("Package `{package}` not found in workspace"))?,
            )
        } else {
            VirtualProject::discover(project_dir, &DiscoveryOptions::default()).await?
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
                DependencyType::Dev => (),
            }
        }

        // Discover or create the virtual environment.
        let venv = project::get_or_init_environment(
            project.workspace(),
            python.as_deref().map(PythonRequest::parse),
            python_preference,
            python_downloads,
            connectivity,
            native_tls,
            allow_insecure_host,
            cache,
            printer,
        )
        .await?;

        Target::Project(project, venv)
    };

    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .keyring(settings.keyring_provider)
        .allow_insecure_host(allow_insecure_host.to_vec());

    // Read the requirements.
    let RequirementsSpecification { requirements, .. } =
        RequirementsSpecification::from_simple_sources(&requirements, &client_builder).await?;

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let bounds = LowerBound::default();
    let build_constraints = Constraints::default();
    let build_hasher = HashStrategy::default();
    let hasher = HashStrategy::default();
    let python_platform = None;
    let python_version = None;
    let sources = SourceStrategy::Enabled;

    // Determine the environment for the resolution.
    let (tags, markers) =
        resolution_environment(python_version, python_platform, target.interpreter())?;

    // Add all authenticated sources to the cache.
    for index in settings.index_locations.allowed_indexes() {
        if let Some(credentials) = index.credentials() {
            uv_auth::store_credentials(index.raw_url(), credentials);
        }
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::try_from(client_builder)?
        .index_urls(settings.index_locations.index_urls())
        .index_strategy(settings.index_strategy)
        .markers(&markers)
        .platform(target.interpreter().platform())
        .build();

    // Determine whether to enable build isolation.
    let environment;
    let build_isolation = if settings.no_build_isolation {
        environment = PythonEnvironment::from_interpreter(target.interpreter().clone());
        BuildIsolation::Shared(&environment)
    } else if settings.no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        environment = PythonEnvironment::from_interpreter(target.interpreter().clone());
        BuildIsolation::SharedPackage(&environment, &settings.no_build_isolation_package)
    };

    // Initialize any shared state.
    let state = SharedState::default();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client
            .fetch(settings.index_locations.flat_indexes().map(Index::url))
            .await?;
        FlatIndex::from_entries(entries, Some(&tags), &hasher, &settings.build_options)
    };

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        target.interpreter(),
        &settings.index_locations,
        &flat_index,
        &settings.dependency_metadata,
        &state.index,
        &state.git,
        &state.capabilities,
        &state.in_flight,
        settings.index_strategy,
        &settings.config_setting,
        build_isolation,
        settings.link_mode,
        &settings.build_options,
        &build_hasher,
        settings.exclude_newer,
        bounds,
        sources,
        concurrency,
    );

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
            requirements.extend(
                NamedRequirementsResolver::new(
                    &hasher,
                    &state.index,
                    DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads),
                )
                .with_reporter(ResolverReporter::from(printer))
                .resolve(unnamed.into_iter())
                .await?,
            );
        }

        requirements
    };

    // If any of the requirements are self-dependencies, bail.
    if matches!(dependency_type, DependencyType::Production) {
        if let Target::Project(project, _) = &target {
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
        Target::Script(script, _) => {
            PyProjectTomlMut::from_toml(&script.metadata.raw, DependencyTarget::Script)
        }
        Target::Project(project, _) => PyProjectTomlMut::from_toml(
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
            Target::Script(_, _) | Target::Project(_, _) if raw_sources => {
                (uv_pep508::Requirement::from(requirement), None)
            }
            Target::Script(ref script, _) => {
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
            Target::Project(ref project, _) => {
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
                })
            }
            _ => source,
        };

        // Update the `pyproject.toml`.
        let edit = match dependency_type {
            DependencyType::Production => toml.add_dependency(&requirement, source.as_ref())?,
            DependencyType::Dev => toml.add_dev_dependency(&requirement, source.as_ref())?,
            DependencyType::Optional(ref group) => {
                toml.add_optional_dependency(group, &requirement, source.as_ref())?
            }
        };

        // If the edit was inserted before the end of the list, update the existing edits.
        if let ArrayEdit::Add(index) = &edit {
            for edit in &mut edits {
                if *edit.dependency_type == dependency_type {
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
            dependency_type: &dependency_type,
            requirement,
            source,
            edit,
        });
    }

    // Add any indexes that were provided on the command-line.
    if !raw_sources {
        for index in indexes {
            toml.add_index(&index)?;
        }
    }

    let content = toml.to_string();

    // Save the modified `pyproject.toml` or script.
    let modified = match &target {
        Target::Script(script, _) => {
            if content == script.metadata.raw {
                debug!("No changes to dependencies; skipping update");
                false
            } else {
                script.write(&content).await?;
                true
            }
        }
        Target::Project(project, _) => {
            if content == *project.pyproject_toml().raw {
                debug!("No changes to dependencies; skipping update");
                false
            } else {
                let pyproject_path = project.root().join("pyproject.toml");
                fs_err::write(pyproject_path, &content)?;
                true
            }
        }
    };

    let (project, venv) = match target {
        Target::Project(project, venv) => (project, venv),
        // If `--script`, exit early. There's no reason to lock and sync.
        Target::Script(script, _) => {
            writeln!(
                printer.stderr(),
                "Updated `{}`",
                script.path.user_display().cyan()
            )?;
            return Ok(ExitStatus::Success);
        }
    };

    // If `--frozen`, exit early. There's no reason to lock and sync, and we don't need a `uv.lock`
    // to exist at all.
    if frozen {
        return Ok(ExitStatus::Success);
    }

    // Store the content prior to any modifications.
    let existing = project.pyproject_toml().as_ref().to_vec();
    let root = project.root().to_path_buf();

    // Update the `pypackage.toml` in-memory.
    let project = project
        .with_pyproject_toml(toml::from_str(&content).map_err(ProjectError::TomlParse)?)
        .ok_or(ProjectError::TomlUpdate)?;

    // Set the Ctrl-C handler to revert changes on exit.
    let _ = ctrlc::set_handler({
        let root = root.clone();
        let existing = existing.clone();
        move || {
            // Revert the changes to the `pyproject.toml`, if necessary.
            if modified {
                let _ = fs_err::write(root.join("pyproject.toml"), &existing);
            }

            #[allow(clippy::exit, clippy::cast_possible_wrap)]
            std::process::exit(if cfg!(windows) {
                0xC000_013A_u32 as i32
            } else {
                130
            });
        }
    });

    match lock_and_sync(
        project,
        &mut toml,
        &edits,
        &venv,
        state,
        locked,
        frozen,
        no_sync,
        &dependency_type,
        raw_sources,
        settings.as_ref(),
        bounds,
        connectivity,
        concurrency,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
    )
    .await
    {
        Ok(()) => Ok(ExitStatus::Success),
        Err(err) => {
            // Revert the changes to the `pyproject.toml`, if necessary.
            if modified {
                fs_err::write(root.join("pyproject.toml"), &existing)?;
            }

            match err {
                ProjectError::Operation(pip::operations::Error::Resolve(
                    uv_resolver::ResolveError::NoSolution(err),
                )) => {
                    diagnostics::no_solution_hint(err, format!("If you want to add the package regardless of the failed resolution, provide the `{}` flag to skip locking and syncing.", "--frozen".green()));
                    Ok(ExitStatus::Failure)
                }
                ProjectError::Operation(pip::operations::Error::Resolve(
                    uv_resolver::ResolveError::FetchAndBuild(dist, err),
                )) => {
                    diagnostics::fetch_and_build(dist, err);
                    Ok(ExitStatus::Failure)
                }
                ProjectError::Operation(pip::operations::Error::Resolve(
                    uv_resolver::ResolveError::Build(dist, err),
                )) => {
                    diagnostics::build(dist, err);
                    Ok(ExitStatus::Failure)
                }
                err => {
                    // Revert the changes to the `pyproject.toml`, if necessary.
                    if modified {
                        fs_err::write(root.join("pyproject.toml"), &existing)?;
                    }
                    Err(err.into())
                }
            }
        }
    }
}

/// Re-lock and re-sync the project after a series of edits.
#[allow(clippy::fn_params_excessive_bools)]
async fn lock_and_sync(
    mut project: VirtualProject,
    toml: &mut PyProjectTomlMut,
    edits: &[DependencyEdit<'_>],
    venv: &PythonEnvironment,
    state: SharedState,
    locked: bool,
    frozen: bool,
    no_sync: bool,
    dependency_type: &DependencyType,
    raw_sources: bool,
    settings: ResolverInstallerSettingsRef<'_>,
    bounds: LowerBound,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
) -> Result<(), ProjectError> {
    let mut lock = project::lock::do_safe_lock(
        locked,
        frozen,
        project.workspace(),
        venv.interpreter(),
        settings.into(),
        bounds,
        &state,
        Box::new(DefaultResolveLogger),
        connectivity,
        concurrency,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
    )
    .await?
    .into_lock();

    // Avoid modifying the user request further if `--raw-sources` is set.
    if !raw_sources {
        // Extract the minimum-supported version for each dependency.
        let mut minimum_version =
            FxHashMap::with_capacity_and_hasher(lock.packages().len(), FxBuildHasher);
        for dist in lock.packages() {
            let name = dist.name();
            let version = dist.version();
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
                DependencyType::Optional(ref group) => {
                    toml.set_optional_dependency_minimum_version(group, *index, minimum)?;
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
            fs_err::write(project.root().join("pyproject.toml"), &content)?;

            // Update the `pypackage.toml` in-memory.
            project = project
                .with_pyproject_toml(toml::from_str(&content).map_err(ProjectError::TomlParse)?)
                .ok_or(ProjectError::TomlUpdate)?;

            // Invalidate the project metadata.
            if let VirtualProject::Project(ref project) = project {
                let url = Url::from_file_path(project.project_root())
                    .expect("project root is a valid URL");
                let version_id = VersionId::from_url(&url);
                let existing = state.index.distributions().remove(&version_id);
                debug_assert!(existing.is_some(), "distribution should exist");
            }

            // If the file was modified, we have to lock again, though the only expected change is
            // the addition of the minimum version specifiers.
            lock = project::lock::do_safe_lock(
                locked,
                frozen,
                project.workspace(),
                venv.interpreter(),
                settings.into(),
                bounds,
                &state,
                Box::new(SummaryResolveLogger),
                connectivity,
                concurrency,
                native_tls,
                allow_insecure_host,
                cache,
                printer,
            )
            .await?
            .into_lock();
        }
    }

    if no_sync {
        return Ok(());
    }

    // Sync the environment.
    let (extras, dev) = match dependency_type {
        DependencyType::Production => {
            let extras = ExtrasSpecification::None;
            let dev = DevMode::Exclude;
            (extras, dev)
        }
        DependencyType::Dev => {
            let extras = ExtrasSpecification::None;
            let dev = DevMode::Include;
            (extras, dev)
        }
        DependencyType::Optional(ref group_name) => {
            let extras = ExtrasSpecification::Some(vec![group_name.clone()]);
            let dev = DevMode::Exclude;
            (extras, dev)
        }
    };

    project::sync::do_sync(
        InstallTarget::from(&project),
        venv,
        &lock,
        &extras,
        dev,
        EditableMode::Editable,
        InstallOptions::default(),
        Modifications::Sufficient,
        settings.into(),
        Box::new(DefaultInstallLogger),
        connectivity,
        concurrency,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
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
) -> UnresolvedRequirement {
    match requirement {
        UnresolvedRequirement::Named(requirement) => {
            UnresolvedRequirement::Named(uv_pypi_types::Requirement {
                source: match requirement.source {
                    RequirementSource::Git {
                        repository,
                        reference,
                        precise,
                        subdirectory,
                        url,
                    } => {
                        let reference = if let Some(rev) = rev {
                            GitReference::from_rev(rev.to_string())
                        } else if let Some(tag) = tag {
                            GitReference::Tag(tag.to_string())
                        } else if let Some(branch) = branch {
                            GitReference::Branch(branch.to_string())
                        } else {
                            reference
                        };
                        RequirementSource::Git {
                            repository,
                            reference,
                            precise,
                            subdirectory,
                            url,
                        }
                    }
                    _ => requirement.source,
                },
                ..requirement
            })
        }
        UnresolvedRequirement::Unnamed(requirement) => {
            UnresolvedRequirement::Unnamed(UnnamedRequirement {
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

/// Represents the destination where dependencies are added, either to a project or a script.
#[derive(Debug)]
enum Target {
    /// A PEP 723 script, with inline metadata.
    Script(Pep723Script, Box<Interpreter>),
    /// A project with a `pyproject.toml`.
    Project(VirtualProject, PythonEnvironment),
}

impl Target {
    /// Returns the [`Interpreter`] for the target.
    fn interpreter(&self) -> &Interpreter {
        match self {
            Self::Script(_, interpreter) => interpreter,
            Self::Project(_, venv) => venv.interpreter(),
        }
    }
}

#[derive(Debug, Clone)]
struct DependencyEdit<'a> {
    dependency_type: &'a DependencyType,
    requirement: Requirement,
    source: Option<Source>,
    edit: ArrayEdit,
}
