//! Extract version information from Windows PE executables.
//!
//! This module provides functionality to extract Python version information
//! directly from Windows PE executable files using the pelite crate, which
//! can be faster than executing the Python interpreter to query its version.

use std::path::Path;

#[cfg(target_os = "windows")]
use std::str::FromStr;

#[cfg(windows)]
use tracing::debug;

#[cfg(target_os = "windows")]
use pelite::{pe32::Pe as Pe32, pe64::Pe as Pe64};

use crate::PythonVersion;

/// Extract Python version information from a Windows PE executable.
///
/// This function reads the PE file's version resource to extract version
/// information without executing the Python interpreter. This can be
/// significantly faster for version discovery.
///
/// # Arguments
///
/// * `path` - Path to the Python executable
///
/// # Returns
///
/// Returns `Ok(Some(PythonVersion))` if version information was successfully
/// extracted, `Ok(None)` if no version information was found, or an error
/// if the file could not be read or parsed.
#[cfg(target_os = "windows")]
pub fn extract_version_from_pe(path: &Path) -> Result<Option<PythonVersion>, std::io::Error> {
    use pelite::FileMap;

    debug!("Extracting version info from PE file: {}", path.display());

    // Read the PE file
    let map = FileMap::open(path)?;

    // Parse as PE64 first, fall back to PE32 if needed
    match parse_pe64_version(&map) {
        Ok(version) => Ok(version),
        Err(_) => parse_pe32_version(&map),
    }
}

#[cfg(target_os = "windows")]
fn parse_pe64_version(map: &pelite::FileMap) -> Result<Option<PythonVersion>, std::io::Error> {
    use pelite::pe64::PeFile;

    let pe = PeFile::from_bytes(map).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to parse PE64 file: {e}"),
        )
    })?;

    extract_version_from_pe64_file(&pe)
}

#[cfg(target_os = "windows")]
fn parse_pe32_version(map: &pelite::FileMap) -> Result<Option<PythonVersion>, std::io::Error> {
    use pelite::pe32::PeFile;

    let pe = PeFile::from_bytes(map).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to parse PE32 file: {e}"),
        )
    })?;

    extract_version_from_pe32_file(&pe)
}

#[cfg(target_os = "windows")]
fn extract_version_from_pe64_file(
    pe: &pelite::pe64::PeFile,
) -> Result<Option<PythonVersion>, std::io::Error> {
    // Get resources from the PE file
    let resources = pe.resources().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to read PE resources: {e}"),
        )
    })?;

    // Try to get version info
    let Ok(version_info) = resources.version_info() else {
        debug!("No version info found in PE file");
        return Ok(None);
    };

    // Get the fixed file info which contains version numbers
    let Some(fixed_info) = version_info.fixed() else {
        debug!("No fixed version info found in PE file");
        return Ok(None);
    };

    // Extract version from the file version field
    let file_version = fixed_info.dwFileVersion;
    #[allow(clippy::cast_possible_truncation)]
    let major = file_version.Major as u8;
    #[allow(clippy::cast_possible_truncation)]
    let minor = file_version.Minor as u8;
    #[allow(clippy::cast_possible_truncation)]
    let patch = file_version.Patch as u8;

    // Validate that this looks like a Python version
    if major == 0 || major > 10 || minor > 50 {
        debug!(
            "Version {}.{}.{} doesn't look like a Python version",
            major, minor, patch
        );
        return Ok(None);
    }

    debug!("Extracted Python version: {}.{}.{}", major, minor, patch);

    match PythonVersion::from_str(&format!("{major}.{minor}.{patch}")) {
        Ok(version) => Ok(Some(version)),
        Err(e) => {
            debug!(
                "Failed to parse version {}.{}.{}: {}",
                major, minor, patch, e
            );
            Ok(None)
        }
    }
}

