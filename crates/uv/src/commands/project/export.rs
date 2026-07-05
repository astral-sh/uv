use std::env;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::ValueEnum;
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::FxHashSet;
use serde::Deserialize;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::{
    Concurrency, DependencyGroups, DependencyGroupsWithDefaults, EditableMode, ExportFormat,
    ExtrasSpecification, ExtrasSpecificationWithDefaults, InstallOptions,
};
use uv_distribution_types::Verbatim;
use uv_normalize::{DefaultExtras, DefaultGroups, ExtraName, GroupName, PackageName};
use uv_preview::Preview;
use uv_python::{PythonDownloads, PythonPreference, PythonRequest};
use uv_requirements::is_pylock_toml;
use uv_resolver::{Lock, PylockToml, RequirementsTxtExport, cyclonedx_json};
use uv_scripts::Pep723Script;
use uv_settings::PythonInstallMirrors;
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, MemberDiscovery, VirtualProject, WorkspaceCache};

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::{LockMode, LockOperation};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    ProjectError, ProjectInterpreter, ScriptInterpreter, UniversalState, WorkspacePython,
    default_dependency_groups, detect_conflicts,
};
use crate::commands::{ExitStatus, OutputWriter, diagnostics};
use crate::printer::Printer;
use crate::settings::{FrozenSource, LockCheck, ResolverSettings};

#[derive(Debug, Clone)]
#[expect(clippy::large_enum_variant)]
enum ExportTarget {
    /// A PEP 723 script, with inline metadata.
    Script(Pep723Script),

