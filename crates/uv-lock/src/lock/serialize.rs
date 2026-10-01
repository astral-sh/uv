use std::collections::BTreeSet;
use std::fmt;

use serde::Serialize;
use toml_edit::Value;
use toml_writer::{TomlWrite, WriteTomlValue};
use uv_distribution_types::{Requirement, RequirementSource, RequiresPython, SimplifiedMarkerTree};
use uv_fs::PortablePath;
use uv_pep440::VersionSpecifiers;
use uv_pep508::MarkerTree;
use uv_pypi_types::ConflictKind;
use uv_redacted::DisplaySafeUrl;

use super::git::GitSourceWire;
use super::{
    Dependency, DirectSource, ExcludeNewerOverride, ExcludeNewerValue, ForkStrategy, Lock, Package,
    PackageId, PackageIdLookup, PrereleaseMode, RegistrySource, ResolutionMode, ResolverManifest,
    ResolverOptions, Source, SourceDist, Wheel, WheelWireSource, simplified_universal_markers,
};

/// Serializes a lockfile directly while preserving the canonical `uv.lock` layout.
pub(super) fn to_toml(lock: &Lock) -> Result<String, toml_edit::ser::Error> {
    let mut writer = LockWriter::default();
    write_lock(&mut writer, lock).map_err(|error| match error {
        WriteError::Format => {
            toml_edit::ser::Error::Custom("failed to write lockfile to a string".to_string())
        }
        WriteError::Serialize(error) => error,
    })?;
    Ok(writer.output)
}

fn write_lock(writer: &mut LockWriter, lock: &Lock) -> Result<(), WriteError> {
    // Catch a lockfile where the union of fork markers doesn't cover the supported
    // environments.
    debug_assert!(lock.check_marker_coverage().is_ok());

    writer.key_value("version", lock.version)?;
    if lock.revision > 0 {
        writer.key_value("revision", lock.revision)?;
    }
    writer.key_value("requires-python", lock.requires_python.to_string())?;

    if !lock.fork_markers.is_empty() {
        let markers = simplified_universal_markers(&lock.fork_markers, &lock.requires_python);
        if !markers.is_empty() {
            writer.key_multiline_array("resolution-markers", markers, |writer, marker| {
                writer.value(&marker)
            })?;
        }
    }

    // The simplified marker space covered by this resolution.
    let simplified_environment =
        SimplifiedMarkerTree::new(&lock.requires_python, lock.fork_markers_union())
            .as_simplified_marker_tree();

    if !lock.supported_environments.is_empty() {
        let markers = lock
            .supported_environments
            .iter()
            .copied()
            .map(|marker| SimplifiedMarkerTree::new(&lock.requires_python, marker))
            .filter_map(SimplifiedMarkerTree::try_to_string);
        let field = if lock.version >= 2 {
            "supported-environments"
        } else {
            "supported-markers"
        };
        writer.key_multiline_array(field, markers, |writer, marker| writer.value(&marker))?;
    }

    if !lock.required_environments.is_empty() {
        let markers = lock
            .required_environments
            .iter()
            .copied()
            .map(|marker| SimplifiedMarkerTree::new(&lock.requires_python, marker))
            .filter_map(SimplifiedMarkerTree::try_to_string);
        let field = if lock.version >= 2 {
            "required-environments"
        } else {
            "required-markers"
        };
        writer.key_multiline_array(field, markers, |writer, marker| writer.value(&marker))?;
    }

    if !lock.conflicts.is_empty() {
        writer.key_start("conflicts")?;
        writer.raw("[");
        for (index, set) in lock.conflicts.iter().enumerate() {
            if index > 0 {
                writer.raw(", ");
            }
            writer.raw("[\n");
            for item in set.iter() {
                writer.raw("    ");
                let mut first = true;
                writer.start_inline_table();
                writer.inline_value(&mut first, "package", item.package().as_ref())?;
                match item.kind() {
                    ConflictKind::Project => {}
                    ConflictKind::Extra(extra) => {
                        writer.inline_value(&mut first, "extra", extra.as_ref())?;
                    }
                    ConflictKind::Group(group) => {
                        writer.inline_value(&mut first, "group", group.as_ref())?;
                    }
                }
                writer.finish_inline_table(first);
                writer.raw(",\n");
            }
            writer.raw("]");
        }
        writer.raw("]\n");
    }

    write_options(writer, &lock.options, lock.version)?;
    write_manifest(writer, &lock.manifest, lock.version)?;

    let package_ids = PackageIdLookup::new(
        lock.version,
        lock.packages.iter().map(|package| &package.id),
    );

    for package in &lock.packages {
        write_package(
            writer,
            package,
            lock.version,
            &lock.requires_python,
            simplified_environment,
            &package_ids,
        )?;
    }

    Ok(())
}