#[cfg(target_os = "windows")]
fn extract_version_from_pe32_file(
    pe: &pelite::pe32::PeFile,
) -> Result<Option<PythonVersion>, std::io::Error> {
    // Get resources from the PE file
    let resources = pe.resources().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to read PE resources: {e}"),
        )
    })?;

    // Try to get version info
    let Ok(version_info) = resources.version_info() else {
        debug!("No version info found in PE file");
        return Ok(None);
    };

    // Get the fixed file info which contains version numbers
    let Some(fixed_info) = version_info.fixed() else {
        debug!("No fixed version info found in PE file");
        return Ok(None);
    };

    // Extract version from the file version field
    let file_version = fixed_info.dwFileVersion;
    #[allow(clippy::cast_possible_truncation)]
    let major = file_version.Major as u8;
    #[allow(clippy::cast_possible_truncation)]
    let minor = file_version.Minor as u8;
    #[allow(clippy::cast_possible_truncation)]
    let patch = file_version.Patch as u8;

    // Validate that this looks like a Python version
    if major == 0 || major > 10 || minor > 50 {
        debug!(
            "Version {}.{}.{} doesn't look like a Python version",
            major, minor, patch
        );
        return Ok(None);
    }

    debug!("Extracted Python version: {}.{}.{}", major, minor, patch);

    match PythonVersion::from_str(&format!("{major}.{minor}.{patch}")) {
        Ok(version) => Ok(Some(version)),
        Err(e) => {
            debug!(
                "Failed to parse version {}.{}.{}: {}",
                major, minor, patch, e
            );
            Ok(None)
        }
    }
}

/// Extract version information from a Windows PE executable.
///
/// On non-Windows platforms, this function always returns `Ok(None)`.
#[cfg(not(target_os = "windows"))]
pub fn extract_version_from_pe(_path: &Path) -> Result<Option<PythonVersion>, std::io::Error> {
    Ok(None)
}

#[test]
fn test_basic_pe_version_functionality() {
    use std::str::FromStr;

    // Basic test for the non-Windows version
    #[cfg(not(target_os = "windows"))]
    {
        let result = extract_version_from_pe(Path::new("test.exe"));
        assert_eq!(result.unwrap(), None);
    }

    // Test PythonVersion parsing
    assert!(PythonVersion::from_str("3.12.0").is_ok());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[cfg(target_os = "windows")]
    use std::io::Write;
    #[cfg(target_os = "windows")]
    use tempfile::NamedTempFile;

    #[test]
    #[cfg(target_os = "windows")]
    fn test_extract_version_from_nonexistent_file() {
        let result = extract_version_from_pe(Path::new("nonexistent.exe"));
        assert!(result.is_err());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_extract_version_from_invalid_pe_file() {
        // Create a temporary file with invalid PE content
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Not a PE file").unwrap();
        temp_file.flush().unwrap();

        let result = extract_version_from_pe(temp_file.path());
        assert!(result.is_err());
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_extract_version_non_windows() {
        let result = extract_version_from_pe(Path::new("python.exe"));
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_version_validation() {
        // Test that valid Python versions work
        assert!(PythonVersion::from_str("3.12.0").is_ok());
        assert!(PythonVersion::from_str("3.9.18").is_ok());
        assert!(PythonVersion::from_str("3.13.1").is_ok());

        // Test some edge cases that should still work
        assert!(PythonVersion::from_str("0.1.0").is_ok()); // PythonVersion allows this

        // Test malformed versions
        assert!(PythonVersion::from_str("not.a.version").is_err());
        assert!(PythonVersion::from_str("").is_err());
    }

    #[test]
    fn test_always_runs() {
        // This test should always run regardless of platform
        // Test that the non-Windows version works
        let result = extract_version_from_pe(Path::new("fake.exe"));
        #[cfg(not(target_os = "windows"))]
        assert_eq!(result.unwrap(), None);
        #[cfg(target_os = "windows")]
        assert!(result.is_err() || result.unwrap().is_none());
    }
}
