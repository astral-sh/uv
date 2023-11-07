//! Taken from `glibc_version` (<https://github.com/delta-incubator/glibc-version-rs>),
//! which used the Apache 2.0 license (but not the MIT license)

use crate::{Os, PlatformError};
use fs_err as fs;
use goblin::elf::Elf;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::trace;

// glibc version is taken from std/sys/unix/os.rs
fn get_version() -> Result<Os, PlatformError> {
    let output = Command::new("ldd")
        .args(["--version"])
        .output()
        .expect("failed to execute ldd");
    let output_str = std::str::from_utf8(&output.stdout).unwrap();
    let version_str = ldd_output_to_version_str(output_str)?;

    parse_glibc_version(version_str).ok_or_else(|| {
        PlatformError::OsVersionDetectionError(format!(
            "Invalid version string from ldd output: {version_str}"
        ))
    })
}

fn ldd_output_to_version_str(output_str: &str) -> Result<&str, PlatformError> {
    let version_reg = Regex::new(r"ldd \(.+\) ([0-9]+\.[0-9]+)").unwrap();
    if let Some(captures) = version_reg.captures(output_str) {
        Ok(captures.get(1).unwrap().as_str())
    } else {
        Err(PlatformError::OsVersionDetectionError(format!(
            "ERROR: failed to detect glibc version. ldd output: {output_str}",
        )))
    }
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
    } else if let Ok(glibc_ld) = fs::read_link(&libc) {
        // Try reading the link first as it's faster
        let filename = glibc_ld
            .file_name()
            .ok_or_else(|| {
                PlatformError::OsVersionDetectionError(
                    "Expected the glibc ld to be a file".to_string(),
                )
            })?
            .to_string_lossy();
        let expr = Regex::new(r"ld-(\d{1,3})\.(\d{1,3})\.so").unwrap();

        if let Some(capture) = expr.captures(&filename) {
            let major = capture.get(1).unwrap().as_str().parse::<u16>().unwrap();
            let minor = capture.get(2).unwrap().as_str().parse::<u16>().unwrap();
            Os::Manylinux { major, minor }
        } else {
            trace!("Couldn't use ld filename, using `ldd --version`");
            // runs `ldd --version`
            get_version().map_err(|err| {
                PlatformError::OsVersionDetectionError(format!(
                    "Failed to determine glibc version with `ldd --version`: {err}"
                ))
            })?
        }
    } else {
        let msg = "\
            Couldn't detect either glibc version nor musl libc version, \
            at least one of which is required\
        ";
        return Err(PlatformError::OsVersionDetectionError(msg.to_string()));
    };
    Ok(linux)
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
    let output = Command::new(ld_path.as_ref())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let expr = Regex::new(r"Version (\d{2,4})\.(\d{2,4})").unwrap();
    if let Some(capture) = expr.captures(&stderr) {
        let major = capture.get(1).unwrap().as_str().parse::<u16>().unwrap();
        let minor = capture.get(2).unwrap().as_str().parse::<u16>().unwrap();
        return Ok(Some((major, minor)));
    }
    Ok(None)
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
            r#"ldd (GNU libc) 2.12
Copyright (C) 2010 Free Software Foundation, Inc.
This is free software; see the source for copying conditions.  There is NO
warranty; not even for MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
Written by Roland McGrath and Ulrich Drepper."#,
        )
        .unwrap();
        assert_eq!(ver_str, "2.12");

        let ver_str = ldd_output_to_version_str(
            r#"ldd (Ubuntu GLIBC 2.31-0ubuntu9.2) 2.31
  Copyright (C) 2020 Free Software Foundation, Inc.
  This is free software; see the source for copying conditions.  There is NO
  warranty; not even for MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
  Written by Roland McGrath and Ulrich Drepper."#,
        )
        .unwrap();
        assert_eq!(ver_str, "2.31");
    }
}
