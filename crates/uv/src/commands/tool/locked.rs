//! Select a tool before reading its packaged lock, without resolving its dependencies.

use std::io;

use anyhow::{Context, bail};

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{Concurrency, Constraints, HashCheckingMode, TargetTriple};
use uv_dispatch::BuildDispatch;
use uv_distribution::{DistributionDatabase, LoweredExtraBuildDependencies};
use uv_distribution_types::{
    CachedDist, Edge, Hashed, Name, NameRequirementSpecification, Node, RequirementSource,
    Resolution, ResolvedDist, UnresolvedRequirement,
};
use uv_lock::PylockToml;
use uv_metadata::{find_flat_dist_info, read_flat_wheel_metadata};
use uv_pep440::VersionSpecifier;
use uv_preview::{Preview, PreviewFeature};
use uv_python::{Interpreter, PythonEnvironment};
use uv_requirements::RequirementsSpecification;
use uv_resolver::FlatIndex;
use uv_types::{BuildIsolation, HashStrategy, SourceTreeEditablePolicy};
use uv_workspace::WorkspaceCache;

use crate::commands::pip::loggers::SummaryResolveLogger;
use crate::commands::pip::resolution_tags;
use crate::commands::project::{EnvironmentResolution, PlatformState, resolve_environment};
use crate::commands::pylock::resolve_pylock_toml;
use crate::printer::Printer;
use crate::settings::ResolverSettings;

pub(super) fn check_preview(locked: bool, preview: Preview) -> anyhow::Result<()> {
    if locked && !preview.is_enabled(PreviewFeature::LockedTools) {
        bail!("`--locked` for tools requires the `locked-tools` preview feature");
    }
    Ok(())
}

