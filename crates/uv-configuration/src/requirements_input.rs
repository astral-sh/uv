use std::path::{Path, PathBuf};
use std::str::FromStr;

use thiserror::Error;

use uv_fs::Simplified;
use uv_pep508::split_scheme;
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};

/// A requirements input.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum RequirementsInput {
    /// Requirements read from stdin.
    Stdin,
    /// A local requirements input.
    Local(PathBuf),
    /// A remote requirements input.
    Remote(DisplaySafeUrl),
}

impl RequirementsInput {
    /// Return `true` if the input represents stdin.
    pub fn is_stdin(&self) -> bool {
        matches!(self, Self::Stdin)
    }

    /// Resolve a nested input relative to this input.
    ///
    /// Local inputs are resolved against the containing file's directory, and inputs referenced
    /// from stdin are resolved against the working directory. Remote inputs are resolved using URL
    /// reference resolution.
    pub fn resolve(&self, input: &str, working_dir: &Path) -> Result<Self, RequirementsInputError> {
        match (self, input.parse()?) {
            (_, Self::Stdin) => Ok(Self::Stdin),
            (_, Self::Remote(url)) => Ok(Self::Remote(url)),
            (Self::Local(_), Self::Local(path)) if path.is_absolute() => Ok(Self::Local(path)),
            (Self::Local(parent), Self::Local(path)) => {
                let parent = parent
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or(working_dir);
                Ok(Self::Local(parent.join(path)))
            }
            (Self::Stdin, Self::Local(path)) => Ok(Self::Local(working_dir.join(path))),
            (Self::Remote(_), Self::Local(path)) if split_scheme(input).is_some() => {
                Ok(Self::Local(path))
            }
            (Self::Remote(url), Self::Local(_)) => Ok(Self::Remote(url.join(input)?)),
        }
    }
}

impl From<PathBuf> for RequirementsInput {
    fn from(path: PathBuf) -> Self {
        if path == Path::new("-") {
            Self::Stdin
        } else {
            Self::Local(path)
        }
    }
}

impl From<&Path> for RequirementsInput {
    fn from(path: &Path) -> Self {
        path.to_path_buf().into()
    }
}

impl From<&PathBuf> for RequirementsInput {
    fn from(path: &PathBuf) -> Self {
        path.clone().into()
    }
}

impl FromStr for RequirementsInput {
    type Err = RequirementsInputError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let Some((scheme, rest)) = split_scheme(input) else {
            return Ok(PathBuf::from(input).into());
        };

        // Avoid interpreting Windows drive paths as URLs on other platforms.
        if scheme.len() == 1 && (rest.starts_with('/') || rest.starts_with('\\')) {
            return Ok(Self::Local(PathBuf::from(input)));
        }

        let url = DisplaySafeUrl::parse(input)?;
        if url.scheme() == "file" {
            let path = url
                .to_file_path()
                .map_err(|()| RequirementsInputError::InvalidFileUrl(url))?;
            Ok(Self::Local(path))
        } else {
            Ok(Self::Remote(url))
        }
    }
}

impl std::fmt::Display for RequirementsInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stdin => f.write_str("-"),
            Self::Local(path) => path.user_display().fmt(f),
            Self::Remote(url) => url.fmt(f),
        }
    }
}

/// An error that can occur when parsing a [`RequirementsInput`].
#[derive(Debug, Error)]
pub enum RequirementsInputError {
    /// The input is not a valid URL.
    #[error(transparent)]
    Url(#[from] DisplaySafeUrlError),
    /// A `file://` URL could not be converted to a local path.
    #[error("invalid file URL: `{0}`")]
    InvalidFileUrl(DisplaySafeUrl),
}
