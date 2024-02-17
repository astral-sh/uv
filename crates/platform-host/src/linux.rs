//! Taken from `glibc_version` (<https://github.com/delta-incubator/glibc-version-rs>),
//! which used the Apache 2.0 license (but not the MIT license)

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use fs_err as fs;
use goblin::elf::Elf;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::{Os, PlatformError};

pub(crate) fn detect_linux_libc() -> Result<Os, PlatformError> {
    let ld_path = find_ld_path()?;

    tracing::trace!("trying to detect musl version by running `{ld_path:?}`");
    match detect_musl_version(&ld_path) {
        Ok(os) => return Ok(os),
        Err(err) => tracing::trace!("tried to find musl version, but failed: {err}"),
    }
    tracing::trace!("trying to detect libc version from possible symlink at {ld_path:?}");
    match detect_linux_libc_from_ld_symlink(&ld_path) {
        Ok(os) => return Ok(os),
        Err(err) => {
            tracing::trace!("tried to find libc version from ld symlink, but failed: {err}");
        }
    }
    tracing::trace!("trying to run `ldd --version` to detect glibc version");
    match detect_glibc_version_from_ldd() {
        Ok(os_version) => return Ok(os_version),
        Err(err) => {
            tracing::trace!("tried to find glibc version from `ldd --version`, but failed: {err}");
        }
    }
    let msg = "\
          could not detect either glibc version nor musl libc version, \
          at least one of which is required\
      ";
    Err(PlatformError::OsVersionDetectionError(msg.to_string()))
}

// glibc version is taken from std/sys/unix/os.rs
fn detect_glibc_version_from_ldd() -> Result<Os, PlatformError> {
    let output = Command::new("ldd")
        .args(["--version"])
        .output()
        .map_err(|err| {
            PlatformError::OsVersionDetectionError(format!(
                "failed to execute `ldd --version` for glibc: {err}"
            ))
        })?;
    match glibc_ldd_output_to_version("stdout", &output.stdout) {
        Ok(os) => return Ok(os),
        Err(err) => {
            tracing::trace!("failed to parse glibc version from stdout of `ldd --version`: {err}");
        }
    }
    match glibc_ldd_output_to_version("stderr", &output.stderr) {
        Ok(os) => return Ok(os),
        Err(err) => {
            tracing::trace!("failed to parse glibc version from stderr of `ldd --version`: {err}");
        }
    }
    Err(PlatformError::OsVersionDetectionError(
        "could not find glibc version from stdout or stderr of `ldd --version`".to_string(),
    ))
}

fn glibc_ldd_output_to_version(kind: &str, output: &[u8]) -> Result<Os, PlatformError> {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"ldd \(.+\) ([0-9]+\.[0-9]+)").unwrap());

    let output = std::str::from_utf8(output).map_err(|err| {
        PlatformError::OsVersionDetectionError(format!(
            "failed to parse `ldd --version` {kind} as UTF-8: {err}"
        ))
    })?;
    tracing::trace!("{kind} output from `ldd --version`: {output:?}");
    let Some((_, [version])) = RE.captures(output).map(|c| c.extract()) else {
        return Err(PlatformError::OsVersionDetectionError(
            "failed to detect glibc version on {kind}".to_string(),
        ));
    };
    let Some(os) = parse_glibc_version(version) else {
        return Err(PlatformError::OsVersionDetectionError(format!(
            "failed to parse glibc version on {kind} from: {version:?}",
        )));
    };
    Ok(os)
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