    /// A project with a `pyproject.toml`.
    Project(VirtualProject),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportPlan {
    projection: Vec<ExportProjectionWire>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct ExportProjectionWire {
    format: Option<ExportFormat>,
    all_packages: bool,
    package: Vec<PackageName>,
    prune: Vec<PackageName>,
    extra: Vec<ExtraName>,
    all_extras: bool,
    no_extra: Vec<ExtraName>,
    dev: Option<bool>,
    only_dev: bool,
    group: Vec<GroupName>,
    no_group: Vec<GroupName>,
    no_default_groups: bool,
    only_group: Vec<GroupName>,
    all_groups: bool,
    annotate: Option<bool>,
    header: Option<bool>,
    emit_index_url: Option<bool>,
    emit_find_links: Option<bool>,
    editable: Option<bool>,
    no_editable_package: Vec<PackageName>,
    hashes: Option<bool>,
    output_file: Option<PathBuf>,
    no_emit_project: bool,
    only_emit_project: bool,
    no_emit_workspace: bool,
    only_emit_workspace: bool,
    no_emit_local: bool,
    only_emit_local: bool,
    no_emit_package: Vec<PackageName>,
    only_emit_package: Vec<PackageName>,
}

#[derive(Debug)]
struct ExportProjection {
    format: Option<ExportFormat>,
    all_packages: bool,
    package: Vec<PackageName>,
    prune: Vec<PackageName>,
    hashes: bool,
    install_options: InstallOptions,
    output_file: Option<PathBuf>,
    extras: ExtrasSpecification,
    groups: DependencyGroups,
    editable: Option<EditableMode>,
    include_annotations: bool,
    include_header: bool,
    include_index_url: bool,
    include_find_links: bool,
}

struct PreparedExport<'a> {
    projection: &'a ExportProjection,
    target: InstallTarget<'a>,
    format: ExportFormat,
    extras: ExtrasSpecificationWithDefaults,
    groups: DependencyGroupsWithDefaults,
}

impl ExportProjectionWire {
    fn resolve(self) -> Result<ExportProjection> {
        let Self {
            format,
            all_packages,
            package,
            prune,
            extra,
            all_extras,
            no_extra,
            dev,
            only_dev,
            group,
            no_group,
            no_default_groups,
            only_group,
            all_groups,
            annotate,
            header,
            emit_index_url,
            emit_find_links,
            editable,
            no_editable_package,
            hashes,
            output_file,
            no_emit_project,
            only_emit_project,
            no_emit_workspace,
            only_emit_workspace,
            no_emit_local,
            only_emit_local,
            no_emit_package,
            only_emit_package,
        } = self;

        if all_packages && (!package.is_empty() || !prune.is_empty()) {
            return Err(anyhow!(
                "`all-packages` cannot be combined with `package` or `prune`"
            ));
        }
        if all_extras && !extra.is_empty() {
            return Err(anyhow!("`all-extras` cannot be combined with `extra`"));
        }
        if !only_group.is_empty() && (!extra.is_empty() || all_extras) {
            return Err(anyhow!(
                "`only-group` cannot be combined with `extra` or `all-extras`"
            ));
        }
        if only_dev && (!group.is_empty() || all_groups || dev == Some(false)) {
            return Err(anyhow!(
                "`only-dev` cannot be combined with `group`, `all-groups`, or `dev = false`"
            ));
        }
        if !only_group.is_empty()
            && (!group.is_empty() || all_groups || dev == Some(true) || only_dev)
        {
            return Err(anyhow!(
                "`only-group` cannot be combined with `group`, `all-groups`, `dev = true`, or `only-dev`"
            ));
        }
        if no_emit_project && only_emit_project {
            return Err(anyhow!(
                "`no-emit-project` cannot be combined with `only-emit-project`"
            ));
        }
        if no_emit_workspace && only_emit_workspace {
            return Err(anyhow!(
                "`no-emit-workspace` cannot be combined with `only-emit-workspace`"
            ));
        }
        if no_emit_local && only_emit_local {
            return Err(anyhow!(
                "`no-emit-local` cannot be combined with `only-emit-local`"
            ));
        }
        if !no_emit_package.is_empty() && !only_emit_package.is_empty() {
            return Err(anyhow!(
                "`no-emit-package` cannot be combined with `only-emit-package`"
            ));
        }

        let output_file = output_file
            .ok_or_else(|| anyhow!("Every export projection must define `output-file`"))?;
        let (dev, no_dev) = match dev {
            Some(true) => (true, false),
            Some(false) => (false, true),
            None => (false, false),
        };

        Ok(ExportProjection {
            format,
            all_packages,
            package,
            prune,
            hashes: hashes.unwrap_or(true),
            install_options: InstallOptions::new(
                no_emit_project,
                only_emit_project,
                no_emit_workspace,
                only_emit_workspace,
                no_emit_local,
                only_emit_local,
                no_emit_package,
                only_emit_package,
            ),
            output_file: Some(output_file),
            extras: ExtrasSpecification::from_args(extra, no_extra, false, vec![], all_extras),
            groups: DependencyGroups::from_args(
                dev,
                no_dev,
                only_dev,
                group,
                no_group,
                no_default_groups,
                only_group,
                all_groups,
            ),
            editable: EditableMode::from_args(editable, no_editable_package),
            include_annotations: annotate.unwrap_or(true),
            include_header: header.unwrap_or(true),
            include_index_url: emit_index_url.unwrap_or(false),
            include_find_links: emit_find_links.unwrap_or(false),
        })
    }
}

fn read_export_plan(path: &Path) -> Result<Vec<ExportProjection>> {
    let contents = fs_err::read_to_string(path)
        .with_context(|| format!("Failed to read export plan at `{}`", path.display()))?;
    let plan = toml::from_str::<ExportPlan>(&contents)
        .with_context(|| format!("Failed to parse export plan at `{}`", path.display()))?;

    if plan.projection.is_empty() {
        return Err(anyhow!(
            "Export plan must contain at least one `[[projection]]`"
        ));
    }

    plan.projection
        .into_iter()
        .enumerate()
        .map(|(index, projection)| {
            projection
                .resolve()
                .with_context(|| format!("Invalid export projection {}", index + 1))
        })
        .collect()
}

impl<'lock> From<&'lock ExportTarget> for LockTarget<'lock> {
    fn from(value: &'lock ExportTarget) -> Self {
        match value {
            ExportTarget::Script(script) => Self::Script(script),
            ExportTarget::Project(project) => Self::Workspace(project.workspace()),
        }
    }
}