fn write_options(
    writer: &mut LockWriter,
    options: &ResolverOptions,
    version: u32,
) -> Result<(), WriteError> {
    let has_options = options.resolution_mode != ResolutionMode::default()
        || options.prerelease.global != PrereleaseMode::default()
        || !options.prerelease.package.is_empty()
        || options.fork_strategy != ForkStrategy::default()
        || options.minimum_libc_version.is_some()
        || !options.exclude_newer.is_empty();
    if !has_options {
        return Ok(());
    }

    writer.table(&["options"])?;
    if options.resolution_mode != ResolutionMode::default() {
        writer.key_value("resolution-mode", options.resolution_mode.to_string())?;
    }
    if options.prerelease.global != PrereleaseMode::default() {
        writer.key_value("prerelease-mode", options.prerelease.global.to_string())?;
    }
    if options.fork_strategy != ForkStrategy::default() {
        writer.key_value("fork-strategy", options.fork_strategy.to_string())?;
    }
    if let Some(version) = options.minimum_libc_version {
        writer.key_value("minimum-libc-version", serialize_value(&version)?)?;
    }

    let exclude_newer = &options.exclude_newer;
    if let Some(global) = &exclude_newer.global {
        if version >= 2 {
            writer.key_start("exclude-newer")?;
            write_exclude_newer_value(writer, global, version)?;
            writer.raw("\n");
        } else if let Some(span) = global.span() {
            writer.key_start("exclude-newer")?;
            writer.value(ExcludeNewerValue::PLACEHOLDER)?;
            writer.raw(" # This has no effect and is included for backwards compatibility when using relative exclude-newer values.\n");
            writer.key_value("exclude-newer-span", span.to_string())?;
        } else {
            writer.key_value("exclude-newer", global.to_string())?;
        }
    }

    if !options.prerelease.package.is_empty() {
        writer.table(&["options", "prerelease-package"])?;
        let mut packages = options.prerelease.package.iter().collect::<Vec<_>>();
        packages.sort_unstable_by_key(|(name, _)| *name);
        for (name, mode) in packages {
            writer.key_value(name.as_ref(), mode.to_string())?;
        }
    }

    if !exclude_newer.package.is_empty() {
        writer.table(&["options", "exclude-newer-package"])?;
        for (name, setting) in &exclude_newer.package {
            match setting {
                ExcludeNewerOverride::Enabled(value) => {
                    writer.key_start(name.as_ref())?;
                    write_exclude_newer_value(writer, value, version)?;
                    writer.raw("\n");
                }
                ExcludeNewerOverride::Disabled => {
                    writer.key_value(name.as_ref(), false)?;
                }
            }
        }
    }

    Ok(())
}

/// Writes the shared value format for global and package-specific upload cutoffs.
fn write_exclude_newer_value(
    writer: &mut LockWriter,
    value: &ExcludeNewerValue,
    version: u32,
) -> Result<(), WriteError> {
    if let Some(span) = value.span() {
        let mut first = true;
        writer.start_inline_table();
        if version < 2 {
            writer.inline_value(&mut first, "timestamp", ExcludeNewerValue::PLACEHOLDER)?;
        }
        writer.inline_value(&mut first, "span", span.to_string())?;
        writer.finish_inline_table(first);
        Ok(())
    } else {
        writer.value(value.to_string())
    }
}

