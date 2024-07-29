use anyhow::{Context, Result};

use pep508_rs::ExtraName;
use uv_auth::store_credentials_from_url;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode, SetupPyStrategy};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_fs::CWD;
use uv_normalize::PackageName;
use uv_python::{PythonFetch, PythonPreference, PythonRequest};
use uv_requirements::{NamedRequirementsResolver, RequirementsSource, RequirementsSpecification};
use uv_resolver::FlatIndex;
use uv_types::{BuildIsolation, HashStrategy};
use uv_warnings::warn_user_once;
use uv_workspace::pyproject::{DependencyType, Source, SourceError};
use uv_workspace::pyproject_mut::PyProjectTomlMut;
use uv_workspace::{DiscoveryOptions, ProjectWorkspace, VirtualProject, Workspace};

use crate::commands::pip::operations::Modifications;
use crate::commands::pip::resolution_environment;
use crate::commands::reporters::ResolverReporter;
use crate::commands::{project, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Add one or more packages to the project requirements.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn add(
    locked: bool,
    frozen: bool,
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
    python_preference: PythonPreference,
    python_fetch: PythonFetch,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv add` is experimental and may change without warning");
    }

    // Find the project in the workspace.
    let project = if let Some(package) = package {
        Workspace::discover(&CWD, &DiscoveryOptions::default())
            .await?
            .with_current_project(package.clone())
            .with_context(|| format!("Package `{package}` not found in workspace"))?
    } else {
        ProjectWorkspace::discover(&CWD, &DiscoveryOptions::default()).await?
    };

    // Discover or create the virtual environment.
    let venv = project::get_or_init_environment(
        project.workspace(),
        python.as_deref().map(PythonRequest::parse),
        python_preference,
        python_fetch,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?;

    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .keyring(settings.keyring_provider);

    // Read the requirements.
    let RequirementsSpecification { requirements, .. } =
        RequirementsSpecification::from_sources(&requirements, &[], &[], &client_builder).await?;

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let python_version = None;
    let python_platform = None;
    let hasher = HashStrategy::default();
    let setup_py = SetupPyStrategy::default();
    let build_isolation = BuildIsolation::default();

    // Determine the environment for the resolution.
    let (tags, markers) =
        resolution_environment(python_version, python_platform, venv.interpreter())?;

    // Add all authenticated sources to the cache.
    for url in settings.index_locations.urls() {
        store_credentials_from_url(url);
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::from(client_builder)
        .index_urls(settings.index_locations.index_urls())
        .index_strategy(settings.index_strategy)
        .markers(&markers)
        .platform(venv.interpreter().platform())
        .build();

    // Initialize any shared state.
    let state = SharedState::default();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client.fetch(settings.index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, Some(&tags), &hasher, &settings.build_options)
    };

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        venv.interpreter(),
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
        concurrency,
        preview,
    );

    // Resolve any unnamed requirements.
    let requirements = NamedRequirementsResolver::new(
        requirements,
        &hasher,
        &state.index,
        DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads, preview),
    )
    .with_reporter(ResolverReporter::from(printer))
    .resolve()
    .await?;

    // Add the requirements to the `pyproject.toml`.
    let mut pyproject = PyProjectTomlMut::from_toml(project.current_project().pyproject_toml())?;
    for mut req in requirements {
        // Add the specified extras.
        req.extras.extend(extras.iter().cloned());
        req.extras.sort_unstable();
        req.extras.dedup();

        let (req, source) = if raw_sources {
            // Use the PEP 508 requirement directly.
            (pep508_rs::Requirement::from(req), None)
        } else {
            // Otherwise, try to construct the source.
            let workspace = project.workspace().packages().contains_key(&req.name);
            let result = Source::from_requirement(
                &req.name,
                req.source.clone(),
                workspace,
                editable,
                rev.clone(),
                tag.clone(),
                branch.clone(),
            );

            let source = match result {
                Ok(source) => source,
                Err(SourceError::UnresolvedReference(rev)) => {
                    anyhow::bail!("Cannot resolve Git reference `{rev}` for requirement `{}`. Specify the reference with one of `--tag`, `--branch`, or `--rev`, or use the `--raw-sources` flag.", req.name)
                }
                Err(err) => return Err(err.into()),
            };

            // Ignore the PEP 508 source.
            let mut req = pep508_rs::Requirement::from(req);
            req.clear_url();

            (req, source)
        };

        match dependency_type {
            DependencyType::Production => {
                pyproject.add_dependency(req, source)?;
            }
            DependencyType::Dev => {
                pyproject.add_dev_dependency(req, source)?;
            }
            DependencyType::Optional(ref group) => {
                pyproject.add_optional_dependency(req, group, source)?;
            }
        }
    }

    // Save the modified `pyproject.toml`.
    fs_err::write(
        project.current_project().root().join("pyproject.toml"),
        pyproject.to_string(),
    )?;

    // If `--frozen`, exit early. There's no reason to lock and sync, and we don't need a `uv.lock`
    // to exist at all.
    if frozen {
        return Ok(ExitStatus::Success);
    }

    // Initialize any shared state.
    let state = SharedState::default();

    // Lock and sync the environment, if necessary.
    let lock = project::lock::do_safe_lock(
        locked,
        frozen,
        project.workspace(),
        venv.interpreter(),
        settings.as_ref().into(),
        &state,
        preview,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await?;

    // Perform a full sync, because we don't know what exactly is affected by the removal.
    // TODO(ibraheem): Should we accept CLI overrides for this? Should we even sync here?
    let extras = ExtrasSpecification::All;
    let dev = true;

    project::sync::do_sync(
        &VirtualProject::Project(project),
        &venv,
        &lock.lock,
        &extras,
        dev,
        Modifications::Sufficient,
        settings.as_ref().into(),
        &state,
        preview,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await?;

    Ok(ExitStatus::Success)
}
