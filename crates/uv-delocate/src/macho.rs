//! Mach-O binary parsing and modification.
//!
//! This module provides functionality to detect Mach-O binaries, parse their
//! dependencies and rpaths, detect architectures, and modify install names.

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::process::Command;

use fs_err as fs;
use goblin::Hint;
use goblin::mach::load_command;
use goblin::mach::{Mach, MachO};
use tracing::trace;

use uv_platform::MacOSVersion;

use crate::error::DelocateError;

/// CPU architecture of a Mach-O binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arch {
    X86_64,
    Arm64,
    I386,
    Arm64_32,
    PowerPC,
    PowerPC64,
    Unknown(u32),
}

impl Arch {
    fn from_cputype(cputype: u32) -> Self {
        use goblin::mach::cputype::{
            CPU_TYPE_ARM64, CPU_TYPE_ARM64_32, CPU_TYPE_I386, CPU_TYPE_POWERPC, CPU_TYPE_POWERPC64,
            CPU_TYPE_X86_64,
        };
        match cputype {
            CPU_TYPE_X86_64 => Self::X86_64,
            CPU_TYPE_ARM64 => Self::Arm64,
            CPU_TYPE_I386 => Self::I386,
            CPU_TYPE_ARM64_32 => Self::Arm64_32,
            CPU_TYPE_POWERPC => Self::PowerPC,
            CPU_TYPE_POWERPC64 => Self::PowerPC64,
            other => Self::Unknown(other),
        }
    }

    /// Returns the architecture name as used in wheel platform tags.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Arm64 => "arm64",
            Self::I386 => "i386",
            Self::Arm64_32 => "arm64_32",
            Self::PowerPC => "ppc",
            Self::PowerPC64 => "ppc64",
            Self::Unknown(_) => "unknown",
        }
    }
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Arch {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "x86_64" => Ok(Self::X86_64),
            "arm64" | "aarch64" => Ok(Self::Arm64),
            "i386" | "i686" | "x86" => Ok(Self::I386),
            "arm64_32" => Ok(Self::Arm64_32),
            "ppc" | "powerpc" => Ok(Self::PowerPC),
            "ppc64" | "powerpc64" => Ok(Self::PowerPC64),
            _ => Err(format!("Unknown architecture: {s}")),
        }
    }
}

/// Parsed Mach-O file information.
#[derive(Debug)]
pub struct MachOFile {
    /// Architectures present in the binary.
    pub archs: HashSet<Arch>,
    /// Dylib dependencies (`LC_LOAD_DYLIB`, `LC_LOAD_WEAK_DYLIB`, etc.).
    pub dependencies: Vec<String>,
    /// Runtime search paths (`LC_RPATH`).
    pub rpaths: Vec<String>,
    /// Install name of this library (`LC_ID_DYLIB`), if present.
    pub install_name: Option<String>,
    /// Minimum macOS version required, per architecture.
    pub min_macos_version: Option<MacOSVersion>,
}

/// Check if a file is a Mach-O binary by examining its magic bytes.
pub fn is_macho_file(path: &Path) -> Result<bool, DelocateError> {
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };

    let mut bytes = [0u8; 16];
    if file.read_exact(&mut bytes).is_err() {
        return Ok(false);
    }

    Ok(matches!(
        goblin::mach::peek_bytes(&bytes),
        Ok(Hint::Mach(_) | Hint::MachFat(_))
    ))
}

/// Parse a Mach-O file and extract dependency information.
pub fn parse_macho(path: &Path) -> Result<MachOFile, DelocateError> {
    let data = fs::read(path)?;
    parse_macho_bytes(&data)
}

/// Parse Mach-O data from bytes.
pub fn parse_macho_bytes(data: &[u8]) -> Result<MachOFile, DelocateError> {
    let mach = Mach::parse(data).map_err(|err| DelocateError::MachOParse(err.to_string()))?;

    match mach {
        Mach::Binary(macho) => Ok(parse_single_macho(&macho)),
        Mach::Fat(fat) => {
            let mut archs = HashSet::new();
            let mut all_deps: HashSet<String> = HashSet::new();
            let mut all_rpaths: HashSet<String> = HashSet::new();
            let mut install_name: Option<String> = None;
            let mut min_macos_version: Option<MacOSVersion> = None;

            for arch in fat.iter_arches().flatten() {
                let slice_data = &data[arch.offset as usize..(arch.offset + arch.size) as usize];
                let macho = MachO::parse(slice_data, 0)
                    .map_err(|err| DelocateError::MachOParse(err.to_string()))?;

                let parsed = parse_single_macho(&macho);
                archs.extend(parsed.archs);
                all_deps.extend(parsed.dependencies);
                all_rpaths.extend(parsed.rpaths);
                install_name = install_name.or(parsed.install_name);

                // Take the maximum macOS version across all architectures.
                if let Some(version) = parsed.min_macos_version {
                    min_macos_version = Some(
                        min_macos_version
                            .map_or(version, |current| std::cmp::max(current, version)),
                    );
                }
            }

            Ok(MachOFile {
                archs,
                dependencies: all_deps.into_iter().collect(),
                rpaths: all_rpaths.into_iter().collect(),
                install_name,
                min_macos_version,
            })
        }
    }
}