fn write_manifest(
    writer: &mut LockWriter,
    manifest: &ResolverManifest,
    version: u32,
) -> Result<(), WriteError> {
    let groups = manifest
        .dependency_groups
        .keys()
        .chain(
            manifest
                .group_requires_python
                .keys()
                .filter(|_| version >= 2),
        )
        .collect::<BTreeSet<_>>();
    let has_manifest = manifest.default_groups.is_some()
        || !manifest.members.is_empty()
        || !manifest.requirements.is_empty()
        || !manifest.constraints.is_empty()
        || !manifest.overrides.is_empty()
        || !manifest.excludes.is_empty()
        || !manifest.build_constraints.is_empty();
    // Subtables define their parent implicitly, so only write the header for direct entries.
    if has_manifest {
        writer.table(&["manifest"])?;
    }

    if !manifest.members.is_empty() {
        writer.key_multiline_array("members", &manifest.members, |writer, member| {
            writer.value(member.as_ref())
        })?;
    }
    if let Some(groups) = &manifest.default_groups {
        writer.key_value("default-groups", serialize_value(groups)?)?;
    }
    let field = if version >= 2 {
        "dependencies"
    } else {
        "requirements"
    };
    if !manifest.requirements.is_empty() {
        write_requirements(writer, field, &manifest.requirements, version)?;
    }
    write_serialized_non_empty_array(writer, "constraints", &manifest.constraints, version)?;
    write_serialized_non_empty_array(writer, "overrides", &manifest.overrides, version)?;
    write_serialized_non_empty_array(writer, "excludes", &manifest.excludes, version)?;
    write_serialized_non_empty_array(
        writer,
        "build-constraints",
        &manifest.build_constraints,
        version,
    )?;

    if !groups.is_empty() {
        writer.table(&["manifest", "dependency-groups"])?;
        for group in groups {
            let empty = BTreeSet::new();
            let requirements = manifest.dependency_groups.get(group).unwrap_or(&empty);
            let requires_python = manifest
                .group_requires_python
                .get(group)
                .and_then(|metadata| metadata.requires_python.as_ref())
                .filter(|_| version >= 2);
            if let Some(requires_python) = requires_python {
                write_dependency_group_inline(
                    writer,
                    group.as_ref(),
                    requires_python,
                    requirements,
                    |writer, requirement| write_requirement_inline(writer, requirement, version),
                )?;
            } else {
                write_requirements(writer, group.as_ref(), requirements, version)?;
            }
        }
    }

    if version < 2 && !manifest.group_requires_python.is_empty() {
        writer.table(&["manifest", "group-requires-python"])?;
        for (group, metadata) in &manifest.group_requires_python {
            if let Some(requires_python) = &metadata.requires_python {
                writer.key_value(group.as_ref(), serialize_value(requires_python)?)?;
            }
        }
    }

    for metadata in &manifest.dependency_metadata {
        writer.array_of_tables(&["manifest", "dependency-metadata"])?;
        writer.key_value("name", metadata.name.as_ref())?;
        if let Some(version) = metadata.version.as_ref() {
            writer.key_value("version", version.to_string())?;
        }
        if !metadata.requires_dist.is_empty() {
            let value = serialize_value(&metadata.requires_dist)?;
            writer.key_value("requires-dist", value)?;
        }
        if let Some(requires_python) = metadata.requires_python.as_ref() {
            writer.key_value("requires-python", requires_python.to_string())?;
        }
        if !metadata.provides_extra.is_empty() {
            let value = serialize_value(&metadata.provides_extra)?;
            writer.key_value("provides-extras", value)?;
        }
    }

    Ok(())
}

