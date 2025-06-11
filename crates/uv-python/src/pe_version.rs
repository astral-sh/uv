//! Extract version information from Windows PE executables.
use std::path::Path;

#[cfg(target_os = "windows")]
use std::str::FromStr;

#[cfg(target_os = "windows")]
use thiserror::Error;

#[cfg(target_os = "windows")]
use tracing::{debug, trace};

#[cfg(target_os = "windows")]
use crate::PythonVersion;

#[cfg(target_os = "windows")]
#[derive(Debug, Error)]
pub(crate) enum PeVersionError {
    #[error("Failed to read PE file: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to parse PE file: {0}")]
    InvalidPeFormat(#[from] pelite::Error),

    #[error("No version info found in PE file")]
    NoVersionInfo,

    #[error("Failed to parse version: {0}")]
    InvalidVersion(String),

    #[error("Version component too large: {0}")]
    VersionComponentTooLarge(#[from] std::num::TryFromIntError),
}

/// Try to extract Python version information from a Windows PE executable.
///
/// On error, return [`None`].
/// On non-Windows platforms, this function always returns [`None`].
#[cfg(not(target_os = "windows"))]
pub(crate) fn try_extract_version_from_pe(_path: &Path) -> Option<crate::PythonVersion> {
    None
}

/// Try to extract Python version information from a Windows PE executable.
///
/// On error, return [`None`].
#[cfg(target_os = "windows")]
pub(crate) fn try_extract_version_from_pe(path: &Path) -> Option<PythonVersion> {
    match extract_version_from_pe(path) {
        Ok(version) => Some(version),
        Err(err) => {
            debug!(
                "Failed to extract version from PE file `{}`: {}",
                path.display(),
                err
            );
            None
        }
    }
}

/// Extract Python version information from a Windows PE executable.
#[cfg(target_os = "windows")]
fn extract_version_from_pe(path: &Path) -> Result<PythonVersion, PeVersionError> {
    use pelite::FileMap;
    trace!("Extracting version info from PE file: {}", path.display());

    // Read the PE file
    let map = FileMap::open(path)?;

    // Parse as PE64 first, then fall back to PE32
    let resources = read_pe64_resources(&map).or_else(|_| read_pe32_resources(&map))?;

    // Try to get version info
    let version_info = resources
        .version_info()
        .map_err(|_| PeVersionError::NoVersionInfo)?;

    // Get the fixed file info which contains version numbers
    let fixed_info = version_info.fixed().ok_or(PeVersionError::NoVersionInfo)?;

    // Extract version from the file version field
    let file_version = fixed_info.dwFileVersion;
    let major = u8::try_from(file_version.Major)?;
    let minor = u8::try_from(file_version.Minor)?;
    let patch = u8::try_from(file_version.Patch)?;

    PythonVersion::from_str(&format!("{major}.{minor}.{patch}"))
        .map_err(PeVersionError::InvalidVersion)
}

#[cfg(target_os = "windows")]
fn read_pe64_resources(
    map: &pelite::FileMap,
) -> Result<pelite::resources::Resources, PeVersionError> {
    use pelite::pe64::{Pe, PeFile};

    let pe = PeFile::from_bytes(map)?;
    Ok(pe.resources()?)
}

#[cfg(target_os = "windows")]
fn read_pe32_resources(
    map: &pelite::FileMap,
) -> Result<pelite::resources::Resources, PeVersionError> {
    use pelite::pe32::{Pe, PeFile};

    let pe = PeFile::from_bytes(map)?;
    Ok(pe.resources()?)
}