fn installation_target<'a>(
    target: &'a ExportTarget,
    lock: &'a Lock,
    projection: &'a ExportProjection,
) -> InstallTarget<'a> {
    match target {
        ExportTarget::Project(VirtualProject::Project(project)) => {
            if projection.all_packages {
                InstallTarget::Workspace {
                    workspace: project.workspace(),
                    lock,
                }
            } else {
                match projection.package.as_slice() {
                    [] => InstallTarget::Project {
                        workspace: project.workspace(),
                        name: project.project_name(),
                        lock,
                    },
                    [name] => InstallTarget::Project {
                        workspace: project.workspace(),
                        name,
                        lock,
                    },
                    names => InstallTarget::Projects {
                        workspace: project.workspace(),
                        names,
                        lock,
                    },
                }
            }
        }
        ExportTarget::Project(VirtualProject::NonProject(workspace)) => {
            if projection.all_packages {
                InstallTarget::NonProjectWorkspace { workspace, lock }
            } else {
                match projection.package.as_slice() {
                    [] => InstallTarget::NonProjectWorkspace { workspace, lock },
                    [name] => InstallTarget::Project {
                        workspace,
                        name,
                        lock,
                    },
                    names => InstallTarget::Projects {
                        workspace,
                        names,
                        lock,
                    },
                }
            }
        }
        ExportTarget::Script(script) => InstallTarget::Script { script, lock },
    }
}

