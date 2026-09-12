use std::env;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::ValueEnum;
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::FxHashSet;
use serde::Deserialize;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{
    ActiveEnvironment, Concurrency, DependencyGroups, DependencyGroupsWithDefaults, EditableMode,
    ExportFormat, ExtrasSpecification, ExtrasSpecificationWithDefaults, InstallOptions,
};
use uv_distribution_types::Verbatim;
use uv_normalize::{DefaultExtras, DefaultGroups, ExtraName, GroupName, PackageName};
use uv_preview::{Preview, PreviewFeature};
use uv_python::{ConfigDiscovery, PythonDownloads, PythonPreference, PythonRequest};
use uv_requirements::is_pylock_toml;
use uv_resolver::{Installable, Lock, PylockToml, RequirementsTxtExport, cyclonedx_json};
use uv_scripts::Pep723Script;
use uv_settings::PythonInstallMirrors;
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, MemberDiscovery, VirtualProject, WorkspaceCache};

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::{LockMode, LockOperation};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    ProjectEnvironmentPolicy, ProjectInterpreter, ScriptInterpreter, UniversalState,
    WorkspacePython, default_dependency_groups, detect_conflicts,
};
use crate::commands::{ExitStatus, OutputWriter, UvError};
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

impl<'lock> From<&'lock ExportTarget> for LockTarget<'lock> {
    fn from(value: &'lock ExportTarget) -> Self {
        match value {
            ExportTarget::Script(script) => Self::Script(script),
            ExportTarget::Project(project) => Self::Workspace(project.workspace()),
        }
    }
}

/// Independent selections and destinations for a frozen batch export.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportBatch {
    export: Vec<BatchExport>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct BatchExport {
    output_file: PathBuf,
    #[serde(default)]
    package: Vec<PackageName>,
    #[serde(default)]
    all_packages: bool,
    #[serde(default)]
    extra: Vec<ExtraName>,
    #[serde(default)]
    no_extra: Vec<ExtraName>,
    #[serde(default)]
    all_extras: bool,
    #[serde(default)]
    group: Vec<GroupName>,
    #[serde(default)]
    no_group: Vec<GroupName>,
    #[serde(default)]
    only_group: Vec<GroupName>,
    #[serde(default)]
    all_groups: bool,
    #[serde(default)]
    no_default_groups: bool,
}

