//! Patch `sysconfig` data in a Python installation.
//!
//! Inspired by: <https://github.com/bluss/sysconfigpatcher/blob/c1ebf8ab9274dcde255484d93ce0f1fd1f76a248/src/sysconfigpatcher.py#L137C1-L140C100>,
//! available under the MIT license:
//!
//! ```text
//! Copyright 2024 Ulrik Sverdrup "bluss"
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of
//! this software and associated documentation files (the "Software"), to deal in
//! the Software without restriction, including without limitation the rights to
//! use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
//! the Software, and to permit persons to whom the Software is furnished to do so,
//! subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
//! FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
//! COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
//! IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
//! CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use tracing::trace;

use crate::sysconfig::parser::{Error as ParseError, SysconfigData, Value};

mod cursor;
mod parser;

/// Update the `sysconfig` data in a Python installation.
pub(crate) fn update_sysconfig(
    install_root: &Path,
    major: u8,
    minor: u8,
    suffix: &str,
) -> Result<(), Error> {
    // Find the `_sysconfigdata_` file in the Python installation.
    let real_prefix = std::path::absolute(install_root)?;
    let sysconfigdata = find_sysconfigdata(&real_prefix, major, minor, suffix)?;
    trace!(
        "Discovered `sysconfig` data at: {}",
        sysconfigdata.display()
    );

    // Update the `_sysconfigdata_` file in-memory.
    let contents = fs_err::read_to_string(&sysconfigdata)?;
    let data = SysconfigData::from_str(&contents)?;
    let data = patch_sysconfigdata(data, &real_prefix);
    let contents = data.to_string_pretty()?;

    // Write the updated `_sysconfigdata_` file.
    let mut file = fs_err::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&sysconfigdata)?;
    file.write_all(contents.as_bytes())?;
    file.sync_data()?;

    Ok(())
}

/// Find the `_sysconfigdata_` file in a Python installation.
///
/// For example, on macOS, returns `{real_prefix}/lib/python3.12/_sysconfigdata__darwin_darwin.py"`.
fn find_sysconfigdata(
    real_prefix: &Path,
    major: u8,
    minor: u8,
    suffix: &str,
) -> Result<PathBuf, Error> {
    // Find the `lib` directory in the Python installation.
    let lib = real_prefix
        .join("lib")
        .join(format!("python{major}.{minor}{suffix}"));
    if !lib.exists() {
        return Err(Error::MissingLib);
    }

    // Probe the `lib` directory for `_sysconfigdata_`.
    for entry in lib.read_dir()? {
        let entry = entry?;

        if entry.path().extension().is_none_or(|ext| ext != "py") {
            continue;
        }

        if !entry
            .path()
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| stem.starts_with("_sysconfigdata_"))
        {
            continue;
        }

        let metadata = entry.metadata()?;
        if metadata.is_symlink() {
            continue;
        };

        if metadata.is_file() {
            return Ok(entry.path());
        }
    }

    Err(Error::MissingSysconfigdata)
}

/// Patch the given `_sysconfigdata_` contents.
fn patch_sysconfigdata(mut data: SysconfigData, real_prefix: &Path) -> SysconfigData {
    /// Update the `/install` prefix in a whitespace-separated string.
    fn update_prefix(s: &str, real_prefix: &Path) -> String {
        s.split_whitespace()
            .map(|part| {
                if let Some(rest) = part.strip_prefix("/install") {
                    if rest.is_empty() {
                        real_prefix.display().to_string()
                    } else {
                        real_prefix.join(&rest[1..]).display().to_string()
                    }
                } else {
                    part.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Remove any references to `-isysroot` in a whitespace-separated string.
    fn remove_isysroot(s: &str) -> String {
        // If we see `-isysroot`, drop it and the next part.
        let mut parts = s.split_whitespace().peekable();
        let mut result = Vec::with_capacity(parts.size_hint().0);
        while let Some(part) = parts.next() {
            if part == "-isysroot" {
                parts.next();
            } else {
                result.push(part);
            }
        }
        result.join(" ")
    }

    // Patch each value, as needed.
    let mut count = 0;
    for (key, value) in data.iter_mut() {
        let Value::String(value) = value else {
            continue;
        };
        let patched = update_prefix(value, real_prefix);
        let patched = remove_isysroot(&patched);
        if *value != patched {
            trace!("Updated `{key}` from `{value}` to `{patched}`");
            count += 1;
            *value = patched;
        }
    }

    match count {
        0 => trace!("No updates required"),
        1 => trace!("Updated 1 value"),
        n => trace!("Updated {n} values"),
    }

    // Mark the Python installation as standalone.
    data.insert("PYTHON_BUILD_STANDALONE".to_string(), Value::Int(1));

    data
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Python installation is missing a `lib` directory")]
    MissingLib,
    #[error("Python installation is missing a `_sysconfigdata_` file")]
    MissingSysconfigdata,
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    #[test]
    fn update_real_prefix() -> Result<(), Error> {
        let sysconfigdata = [
            ("BASEMODLIBS", ""),
            ("BINDIR", "/install/bin"),
            ("BINLIBDEST", "/install/lib/python3.10"),
            ("BLDLIBRARY", "-L. -lpython3.10"),
            ("BUILDPYTHON", "python.exe"),
            ("prefix", "/install/prefix"),
            ("exec_prefix", "/install/exec_prefix"),
            ("base", "/install/base"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
        .collect::<SysconfigData>();

        let real_prefix = Path::new("/real/prefix");
        let data = patch_sysconfigdata(sysconfigdata, real_prefix);

        insta::assert_snapshot!(data.to_string_pretty()?, @r###"
        # system configuration generated and used by the sysconfig module
        build_time_vars = {
            "BASEMODLIBS": "",
            "BINDIR": "/real/prefix/bin",
            "BINLIBDEST": "/real/prefix/lib/python3.10",
            "BLDLIBRARY": "-L. -lpython3.10",
            "BUILDPYTHON": "python.exe",
            "PYTHON_BUILD_STANDALONE": 1,
            "base": "/real/prefix/base",
            "exec_prefix": "/real/prefix/exec_prefix",
            "prefix": "/real/prefix/prefix"
        }
        "###);

        Ok(())
    }

    #[test]
    fn remove_isysroot() -> Result<(), Error> {
        let sysconfigdata = [
            ("BLDSHARED", "clang -bundle -undefined dynamic_lookup -arch arm64 -isysroot /Applications/MacOSX14.2.sdk"),
        ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect::<SysconfigData>();

        let real_prefix = Path::new("/real/prefix");
        let data = patch_sysconfigdata(sysconfigdata, real_prefix);

        insta::assert_snapshot!(data.to_string_pretty()?, @r###"
        # system configuration generated and used by the sysconfig module
        build_time_vars = {
            "BLDSHARED": "clang -bundle -undefined dynamic_lookup -arch arm64",
            "PYTHON_BUILD_STANDALONE": 1
        }
        "###);

        Ok(())
    }
}
