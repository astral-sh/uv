//! Delocate operations for macOS wheels.

use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use fs_err as fs;
use tracing::{debug, trace};
use walkdir::WalkDir;

use uv_distribution_filename::WheelFilename;
use uv_fs::relative_to;
use uv_platform::MacOSVersion;
use uv_platform_tags::PlatformTag;
use uv_static::EnvVars;

use crate::error::DelocateError;
use crate::macho::{self, Arch};
use crate::wheel;

/// Options for delocating a wheel.
#[derive(Debug, Clone)]
pub struct DelocateOptions {
    /// Subdirectory within the package to store copied libraries.
    /// Defaults to ".dylibs".
    pub lib_subdir: String,
    /// Required architectures to validate.
    pub require_archs: Vec<Arch>,
    /// Libraries to exclude from delocating (by name pattern).
    pub exclude: Vec<String>,
}

impl Default for DelocateOptions {
    fn default() -> Self {
        Self {
            lib_subdir: ".dylibs".to_string(),
            require_archs: Vec::new(),
            exclude: Vec::new(),
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

/// Collect library search paths from DYLD environment variables.
fn collect_dyld_paths() -> Vec<PathBuf> {
    const DEFAULT_FALLBACK_PATHS: &[&str] = &["/usr/local/lib", "/usr/lib"];

    let mut paths = Vec::new();

    // DYLD_LIBRARY_PATH first.
    if let Ok(env_paths) = env::var(EnvVars::DYLD_LIBRARY_PATH) {
        paths.extend(env_paths.split(':').map(PathBuf::from));
    }

    // Then DYLD_FALLBACK_LIBRARY_PATH.
    if let Ok(env_paths) = env::var(EnvVars::DYLD_FALLBACK_LIBRARY_PATH) {
        paths.extend(env_paths.split(':').map(PathBuf::from));
    }

    // Default fallback paths.
    paths.extend(DEFAULT_FALLBACK_PATHS.iter().map(PathBuf::from));

    paths
}

/// Search for a library in the given search paths.
fn search_library_paths(lib_name: &str, search_paths: &[PathBuf]) -> Option<PathBuf> {
    for dir in search_paths {
        let candidate = dir.join(lib_name);
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
    dyld_paths: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(relative) = install_name
        .strip_prefix("@loader_path/")
        .or_else(|| install_name.strip_prefix("@executable_path/"))
    {
        // @loader_path and @executable_path are relative to the binary containing the reference.
        let parent = binary_path.parent()?;
        let resolved = parent.join(relative);
        if let Ok(path) = resolved.canonicalize() {
            return Some(path);
        }
    } else if let Some(relative) = install_name.strip_prefix("@rpath/") {
        // @rpath; search through rpaths.
        for rpath in rpaths {
            // Resolve rpath itself if it contains tokens.
            let resolved_rpath = if let Some(rpath_relative) = rpath
                .strip_prefix("@loader_path/")
                .or_else(|| rpath.strip_prefix("@executable_path/"))
            {
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
        if !install_name.contains('/') {
            if let Some(resolved) = search_library_paths(install_name, dyld_paths) {
                return Some(resolved);
            }
        }
    }

    None
}

/// Information about an external library to bundle.
#[derive(Debug)]
struct LibraryInfo {
    /// Wheel binaries that depend on this library, mapped to the install name they use to reference it.
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
            PlatformTag::Macos { major, minor, .. } => Some(MacOSVersion::new(*major, *minor)),
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

/// Find the maximum macOS version required by any of the given binaries.
fn find_max_macos_version<'a>(paths: impl Iterator<Item = &'a Path>) -> Option<MacOSVersion> {
    let mut max_version: Option<MacOSVersion> = None;

    for path in paths {
        if let Ok(macho) = macho::parse_macho(path) {
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
        (version.major, 0)
    } else {
        (version.major, version.minor)
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
    let dyld_paths = collect_dyld_paths();

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
            if let Some(resolved) =
                resolve_dynamic_path(dep, binary_path, &macho.rpaths, &dyld_paths)
            {
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
            let macho = macho::parse_macho(&lib_path)?;
            for dep in &macho.dependencies {
                if exclude.iter().any(|pat| dep.contains(pat)) {
                    continue;
                }

                if is_system_library(dep) {
                    continue;
                }

                if let Some(resolved) =
                    resolve_dynamic_path(dep, &lib_path, &macho.rpaths, &dyld_paths)
                {
                    if resolved.starts_with(dir) {
                        continue;
                    }

                    debug!(
                        "Found transitive dependency: {} -> {}",
                        lib_path.display(),
                        resolved.display()
                    );
                    to_process.push((resolved, lib_path.clone(), dep.clone()));
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
    let filename = WheelFilename::from_str(filename_str)?;

    let temp_dir = tempfile::tempdir()?;
    let wheel_dir = temp_dir.path();
    let platform_tags = filename.platform_tags();

    // Determine the target macOS version:
    // 1. Use `MACOSX_DEPLOYMENT_TARGET` environment variable if set.
    // 2. Fall back to parsing from wheel's platform tag.
    let wheel_platform_version = get_macos_version(platform_tags);
    let target_version = MacOSVersion::from_env().or(wheel_platform_version);

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
        if let Some(version) = target_version {
            check_macos_version_compatible(lib_path, version)?;
        }
    }

    // Find the maximum macOS version required by all binaries.
    let max_required_version = find_max_macos_version(
        binaries
            .iter()
            .map(PathBuf::as_path)
            .chain(libraries.keys().map(PathBuf::as_path)),
    );

    // Determine the final platform tags; update if binaries require higher version.
    let final_platform_tags = match (&wheel_platform_version, &max_required_version) {
        (Some(wheel_ver), Some(max_ver)) if max_ver > wheel_ver => {
            update_platform_tags_version(platform_tags, *max_ver)
        }
        _ => platform_tags.to_vec(),
    };

    // No external dependencies to bundle.
    if libraries.is_empty() {
        debug!("No external dependencies found");
    }

    // Bundle external libraries into the wheel.
    if !libraries.is_empty() {
        debug!("Found {} external libraries to bundle", libraries.len());

        // Create the library directory following delocate's placement logic.
        let lib_dir = find_lib_dir(wheel_dir, &filename, &options.lib_subdir)?;
        fs::create_dir_all(&lib_dir)?;

        // Check for library name collisions.
        let mut lib_names: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for path in libraries.keys() {
            let file_name =
                path.file_name()
                    .ok_or_else(|| DelocateError::InvalidLibraryFilename {
                        path: path.clone(),
                        reason: "path has no filename",
                    })?;
            let name = file_name
                .to_str()
                .ok_or_else(|| DelocateError::InvalidLibraryFilename {
                    path: path.clone(),
                    reason: "filename is not valid UTF-8",
                })?
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
            sanitize_rpaths(&dest_lib)?;

            // Set the install ID of the copied library.
            let new_id = format!(
                "@loader_path/{}/{}",
                options.lib_subdir,
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
                let relative_to_package = relative_to(&lib_dir, dependent_parent)?;

                let new_install_name = format!(
                    "@loader_path/{}/{}",
                    relative_to_package.to_string_lossy(),
                    lib_name.to_string_lossy()
                );

                // Update the install name in the dependent binary. Skip if the dependent
                // doesn't exist yet (e.g., a transitive dependency that will be copied
                // in a later iteration).
                if dependent_in_wheel.exists() {
                    macho::change_install_name(
                        &dependent_in_wheel,
                        old_install_name,
                        &new_install_name,
                    )?;
                }
            }
        }
    }

    // Sanitize rpaths in original wheel binaries.
    for binary in &binaries {
        sanitize_rpaths(binary)?;
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

/// Find the directory to place bundled libraries.
///
/// Follows Python delocate's placement logic:
/// 1. If a package directory matches the package name, use `package/.dylibs/`.
/// 2. If packages exist but none match, use the first package alphabetically.
/// 3. If no packages exist, use `{package_name}.dylibs/` at wheel root.
fn find_lib_dir(
    wheel_dir: &Path,
    filename: &WheelFilename,
    lib_subdir: &str,
) -> Result<PathBuf, DelocateError> {
    let dist_info_name = filename.name.as_dist_info_name();

    let mut first_package: Option<PathBuf> = None;

    for entry in fs::read_dir(wheel_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip special directories.
        if name_str.ends_with(".dist-info") || name_str.ends_with(".data") || name_str == lib_subdir
        {
            continue;
        }

        let path = entry.path();

        // 1. Use matching package if found.
        if name_str == *dist_info_name || name_str.replace('-', "_") == *dist_info_name {
            return Ok(path.join(lib_subdir));
        }

        // Track first package alphabetically.
        if first_package.as_ref().is_none_or(|p| path < *p) {
            first_package = Some(path);
        }
    }

    // 2. Use first package alphabetically if any exist.
    if let Some(pkg) = first_package {
        return Ok(pkg.join(lib_subdir));
    }

    // 3. No packages; use {package_name}.dylibs at wheel root.
    Ok(wheel_dir.join(format!(
        "{}.{}",
        dist_info_name,
        lib_subdir.trim_start_matches('.')
    )))
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

    #[test]
    fn test_resolve_dynamic_path_bare_vs_relative() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let lib_dir = temp_dir.path().join("lib");
        fs::create_dir_all(&lib_dir).unwrap();

        // Create a dummy library file.
        let lib_path = lib_dir.join("libtest.dylib");
        fs::write(&lib_path, b"dummy").unwrap();

        // Create a binary path (doesn't need to exist for this test).
        let binary_path = temp_dir.path().join("bin").join("test");

        // Use the lib directory as a search path.
        let dyld_paths = vec![lib_dir];

        // Bare library name should be found via DYLD paths.
        let result = resolve_dynamic_path("libtest.dylib", &binary_path, &[], &dyld_paths);
        assert!(
            result.is_some(),
            "bare library name should resolve via DYLD paths"
        );

        // Relative path with `/` should NOT search DYLD paths.
        // (It would try relative to binary, which doesn't exist.)
        let result = resolve_dynamic_path("../lib/libtest.dylib", &binary_path, &[], &dyld_paths);
        assert!(
            result.is_none(),
            "relative path should not search DYLD paths"
        );
    }

    #[test]
    fn test_analyze_transitive_dependencies() {
        use tempfile::TempDir;

        let test_data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data");
        let temp_dir = TempDir::new().unwrap();
        let wheel_dir = temp_dir.path().join("wheel");
        let lib_dir = temp_dir.path().join("libs");
        fs::create_dir_all(&wheel_dir).unwrap();
        fs::create_dir_all(&lib_dir).unwrap();

        // Copy libc.dylib into the "wheel" as a binary (it depends on liba.dylib and libb.dylib).
        // Copy liba.dylib and libb.dylib to the lib dir (libb depends on liba).
        fs::copy(test_data.join("libc.dylib"), wheel_dir.join("libc.dylib")).unwrap();
        fs::copy(test_data.join("liba.dylib"), lib_dir.join("liba.dylib")).unwrap();
        fs::copy(test_data.join("libb.dylib"), lib_dir.join("libb.dylib")).unwrap();

        // Verify the test data relationships.
        let libc_macho = macho::parse_macho(&wheel_dir.join("libc.dylib")).unwrap();
        assert!(libc_macho.dependencies.iter().any(|d| d.contains("liba")));
        assert!(libc_macho.dependencies.iter().any(|d| d.contains("libb")));

        let libb_macho = macho::parse_macho(&lib_dir.join("libb.dylib")).unwrap();
        assert!(
            libb_macho.dependencies.iter().any(|d| d.contains("liba")),
            "libb.dylib should depend on liba.dylib for transitive test"
        );

        // The dependencies use bare names, which need DYLD paths to resolve.
        // We can't easily set env vars in tests, so we test the resolve_dynamic_path
        // function directly with explicit DYLD paths.
        let dyld_paths = vec![lib_dir.clone()];
        let binary_path = wheel_dir.join("libc.dylib");

        // Resolve libc's direct dependencies.
        let mut direct_deps = Vec::new();
        for dep in &libc_macho.dependencies {
            if is_system_library(dep) {
                continue;
            }
            if let Some(resolved) =
                resolve_dynamic_path(dep, &binary_path, &libc_macho.rpaths, &dyld_paths)
            {
                direct_deps.push(resolved);
            }
        }

        // Should find both liba and libb.
        assert!(
            direct_deps.iter().any(|p| p.ends_with("liba.dylib")),
            "should resolve liba.dylib"
        );
        assert!(
            direct_deps.iter().any(|p| p.ends_with("libb.dylib")),
            "should resolve libb.dylib"
        );

        // Now resolve libb's transitive dependencies.
        let libb_path = direct_deps
            .iter()
            .find(|p| p.ends_with("libb.dylib"))
            .unwrap();
        let libb_macho = macho::parse_macho(libb_path).unwrap();

        let mut transitive_deps = Vec::new();
        for dep in &libb_macho.dependencies {
            if is_system_library(dep) {
                continue;
            }
            if let Some(resolved) =
                resolve_dynamic_path(dep, libb_path, &libb_macho.rpaths, &dyld_paths)
            {
                transitive_deps.push(resolved);
            }
        }

        // libb should transitively depend on liba.
        assert!(
            transitive_deps.iter().any(|p| p.ends_with("liba.dylib")),
            "libb.dylib should have liba.dylib as a transitive dependency"
        );
    }

    #[test]
    fn test_find_lib_dir_matching_package() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let wheel_dir = temp_dir.path();

        // Create a package directory matching the package name.
        fs::create_dir(wheel_dir.join("my_package")).unwrap();
        fs::create_dir(wheel_dir.join("my_package-1.0.dist-info")).unwrap();

        let filename = WheelFilename::from_str("my_package-1.0-py3-none-any.whl").unwrap();
        let lib_dir = find_lib_dir(wheel_dir, &filename, ".dylibs").unwrap();

        assert_eq!(lib_dir, wheel_dir.join("my_package").join(".dylibs"));
    }

    #[test]
    fn test_find_lib_dir_first_package_alphabetically() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let wheel_dir = temp_dir.path();

        // Create package directories that don't match the package name.
        fs::create_dir(wheel_dir.join("zebra")).unwrap();
        fs::create_dir(wheel_dir.join("alpha")).unwrap();
        fs::create_dir(wheel_dir.join("beta")).unwrap();
        fs::create_dir(wheel_dir.join("my_package-1.0.dist-info")).unwrap();

        let filename = WheelFilename::from_str("my_package-1.0-py3-none-any.whl").unwrap();
        let lib_dir = find_lib_dir(wheel_dir, &filename, ".dylibs").unwrap();

        // Should use "alpha" as it's first alphabetically.
        assert_eq!(lib_dir, wheel_dir.join("alpha").join(".dylibs"));
    }

    #[test]
    fn test_find_lib_dir_no_packages() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let wheel_dir = temp_dir.path();

        // Only create dist-info, no package directories.
        fs::create_dir(wheel_dir.join("my_package-1.0.dist-info")).unwrap();
        // Create a standalone module file (not a directory).
        fs::write(wheel_dir.join("my_module.py"), b"# module").unwrap();

        let filename = WheelFilename::from_str("my_package-1.0-py3-none-any.whl").unwrap();
        let lib_dir = find_lib_dir(wheel_dir, &filename, ".dylibs").unwrap();

        // Should use {package_name}.dylibs at wheel root.
        assert_eq!(lib_dir, wheel_dir.join("my_package.dylibs"));
    }

    #[test]
    fn test_find_lib_dir_skips_existing_dylibs() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let wheel_dir = temp_dir.path();

        // Create package and an existing .dylibs directory.
        fs::create_dir(wheel_dir.join("my_package")).unwrap();
        fs::create_dir(wheel_dir.join(".dylibs")).unwrap();
        fs::create_dir(wheel_dir.join("my_package-1.0.dist-info")).unwrap();

        let filename = WheelFilename::from_str("my_package-1.0-py3-none-any.whl").unwrap();
        let lib_dir = find_lib_dir(wheel_dir, &filename, ".dylibs").unwrap();

        // Should use the package, not the existing .dylibs.
        assert_eq!(lib_dir, wheel_dir.join("my_package").join(".dylibs"));
    }

    #[test]
    fn test_find_lib_dir_skips_data_directory() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let wheel_dir = temp_dir.path();

        // Create .data directory and a regular package.
        fs::create_dir(wheel_dir.join("my_package-1.0.data")).unwrap();
        fs::create_dir(wheel_dir.join("actual_package")).unwrap();
        fs::create_dir(wheel_dir.join("my_package-1.0.dist-info")).unwrap();

        let filename = WheelFilename::from_str("my_package-1.0-py3-none-any.whl").unwrap();
        let lib_dir = find_lib_dir(wheel_dir, &filename, ".dylibs").unwrap();

        // Should use actual_package, not the .data directory.
        assert_eq!(lib_dir, wheel_dir.join("actual_package").join(".dylibs"));
    }
}