fn write_package(
    writer: &mut LockWriter,
    package: &Package,
    version: u32,
    requires_python: &RequiresPython,
    simplified_environment: MarkerTree,
    package_ids: &PackageIdLookup<'_>,
) -> Result<(), WriteError> {
    writer.array_of_tables(&["package"])?;
    write_package_id(writer, &package.id, version, None, PackageIdLocation::Table)?;
    if let Some(groups) = &package.default_groups {
        writer.key_value("default-groups", serialize_value(groups)?)?;
    }

    if !package.fork_markers.is_empty() {
        let markers = simplified_universal_markers(&package.fork_markers, requires_python);
        if !markers.is_empty() {
            writer.key_multiline_array("resolution-markers", markers, |writer, marker| {
                writer.value(&marker)
            })?;
        }
    }

    if !package.dependencies.is_empty() {
        writer.key_multiline_array(
            "dependencies",
            &package.dependencies,
            |writer, dependency| {
                write_dependency_inline(
                    writer,
                    dependency,
                    version,
                    simplified_environment,
                    package_ids,
                )
            },
        )?;
    }

    if let Some(source_dist) = &package.sdist {
        writer.key_start("sdist")?;
        write_source_dist_inline(writer, source_dist)?;
        writer.raw("\n");
    }

    if !package.wheels.is_empty() {
        writer.key_multiline_array("wheels", &package.wheels, write_wheel_inline)?;
    }

    if !package.optional_dependencies.is_empty() {
        writer.table(&["package", "optional-dependencies"])?;
        for (extra, dependencies) in &package.optional_dependencies {
            if dependencies.is_empty() {
                writer.key_start(extra.as_ref())?;
                writer.raw("[]\n");
                continue;
            }
            writer.key_multiline_array(extra.as_ref(), dependencies, |writer, dependency| {
                write_dependency_inline(
                    writer,
                    dependency,
                    version,
                    simplified_environment,
                    package_ids,
                )
            })?;
        }
    }

    let groups = package
        .dependency_groups
        .keys()
        .chain(
            package
                .group_requires_python
                .keys()
                .filter(|_| version >= 2),
        )
        .collect::<BTreeSet<_>>();
    if !groups.is_empty() {
        let field = if version >= 2 {
            "dependency-groups"
        } else {
            "dev-dependencies"
        };
        writer.table(&["package", field])?;
        for group in groups {
            let dependencies = package
                .dependency_groups
                .get(group)
                .map_or(&[][..], Vec::as_slice);
            let requires_python = package
                .group_requires_python
                .get(group)
                .and_then(|metadata| metadata.requires_python.as_ref())
                .filter(|_| version >= 2);
            let write_dependency = |writer: &mut LockWriter, dependency: &Dependency| {
                write_dependency_inline(
                    writer,
                    dependency,
                    version,
                    simplified_environment,
                    package_ids,
                )
            };
            if let Some(requires_python) = requires_python {
                write_dependency_group_inline(
                    writer,
                    group.as_ref(),
                    requires_python,
                    dependencies,
                    write_dependency,
                )?;
            } else if dependencies.is_empty() {
                writer.key_start(group.as_ref())?;
                writer.raw("[]\n");
            } else {
                writer.key_multiline_array(group.as_ref(), dependencies, write_dependency)?;
            }
        }
    }

    if version < 2 && !package.group_requires_python.is_empty() {
        writer.table(&["package", "group-requires-python"])?;
        for (group, metadata) in &package.group_requires_python {
            if let Some(requires_python) = &metadata.requires_python {
                writer.key_value(group.as_ref(), serialize_value(requires_python)?)?;
            }
        }
    }

    let metadata = &package.metadata;
    let has_metadata = !metadata.requires_dist.is_empty()
        || !metadata.dependency_groups.is_empty()
        || !metadata.provides_extra.is_empty();
    if has_metadata {
        writer.table(&["package", "metadata"])?;
        let field = if version >= 2 {
            "dependencies"
        } else {
            "requires-dist"
        };
        if !metadata.requires_dist.is_empty() {
            write_requirements(writer, field, &metadata.requires_dist, version)?;
        }
        if !metadata.provides_extra.is_empty() {
            writer.key_start("provides-extras")?;
            writer.array(&metadata.provides_extra, |writer, extra| {
                writer.value(extra.as_ref())
            })?;
            writer.raw("\n");
        }

        if !metadata.dependency_groups.is_empty() {
            let field = if version >= 2 {
                "dependency-groups"
            } else {
                "requires-dev"
            };
            writer.table(&["package", "metadata", field])?;
            for (group, requirements) in &metadata.dependency_groups {
                write_requirements(writer, group.as_ref(), requirements, version)?;
            }
        }
    }

    Ok(())
}

