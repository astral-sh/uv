//! Taken from `glibc_version` (<https://github.com/delta-incubator/glibc-version-rs>),
//! which used the Apache 2.0 license (but not the MIT license)

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use fs_err as fs;
use goblin::elf::Elf;
use once_cell::sync::Lazy;
use regex::Regex;
use tracing::trace;

use crate::{Os, PlatformError};

// glibc version is taken from std/sys/unix/os.rs
fn glibc_version_from_ldd() -> Result<Os, PlatformError> {
    trace!("Falling back to `ldd --version` to detect OS libc version");
    let output = Command::new("ldd")
        .args(["--version"])
        .output()
        .map_err(|err| {
            PlatformError::OsVersionDetectionError(format!("Failed to execute ldd: {err}"))
        })?;
    let output_str = std::str::from_utf8(&output.stdout).map_err(|err| {
        PlatformError::OsVersionDetectionError(format!(
            "Failed to parse ldd output as UTF-8: {err}"
        ))
    })?;
    let version_str = ldd_output_to_version_str(output_str)?;

    parse_glibc_version(version_str).ok_or_else(|| {
        PlatformError::OsVersionDetectionError(format!(
            "Invalid version string from ldd output: {version_str}"
        ))
    })
}

fn ldd_output_to_version_str(output_str: &str) -> Result<&str, PlatformError> {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"ldd \(.+\) ([0-9]+\.[0-9]+)").unwrap());
    let Some((_, [version])) = RE.captures(output_str).map(|c| c.extract()) else {
        return Err(PlatformError::OsVersionDetectionError(format!(
            "ERROR: failed to detect glibc version. ldd output: {output_str}",
        )));
    };
    Ok(version)
}

// Returns Some((major, minor)) if the string is a valid "x.y" version,
// ignoring any extra dot-separated parts. Otherwise return None.
fn parse_glibc_version(version: &str) -> Option<Os> {
    let mut parsed_ints = version.split('.').map(str::parse).fuse();
    match (parsed_ints.next(), parsed_ints.next()) {
        (Some(Ok(major)), Some(Ok(minor))) => Some(Os::Manylinux { major, minor }),
        _ => None,
    }
}

pub(crate) fn detect_linux_libc() -> Result<Os, PlatformError> {
    let libc = find_libc()?;
    let linux = if let Ok(Some((major, minor))) = get_musl_version(&libc) {
        Os::Musllinux { major, minor }
    } else if let Some(os_version) = detect_linux_libc_from_ld_symlink(&libc) {
        return Ok(os_version);
    } else if let Ok(os_version) = glibc_version_from_ldd() {
        return Ok(os_version);
    } else {
        let msg = "\
            Couldn't detect either glibc version nor musl libc version, \
            at least one of which is required\
        ";
        return Err(PlatformError::OsVersionDetectionError(msg.to_string()));
    };
    Ok(linux)
}

fn detect_linux_libc_from_ld_symlink(path: &Path) -> Option<Os> {
    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^ld-([0-9]{1,3})\.([0-9]{1,3})\.so$").unwrap());

    let target = fs::read_link(path).ok()?;
    let Some(filename) = target.file_name() else {
        trace!("expected dynamic linker symlink {target:?} to have a filename");
        return None;
    };
    let filename = filename.to_string_lossy();
    let Some((_, [major, minor])) = RE.captures(&filename).map(|c| c.extract()) else {
        trace!(
            "couldn't find major/minor version in dynamic linker symlink \
             filename {filename:?} from its path {target:?}"
        );
        return None;
    };
    // OK since we are guaranteed to have between 1 and 3 ASCII digits and the
    // maximum possible value, 999, fits into a u16.
    let major = major.parse().expect("valid major version");
    let minor = minor.parse().expect("valid minor version");
    Some(Os::Manylinux { major, minor })
}

/// Read the musl version from libc library's output. Taken from maturin.
///
/// The libc library should output something like this to `stderr`:
///
/// ```text
/// musl libc (`x86_64`)
/// Version 1.2.2
/// Dynamic Program Loader
/// ```
fn get_musl_version(ld_path: impl AsRef<Path>) -> std::io::Result<Option<(u16, u16)>> {
    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"Version ([0-9]{2,4})\.([0-9]{2,4})").unwrap());

    let output = Command::new(ld_path.as_ref())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let Some((_, [major, minor])) = RE.captures(&stderr).map(|c| c.extract()) else {
        return Ok(None);
    };
    // OK since we are guaranteed to have between 2 and 4 ASCII digits and the
    // maximum possible value, 9999, fits into a u16.
    let major = major.parse().expect("valid major version");
    let minor = minor.parse().expect("valid minor version");
    Ok(Some((major, minor)))
}

/// Find musl libc path from executable's ELF header.
fn find_libc() -> Result<PathBuf, PlatformError> {
    let buffer = fs::read("/bin/ls")?;
    let error_str = "Couldn't parse /bin/ls for detecting the ld version";
    let elf = Elf::parse(&buffer)
        .map_err(|err| PlatformError::OsVersionDetectionError(format!("{error_str}: {err}")))?;
    if let Some(elf_interpreter) = elf.interpreter {
        Ok(PathBuf::from(elf_interpreter))
    } else {
        Err(PlatformError::OsVersionDetectionError(
            error_str.to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ldd_output() {
        let ver_str = ldd_output_to_version_str(
            r"ldd (GNU libc) 2.12
Copyright (C) 2010 Free Software Foundation, Inc.
This is free software; see the source for copying conditions.  There is NO
warranty; not even for MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
Written by Roland McGrath and Ulrich Drepper.",
        )
        .unwrap();
        assert_eq!(ver_str, "2.12");

        let ver_str = ldd_output_to_version_str(
            r"ldd (Ubuntu GLIBC 2.31-0ubuntu9.2) 2.31
  Copyright (C) 2020 Free Software Foundation, Inc.
  This is free software; see the source for copying conditions.  There is NO
  warranty; not even for MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
  Written by Roland McGrath and Ulrich Drepper.",
        )
        .unwrap();
        assert_eq!(ver_str, "2.31");
    }
}