/// Export the project's `uv.lock` in an alternate format.
#[expect(clippy::fn_params_excessive_bools)]
pub(crate) async fn export(
    project_dir: &Path,
    format: Option<ExportFormat>,
    all_packages: bool,
    package: Vec<PackageName>,
    prune: Vec<PackageName>,
    hashes: bool,
    install_options: InstallOptions,
    output_file: Option<PathBuf>,
    plan: Option<PathBuf>,
    extras: ExtrasSpecification,
    groups: DependencyGroups,
    editable: Option<EditableMode>,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    include_annotations: bool,
    include_header: bool,
    include_index_url: bool,
    include_find_links: bool,
    script: Option<Pep723Script>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverSettings,
    client_builder: BaseClientBuilder<'_>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    no_config: bool,
    quiet: bool,
    cache: &Cache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let using_plan = plan.is_some();
    let projections = if let Some(plan) = plan {
        read_export_plan(&plan)?
    } else {
        vec![ExportProjection {
            format,
            all_packages,
            package,
            prune,
            hashes,
            install_options,
            output_file,
            extras,
            groups,
            editable,
            include_annotations,
            include_header,
            include_index_url,
            include_find_links,
        }]
    };

    // Identify the target.
    let workspace_cache = WorkspaceCache::default();
    let target = if let Some(script) = script {
        ExportTarget::Script(script)
    } else {
        let project = if frozen.is_some() {
            VirtualProject::discover(
                project_dir,
                &DiscoveryOptions {
                    members: MemberDiscovery::None,
                    ..DiscoveryOptions::default()
                },
                cache,
                &workspace_cache,
            )
            .await?
        } else if let [projection] = projections.as_slice()
            && let [name] = projection.package.as_slice()
        {
            VirtualProject::discover_with_package(
                project_dir,
                &DiscoveryOptions::default(),
                cache,
                &workspace_cache,
                name.clone(),
            )
            .await?
        } else {
            let project = VirtualProject::discover(
                project_dir,
                &DiscoveryOptions::default(),
                cache,
                &workspace_cache,
            )
            .await?;

            for projection in &projections {
                for name in &projection.package {
                    if !project.workspace().packages().contains_key(name) {
                        return Err(anyhow::anyhow!("Package `{name}` not found in workspace"));
                    }
                }
            }

            project
        };
        ExportTarget::Project(project)
    };

    // Determine the default groups to include.
    let default_groups = match &target {
        ExportTarget::Project(project) => default_dependency_groups(project.pyproject_toml())?,
        ExportTarget::Script(_) => DefaultGroups::default(),
    };

    // Determine the default extras to include.
    let default_extras = match &target {
        ExportTarget::Project(_project) => DefaultExtras::default(),
        ExportTarget::Script(_) => DefaultExtras::default(),
    };

    // Find an interpreter for the project, unless `--frozen` is set.
    let interpreter = if frozen.is_some() {
        None
    } else {
        Some(match &target {
            ExportTarget::Script(script) => ScriptInterpreter::discover(
                script.into(),
                python.as_deref().map(PythonRequest::parse),
                &client_builder,
                python_preference,
                python_downloads,
                &install_mirrors,
                no_config,
                false,
                Some(false),
                cache,
                printer,
            )
            .await?
            .into_interpreter(),
            ExportTarget::Project(project) => {
                let groups = if using_plan {
                    DependencyGroupsWithDefaults::none()
                } else {
                    projections[0].groups.with_defaults(default_groups.clone())
                };
                let workspace_python = WorkspacePython::from_request(
                    python.as_deref().map(PythonRequest::parse),
                    Some(project.workspace()),
                    &groups,
                    project_dir,
                    no_config,
                )
                .await?;
                ProjectInterpreter::discover(
                    project.workspace(),
                    &groups,
                    workspace_python,
                    &client_builder,
                    python_preference,
                    python_downloads,
                    &install_mirrors,
                    false,
                    Some(false),
                    cache,
                    printer,
                )
                .await?
                .into_interpreter()
            }
        })
    };

    // Determine the lock mode.
    let mode = if let Some(frozen_source) = frozen {
        LockMode::Frozen(frozen_source.into())
    } else if let LockCheck::Enabled(lock_check) = lock_check {
        LockMode::Locked(interpreter.as_ref().unwrap(), lock_check)
    } else if matches!(target, ExportTarget::Script(_))
        && !LockTarget::from(&target).lock_path().is_file()
    {
        // If we're locking a script, avoid creating a lockfile if it doesn't already exist.
        LockMode::DryRun(interpreter.as_ref().unwrap())
    } else {
        LockMode::Write(interpreter.as_ref().unwrap())
    };

    // Initialize any shared state.
    let state = UniversalState::default();

    // Lock the project.
    let lock = match Box::pin(
        LockOperation::new(
            mode,
            &settings,
            &client_builder,
            &state,
            Box::new(DefaultResolveLogger),
            &concurrency,
            cache,
            &workspace_cache,
            printer,
            preview,
        )
        .execute((&target).into()),
    )
    .await
    {
        Ok(result) => result.into_lock(),
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::with_system_certs(
                client_builder.system_certs(),
            )
            .report(err)
            .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
        Err(err) => return Err(err.into()),
    };

    let mut output_files = FxHashSet::default();
    let mut prepared = Vec::with_capacity(projections.len());
    for (index, projection) in projections.iter().enumerate() {
        if let Some(output_file) = &projection.output_file
            && !output_files.insert(output_file)
        {
            return Err(anyhow!(
                "Cannot write multiple exports to the same output file: `{}`",
                output_file.display()
            ));
        }

        validate_output_file(projection.output_file.as_deref())?;

        let target = installation_target(&target, &lock, projection);
        let extras = projection.extras.with_defaults(default_extras.clone());
        let groups = projection.groups.with_defaults(default_groups.clone());

        // Validate that the set of requested extras and development groups are defined in the lockfile.
        if using_plan {
            target
                .validate_extras(&extras)
                .with_context(|| format!("Invalid export projection {}", index + 1))?;
            target
                .validate_groups(&groups)
                .with_context(|| format!("Invalid export projection {}", index + 1))?;
        } else {
            target.validate_extras(&extras)?;
            target.validate_groups(&groups)?;
        }

        let format = export_format(projection.format, projection.output_file.as_deref());

        // Skip conflict detection for CycloneDX exports, as SBOMs are meant to document all dependencies including conflicts.
        if !matches!(format, ExportFormat::CycloneDX1_5) {
            if using_plan {
                detect_conflicts(&target, &extras, &groups)
                    .with_context(|| format!("Invalid export projection {}", index + 1))?;
            } else {
                detect_conflicts(&target, &extras, &groups)?;
            }
        }

        // If the user is exporting to PEP 751, ensure the filename matches the specification.
        if matches!(format, ExportFormat::PylockToml)
            && let Some(file_name) = projection
                .output_file
                .as_deref()
                .and_then(Path::file_name)
                .and_then(OsStr::to_str)
            && !is_pylock_toml(file_name)
        {
            return Err(anyhow!(
                "Expected the output filename to start with `pylock.` and end with `.toml` (e.g., `pylock.toml`, `pylock.dev.toml`); `{file_name}` won't be recognized as a `pylock.toml` file in subsequent commands",
            ));
        }

        prepared.push(PreparedExport {
            projection,
            target,
            format,
            extras,
            groups,
        });
    }

    // Render every projection before committing any output file.
    let mut writers = Vec::with_capacity(prepared.len());
    for export in &prepared {
        let projection = export.projection;
        let mut writer = OutputWriter::new(
            !using_plan && (!quiet || projection.output_file.is_none()),
            projection.output_file.as_deref(),
        );

        match export.format {
            ExportFormat::RequirementsTxt => {
                write_requirements_txt_export(
                    &mut writer,
                    &export.target,
                    &projection.prune,
                    projection.hashes,
                    &projection.install_options,
                    &export.extras,
                    &export.groups,
                    projection.editable.as_ref(),
                    projection.include_annotations,
                    projection.include_header,
                    projection.include_index_url,
                    projection.include_find_links,
                    &settings,
                )?;
            }
            ExportFormat::PylockToml => {
                let export = PylockToml::from_lock(
                    &export.target,
                    &projection.prune,
                    &export.extras,
                    &export.groups,
                    projection.include_annotations,
                    projection.editable.as_ref(),
                    &projection.install_options,
                )?;

                if projection.include_header {
                    writeln!(
                        writer,
                        "{}",
                        "# This file was autogenerated by uv via the following command:".green()
                    )?;
                    writeln!(writer, "{}", format!("#    {}", cmd()).green())?;
                }
                write!(writer, "{}", export.to_toml()?)?;
            }
            ExportFormat::CycloneDX1_5 => {
                let export = cyclonedx_json::from_lock(
                    &export.target,
                    &projection.prune,
                    &export.extras,
                    &export.groups,
                    projection.include_annotations,
                    &projection.install_options,
                    preview,
                    projection.all_packages,
                )?;

                export.output_as_json_v1_5(&mut writer)?;
            }
        }

        writers.push(writer);
    }

    for writer in writers {
        writer.commit().await?;
    }

    Ok(ExitStatus::Success)
}

