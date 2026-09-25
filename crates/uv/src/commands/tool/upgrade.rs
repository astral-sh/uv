use anyhow::{Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use std::collections::BTreeMap;
use std::fmt::Write;

use std::collections::BTreeSet;
use std::ffi::OsStr;
#[cfg(windows)]
use std::io::{BufReader, Read};
use std::path::Path;
use tracing::{debug, trace};

use uv_cache::Cache;
use uv_cache_key::CanonicalUrl;
use uv_client::BaseClientBuilder;
use uv_configuration::{Concurrency, Constraints, DryRun, HashCheckingMode, TargetTriple};
use uv_distribution::LoweredExtraBuildDependencies;
use uv_distribution_types::{ExtraBuildRequires, Index, Name, Requirement, RequirementSource};
use uv_fs::{CWD, Simplified};
use uv_installer::{InstallationStrategy, Planner, SitePackages};
use uv_normalize::PackageName;
use uv_pep440::{Operator, Version};
use uv_preview::{Preview, PreviewFeature};
use uv_python::PythonEnvironment;
use uv_python::{
    EnvironmentPreference, Interpreter, PythonDownloads, PythonInstallation, PythonPreference,
    PythonRequest,
};
use uv_requirements::RequirementsSpecification;
use uv_settings::{Combine, PythonInstallMirrors, ResolverInstallerOptions, ToolOptions};
use uv_tool::{InstalledTools, Tool, ToolEntrypoint, entrypoint_paths};
use uv_types::{HashStrategy, SourceTreeEditablePolicy};
use uv_workspace::WorkspaceCache;

use crate::commands::pip::loggers::{
    DefaultInstallLogger, SummaryResolveLogger, UpgradeInstallLogger,
};
use crate::commands::pip::{operations::Modifications, resolution_tags};
use crate::commands::project::{
    EnvironmentResolution, EnvironmentUpdate, PlatformState, resolve_environment, sync_environment,
    update_environment,
};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::tool::common::{ToolLock, remove_entrypoints, tool_environment_spec};
use crate::commands::{ExitStatus, conjunction, tool::common::finalize_tool_install};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Upgrade a tool.
pub(crate) async fn upgrade(
    names: Vec<String>,
    python: Option<String>,
    python_platform: Option<TargetTriple>,
    install_mirrors: PythonInstallMirrors,
    args: ResolverInstallerOptions,
    filesystem: ResolverInstallerOptions,
    client_builder: BaseClientBuilder<'_>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let installed_tools = InstalledTools::from_settings()?.init()?;
    let _lock = installed_tools.lock().await?;

    // Collect the tools to upgrade, along with any constraints.
    let names: BTreeMap<PackageName, Vec<Requirement>> = {
        if names.is_empty() {
            installed_tools
                .tools()
                .with_context(|| {
                    format!(
                        "Failed to inspect installed tools in `{}`",
                        installed_tools.root().user_display()
                    )
                })?
                .into_iter()
                .map(|(name, _)| (name, Vec::new()))
                .collect()
        } else {
            let mut map = BTreeMap::new();
            for name in names {
                let requirement = Requirement::from(uv_pep508::Requirement::parse(&name, &*CWD)?);
                map.entry(requirement.name.clone())
                    .or_insert_with(Vec::new)
                    .push(requirement);
            }
            map
        }
    };

    if names.is_empty() {
        writeln!(printer.stderr(), "Nothing to upgrade")?;
        return Ok(ExitStatus::Success);
    }

    let reporter = PythonDownloadReporter::single(printer);

    let python_request = python.as_deref().map(PythonRequest::parse);

    let interpreter = if python_request.is_some() {
        Some(
            PythonInstallation::find_or_download(
                python_request.as_ref(),
                EnvironmentPreference::OnlySystem,
                python_preference,
                python_downloads,
                &client_builder,
                cache,
                Some(&reporter),
                install_mirrors.python_install_mirror.as_deref(),
                install_mirrors.pypy_install_mirror.as_deref(),
                install_mirrors.python_downloads_json_url.as_deref(),
            )
            .await?
            .into_interpreter(),
        )
    } else {
        None
    };

    // Determine whether we applied any upgrades.
    let mut did_upgrade_tool = vec![];

    // Determine whether we applied any upgrades.
    let mut did_upgrade_environment = vec![];

    // Constraints that caused upgrades to be skipped or altered.
    let mut collected_constraints: Vec<(PackageName, UpgradeConstraint)> = Vec::new();

    let mut errors = Vec::new();
    for (name, constraints) in &names {
        debug!("Upgrading tool: `{name}`");
        let result = Box::pin(upgrade_tool(
            name,
            constraints,
            interpreter.as_ref(),
            python_platform.as_ref(),
            printer,
            &installed_tools,
            &args,
            &client_builder,
            cache,
            workspace_cache,
            &filesystem,
            installer_metadata,
            &concurrency,
            preview,
        ))
        .await;

        match result {
            Ok(report) => {
                match report.outcome {
                    UpgradeOutcome::UpgradeEnvironment => {
                        did_upgrade_environment.push(name);
                    }
                    UpgradeOutcome::UpgradeTool | UpgradeOutcome::UpgradeDependencies => {
                        did_upgrade_tool.push(name);
                    }
                    UpgradeOutcome::Entrypoints => {
                        did_upgrade_tool.push(name);
                    }
                    UpgradeOutcome::NoOp => {
                        debug!("Upgrading `{name}` was a no-op");
                    }
                }

                if let Some(constraint) = report.constraint.clone() {
                    collected_constraints.push((name.clone(), constraint));
                }
            }
            Err(err) => {
                errors.push((name, err));
            }
        }
    }

    if !errors.is_empty() {
        for (name, err) in errors
            .into_iter()
            .sorted_unstable_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b))
        {
            trace!("Error trace: {err:?}");
            crate::commands::diagnostics::write_error_chain(
                &err.context(format!("Failed to upgrade {}", name.green())),
                printer,
            )?;
        }
        return Ok(ExitStatus::Failure);
    }

    if did_upgrade_tool.is_empty() && did_upgrade_environment.is_empty() {
        writeln!(printer.stderr(), "Nothing to upgrade")?;
    }

    if let Some(python_request) = python_request {
        if !did_upgrade_environment.is_empty() {
            let tools = did_upgrade_environment
                .iter()
                .map(|name| format!("`{}`", name.cyan()))
                .collect::<Vec<_>>();
            let s = if tools.len() > 1 { "s" } else { "" };
            writeln!(
                printer.stderr(),
                "Upgraded tool environment{s} for {} to {}",
                conjunction(tools),
                python_request.cyan(),
            )?;
        }
    }

    if !collected_constraints.is_empty() {
        writeln!(printer.stderr())?;
    }

    for (name, constraint) in collected_constraints {
        constraint.print(&name, printer)?;
    }

    Ok(ExitStatus::Success)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpgradeOutcome {
    /// The tool itself was upgraded.
    UpgradeTool,
    /// The tool's dependencies were upgraded, but the tool itself was unchanged.
    UpgradeDependencies,
    /// The tool's environment was upgraded.
    UpgradeEnvironment,
    /// The tool's executables were repaired or require attention.
    Entrypoints,
    /// The tool was already up-to-date.
    NoOp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UpgradeConstraint {
    /// The tool remains pinned to an exact version, so an upgrade was skipped.
    PinnedVersion { version: Version },
}

impl UpgradeConstraint {
    fn print(&self, name: &PackageName, printer: Printer) -> Result<()> {
        match self {
            Self::PinnedVersion { version } => {
                let name = name.to_string();
                let reinstall_command = format!("uv tool install {name}@latest");

                writeln!(
                    printer.stderr(),
                    "hint: `{}` is pinned to `{}` (installed with an exact version pin); reinstall with `{}` to upgrade to a new version.",
                    name.cyan(),
                    version.to_string().magenta(),
                    reinstall_command.green(),
                )?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UpgradeReport {
    outcome: UpgradeOutcome,
    constraint: Option<UpgradeConstraint>,
}

/// Reconcile the recorded executable paths with the installed packages.
fn reconcile_entrypoints(
    environment: &PythonEnvironment,
    name: &PackageName,
    receipt: &Tool,
    installed_tools: &InstalledTools,
    printer: Printer,
) -> Result<Option<Tool>> {
    let site_packages = SitePackages::from_environment(environment)?;
    let packages = receipt
        .executable_packages()
        .into_iter()
        .filter(|package| package != name)
        .collect::<BTreeSet<_>>();
    let executable_directory = if let Some(parent) = receipt
        .entrypoints()
        .first()
        .and_then(|entry| entry.install_path.parent())
    {
        parent.to_path_buf()
    } else {
        uv_tool::tool_executable_dir()?
    };
    let mut expected = BTreeMap::new();
    for package in packages.iter().chain(std::iter::once(name)) {
        let installed = site_packages.get_packages(package);
        let Some(dist) = installed.first() else {
            anyhow::bail!("Expected package `{package}` to be installed");
        };
        for (entrypoint, source) in entrypoint_paths(&site_packages, dist.name(), dist.version())? {
            let filename = source
                .file_name()
                .map(std::borrow::ToOwned::to_owned)
                .unwrap_or_else(|| entrypoint.clone().into());
            let target = receipt
                .entrypoints()
                .iter()
                .find(|entry| same_filename(entry.install_path.file_name(), Some(&filename)))
                .map_or_else(
                    || executable_directory.join(&filename),
                    |entry| entry.install_path.clone(),
                );
            let entry = ToolEntrypoint::new(&entrypoint, target.clone(), package.to_string());
            expected.insert(target, (entry, source));
        }
    }

    let recorded = receipt
        .entrypoints()
        .iter()
        .map(|entry| entry.install_path.as_path())
        .collect::<BTreeSet<_>>();
    let mut install = Vec::new();
    for (target, (entry, source)) in &expected {
        if !recorded.contains(target.as_path()) || !entrypoint_matches(source, target)? {
            install.push((target, entry, source));
        }
    }
    let obsolete = recorded
        .iter()
        .filter(|path| !expected.contains_key(**path))
        .copied()
        .collect::<Vec<_>>();
    let mut desired = expected
        .values()
        .map(|(entry, _)| entry.clone())
        .collect::<Vec<_>>();
    desired.sort();
    if install.is_empty() && obsolete.is_empty() && receipt.entrypoints() == desired {
        return Ok(None);
    }

    // Check all destinations before changing any of them.
    for (target, _, _) in &install {
        check_entrypoint_ownership(
            name,
            target,
            recorded.contains(target.as_path()),
            installed_tools,
        )?;
    }
    for target in &obsolete {
        match fs_err::symlink_metadata(target) {
            Ok(_) => check_entrypoint_ownership(name, target, true, installed_tools)?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }

    let mut entries = receipt.entrypoints().to_vec();
    let mut updated = receipt.clone().with_executable_packages(packages);
    for (target, entry, source) in install {
        let parent = target.parent().context("Executable path has no parent")?;
        fs_err::create_dir_all(parent)?;
        if recorded.contains(target.as_path()) {
            #[cfg(unix)]
            uv_fs::replace_symlink(source, target).context("Failed to install executable")?;
            #[cfg(windows)]
            if std::env::current_exe().is_ok_and(|itself| {
                std::path::absolute(target).is_ok_and(|target| itself == target)
            }) {
                self_replace::self_replace(source).context("Failed to install entrypoint")?;
            } else {
                uv_fs::copy_atomic_sync(source, target).context("Failed to install entrypoint")?;
            }
        } else {
            #[cfg(unix)]
            fs_err::os::unix::fs::symlink(source, target)
                .context("Failed to install executable")?;
            #[cfg(windows)]
            {
                let temporary = uv_fs::tempfile_in(parent)?;
                fs_err::copy(source, &temporary)?;
                temporary
                    .persist_noclobber(target)
                    .map_err(|error| error.error)?;
            }
        }
        entries.retain(|old| old.install_path != *target);
        entries.push(entry.clone());
        updated = updated.with_entrypoints(entries.iter().cloned());
        installed_tools.add_tool_receipt(name, updated.clone())?;
        let action = if recorded.contains(target.as_path()) {
            "Repaired"
        } else {
            "Installed"
        };
        writeln!(
            printer.stderr(),
            "{action} 1 executable: {}",
            entry.name.bold()
        )?;
    }
    for target in obsolete {
        match fs_err::remove_file(target) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
        let names = entries
            .iter()
            .filter(|entry| entry.install_path == target)
            .map(|entry| entry.name.clone())
            .collect::<BTreeSet<_>>();
        entries.retain(|entry| entry.install_path != target);
        updated = updated.with_entrypoints(entries.iter().cloned());
        installed_tools.add_tool_receipt(name, updated.clone())?;
        if !names.is_empty() {
            let suffix = if names.len() == 1 { "" } else { "s" };
            writeln!(
                printer.stderr(),
                "Removed {} executable{suffix}: {}",
                names.len(),
                names.iter().map(|name| name.bold()).join(", ")
            )?;
        }
    }
    updated = updated.with_entrypoints(desired);
    installed_tools.add_tool_receipt(name, updated.clone())?;
    Ok(Some(updated))
}

/// Compare an exported executable with the one in the tool environment.
#[cfg(unix)]
fn entrypoint_matches(source: &Path, target: &Path) -> Result<bool> {
    fs_err::metadata(source)?;
    match fs_err::symlink_metadata(target) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Ok(uv_fs::is_same_file_allow_missing(source, target) == Some(true))
        }
        Ok(_) => Ok(false),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err.into()),
    }
}

/// Compare a copied executable with the executable in the tool environment.
#[cfg(windows)]
fn entrypoint_matches(source: &Path, target: &Path) -> Result<bool> {
    let metadata = match fs_err::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err.into()),
    };
    if !metadata.is_file() || metadata.len() != fs_err::metadata(source)?.len() {
        return Ok(false);
    }

    let mut source = BufReader::new(fs_err::File::open(source)?);
    let mut target = BufReader::new(fs_err::File::open(target)?);
    let mut source_buffer = [0; 64 * 1024];
    let mut target_buffer = [0; 64 * 1024];
    loop {
        let count = source.read(&mut source_buffer)?;
        if count == 0 {
            return Ok(target.read(&mut target_buffer)? == 0);
        }
        target.read_exact(&mut target_buffer[..count])?;
        if source_buffer[..count] != target_buffer[..count] {
            return Ok(false);
        }
    }
}

/// Refuse to alter an executable claimed by another tool or an unrelated file.
fn check_entrypoint_ownership(
    name: &PackageName,
    target: &Path,
    recorded: bool,
    installed_tools: &InstalledTools,
) -> Result<()> {
    for (other_name, other_receipt) in installed_tools.tools()? {
        if &other_name == name {
            continue;
        }
        let other_receipt = other_receipt.with_context(|| {
            format!("Cannot check executable ownership for tool `{other_name}`")
        })?;
        for other in other_receipt.entrypoints() {
            if same_entrypoint_path(target, &other.install_path) {
                anyhow::bail!(
                    "Cannot repair executable `{}`: it is also recorded by tool `{other_name}`",
                    target.user_display()
                );
            }
        }
    }
    let metadata = match fs_err::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    if !recorded {
        anyhow::bail!(
            "Cannot install executable `{}`: it already exists",
            target.user_display()
        );
    }
    #[cfg(windows)]
    if !metadata.file_type().is_file() {
        anyhow::bail!(
            "Cannot repair executable `{}`: it is not a file",
            target.user_display()
        );
    }
    #[cfg(unix)]
    {
        if !metadata.file_type().is_symlink() {
            anyhow::bail!(
                "Cannot repair executable `{}`: it is not a link to this tool",
                target.user_display()
            );
        }
        let parent = target.parent().context("Executable path has no parent")?;
        let link = parent.join(fs_err::read_link(target)?);
        let link_parent = link.parent().context("Executable link has no parent")?;
        if !fs_err::canonicalize(link_parent)?
            .starts_with(fs_err::canonicalize(installed_tools.tool_dir(name))?)
        {
            anyhow::bail!(
                "Cannot repair executable `{}`: it is not a link to this tool",
                target.user_display()
            );
        }
    }
    Ok(())
}

/// Compare executable filenames using the target platform's case conventions.
fn same_filename(left: Option<&OsStr>, right: Option<&OsStr>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            #[cfg(windows)]
            {
                left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase()
            }
            #[cfg(not(windows))]
            {
                left == right
            }
        }
        (Some(_) | None, None) | (None, Some(_)) => false,
    }
}

