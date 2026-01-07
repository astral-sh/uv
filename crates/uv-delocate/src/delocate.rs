//! Delocate operations for macOS wheels.

use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use fs_err as fs;
use tracing::{debug, trace};
use walkdir::WalkDir;

use uv_distribution_filename::WheelFilename;
use uv_platform_tags::PlatformTag;
use uv_static::EnvVars;

use crate::error::DelocateError;
use crate::macho::{self, Arch, MacOSVersion};
use crate::wheel;

/// Options for delocating a wheel.
#[derive(Debug, Clone)]
pub struct DelocateOptions {
    /// Subdirectory within the package to store copied libraries.
    /// Defaults to ".dylibs".
    pub lib_sdir: String,
    /// Required architectures to validate.
    pub require_archs: Vec<Arch>,
    /// Libraries to exclude from delocating (by name pattern).
    pub exclude: Vec<String>,
    /// Remove absolute rpaths from binaries.
    /// This prevents issues when wheels are installed in different locations.
    pub sanitize_rpaths: bool,
    /// Check that bundled libraries don't require a newer macOS version than
    /// declared in the wheel's platform tag.
    pub check_version_compatibility: bool,
    /// Target macOS version. If set, overrides the version from the wheel's platform tag.
    /// This can also be set via the `MACOSX_DEPLOYMENT_TARGET` environment variable.
    pub target_macos_version: Option<MacOSVersion>,
}

impl Default for DelocateOptions {
    fn default() -> Self {
        let target_macos_version = env::var(EnvVars::MACOSX_DEPLOYMENT_TARGET)
            .ok()
            .and_then(|value| MacOSVersion::parse(&value));

        Self {
            lib_sdir: ".dylibs".to_string(),
            require_archs: Vec::new(),
            exclude: Vec::new(),
            sanitize_rpaths: true,
            check_version_compatibility: true,
            target_macos_version,
        }
    }
}

/// System library prefixes that should not be bundled.
///
/// See: <https://github.com/matthew-brett/delocate/blob/d0ec232826dd31cc80bfcc8adedfd8be78aff0b4/delocate/libsana.py#L35>
const SYSTEM_PREFIXES: &[&str] = &["/usr/lib", "/System"];

