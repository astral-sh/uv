use itertools::Itertools;
use rustc_hash::FxHashSet;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_command_support::Printer;
use uv_configuration::{
    Concurrency, DependencyGroupsWithDefaults, DryRun, EditableMode,
    ExtrasSpecificationWithDefaults, HashCheckingMode, InstallOptions, Modifications, TargetTriple,
};
use uv_dispatch::{BuildDispatch, PlatformState};
use uv_distribution::LoweredExtraBuildDependencies;
use uv_distribution_types::{Dist, Resolution, ResolvedDist, SourceDist};
use uv_install_operations::editable::apply_editable_mode;
use uv_install_operations::loggers::InstallLogger;
use uv_install_operations::{BytecodeCompilation, Changelog, InstallationPlan};
use uv_installer::{InstallationStrategy, SitePackages};
use uv_lock::Installable;
use uv_pep508::{MarkerTree, VersionOrUrl};
use uv_preview::Preview;
use uv_pypi_types::{ParsedArchiveUrl, ParsedGitDirectoryUrl, ParsedGitPathUrl, ParsedUrl};
use uv_python_interpreter::PythonEnvironment;
use uv_resolve_operations::{resolution_markers, resolution_tags};
use uv_resolver::FlatIndex;
use uv_settings::InstallerSettingsRef;
use uv_types::{BuildIsolation, HashStrategy, SourceTreeEditablePolicy};
use uv_workspace::pyproject::Source;
use uv_workspace::{DiscoveryOptions, MemberDiscovery, Workspace, WorkspaceCache};

use crate::install_target::InstallTarget;
use crate::malware::{MalwareCheckContext, maybe_check_malware};
use crate::{EnvironmentError, detect_conflicts};
use uv_requirements::script_extra_build_requires;