/// Check whether two paths name the same directory entry.
fn same_entrypoint_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    if let (Some(left_parent), Some(right_parent)) = (left.parent(), right.parent()) {
        return same_filename(left.file_name(), right.file_name())
            && uv_fs::is_same_file_allow_missing(left_parent, right_parent) != Some(false);
    }
    false
}

/// Upgrade a specific tool.
async fn upgrade_tool(
    name: &PackageName,
    constraints: &[Requirement],
    interpreter: Option<&Interpreter>,
    python_platform: Option<&TargetTriple>,
    printer: Printer,
    installed_tools: &InstalledTools,
    args: &ResolverInstallerOptions,
    client_builder: &BaseClientBuilder<'_>,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    filesystem: &ResolverInstallerOptions,
    installer_metadata: bool,
    concurrency: &Concurrency,
    preview: Preview,
) -> Result<UpgradeReport> {
    let tool_locks = preview.is_enabled(PreviewFeature::ToolInstallLocks);
    // Ensure the tool is installed.
    let mut existing_tool_receipt = match installed_tools.get_tool_receipt(name) {
        Ok(Some(receipt)) => receipt,
        Ok(None) => {
            let install_command = format!("uv tool install {name}");
            return Err(anyhow::anyhow!(
                "`{}` is not installed; run `{}` to install",
                name.cyan(),
                install_command.green()
            ));
        }
        Err(_) => {
            let install_command = format!("uv tool install --force {name}");
            return Err(anyhow::anyhow!(
                "`{}` is missing a valid receipt; run `{}` to reinstall",
                name.cyan(),
                install_command.green()
            ));
        }
    };

    let environment = match installed_tools.get_environment(name, cache) {
        Ok(Some(environment)) => environment,
        Ok(None) => {
            let install_command = format!("uv tool install {name}");
            return Err(anyhow::anyhow!(
                "`{}` is not installed; run `{}` to install",
                name.cyan(),
                install_command.green()
            ));
        }
        Err(_) => {
            let install_command = format!("uv tool install --force {name}");
            return Err(anyhow::anyhow!(
                "`{}` is missing a valid environment; run `{}` to reinstall",
                name.cyan(),
                install_command.green()
            ));
        }
    };

    // Restore credentials from user configuration when the receipt refers to the same index.
    // Receipts intentionally omit credentials, including usernames needed for keyring lookups.
    let mut receipt = ResolverInstallerOptions::from(existing_tool_receipt.options().clone());
    if let (Some(stored), Some(configured)) = (
        receipt.indexes.index_url.as_ref(),
        filesystem.indexes.index_url.as_ref(),
    ) {
        let stored = Index::from(stored.clone());
        let configured = Index::from(configured.clone());

        if stored.raw_url().username().is_empty()
            && stored.raw_url().password().is_none()
            && (!configured.raw_url().username().is_empty()
                || configured.raw_url().password().is_some())
            && CanonicalUrl::new(stored.raw_url().clone())
                == CanonicalUrl::new(configured.raw_url().clone())
        {
            receipt.indexes.index_url = Some(configured.into());
        }
    }

    // Resolve the appropriate settings, preferring: CLI > receipt > user.
    let options = args.clone().combine(receipt.combine(filesystem.clone()));
    let settings = ResolverInstallerSettings::from(options.clone());

    let build_constraints = existing_tool_receipt.build_constraints().to_vec();
    let manifest_constraints = existing_tool_receipt
        .constraints()
        .iter()
        .chain(constraints)
        .cloned()
        .collect::<Vec<_>>();
    let manifest_overrides = existing_tool_receipt.overrides().to_vec();
    let manifest_excludes = existing_tool_receipt.excludes().to_vec();
    let lock_manifest = ToolLock::manifest(
        existing_tool_receipt.requirements(),
        &manifest_constraints,
        &manifest_overrides,
        &manifest_excludes,
        &build_constraints,
        &settings.resolver.dependency_metadata,
    );
    let build_constraints = Constraints::from_specifications(build_constraints);

    // Resolve the requirements.
    let spec = RequirementsSpecification::from_excludes(
        existing_tool_receipt.requirements().to_vec(),
        manifest_constraints,
        manifest_overrides,
        manifest_excludes,
    );
    // Initialize any shared state.
    let state = PlatformState::default();
    // Check if we need to create a new environment — if so, resolve it first, then install the
    // requested tool.
    let requested_interpreter =
        interpreter.filter(|interpreter| !environment.environment().uses(interpreter));
    let tool_dir = installed_tools.tool_dir(name);
    // TODO(zanieb): When updating an existing environment, build it in the cache directory then
    // copy it into the tool directory.
    let (environment, outcome, tool_lock) = if tool_locks {
        let target_interpreter =
            requested_interpreter.unwrap_or_else(|| environment.environment().interpreter());
        let site_packages = SitePackages::from_environment(environment.environment())?;
        let universal_resolution = resolve_environment(
            tool_environment_spec(spec, None, Some(&site_packages)),
            EnvironmentResolution::Universal,
            target_interpreter,
            python_platform,
            SourceTreeEditablePolicy::Tool,
            build_constraints.clone(),
            &settings.resolver,
            client_builder,
            &state,
            Box::new(SummaryResolveLogger),
            concurrency,
            cache,
            workspace_cache,
            printer,
            preview,
        )
        .await?;
        let tool_lock = ToolLock::from_resolution(
            &tool_dir,
            &universal_resolution,
            &lock_manifest,
            &settings.resolver.index_locations,
        )?;
        let resolution = tool_lock.to_resolution(
            Some(name),
            target_interpreter,
            python_platform,
            &settings.resolver.build_options,
        )?;
        let hash_strategy = HashStrategy::from_resolution(&resolution, HashCheckingMode::Verify)?;

        if requested_interpreter.is_some() {
            let environment =
                installed_tools.create_environment(name, target_interpreter.clone())?;
            let environment = sync_environment(
                environment,
                &resolution,
                hash_strategy,
                Modifications::Exact,
                build_constraints,
                (&settings).into(),
                client_builder,
                &state,
                Box::new(DefaultInstallLogger),
                installer_metadata,
                concurrency,
                cache,
                printer,
                preview,
            )
            .await?;
            (
                environment,
                UpgradeOutcome::UpgradeEnvironment,
                Some(tool_lock),
            )
        } else {
            // Otherwise, upgrade the existing environment.
            let ResolverInstallerSettings {
                resolver:
                    crate::settings::ResolverSettings {
                        config_setting,
                        config_settings_package,
                        extra_build_dependencies,
                        extra_build_variables,
                        ..
                    },
                ..
            } = &settings;
            let extra_build_requires =
                LoweredExtraBuildDependencies::from_non_lowered(extra_build_dependencies.clone())
                    .into_inner();
            let tags = resolution_tags(
                None,
                python_platform,
                environment.environment().interpreter(),
            )?;
            let plan = Planner::new(&resolution).build(
                site_packages,
                InstallationStrategy::Permissive,
                &settings.reinstall,
                &settings.resolver.build_options,
                &hash_strategy,
                &settings.resolver.index_locations,
                config_setting,
                config_settings_package,
                &extra_build_requires,
                extra_build_variables,
                cache,
                environment.environment(),
                &tags,
            )?;
            let plan_is_empty = plan.is_empty();
            let changes_tool = plan.cached.iter().any(|dist| dist.name() == name)
                || plan.remote.iter().any(|dist| dist.name() == name)
                || plan.reinstalls.iter().any(|dist| dist.name() == name)
                || plan.extraneous.iter().any(|dist| dist.name() == name);
            let outcome = if plan_is_empty {
                UpgradeOutcome::NoOp
            } else if changes_tool {
                UpgradeOutcome::UpgradeTool
            } else {
                UpgradeOutcome::UpgradeDependencies
            };
            let environment = if plan_is_empty && !settings.compile_bytecode {
                environment.into_environment()
            } else {
                sync_environment(
                    environment.into_environment(),
                    &resolution,
                    hash_strategy,
                    Modifications::Exact,
                    build_constraints,
                    (&settings).into(),
                    client_builder,
                    &state,
                    Box::new(UpgradeInstallLogger::new(name.clone())),
                    installer_metadata,
                    concurrency,
                    cache,
                    printer,
                    preview,
                )
                .await?
            };
            (environment, outcome, Some(tool_lock))
        }
    } else if let Some(interpreter) = requested_interpreter {
        let resolution = resolve_environment(
            spec.into(),
            EnvironmentResolution::Specific,
            interpreter,
            python_platform,
            SourceTreeEditablePolicy::Tool,
            build_constraints.clone(),
            &settings.resolver,
            client_builder,
            &state,
            Box::new(SummaryResolveLogger),
            concurrency,
            cache,
            workspace_cache,
            printer,
            preview,
        )
        .await?;
        let environment = installed_tools.create_environment(name, interpreter.clone())?;
        let environment = sync_environment(
            environment,
            &resolution.into(),
            HashStrategy::default(),
            Modifications::Exact,
            build_constraints,
            (&settings).into(),
            client_builder,
            &state,
            Box::new(DefaultInstallLogger),
            installer_metadata,
            concurrency,
            cache,
            printer,
            preview,
        )
        .await?;
        (environment, UpgradeOutcome::UpgradeEnvironment, None)
    } else {
        // Otherwise, upgrade the existing environment.
        let EnvironmentUpdate {
            environment,
            changelog,
        } = update_environment(
            environment.into_environment(),
            spec,
            Modifications::Exact,
            python_platform,
            SourceTreeEditablePolicy::Tool,
            build_constraints,
            ExtraBuildRequires::default(),
            &settings,
            client_builder,
            &state,
            Box::new(SummaryResolveLogger),
            Box::new(UpgradeInstallLogger::new(name.clone())),
            installer_metadata,
            concurrency,
            cache,
            workspace_cache,
            DryRun::Disabled,
            printer,
            preview,
        )
        .await?;

        let outcome = if changelog.includes(name) {
            UpgradeOutcome::UpgradeTool
        } else if changelog.is_empty() {
            UpgradeOutcome::NoOp
        } else {
            UpgradeOutcome::UpgradeDependencies
        };

        (environment, outcome, None)
    };

    let outcome = match outcome {
        UpgradeOutcome::NoOp | UpgradeOutcome::UpgradeDependencies => {
            if let Some(updated) = reconcile_entrypoints(
                &environment,
                name,
                &existing_tool_receipt,
                installed_tools,
                printer,
            )? {
                existing_tool_receipt = updated;
                if outcome == UpgradeOutcome::NoOp {
                    UpgradeOutcome::Entrypoints
                } else {
                    outcome
                }
            } else {
                outcome
            }
        }
        UpgradeOutcome::UpgradeEnvironment
        | UpgradeOutcome::UpgradeTool
        | UpgradeOutcome::Entrypoints => outcome,
    };

    if matches!(
        outcome,
        UpgradeOutcome::UpgradeEnvironment | UpgradeOutcome::UpgradeTool
    ) {
        // At this point, we updated the existing environment, so we should remove any of its
        // existing executables.
        remove_entrypoints(&existing_tool_receipt);

        let entrypoints: Vec<_> = existing_tool_receipt
            .executable_packages()
            .into_iter()
            .filter(|package| package != name)
            .collect();

        // If we modified the target tool, reinstall the entrypoints.
        finalize_tool_install(
            &environment,
            name,
            &entrypoints,
            installed_tools,
            &ToolOptions::from(options),
            true,
            existing_tool_receipt.python().to_owned(),
            existing_tool_receipt.requirements().to_vec(),
            existing_tool_receipt.constraints().to_vec(),
            existing_tool_receipt.overrides().to_vec(),
            existing_tool_receipt.excludes().to_vec(),
            existing_tool_receipt.build_constraints().to_vec(),
            tool_lock.as_ref(),
            Some(&existing_tool_receipt),
            printer,
        )?;
    } else if tool_locks {
        ToolLock::write(&tool_dir, tool_lock.as_ref())?;
        installed_tools.add_tool_receipt(
            name,
            existing_tool_receipt
                .clone()
                .with_options(ToolOptions::from(options)),
        )?;
    }

    let constraint = match &outcome {
        UpgradeOutcome::UpgradeDependencies | UpgradeOutcome::NoOp => {
            pinned_requirement_version(&existing_tool_receipt, name)
                .map(|version| UpgradeConstraint::PinnedVersion { version })
        }
        UpgradeOutcome::Entrypoints => pinned_requirement_version(&existing_tool_receipt, name)
            .map(|version| UpgradeConstraint::PinnedVersion { version }),
        UpgradeOutcome::UpgradeTool | UpgradeOutcome::UpgradeEnvironment => None,
    };

    Ok(UpgradeReport {
        outcome,
        constraint,
    })
}

fn pinned_requirement_version(tool: &Tool, name: &PackageName) -> Option<Version> {
    pinned_version_from(tool.requirements(), name)
        .or_else(|| pinned_version_from(tool.constraints(), name))
}

fn pinned_version_from(requirements: &[Requirement], name: &PackageName) -> Option<Version> {
    requirements
        .iter()
        .filter(|requirement| requirement.name == *name)
        .find_map(|requirement| match &requirement.source {
            RequirementSource::Registry { specifier, .. } => {
                specifier
                    .iter()
                    .find_map(|specifier| match specifier.operator() {
                        Operator::Equal | Operator::ExactEqual => Some(specifier.version().clone()),
                        _ => None,
                    })
            }
            _ => None,
        })
}