impl ExportBatch {
    /// Read and validate the manifest, resolving output paths relative to its directory.
    async fn read(path: &Path) -> Result<Self> {
        let contents = fs_err::tokio::read_to_string(path).await?;
        let mut batch: Self = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse export manifest `{}`", path.display()))?;
        if batch.export.is_empty() {
            bail!("Export manifest must contain at least one `[[export]]` entry");
        }
        let parent = path.parent().unwrap_or(Path::new("."));
        let mut outputs = FxHashSet::default();
        for entry in &mut batch.export {
            entry.output_file = uv_fs::normalize_absolute_path(&std::path::absolute(
                parent.join(&entry.output_file),
            )?)?;
            if !outputs.insert(entry.output_file.clone()) {
                bail!("Duplicate export output: `{}`", entry.output_file.display());
            }
            if entry.all_packages && !entry.package.is_empty() {
                bail!("`all-packages` cannot be combined with `package`");
            }
            if entry.all_extras && !entry.extra.is_empty() {
                bail!("`all-extras` cannot be combined with `extra`");
            }
            if !entry.only_group.is_empty() && (!entry.extra.is_empty() || entry.all_extras) {
                bail!("`only-group` cannot be combined with `extra` or `all-extras`");
            }
            if !entry.only_group.is_empty() && (!entry.group.is_empty() || entry.all_groups) {
                bail!("`only-group` cannot be combined with `group` or `all-groups`");
            }
        }
        Ok(batch)
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
    batch: Option<PathBuf>,
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
    config_discovery: ConfigDiscovery,
    quiet: bool,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let batch = if let Some(path) = batch {
        if !preview.is_enabled(PreviewFeature::BatchExport) {
            warn_user!(
                "`uv export --batch` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
                PreviewFeature::BatchExport
            );
        }
        let Some(frozen_source) = frozen else {
            bail!("`--batch` requires `--frozen`");
        };
        Some((ExportBatch::read(&path).await?, frozen_source))
    } else {
        None
    };

    // Identify the target.
    let target = if let Some(script) = script {
        ExportTarget::Script(script)
    } else {
        let project = if frozen.is_some() {
            let options = DiscoveryOptions {
                members: if package.is_empty()
                    && batch.as_ref().is_none_or(|(batch, _)| {
                        batch.export.iter().all(|entry| entry.package.is_empty())
                    }) {
                    MemberDiscovery::None
                } else {
                    MemberDiscovery::Existing
                },
                ..DiscoveryOptions::default()
            };

            if let [name] = package.as_slice() {
                VirtualProject::discover_with_package(
                    project_dir,
                    &options,
                    cache,
                    workspace_cache,
                    name.clone(),
                )
                .await?
            } else {
                VirtualProject::discover(project_dir, &options, cache, workspace_cache).await?
            }
        } else if let [name] = package.as_slice() {
            VirtualProject::discover_with_package(
                project_dir,
                &DiscoveryOptions::default(),
                cache,
                workspace_cache,
                name.clone(),
            )
            .await?
        } else {
            let project = VirtualProject::discover(
                project_dir,
                &DiscoveryOptions::default(),
                cache,
                workspace_cache,
            )
            .await?;

            for name in &package {
                if !project.workspace().packages().contains_key(name) {
                    return Err(anyhow::anyhow!("Package `{name}` not found in workspace"));
                }
            }

            project
        };
        ExportTarget::Project(project)
    };

    if let Some((batch, frozen_source)) = &batch {
        let ExportTarget::Project(project) = &target else {
            bail!("`--batch` does not support scripts");
        };
        let lock = LockTarget::from(&target)
            .read_frozen((*frozen_source).into())
            .await
            .map_err(UvError::from)?;
        let mut writers = Vec::with_capacity(batch.export.len());
        for entry in &batch.export {
            let pyproject = if let [name] = entry.package.as_slice() {
                project
                    .workspace()
                    .packages()
                    .get(name)
                    .ok_or_else(|| anyhow!("Package `{name}` not found in workspace"))?
                    .pyproject_toml()
            } else {
                for name in &entry.package {
                    if !project.workspace().packages().contains_key(name) {
                        bail!("Package `{name}` not found in workspace");
                    }
                }
                project.pyproject_toml()
            };
            let groups = DependencyGroups::from_args(
                None,
                entry.group.clone(),
                entry.no_group.clone(),
                entry.no_default_groups,
                entry.only_group.clone(),
                entry.all_groups,
            )
            .with_defaults(default_dependency_groups(pyproject)?);
            let extras = ExtrasSpecification::from_args(
                entry.extra.clone(),
                entry.no_extra.clone(),
                false,
                vec![],
                entry.all_extras,
            )
            .with_defaults(DefaultExtras::default());
            writers.push(
                render_export(
                    &target,
                    &lock,
                    format,
                    entry.all_packages,
                    &entry.package,
                    &prune,
                    hashes,
                    &install_options,
                    Some(&entry.output_file),
                    &extras,
                    &groups,
                    editable.clone(),
                    include_annotations,
                    include_header,
                    include_index_url,
                    include_find_links,
                    &settings,
                    &client_builder,
                    &concurrency,
                    true,
                    cache,
                    preview,
                )
                .await
                .with_context(|| format!("Failed to export `{}`", entry.output_file.display()))?,
            );
        }
        // Render every selection before replacing any output, so invalid selections leave files intact.
        for writer in writers {
            writer.commit().await?;
        }
        return Ok(ExitStatus::Success);
    }

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

    let groups = groups.with_defaults(default_groups);
    let extras = extras.with_defaults(default_extras);

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
                false,
                config_discovery,
                ActiveEnvironment::Ignore,
                cache,
                printer,
            )
            .await?
            .into_interpreter(),
            ExportTarget::Project(project) => {
                let workspace_python = WorkspacePython::from_request(
                    python.as_deref().map(PythonRequest::parse),
                    Some(project.workspace()),
                    &groups,
                    project_dir,
                    config_discovery,
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
                    ProjectEnvironmentPolicy::Optional,
                    ActiveEnvironment::Ignore,
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
            workspace_cache,
            printer,
            preview,
        )
        .execute((&target).into()),
    )
    .await
    {
        Ok(result) => result.into_lock(),
        Err(err) => return Err(UvError::from(err).into()),
    };