/// Check if a path is a system library that shouldn't be bundled.
fn is_system_library(path: &str) -> bool {
    SYSTEM_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

/// Search for a library in DYLD environment paths.
fn search_dyld_paths(lib_name: &str) -> Option<PathBuf> {
    const DEFAULT_FALLBACK_PATHS: &[&str] = &["/usr/local/lib", "/usr/lib"];

    // Search DYLD_LIBRARY_PATH first.
    if let Ok(paths) = env::var(EnvVars::DYLD_LIBRARY_PATH) {
        for dir in paths.split(':') {
            let candidate = Path::new(dir).join(lib_name);
            if let Ok(path) = candidate.canonicalize() {
                return Some(path);
            }
        }
    }

    // Then search DYLD_FALLBACK_LIBRARY_PATH.
    if let Ok(paths) = env::var(EnvVars::DYLD_FALLBACK_LIBRARY_PATH) {
        for dir in paths.split(':') {
            let candidate = Path::new(dir).join(lib_name);
            if let Ok(path) = candidate.canonicalize() {
                return Some(path);
            }
        }
    }

    // Default fallback paths.
    for dir in DEFAULT_FALLBACK_PATHS {
        let candidate = Path::new(dir).join(lib_name);
        if let Ok(path) = candidate.canonicalize() {
            return Some(path);
        }
    }

    None
}

/// Resolve a dynamic path token (`@loader_path`, `@rpath`, `@executable_path`).
fn resolve_dynamic_path(
    install_name: &str,
    binary_path: &Path,
    rpaths: &[String],
) -> Option<PathBuf> {
    if install_name.starts_with("@loader_path/") {
        // @loader_path is relative to the binary containing the reference.
        let relative = install_name.strip_prefix("@loader_path/").unwrap();
        let parent = binary_path.parent()?;
        let resolved = parent.join(relative);
        if let Ok(path) = resolved.canonicalize() {
            return Some(path);
        }
    } else if install_name.starts_with("@executable_path/") {
        // @executable_path; for wheels, we treat this similarly to @loader_path.
        let relative = install_name.strip_prefix("@executable_path/").unwrap();
        let parent = binary_path.parent()?;
        let resolved = parent.join(relative);
        if let Ok(path) = resolved.canonicalize() {
            return Some(path);
        }
    } else if install_name.starts_with("@rpath/") {
        // @rpath; search through rpaths.
        let relative = install_name.strip_prefix("@rpath/").unwrap();
        for rpath in rpaths {
            // Resolve rpath itself if it contains tokens.
            let resolved_rpath = if rpath.starts_with("@loader_path/") {
                let rpath_relative = rpath.strip_prefix("@loader_path/").unwrap();
                binary_path.parent()?.join(rpath_relative)
            } else if rpath.starts_with("@executable_path/") {
                let rpath_relative = rpath.strip_prefix("@executable_path/").unwrap();
                binary_path.parent()?.join(rpath_relative)
            } else {
                PathBuf::from(rpath)
            };

            let candidate = resolved_rpath.join(relative);
            if let Ok(path) = candidate.canonicalize() {
                return Some(path);
            }
        }
    } else if !install_name.starts_with('@') {
        // Absolute or relative path.
        let path = PathBuf::from(install_name);
        if path.is_absolute() {
            if let Ok(resolved) = path.canonicalize() {
                return Some(resolved);
            }
        }
        // Try relative to binary.
        if let Some(parent) = binary_path.parent() {
            let candidate = parent.join(&path);
            if let Ok(resolved) = candidate.canonicalize() {
                return Some(resolved);
            }
        }
        // Try DYLD environment paths for bare library names.
        if let Some(lib_name) = Path::new(install_name).file_name() {
            if let Some(resolved) = search_dyld_paths(&lib_name.to_string_lossy()) {
                return Some(resolved);
            }
        }
    }

    None
}

/// Information about a library and its dependents.
#[derive(Debug)]
struct LibraryInfo {
    /// Binaries that depend on this library and their install names.
    dependents: HashMap<PathBuf, String>,
}

/// Remove absolute rpaths from a binary.
///
/// This prevents issues when wheels are installed in different locations,
/// as absolute rpaths would point to the original build location.
fn sanitize_rpaths(path: &Path) -> Result<(), DelocateError> {
    let macho = macho::parse_macho(path)?;

    for rpath in &macho.rpaths {
        // Remove rpaths that are absolute and don't use @ tokens.
        if !rpath.starts_with('@') && Path::new(rpath).is_absolute() {
            macho::delete_rpath(path, rpath)?;
        }
    }

    Ok(())
}

/// Get the minimum macOS version from platform tags.
///
/// Returns the minimum version across all macOS platform tags, or `None` if there are no macOS tags.
fn get_macos_version(platform_tags: &[PlatformTag]) -> Option<MacOSVersion> {
    platform_tags
        .iter()
        .filter_map(|tag| match tag {
            PlatformTag::Macos { major, minor, .. } => {
                Some(MacOSVersion::new(u32::from(*major), u32::from(*minor)))
            }
            _ => None,
        })
        .min()
}

/// Check that a library's macOS version requirement is compatible with the wheel's platform tag.
fn check_macos_version_compatible(
    lib_path: &Path,
    wheel_version: MacOSVersion,
) -> Result<(), DelocateError> {
    let macho = macho::parse_macho(lib_path)?;

    if let Some(lib_version) = macho.min_macos_version {
        if lib_version > wheel_version {
            return Err(DelocateError::IncompatibleMacOSVersion {
                library: lib_path.to_path_buf(),
                library_version: lib_version.to_string(),
                wheel_version: wheel_version.to_string(),
            });
        }
    }

    Ok(())
}

/// Check that a library has all architectures required by its dependents.
fn check_dependency_archs(lib_path: &Path, required_archs: &[Arch]) -> Result<(), DelocateError> {
    if required_archs.is_empty() {
        return Ok(());
    }

    macho::check_archs(lib_path, required_archs)
}

/// Find the maximum macOS version required by any binary in the wheel or its dependencies.
fn find_max_macos_version(
    binaries: &[PathBuf],
    libraries: &HashMap<PathBuf, LibraryInfo>,
) -> Option<MacOSVersion> {
    let mut max_version: Option<MacOSVersion> = None;

    // Check wheel binaries.
    for binary in binaries {
        if let Ok(macho) = macho::parse_macho(binary) {
            if let Some(version) = macho.min_macos_version {
                max_version =
                    Some(max_version.map_or(version, |current| std::cmp::max(current, version)));
            }
        }
    }

    // Check external libraries.
    for lib_path in libraries.keys() {
        if let Ok(macho) = macho::parse_macho(lib_path) {
            if let Some(version) = macho.min_macos_version {
                max_version =
                    Some(max_version.map_or(version, |current| std::cmp::max(current, version)));
            }
        }
    }

    max_version
}

/// Update platform tags to reflect a new macOS version.
///
/// For example, `macosx_10_9_x86_64` with version 11.0 becomes `macosx_11_0_x86_64`.
/// Non-macOS tags are preserved unchanged.
fn update_platform_tags_version(
    platform_tags: &[PlatformTag],
    version: MacOSVersion,
) -> Vec<PlatformTag> {
    // Handle macOS 11+ where minor version is always 0 for tagging.
    let (major, minor) = if version.major >= 11 {
        (u16::try_from(version.major).unwrap_or(u16::MAX), 0)
    } else {
        (
            u16::try_from(version.major).unwrap_or(u16::MAX),
            u16::try_from(version.minor).unwrap_or(u16::MAX),
        )
    };

    platform_tags
        .iter()
        .map(|tag| match tag {
            PlatformTag::Macos { binary_format, .. } => PlatformTag::Macos {
                major,
                minor,
                binary_format: *binary_format,
            },
            other => other.clone(),
        })
        .collect()
}

/// Find all Mach-O binaries in a directory.
fn find_binaries(dir: &Path) -> Result<Vec<PathBuf>, DelocateError> {
    let mut binaries = Vec::new();

    for entry in WalkDir::new(dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();

        // Check extension.
        let ext = path.extension().and_then(|ext| ext.to_str());
        if !matches!(ext, Some("so" | "dylib")) {
            // Also check if it's a Mach-O without extension.
            if ext.is_some() {
                continue;
            }
        }

        if macho::is_macho_file(path)? {
            binaries.push(path.to_path_buf());
        }
    }

    Ok(binaries)
}

/// Analyze dependencies for all binaries in a directory.
fn analyze_dependencies(
    dir: &Path,
    binaries: &[PathBuf],
    exclude: &[String],
) -> Result<HashMap<PathBuf, LibraryInfo>, DelocateError> {
    let mut libraries: HashMap<PathBuf, LibraryInfo> = HashMap::new();
    let mut to_process: Vec<(PathBuf, PathBuf, String)> = Vec::new();

    // Initial pass: collect direct dependencies.
    for binary_path in binaries {
        let macho = macho::parse_macho(binary_path)?;

        for dep in &macho.dependencies {
            // Skip excluded libraries.
            if exclude.iter().any(|pat| dep.contains(pat)) {
                continue;
            }

            // Skip system libraries.
            if is_system_library(dep) {
                continue;
            }

            // Try to resolve the dependency.
            if let Some(resolved) = resolve_dynamic_path(dep, binary_path, &macho.rpaths) {
                // Skip if the resolved path is within our directory (already bundled).
                if resolved.starts_with(dir) {
                    continue;
                }

                to_process.push((resolved, binary_path.clone(), dep.clone()));
            }
        }
    }

    // Process dependencies and their transitive dependencies.
    let mut processed: HashSet<PathBuf> = HashSet::new();

    while let Some((lib_path, dependent_path, install_name)) = to_process.pop() {
        // Add to libraries map.
        libraries
            .entry(lib_path.clone())
            .or_insert_with(|| LibraryInfo {
                dependents: HashMap::new(),
            })
            .dependents
            .insert(dependent_path, install_name);

        // Process transitive dependencies.
        if processed.insert(lib_path.clone()) {
            if let Ok(macho) = macho::parse_macho(&lib_path) {
                for dep in &macho.dependencies {
                    if exclude.iter().any(|pat| dep.contains(pat)) {
                        continue;
                    }

                    if is_system_library(dep) {
                        continue;
                    }

                    if let Some(resolved) = resolve_dynamic_path(dep, &lib_path, &macho.rpaths) {
                        if resolved.starts_with(dir) {
                            continue;
                        }

                        to_process.push((resolved, lib_path.clone(), dep.clone()));
                    }
                }
            }
        }
    }

    Ok(libraries)
}

/// List dependencies of a wheel.
pub fn list_wheel_dependencies(
    wheel_path: &Path,
) -> Result<Vec<(String, Vec<PathBuf>)>, DelocateError> {
    let temp_dir = tempfile::tempdir()?;
    let wheel_dir = temp_dir.path();

    wheel::unpack_wheel(wheel_path, wheel_dir)?;

    let binaries = find_binaries(wheel_dir)?;
    let libraries = analyze_dependencies(wheel_dir, &binaries, &[])?;

    let mut deps: Vec<(String, Vec<PathBuf>)> = libraries
        .into_iter()
        .map(|(path, info)| {
            let dependents: Vec<PathBuf> = info
                .dependents
                .keys()
                .map(|dep_path| {
                    dep_path
                        .strip_prefix(wheel_dir)
                        .unwrap_or(dep_path)
                        .to_path_buf()
                })
                .collect();
            (path.to_string_lossy().into_owned(), dependents)
        })
        .collect();

    deps.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(deps)
}

/// Delocate a wheel: copy external libraries and update install names.
pub fn delocate_wheel(
    wheel_path: &Path,
    dest_dir: &Path,
    options: &DelocateOptions,
) -> Result<PathBuf, DelocateError> {
    debug!("Delocating wheel: {}", wheel_path.display());

    let filename_str = wheel_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| DelocateError::InvalidWheelPath {
            path: wheel_path.to_path_buf(),
        })?;
    let filename = WheelFilename::from_str(filename_str).map_err(|err| {
        DelocateError::InvalidWheelFilename {
            filename: filename_str.to_string(),
            err,
        }
    })?;

    let temp_dir = tempfile::tempdir()?;
    let wheel_dir = temp_dir.path();
    let platform_tags = filename.platform_tags();

    // Determine the target macOS version:
    // 1. Use explicitly set target_macos_version from options (includes MACOSX_DEPLOYMENT_TARGET).
    // 2. Fall back to parsing from wheel's platform tag.
    let wheel_platform_version = get_macos_version(platform_tags);
    let target_version = options.target_macos_version.or(wheel_platform_version);

    // Unpack the wheel.
    wheel::unpack_wheel(wheel_path, wheel_dir)?;

    // Find all binaries.
    let binaries = find_binaries(wheel_dir)?;

    // Check required architectures.
    if !options.require_archs.is_empty() {
        for binary in &binaries {
            macho::check_archs(binary, &options.require_archs)?;
        }
    }

    // Analyze dependencies.
    let libraries = analyze_dependencies(wheel_dir, &binaries, &options.exclude)?;

    // Validate dependencies before copying.
    for lib_path in libraries.keys() {
        // Check architecture compatibility.
        if !options.require_archs.is_empty() {
            check_dependency_archs(lib_path, &options.require_archs)?;
        }

        // Check macOS version compatibility against target version.
        if options.check_version_compatibility {
            if let Some(version) = target_version {
                check_macos_version_compatible(lib_path, version)?;
            }
        }
    }

    // Find the maximum macOS version required by all binaries.
    let max_required_version = find_max_macos_version(&binaries, &libraries);

    // Determine the final platform tags; update if binaries require higher version.
    let final_platform_tags = match (&wheel_platform_version, &max_required_version) {
        (Some(wheel_ver), Some(max_ver)) if max_ver > wheel_ver => {
            update_platform_tags_version(platform_tags, *max_ver)
        }
        _ => platform_tags.to_vec(),
    };

    if libraries.is_empty() {
        debug!("No external dependencies found");

        // No external dependencies, but still sanitize rpaths if requested.
        if options.sanitize_rpaths {
            for binary in &binaries {
                sanitize_rpaths(binary)?;
            }
            // Need to update RECORD after modifying binaries.
            let dist_info = wheel::find_dist_info(wheel_dir)?;
            wheel::update_record(wheel_dir, &dist_info)?;

            // Repack the wheel.
            let output_filename = filename.with_platform_tags(&final_platform_tags);
            let output_path = dest_dir.join(output_filename.to_string());
            wheel::pack_wheel(wheel_dir, &output_path)?;
            return Ok(output_path);
        }

        // No modifications needed, just copy the wheel.
        let dest_path = dest_dir.join(wheel_path.file_name().unwrap());
        fs::copy(wheel_path, &dest_path)?;
        return Ok(dest_path);
    }

    debug!("Found {} external libraries to bundle", libraries.len());

    // Find the package directory (first directory that's not .dist-info or .data)
    let package_dir = find_package_dir(wheel_dir, &filename)?;

    // Create the library directory.
    let lib_dir = package_dir.join(&options.lib_sdir);
    fs::create_dir_all(&lib_dir)?;

    // Check for library name collisions.
    let mut lib_names: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for path in libraries.keys() {
        let name = path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .unwrap_or("unknown")
            .to_string();
        lib_names.entry(name).or_default().push(path.clone());
    }

    for (name, paths) in &lib_names {
        if paths.len() > 1 {
            return Err(DelocateError::LibraryCollision {
                name: name.clone(),
                paths: paths.clone(),
            });
        }
    }

    // Copy libraries and update install names.
    for (lib_path, info) in &libraries {
        let lib_name = lib_path.file_name().unwrap();
        let dest_lib = lib_dir.join(lib_name);

        // Copy the library.
        trace!("Copying {} to {}", lib_path.display(), dest_lib.display());
        fs::copy(lib_path, &dest_lib)?;

        // Sanitize rpaths in the copied library.
        if options.sanitize_rpaths {
            sanitize_rpaths(&dest_lib)?;
        }

        // Set the install ID of the copied library.
        let new_id = format!(
            "@loader_path/{}/{}",
            options.lib_sdir,
            lib_name.to_string_lossy()
        );
        macho::change_install_id(&dest_lib, &new_id)?;

        // Update install names in dependents.
        for (dependent_path, old_install_name) in &info.dependents {
            // Calculate the relative path from dependent to library.
            let dependent_in_wheel = if dependent_path.starts_with(wheel_dir) {
                dependent_path.clone()
            } else {
                // This is a transitive dependency that was copied.
                lib_dir.join(dependent_path.file_name().unwrap())
            };

            let dependent_parent = dependent_in_wheel.parent().unwrap();
            let relative_to_package = pathdiff::diff_paths(&lib_dir, dependent_parent)
                .unwrap_or_else(|| PathBuf::from(&options.lib_sdir));

            let new_install_name = format!(
                "@loader_path/{}/{}",
                relative_to_package.to_string_lossy(),
                lib_name.to_string_lossy()
            );

            // Update the install name.
            if dependent_in_wheel.exists() {
                macho::change_install_name(
                    &dependent_in_wheel,
                    old_install_name,
                    &new_install_name,
                )?;
            }
        }
    }

    // Sanitize rpaths in original wheel binaries.
    if options.sanitize_rpaths {
        for binary in &binaries {
            sanitize_rpaths(binary)?;
        }
    }

    // Update RECORD.
    let dist_info = wheel::find_dist_info(wheel_dir)?;
    wheel::update_record(wheel_dir, &dist_info)?;

    // Create output wheel with potentially updated platform tag.
    let output_filename = filename.with_platform_tags(&final_platform_tags);
    let output_path = dest_dir.join(output_filename.to_string());

    wheel::pack_wheel(wheel_dir, &output_path)?;

    Ok(output_path)
}

