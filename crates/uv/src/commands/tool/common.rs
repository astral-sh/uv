use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, Bound},
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
use uv_client::BaseClientBuilder;
use uv_configuration::{BuildOptions, Concurrency, Constraints, GitLfsSetting, TargetTriple};
use uv_distribution::StaticMetadataDatabase;
use uv_distribution_types::{
    InstalledDist, Name, Requirement, RequiresPython, Resolution, UnresolvedRequirement,
};
use uv_errors::{ErrorWithHints, Hint, Hints};
#[cfg(unix)]
use uv_fs::replace_symlink;
use uv_fs::{CWD, Simplified};
use uv_git::GitResolver;
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifier, VersionSpecifiers};
use uv_pep508::{MarkerExpression, MarkerTree, MarkerValueVersion};
use uv_platform_tags::Os;
use uv_preview::Preview;
use uv_python::downloads::{ManagedPythonDownloadList, PythonDownloadRequest};
use uv_python::{
    EnvironmentPreference, Interpreter, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest, PythonVariant, PythonVersion, PythonVersionFile,
    VersionFileDiscoveryOptions, VersionRequest, find_python_installations,
};
use uv_resolver::{PylockToml, PythonRequirement, ResolutionMode, ResolverOutput};
use uv_settings::{PythonInstallMirrors, ToolOptions};
use uv_shell::Shell;
use uv_tool::{InstalledTools, Tool, ToolEntrypoint, entrypoint_paths};
use uv_types::SourceTreeEditablePolicy;
use uv_warnings::warn_user_once;
use uv_workspace::WorkspaceCache;

use crate::commands::pip;
use crate::commands::pip::loggers::ResolveLogger;

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
    EnvironmentSpecification, PlatformState, ProjectError, PythonRequestSource,
    resolve_with_python_alternatives,
};
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
    /// version file, and static `requires-python` metadata from a source requirement.
    pub(crate) python_request: Option<PythonRequest>,
}

