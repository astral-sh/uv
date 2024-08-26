//! Determine the libc (glibc or musl) on linux.
//!
//! Taken from `glibc_version` (<https://github.com/delta-incubator/glibc-version-rs>),
//! which used the Apache 2.0 license (but not the MIT license)

use fs_err as fs;
use goblin::elf::Elf;
use regex::Regex;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use thiserror::Error;
use tracing::trace;
use uv_fs::Simplified;

#[derive(Debug, Error)]
pub enum LibcDetectionError {
    #[error("Could not detect either glibc version nor musl libc version, at least one of which is required")]
    NoLibcFound,
    #[error("Failed to get base name of symbolic link path {0}")]
    MissingBasePath(PathBuf),
    #[error("Failed to find glibc version in the filename of linker: `{0}`")]
    GlibcExtractionMismatch(PathBuf),
    #[error("Failed to determine {libc} version by running: `{program}`")]
    FailedToRun {
        libc: &'static str,
        program: String,
        #[source]
        err: io::Error,
    },
    #[error("Could not find glibc version in output of: `ldd --version`")]
    InvalidLddOutputGnu,
    #[error("Could not find musl version in output of: `{0}`")]
    InvalidLddOutputMusl(PathBuf),
    #[error("Could not read ELF interpreter from any of the following paths: {0}")]
    CoreBinaryParsing(String),
    #[error("Failed to determine libc")]
    Io(#[from] io::Error),
}

/// We support glibc (manylinux) and musl (musllinux) on linux.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LibcVersion {
    Manylinux { major: u32, minor: u32 },
    Musllinux { major: u32, minor: u32 },
}

/// Determine whether we're running glibc or musl and in which version, given we are on linux.
///
/// Normally, we determine this from the python interpreter, which is more accurate, but when
/// deciding which python interpreter to download, we need to figure this out from the environment.
///
/// A platform can have both musl and glibc installed. We determine the preferred platform by
/// inspecting core binaries.
pub(crate) fn detect_linux_libc() -> Result<LibcVersion, LibcDetectionError> {
    let ld_path = find_ld_path()?;
    trace!("ld path: {}", ld_path.user_display());

    match detect_musl_version(&ld_path) {
        Ok(os) => return Ok(os),
        Err(err) => {
            trace!("Tried to find musl version by running `{ld_path:?}`, but failed: {err}");
        }
    }
    match detect_linux_libc_from_ld_symlink(&ld_path) {
        Ok(os) => return Ok(os),
        Err(err) => {
            trace!("Tried to find libc version from possible symlink at {ld_path:?}, but failed: {err}");
        }
    }
    match detect_glibc_version_from_ldd(&ld_path) {
        Ok(os_version) => return Ok(os_version),
        Err(err) => {
            trace!("Tried to find glibc version from `ldd --version`, but failed: {err}");
        }
    }
    Err(LibcDetectionError::NoLibcFound)
}

// glibc version is taken from `std/sys/unix/os.rs`.
fn detect_glibc_version_from_ldd(ldd: &Path) -> Result<LibcVersion, LibcDetectionError> {
    let output = Command::new(ldd)
        .args(["--version"])
        .output()
        .map_err(|err| LibcDetectionError::FailedToRun {
            libc: "glibc",
            program: format!("{} --version", ldd.user_display()),
            err,
        })?;
    if let Some(os) = glibc_ldd_output_to_version("stdout", &output.stdout) {
        return Ok(os);
    }
    if let Some(os) = glibc_ldd_output_to_version("stderr", &output.stderr) {
        return Ok(os);
    }
    Err(LibcDetectionError::InvalidLddOutputGnu)
}

/// Parse `ldd --version` output.
///
/// Example: `ld.so (Ubuntu GLIBC 2.39-0ubuntu8.3) stable release version 2.39.`.
fn glibc_ldd_output_to_version(kind: &str, output: &[u8]) -> Option<LibcVersion> {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"ld.so \(.+\) .* ([0-9]+\.[0-9]+)").unwrap());

    let output = String::from_utf8_lossy(output);
    trace!("{kind} output from `ldd --version`: {output:?}");
    let (_, [version]) = RE.captures(output.as_ref()).map(|c| c.extract())?;
    // Parse the input as "x.y" glibc version.
    let mut parsed_ints = version.split('.').map(str::parse).fuse();
    let major = parsed_ints.next()?.ok()?;
    let minor = parsed_ints.next()?.ok()?;
    trace!("Found manylinux {major}.{minor} in {kind} of `ldd --version`");
    Some(LibcVersion::Manylinux { major, minor })
}

