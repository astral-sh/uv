use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use url::Url;
use uv_configuration::{
    DependencyGroups, DependencyGroupsWithDefaults, ExtrasSpecification, InstallOptions,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_distribution_filename::SourceDistFilename;
use uv_distribution_types::{
    BuildableSource, DirectorySourceUrl, MetadataHashPolicy, PYPI_URL, Requirement,
    RequirementSource, RequiresPython, SourceUrl,
};
use uv_fs::is_same_file_allow_missing;
use uv_lock::{Installable, Lock, Package, PylockToml};
use uv_normalize::{DefaultExtras, DefaultGroups, ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers, release_specifiers_to_ranges};
use uv_preview::{Preview, PreviewFeature};
use uv_pypi_types::{HashDigest, ResolutionMetadata};
use uv_static::{EnvVars, parse_boolish_environment_variable};
use uv_workspace::Workspace;
use uv_workspace::dependency_groups::FlatDependencyGroups;
use version_ranges::Ranges;

use crate::settings::BuildLockSettings;

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Pyproject {
    project: Option<Project>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Project {
    name: PackageName,
    version: Option<Version>,
    requires_python: Option<VersionSpecifiers>,
    #[serde(default)]
    dynamic: Vec<String>,
}

pub(super) struct ExportedLock {
    pub(super) pylock: String,
    metadata: LockMetadata,
}

enum LockMetadata {
    Project {
        root: PathBuf,
        lock: Box<Lock>,
        requires_dist: Vec<Requirement>,
        project_metadata: ResolutionMetadata,
    },
    Sdist {
        root: PathBuf,
        requires_python: RequiresPython,
        metadata: ResolutionMetadata,
    },
}

impl ExportedLock {
    pub(super) fn project_metadata(&self) -> &ResolutionMetadata {
        match &self.metadata {
            LockMetadata::Project {
                project_metadata, ..
            } => project_metadata,
            LockMetadata::Sdist { metadata, .. } => metadata,
        }
    }

    pub(super) fn matches_metadata(&self, metadata: ResolutionMetadata) -> Result<bool> {
        match &self.metadata {
            LockMetadata::Project {
                root,
                lock,
                requires_dist,
                project_metadata,
            } => {
                if !metadata_fields_match(project_metadata, &metadata) {
                    return Ok(false);
                }
                let python = metadata
                    .requires_python
                    .map(release_specifiers_to_ranges)
                    .unwrap_or_else(Ranges::full);
                let locked_python =
                    release_specifiers_to_ranges(lock.requires_python().specifiers().clone());
                if !python.subset_of(&locked_python) {
                    return Ok(false);
                }
                let actual_requirements = metadata
                    .requires_dist
                    .into_vec()
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>();
                Ok(lock.matches_requirements(root, requires_dist, &actual_requirements)?)
            }
            LockMetadata::Sdist {
                root,
                requires_python,
                metadata: expected,
            } => metadata_matches(expected, &metadata, root, requires_python),
        }
    }
}

fn metadata_fields_match(expected: &ResolutionMetadata, actual: &ResolutionMetadata) -> bool {
    let expected_python = expected
        .requires_python
        .clone()
        .map(release_specifiers_to_ranges)
        .unwrap_or_else(Ranges::full);
    let actual_python = actual
        .requires_python
        .clone()
        .map(release_specifiers_to_ranges)
        .unwrap_or_else(Ranges::full);
    let expected_extras: BTreeSet<_> = expected.provides_extra.iter().collect();
    let actual_extras: BTreeSet<_> = actual.provides_extra.iter().collect();
    expected.name == actual.name
        && expected.version == actual.version
        && expected_python == actual_python
        && expected_extras == actual_extras
}

fn metadata_matches(
    expected: &ResolutionMetadata,
    actual: &ResolutionMetadata,
    root: &Path,
    requires_python: &RequiresPython,
) -> Result<bool> {
    let expected_requirements: Vec<Requirement> = expected
        .requires_dist
        .iter()
        .cloned()
        .map(Into::into)
        .collect();
    let actual_requirements: Vec<Requirement> = actual
        .requires_dist
        .iter()
        .cloned()
        .map(Into::into)
        .collect();
    Ok(metadata_fields_match(expected, actual)
        && Lock::matches_requirements_for_python(
            root,
            requires_python,
            &expected_requirements,
            &actual_requirements,
        )?)
}

/// Read a lock from an extracted source distribution, checking its metadata before reuse.
pub(super) fn from_sdist(
    source_tree: &Path,
    filename: Option<&SourceDistFilename>,
    preview: Preview,
) -> Result<Option<ExportedLock>> {
    let environment_export = parse_boolish_environment_variable(EnvVars::UV_EXPORT_LOCK)?;
    let pyproject = match fs_err::read_to_string(source_tree.join("pyproject.toml")) {
        Ok(pyproject) => pyproject,
        Err(error)
            if error.kind() == io::ErrorKind::NotFound && environment_export != Some(true) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    let raw: toml::Value = toml::from_str(&pyproject)?;
    let enabled = environment_export.or(raw
        .get("tool")
        .and_then(|tool| tool.get("uv"))
        .and_then(|uv| uv.get("export-lock"))
        .map(|value| {
            value
                .as_bool()
                .context("The project setting `tool.uv.export-lock` must be a boolean")
        })
        .transpose()?);
    let uv_build = raw
        .get("build-system")
        .and_then(|system| system.get("build-backend"))
        .and_then(toml::Value::as_str)
        == Some("uv_build");
    if enabled == Some(false) || (enabled.is_none() && !uv_build) {
        return Ok(None);
    }
    if !preview.is_enabled(PreviewFeature::LockedTools) {
        if enabled == Some(true) {
            bail!("Exporting locks requires the `locked-tools` preview feature");
        }
        return Ok(None);
    }
    let path = source_tree.join("pylock.toml");
    let file_metadata = match fs_err::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            bail!("Cannot export a lock: the source distribution has no `pylock.toml`")
        }
        Err(error) => return Err(error.into()),
    };
    if !file_metadata.is_file() || file_metadata.len() > 16 * 1024 * 1024 {
        bail!("The source distribution has an invalid `pylock.toml` file");
    }
    let mut contents = fs_err::read_to_string(&path)?;
    let raw: toml::Value = toml::from_str(&contents)?;
    let lock: PylockToml = toml::from_str(&contents)?;
    if enabled.is_none() && (!has_only_pypi_sources(&lock) || !has_no_unchecked_urls(&raw)) {
        return Ok(None);
    }
    if lock.has_missing_hashes() || lock.has_non_registry_sources() {
        bail!("The source distribution has an unsupported `pylock.toml`");
    }
    if enabled.is_none() && contents.contains('#') {
        contents = toml::to_string(&raw)?;
        if toml::from_str::<toml::Value>(&contents)? != raw {
            bail!("Cannot sanitize the source distribution's `pylock.toml`");
        }
    }
    lock.validate_registry_packages()
        .context("The source distribution has an invalid `pylock.toml`")?;
    let pkg_info = source_tree.join("PKG-INFO");
    let info_metadata = fs_err::symlink_metadata(&pkg_info)?;
    if !info_metadata.is_file() || info_metadata.len() > 16 * 1024 * 1024 {
        bail!("The source distribution has an invalid `PKG-INFO` file");
    }
    let metadata = ResolutionMetadata::parse_pkg_info(&fs_err::read(&pkg_info)?)?;
    if metadata.dynamic {
        bail!("The source distribution has a dynamic version in `PKG-INFO`");
    }
    let requires_python = lock
        .requires_python
        .clone()
        .unwrap_or_else(|| RequiresPython::from_specifiers(VersionSpecifiers::default()));
    let filename = filename.context("Cannot verify the source distribution filename")?;
    if metadata.name != filename.name || metadata.version != filename.version {
        bail!("The source distribution's metadata does not match its filename");
    }
    let project_python = metadata
        .requires_python
        .clone()
        .map(release_specifiers_to_ranges)
        .unwrap_or_else(Ranges::full);
    let lock_python = release_specifiers_to_ranges(requires_python.specifiers().clone());
    if !project_python.subset_of(&lock_python) {
        bail!("The source distribution's lock does not cover its supported Python versions");
    }
    Ok(Some(ExportedLock {
        pylock: contents,
        metadata: LockMetadata::Sdist {
            root: source_tree.to_path_buf(),
            requires_python,
            metadata,
        },
    }))
}

/// Return whether the URL is the PyPI simple index, with or without a trailing slash.
fn is_pypi_index_url(url: &str) -> bool {
    url.strip_suffix('/').unwrap_or(url) == PYPI_URL.as_str()
}

/// Check the package indexes recorded in the source distribution's lock.
fn has_only_pypi_sources(lock: &PylockToml) -> bool {
    lock.packages.iter().all(|package| {
        package
            .index
            .as_ref()
            .is_none_or(|index| is_pypi_index_url(index.as_str()))
            && !package.has_non_registry_source()
    })
}

/// Export the runtime dependencies of the project supplied by the distribution.
pub(super) async fn export(
    source_tree: &Path,
    workspace: Option<&Workspace>,
    database: &DistributionDatabase<'_, BuildDispatch<'_>>,
    lock_settings: &BuildLockSettings,
    preview: Preview,
) -> Result<Option<ExportedLock>> {
    let environment_export = parse_boolish_environment_variable(EnvVars::UV_EXPORT_LOCK)?;
    let pyproject_path = source_tree.join("pyproject.toml");
    let pyproject_contents = match fs_err::read_to_string(&pyproject_path) {
        Ok(contents) => contents,
        Err(error)
            if error.kind() == io::ErrorKind::NotFound && environment_export == Some(true) =>
        {
            if !preview.is_enabled(PreviewFeature::LockedTools) {
                bail!("Exporting locks requires the `locked-tools` preview feature");
            }
            bail!("Cannot export a lock without project metadata");
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let raw: toml::Value = toml::from_str(&pyproject_contents)
        .with_context(|| format!("Failed to parse {}", pyproject_path.display()))?;
    let uv_build = raw
        .get("build-system")
        .and_then(|system| system.get("build-backend"))
        .and_then(toml::Value::as_str)
        == Some("uv_build");
    let configured = raw
        .get("tool")
        .and_then(|tool| tool.get("uv"))
        .and_then(|uv| uv.get("export-lock"))
        .map(|value| {
            value
                .as_bool()
                .context("The project setting `tool.uv.export-lock` must be a boolean")
        })
        .transpose()?;
    let enabled = environment_export.or(configured);
    if enabled == Some(false) || (enabled.is_none() && !uv_build) {
        return Ok(None);
    }
    if !preview.is_enabled(PreviewFeature::LockedTools) {
        if enabled == Some(true) {
            bail!("Exporting locks requires the `locked-tools` preview feature");
        }
        return Ok(None);
    }
    let pyproject: Pyproject = raw.clone().try_into()?;
    let Some(project) = pyproject.project else {
        bail!("Cannot export a lock without project metadata");
    };
    let root = workspace
        .filter(|workspace| {
            workspace.packages().values().any(|package| {
                is_same_file_allow_missing(package.root(), source_tree).unwrap_or(false)
            })
        })
        .map_or(source_tree, |workspace| workspace.install_path().as_path());
    let path = root.join("uv.lock");
    let contents = match fs_err::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if enabled.is_none()
                && has_ineligible_runtime_source(
                    source_tree,
                    &pyproject_path,
                    &pyproject_contents,
                    &project,
                    workspace,
                    database,
                    lock_settings,
                )
                .await
            {
                return Ok(None);
            }
            bail!("Cannot export a lock: `uv.lock` was not found; run `uv lock` before building")
        }
        Err(error) => return Err(error.into()),
    };
    let lock = Lock::from_toml(&contents).context("Failed to parse `uv.lock`")?;
    let package = match lock.find_by_name(&project.name) {
        Ok(Some(package)) => package,
        Ok(None) => bail!("`uv.lock` does not contain `{}`", project.name),
        Err(error) => return Err(anyhow::Error::msg(error)),
    };
    let target = BuildLock {
        root,
        lock: &lock,
        name: &project.name,
    };
    let install_options =
        InstallOptions::new(true, false, false, false, false, false, vec![], vec![]);
    let extras = ExtrasSpecification::from_all_extras().with_defaults(DefaultExtras::default());
    let groups = DependencyGroupsWithDefaults::none();
    let packages =
        PylockToml::packages_for_source_check(&target, &extras, &groups, &install_options)?;
    let all_groups = DependencyGroups::from_all_groups().with_defaults(DefaultGroups::default());
    let all_packages =
        PylockToml::packages_for_source_check(&target, &extras, &all_groups, &install_options)?;
    let workspace_dependency = PylockToml::workspace_dependency(&target, &all_packages);
    if enabled.is_none()
        && (workspace_dependency.is_some()
            || !lock.has_only_pypi_and_workspace_sources(&all_packages))
    {
        check_skipped_lock(
            &lock,
            package,
            &project,
            &raw,
            &pyproject_contents,
            &pyproject_path,
            source_tree,
            root,
            workspace,
            database,
            lock_settings,
        )
        .await?;
        return Ok(None);
    }
    if let Some(dependency) = workspace_dependency {
        bail!("Cannot export a lock with workspace dependency `{dependency}`");
    }
    if enabled.is_none() && Lock::has_pypi_artifacts_without_urls(&packages) {
        bail!(
            "Cannot export a lock with registry artifacts without URLs; run `uv lock` before building"
        );
    }
    let project_metadata =
        uv_pypi_types::PyProjectToml::from_toml(&pyproject_contents, pyproject_path.display())?;
    let unlowered = uv_pypi_types::RequiresDist::from_pyproject_toml(project_metadata.clone());
    let requirements = match database.requires_dist(source_tree, &project_metadata).await {
        Ok(requirements) => requirements
            .map(|requirements| (requirements.requires_dist, requirements.provides_extra)),
        Err(error) if source_tree.join("PKG-INFO").is_file() => {
            // A source distribution need not contain the other members of the original workspace.
            // Compare its static declarations directly when workspace discovery cannot lower them.
            tracing::debug!("Could not lower source distribution requirements: {error}");
            let requirements = uv_pypi_types::RequiresDist::from_pyproject_toml(project_metadata)?;
            Some((
                requirements
                    .requires_dist
                    .into_vec()
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                requirements.provides_extra,
            ))
        }
        Err(error) => return Err(error.into()),
    };
    let metadata = if project.version.is_none()
        || requirements.is_none()
        || project
            .dynamic
            .iter()
            .any(|field| field == "requires-python")
    {
        let url = url::Url::from_directory_path(source_tree)
            .map(uv_redacted::DisplaySafeUrl::from_url)
            .map_err(|()| anyhow::anyhow!("Failed to convert project path to a URL"))?;
        let source = BuildableSource::Url(SourceUrl::Directory(DirectorySourceUrl {
            url: &url,
            install_path: source_tree,
            editable: None,
        }));
        Some(
            database
                .build_wheel_metadata(&source, MetadataHashPolicy::default())
                .await?
                .metadata,
        )
    } else {
        None
    };
    if metadata
        .as_ref()
        .is_some_and(|metadata| metadata.name != project.name)
    {
        bail!(
            "The build backend reported a different project name for `{}`",
            project.name
        );
    }
    let version = project
        .version
        .as_ref()
        .or_else(|| metadata.as_ref().map(|metadata| &metadata.version));
    if (project.version.is_some() || package.version().is_some()) && package.version() != version {
        bail!(
            "`uv.lock` does not match the version of `{}`; run `uv lock` before building",
            project.name
        );
    }
    let requirements = if let Some(requirements) = requirements {
        requirements
    } else if let Some(metadata) = metadata.as_ref() {
        let requirements =
            uv_distribution::RequiresDist::from(metadata.clone().with_force_relative(false));
        (requirements.requires_dist, requirements.provides_extra)
    } else {
        bail!("Cannot verify the project's dependency declarations against `uv.lock`");
    };
    if !lock.matches_package_requirements(root, package, &requirements.0, &requirements.1)? {
        bail!(
            "`uv.lock` does not match the dependencies of `{}`; run `uv lock` before building",
            project.name
        );
    }
    let requires_python = project.requires_python.clone().or_else(|| {
        metadata
            .as_ref()
            .and_then(|metadata| metadata.requires_python.clone())
    });
    let project_python = requires_python
        .clone()
        .map(release_specifiers_to_ranges)
        .unwrap_or_else(Ranges::full);
    let locked_python = release_specifiers_to_ranges(lock.requires_python().specifiers().clone());
    if !project_python.subset_of(&locked_python) {
        bail!(
            "Cannot export a lock that does not cover the Python versions supported by `{}`",
            project.name
        );
    }
    if !lock.supported_environments().is_empty() {
        bail!("Cannot export a lock restricted to specific environments");
    }
    let selected_extras = ExtrasSpecification::default().with_defaults(DefaultExtras::default());
    let pylock = PylockToml::from_lock(
        &target,
        root,
        &[],
        &selected_extras,
        &groups,
        false,
        None,
        &install_options,
    )?;
    if pylock.has_missing_hashes() {
        bail!(
            "Cannot export a lock with missing artifact hashes; regenerate `uv.lock` with artifact hashes before building"
        );
    }
    if pylock.has_non_registry_sources() {
        bail!("Cannot export a lock without registry distribution artifact URLs");
    }
    pylock
        .validate_registry_packages()
        .context("Cannot export an invalid lock")?;
    let (requires_dist, provides_extra): (Vec<Requirement>, Vec<ExtraName>) = match unlowered {
        Ok(requirements) => (
            requirements
                .requires_dist
                .into_vec()
                .into_iter()
                .map(Into::into)
                .collect(),
            requirements.provides_extra.into_vec(),
        ),
        Err(error) => {
            let metadata = metadata
                .as_ref()
                .with_context(|| format!("Cannot read dynamic project metadata: {error}"))?;
            (
                metadata.requires_dist.to_vec(),
                metadata.provides_extra.to_vec(),
            )
        }
    };
    let pylock = pylock.to_toml()?;
    let project_metadata = ResolutionMetadata {
        name: project.name.clone(),
        version: version
            .cloned()
            .context("Cannot export a lock without a project version")?,
        requires_python,
        requires_dist: requires_dist.iter().cloned().map(Into::into).collect(),
        provides_extra: provides_extra.clone().into_boxed_slice(),
        dynamic: false,
    };
    if enabled.is_none() && !has_no_unchecked_urls(&toml::from_str(&pylock)?) {
        tracing::debug!("Not exporting a lock because it may contain unchecked URL data");
        return Ok(None);
    }
    Ok(Some(ExportedLock {
        pylock,
        metadata: LockMetadata::Project {
            root: root.to_path_buf(),
            requires_dist,
            lock: Box::new(lock),
            project_metadata,
        },
    }))
}

/// Check whether a mandatory runtime dependency has a source that excludes automatic export.
async fn has_ineligible_runtime_source(
    source_tree: &Path,
    pyproject_path: &Path,
    pyproject_contents: &str,
    project: &Project,
    workspace: Option<&Workspace>,
    database: &DistributionDatabase<'_, BuildDispatch<'_>>,
    lock_settings: &BuildLockSettings,
) -> bool {
    let Some(workspace) = workspace.filter(|workspace| {
        workspace
            .packages()
            .get(&project.name)
            .is_some_and(|member| {
                is_same_file_allow_missing(member.root(), source_tree).unwrap_or(false)
            })
    }) else {
        return false;
    };
    if !workspace.constraints().is_empty()
        || !workspace.overrides().is_empty()
        || !workspace.exclude_dependencies().is_empty()
        || !lock_settings
            .has_dependency_modifiers(workspace.install_path())
            .is_ok_and(|has_modifiers| !has_modifiers)
    {
        return false;
    }
    let Ok(settings) = lock_settings.resolve(workspace.install_path()) else {
        return false;
    };
    let Ok(pyproject) =
        uv_pypi_types::PyProjectToml::from_toml(pyproject_contents, pyproject_path.display())
    else {
        return false;
    };
    let Ok(Some(requirements)) = database.requires_dist(source_tree, &pyproject).await else {
        return false;
    };
    requirements.requires_dist.iter().any(|requirement| {
        if requirement.marker.is_false()
            || requirement.name == project.name
            || settings.sources.for_package(&requirement.name)
        {
            return false;
        }
        match &requirement.source {
            RequirementSource::Registry {
                index: Some(index),
                conflict: None,
                ..
            } => !is_pypi_index_url(index.url.without_credentials().as_str()),
            RequirementSource::Registry { .. } => false,
            RequirementSource::Url { .. }
            | RequirementSource::GitDirectory { .. }
            | RequirementSource::GitPath { .. }
            | RequirementSource::Path { .. }
            | RequirementSource::Directory { .. } => true,
        }
    })
}

/// Report a stale skipped lock when static project inputs establish a mismatch.
async fn check_skipped_lock(
    lock: &Lock,
    package: &Package,
    project: &Project,
    raw: &toml::Value,
    pyproject_contents: &str,
    pyproject_path: &Path,
    source_tree: &Path,
    root: &Path,
    workspace: Option<&Workspace>,
    database: &DistributionDatabase<'_, BuildDispatch<'_>>,
    lock_settings: &BuildLockSettings,
) -> Result<()> {
    if let (Some(expected), Some(actual)) = (&project.version, package.version())
        && expected != actual
    {
        bail!(
            "`uv.lock` does not match the version of `{}`; run `uv lock` before building",
            project.name
        );
    }
    let Ok(pyproject) =
        uv_pypi_types::PyProjectToml::from_toml(pyproject_contents, pyproject_path.display())
    else {
        return Ok(());
    };
    let Ok(Some(requirements)) = database.requires_dist(source_tree, &pyproject).await else {
        return Ok(());
    };
    if !package.has_metadata() {
        // Scoped overrides can add dependencies even when the project declares none.
        if requirements.requires_dist.is_empty()
            && workspace.is_some_and(|workspace| workspace.overrides().is_empty())
            && package.has_unconditional_dependencies()
        {
            bail!(
                "`uv.lock` does not match the dependencies of `{}`; run `uv lock` before building",
                project.name
            );
        }
        return Ok(());
    }
    match lock.matches_package_requirements(
        root,
        package,
        &requirements.requires_dist,
        &requirements.provides_extra,
    ) {
        Ok(false) => bail!(
            "`uv.lock` does not match the dependencies of `{}`; run `uv lock` before building",
            project.name
        ),
        Ok(true) => {}
        Err(_) => return Ok(()),
    }

    // An explicit index configuration rules out a locked direct registry dependency only when
    // no source tree or dependency group could introduce another index.
    let Some(workspace) = workspace else {
        return Ok(());
    };
    if !workspace
        .workspace_dependency_groups()
        .is_ok_and(|groups| groups.is_empty())
        || !workspace.sources().is_empty()
        || !workspace.constraints().is_empty()
        || !workspace.overrides().is_empty()
        || raw.get("dependency-groups").is_some()
        || !package.dependency_groups().is_empty()
        || requirements.requires_dist.iter().any(|requirement| {
            matches!(
                requirement.source,
                RequirementSource::Registry { index: Some(_), .. }
            )
        })
    {
        return Ok(());
    }
    let Ok(settings) = lock_settings.resolve(root) else {
        return Ok(());
    };
    if settings.dependency_metadata.values().next().is_some() {
        return Ok(());
    }

    // Every workspace member can contribute sources to a workspace resolution. Require static
    // registry requirements and exclude members with their own sources or dependency groups.
    if workspace.packages().len() > 1
        && (lock.members().len() != workspace.packages().len()
            || workspace
                .packages()
                .keys()
                .any(|name| !lock.members().contains(name)))
    {
        return Ok(());
    }
    for (name, member) in workspace.packages() {
        let member_pyproject = member.pyproject_toml();
        if !FlatDependencyGroups::from_pyproject_toml(member.root(), member_pyproject)
            .is_ok_and(|groups| groups.into_iter().next().is_none())
            || member_pyproject
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.sources.as_ref())
                .is_some_and(|sources| !sources.inner().is_empty())
        {
            return Ok(());
        }
        if name == &project.name {
            continue;
        }
        let Ok(Some(member_package)) = lock.find_by_name(name) else {
            return Ok(());
        };
        if !member_package.source_tree().is_some_and(|path| {
            is_same_file_allow_missing(member.root(), &root.join(path)).unwrap_or(false)
        }) {
            return Ok(());
        }
        let Ok(pyproject) = uv_pypi_types::PyProjectToml::from_toml(
            &member_pyproject.raw,
            member.root().join("pyproject.toml").display(),
        ) else {
            return Ok(());
        };
        let Ok(Some(member_requirements)) = database.requires_dist(member.root(), &pyproject).await
        else {
            return Ok(());
        };
        if member_requirements.requires_dist.iter().any(|requirement| {
            !matches!(
                requirement.source,
                RequirementSource::Registry { index: None, .. }
            )
        }) {
            return Ok(());
        }
    }
    if lock.packages().iter().any(|dependency| {
        dependency.name() != &project.name
            && !matches!(dependency.index(root), Ok(Some(_)))
            && !workspace
                .packages()
                .get(dependency.name())
                .is_some_and(|member| {
                    dependency.source_tree().is_some_and(|path| {
                        is_same_file_allow_missing(member.root(), &root.join(path)).unwrap_or(false)
                    })
                })
    }) {
        return Ok(());
    }
    let indexes = &settings.index_locations;
    if indexes.is_none() || indexes.no_index() {
        return Ok(());
    }
    for requirement in &requirements.requires_dist {
        if !requirement.marker.is_true()
            || !matches!(
                requirement.source,
                RequirementSource::Registry { index: None, .. }
            )
            || !package
                .dependencies()
                .iter()
                .any(|dependency| dependency.package_name() == &requirement.name)
        {
            continue;
        }
        let Ok(Some(dependency)) = lock.find_by_name(&requirement.name) else {
            continue;
        };
        let Ok(Some(index)) = dependency.index(root) else {
            continue;
        };
        if !indexes.allowed_indexes().iter().any(|allowed| {
            allowed.url().without_credentials().as_ref() == index.without_credentials().as_ref()
        }) {
            bail!(
                "`uv.lock` references an index that is no longer configured; run `uv lock` before building"
            );
        }
    }
    Ok(())
}

