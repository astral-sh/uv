use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub use error::Error;
use regex::Regex;
use uv_static::EnvVars;

mod error;
pub mod hash;
pub mod stream;

static CONTROL_CHARACTERS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{C}").unwrap());
static REPLACEMENT_CHARACTER: &str = "\u{FFFD}";

/// Extract the top-level directory from an unpacked archive.
///
/// The specification says:
/// > A .tar.gz source distribution (sdist) contains a single top-level directory called
/// > `{name}-{version}` (e.g. foo-1.0), containing the source files of the package.
///
/// This function returns the path to that top-level directory.
pub fn strip_component(source: impl AsRef<Path>) -> Result<PathBuf, Error> {
    // TODO(konstin): Verify the name of the directory.
    let top_level = fs_err::read_dir(source.as_ref())
        .map_err(Error::Io)?
        .collect::<std::io::Result<Vec<fs_err::DirEntry>>>()
        .map_err(Error::Io)?;
    match top_level.as_slice() {
        [root] => Ok(root.path()),
        [] => Err(Error::EmptyArchive),
        _ => Err(Error::NonSingularArchive(
            top_level
                .into_iter()
                .map(|entry| entry.file_name())
                .collect(),
        )),
    }
}

/// Validate that a given filename (e.g. reported by a ZIP archive's
/// local file entries or central directory entries) is "safe" to use.
///
/// "Safe" in this context doesn't refer to directory traversal
/// risk, but whether we believe that other ZIP implementations
/// handle the name correctly and consistently.
///
/// Specifically, we want to avoid names that:
///
/// - Contain *any* non-printable characters
/// - Are empty
///
/// In the future, we may also want to check for names that contain
/// leading/trailing whitespace, or names that are exceedingly long.
pub(crate) fn validate_archive_member_name(name: &str) -> Result<(), Error> {
    if name.is_empty() {
        return Err(Error::EmptyFilename);
    }

    match CONTROL_CHARACTERS_RE.replace_all(name, REPLACEMENT_CHARACTER) {
        // No replacements mean no control characters.
        std::borrow::Cow::Borrowed(_) => Ok(()),
        std::borrow::Cow::Owned(sanitized) => Err(Error::UnacceptableFilename {
            filename: sanitized,
        }),
    }
}

/// Returns `true` if ZIP validation is disabled.
pub(crate) fn insecure_no_validate() -> bool {
    // TODO(charlie) Parse this in `EnvironmentOptions`.
    let Some(value) = std::env::var_os(EnvVars::UV_INSECURE_NO_ZIP_VALIDATION) else {
        return false;
    };
    let Some(value) = value.to_str() else {
        return false;
    };
    matches!(
        value.to_lowercase().as_str(),
        "y" | "yes" | "t" | "true" | "on" | "1"
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_validate_archive_member_name() {
        for (testcase, ok) in &[
            // Valid cases.
            ("normal.txt", true),
            ("__init__.py", true),
            ("fine i guess.py", true),
            ("🌈.py", true),
            // Invalid cases.
            ("", false),
            ("new\nline.py", false),
            ("carriage\rreturn.py", false),
            ("tab\tcharacter.py", false),
            ("null\0byte.py", false),
            ("control\x01code.py", false),
            ("control\x02code.py", false),
            ("control\x03code.py", false),
            ("control\x04code.py", false),
            ("backspace\x08code.py", false),
            ("delete\x7fcode.py", false),
        ] {
            assert_eq!(
                super::validate_archive_member_name(testcase).is_ok(),
                *ok,
                "testcase: {testcase}"
            );
        }
    }

    #[test]
    fn test_unacceptable_filename_error_replaces_control_characters() {
        let err = super::validate_archive_member_name("bad\nname").unwrap_err();
        match err {
            super::Error::UnacceptableFilename { filename } => {
                assert_eq!(filename, "bad�name");
            }
            _ => panic!("expected UnacceptableFilename error"),
        }
    }
}