/// Writes the minimum package identity supported by the lockfile version.
///
/// Package entries carry the full identity; dependency edges may omit redundant fields.
fn write_package_id(
    writer: &mut LockWriter,
    package_id: &PackageId,
    version: u32,
    package_ids: Option<&PackageIdLookup<'_>>,
    mut location: PackageIdLocation<'_>,
) -> Result<(), WriteError> {
    location.value(writer, "name", package_id.name.as_ref())?;
    if let Some(package_ids) = package_ids {
        if package_ids
            .unambiguous(&package_id.name, None, None)
            .is_some()
        {
            return Ok(());
        }
        if version >= 2 {
            if let Some(package_version) = &package_id.version
                && package_ids
                    .unambiguous(&package_id.name, Some(package_version), None)
                    .is_some()
            {
                return location.value(writer, "version", package_version.to_string());
            }
            if package_ids
                .unambiguous(&package_id.name, None, Some(&package_id.source))
                .is_some()
            {
                return location.nested_value(writer, "source", |writer| {
                    write_source_inline(writer, &package_id.source, version)
                });
            }
        }
    }
    if let Some(package_version) = &package_id.version {
        location.value(writer, "version", package_version.to_string())?;
    }
    location.nested_value(writer, "source", |writer| {
        write_source_inline(writer, &package_id.source, version)
    })?;
    Ok(())
}

enum PackageIdLocation<'a> {
    Table,
    Inline(&'a mut bool),
}

impl PackageIdLocation<'_> {
    fn value(
        &mut self,
        writer: &mut LockWriter,
        key: &str,
        value: impl WriteTomlValue,
    ) -> Result<(), WriteError> {
        match self {
            Self::Table => writer.key_value(key, value),
            Self::Inline(first) => writer.inline_value(first, key, value),
        }
    }

    fn nested_value(
        &mut self,
        writer: &mut LockWriter,
        key: &str,
        write_value: impl FnOnce(&mut LockWriter) -> Result<(), WriteError>,
    ) -> Result<(), WriteError> {
        match self {
            Self::Table => {
                writer.key_start(key)?;
                write_value(writer)?;
                writer.raw("\n");
            }
            Self::Inline(first) => {
                writer.inline_key_start(first, key)?;
                write_value(writer)?;
            }
        }
        Ok(())
    }
}

fn write_source_inline(
    writer: &mut LockWriter,
    source: &Source,
    version: u32,
) -> Result<(), WriteError> {
    if version >= 2
        && let Source::Git(url, _) = source
    {
        return writer.value(serialize_git_source(url.as_ref())?);
    }
    let mut first = true;
    writer.start_inline_table();
    match source {
        Source::Registry(source) => match source {
            RegistrySource::Url(url) => {
                writer.inline_value(&mut first, "registry", url.as_ref())?;
            }
            RegistrySource::Path(path) => {
                writer.inline_value(
                    &mut first,
                    "registry",
                    PortablePath::from(path).to_string(),
                )?;
            }
        },
        Source::Git(url, _) => {
            writer.inline_value(&mut first, "git", url.as_ref())?;
        }
        Source::Direct(url, DirectSource { subdirectory }) => {
            writer.inline_value(&mut first, "url", url.as_ref())?;
            if let Some(subdirectory) = subdirectory {
                writer.inline_value(
                    &mut first,
                    "subdirectory",
                    PortablePath::from(subdirectory).to_string(),
                )?;
            }
        }
        Source::Path(path) => {
            writer.inline_value(&mut first, "path", PortablePath::from(path).to_string())?;
        }
        Source::Directory(path) => {
            writer.inline_value(
                &mut first,
                "directory",
                PortablePath::from(path).to_string(),
            )?;
        }
        Source::Editable(path) => {
            writer.inline_value(&mut first, "editable", PortablePath::from(path).to_string())?;
        }
        Source::Virtual(path) => {
            writer.inline_value(&mut first, "virtual", PortablePath::from(path).to_string())?;
        }
    }
    writer.finish_inline_table(first);
    Ok(())
}

fn write_source_dist_inline(
    writer: &mut LockWriter,
    source_dist: &SourceDist,
) -> Result<(), WriteError> {
    let mut first = true;
    writer.start_inline_table();
    match source_dist {
        SourceDist::Metadata { .. } => {}
        SourceDist::Url { url, .. } => {
            writer.inline_value(&mut first, "url", url.as_ref())?;
        }
        SourceDist::Path { path, .. } => {
            writer.inline_value(&mut first, "path", PortablePath::from(path).to_string())?;
        }
    }
    if let Some(hash) = source_dist.hash() {
        writer.inline_value(&mut first, "hash", hash.to_string())?;
    }
    if let Some(size) = source_dist.size() {
        writer.inline_value(&mut first, "size", size)?;
    }
    if let Some(upload_time) = source_dist.upload_time() {
        writer.inline_value(&mut first, "upload-time", upload_time.to_string())?;
    }
    writer.finish_inline_table(first);
    Ok(())
}