/// Reject URL-like values outside package indexes and distribution artifact URLs.
fn has_no_unchecked_urls(value: &toml::Value) -> bool {
    fn is_safe(value: &str) -> bool {
        if !value.contains(':') && !value.contains("//") {
            return true;
        }
        // Hashes and timestamps also contain colons, but cannot contain URLs.
        if value.parse::<HashDigest>().is_ok() || value.parse::<toml::value::Datetime>().is_ok() {
            return true;
        }
        false
    }

    fn contains_only_safe_values(value: &toml::Value) -> bool {
        match value {
            toml::Value::String(value) => is_safe(value),
            toml::Value::Array(values) => values.iter().all(contains_only_safe_values),
            toml::Value::Table(values) => values
                .iter()
                .all(|(key, value)| is_safe(key) && contains_only_safe_values(value)),
            toml::Value::Integer(_)
            | toml::Value::Float(_)
            | toml::Value::Boolean(_)
            | toml::Value::Datetime(_) => true,
        }
    }

    fn take_artifact_url(artifact: &mut toml::Value) -> bool {
        let Some(url) = artifact
            .as_table_mut()
            .and_then(|table| table.remove("url"))
        else {
            return true;
        };
        url.as_str().is_some_and(|url| {
            Url::parse(url).is_ok_and(|url| {
                let path = url.path().to_ascii_lowercase();
                url.scheme() == "https"
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none()
                    && !path.contains(':')
                    && !path.contains("//")
                    && !path.contains("%3a")
                    && !path.contains("%2f")
            })
        })
    }

    let mut remaining = value.clone();
    if let Some(packages) = remaining
        .get_mut("packages")
        .and_then(toml::Value::as_array_mut)
    {
        for package in packages {
            let Some(package) = package.as_table_mut() else {
                return false;
            };
            // The index is checked against the package source before this function is called.
            package.remove("index");
            if let Some(sdist) = package.get_mut("sdist")
                && !take_artifact_url(sdist)
            {
                return false;
            }
            if let Some(wheels) = package
                .get_mut("wheels")
                .and_then(toml::Value::as_array_mut)
                && !wheels.iter_mut().all(take_artifact_url)
            {
                return false;
            }
        }
    }
    contains_only_safe_values(&remaining)
}

struct BuildLock<'lock> {
    root: &'lock Path,
    lock: &'lock Lock,
    name: &'lock PackageName,
}

impl<'lock> Installable<'lock> for BuildLock<'lock> {
    fn install_path(&self) -> &'lock Path {
        self.root
    }
    fn lock(&self) -> &'lock Lock {
        self.lock
    }
    fn roots(&self) -> impl Iterator<Item = &PackageName> {
        std::iter::once(self.name)
    }
    fn project_name(&self) -> Option<&PackageName> {
        Some(self.name)
    }
}
