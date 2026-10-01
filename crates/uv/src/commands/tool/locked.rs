//! Select a tool before reading its packaged lock, without resolving its dependencies.

use std::io;

use anyhow::{Context, bail};
use tracing::debug;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, Constraints, HashCheckingMode, NoBinary, NoBuild, TargetTriple,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::{DistributionDatabase, LocalWheel, LoweredExtraBuildDependencies};
use uv_distribution_filename::DistExtension;
use uv_distribution_types::{
    ArchiveHashPolicy, BuiltDist, CachedDist, Dist, Edge, Hashed, Name,
    NameRequirementSpecification, Node, Requirement, RequirementSource, Resolution, ResolvedDist,
    UnresolvedRequirement, parse_all_url_hashes,
};
use uv_lock::PylockToml;
use uv_metadata::{find_flat_dist_info, read_flat_wheel_metadata};
use uv_pep440::VersionSpecifier;
use uv_platform_tags::Tags;
use uv_preview::{Preview, PreviewFeature};
use uv_pypi_types::{HashAlgorithm, HashDigest, Hashes};
use uv_python::{
    Interpreter, PythonArchitecture, PythonDownloads, PythonEnvironment, PythonPreference,
    PythonRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_resolver::FlatIndex;
use uv_settings::PythonInstallMirrors;
use uv_types::{BuildIsolation, HashStrategy, SourceTreeEditablePolicy};
use uv_workspace::WorkspaceCache;

use crate::commands::pip::loggers::SummaryResolveLogger;
use crate::commands::pip::resolution_tags;
use crate::commands::project::{
    EnvironmentResolution, PlatformState, ProjectError, resolve_environment,
};
use crate::commands::pylock::resolve_pylock_toml;
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::tool::common::refine_interpreter;
use crate::printer::Printer;
use crate::settings::ResolverSettings;

/// A failure to select the tool wheel, before inspecting its packaged lock.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
struct RootSelectionError(#[from] ProjectError);

pub(super) fn root_selection_error(err: anyhow::Error) -> Result<ProjectError, anyhow::Error> {
    err.downcast::<RootSelectionError>()
        .map(|RootSelectionError(err)| err)
}

/// Resolve the tool, retrying root selection with a compatible interpreter when possible.
pub(super) async fn resolve_with_interpreter(
    spec: RequirementsSpecification,
    interpreter: Interpreter,
    refine: bool,
    python_request: Option<&PythonRequest>,
    python_platform: Option<&TargetTriple>,
    build_constraints: &Constraints,
    settings: &ResolverSettings,
    client_builder: &BaseClientBuilder<'_>,
    reporter: &PythonDownloadReporter,
    install_mirrors: &PythonInstallMirrors,
    python_preference: PythonPreference,
    python_arch: Option<PythonArchitecture>,
    python_downloads: PythonDownloads,
    state: &PlatformState,
    concurrency: &Concurrency,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> anyhow::Result<(Resolution, Interpreter)> {
    let result = Box::pin(resolve(
        spec.clone(),
        &interpreter,
        python_platform,
        build_constraints,
        settings,
        client_builder,
        state,
        concurrency,
        cache,
        workspace_cache,
        printer,
        preview,
    ))
    .await;
    let err = match result {
        Ok(resolution) => return Ok((resolution, interpreter)),
        Err(err) => err,
    };
    let Some(RootSelectionError(ProjectError::Operation(operation))) =
        err.downcast_ref::<RootSelectionError>()
    else {
        return Err(err);
    };
    if !refine {
        return Err(err);
    }
    let Some(interpreter) = refine_interpreter(
        &interpreter,
        python_request,
        operation,
        client_builder,
        reporter,
        install_mirrors,
        python_preference,
        python_arch,
        python_downloads,
        cache,
    )
    .await
    .ok()
    .flatten() else {
        return Err(err);
    };
    debug!(
        "Re-resolving with Python {} (`{}`)",
        interpreter.python_version(),
        interpreter.sys_executable().display()
    );
    let resolution = Box::pin(resolve(
        spec,
        &interpreter,
        python_platform,
        build_constraints,
        settings,
        client_builder,
        state,
        concurrency,
        cache,
        workspace_cache,
        printer,
        preview,
    ))
    .await?;
    Ok((resolution, interpreter))
}

pub(super) fn check_supported_modifiers(
    locked: bool,
    spec: &RequirementsSpecification,
) -> anyhow::Result<()> {
    if locked
        && (!spec.overrides.is_empty()
            || !spec.override_dependencies.is_empty()
            || !spec.excludes.is_empty())
    {
        bail!("Overrides and exclusions are not supported with `--locked`");
    }
    Ok(())
}

pub(super) fn check_preview(locked: bool, preview: Preview) -> anyhow::Result<()> {
    if locked && !preview.is_enabled(PreviewFeature::LockedTools) {
        bail!("`--locked` for tools requires the `locked-tools` preview feature");
    }
    Ok(())
}

pub(super) fn check_tool_requirement(requirement: &UnresolvedRequirement) -> anyhow::Result<()> {
    if let Some(url) = requirement.source().to_verbatim_parsed_url() {
        parse_all_url_hashes(&url.verbatim)?;
    }
    match requirement.source().as_ref() {
        RequirementSource::Registry { .. }
        | RequirementSource::Url {
            ext: DistExtension::Wheel,
            ..
        }
        | RequirementSource::Path {
            ext: DistExtension::Wheel,
            ..
        } => Ok(()),
        RequirementSource::Url {
            ext: DistExtension::Source(_),
            ..
        }
        | RequirementSource::Path {
            ext: DistExtension::Source(_),
            ..
        }
        | RequirementSource::GitDirectory { .. }
        | RequirementSource::GitPath { .. }
        | RequirementSource::Directory { .. } => {
            bail!("A wheel is required to install a tool with `--locked`")
        }
    }
}

pub(super) fn check_arguments(locked: bool, with: &[RequirementsSource]) -> anyhow::Result<()> {
    if locked && !with.is_empty() {
        bail!("`--locked` requires a single tool package and cannot be combined with `--with`");
    }
    Ok(())
}

pub(super) fn check_constraints(
    constraints: &[NameRequirementSpecification],
) -> anyhow::Result<()> {
    for entry in constraints {
        requirement_url_hashes(&entry.requirement)?;
        check_unsupported_hash_fragment(&entry.requirement)?;
        entry.requirement.hashes()?;
        let mut extra_marker = false;
        entry
            .requirement
            .marker
            .visit_extras(|_, _| extra_marker = true);
        if extra_marker {
            bail!("Constraints with extra markers are not supported with `--locked`");
        }
    }
    Ok(())
}

fn check_unsupported_hash_fragment(requirement: &Requirement) -> anyhow::Result<()> {
    match &requirement.source {
        RequirementSource::GitDirectory { url, .. }
        | RequirementSource::GitPath { url, .. }
        | RequirementSource::Directory { url, .. } => {
            if let Some(fragment) = url.fragment()
                && Hashes::parse_url_fragment(fragment)?.is_some()
            {
                bail!("Cannot verify archive hashes for `{requirement}`");
            }
        }
        RequirementSource::Registry { .. }
        | RequirementSource::Url { .. }
        | RequirementSource::Path { .. } => {}
    }
    Ok(())
}

fn requirement_url_hashes(requirement: &Requirement) -> anyhow::Result<Vec<HashDigest>> {
    if let Some(url) = requirement.source.to_verbatim_parsed_url() {
        Ok(parse_all_url_hashes(&url.verbatim)?)
    } else {
        Ok(Vec::new())
    }
}

fn add_requirement_hashes(
    groups: &mut Vec<Vec<HashDigest>>,
    requirement: &Requirement,
    hashes: &[String],
) -> anyhow::Result<()> {
    let hashes = hashes
        .iter()
        .map(|hash| hash.parse::<HashDigest>())
        .collect::<Result<Vec<_>, _>>()?;
    if !hashes.is_empty() {
        match &requirement.source {
            RequirementSource::Registry { .. } => groups.push(hashes),
            RequirementSource::Url { .. }
            | RequirementSource::Path { .. }
            | RequirementSource::GitDirectory { .. }
            | RequirementSource::GitPath { .. }
            | RequirementSource::Directory { .. } => {
                groups.extend(hashes.into_iter().map(|hash| vec![hash]));
            }
        }
    }
    groups.extend(
        requirement_url_hashes(requirement)?
            .into_iter()
            .map(|hash| vec![hash]),
    );
    Ok(())
}

pub(super) fn override_specifications(
    requirements: &[Requirement],
    hashes: &[Vec<String>],
) -> Vec<NameRequirementSpecification> {
    // `resolve_names` returns named requirements first, in their original order.
    requirements
        .iter()
        .enumerate()
        .map(|(index, requirement)| NameRequirementSpecification {
            requirement: requirement.clone(),
            hashes: hashes.get(index).cloned().unwrap_or_default(),
        })
        .collect()
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
    check_supported_modifiers(true, &spec)?;
    if spec.pylock.is_some() || !spec.source_trees.is_empty() {
        bail!("`--locked` cannot be combined with additional requirements files");
    }
    let [requirement] = spec.requirements.as_slice() else {
        bail!("`--locked` requires a single tool package and cannot be combined with `--with`");
    };
    check_tool_requirement(&requirement.requirement)?;
    let UnresolvedRequirement::Named(requirement) = &requirement.requirement else {
        bail!("Expected a named tool requirement");
    };
    if !requirement.extras.is_empty() {
        bail!("Extras are not supported with `--locked`");
    }
    let requirement = requirement.clone();
    check_constraints(&spec.constraints)?;
    if settings.build_options.no_binary_package(&requirement.name) {
        bail!("`--locked` requires a wheel for the tool, but binary distributions are disabled");
    }
    let mut selection_settings = settings.clone();
    selection_settings.build_options = selection_settings.build_options.combine(
        NoBinary::None,
        NoBuild::Packages(vec![requirement.name.clone()]),
    );
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
    let markers = crate::commands::pip::resolution_markers(None, python_platform, interpreter);
    let mut root_hashes = Vec::new();
    add_requirement_hashes(&mut root_hashes, &requirement, &[])?;
    for constraint in &spec.constraints {
        if constraint.requirement.name == requirement.name
            && constraint
                .requirement
                .evaluate_markers(Some(markers.markers()), &[])
        {
            add_requirement_hashes(
                &mut root_hashes,
                &constraint.requirement,
                &constraint.hashes,
            )?;
        }
    }
    let mut spec = spec;
    let database =
        DistributionDatabase::new(&client, &dispatch, concurrency.downloads_semaphore.clone());
    let (selected, wheel, root_hash) = loop {
        let selected = Resolution::from(
            resolve_environment(
                spec.clone().into(),
                EnvironmentResolution::Direct,
                interpreter,
                python_platform,
                SourceTreeEditablePolicy::Tool,
                build_constraints.clone(),
                &selection_settings,
                client_builder,
                state,
                Box::new(SummaryResolveLogger),
                concurrency,
                cache,
                workspace_cache,
                printer,
                preview,
            )
            .await
            .map_err(RootSelectionError)?,
        );
        let selected_dist = selected
            .distributions()
            .next()
            .context("No compatible tool was selected")?;
        let ResolvedDist::Installable { dist, .. } = selected_dist else {
            bail!("Expected an installable tool distribution");
        };
        if let Dist::Source(_) = dist.as_ref() {
            bail!("A wheel is required to install a tool with `--locked`");
        }

        let mut artifact_hashes = root_hashes.clone();
        if let Dist::Built(BuiltDist::Registry(dist)) = dist.as_ref() {
            let file = &dist.best_wheel().file;
            artifact_hashes.extend(
                file.hashes
                    .iter()
                    .filter(|hash| hash.algorithm() != HashAlgorithm::Md5)
                    .cloned()
                    .map(|hash| vec![hash]),
            );
            artifact_hashes.extend(
                parse_all_url_hashes(&file.url.to_url()?)?
                    .into_iter()
                    .map(|hash| vec![hash]),
            );
        }
        let hasher = HashStrategy::from_resolution(&selected, HashCheckingMode::Verify)?;
        let (wheel, root_hash) = verify_hash_constraints(
            &database,
            dist,
            &tags,
            hasher.archive_policy(dist.as_ref()),
            &artifact_hashes,
            true,
        )
        .await?;
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
        break (selected, wheel, root_hash);
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
    if !lock.packages.is_empty() {
        bail!("Packaged locks with dependencies are not supported with `--locked`");
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
            let mut selected = node.clone();
            if let Node::Dist { hashes, .. } = &mut selected
                && let Some(hash) = &root_hash
            {
                *hashes = vec![hash.clone()].into();
            }
            let node = graph.add_node(selected);
            graph.add_edge(root, node, Edge::Prod);
        }
    }
    Ok(Resolution::new(graph))
}

/// Verify each constraint against the same archive as the lock, including when the constraints
/// use a different hash algorithm. Retain a verified digest to bind the later installation to it.
async fn verify_hash_constraints(
    database: &DistributionDatabase<'_, BuildDispatch<'_>>,
    dist: &Dist,
    tags: &Tags,
    policy: ArchiveHashPolicy<'_>,
    constraints: &[Vec<HashDigest>],
    generate_sha256: bool,
) -> anyhow::Result<(LocalWheel, Option<HashDigest>)> {
    let first_policy = if generate_sha256 {
        ArchiveHashPolicy::Generate
    } else if policy.requires_validation() {
        policy
    } else if let Some(first) = constraints.first() {
        ArchiveHashPolicy::Any(first)
    } else {
        ArchiveHashPolicy::Generate
    };
    let mut wheel = database
        .get_or_build_wheel(dist, tags, first_policy)
        .await?;
    if !wheel.satisfies(first_policy) {
        return Err(uv_distribution::Error::hash_mismatch(
            dist.to_string(),
            first_policy.digests(),
            wheel.hashes(),
        )
        .into());
    }
    let mut verified = wheel
        .hashes()
        .iter()
        .find(|hash| {
            first_policy.digests().contains(hash)
                || (first_policy == ArchiveHashPolicy::Generate
                    && hash.algorithm() == HashAlgorithm::Sha256)
        })
        .cloned();
    let policy_constraints = if generate_sha256 {
        match policy {
            ArchiveHashPolicy::Any(hashes) => vec![hashes],
            ArchiveHashPolicy::All(hashes) if hashes.is_empty() => vec![hashes],
            ArchiveHashPolicy::All(hashes) => hashes.iter().map(std::slice::from_ref).collect(),
            ArchiveHashPolicy::AllOfAny(groups) => {
                groups.groups().iter().map(Vec::as_slice).collect()
            }
            ArchiveHashPolicy::None | ArchiveHashPolicy::Generate => vec![],
        }
    } else {
        vec![]
    };
    for constraint in policy_constraints
        .into_iter()
        .chain(constraints.iter().map(Vec::as_slice))
    {
        if let Some(hash) = wheel.hashes().iter().find(|hash| constraint.contains(hash)) {
            if verified
                .as_ref()
                .is_none_or(|current| hash_rank(hash) > hash_rank(current))
            {
                verified = Some(hash.clone());
            }
            continue;
        }
        let binding = verified.as_ref().context("Missing verified archive hash")?;
        let mut matched = None;
        for hash in constraint {
            let expected = [binding.clone(), hash.clone()];
            let policy = ArchiveHashPolicy::All(&expected);
            if let Ok(candidate) = database.get_or_build_wheel(dist, tags, policy).await
                && candidate.satisfies(policy)
            {
                matched = Some((candidate, hash.clone()));
                break;
            }
        }
        let Some((candidate, hash)) = matched else {
            bail!("The selected artifact for `{dist}` does not match the required hashes");
        };
        wheel = candidate;
        if hash_rank(&hash) > hash_rank(binding) {
            verified = Some(hash);
        }
    }
    Ok((wheel, verified))
}

fn hash_rank(hash: &HashDigest) -> u8 {
    match hash.algorithm() {
        HashAlgorithm::Sha512 => 5,
        HashAlgorithm::Sha384 => 4,
        HashAlgorithm::Sha256 => 3,
        HashAlgorithm::Blake2b256 => 2,
        HashAlgorithm::Md5 => 1,
    }
}
