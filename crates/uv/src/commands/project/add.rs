use std::collections::hash_map::Entry;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use cache_key::RepositoryUrl;
use owo_colors::OwoColorize;
use pep508_rs::{ExtraName, Requirement, VersionOrUrl};
use rustc_hash::{FxBuildHasher, FxHashMap};
use tracing::debug;
use uv_auth::{store_credentials_from_url, Credentials};
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{Concurrency, ExtrasSpecification, SetupPyStrategy, SourceStrategy};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_fs::CWD;
use uv_git::GIT_STORE;
use uv_normalize::PackageName;
use uv_python::{
    request_from_version_file, EnvironmentPreference, Interpreter, PythonDownloads,
    PythonEnvironment, PythonInstallation, PythonPreference, PythonRequest, VersionRequest,
};
use uv_requirements::{NamedRequirementsResolver, RequirementsSource, RequirementsSpecification};
use uv_resolver::{FlatIndex, RequiresPython};
use uv_scripts::Pep723Script;
use uv_types::{BuildIsolation, HashStrategy};
use uv_warnings::warn_user_once;
use uv_workspace::pyproject::{DependencyType, Source, SourceError};
use uv_workspace::pyproject_mut::{ArrayEdit, DependencyTarget, PyProjectTomlMut};
use uv_workspace::{DiscoveryOptions, VirtualProject, Workspace};

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::pip::resolution_environment;
use crate::commands::project::ProjectError;
use crate::commands::reporters::{PythonDownloadReporter, ResolverReporter};
use crate::commands::{pip, project, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Add one or more packages to the project requirements.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn add(
    locked: bool,
    frozen: bool,
    no_sync: bool,
    requirements: Vec<RequirementsSource>,
    editable: Option<bool>,
    dependency_type: DependencyType,
    raw_sources: bool,
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
                "`--no_sync` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }

        let client_builder = BaseClientBuilder::new()
            .connectivity(connectivity)
            .native_tls(native_tls);

        // If we found a script, add to the existing metadata. Otherwise, create a new inline
        // metadata tag.
        let script = if let Some(script) = Pep723Script::read(&script).await? {
            script
        } else {
            let python_request = if let Some(request) = python.as_deref() {
                // (1) Explicit request from user
                PythonRequest::parse(request)
            } else if let Some(request) = request_from_version_file(&CWD).await? {
                // (2) Request from `.python-version`
                request
            } else {
                // (3) Assume any Python version
                PythonRequest::Any
            };

            let interpreter = PythonInstallation::find_or_download(
                Some(python_request),
                EnvironmentPreference::Any,
                python_preference,
                python_downloads,
                &client_builder,
                cache,
                Some(&reporter),
            )
            .await?
            .into_interpreter();

            let requires_python =
                RequiresPython::greater_than_equal_version(&interpreter.python_minor_version());
            Pep723Script::create(&script, requires_python.specifiers()).await?
        };

        let python_request = if let Some(request) = python.as_deref() {
            // (1) Explicit request from user
            Some(PythonRequest::parse(request))
        } else if let Some(request) = request_from_version_file(&CWD).await? {
            // (2) Request from `.python-version`
            Some(request)
        } else {
            // (3) `Requires-Python` in `pyproject.toml`
            script
                .metadata
                .requires_python
                .clone()
                .map(|requires_python| {
                    PythonRequest::Version(VersionRequest::Range(requires_python))
                })
        };

        let interpreter = PythonInstallation::find_or_download(
            python_request,
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
                Workspace::discover(&CWD, &DiscoveryOptions::default())
                    .await?
                    .with_current_project(package.clone())
                    .with_context(|| format!("Package `{package}` not found in workspace"))?,
            )
        } else {
            VirtualProject::discover(&CWD, &DiscoveryOptions::default()).await?
        };

        // For virtual projects, allow dev dependencies, but nothing else.
        if project.is_virtual() {
            match dependency_type {
                DependencyType::Production => {
                    anyhow::bail!("Found a virtual workspace root, but virtual projects do not support production dependencies (instead, use: `{}`)", "uv add --dev".green())
                }
                DependencyType::Optional(_) => {
                    anyhow::bail!("Found a virtual workspace root, but virtual projects do not support optional dependencies (instead, use: `{}`)", "uv add --dev".green())
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
            cache,
            printer,
        )
        .await?;

        Target::Project(project, venv)
    };

    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .keyring(settings.keyring_provider);

    // Read the requirements.
    let RequirementsSpecification { requirements, .. } =
        RequirementsSpecification::from_simple_sources(&requirements, &client_builder).await?;

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let python_version = None;
    let python_platform = None;
    let hasher = HashStrategy::default();
    let setup_py = SetupPyStrategy::default();
    let build_isolation = BuildIsolation::default();

    // Determine the environment for the resolution.
    let (tags, markers) =
        resolution_environment(python_version, python_platform, target.interpreter())?;

    // Add all authenticated sources to the cache.
    for url in settings.index_locations.urls() {
        store_credentials_from_url(url);
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::from(client_builder)
        .index_urls(settings.index_locations.index_urls())
        .index_strategy(settings.index_strategy)
        .markers(&markers)
        .platform(target.interpreter().platform())
        .build();

    // Initialize any shared state.
    let state = SharedState::default();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client.fetch(settings.index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, Some(&tags), &hasher, &settings.build_options)
    };

    let build_constraints = [];
    let sources = SourceStrategy::Enabled;

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        &build_constraints,
        target.interpreter(),
        &settings.index_locations,
        &flat_index,
        &state.index,
        &state.git,
        &state.in_flight,
        settings.index_strategy,
        setup_py,
        &settings.config_setting,
        build_isolation,
        settings.link_mode,
        &settings.build_options,
        settings.exclude_newer,
        sources,
        concurrency,
    );

    // Resolve any unnamed requirements.
    let requirements = NamedRequirementsResolver::new(
        requirements,
        &hasher,
        &state.index,
        DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads),
    )
    .with_reporter(ResolverReporter::from(printer))
    .resolve()
    .await?;

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
    let mut edits = Vec::with_capacity(requirements.len());
    for mut requirement in requirements {
        // Add the specified extras.
        requirement.extras.extend(extras.iter().cloned());
        requirement.extras.sort_unstable();
        requirement.extras.dedup();

        let (requirement, source) = match target {
            Target::Script(_, _) | Target::Project(_, _) if raw_sources => {
                (pep508_rs::Requirement::from(requirement), None)
            }
            Target::Script(_, _) => resolve_requirement(
                requirement,
                false,
                editable,
                rev.clone(),
                tag.clone(),
                branch.clone(),
            )?,
            Target::Project(ref project, _) => {
                let workspace = project
                    .workspace()
                    .packages()
                    .contains_key(&requirement.name);
                resolve_requirement(
                    requirement,
                    workspace,
                    editable,
                    rev.clone(),
                    tag.clone(),
                    branch.clone(),
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
            }) => {
                let credentials = Credentials::from_url(&git);
                if let Some(credentials) = credentials {
                    debug!("Caching credentials for: {git}");
                    GIT_STORE.insert(RepositoryUrl::new(&git), credentials);
                    let _ = git.set_username("");
                    let _ = git.set_password(None);
                };
                Some(Source::Git {
                    git,
                    subdirectory,
                    rev,
                    tag,
                    branch,
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

        // Keep track of the exact location of the edit.
        edits.push(DependencyEdit {
            dependency_type: &dependency_type,
            requirement,
            source,
            edit,
        });
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

    // If `--script`, exit early. There's no reason to lock and sync.
    let Target::Project(project, venv) = target else {
        return Ok(ExitStatus::Success);
    };

    // If `--frozen`, exit early. There's no reason to lock and sync, and we don't need a `uv.lock`
    // to exist at all.
    if frozen {
        return Ok(ExitStatus::Success);
    }

    let existing = project.pyproject_toml();

    // Update the `pypackage.toml` in-memory.
    let project = project
        .clone()
        .with_pyproject_toml(toml::from_str(&content)?)
        .context("Failed to update `pyproject.toml`")?;

    // Lock and sync the environment, if necessary.
    let lock = match project::lock::do_safe_lock(
        locked,
        frozen,
        project.workspace(),
        venv.interpreter(),
        settings.as_ref().into(),
        Box::new(DefaultResolveLogger),
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await
    {
        Ok(result) => result.into_lock(),
        Err(ProjectError::Operation(pip::operations::Error::Resolve(
            uv_resolver::ResolveError::NoSolution(err),
        ))) => {
            let header = err.header();
            let report = miette::Report::new(WithHelp { header, cause: err, help: Some("If this is intentional, run `uv add --frozen` to skip the lock and sync steps.") });
            anstream::eprint!("{report:?}");

            // Revert the changes to the `pyproject.toml`, if necessary.
            if modified {
                fs_err::write(project.root().join("pyproject.toml"), existing)?;
            }

            return Ok(ExitStatus::Failure);
        }
        Err(err) => return Err(err.into()),
    };

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
        for edit in &edits {
            // Only set a minimum version for newly-added dependencies (as opposed to updates).
            let ArrayEdit::Add(index) = &edit.edit else {
                continue;
            };

            // Only set a minimum version for registry requirements.
            if edit.source.is_some() {
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
            fs_err::write(project.root().join("pyproject.toml"), toml.to_string())?;
        }
    }

    if no_sync {
        return Ok(ExitStatus::Success);
    }

    // Sync the environment.
    let (extras, dev) = match dependency_type {
        DependencyType::Production => {
            let extras = ExtrasSpecification::None;
            let dev = false;
            (extras, dev)
        }
        DependencyType::Dev => {
            let extras = ExtrasSpecification::None;
            let dev = true;
            (extras, dev)
        }
        DependencyType::Optional(ref group_name) => {
            let extras = ExtrasSpecification::Some(vec![group_name.clone()]);
            let dev = false;
            (extras, dev)
        }
    };

    // Initialize any shared state.
    let state = SharedState::default();

    project::sync::do_sync(
        &project,
        &venv,
        &lock,
        &extras,
        dev,
        Modifications::Sufficient,
        settings.as_ref().into(),
        &state,
        Box::new(DefaultInstallLogger),
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await?;

    Ok(ExitStatus::Success)
}

/// Resolves the source for a requirement and processes it into a PEP 508 compliant format.
fn resolve_requirement(
    requirement: pypi_types::Requirement,
    workspace: bool,
    editable: Option<bool>,
    rev: Option<String>,
    tag: Option<String>,
    branch: Option<String>,
) -> Result<(Requirement, Option<Source>), anyhow::Error> {
    let result = Source::from_requirement(
        &requirement.name,
        requirement.source.clone(),
        workspace,
        editable,
        rev,
        tag,
        branch,
    );

    let source = match result {
        Ok(source) => source,
        Err(SourceError::UnresolvedReference(rev)) => {
            anyhow::bail!(
                "Cannot resolve Git reference `{rev}` for requirement `{name}`. Specify the reference with one of `--tag`, `--branch`, or `--rev`, or use the `--raw-sources` flag.",
                name = requirement.name
            )
        }
        Err(err) => return Err(err.into()),
    };

    // Ignore the PEP 508 source by clearing the URL.
    let mut processed_requirement = pep508_rs::Requirement::from(requirement);
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

/// Render a [`uv_resolver::NoSolutionError`] with a help message.
#[derive(Debug, miette::Diagnostic, thiserror::Error)]
#[error("{header}")]
#[diagnostic()]
struct WithHelp {
    /// The header to render in the error message.
    header: uv_resolver::NoSolutionHeader,

    /// The underlying error.
    #[source]
    cause: uv_resolver::NoSolutionError,

    /// The help message to display.
    #[help]
    help: Option<&'static str>,
}
