//! Extract version information from Windows PE executables.
use std::path::Path;

#[cfg(target_os = "windows")]
use thiserror::Error;

#[cfg(target_os = "windows")]
use tracing::{debug, trace};

#[cfg(target_os = "windows")]
use crate::PythonVersion;

#[cfg(target_os = "windows")]
use uv_pep440::{Prerelease, PrereleaseKind, Version};

#[cfg(target_os = "windows")]
use uv_pep508::StringVersion;

#[cfg(target_os = "windows")]
#[derive(Debug, Error)]
pub(crate) enum PeVersionError {
    #[error("Failed to read PE file: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to parse PE file: {0}")]
    InvalidPeFormat(#[from] pelite::Error),

    #[error("No version info found in PE file")]
    NoVersionInfo,

    #[error("Failed to parse release level: {0}")]
    InvalidReleaseLevel(u16),

    #[error("Version component too large: {0}")]
    VersionComponentTooLarge(#[from] std::num::TryFromIntError),
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
    let major = file_version.Major;
    let minor = file_version.Minor;

    // https://github.com/python/cpython/blob/6fcac09401e336b25833dcef2610d498e73b27a1/PC/python_exe.rc#L24
    // FILEVERSION PYVERSION64

    // https://github.com/python/cpython/blob/6fcac09401e336b25833dcef2610d498e73b27a1/PC/python_ver_rc.h#L34
    // #define PYVERSION64 PY_MAJOR_VERSION, PY_MINOR_VERSION, FIELD3, PYTHON_API_VERSION

    // https://github.com/python/cpython/blob/6fcac09401e336b25833dcef2610d498e73b27a1/PCbuild/field3.py#L31-L35
    // field3 = micro * 1000 + levelnum * 10 + serial

    let field3 = file_version.Patch;
    let patch = field3 / 1000;
    let levelnum = field3 % 1000 / 10;
    let serial = field3 % 10;

    // https://github.com/python/cpython/blob/96b7a2eba423b42320f15fd4974740e3e930bb8b/Include/patchlevel.h#L11-L16
    let prerelease = match levelnum {
        0xA => Some(PrereleaseKind::Alpha),
        0xB => Some(PrereleaseKind::Beta),
        0xC => Some(PrereleaseKind::Rc),
        0xF => None,
        _ => {
            return Err(PeVersionError::InvalidReleaseLevel(levelnum));
        }
    }
    .map(|kind| Prerelease {
        kind,
        number: u64::from(serial),
    });

    let version =
        Version::new([u64::from(major), u64::from(minor), u64::from(patch)]).with_pre(prerelease);

    Ok(PythonVersion::from(StringVersion::from(version)))
}

#[cfg(target_os = "windows")]
fn read_pe64_resources(
    map: &'_ pelite::FileMap,
) -> Result<pelite::resources::Resources<'_>, PeVersionError> {
    use pelite::pe64::{Pe, PeFile};

    let pe = PeFile::from_bytes(map)?;
    Ok(pe.resources()?)
}

#[cfg(target_os = "windows")]
fn read_pe32_resources(
    map: &'_ pelite::FileMap,
) -> Result<pelite::resources::Resources<'_>, PeVersionError> {
    use pelite::pe32::{Pe, PeFile};

    let pe = PeFile::from_bytes(map)?;
    Ok(pe.resources()?)
}
