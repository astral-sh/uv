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
use uv_command_support::{ExitStatus, OutputWriter, Printer, UvError};
use uv_configuration::{
    ActiveEnvironment, Concurrency, DependencyGroups, DependencyGroupsWithDefaults, EditableMode,
    ExportFormat, ExtrasSpecification, ExtrasSpecificationWithDefaults, InstallOptions,
};
use uv_dispatch::UniversalState;
use uv_distribution_types::Verbatim;
use uv_environment_operations::install_target::{InstallTarget, PackageSelection};
use uv_environment_operations::{
    ProjectEnvironmentPolicy, ProjectEnvironmentTarget, ProjectInterpreter, detect_conflicts,
};
use uv_fs::CWD;
use uv_lock::{Lock, PylockToml, RequirementsTxtExport, cyclonedx_json};
use uv_lock_operations::{DiscoveredProject, FrozenWorkspace, LockMode, LockOperation, LockTarget};
use uv_normalize::{DefaultExtras, DefaultGroups, ExtraName, GroupName, PackageName};
use uv_preview::{Preview, PreviewFeature};
use uv_python_discovery::ConfigDiscovery;
use uv_python_discovery::ProjectPythonRequest;
use uv_python_discovery::ScriptInterpreter;
use uv_python_types::{PythonArchitecture, PythonDownloads, PythonPreference, PythonRequest};
use uv_requirements::is_pylock_toml;
use uv_resolve_operations::loggers::DefaultResolveLogger;
use uv_scripts::Pep723Script;
use uv_settings::{FrozenSource, LockCheck, PythonInstallMirrors, ResolverSettings};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, MemberDiscovery, VirtualProject, WorkspaceCache};

#[derive(Debug, Clone)]
#[expect(clippy::large_enum_variant)]
enum ExportTarget {
    /// A PEP 723 script, with inline metadata.
    Script(Pep723Script),

    /// A project with a `pyproject.toml`.
    Project(VirtualProject),
}

impl<'lock> From<&'lock ExportTarget> for LockTarget<'lock> {
    fn from(target: &'lock ExportTarget) -> Self {
        match target {
            ExportTarget::Script(script) => Self::Script(script),
            ExportTarget::Project(project) => Self::Workspace(project.workspace()),
        }
    }
}

/// An export reads an existing workspace lock or resolves a project or script manifest.
#[derive(Debug)]
enum ExportSource<'a> {
    Manifest(&'a ExportTarget),
    Lockfile {
        workspace: &'a FrozenWorkspace,
        project_name: Option<PackageName>,
    },
}

/// Independent selections and destinations for a batch export.
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
                bail!("Duplicate export output: {}", entry.output_file.display());
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

/// Resolve export groups, preferring the defaults of a single selected package.
fn resolve_lockfile_groups(
    groups: &DependencyGroups,
    workspace: &FrozenWorkspace,
    project: Option<&PackageName>,
    packages: &[PackageName],
) -> Result<DependencyGroupsWithDefaults> {
    let project = match packages {
        [name] => Some(name),
        _ => project,
    };
    workspace.resolve_groups(groups, project)
}

