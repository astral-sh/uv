use anyhow::Result;
use uv_client::{BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_dispatch::BuildDispatch;
use uv_distribution::pyproject::{Source, SourceError};
use uv_distribution::pyproject_mut::PyProjectTomlMut;
use uv_git::GitResolver;
use uv_requirements::{NamedRequirementsResolver, RequirementsSource, RequirementsSpecification};
use uv_resolver::{FlatIndex, InMemoryIndex};
use uv_toolchain::{ToolchainPreference, ToolchainRequest};
use uv_types::{BuildIsolation, HashStrategy, InFlight};

use uv_cache::Cache;
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode, SetupPyStrategy};
use uv_distribution::{DistributionDatabase, ProjectWorkspace};
use uv_warnings::warn_user;

use crate::commands::pip::operations::Modifications;
use crate::commands::pip::resolution_environment;
use crate::commands::reporters::ResolverReporter;
use crate::commands::{project, ExitStatus};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Add one or more packages to the project requirements.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(crate) async fn add(
    requirements: Vec<RequirementsSource>,
    workspace: bool,
    dev: bool,
    editable: Option<bool>,
    raw: bool,
    rev: Option<String>,
    tag: Option<String>,
    branch: Option<String>,
    python: Option<String>,
    settings: ResolverInstallerSettings,
    toolchain_preference: ToolchainPreference,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv add` is experimental and may change without warning.");
    }

    // Find the project requirements.
    let project = ProjectWorkspace::discover(&std::env::current_dir()?, None).await?;

    // Discover or create the virtual environment.
    let venv = project::init_environment(
        project.workspace(),
        python.as_deref().map(ToolchainRequest::parse),
        toolchain_preference,
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

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(settings.index_locations.index_urls())
        .index_strategy(settings.index_strategy)
        .keyring(settings.keyring_provider)
        .markers(&markers)
        .platform(venv.interpreter().platform())
        .build();

    // Initialize any shared state.
    let git = GitResolver::default();
    let in_flight = InFlight::default();
    let index = InMemoryIndex::default();

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
        &index,
        &git,
        &in_flight,
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
        &index,
        DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads, preview),
    )
    .with_reporter(ResolverReporter::from(printer))
    .resolve()
    .await?;

    // Add the requirements to the `pyproject.toml`.
    let mut pyproject = PyProjectTomlMut::from_toml(project.current_project().pyproject_toml())?;
    for req in requirements {
        let (req, source) = if raw {
            // Use the PEP 508 requirement directly.
            (pep508_rs::Requirement::from(req), None)
        } else {
            // Otherwise, try to construct the source.
            let result = Source::from_requirement(
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
                    anyhow::bail!("Cannot resolve Git reference `{rev}` for requirement `{}`. Specify the reference with one of `--tag`, `--branch`, or `--rev`, or use the `--raw` flag.", req.name)
                }
                Err(err) => return Err(err.into()),
            };

            // Ignore the PEP 508 source.
            let mut req = pep508_rs::Requirement::from(req);
            req.clear_url();

            (req, source)
        };

        if dev {
            pyproject.add_dev_dependency(&req, source.as_ref())?;
        } else {
            pyproject.add_dependency(&req, source.as_ref())?;
        }
    }

    // Save the modified `pyproject.toml`.
    fs_err::write(
        project.current_project().root().join("pyproject.toml"),
        pyproject.to_string(),
    )?;

    // Lock and sync the environment.
    let lock = project::lock::do_lock(
        project.workspace(),
        venv.interpreter(),
        &settings.upgrade,
        &settings.index_locations,
        settings.index_strategy,
        settings.keyring_provider,
        settings.resolution,
        settings.prerelease,
        &settings.config_setting,
        settings.exclude_newer,
        settings.link_mode,
        &settings.build_options,
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
        project.project_name(),
        project.workspace().root(),
        &venv,
        &lock,
        extras,
        dev,
        Modifications::Sufficient,
        &settings.reinstall,
        &settings.index_locations,
        settings.index_strategy,
        settings.keyring_provider,
        &settings.config_setting,
        settings.link_mode,
        settings.compile_bytecode,
        &settings.build_options,
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