fn export_format(format: Option<ExportFormat>, output_file: Option<&Path>) -> ExportFormat {
    format.unwrap_or_else(|| {
        if output_file
            .and_then(Path::extension)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
        {
            ExportFormat::RequirementsTxt
        } else if output_file
            .and_then(Path::file_name)
            .and_then(OsStr::to_str)
            .is_some_and(is_pylock_toml)
        {
            ExportFormat::PylockToml
        } else {
            ExportFormat::RequirementsTxt
        }
    })
}

fn validate_output_file(output_file: Option<&Path>) -> Result<()> {
    if output_file
        .and_then(Path::file_name)
        .is_some_and(|name| name.eq_ignore_ascii_case("pyproject.toml"))
    {
        return Err(anyhow!(
            "`pyproject.toml` is not a supported output format for `{}` (supported formats: {})",
            "uv export".green(),
            ExportFormat::value_variants()
                .iter()
                .filter_map(clap::ValueEnum::to_possible_value)
                .map(|value| value.get_name().to_string())
                .join(", ")
        ));
    }

    Ok(())
}

#[expect(clippy::fn_params_excessive_bools, clippy::too_many_arguments)]
fn write_requirements_txt_export(
    writer: &mut impl Write,
    target: &InstallTarget<'_>,
    prune: &[PackageName],
    hashes: bool,
    install_options: &InstallOptions,
    extras: &ExtrasSpecificationWithDefaults,
    groups: &DependencyGroupsWithDefaults,
    editable: Option<&EditableMode>,
    include_annotations: bool,
    include_header: bool,
    include_index_url: bool,
    include_find_links: bool,
    settings: &ResolverSettings,
) -> Result<()> {
    let export = RequirementsTxtExport::from_lock(
        target,
        prune,
        extras,
        groups,
        include_annotations,
        editable.cloned(),
        hashes,
        install_options,
    )?;

    if include_header {
        writeln!(
            writer,
            "{}",
            "# This file was autogenerated by uv via the following command:".green()
        )?;
        writeln!(writer, "{}", format!("#    {}", cmd()).green())?;
    }

    let mut wrote_preamble = false;

    // If necessary, include the `--index-url` and `--extra-index-url` locations.
    if include_index_url {
        let mut seen = FxHashSet::default();
        let mut emitted_explicit_index = false;

        if let Some(index) = settings.index_locations.default_index() {
            writeln!(writer, "--index-url {}", index.url().verbatim())?;
            seen.insert(index.url());
            wrote_preamble = true;
            emitted_explicit_index |= index.explicit;
        }
        for index in settings
            .index_locations
            .implicit_indexes()
            .chain(settings.index_locations.explicit_indexes())
        {
            if seen.insert(index.url()) {
                writeln!(writer, "--extra-index-url {}", index.url().verbatim())?;
                wrote_preamble = true;
            }
            emitted_explicit_index |= index.explicit;
        }

        if emitted_explicit_index {
            warn_user!(
                "`requirements.txt` does not support per-package index pinning; explicit indexes were emitted globally via `--extra-index-url`."
            );
        }
    }

    // If necessary, include the `--find-links` locations.
    if include_find_links {
        for flat_index in settings.index_locations.flat_indexes() {
            writeln!(writer, "--find-links {}", flat_index.url().verbatim())?;
            wrote_preamble = true;
        }
    }

    if wrote_preamble {
        writeln!(writer)?;
    }

    write!(writer, "{export}")?;

    Ok(())
}

/// Format the uv command used to generate the output file.
fn cmd() -> String {
    let args = env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().to_string())
        .scan(None, move |skip_next, arg| {
            if matches!(skip_next, Some(true)) {
                // Reset state; skip this iteration.
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--upgrade` flag.
            if arg == "--upgrade" || arg == "-U" {
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--upgrade-package` and mark the next item to be skipped
            if arg == "--upgrade-package" || arg == "-P" {
                *skip_next = Some(true);
                return Some(None);
            }

            // Skip only this argument if option and value are together
            if arg.starts_with("--upgrade-package=") || arg.starts_with("-P") {
                // Reset state; skip this iteration.
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--upgrade-group` and mark the next item to be skipped
            if arg == "--upgrade-group" {
                *skip_next = Some(true);
                return Some(None);
            }

            // Skip only this argument if option and value are together
            if arg.starts_with("--upgrade-group=") {
                // Reset state; skip this iteration.
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--quiet` flag.
            if arg == "--quiet" || arg == "-q" {
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--verbose` flag.
            if arg == "--verbose" || arg == "-v" {
                *skip_next = None;
                return Some(None);
            }

            // Return the argument.
            Some(Some(arg))
        })
        .flatten()
        .join(" ");
    format!("uv {args}")
}