fn write_wheel_inline(writer: &mut LockWriter, wheel: &Wheel) -> Result<(), WriteError> {
    let mut first = true;
    writer.start_inline_table();
    match &wheel.url {
        WheelWireSource::Url { url } => {
            writer.inline_value(&mut first, "url", url.as_ref())?;
        }
        WheelWireSource::Path { path } => {
            writer.inline_value(&mut first, "path", PortablePath::from(path).to_string())?;
        }
        WheelWireSource::Filename { filename } => {
            writer.inline_value(&mut first, "filename", filename.to_string())?;
        }
    }
    if let Some(hash) = &wheel.hash {
        writer.inline_value(&mut first, "hash", hash.to_string())?;
    }
    if let Some(size) = wheel.size {
        writer.inline_value(&mut first, "size", size)?;
    }
    if let Some(upload_time) = wheel.upload_time {
        writer.inline_value(&mut first, "upload-time", upload_time.to_string())?;
    }
    writer.finish_inline_table(first);
    Ok(())
}

/// Writes a dependency edge without identity or marker data implied by the enclosing resolution.
fn write_dependency_inline(
    writer: &mut LockWriter,
    dependency: &Dependency,
    version: u32,
    simplified_environment: MarkerTree,
    package_ids: &PackageIdLookup<'_>,
) -> Result<(), WriteError> {
    // Avoid restating the resolution's environment on every dependency edge.
    let marker = dependency
        .simplified_marker
        .as_simplified_marker_tree()
        .restrict(simplified_environment)
        .try_to_string();
    if version >= 2
        && package_ids
            .unambiguous(&dependency.package_id.name, None, None)
            .is_some()
        && dependency.extra.is_empty()
        && marker.is_none()
    {
        return writer.value(dependency.package_id.name.as_ref());
    }

    let mut first = true;
    writer.start_inline_table();

    write_package_id(
        writer,
        &dependency.package_id,
        version,
        Some(package_ids),
        PackageIdLocation::Inline(&mut first),
    )?;

    if !dependency.extra.is_empty() {
        writer.inline_key_start(&mut first, if version >= 2 { "extras" } else { "extra" })?;
        writer.array(&dependency.extra, |writer, extra| {
            writer.value(extra.as_ref())
        })?;
    }

    if let Some(marker) = marker {
        writer.inline_value(&mut first, "marker", &marker)?;
    }

    writer.finish_inline_table(first);
    Ok(())
}

/// Writes a dependency group carrying a Python requirement alongside its dependencies.
fn write_dependency_group_inline<I, T, F>(
    writer: &mut LockWriter,
    group: &str,
    requires_python: &VersionSpecifiers,
    dependencies: I,
    write_dependency: F,
) -> Result<(), WriteError>
where
    I: IntoIterator<Item = T>,
    F: FnMut(&mut LockWriter, T) -> Result<(), WriteError>,
{
    writer.key_start(group)?;
    let mut first = true;
    writer.start_inline_table();
    writer.inline_value(
        &mut first,
        "requires-python",
        serialize_value(requires_python)?,
    )?;
    writer.inline_key_start(&mut first, "dependencies")?;
    writer.array(dependencies, write_dependency)?;
    writer.finish_inline_table(first);
    writer.raw("\n");
    Ok(())
}

/// Writes declared requirements using the canonical layout for their cardinality.
fn write_requirements(
    writer: &mut LockWriter,
    key: &str,
    requirements: &BTreeSet<Requirement>,
    version: u32,
) -> Result<(), WriteError> {
    writer.key_start(key)?;
    let write_requirement = |writer: &mut LockWriter, requirement: &Requirement| {
        write_requirement_inline(writer, requirement, version)
    };
    if requirements.len() <= 1 {
        writer.array(requirements, write_requirement)?;
        writer.raw("\n");
    } else {
        writer.multiline_array(requirements, write_requirement)?;
    }
    Ok(())
}

