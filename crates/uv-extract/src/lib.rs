use std::{fmt::Display, sync::LazyLock};

pub use error::Error;
use regex::Regex;
pub use sync::*;
use uv_static::EnvVars;

mod error;
pub mod hash;
pub mod stream;
mod sync;
mod vendor;

static CONTROL_CHARACTERS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{C}").unwrap());
static REPLACEMENT_CHARACTER: &str = "\u{FFFD}";

/// Compression methods that we consider supported.
///
/// Our underlying ZIP dependencies may support more.
pub(crate) enum CompressionMethod {
    Stored,
    Deflated,
    Zstd,
    // NOTE: This will become `Unsupported(...)` in the future.
    Deprecated(&'static str),
}

impl CompressionMethod {
    /// Returns `true` if this is a well-known compression method that we
    /// expect other ZIP implementations to support.
    pub(crate) fn is_well_known(&self) -> bool {
        matches!(self, Self::Stored | Self::Deflated | Self::Zstd)
    }
}

impl Display for CompressionMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stored => write!(f, "stored"),
            Self::Deflated => write!(f, "DEFLATE"),
            Self::Zstd => write!(f, "zstd"),
            Self::Deprecated(name) => write!(f, "{name}"),
        }
    }
}

impl From<async_zip::Compression> for CompressionMethod {
    fn from(value: async_zip::Compression) -> Self {
        match value {
            async_zip::Compression::Stored => Self::Stored,
            async_zip::Compression::Deflate => Self::Deflated,
            async_zip::Compression::Zstd => Self::Zstd,
            async_zip::Compression::Bz => Self::Deprecated("bzip2"),
            async_zip::Compression::Lzma => Self::Deprecated("lzma"),
            async_zip::Compression::Xz => Self::Deprecated("xz"),
            _ => Self::Deprecated("unknown"),
        }
    }
}

impl From<zip::CompressionMethod> for CompressionMethod {
    fn from(value: zip::CompressionMethod) -> Self {
        match value {
            zip::CompressionMethod::Stored => Self::Stored,
            zip::CompressionMethod::Deflated => Self::Deflated,
            zip::CompressionMethod::Zstd => Self::Zstd,
            zip::CompressionMethod::Bzip2 => Self::Deprecated("bzip2"),
            zip::CompressionMethod::Lzma => Self::Deprecated("lzma"),
            zip::CompressionMethod::Xz => Self::Deprecated("xz"),
            _ => Self::Deprecated("unknown"),
        }
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