    render_export(
        &target,
        &lock,
        format,
        all_packages,
        &package,
        &prune,
        hashes,
        &install_options,
        output_file.as_deref(),
        &extras,
        &groups,
        editable,
        include_annotations,
        include_header,
        include_index_url,
        include_find_links,
        &settings,
        &client_builder,
        &concurrency,
        quiet,
        cache,
        preview,
    )
    .await?
    .commit()
    .await?;

    Ok(ExitStatus::Success)
}

/// Render one selection from a shared lockfile, deferring its file write until validation completes.
#[expect(clippy::fn_params_excessive_bools)]
async fn render_export<'output>(
    target: &ExportTarget,
    lock: &Lock,
    format: Option<ExportFormat>,
    all_packages: bool,
    package: &[PackageName],
    prune: &[PackageName],
    hashes: bool,
    install_options: &InstallOptions,
    output_file: Option<&'output Path>,
    extras: &ExtrasSpecificationWithDefaults,
    groups: &DependencyGroupsWithDefaults,
    editable: Option<EditableMode>,
    include_annotations: bool,
    include_header: bool,
    include_index_url: bool,
    include_find_links: bool,
    settings: &ResolverSettings,
    client_builder: &BaseClientBuilder<'_>,
    concurrency: &Concurrency,
    quiet: bool,
    cache: &Cache,
    preview: Preview,
) -> Result<OutputWriter<'output>> {
    // Identify the installation target.
    let target = match target {
        ExportTarget::Project(VirtualProject::Project(project)) => {
            if all_packages {
                InstallTarget::Workspace {
                    workspace: project.workspace(),
                    lock,
                }
            } else {
                match package {
                    // By default, install the root project.
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
            if all_packages {
                InstallTarget::NonProjectWorkspace { workspace, lock }
            } else {
                match package {
                    // By default, install the entire workspace.
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
    };

    // Validate that the set of requested extras and development groups are defined in the lockfile.
    target.validate_extras(extras)?;
    target.validate_groups(groups)?;

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

    // Write the resolved dependencies to the output channel.
    let mut writer = OutputWriter::new(!quiet || output_file.is_none(), output_file);

    // Determine the output format.
    let format = format.unwrap_or_else(|| {
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
    });

    // Skip conflict detection for CycloneDX exports, as SBOMs are meant to document all dependencies including conflicts.
    if !matches!(format, ExportFormat::CycloneDX1_5) {
        detect_conflicts(&target, extras, groups)?;
    }

    // If the user is exporting to PEP 751, ensure the filename matches the specification.
    if matches!(format, ExportFormat::PylockToml) {
        if let Some(file_name) = output_file
            .and_then(Path::file_name)
            .and_then(OsStr::to_str)
        {
            if !is_pylock_toml(file_name) {
                return Err(anyhow!(
                    "Expected the output filename to be `pylock.toml` or `pylock.<name>.toml`, where `<name>` is non-empty and contains no dots; found `{file_name}`",
                ));
            }
        }
    }

    // Generate the export.
    match format {
        ExportFormat::RequirementsTxt => {
            let export = RequirementsTxtExport::from_lock(
                &target,
                prune,
                extras,
                groups,
                include_annotations,
                editable,
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
        }
        ExportFormat::PylockToml => {
            let mut export = PylockToml::from_lock(
                &target,
                prune,
                extras,
                groups,
                include_annotations,
                editable.as_ref(),
                install_options,
            )?;

            // Registries don't always provide hashes, but `packages.*.hashes` is a required
            // key in PEP 751, so we have to download and hash files with missing hashes.
            if export.has_missing_hashes() {
                let client = RegistryClientBuilder::new(client_builder.clone(), cache.clone())
                    .index_locations(settings.index_locations.clone())
                    .build()?;
                export
                    .generate_missing_hashes(&client, concurrency.downloads, target.install_path())
                    .await?;
            }

            if include_header {
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
                &target,
                prune,
                extras,
                groups,
                include_annotations,
                hashes,
                install_options,
                preview,
                all_packages,
            )?;

            export.output_as_json_v1_5(&mut writer)?;
        }
    }

    Ok(writer)
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