pub(super) async fn resolve(
    spec: RequirementsSpecification,
    interpreter: &Interpreter,
    python_platform: Option<&TargetTriple>,
    build_constraints: &Constraints,
    settings: &ResolverSettings,
    client_builder: &BaseClientBuilder<'_>,
    state: &PlatformState,
    concurrency: &Concurrency,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> anyhow::Result<Resolution> {
    if spec.pylock.is_some() || !spec.source_trees.is_empty() {
        bail!("`--locked` cannot be combined with additional requirements files");
    }
    let [requirement] = spec.requirements.as_slice() else {
        bail!("`--locked` requires a single tool package and cannot be combined with `--with`");
    };
    let UnresolvedRequirement::Named(requirement) = &requirement.requirement else {
        bail!("Expected a named tool requirement");
    };
    let requirement = requirement.clone();
    if !spec.overrides.is_empty()
        || !spec.override_dependencies.is_empty()
        || !spec.excludes.is_empty()
    {
        bail!("`--locked` cannot be combined with dependency overrides or exclusions");
    }
    if spec
        .constraints
        .iter()
        .any(|constraint| constraint.requirement.name != requirement.name)
    {
        bail!(
            "`--locked` only supports constraints on the tool itself; dependencies are selected by its lock"
        );
    }

    let client = RegistryClientBuilder::new(
        client_builder.clone().keyring(settings.keyring_provider),
        cache.clone(),
    )
    .index_locations(settings.index_locations.clone())
    .index_strategy(settings.index_strategy)
    .markers(interpreter.markers())
    .platform(interpreter.platform())
    .build()?;
    let flat_index = FlatIndex::load(&client, cache, &settings.index_locations).await?;
    let environment;
    let build_isolation = match &settings.build_isolation {
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
    let extra_build_requires =
        LoweredExtraBuildDependencies::from_non_lowered(settings.extra_build_dependencies.clone())
            .into_inner();
    let build_hasher = HashStrategy::from_constraints(
        build_constraints,
        Some(&interpreter.to_resolver_marker_environment()),
        HashCheckingMode::Verify,
    )?;
    let dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        interpreter,
        &settings.index_locations,
        &flat_index,
        &settings.dependency_metadata,
        state.clone().into_inner(),
        settings.index_strategy,
        &settings.config_setting,
        &settings.config_settings_package,
        build_isolation,
        &extra_build_requires,
        &settings.extra_build_variables,
        settings.link_mode,
        &settings.build_options,
        &build_hasher,
        settings.exclude_newer.clone(),
        settings.sources.clone(),
        SourceTreeEditablePolicy::Tool,
        workspace_cache.clone(),
        concurrency.clone(),
        preview,
    );
    let tags = resolution_tags(None, python_platform, interpreter)?;
    let mut spec = spec;
    let (selected, wheel) = loop {
        let selected = Resolution::from(
            resolve_environment(
                spec.clone().into(),
                EnvironmentResolution::Direct,
                interpreter,
                python_platform,
                SourceTreeEditablePolicy::Tool,
                build_constraints.clone(),
                settings,
                client_builder,
                state,
                Box::new(SummaryResolveLogger),
                concurrency,
                cache,
                workspace_cache,
                printer,
                preview,
            )
            .await?,
        );
        let selected_dist = selected
            .distributions()
            .next()
            .context("No compatible tool was selected")?;
        let ResolvedDist::Installable { dist, .. } = selected_dist else {
            bail!("Expected an installable tool distribution");
        };

        let hasher = HashStrategy::from_resolution(&selected, HashCheckingMode::Verify)?;
        let policy = hasher.archive_policy(dist.as_ref());
        let wheel =
            DistributionDatabase::new(&client, &dispatch, concurrency.downloads_semaphore.clone())
                .get_or_build_wheel(dist, &tags, policy)
                .await?;
        if !wheel.satisfies(policy) {
            return Err(uv_distribution::Error::hash_mismatch(
                dist.to_string(),
                policy.digests(),
                wheel.hashes(),
            )
            .into());
        }
        let wheel = CachedDist::from(wheel);

        let metadata = read_flat_wheel_metadata(wheel.filename(), wheel.path())?;
        if let Some(requires_python) = metadata.requires_python.as_ref()
            && !requires_python.contains(interpreter.python_version())
        {
            // Index metadata may omit Requires-Python, particularly for --find-links.
            // Eliminate incompatible releases before considering their packaged locks.
            let mut excluded = requirement.clone();
            if let RequirementSource::Registry { specifier, .. } = &mut excluded.source
                && dist.index().is_some()
            {
                *specifier =
                    VersionSpecifier::not_equals_version(wheel.filename().version.clone()).into();
                spec.constraints
                    .push(NameRequirementSpecification::from(excluded));
                continue;
            }
            bail!(
                "`{selected_dist}` requires Python `{requires_python}`, but the selected interpreter is Python {}",
                interpreter.python_version()
            );
        }
        break (selected, wheel);
    };
    let selected_dist = selected
        .distributions()
        .next()
        .context("No compatible tool was selected")?;
    let dist_info = find_flat_dist_info(wheel.filename(), wheel.path())?;
    let dist_info = wheel.path().join(format!("{dist_info}.dist-info"));
    let contents = match fs_err::read_to_string(dist_info.join("pylock.toml")) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            bail!(
                "`{selected_dist}` does not contain `pylock.toml` in its `.dist-info` directory; `--locked` requires a packaged lock"
            );
        }
        Err(err) => {
            return Err(err).with_context(|| {
                format!("Failed to read the packaged lock for `{selected_dist}`")
            });
        }
    };
    let lock: PylockToml = toml::from_str(&contents)
        .with_context(|| format!("`{selected_dist}` contains an invalid `pylock.toml`"))?;
    if lock.has_missing_hashes() {
        bail!(
            "The packaged lock for `{selected_dist}` is missing artifact hashes; regenerate the lock before publishing the package"
        );
    }
    for extra in &requirement.extras {
        if !lock.extras.contains(extra) {
            bail!(
                "The packaged lock for `{}` does not support the extra `{extra}`",
                requirement.name
            );
        }
    }
    for group in &requirement.groups {
        if !lock.dependency_groups.contains(group) {
            bail!(
                "The packaged lock for `{}` does not support the dependency group `{group}`",
                requirement.name
            );
        }
    }
    let mut groups = lock.default_groups.clone();
    groups.extend(requirement.groups.iter().cloned());
    groups.sort_unstable();
    groups.dedup();
    let (dependencies, _) = resolve_pylock_toml(
        lock,
        &dist_info,
        interpreter,
        None,
        python_platform,
        &requirement.extras,
        &groups,
        &settings.build_options,
        Some(HashCheckingMode::Verify),
    )?;
    if dependencies
        .distributions()
        .any(|dist| dist.name() == &requirement.name)
    {
        bail!(
            "The packaged lock for `{}` must not include the tool itself",
            requirement.name
        );
    }

    let mut graph = dependencies.graph().clone();
    let root = graph
        .node_indices()
        .find(|&index| match graph[index] {
            Node::Root => true,
            Node::Dist { .. } => false,
        })
        .context("Expected a root in the packaged lock")?;
    for node in selected.graph().node_weights() {
        if let Node::Dist { .. } = node {
            let node = graph.add_node(node.clone());
            graph.add_edge(root, node, Edge::Prod);
        }
    }
    Ok(Resolution::new(graph))
}
