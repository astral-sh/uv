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

use std::borrow::Cow;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use itertools::{Either, Itertools};
use tracing::trace;

use crate::sysconfig::generated_mappings::DEFAULT_VARIABLE_UPDATES;
use crate::sysconfig::parser::{Error as ParseError, SysconfigData, Value};

mod cursor;
mod generated_mappings;
mod parser;
mod replacements;

const RELOCATABLE_PREFIX: &str = "__UV_PYTHON_INSTALL_PREFIX__";

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

    // Find the `pkgconfig` files in the Python installation.
    for pkgconfig in find_pkgconfigs(&real_prefix)? {
        let pkgconfig = pkgconfig?;
        trace!("Discovered `pkgconfig` data at: {}", pkgconfig.display());

        // Update the `pkgconfig` file in-memory.
        let contents = fs_err::read_to_string(&pkgconfig)?;
        if let Some(new_contents) = patch_pkgconfig(&contents) {
            // Write the updated `pkgconfig` file.
            let mut file = fs_err::OpenOptions::new()
                .write(true)
                .truncate(true)
                .create(true)
                .open(&pkgconfig)?;
            file.write_all(new_contents.as_bytes())?;
            file.sync_data()?;
        }
    }

    Ok(())
}

/// Make a Python installation's `sysconfig` data follow its location.
pub(crate) fn make_sysconfig_relocatable(
    install_root: &Path,
    major: u8,
    minor: u8,
    suffix: &str,
) -> Result<(), Error> {
    let install_root = std::path::absolute(install_root)?;
    let sysconfigdata = find_sysconfigdata(&install_root, major, minor, suffix)?;
    trace!(
        "Making `sysconfig` data relocatable at: {}",
        sysconfigdata.display()
    );

    let contents = fs_err::read_to_string(&sysconfigdata)?;
    let mut data = SysconfigData::from_str(&contents)?;
    let Some((_, Value::String(previous_prefix))) =
        data.iter_mut().find(|(key, _)| key.as_str() == "prefix")
    else {
        return Err(Error::MissingSysconfigPrefix);
    };
    let previous_prefix = std::mem::replace(previous_prefix, RELOCATABLE_PREFIX.to_string());

    for (_, value) in data.iter_mut() {
        if let Value::String(value) = value {
            *value = value.replace(&previous_prefix, RELOCATABLE_PREFIX);
        }
    }

    let mut file = fs_err::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&sysconfigdata)?;
    file.write_all(data.to_string_pretty()?.as_bytes())?;
    file.write_all(
        br#"
_uv_path = __import__("os").path
_uv_previous_prefix = build_time_vars["prefix"]
_uv_current_prefix = _uv_path.dirname(
    _uv_path.dirname(_uv_path.dirname(_uv_path.abspath(__file__)))
)
for _uv_key, _uv_value in build_time_vars.items():
    if isinstance(_uv_value, str):
        build_time_vars[_uv_key] = _uv_value.replace(
            _uv_previous_prefix, _uv_current_prefix
        )
"#,
    )?;
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
        return Err(Error::MissingLib(lib));
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
        }

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
        let mut patched = remove_isysroot(&patched);

        if let Some(replacement_entries) = DEFAULT_VARIABLE_UPDATES.get(key) {
            for replacement_entry in replacement_entries {
                patched = replacement_entry.patch(&patched);
            }
        }

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

/// Find the location of all `pkg-config` files in a Python installation.
///
/// Specifically, searches for files under `lib/pkgconfig` with the `.pc` extension.
fn find_pkgconfigs(
    install_root: &Path,
) -> Result<impl Iterator<Item = Result<PathBuf, std::io::Error>>, std::io::Error> {
    let pkgconfig = install_root.join("lib").join("pkgconfig");

    let read_dir = match pkgconfig.read_dir() {
        Ok(read_dir) => read_dir,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Either::Left(std::iter::empty()));
        }
        Err(err) => return Err(err),
    };

    Ok(Either::Right(
        read_dir
            .filter_ok(|entry| entry.path().extension().is_some_and(|ext| ext == "pc"))
            .filter_ok(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_file()))
            .map_ok(|entry| entry.path()),
    ))
}