/// Install the selected packages from a lockfile into an environment.
///
/// Validates interpreter, platform, extras, and groups before planning or applying changes.
pub async fn sync_from_lock(
    target: InstallTarget<'_>,
    venv: &PythonEnvironment,
    extras: &ExtrasSpecificationWithDefaults,
    groups: &DependencyGroupsWithDefaults,
    editable: Option<EditableMode>,
    install_options: InstallOptions,
    modifications: Modifications,
    python_platform: Option<&TargetTriple>,
    settings: InstallerSettingsRef<'_>,
    client_builder: &BaseClientBuilder<'_>,
    state: &PlatformState,
    logger: Box<dyn InstallLogger>,
    installer_metadata: bool,
    concurrency: &Concurrency,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    dry_run: DryRun,
    printer: Printer,
    preview: Preview,
    malware_context: MalwareCheckContext<'_>,
) -> Result<Changelog, EnvironmentError> {
    // Extract the project settings.
    let InstallerSettingsRef {
        index_locations,
        index_strategy,
        keyring_provider,
        dependency_metadata,
        config_setting,
        config_settings_package,
        build_isolation,
        extra_build_dependencies,
        extra_build_variables,
        exclude_newer,
        link_mode,
        compile_bytecode,
        reinstall,
        build_options,
        sources,
    } = settings;

    // Lower the extra build dependencies with source resolution.
    let extra_build_requires = match &target {
        InstallTarget::Workspace { workspace, .. }
        | InstallTarget::Project { workspace, .. }
        | InstallTarget::Projects { workspace, .. }
        | InstallTarget::NonProjectWorkspace { workspace, .. } => {
            LoweredExtraBuildDependencies::from_workspace(
                extra_build_dependencies.clone(),
                workspace,
                index_locations,
                &sources,
                cache,
                workspace_cache,
                client_builder.credentials_cache(),
            )
            .await?
        }
        InstallTarget::Lockfile {
            root,
            project_name,
            lock,
            ..
        } => {
            let member = project_name.and_then(|name| {
                lock.workspace_member_paths()
                    .find_map(|(member, path)| (member == name).then(|| root.join(path)))
            });
            if let Some(member) = member.filter(|path| {
                !extra_build_dependencies.is_empty() && path.join("pyproject.toml").is_file()
            }) {
                let workspace = Workspace::discover(
                    &member,
                    &DiscoveryOptions {
                        members: MemberDiscovery::Existing,
                        stop_discovery_at: Some(root.to_path_buf()),
                    },
                    cache,
                    workspace_cache,
                )
                .await?;
                LoweredExtraBuildDependencies::from_workspace(
                    extra_build_dependencies.clone(),
                    &workspace,
                    index_locations,
                    &sources,
                    cache,
                    workspace_cache,
                    client_builder.credentials_cache(),
                )
                .await?
            } else {
                LoweredExtraBuildDependencies::from_non_lowered(extra_build_dependencies.clone())
            }
        }
        InstallTarget::Script { script, .. } => {
            script_extra_build_requires(
                (*script).into(),
                &sources,
                index_locations,
                cache,
                workspace_cache,
                client_builder.credentials_cache(),
            )
            .await?
        }
    }
    .into_inner();

    let client_builder = client_builder.clone().keyring(keyring_provider);
    // Save an authenticated builder for the malware check before moving the
    // primary builder into the registry client below.
    let malware_check_client_builder = client_builder.clone();

    // Validate that the Python version is supported by the lockfile.
    if !target
        .lock()
        .requires_python()
        .contains(venv.interpreter().python_version())
    {
        return Err(EnvironmentError::LockedPythonIncompatibility(
            venv.interpreter().python_version().clone(),
            target.lock().requires_python().clone(),
        ));
    }

    // Validate that the set of requested extras and development groups are compatible.
    detect_conflicts(&target, extras, groups)?;

    // Validate that the set of requested extras and development groups are defined in the lockfile.
    target.validate_extras(extras)?;
    target.validate_groups(groups)?;

    // Determine the markers to use for resolution.
    let marker_env = resolution_markers(None, python_platform, venv.interpreter());

    // Validate that the platform is supported by the lockfile.
    let environments = target.lock().supported_environments();
    if !environments.is_empty() {
        if !environments
            .iter()
            .any(|env| env.evaluate(&marker_env, &[]))
        {
            return Err(EnvironmentError::LockedPlatformIncompatibility(
                // For error reporting, we use the "simplified"
                // supported environments, because these correspond to
                // what the end user actually wrote. The non-simplified
                // environments, by contrast, are explicitly
                // constrained by `requires-python`.
                target
                    .lock()
                    .simplified_supported_environments()
                    .into_iter()
                    .filter_map(MarkerTree::contents)
                    .map(|env| format!("`{env}`"))
                    .join(", "),
            ));
        }
    }

    // Determine the tags to use for the resolution.
    let tags = resolution_tags(None, python_platform, venv.interpreter())
        .map_err(EnvironmentError::from)?;

    // Read the lockfile.
    let resolution = target.to_resolution(
        &marker_env,
        &tags,
        extras,
        groups,
        build_options,
        &install_options,
    )?;

    // Always skip virtual projects, which shouldn't be built or installed.
    let resolution = apply_no_virtual_project(resolution);

    // If necessary, convert editable to non-editable distributions.
    let resolution = apply_editable_mode(resolution, editable);

    // Constrain any build requirements marked as `match-runtime = true`.
    let extra_build_requires = extra_build_requires.match_runtime(&resolution)?;

    // Extract the hashes from the lockfile.
    let hasher = HashStrategy::from_resolution(&resolution, HashCheckingMode::Verify)?;

    // Populate credentials from the target.
    store_credentials_from_target(target, &client_builder)?;

    let bytecode_compilation = compile_bytecode.then_some(BytecodeCompilation::All);
    let site_packages = SitePackages::from_environment(venv)?;
    let installation_plan = InstallationPlan::build(
        &resolution,
        site_packages,
        InstallationStrategy::Strict,
        reinstall,
        build_options,
        &hasher,
        index_locations,
        config_setting,
        config_settings_package,
        &extra_build_requires,
        extra_build_variables,
        cache,
        venv,
        &tags,
    )?;

    // Avoid constructing an HTTP client and build dispatch when planning shows that there is no
    // installation work to perform.
    if installation_plan.is_noop(modifications, bytecode_compilation, dry_run) {
        maybe_check_malware(
            &target,
            &resolution,
            &malware_check_client_builder,
            concurrency,
            cache,
            preview,
            &malware_context,
        )
        .await?;

        return Ok(installation_plan.finish_noop(
            &resolution,
            modifications,
            bytecode_compilation,
            logger.as_ref(),
            dry_run,
            printer,
        )?);
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(client_builder, cache.clone())
        .index_locations(index_locations.clone())
        .index_strategy(index_strategy)
        .markers(venv.interpreter().markers())
        .platform(venv.interpreter().platform())
        .build()?;

    // Determine whether to enable build isolation.
    let build_isolation = match build_isolation {
        uv_configuration::BuildIsolation::Isolate => BuildIsolation::Isolated,
        uv_configuration::BuildIsolation::Shared => BuildIsolation::Shared(venv),
        uv_configuration::BuildIsolation::SharedPackage(packages) => {
            BuildIsolation::SharedPackage(venv, packages)
        }
    };

    // Read the build constraints from the lockfile.
    let build_constraints = target.build_constraints();

    let build_hasher = HashStrategy::from_constraints(
        &build_constraints,
        Some(&venv.interpreter().to_resolver_marker_environment()),
        uv_configuration::HashCheckingMode::Verify,
    )?;
    // Also verify artifacts in the full lockfile, including unselected extras and groups.
    let build_hasher = target
        .lock()
        .hash_strategy(target.install_path(), &FxHashSet::default())?
        .with_constraint_hashes(&build_hasher)?;

    // Resolve the flat indexes from `--find-links`.
    let flat_index = FlatIndex::load(&client, cache, index_locations).await?;

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        &build_constraints,
        venv.interpreter(),
        index_locations,
        &flat_index,
        dependency_metadata,
        state.clone().into_inner(),
        index_strategy,
        config_setting,
        config_settings_package,
        build_isolation,
        &extra_build_requires,
        extra_build_variables,
        link_mode,
        build_options,
        &build_hasher,
        exclude_newer.clone(),
        sources.clone(),
        SourceTreeEditablePolicy::Project,
        workspace_cache.clone(),
        concurrency.clone(),
        preview,
    );

    // Run a malware check against OSV before installing.
    maybe_check_malware(
        &target,
        &resolution,
        &malware_check_client_builder,
        concurrency,
        cache,
        preview,
        &malware_context,
    )
    .await?;

    // Sync the environment.
    let changelog = installation_plan
        .execute(
            &resolution,
            modifications,
            build_options,
            link_mode,
            bytecode_compilation,
            &hasher,
            &tags,
            &client,
            state.in_flight(),
            concurrency,
            &build_dispatch,
            cache,
            venv,
            logger,
            installer_metadata,
            dry_run,
            printer,
            preview,
        )
        .await?;

    Ok(changelog)
}