/// Writes unqualified requirements as names in v2, retaining tables for all other declarations.
fn write_requirement_inline(
    writer: &mut LockWriter,
    requirement: &Requirement,
    version: u32,
) -> Result<(), WriteError> {
    let name_only = match &requirement.source {
        RequirementSource::Registry {
            specifier,
            index,
            conflict,
        } => specifier.is_empty() && index.is_none() && conflict.is_none(),
        RequirementSource::Url { .. }
        | RequirementSource::GitDirectory { .. }
        | RequirementSource::GitPath { .. }
        | RequirementSource::Path { .. }
        | RequirementSource::Directory { .. } => false,
    };
    if version >= 2
        && name_only
        && requirement.extras.is_empty()
        && requirement.groups.is_empty()
        && requirement.marker.is_true()
    {
        writer.value(requirement.name.as_ref())
    } else {
        let mut value = serialize_value(requirement)?;
        if version >= 2 {
            structure_git_sources(&mut value.0)?;
        }
        writer.value(value)
    }
}

/// Expands Git URLs in requirements and nested package override dependencies.
fn structure_git_sources(value: &mut Value) -> Result<(), WriteError> {
    match value {
        Value::InlineTable(table) => {
            if let Some(git) = table.get("git").and_then(Value::as_str) {
                let source = serialize_git_source(git)?;
                if let Some(source) = source.0.as_inline_table() {
                    table.remove("git");
                    for (key, value) in source {
                        table.insert(key, value.clone());
                    }
                }
            }
            if let Some(dependencies) = table.get_mut("dependencies") {
                structure_git_sources(dependencies)?;
            }
        }
        Value::Array(array) => {
            for value in array.iter_mut() {
                structure_git_sources(value)?;
            }
        }
        Value::String(_)
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Boolean(_)
        | Value::Datetime(_) => {}
    }
    Ok(())
}

/// Adapts the internal Git URL to explicit repository and checkout fields.
fn serialize_git_source(url: &str) -> Result<SerializedValue, WriteError> {
    let url =
        DisplaySafeUrl::parse(url).map_err(|err| toml_edit::ser::Error::Custom(err.to_string()))?;
    serialize_value(&GitSourceWire::from_url(url))
}

/// Writes a Serde-backed array, omitting the key when the array is empty.
fn write_serialized_non_empty_array<T: Serialize>(
    writer: &mut LockWriter,
    key: &str,
    values: &BTreeSet<T>,
    version: u32,
) -> Result<(), WriteError> {
    if values.is_empty() {
        return Ok(());
    }
    write_serialized_array(writer, key, values, version)
}

/// Writes a Serde-backed array using the canonical layout for its cardinality.
///
/// Empty and single-element arrays stay on one line, while larger arrays place each element on
/// its own line. Unlike [`write_serialized_non_empty_array`], this retains empty dependency groups.
fn write_serialized_array<T: Serialize>(
    writer: &mut LockWriter,
    key: &str,
    values: &BTreeSet<T>,
    version: u32,
) -> Result<(), WriteError> {
    writer.key_start(key)?;
    let write_value = |writer: &mut LockWriter, value: &T| {
        let mut value = serialize_value(value)?;
        if version >= 2 {
            structure_git_sources(&mut value.0)?;
        }
        writer.value(value)
    };
    if values.len() <= 1 {
        writer.array(values, write_value)?;
        writer.raw("\n");
    } else {
        writer.multiline_array(values, write_value)?;
    }
    Ok(())
}

/// Adapts values without native `toml_writer` support through Serde's TOML value serializer.
struct SerializedValue(Value);

impl WriteTomlValue for SerializedValue {
    fn write_toml_value<W: TomlWrite + ?Sized>(&self, writer: &mut W) -> fmt::Result {
        writer.write_str(&self.0.to_string())
    }
}

fn serialize_value<T: Serialize + ?Sized>(value: &T) -> Result<SerializedValue, WriteError> {
    Ok(SerializedValue(Serialize::serialize(
        value,
        toml_edit::ser::ValueSerializer::new(),
    )?))
}

#[derive(Debug)]
enum WriteError {
    Format,
    Serialize(toml_edit::ser::Error),
}

impl From<fmt::Error> for WriteError {
    fn from(_: fmt::Error) -> Self {
        Self::Format
    }
}

impl From<toml_edit::ser::Error> for WriteError {
    fn from(error: toml_edit::ser::Error) -> Self {
        Self::Serialize(error)
    }
}