fn parse_single_macho(macho: &MachO) -> MachOFile {
    let mut min_macos_version: Option<MacOSVersion> = None;

    for cmd in &macho.load_commands {
        match cmd.command {
            load_command::CommandVariant::BuildVersion(ref build_ver) => {
                // LC_BUILD_VERSION is used in modern binaries; platform 1 = MACOS.
                if build_ver.platform == 1 {
                    let version = MacOSVersion::from_packed(build_ver.minos);
                    min_macos_version = Some(
                        min_macos_version
                            .map_or(version, |current| std::cmp::max(current, version)),
                    );
                }
            }
            load_command::CommandVariant::VersionMinMacosx(ref ver) => {
                // LC_VERSION_MIN_MACOSX is used in older binaries.
                let version = MacOSVersion::from_packed(ver.version);
                min_macos_version = Some(
                    min_macos_version.map_or(version, |current| std::cmp::max(current, version)),
                );
            }
            _ => {}
        }
    }

    MachOFile {
        archs: HashSet::from([Arch::from_cputype(macho.header.cputype())]),
        dependencies: macho.libs.iter().map(|s| (*s).to_string()).collect(),
        rpaths: macho.rpaths.iter().map(|s| (*s).to_string()).collect(),
        install_name: macho.name.map(ToString::to_string),
        min_macos_version,
    }
}

/// Change an install name in a Mach-O binary file.
pub fn change_install_name(
    path: &Path,
    old_name: &str,
    new_name: &str,
) -> Result<(), DelocateError> {
    trace!(
        "Changing install name in {}: {} -> {}",
        path.display(),
        old_name,
        new_name
    );

    let output = Command::new("install_name_tool")
        .args(["-change", old_name, new_name])
        .arg(path)
        .output()?;

    if output.status.success() {
        sign_adhoc(path)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(DelocateError::InstallNameToolFailed {
            path: path.to_path_buf(),
            stderr,
        })
    }
}

/// Change the install ID (`LC_ID_DYLIB`) of a Mach-O library.
pub fn change_install_id(path: &Path, new_id: &str) -> Result<(), DelocateError> {
    trace!("Changing install ID of {} to {}", path.display(), new_id);

    let output = Command::new("install_name_tool")
        .args(["-id", new_id])
        .arg(path)
        .output()?;

    if output.status.success() {
        sign_adhoc(path)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(DelocateError::InstallNameToolFailed {
            path: path.to_path_buf(),
            stderr,
        })
    }
}

/// Delete an rpath from a Mach-O binary.
pub fn delete_rpath(path: &Path, rpath: &str) -> Result<(), DelocateError> {
    trace!("Deleting rpath {} from {}", rpath, path.display());

    let output = Command::new("install_name_tool")
        .args(["-delete_rpath", rpath])
        .arg(path)
        .output()?;

    if output.status.success() {
        sign_adhoc(path)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(DelocateError::InstallNameToolFailed {
            path: path.to_path_buf(),
            stderr,
        })
    }
}

/// Apply ad-hoc code signing to a binary.
///
/// This forcefully replaces any existing signature with an ad-hoc signature.
/// This is required on macOS (especially Apple Silicon) after modifying binaries,
/// as the modification invalidates the existing signature.
fn sign_adhoc(path: &Path) -> Result<(), DelocateError> {
    trace!("Applying ad-hoc code signature to {}", path.display());

    let output = Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(path)
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(DelocateError::CodesignFailed {
            path: path.to_path_buf(),
            stderr,
        })
    }
}

/// Check if a binary has all the required architectures.
pub fn check_archs(path: &Path, required: &[Arch]) -> Result<(), DelocateError> {
    let macho = parse_macho(path)?;

    for arch in required {
        if !macho.archs.contains(arch) {
            return Err(DelocateError::MissingArchitecture {
                arch: arch.to_string(),
                path: path.to_path_buf(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arch_display() {
        assert_eq!(Arch::X86_64.as_str(), "x86_64");
        assert_eq!(Arch::Arm64.as_str(), "arm64");
    }

    #[test]
    fn test_arch_from_str() {
        assert_eq!("x86_64".parse::<Arch>().unwrap(), Arch::X86_64);
        assert_eq!("arm64".parse::<Arch>().unwrap(), Arch::Arm64);
        assert_eq!("aarch64".parse::<Arch>().unwrap(), Arch::Arm64);
        assert_eq!("i386".parse::<Arch>().unwrap(), Arch::I386);
        assert!("unknown_arch".parse::<Arch>().is_err());
    }
}