/// Filter out any virtual workspace members.
fn apply_no_virtual_project(resolution: Resolution) -> Resolution {
    resolution.filter(|dist| {
        let ResolvedDist::Installable { dist, .. } = dist else {
            return true;
        };

        let Dist::Source(dist) = dist.as_ref() else {
            return true;
        };

        let SourceDist::Directory(dist) = dist else {
            return true;
        };

        !dist.r#virtual.unwrap_or(false)
    })
}

/// Extract any credentials that are defined on the workspace dependencies themselves. While we
/// don't store plaintext credentials in the `uv.lock`, we do respect credentials that are defined
/// in the `pyproject.toml`.
///
/// These credentials can come from any of `tool.uv.sources`, `tool.uv.dev-dependencies`,
/// `project.dependencies`, and `project.optional-dependencies`.
pub fn store_credentials_from_target(
    target: InstallTarget<'_>,
    client_builder: &BaseClientBuilder,
) -> Result<(), EnvironmentError> {
    // Iterate over any indexes in the target.
    for index in target.indexes() {
        if let Some(credentials) = index.credentials()? {
            if let Some(root_url) = index.root_url() {
                client_builder.store_credentials(&root_url, credentials.clone());
            }
            client_builder.store_credentials(index.raw_url(), credentials);
        }
    }

    // Iterate over any sources in the target.
    for source in target.sources() {
        match source {
            Source::Git { git, .. } => {
                uv_git::store_credentials_from_url(git)?;
            }
            Source::Url { url, .. } => {
                client_builder.store_credentials_from_url(url)?;
            }
            _ => {}
        }
    }

    // Iterate over any dependencies defined in the target.
    for requirement in target.requirements() {
        let Some(VersionOrUrl::Url(url)) = &requirement.version_or_url else {
            continue;
        };
        match &url.parsed_url {
            ParsedUrl::GitDirectory(ParsedGitDirectoryUrl { url, .. })
            | ParsedUrl::GitPath(ParsedGitPathUrl { url, .. }) => {
                uv_git::store_credentials_from_url(url.url())?;
            }
            ParsedUrl::Archive(ParsedArchiveUrl { url, .. }) => {
                client_builder.store_credentials_from_url(url)?;
            }
            _ => {}
        }
    }
    Ok(())
}