/// Emits TOML while retaining the established whitespace and inline-table layout of `uv.lock`.
#[derive(Default)]
struct LockWriter {
    output: String,
}

impl LockWriter {
    fn raw(&mut self, value: &str) {
        self.output.push_str(value);
    }

    fn key(&mut self, key: &str) -> fmt::Result {
        self.output.key(key)
    }

    fn value(&mut self, value: impl WriteTomlValue) -> Result<(), WriteError> {
        self.output.value(value)?;
        Ok(())
    }

    fn key_start(&mut self, key: &str) -> Result<(), WriteError> {
        self.key(key)?;
        self.raw(" = ");
        Ok(())
    }

    fn key_value(&mut self, key: &str, value: impl WriteTomlValue) -> Result<(), WriteError> {
        self.key_start(key)?;
        self.value(value)?;
        self.raw("\n");
        Ok(())
    }

    fn table(&mut self, path: &[&str]) -> Result<(), WriteError> {
        self.header(path, false)
    }

    fn array_of_tables(&mut self, path: &[&str]) -> Result<(), WriteError> {
        self.header(path, true)
    }

    /// Starts a table header on a new line, separating it from the preceding table body.
    fn header(&mut self, path: &[&str], array: bool) -> Result<(), WriteError> {
        self.raw("\n");
        if array {
            self.raw("[[");
        } else {
            self.raw("[");
        }
        for (index, key) in path.iter().enumerate() {
            if index > 0 {
                self.raw(".");
            }
            self.key(key)?;
        }
        if array {
            self.raw("]]\n");
        } else {
            self.raw("]\n");
        }
        Ok(())
    }

    fn key_multiline_array<I, T, F>(
        &mut self,
        key: &str,
        values: I,
        write_value: F,
    ) -> Result<(), WriteError>
    where
        I: IntoIterator<Item = T>,
        F: FnMut(&mut Self, T) -> Result<(), WriteError>,
    {
        self.key_start(key)?;
        self.multiline_array(values, write_value)
    }

    fn multiline_array<I, T, F>(&mut self, values: I, mut write_value: F) -> Result<(), WriteError>
    where
        I: IntoIterator<Item = T>,
        F: FnMut(&mut Self, T) -> Result<(), WriteError>,
    {
        self.raw("[\n");
        for value in values {
            self.raw("    ");
            write_value(self, value)?;
            self.raw(",\n");
        }
        self.raw("]\n");
        Ok(())
    }

    fn array<I, T, F>(&mut self, values: I, mut write_value: F) -> Result<(), WriteError>
    where
        I: IntoIterator<Item = T>,
        F: FnMut(&mut Self, T) -> Result<(), WriteError>,
    {
        self.raw("[");
        for (index, value) in values.into_iter().enumerate() {
            if index > 0 {
                self.raw(", ");
            }
            write_value(self, value)?;
        }
        self.raw("]");
        Ok(())
    }

    fn start_inline_table(&mut self) {
        self.raw("{");
    }

    fn finish_inline_table(&mut self, first: bool) {
        if !first {
            self.raw(" ");
        }
        self.raw("}");
    }

    /// Writes the separator and key for the next inline-table entry.
    fn inline_key_start(&mut self, first: &mut bool, key: &str) -> Result<(), WriteError> {
        if *first {
            self.raw(" ");
            *first = false;
        } else {
            self.raw(", ");
        }
        self.key_start(key)
    }

    fn inline_value(
        &mut self,
        first: &mut bool,
        key: &str,
        value: impl WriteTomlValue,
    ) -> Result<(), WriteError> {
        self.inline_key_start(first, key)?;
        self.value(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{LockWriter, Value};

    #[test]
    fn string_encoding_matches_toml_edit() {
        for value in [
            "",
            "https://example.com/packages/example-1.0.0-py3-none-any.whl",
            "it's valid",
            "unicode-λ",
            "contains\"quote",
            r"contains\backslash",
            "contains\ttab",
            "contains\nnewline",
            "contains\u{7f}delete",
        ] {
            let mut writer = LockWriter::default();
            writer.value(value).expect("writing to a string succeeds");
            assert_eq!(writer.output, Value::from(value).to_string());
        }
    }
}