/// Patch the given `pkgconfig` contents.
///
/// Returns the updated contents, if an update is needed.
fn patch_pkgconfig(contents: &str) -> Option<String> {
    let mut changed = false;
    let new_contents = contents
        .lines()
        .map(|line| {
            // python-build-standalone is compiled with a prefix of
            // /install. Replace lines like `prefix=/install` with
            // `prefix=${pcfiledir}/../..` (since the .pc file is in
            // lib/pkgconfig/). Newer versions of python-build-standalone
            // already have this change.
            let Some((prefix, suffix)) = line.split_once('=') else {
                return Cow::Borrowed(line);
            };

            // The content before the `=` must be an ASCII alphabetic string.
            if !prefix.chars().all(|c| c.is_ascii_alphabetic()) {
                return Cow::Borrowed(line);
            }

            // The content after the `=` must be equal to the expected prefix.
            if suffix != "/install" {
                return Cow::Borrowed(line);
            }

            changed = true;
            Cow::Owned(format!("{prefix}=${{pcfiledir}}/../.."))
        })
        .join("\n");
    if changed { Some(new_contents) } else { None }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Python installation is missing a `lib` directory at: {0}")]
    MissingLib(PathBuf),
    #[error("Python installation is missing a `_sysconfigdata_` file")]
    MissingSysconfigdata,
    #[error("Python installation's `_sysconfigdata_` is missing its installation prefix")]
    MissingSysconfigPrefix,
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use indoc::indoc;

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

        insta::assert_snapshot!(data.to_string_pretty()?, @r#"
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
        "#);

        Ok(())
    }

    #[test]
    fn relocate_sysconfig_preserves_install_paths() -> Result<(), Error> {
        let root = tempfile::tempdir()?;
        let lib = root.path().join("lib/python3.12");
        fs_err::create_dir_all(&lib)?;
        let sysconfigdata = lib.join("_sysconfigdata_test.py");
        fs_err::write(
            &sysconfigdata,
            indoc! {r#"
                # system configuration generated and used by the sysconfig module
                build_time_vars = {
                    "BINDIR": "/source/python/bin",
                    "CONFIG_ARGS": "--prefix=/install",
                    "INSTALL": "/usr/bin/install -c",
                    "WASM_ASSETS_DIR": "./install",
                    "prefix": "/source/python"
                }
            "#},
        )?;

        make_sysconfig_relocatable(root.path(), 3, 12, "")?;

        let contents = fs_err::read_to_string(&sysconfigdata)?;
        let data = SysconfigData::from_str(&contents)?;
        insta::assert_snapshot!(data.to_string_pretty()?, @r#"
        # system configuration generated and used by the sysconfig module
        build_time_vars = {
            "BINDIR": "__UV_PYTHON_INSTALL_PREFIX__/bin",
            "CONFIG_ARGS": "--prefix=/install",
            "INSTALL": "/usr/bin/install -c",
            "WASM_ASSETS_DIR": "./install",
            "prefix": "__UV_PYTHON_INSTALL_PREFIX__"
        }
        "#);

        Ok(())
    }

    #[test]
    fn test_replacements() -> Result<(), Error> {
        let sysconfigdata = [
            ("CC", "clang -pthread"),
            ("CXX", "clang++ -pthread"),
            ("AR", "/tools/llvm/bin/llvm-ar"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
        .collect::<SysconfigData>();

        let real_prefix = Path::new("/real/prefix");
        let data = patch_sysconfigdata(sysconfigdata, real_prefix);

        insta::assert_snapshot!(data.to_string_pretty()?, @r#"
        # system configuration generated and used by the sysconfig module
        build_time_vars = {
            "AR": "ar",
            "CC": "cc -pthread",
            "CXX": "c++ -pthread",
            "PYTHON_BUILD_STANDALONE": 1
        }
        "#);

        // Cross-compiles may embed historical compiler paths.
        let sysconfigdata = [
            ("BLDSHARED", "/usr/bin/aarch64-linux-gnu-gcc"),
            ("CC", "/usr/bin/riscv64-linux-gnu-gcc"),
            ("CXX", "/usr/bin/riscv64-linux-gnu-g++"),
            ("LDCXXSHARED", "/usr/bin/aarch64-linux-gnu-g++"),
            ("LDSHARED", "/usr/bin/aarch64-linux-gnu-gcc"),
            ("LINKCC", "/usr/bin/riscv64-linux-gnu-gcc"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
        .collect::<SysconfigData>();

        let real_prefix = Path::new("/real/prefix");
        let data = patch_sysconfigdata(sysconfigdata, real_prefix);

        insta::assert_snapshot!(data.to_string_pretty()?, @r#"
        # system configuration generated and used by the sysconfig module
        build_time_vars = {
            "BLDSHARED": "cc",
            "CC": "cc",
            "CXX": "c++",
            "LDCXXSHARED": "c++",
            "LDSHARED": "cc",
            "LINKCC": "cc",
            "PYTHON_BUILD_STANDALONE": 1
        }
        "#);

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

        insta::assert_snapshot!(data.to_string_pretty()?, @r#"
        # system configuration generated and used by the sysconfig module
        build_time_vars = {
            "BLDSHARED": "cc -bundle -undefined dynamic_lookup -arch arm64",
            "PYTHON_BUILD_STANDALONE": 1
        }
        "#);

        Ok(())
    }

    #[test]
    fn update_pkgconfig() {
        let pkgconfig = indoc! {
            r"
            # See: man pkg-config
            prefix=/install
            exec_prefix=${prefix}
            libdir=${exec_prefix}/lib
            includedir=${prefix}/include

            Name: Python
            Description: Build a C extension for Python
            Requires:
            Version: 3.10
            Libs.private: -ldl   -framework CoreFoundation
            Libs:
            Cflags: -I${includedir}/python3.10
            "
        };

        let pkgconfig = patch_pkgconfig(pkgconfig).unwrap();

        insta::assert_snapshot!(pkgconfig, @"
        # See: man pkg-config
        prefix=${pcfiledir}/../..
        exec_prefix=${prefix}
        libdir=${exec_prefix}/lib
        includedir=${prefix}/include

        Name: Python
        Description: Build a C extension for Python
        Requires:
        Version: 3.10
        Libs.private: -ldl   -framework CoreFoundation
        Libs:
        Cflags: -I${includedir}/python3.10
        ");
    }
}