fn detect_linux_libc_from_ld_symlink(path: &Path) -> Result<Os, PlatformError> {
    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^ld-([0-9]{1,3})\.([0-9]{1,3})\.so$").unwrap());

    let target = fs::read_link(path).map_err(|err| {
        PlatformError::OsVersionDetectionError(format!(
            "failed to read {path:?} as a symbolic link: {err}",
        ))
    })?;
    let Some(filename) = target.file_name() else {
        return Err(PlatformError::OsVersionDetectionError(format!(
            "failed to get base name of symbolic link path {target:?}",
        )));
    };
    let filename = filename.to_string_lossy();
    let Some((_, [major, minor])) = RE.captures(&filename).map(|c| c.extract()) else {
        return Err(PlatformError::OsVersionDetectionError(format!(
            "failed to find major/minor version in dynamic linker symlink \
             filename {filename:?} from its path {target:?} via regex {regex}",
            regex = RE.as_str(),
        )));
    };
    // OK since we are guaranteed to have between 1 and 3 ASCII digits and the
    // maximum possible value, 999, fits into a u16.
    let major = major.parse().expect("valid major version");
    let minor = minor.parse().expect("valid minor version");
    Ok(Os::Manylinux { major, minor })
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
fn detect_musl_version(ld_path: impl AsRef<Path>) -> Result<Os, PlatformError> {
    let ld_path = ld_path.as_ref();
    let output = Command::new(ld_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| {
            PlatformError::OsVersionDetectionError(format!(
                "failed to execute `{ld_path:?}` for musl: {err}"
            ))
        })?;
    match musl_ld_output_to_version("stdout", &output.stdout) {
        Ok(os) => return Ok(os),
        Err(err) => {
            tracing::trace!("failed to parse musl version from stdout of `{ld_path:?}`: {err}");
        }
    }
    match musl_ld_output_to_version("stderr", &output.stderr) {
        Ok(os) => return Ok(os),
        Err(err) => {
            tracing::trace!("failed to parse musl version from stderr of `{ld_path:?}`: {err}");
        }
    }
    Err(PlatformError::OsVersionDetectionError(format!(
        "could not find musl version from stdout or stderr of `{ld_path:?}`",
    )))
}

fn musl_ld_output_to_version(kind: &str, output: &[u8]) -> Result<Os, PlatformError> {
    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"Version ([0-9]{1,4})\.([0-9]{1,4})").unwrap());

    let output = std::str::from_utf8(output).map_err(|err| {
        PlatformError::OsVersionDetectionError(format!("failed to parse {kind} as UTF-8: {err}"))
    })?;
    tracing::trace!("{kind} output from `ld`: {output:?}");
    let Some((_, [major, minor])) = RE.captures(output).map(|c| c.extract()) else {
        return Err(PlatformError::OsVersionDetectionError(format!(
            "could not find musl version from on {kind} via regex: {}",
            RE.as_str(),
        )));
    };
    // OK since we are guaranteed to have between 1 and 4 ASCII digits and the
    // maximum possible value, 9999, fits into a u16.
    let major = major.parse().expect("valid major version");
    let minor = minor.parse().expect("valid minor version");
    Ok(Os::Musllinux { major, minor })
}

/// Find musl libc path from executable's ELF header.
fn find_ld_path() -> Result<PathBuf, PlatformError> {
    let buffer = fs::read("/bin/sh")?;
    let error_str = "Couldn't parse /bin/sh for detecting the ld version";
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
        let ver_str = glibc_ldd_output_to_version(
            "stdout",
            br"ldd (GNU libc) 2.12
Copyright (C) 2010 Free Software Foundation, Inc.
This is free software; see the source for copying conditions.  There is NO
warranty; not even for MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
Written by Roland McGrath and Ulrich Drepper.",
        )
        .unwrap();
        assert_eq!(
            ver_str,
            Os::Manylinux {
                major: 2,
                minor: 12
            }
        );

        let ver_str = glibc_ldd_output_to_version(
            "stderr",
            br"ldd (Ubuntu GLIBC 2.31-0ubuntu9.2) 2.31
  Copyright (C) 2020 Free Software Foundation, Inc.
  This is free software; see the source for copying conditions.  There is NO
  warranty; not even for MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
  Written by Roland McGrath and Ulrich Drepper.",
        )
        .unwrap();
        assert_eq!(
            ver_str,
            Os::Manylinux {
                major: 2,
                minor: 31
            }
        );
    }

    #[test]
    fn parse_musl_ld_output() {
        // This output was generated by running `/lib/ld-musl-x86_64.so.1`
        // in an Alpine Docker image. The Alpine version:
        //
        // # cat /etc/alpine-release
        // 3.19.1
        let output = b"\
musl libc (x86_64)
Version 1.2.4_git20230717
Dynamic Program Loader
Usage: /lib/ld-musl-x86_64.so.1 [options] [--] pathname [args]\
        ";
        let got = musl_ld_output_to_version("stderr", output).unwrap();
        assert_eq!(got, Os::Musllinux { major: 1, minor: 2 });
    }
}