fn detect_linux_libc_from_ld_symlink(path: &Path) -> Result<LibcVersion, LibcDetectionError> {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^ld-([0-9]{1,3})\.([0-9]{1,3})\.so$").unwrap());

    let ld_path = fs::read_link(path)?;
    let filename = ld_path
        .file_name()
        .ok_or_else(|| LibcDetectionError::MissingBasePath(ld_path.clone()))?
        .to_string_lossy();
    let (_, [major, minor]) = RE
        .captures(&filename)
        .map(|c| c.extract())
        .ok_or_else(|| LibcDetectionError::GlibcExtractionMismatch(ld_path.clone()))?;
    // OK since we are guaranteed to have between 1 and 3 ASCII digits and the
    // maximum possible value, 999, fits into a u16.
    let major = major.parse().expect("valid major version");
    let minor = minor.parse().expect("valid minor version");
    Ok(LibcVersion::Manylinux { major, minor })
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
fn detect_musl_version(ld_path: impl AsRef<Path>) -> Result<LibcVersion, LibcDetectionError> {
    let ld_path = ld_path.as_ref();
    let output = Command::new(ld_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| LibcDetectionError::FailedToRun {
            libc: "musl",
            program: ld_path.to_string_lossy().to_string(),
            err,
        })?;

    if let Some(os) = musl_ld_output_to_version("stdout", &output.stdout) {
        return Ok(os);
    }
    if let Some(os) = musl_ld_output_to_version("stderr", &output.stderr) {
        return Ok(os);
    }
    Err(LibcDetectionError::InvalidLddOutputMusl(
        ld_path.to_path_buf(),
    ))
}

/// Parse the musl version from ld output.
///
/// Example: `Version 1.2.5`.
fn musl_ld_output_to_version(kind: &str, output: &[u8]) -> Option<LibcVersion> {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"Version ([0-9]{1,4})\.([0-9]{1,4})").unwrap());

    let output = String::from_utf8_lossy(output);
    trace!("{kind} output from `ld`: {output:?}");
    let (_, [major, minor]) = RE.captures(output.as_ref()).map(|c| c.extract())?;
    // unwrap-safety: Since we are guaranteed to have between 1 and 4 ASCII digits and the
    // maximum possible value, 9999, fits into a u16.
    let major = major.parse().expect("valid major version");
    let minor = minor.parse().expect("valid minor version");
    trace!("Found musllinux {major}.{minor} in {kind} of `ld`");
    Some(LibcVersion::Musllinux { major, minor })
}

/// Find musl ld path from executable's ELF header.
fn find_ld_path() -> Result<PathBuf, LibcDetectionError> {
    // At first, we just looked for /bin/ls. But on some Linux distros, /bin/ls
    // is a shell script that just calls /usr/bin/ls. So we switched to looking
    // at /bin/sh. But apparently in some environments, /bin/sh is itself just
    // a shell script that calls /bin/dash. So... We just try a few different
    // paths. In most cases, /bin/sh should work.
    //
    // See: https://github.com/astral-sh/uv/pull/1493
    // See: https://github.com/astral-sh/uv/issues/1810
    // See: https://github.com/astral-sh/uv/issues/4242#issuecomment-2306164449
    let attempts = ["/bin/sh", "/usr/bin/env", "/bin/dash", "/bin/ls"];
    for path in attempts {
        if let Some(ld_path) = find_ld_path_at(path) {
            return Ok(ld_path);
        }
    }
    Err(LibcDetectionError::CoreBinaryParsing(attempts.join(", ")))
}

/// Attempt to find the path to the `ld` executable by
/// ELF parsing the given path. If this fails for any
/// reason, then an error is returned.
fn find_ld_path_at(path: impl AsRef<Path>) -> Option<PathBuf> {
    let path = path.as_ref();
    // Not all linux distributions have all of these paths.
    let buffer = fs::read(path).ok()?;
    let elf = match Elf::parse(&buffer) {
        Ok(elf) => elf,
        Err(err) => {
            trace!(
                "Could not parse ELF file at `{}`: `{}`",
                path.user_display(),
                err
            );
            return None;
        }
    };
    let Some(elf_interpreter) = elf.interpreter else {
        trace!(
            "Couldn't find ELF interpreter path from {}",
            path.user_display()
        );
        return None;
    };

    Some(PathBuf::from(elf_interpreter))
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn parse_ldd_output() {
        let ver_str = glibc_ldd_output_to_version(
            "stdout",
            indoc! {br"ld.so (Ubuntu GLIBC 2.39-0ubuntu8.3) stable release version 2.39.
                Copyright (C) 2024 Free Software Foundation, Inc.
                This is free software; see the source for copying conditions.
                There is NO warranty; not even for MERCHANTABILITY or FITNESS FOR A
                PARTICULAR PURPOSE.
            "},
        )
        .unwrap();
        assert_eq!(
            ver_str,
            LibcVersion::Manylinux {
                major: 2,
                minor: 39
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
        assert_eq!(got, LibcVersion::Musllinux { major: 1, minor: 2 });
    }
}
