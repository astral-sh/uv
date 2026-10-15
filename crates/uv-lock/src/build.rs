use std::collections::BTreeSet;
use std::fmt::Display;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use uv_configuration::{
    DependencyGroups, DependencyGroupsWithDefaults, ExtrasSpecification, InstallOptions,
};
use uv_distribution::DistributionDatabase;
use uv_distribution_types::{
    BuildableSource, DirectorySourceUrl, MetadataHashPolicy, Requirement, RequiresPython, SourceUrl,
};
use uv_fs::is_same_file_allow_missing;
use uv_normalize::{DefaultExtras, DefaultGroups, PackageName};
use uv_pep440::{Version, VersionSpecifiers, release_specifiers_to_ranges};
use uv_preview::{Preview, PreviewFeature};
use uv_pypi_types::{HashDigest, ResolutionMetadata};
use uv_redacted::SafeUrl;
use uv_static::{EnvVars, parse_boolish_environment_variable};
use uv_types::BuildContext;
use uv_workspace::Workspace;
use version_ranges::Ranges;

use crate::{Installable, Lock, PylockToml};

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExportMode {
    Automatic,
    Required,
}

impl ExportMode {
    fn skip_on_error<T, E: Display>(self, result: Result<Option<T>, E>) -> Result<Option<T>, E> {
        match result {
            Err(error) if self == Self::Automatic => {
                tracing::debug!("Skipping automatic lock export: {error:#}");
                Ok(None)
            }
            result => result,
        }
    }

    fn from_pyproject(
        pyproject: &toml::Value,
        environment_export: Option<bool>,
        preview: Preview,
    ) -> Result<Option<Self>> {
        let configured = pyproject
            .get("tool")
            .and_then(|tool| tool.get("uv"))
            .and_then(|uv| uv.get("export-lock"))
            .map(|value| {
                value
                    .as_bool()
                    .context("The project setting `tool.uv.export-lock` must be a boolean")
            })
            .transpose()?;
        let mode = match environment_export.or(configured) {
            Some(true) => Self::Required,
            Some(false) => return Ok(None),
            None => {
                if pyproject
                    .get("build-system")
                    .and_then(|system| system.get("build-backend"))
                    .and_then(toml::Value::as_str)
                    != Some("uv_build")
                {
                    return Ok(None);
                }
                Self::Automatic
            }
        };
        if !preview.is_enabled(PreviewFeature::LockedTools) {
            if mode == Self::Required {
                bail!("Exporting locks requires the `locked-tools` preview feature");
            }
            return Ok(None);
        }
        if pyproject
            .get("project")
            .and_then(|project| project.get("dynamic"))
            .and_then(toml::Value::as_array)
            .is_some_and(|fields| {
                fields.iter().any(|field| {
                    field.as_str().is_some_and(|field| {
                        field == "dependencies" || field == "optional-dependencies"
                    })
                })
            })
        {
            if mode == Self::Required {
                bail!("Cannot export a lock with dynamic dependency declarations");
            }
            return Ok(None);
        }
        Ok(Some(mode))
    }
}

pub struct ExportedLock {
    pub pylock: String,
    mode: ExportMode,
    root: PathBuf,
    requires_python: RequiresPython,
    metadata: ResolutionMetadata,
}

impl ExportedLock {
    /// Return `None` for errors during automatic export, and propagate errors for required export.
    pub fn skip_on_error<T, E: Display>(&self, result: Result<T, E>) -> Result<Option<T>, E> {
        self.mode.skip_on_error(result.map(Some))
    }

    pub fn project_metadata(&self) -> &ResolutionMetadata {
        &self.metadata
    }

    pub fn matches_metadata(&self, metadata: &ResolutionMetadata) -> Result<bool> {
        metadata_matches(&self.metadata, metadata, &self.root, &self.requires_python)
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

/// Export the runtime dependencies of the project supplied by the distribution.
pub async fn export(
    source_tree: &Path,
    workspace: Option<&Workspace>,
    database: &DistributionDatabase<'_, impl BuildContext>,
    preview: Preview,
    supports_export: bool,
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
    let Some(mode) = ExportMode::from_pyproject(&raw, environment_export, preview)? else {
        return Ok(None);
    };
    if !supports_export {
        if mode == ExportMode::Required {
            bail!("Packaged lock export requires the bundled uv_build backend");
        }
        return Ok(None);
    }
    mode.skip_on_error(
        export_inner(
            source_tree,
            workspace,
            database,
            &pyproject_path,
            &pyproject_contents,
            raw,
            mode,
        )
        .await,
    )
}

async fn export_inner(
    source_tree: &Path,
    workspace: Option<&Workspace>,
    database: &DistributionDatabase<'_, impl BuildContext>,
    pyproject_path: &Path,
    pyproject_contents: &str,
    raw: toml::Value,
    mode: ExportMode,
) -> Result<Option<ExportedLock>> {
    let pyproject: Pyproject = raw.try_into()?;
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
    if mode == ExportMode::Automatic
        && (workspace_dependency.is_some()
            || !lock.has_only_pypi_and_workspace_sources(&all_packages))
    {
        return Ok(None);
    }
    if let Some(dependency) = workspace_dependency {
        bail!("Cannot export a lock with workspace dependency `{dependency}`");
    }
    if mode == ExportMode::Automatic && Lock::has_pypi_artifacts_without_urls(&packages) {
        bail!(
            "Cannot export a lock with registry artifacts without URLs; run `uv lock` before building"
        );
    }
    let project_metadata =
        uv_pypi_types::PyProjectToml::from_toml(pyproject_contents, pyproject_path.display())?;
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
        let url = SafeUrl::from_directory_path(source_tree)
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
    pylock
        .validate_packaged_packages()
        .context("Cannot export an invalid lock")?;
    let (requires_dist, provides_extra) = match unlowered {
        Ok(requirements) => (requirements.requires_dist, requirements.provides_extra),
        Err(error) => {
            let metadata = metadata
                .as_ref()
                .with_context(|| format!("Cannot read dynamic project metadata: {error}"))?;
            (
                metadata
                    .requires_dist
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .collect(),
                metadata.provides_extra.clone(),
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
        requires_dist,
        provides_extra,
        dynamic: false,
    };
    if mode == ExportMode::Automatic && !has_only_expected_urls(toml::from_str(&pylock)?) {
        tracing::debug!("Not exporting a lock because it may contain unchecked URL data");
        return Ok(None);
    }
    Ok(Some(ExportedLock {
        pylock,
        mode,
        root: root.to_path_buf(),
        requires_python: lock.requires_python().clone(),
        metadata: project_metadata,
    }))
}

/// Reject unexpected URL-like values and unsupported artifact URLs.
fn has_only_expected_urls(mut remaining: toml::Value) -> bool {
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
            SafeUrl::parse_relaxed(url).is_ok_and(|url| {
                let path = url.path().to_ascii_lowercase();
                url.scheme() == "https"
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.fragment().is_none()
                    && !path.contains(':')
                    && !path.contains("//")
                    && !path.contains("%3a")
                    && !path.contains("%2f")
                    && url.into_unsafe_url().query().is_none()
            })
        })
    }

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
