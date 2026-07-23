use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use rustc_hash::FxHashMap;
use serde::Serialize;
use toml_edit::Value;
use toml_writer::{TomlWrite, WriteTomlValue};
use uv_distribution_types::{Requirement, RequirementSource, RequiresPython, SimplifiedMarkerTree};
use uv_fs::PortablePath;
use uv_normalize::PackageName;
use uv_pep508::MarkerTree;
use uv_preview::PreviewFeature;
use uv_pypi_types::ConflictKind;

use super::{
    COMPACT_REVISION, Dependency, DirectSource, ExcludeNewerOverride, ExcludeNewerValue,
    ForkStrategy, Lock, Package, PackageId, PrereleaseMode, REVISION, RegistrySource,
    ResolutionMode, ResolverManifest, ResolverOptions, Source, SourceDist, Wheel, WheelWireSource,
    simplified_universal_markers,
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

    let virtual_members = if uv_preview::is_enabled(PreviewFeature::NoLockVirtual) {
        lock.packages
            .iter()
            .filter(|package| lock.manifest.members.contains(&package.id.name))
            .filter_map(|package| {
                if let Source::Virtual(path) = &package.id.source {
                    Some((package.id.name.clone(), path.as_ref()))
                } else {
                    None
                }
            })
            .collect()
    } else {
        FxHashMap::default()
    };
    let compact_lockfile = lock.packages.iter().any(|package| {
        package
            .metadata
            .requires_dist
            .iter()
            .chain(package.metadata.dependency_groups.values().flatten())
            .any(|requirement| {
                can_omit_virtual_source(requirement, &package.id.name, &virtual_members)
            })
    });
    let revision = if compact_lockfile {
        lock.revision.max(COMPACT_REVISION)
    } else if lock.uses_compact_format() {
        REVISION
    } else {
        lock.revision
    };

    writer.key_value("version", lock.version)?;
    if revision > 0 {
        writer.key_value("revision", revision)?;
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
        writer.key_multiline_array("supported-markers", markers, |writer, marker| {
            writer.value(&marker)
        })?;
    }

    if !lock.required_environments.is_empty() {
        let markers = lock
            .required_environments
            .iter()
            .copied()
            .map(|marker| SimplifiedMarkerTree::new(&lock.requires_python, marker))
            .filter_map(SimplifiedMarkerTree::try_to_string);
        writer.key_multiline_array("required-markers", markers, |writer, marker| {
            writer.value(&marker)
        })?;
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

    write_options(writer, &lock.options)?;
    write_manifest(writer, &lock.manifest)?;

    // Count the number of packages for each package name. When there's only one package for a
    // particular package name (the overwhelmingly common case), we can omit some data (like
    // source and version) on dependency edges since it is strictly redundant.
    let mut dist_count_by_name: FxHashMap<PackageName, u64> = FxHashMap::default();
    for package in &lock.packages {
        *dist_count_by_name
            .entry(package.id.name.clone())
            .or_default() += 1;
    }

    for package in &lock.packages {
        write_package(
            writer,
            package,
            &lock.requires_python,
            simplified_environment,
            &dist_count_by_name,
            &virtual_members,
        )?;
    }

    Ok(())
}

fn write_options(writer: &mut LockWriter, options: &ResolverOptions) -> Result<(), WriteError> {
    let has_options = options.resolution_mode != ResolutionMode::default()
        || options.prerelease_mode != PrereleaseMode::default()
        || options.fork_strategy != ForkStrategy::default()
        || !options.exclude_newer.is_empty();
    if !has_options {
        return Ok(());
    }

    writer.table(&["options"])?;
    if options.resolution_mode != ResolutionMode::default() {
        writer.key_value("resolution-mode", options.resolution_mode.to_string())?;
    }
    if options.prerelease_mode != PrereleaseMode::default() {
        writer.key_value("prerelease-mode", options.prerelease_mode.to_string())?;
    }
    if options.fork_strategy != ForkStrategy::default() {
        writer.key_value("fork-strategy", options.fork_strategy.to_string())?;
    }

    let exclude_newer = &options.exclude_newer;
    if let Some(global) = &exclude_newer.global {
        if let Some(span) = global.span() {
            writer.key_start("exclude-newer")?;
            writer.value(ExcludeNewerValue::PLACEHOLDER)?;
            writer.raw(" # This has no effect and is included for backwards compatibility when using relative exclude-newer values.\n");
            writer.key_value("exclude-newer-span", span.to_string())?;
        } else {
            writer.key_value("exclude-newer", global.to_string())?;
        }
    }

    if !exclude_newer.package.is_empty() {
        writer.table(&["options", "exclude-newer-package"])?;
        for (name, setting) in &exclude_newer.package {
            match setting {
                ExcludeNewerOverride::Enabled(value) => {
                    if let Some(span) = value.span() {
                        writer.key_start(name.as_ref())?;
                        let mut first = true;
                        writer.start_inline_table();
                        writer.inline_value(
                            &mut first,
                            "timestamp",
                            ExcludeNewerValue::PLACEHOLDER,
                        )?;
                        writer.inline_value(&mut first, "span", span.to_string())?;
                        writer.finish_inline_table(first);
                        writer.raw("\n");
                    } else {
                        writer.key_value(name.as_ref(), value.to_string())?;
                    }
                }
                ExcludeNewerOverride::Disabled => {
                    writer.key_value(name.as_ref(), false)?;
                }
            }
        }
    }

    Ok(())
}

fn write_manifest(writer: &mut LockWriter, manifest: &ResolverManifest) -> Result<(), WriteError> {
    let has_dependency_groups = manifest
        .dependency_groups
        .values()
        .any(|requirements| !requirements.is_empty());
    let has_manifest = !manifest.members.is_empty()
        || !manifest.requirements.is_empty()
        || !manifest.constraints.is_empty()
        || !manifest.overrides.is_empty()
        || !manifest.excludes.is_empty()
        || !manifest.build_constraints.is_empty()
        || has_dependency_groups
        || !manifest.dependency_metadata.is_empty();
    if !has_manifest {
        return Ok(());
    }

    writer.table(&["manifest"])?;
    if !manifest.members.is_empty() {
        writer.key_multiline_array("members", &manifest.members, |writer, member| {
            writer.value(member.as_ref())
        })?;
    }
    write_serialized_non_empty_array(writer, "requirements", &manifest.requirements)?;
    write_serialized_non_empty_array(writer, "constraints", &manifest.constraints)?;
    write_serialized_non_empty_array(writer, "overrides", &manifest.overrides)?;
    write_serialized_non_empty_array(writer, "excludes", &manifest.excludes)?;
    write_serialized_non_empty_array(writer, "build-constraints", &manifest.build_constraints)?;

    if has_dependency_groups {
        writer.table(&["manifest", "dependency-groups"])?;
        for (group, requirements) in &manifest.dependency_groups {
            if requirements.is_empty() {
                continue;
            }
            write_serialized_non_empty_array(writer, group.as_ref(), requirements)?;
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
    requires_python: &RequiresPython,
    simplified_environment: MarkerTree,
    dist_count_by_name: &FxHashMap<PackageName, u64>,
    virtual_members: &FxHashMap<PackageName, &Path>,
) -> Result<(), WriteError> {
    writer.array_of_tables(&["package"])?;
    write_package_id(writer, &package.id, None, PackageIdLocation::Table)?;

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
                    simplified_environment,
                    dist_count_by_name,
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

    if package
        .optional_dependencies
        .values()
        .any(|dependencies| !dependencies.is_empty())
    {
        writer.table(&["package", "optional-dependencies"])?;
        for (extra, dependencies) in &package.optional_dependencies {
            if dependencies.is_empty() {
                continue;
            }
            writer.key_multiline_array(extra.as_ref(), dependencies, |writer, dependency| {
                write_dependency_inline(
                    writer,
                    dependency,
                    simplified_environment,
                    dist_count_by_name,
                )
            })?;
        }
    }

    if package
        .dependency_groups
        .values()
        .any(|dependencies| !dependencies.is_empty())
    {
        writer.table(&["package", "dev-dependencies"])?;
        for (group, dependencies) in &package.dependency_groups {
            if dependencies.is_empty() {
                continue;
            }
            writer.key_multiline_array(group.as_ref(), dependencies, |writer, dependency| {
                write_dependency_inline(
                    writer,
                    dependency,
                    simplified_environment,
                    dist_count_by_name,
                )
            })?;
        }
    }

    let metadata = &package.metadata;
    let has_metadata = !metadata.requires_dist.is_empty()
        || !metadata.dependency_groups.is_empty()
        || !metadata.provides_extra.is_empty();
    if has_metadata {
        writer.table(&["package", "metadata"])?;
        if !metadata.requires_dist.is_empty() {
            write_package_requirements(
                writer,
                "requires-dist",
                &package.id.name,
                &metadata.requires_dist,
                virtual_members,
            )?;
        }
        if !metadata.provides_extra.is_empty() {
            writer.key_start("provides-extras")?;
            writer.array(&metadata.provides_extra, |writer, extra| {
                writer.value(extra.as_ref())
            })?;
            writer.raw("\n");
        }

        if !metadata.dependency_groups.is_empty() {
            writer.table(&["package", "metadata", "requires-dev"])?;
            for (group, requirements) in &metadata.dependency_groups {
                write_package_requirements(
                    writer,
                    group.as_ref(),
                    &package.id.name,
                    requirements,
                    virtual_members,
                )?;
            }
        }
    }

    Ok(())
}

/// Writes requirement metadata, omitting virtual sources already recorded for workspace members.
fn write_package_requirements(
    writer: &mut LockWriter,
    key: &str,
    package_name: &PackageName,
    requirements: &BTreeSet<Requirement>,
    virtual_members: &FxHashMap<PackageName, &Path>,
) -> Result<(), WriteError> {
    writer.key_start(key)?;
    let write_requirement = |writer: &mut LockWriter, requirement: &Requirement| {
        let mut value = serialize_value(requirement)?;
        if can_omit_virtual_source(requirement, package_name, virtual_members)
            && let Value::InlineTable(table) = &mut value.0
        {
            table.remove("virtual");
        }
        writer.value(value)
    };

    if requirements.len() <= 1 {
        writer.array(requirements, write_requirement)?;
        writer.raw("\n");
    } else {
        writer.multiline_array(requirements, write_requirement)?;
    }

    Ok(())
}

/// Returns `true` if the requirement's virtual source can be inferred from a workspace member.
fn can_omit_virtual_source(
    requirement: &Requirement,
    package_name: &PackageName,
    virtual_members: &FxHashMap<PackageName, &Path>,
) -> bool {
    if &requirement.name == package_name {
        return false;
    }

    if let RequirementSource::Directory {
        install_path,
        r#virtual: Some(true),
        ..
    } = &requirement.source
    {
        virtual_members
            .get(&requirement.name)
            .is_some_and(|path| *path == install_path.as_ref())
    } else {
        false
    }
}

/// Writes a package identity, omitting fields that a unique package name makes redundant.
///
/// Passing no distribution counts forces the full version and source identity to be written.
fn write_package_id(
    writer: &mut LockWriter,
    package_id: &PackageId,
    dist_count_by_name: Option<&FxHashMap<PackageName, u64>>,
    mut location: PackageIdLocation<'_>,
) -> Result<(), WriteError> {
    let count = dist_count_by_name.and_then(|map| map.get(&package_id.name).copied());
    location.value(writer, "name", package_id.name.as_ref())?;
    if count.is_none_or(|count| count > 1) {
        if let Some(version) = &package_id.version {
            location.value(writer, "version", version.to_string())?;
        }
        location.nested_value(writer, "source", |writer| {
            write_source_inline(writer, &package_id.source)
        })?;
    }
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

fn write_source_inline(writer: &mut LockWriter, source: &Source) -> Result<(), WriteError> {
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
    if let Some(zstd) = &wheel.zstd {
        writer.inline_key_start(&mut first, "zstd")?;
        let mut zstd_first = true;
        writer.start_inline_table();
        if let Some(hash) = &zstd.hash {
            writer.inline_value(&mut zstd_first, "hash", hash.to_string())?;
        }
        if let Some(size) = zstd.size {
            writer.inline_value(&mut zstd_first, "size", size)?;
        }
        writer.finish_inline_table(zstd_first);
    }
    writer.finish_inline_table(first);
    Ok(())
}

/// Writes a dependency edge without identity or marker data implied by the enclosing resolution.
fn write_dependency_inline(
    writer: &mut LockWriter,
    dependency: &Dependency,
    simplified_environment: MarkerTree,
    dist_count_by_name: &FxHashMap<PackageName, u64>,
) -> Result<(), WriteError> {
    let mut first = true;
    writer.start_inline_table();

    write_package_id(
        writer,
        &dependency.package_id,
        Some(dist_count_by_name),
        PackageIdLocation::Inline(&mut first),
    )?;

    if !dependency.extra.is_empty() {
        writer.inline_key_start(&mut first, "extra")?;
        writer.array(&dependency.extra, |writer, extra| {
            writer.value(extra.as_ref())
        })?;
    }

    // Avoid restating the resolution's environment on every dependency edge.
    if let Some(marker) = dependency
        .simplified_marker
        .as_simplified_marker_tree()
        .restrict(simplified_environment)
        .try_to_string()
    {
        writer.inline_value(&mut first, "marker", &marker)?;
    }

    writer.finish_inline_table(first);
    Ok(())
}

/// Writes a Serde-backed array, omitting the key when the array is empty.
fn write_serialized_non_empty_array<T: Serialize>(
    writer: &mut LockWriter,
    key: &str,
    values: &BTreeSet<T>,
) -> Result<(), WriteError> {
    if values.is_empty() {
        return Ok(());
    }
    write_serialized_array(writer, key, values)
}

/// Writes a Serde-backed array using the canonical layout for its cardinality.
///
/// Empty and single-element arrays stay on one line, while larger arrays place each element on
/// its own line. Unlike [`write_serialized_non_empty_array`], this retains empty dependency groups.
fn write_serialized_array<T: Serialize>(
    writer: &mut LockWriter,
    key: &str,
    values: &BTreeSet<T>,
) -> Result<(), WriteError> {
    writer.key_start(key)?;
    let write_value = |writer: &mut LockWriter, value: &T| {
        let value = serialize_value(value)?;
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