/// Find the main package directory in a wheel.
fn find_package_dir(wheel_dir: &Path, filename: &WheelFilename) -> Result<PathBuf, DelocateError> {
    // Look for a directory that matches the package name.
    let dist_info_name = filename.name.as_dist_info_name();

    for entry in fs::read_dir(wheel_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip `.dist-info` and `.data` directories.
        if name_str.ends_with(".dist-info") || name_str.ends_with(".data") {
            continue;
        }

        // Check if it matches normalized name.
        if name_str == *dist_info_name || name_str.replace('-', "_") == *dist_info_name {
            return Ok(entry.path());
        }
    }

    // If no matching directory, use the wheel directory itself.
    Ok(wheel_dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uv_platform_tags::BinaryFormat;

    #[test]
    fn test_is_system_library() {
        assert!(is_system_library("/usr/lib/libSystem.B.dylib"));
        assert!(is_system_library(
            "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation"
        ));
        assert!(!is_system_library("/usr/local/lib/libfoo.dylib"));
        assert!(!is_system_library("/opt/homebrew/lib/libbar.dylib"));
    }

    #[test]
    fn test_get_macos_version() {
        // Standard format.
        let tags = [PlatformTag::Macos {
            major: 10,
            minor: 9,
            binary_format: BinaryFormat::X86_64,
        }];
        let version = get_macos_version(&tags).unwrap();
        assert_eq!(version.major, 10);
        assert_eq!(version.minor, 9);

        // macOS 11+.
        let tags = [PlatformTag::Macos {
            major: 11,
            minor: 0,
            binary_format: BinaryFormat::Arm64,
        }];
        let version = get_macos_version(&tags).unwrap();
        assert_eq!(version.major, 11);
        assert_eq!(version.minor, 0);

        // Universal binary.
        let tags = [PlatformTag::Macos {
            major: 10,
            minor: 9,
            binary_format: BinaryFormat::Universal2,
        }];
        let version = get_macos_version(&tags).unwrap();
        assert_eq!(version.major, 10);
        assert_eq!(version.minor, 9);

        // Multiple tags; returns minimum version.
        let tags = [
            PlatformTag::Macos {
                major: 11,
                minor: 0,
                binary_format: BinaryFormat::Arm64,
            },
            PlatformTag::Macos {
                major: 10,
                minor: 9,
                binary_format: BinaryFormat::X86_64,
            },
        ];
        let version = get_macos_version(&tags).unwrap();
        assert_eq!(version.major, 10);
        assert_eq!(version.minor, 9);

        // Not a macOS platform.
        let tags = [PlatformTag::Linux {
            arch: uv_platform_tags::Arch::X86_64,
        }];
        assert!(get_macos_version(&tags).is_none());

        let tags = [PlatformTag::WinAmd64];
        assert!(get_macos_version(&tags).is_none());
    }

    #[test]
    fn test_parse_macos_version() {
        let version = MacOSVersion::parse("10.9").unwrap();
        assert_eq!(version.major, 10);
        assert_eq!(version.minor, 9);

        let version = MacOSVersion::parse("11.0").unwrap();
        assert_eq!(version.major, 11);
        assert_eq!(version.minor, 0);

        // Patch is ignored.
        let version = MacOSVersion::parse("14.2.1").unwrap();
        assert_eq!(version.major, 14);
        assert_eq!(version.minor, 2);

        let version = MacOSVersion::parse("15").unwrap();
        assert_eq!(version.major, 15);
        assert_eq!(version.minor, 0);
    }

    #[test]
    fn test_update_platform_tags_version() {
        // Upgrade from 10.9 to 11.0.
        let tags = [PlatformTag::Macos {
            major: 10,
            minor: 9,
            binary_format: BinaryFormat::X86_64,
        }];
        let updated = update_platform_tags_version(&tags, MacOSVersion::new(11, 0));
        assert_eq!(
            updated,
            vec![PlatformTag::Macos {
                major: 11,
                minor: 0,
                binary_format: BinaryFormat::X86_64,
            }]
        );

        // Upgrade from 10.9 to 10.15.
        let updated = update_platform_tags_version(&tags, MacOSVersion::new(10, 15));
        assert_eq!(
            updated,
            vec![PlatformTag::Macos {
                major: 10,
                minor: 15,
                binary_format: BinaryFormat::X86_64,
            }]
        );

        // Universal2.
        let tags = [PlatformTag::Macos {
            major: 10,
            minor: 9,
            binary_format: BinaryFormat::Universal2,
        }];
        let updated = update_platform_tags_version(&tags, MacOSVersion::new(11, 0));
        assert_eq!(
            updated,
            vec![PlatformTag::Macos {
                major: 11,
                minor: 0,
                binary_format: BinaryFormat::Universal2,
            }]
        );

        // macOS 11+ always has minor=0 for tagging.
        let tags = [PlatformTag::Macos {
            major: 11,
            minor: 0,
            binary_format: BinaryFormat::Arm64,
        }];
        let updated = update_platform_tags_version(&tags, MacOSVersion::new(14, 2));
        assert_eq!(
            updated,
            vec![PlatformTag::Macos {
                major: 14,
                minor: 0,
                binary_format: BinaryFormat::Arm64,
            }]
        );

        // Non-macOS tag unchanged.
        let tags = [PlatformTag::Linux {
            arch: uv_platform_tags::Arch::X86_64,
        }];
        let updated = update_platform_tags_version(&tags, MacOSVersion::new(11, 0));
        assert_eq!(
            updated,
            vec![PlatformTag::Linux {
                arch: uv_platform_tags::Arch::X86_64
            }]
        );
    }
}