impl ToolPython {
    /// Determine the [`ToolPython`] request for a tool invocation.
    pub(crate) async fn from_request(
        python_request: Option<PythonRequest>,
        requirement: Option<&UnresolvedRequirement>,
        no_config: bool,
        lfs: GitLfsSetting,
        git_resolver: &GitResolver,
        client_builder: &BaseClientBuilder<'_>,
        cache: &Cache,
    ) -> Result<Self, ProjectError> {
        let source_requires_python = if python_request.is_none() {
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

        let (source, python_request) =
            Self::select_request(python_request, source_requires_python.as_ref(), no_config)
                .await?;

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

/// A registry tool resolution narrowed to one concrete Python target.
pub(crate) struct RegistryToolResolution {
    pub(crate) interpreter: Interpreter,
    pub(crate) resolution: Resolution,
}

#[derive(Clone)]
struct RegistryToolPythonTarget {
    version: PythonVersion,
    interpreter: Option<Interpreter>,
}

struct RegistryToolPythonTargets {
    /// One tag representative for every Python minor considered by the resolver.
    alternatives: Vec<PythonVersion>,
    /// Concrete Python installations or downloads that can be selected after resolution.
    candidates: Vec<RegistryToolPythonTarget>,
}

/// Resolve a registry tool across discrete Python minor forks and select one concrete result.
pub(crate) async fn resolve_registry_tool(
    package_name: &PackageName,
    spec: EnvironmentSpecification<'_>,
    interpreter: &Interpreter,
    python_platform: Option<&TargetTriple>,
    source_tree_editable_policy: SourceTreeEditablePolicy,
    build_constraints: Constraints,
    settings: &ResolverInstallerSettings,
    client_builder: &BaseClientBuilder<'_>,
    state: &PlatformState,
    reporter: &PythonDownloadReporter,
    install_mirrors: &PythonInstallMirrors,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    concurrency: &Concurrency,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    resolve_logger: Box<dyn ResolveLogger>,
    printer: Printer,
    preview: Preview,
) -> Result<Option<RegistryToolResolution>, ProjectError> {
    let python_targets = registry_tool_python_targets(
        interpreter,
        client_builder,
        install_mirrors.python_downloads_json_url.as_deref(),
        python_preference,
        python_downloads,
        cache,
    )
    .await?;

    if python_targets.candidates.len() == 1 {
        return Ok(None);
    }

    let Some(minimum_python_version) = python_targets
        .candidates
        .iter()
        .map(|python_target| python_target.version.version())
        .min()
        .cloned()
    else {
        return Ok(None);
    };
    let python_requirement = PythonRequirement::from_requires_python(
        interpreter,
        RequiresPython::greater_than_equal_version(&minimum_python_version),
    );
    let python_alternatives = python_targets
        .alternatives
        .iter()
        .cloned()
        .map(|python_target| {
            let markers = registry_tool_python_minor_marker(&python_target);
            (python_target, markers)
        })
        .collect::<Vec<_>>();
    let resolution = resolve_with_python_alternatives(
        spec,
        interpreter,
        python_alternatives,
        python_requirement,
        python_platform,
        source_tree_editable_policy,
        build_constraints,
        &settings.resolver,
        client_builder,
        state,
        resolve_logger,
        concurrency,
        cache,
        workspace_cache,
        printer,
        preview,
    )
    .await?;

    let mut viable_candidates = Vec::new();
    for python_target in python_targets.candidates {
        let python_version = python_target
            .interpreter
            .is_none()
            .then_some(&python_target.version);
        let candidate_interpreter = python_target.interpreter.as_ref().unwrap_or(interpreter);
        let Some((version, narrowed)) = narrow_registry_tool_resolution(
            &resolution,
            package_name,
            python_version,
            python_platform,
            candidate_interpreter,
            &settings.resolver.build_options,
        )?
        else {
            continue;
        };
        viable_candidates.push((version, python_target, narrowed));
    }

    let current_python_version = interpreter.python_version();
    let current_python_minor = interpreter.python_minor_version();
    viable_candidates.sort_by(
        |(left_version, left_target, _), (right_version, right_target, _)| {
            registry_tool_candidate_order(
                left_version,
                left_target,
                right_version,
                right_target,
                settings.resolver.resolution,
                current_python_version,
                &current_python_minor,
            )
        },
    );

    for (predicted_version, python_target, narrowed) in viable_candidates {
        let python_minor = python_target.version.python_version();
        let is_installed = python_target.interpreter.is_some();

        let selected_python = if let Some(interpreter) = python_target.interpreter {
            interpreter
        } else {
            let Some(request) = registry_tool_python_request(interpreter, &python_target.version)
            else {
                continue;
            };
            let candidate = match PythonInstallation::find_or_download(
                Some(&request),
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
            .await
            {
                Ok(candidate) => candidate.into_interpreter(),
                Err(uv_python::Error::MissingPython(..)) => {
                    debug!("No Python {python_minor} interpreter is available for tool resolution");
                    continue;
                }
                Err(err) => return Err(err.into()),
            };
            if !candidate.python_version().is_stable()
                || candidate.python_version() != python_target.version.version()
                || candidate.implementation_name() != interpreter.implementation_name()
                || candidate.variant() != interpreter.variant()
            {
                debug!(
                    "Discarding Python {} interpreter during tool resolution",
                    candidate.python_version()
                );
                continue;
            }
            candidate
        };

        let narrowed = if selected_python.python_version() == python_target.version.version()
            && is_installed
        {
            narrowed
        } else {
            let Some((actual_version, narrowed)) = narrow_registry_tool_resolution(
                &resolution,
                package_name,
                None,
                python_platform,
                &selected_python,
                &settings.resolver.build_options,
            )?
            else {
                continue;
            };
            if actual_version != predicted_version {
                debug!(
                    "Discarding Python {python_minor} because its concrete resolution changed from {predicted_version} to {actual_version}"
                );
                continue;
            }
            narrowed
        };

        return Ok(Some(RegistryToolResolution {
            interpreter: selected_python,
            resolution: narrowed,
        }));
    }

    Ok(None)
}

fn narrow_registry_tool_resolution(
    resolution: &ResolverOutput,
    package_name: &PackageName,
    python_version: Option<&PythonVersion>,
    python_platform: Option<&TargetTriple>,
    interpreter: &Interpreter,
    build_options: &BuildOptions,
) -> Result<Option<(Version, Resolution)>, ProjectError> {
    let markers = pip::resolution_markers(python_version, python_platform, interpreter);
    let tags = pip::resolution_tags(python_version, python_platform, interpreter)?.into_owned();
    let narrowed =
        match PylockToml::from_resolution(resolution, &[], &CWD, Some(&tags), build_options) {
            Ok(lock) => {
                match lock.to_resolution(&CWD, markers.markers(), &[], &[], &tags, build_options) {
                    Ok(narrowed) => narrowed,
                    Err(err) => {
                        debug!("Discarding Python target during tool resolution: {err}");
                        return Ok(None);
                    }
                }
            }
            Err(err) => {
                debug!("Discarding Python target during tool resolution: {err}");
                return Ok(None);
            }
        };

    let version = narrowed
        .distributions()
        .find(|distribution| distribution.name() == package_name)
        .and_then(|distribution| distribution.version())
        .cloned();
    Ok(version.map(|version| (version, narrowed)))
}

fn registry_tool_candidate_order(
    left_version: &Version,
    left_target: &RegistryToolPythonTarget,
    right_version: &Version,
    right_target: &RegistryToolPythonTarget,
    resolution_mode: ResolutionMode,
    current_python_version: &Version,
    current_python_minor: &Version,
) -> Ordering {
    let left_python_minor = left_target.version.python_version();
    let right_python_minor = right_target.version.python_version();
    registry_tool_version_order(left_version, right_version, resolution_mode)
        .then_with(|| {
            (left_target.version.version() != current_python_version)
                .cmp(&(right_target.version.version() != current_python_version))
        })
        .then_with(|| {
            (left_python_minor != *current_python_minor)
                .cmp(&(right_python_minor != *current_python_minor))
        })
        .then_with(|| {
            left_target
                .interpreter
                .is_none()
                .cmp(&right_target.interpreter.is_none())
        })
        .then_with(|| right_python_minor.cmp(&left_python_minor))
        .then_with(|| {
            right_target
                .version
                .version()
                .cmp(left_target.version.version())
        })
}

fn registry_tool_version_order(
    left: &Version,
    right: &Version,
    resolution_mode: ResolutionMode,
) -> Ordering {
    match resolution_mode {
        ResolutionMode::Highest => right.cmp(left),
        ResolutionMode::Lowest | ResolutionMode::LowestDirect => left.cmp(right),
    }
}

async fn registry_tool_python_targets(
    interpreter: &Interpreter,
    client_builder: &BaseClientBuilder<'_>,
    python_downloads_json_url: Option<&str>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    cache: &Cache,
) -> Result<RegistryToolPythonTargets, ProjectError> {
    let current_python_version = PythonVersion::from(interpreter.python_full_version().clone());
    // Alternative tags can be derived from a CPython language version and the current
    // interpreter's variant. Other implementations have independent implementation versions, so
    // their cross-minor tags cannot be inferred from [`PythonVersion`] alone. Pyodide reports
    // itself as CPython, but its Emscripten targets also cannot be inferred this way.
    if interpreter.implementation_name() != "cpython"
        || matches!(
            interpreter.platform().os(),
            Os::Pyodide { .. } | Os::PyEmscripten { .. }
        )
    {
        return Ok(RegistryToolPythonTargets {
            alternatives: vec![current_python_version.clone()],
            candidates: vec![RegistryToolPythonTarget {
                version: current_python_version,
                interpreter: Some(interpreter.clone()),
            }],
        });
    }
    let Ok(download_request) = PythonDownloadRequest::try_from(&interpreter.key()) else {
        return Ok(RegistryToolPythonTargets {
            alternatives: vec![current_python_version.clone()],
            candidates: vec![RegistryToolPythonTarget {
                version: current_python_version,
                interpreter: Some(interpreter.clone()),
            }],
        });
    };
    let download_request = download_request
        .with_version(VersionRequest::Major(
            interpreter.python_major(),
            interpreter.variant(),
        ))
        .with_prereleases(false);
    let mut download_alternatives = BTreeMap::new();
    let mut installed_alternatives = BTreeMap::new();
    let mut candidates = BTreeMap::new();
    let allow_download_targets = python_downloads.is_automatic()
        && client_builder.connectivity.is_online()
        && !matches!(python_preference, PythonPreference::OnlySystem);
    if allow_download_targets {
        let client = client_builder.build()?;
        match ManagedPythonDownloadList::new(&client, python_downloads_json_url).await {
            Ok(download_list) => {
                for python_target in download_list
                    .iter_matching(&download_request)
                    .map(uv_python::downloads::ManagedPythonDownload::python_version)
                {
                    insert_registry_tool_python_alternative(
                        &mut download_alternatives,
                        python_target,
                    );
                }
                for python_target in download_alternatives.values().cloned() {
                    insert_registry_tool_python_candidate(&mut candidates, python_target, None);
                }
            }
            Err(err) => {
                debug!("Skipping managed Python downloads during tool target discovery: {err}");
            }
        }
    }

    let installed_request = PythonRequest::Key(download_request);
    for result in find_python_installations(
        &installed_request,
        EnvironmentPreference::OnlySystem,
        python_preference,
        cache,
    ) {
        let installation = match result {
            Ok(Ok(installation)) => installation,
            Ok(Err(_)) => continue,
            Err(err) => {
                debug!("Skipping Python installation during tool target discovery: {err}");
                continue;
            }
        };
        let python_version =
            PythonVersion::from(installation.interpreter().python_full_version().clone());
        insert_registry_tool_python_alternative(
            &mut installed_alternatives,
            python_version.clone(),
        );
        insert_registry_tool_python_candidate(
            &mut candidates,
            python_version,
            Some(installation.into_interpreter()),
        );
    }

    let current_python_target = RegistryToolPythonTarget {
        version: current_python_version.clone(),
        interpreter: Some(interpreter.clone()),
    };
    candidates.insert(
        current_python_version.version().clone(),
        current_python_target.clone(),
    );

    let mut ordered_candidates = Vec::with_capacity(candidates.len());
    ordered_candidates.push(current_python_target);
    ordered_candidates.extend(candidates.into_iter().rev().filter_map(
        |(python_version, python_target)| {
            (python_version != *current_python_version.version()).then_some(python_target)
        },
    ));
    Ok(RegistryToolPythonTargets {
        alternatives: merge_registry_tool_python_alternatives(
            download_alternatives,
            installed_alternatives,
            current_python_version,
        ),
        candidates: ordered_candidates,
    })
}

fn insert_registry_tool_python_alternative(
    python_targets: &mut BTreeMap<Version, PythonVersion>,
    version: PythonVersion,
) {
    if !version.is_stable() {
        return;
    }
    let python_minor = version.python_version();
    if python_targets
        .get(&python_minor)
        .is_none_or(|current| version.version() > current.version())
    {
        python_targets.insert(python_minor, version);
    }
}

fn merge_registry_tool_python_alternatives(
    mut download_targets: BTreeMap<Version, PythonVersion>,
    installed_targets: BTreeMap<Version, PythonVersion>,
    current_version: PythonVersion,
) -> Vec<PythonVersion> {
    for (python_minor, installed_target) in installed_targets {
        download_targets
            .entry(python_minor)
            .or_insert(installed_target);
    }

    let current_python_minor = current_version.python_version();
    download_targets.insert(current_python_minor.clone(), current_version);

    let mut ordered_targets = Vec::with_capacity(download_targets.len());
    if let Some(current_target) = download_targets.remove(&current_python_minor) {
        ordered_targets.push(current_target);
    }
    ordered_targets.extend(download_targets.into_values().rev());
    ordered_targets
}

fn insert_registry_tool_python_candidate(
    python_targets: &mut BTreeMap<Version, RegistryToolPythonTarget>,
    version: PythonVersion,
    interpreter: Option<Interpreter>,
) {
    if !version.is_stable() {
        return;
    }
    let python_version = version.version().clone();
    let python_target = RegistryToolPythonTarget {
        version,
        interpreter,
    };
    if python_targets
        .get(&python_version)
        .is_none_or(|current| current.interpreter.is_none() && python_target.interpreter.is_some())
    {
        python_targets.insert(python_version, python_target);
    }
}

fn registry_tool_python_request(
    interpreter: &Interpreter,
    python_target: &PythonVersion,
) -> Option<PythonRequest> {
    let release = python_target.release();
    if release.len() != 3 || !python_target.is_stable() {
        return None;
    }
    let major = release.first()?;
    let minor = release.get(1)?;
    let patch = release.get(2)?;
    let request = PythonDownloadRequest::try_from(&interpreter.key())
        .ok()?
        .with_version(VersionRequest::MajorMinorPatch(
            u8::try_from(*major).ok()?,
            u8::try_from(*minor).ok()?,
            u8::try_from(*patch).ok()?,
            interpreter.variant(),
        ))
        .with_prereleases(false);
    Some(PythonRequest::Key(request))
}

fn registry_tool_python_minor_marker(python_target: &PythonVersion) -> MarkerTree {
    MarkerTree::expression(MarkerExpression::Version {
        key: MarkerValueVersion::PythonVersion,
        specifier: VersionSpecifier::equals_version(python_target.python_version()),
    })
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn registry_tool_python_minor_marker_uses_python_version() {
        let python_target = PythonVersion::from_str("3.15.0b1").expect("valid Python version");

        assert_eq!(
            registry_tool_python_minor_marker(&python_target)
                .try_to_string()
                .expect("serializable marker"),
            "python_full_version == '3.15.*'"
        );
    }

    #[test]
    fn registry_tool_python_alternatives_select_latest_stable_patch() {
        let mut targets = BTreeMap::new();
        for version in ["3.14.1", "3.14.2", "3.15.0b1"] {
            insert_registry_tool_python_alternative(
                &mut targets,
                PythonVersion::from_str(version).expect("valid Python version"),
            );
        }

        assert_eq!(
            targets
                .into_values()
                .map(|target| target.version.to_string())
                .collect::<Vec<_>>(),
            ["3.14.2"]
        );
    }

    #[test]
    fn registry_tool_python_alternatives_prefer_downloads_and_current() {
        let mut download_targets = BTreeMap::new();
        let mut installed_targets = BTreeMap::new();
        for version in ["3.13.2", "3.14.2"] {
            insert_registry_tool_python_alternative(
                &mut download_targets,
                PythonVersion::from_str(version).expect("valid Python version"),
            );
        }
        for version in ["3.12.1", "3.12.2", "3.13.1", "3.14.3"] {
            insert_registry_tool_python_alternative(
                &mut installed_targets,
                PythonVersion::from_str(version).expect("valid Python version"),
            );
        }

        assert_eq!(
            merge_registry_tool_python_alternatives(
                download_targets,
                installed_targets,
                PythonVersion::from_str("3.14.1").expect("valid Python version"),
            )
            .into_iter()
            .map(|target| target.version.to_string())
            .collect::<Vec<_>>(),
            ["3.14.1", "3.13.2", "3.12.2"]
        );
    }

    #[test]
    fn registry_tool_python_candidates_keep_same_minor_patches() {
        let mut targets = BTreeMap::new();
        for version in ["3.14.1", "3.14.2", "3.15.0b1"] {
            insert_registry_tool_python_candidate(
                &mut targets,
                PythonVersion::from_str(version).expect("valid Python version"),
                None,
            );
        }

        assert_eq!(
            targets
                .into_values()
                .map(|target| target.version.to_string())
                .collect::<Vec<_>>(),
            ["3.14.1", "3.14.2"]
        );
    }
}