/// Export the project's `uv.lock` in an alternate format.
#[expect(clippy::fn_params_excessive_bools)]
pub async fn export(
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
    python_arch: Option<PythonArchitecture>,
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
        Some(ExportBatch::read(&path).await?)
    } else {
        None
    };

    // Identify the target.
    let manifest_target;
    let frozen_workspace;
    let source = if let Some(script) = script {
        manifest_target = ExportTarget::Script(script);
        ExportSource::Manifest(&manifest_target)
    } else {
        let options = if frozen.is_some() {
            DiscoveryOptions {
                members: if package.is_empty()
                    && batch.as_ref().is_none_or(|batch| {
                        batch.export.iter().all(|entry| entry.package.is_empty())
                    }) {
                    MemberDiscovery::None
                } else {
                    MemberDiscovery::Existing
                },
                ..DiscoveryOptions::default()
            }
        } else {
            DiscoveryOptions::default()
        };
        let selected_package = if let [name] = package.as_slice() {
            Some(name)
        } else {
            None
        };
        match DiscoveredProject::discover(
            project_dir,
            &options,
            selected_package,
            frozen,
            preview,
            cache,
            workspace_cache,
        )
        .await?
        {
            DiscoveredProject::Manifest(project) => {
                if frozen.is_none() {
                    for name in &package {
                        if !project.workspace().packages().contains_key(name) {
                            return Err(anyhow::anyhow!("Package `{name}` not found in workspace"));
                        }
                    }
                }
                manifest_target = ExportTarget::Project(project);
                ExportSource::Manifest(&manifest_target)
            }
            DiscoveredProject::Lockfile(workspace) => {
                if include_index_url || include_find_links {
                    bail!(
                        "`--emit-index-url` and `--emit-find-links` are not supported without a `pyproject.toml`"
                    );
                }
                frozen_workspace = workspace;
                let project_name = frozen_workspace.current_project(project_dir).cloned();
                ExportSource::Lockfile {
                    workspace: &frozen_workspace,
                    project_name,
                }
            }
        }
    };

    let resolved_lock;
    let lock = match &source {
        ExportSource::Lockfile { workspace, .. } => workspace.lock(),
        ExportSource::Manifest(target) => {
            // Find an interpreter for the project, unless `--frozen` is set.
            let interpreter = if frozen.is_some() {
                None
            } else {
                Some(match target {
                    ExportTarget::Script(script) => ScriptInterpreter::discover(
                        script.into(),
                        python.as_deref().map(PythonRequest::parse),
                        &client_builder,
                        python_preference,
                        python_arch,
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
                        // Selected groups can impose additional Python requirements on a single export.
                        // Batch entries may have incompatible group requirements, so choose the interpreter
                        // for locking using only workspace requirements, as `uv lock` does. Each output's
                        // groups and project defaults are applied separately when rendering below.
                        let interpreter_groups = if batch.is_some() {
                            DependencyGroupsWithDefaults::none()
                        } else {
                            groups.with_defaults(project.default_groups()?)
                        };
                        let project_python = ProjectPythonRequest::from_request(
                            python.as_deref().map(PythonRequest::parse),
                            Some(project.workspace()),
                            &interpreter_groups,
                            project_dir,
                            config_discovery,
                        )
                        .await?;
                        ProjectInterpreter::discover(
                            ProjectEnvironmentTarget::from(project.workspace()),
                            project_python,
                            &client_builder,
                            python_preference,
                            python_arch,
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
            } else if let ExportTarget::Script(script) = target
                && !LockTarget::Script(script).lock_path().is_file()
            {
                // If we're locking a script, avoid creating a lockfile if it doesn't already exist.
                LockMode::DryRun(interpreter.as_ref().unwrap())
            } else {
                LockMode::Write(interpreter.as_ref().unwrap())
            };

            // Initialize any shared state.
            let state = UniversalState::default();

            resolved_lock = match Box::pin(
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
                .execute((*target).into()),
            )
            .await
            {
                Ok(result) => result.into_lock(),
                Err(err) => return Err(UvError::from(err).into()),
            };
            &resolved_lock
        }
    };

    if let Some(batch) = &batch {
        let mut writers = Vec::with_capacity(batch.export.len());
        for entry in &batch.export {
            let groups = DependencyGroups::from_args(
                None,
                entry.group.clone(),
                entry.no_group.clone(),
                entry.no_default_groups,
                entry.only_group.clone(),
                entry.all_groups,
            );
            let groups = match &source {
                ExportSource::Manifest(ExportTarget::Project(project)) => {
                    groups.with_defaults(project.default_groups_for_packages(&entry.package)?)
                }
                ExportSource::Lockfile {
                    workspace,
                    project_name,
                } => {
                    workspace.validate_packages(&entry.package)?;
                    resolve_lockfile_groups(
                        &groups,
                        workspace,
                        project_name.as_ref(),
                        &entry.package,
                    )
                    .with_context(|| {
                        format!(
                            "Failed to resolve dependency groups for batch export `{}`",
                            entry.output_file.display()
                        )
                    })?
                }
                ExportSource::Manifest(ExportTarget::Script(_)) => {
                    bail!("`--batch` does not support scripts")
                }
            };
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
                    &source,
                    lock,
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

    let groups = match &source {
        ExportSource::Manifest(ExportTarget::Project(project)) => {
            groups.with_defaults(project.default_groups()?)
        }
        ExportSource::Manifest(ExportTarget::Script(_)) => {
            groups.with_defaults(DefaultGroups::default())
        }
        ExportSource::Lockfile {
            workspace,
            project_name,
        } => {
            workspace.validate_packages(&package)?;
            resolve_lockfile_groups(&groups, workspace, project_name.as_ref(), &package)?
        }
    };
    let extras = extras.with_defaults(DefaultExtras::default());

    render_export(
        &source,
        lock,
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
    source: &ExportSource<'_>,
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
    let target = match source {
        ExportSource::Manifest(ExportTarget::Project(project)) => InstallTarget::from_project(
            project,
            lock,
            PackageSelection::from_args(all_packages, package, project.project_name()),
        ),
        ExportSource::Manifest(ExportTarget::Script(script)) => {
            InstallTarget::Script { script, lock }
        }
        ExportSource::Lockfile {
            workspace,
            project_name,
        } => InstallTarget::Lockfile {
            root: workspace.root(),
            project_name: project_name.as_ref(),
            selection: PackageSelection::from_args(all_packages, package, project_name.as_ref()),
            lock,
        },
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
            let output_file = output_file.map(std::path::absolute).transpose()?;
            let output_dir = output_file
                .as_deref()
                .and_then(Path::parent)
                .unwrap_or(&CWD);
            let mut export = PylockToml::from_lock(
                &target,
                output_dir,
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
                    .generate_missing_hashes(&client, concurrency.downloads, output_dir)
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
