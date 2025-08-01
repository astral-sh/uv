use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

use thiserror::Error;
use url::{ParseError, Url};
use uv_cache_key::{CacheKey, CacheKeyHasher};

use uv_distribution_filename::{DistExtension, ExtensionError};
use uv_git_types::{GitUrl, GitUrlParseError};
use uv_pep508::{
    Pep508Url, UnnamedRequirementUrl, VerbatimUrl, VerbatimUrlError, looks_like_git_repository,
};
use uv_redacted::DisplaySafeUrl;

use crate::{ArchiveInfo, DirInfo, DirectUrl, VcsInfo, VcsKind};

#[derive(Debug, Error)]
pub enum ParsedUrlError {
    #[error("Unsupported URL prefix `{prefix}` in URL: `{url}` ({message})")]
    UnsupportedUrlPrefix {
        prefix: String,
        url: String,
        message: &'static str,
    },
    #[error("Invalid path in file URL: `{0}`")]
    InvalidFileUrl(String),
    #[error(transparent)]
    GitUrlParse(#[from] GitUrlParseError),
    #[error("Not a valid URL: `{0}`")]
    UrlParse(String, #[source] ParseError),
    #[error(transparent)]
    VerbatimUrl(#[from] VerbatimUrlError),
    #[error(
        "Direct URL (`{0}`) references a Git repository, but is missing the `git+` prefix (e.g., `git+{0}`)"
    )]
    MissingGitPrefix(String),
    #[error("Expected direct URL (`{0}`) to end in a supported file extension: {1}")]
    MissingExtensionUrl(String, ExtensionError),
    #[error("Expected path (`{0}`) to end in a supported file extension: {1}")]
    MissingExtensionPath(PathBuf, ExtensionError),
}

#[derive(Debug, Clone, Hash, PartialEq, PartialOrd, Eq, Ord)]
pub struct VerbatimParsedUrl {
    pub parsed_url: ParsedUrl,
    pub verbatim: VerbatimUrl,
}

impl CacheKey for VerbatimParsedUrl {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.verbatim.cache_key(state);
    }
}

impl VerbatimParsedUrl {
    /// Returns `true` if the URL is editable.
    pub fn is_editable(&self) -> bool {
        self.parsed_url.is_editable()
    }
}

impl Pep508Url for VerbatimParsedUrl {
    type Err = ParsedUrlError;

    fn parse_url(url: &str, working_dir: Option<&Path>) -> Result<Self, Self::Err> {
        let verbatim = <VerbatimUrl as Pep508Url>::parse_url(url, working_dir)?;
        Ok(Self {
            parsed_url: ParsedUrl::try_from(verbatim.to_url())?,
            verbatim,
        })
    }

    fn displayable_with_credentials(&self) -> impl Display {
        self.verbatim.displayable_with_credentials()
    }
}

impl UnnamedRequirementUrl for VerbatimParsedUrl {
    fn parse_path(
        path: impl AsRef<Path>,
        working_dir: impl AsRef<Path>,
    ) -> Result<Self, Self::Err> {
        let verbatim = VerbatimUrl::from_path(&path, &working_dir)?;
        let verbatim_path = verbatim.as_path()?;
        let is_dir = if let Ok(metadata) = verbatim_path.metadata() {
            metadata.is_dir()
        } else {
            verbatim_path.extension().is_none()
        };
        let url = verbatim.to_url();
        let install_path = verbatim.as_path()?.into_boxed_path();
        let parsed_url = if is_dir {
            ParsedUrl::Directory(ParsedDirectoryUrl {
                url,
                install_path,
                editable: None,
                r#virtual: None,
            })
        } else {
            ParsedUrl::Path(ParsedPathUrl {
                url,
                install_path,
                ext: DistExtension::from_path(&path).map_err(|err| {
                    ParsedUrlError::MissingExtensionPath(path.as_ref().to_path_buf(), err)
                })?,
            })
        };
        Ok(Self {
            parsed_url,
            verbatim,
        })
    }

    fn parse_absolute_path(path: impl AsRef<Path>) -> Result<Self, Self::Err> {
        let verbatim = VerbatimUrl::from_absolute_path(&path)?;
        let verbatim_path = verbatim.as_path()?;
        let is_dir = if let Ok(metadata) = verbatim_path.metadata() {
            metadata.is_dir()
        } else {
            verbatim_path.extension().is_none()
        };
        let url = verbatim.to_url();
        let install_path = verbatim.as_path()?.into_boxed_path();
        let parsed_url = if is_dir {
            ParsedUrl::Directory(ParsedDirectoryUrl {
                url,
                install_path,
                editable: None,
                r#virtual: None,
            })
        } else {
            ParsedUrl::Path(ParsedPathUrl {
                url,
                install_path,
                ext: DistExtension::from_path(&path).map_err(|err| {
                    ParsedUrlError::MissingExtensionPath(path.as_ref().to_path_buf(), err)
                })?,
            })
        };
        Ok(Self {
            parsed_url,
            verbatim,
        })
    }

    fn parse_unnamed_url(url: impl AsRef<str>) -> Result<Self, Self::Err> {
        let verbatim = <VerbatimUrl as UnnamedRequirementUrl>::parse_unnamed_url(&url)?;
        Ok(Self {
            parsed_url: ParsedUrl::try_from(verbatim.to_url())?,
            verbatim,
        })
    }

    fn with_given(self, given: impl AsRef<str>) -> Self {
        Self {
            verbatim: self.verbatim.with_given(given),
            ..self
        }
    }

    fn given(&self) -> Option<&str> {
        self.verbatim.given()
    }
}

impl Display for VerbatimParsedUrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.verbatim, f)
    }
}

/// We support three types of URLs for distributions:
/// * The path to a file or directory (`file://`)
/// * A Git repository (`git+https://` or `git+ssh://`), optionally with a subdirectory and/or
///   string to checkout.
/// * A remote archive (`https://`), optional with a subdirectory (source dist only).
///
/// A URL in a requirement `foo @ <url>` must be one of the above.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Hash, Ord)]
pub enum ParsedUrl {
    /// The direct URL is a path to a local file.
    Path(ParsedPathUrl),
    /// The direct URL is a path to a local directory.
    Directory(ParsedDirectoryUrl),
    /// The direct URL is path to a Git repository.
    Git(ParsedGitUrl),
    /// The direct URL is a URL to a source archive (e.g., a `.tar.gz` file) or built archive
    /// (i.e., a `.whl` file).
    Archive(ParsedArchiveUrl),
}

impl ParsedUrl {
    /// Returns `true` if the URL is editable.
    pub fn is_editable(&self) -> bool {
        matches!(
            self,
            Self::Directory(ParsedDirectoryUrl {
                editable: Some(true),
                ..
            })
        )
    }
}

/// A local path URL for a file (i.e., a built or source distribution).
///
/// Examples:
/// * `file:///home/ferris/my_project/my_project-0.1.0.tar.gz`
/// * `file:///home/ferris/my_project/my_project-0.1.0-py3-none-any.whl`
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Hash, Ord)]
pub struct ParsedPathUrl {
    pub url: DisplaySafeUrl,
    /// The absolute path to the distribution which we use for installing.
    pub install_path: Box<Path>,
    /// The file extension, e.g. `tar.gz`, `zip`, etc.
    pub ext: DistExtension,
}

impl ParsedPathUrl {
    /// Construct a [`ParsedPathUrl`] from a path requirement source.
    pub fn from_source(install_path: Box<Path>, ext: DistExtension, url: DisplaySafeUrl) -> Self {
        Self {
            url,
            install_path,
            ext,
        }
    }
}

/// A local path URL for a source directory.
///
/// Examples:
/// * `file:///home/ferris/my_project`
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Hash, Ord)]
pub struct ParsedDirectoryUrl {
    pub url: DisplaySafeUrl,
    /// The absolute path to the distribution which we use for installing.
    pub install_path: Box<Path>,
    /// Whether the project at the given URL should be installed in editable mode.
    pub editable: Option<bool>,
    /// Whether the project at the given URL should be treated as a virtual package.
    pub r#virtual: Option<bool>,
}

impl ParsedDirectoryUrl {
    /// Construct a [`ParsedDirectoryUrl`] from a path requirement source.
    pub fn from_source(
        install_path: Box<Path>,
        editable: Option<bool>,
        r#virtual: Option<bool>,
        url: DisplaySafeUrl,
    ) -> Self {
        Self {
            url,
            install_path,
            editable,
            r#virtual,
        }
    }
}

/// A Git repository URL.
///
/// Examples:
/// * `git+https://git.example.com/MyProject.git`
/// * `git+https://git.example.com/MyProject.git@v1.0#egg=pkg&subdirectory=pkg_dir`
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Hash, Ord)]
pub struct ParsedGitUrl {
    pub url: GitUrl,
    pub subdirectory: Option<Box<Path>>,
}

impl ParsedGitUrl {
    /// Construct a [`ParsedGitUrl`] from a Git requirement source.
    pub fn from_source(url: GitUrl, subdirectory: Option<Box<Path>>) -> Self {
        Self { url, subdirectory }
    }
}

impl TryFrom<DisplaySafeUrl> for ParsedGitUrl {
    type Error = ParsedUrlError;

    /// Supports URLs with and without the `git+` prefix.
    ///
    /// When the URL includes a prefix, it's presumed to come from a PEP 508 requirement; when it's
    /// excluded, it's presumed to come from `tool.uv.sources`.
    fn try_from(url_in: DisplaySafeUrl) -> Result<Self, Self::Error> {
        let subdirectory = get_subdirectory(&url_in).map(PathBuf::into_boxed_path);

        let url = url_in
            .as_str()
            .strip_prefix("git+")
            .unwrap_or(url_in.as_str());
        let url = DisplaySafeUrl::parse(url)
            .map_err(|err| ParsedUrlError::UrlParse(url.to_string(), err))?;
        let url = GitUrl::try_from(url)?;
        Ok(Self { url, subdirectory })
    }
}

/// A URL to a source or built archive.
///
/// Examples:
/// * A built distribution: `https://files.pythonhosted.org/packages/62/06/d5604a70d160f6a6ca5fd2ba25597c24abd5c5ca5f437263d177ac242308/tqdm-4.66.1-py2.py3-none-any.whl`
/// * A source distribution with a valid name: `https://files.pythonhosted.org/packages/62/06/d5604a70d160f6a6ca5fd2ba25597c24abd5c5ca5f437263d177ac242308/tqdm-4.66.1.tar.gz`
/// * A source dist with a recognizable extension but invalid name: `https://github.com/foo-labs/foo/archive/master.zip#egg=pkg&subdirectory=packages/bar`
#[derive(Debug, Clone, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct ParsedArchiveUrl {
    pub url: DisplaySafeUrl,
    pub subdirectory: Option<Box<Path>>,
    pub ext: DistExtension,
}

impl ParsedArchiveUrl {
    /// Construct a [`ParsedArchiveUrl`] from a URL requirement source.
    pub fn from_source(
        location: DisplaySafeUrl,
        subdirectory: Option<Box<Path>>,
        ext: DistExtension,
    ) -> Self {
        Self {
            url: location,
            subdirectory,
            ext,
        }
    }
}

impl TryFrom<DisplaySafeUrl> for ParsedArchiveUrl {
    type Error = ParsedUrlError;

    fn try_from(mut url: DisplaySafeUrl) -> Result<Self, Self::Error> {
        // Extract the `#subdirectory` fragment, if present.
        let subdirectory = get_subdirectory(&url).map(PathBuf::into_boxed_path);
        url.set_fragment(None);

        // Infer the extension from the path.
        let ext = match DistExtension::from_path(url.path()) {
            Ok(ext) => ext,
            Err(..) if looks_like_git_repository(&url) => {
                return Err(ParsedUrlError::MissingGitPrefix(url.to_string()));
            }
            Err(err) => return Err(ParsedUrlError::MissingExtensionUrl(url.to_string(), err)),
        };

        Ok(Self {
            url,
            subdirectory,
            ext,
        })
    }
}

/// If the URL points to a subdirectory, extract it, as in (git):
///   `git+https://git.example.com/MyProject.git@v1.0#subdirectory=pkg_dir`
///   `git+https://git.example.com/MyProject.git@v1.0#egg=pkg&subdirectory=pkg_dir`
/// or (direct archive url):
///   `https://github.com/foo-labs/foo/archive/master.zip#subdirectory=packages/bar`
///   `https://github.com/foo-labs/foo/archive/master.zip#egg=pkg&subdirectory=packages/bar`
fn get_subdirectory(url: &Url) -> Option<PathBuf> {
    let fragment = url.fragment()?;
    let subdirectory = fragment
        .split('&')
        .find_map(|fragment| fragment.strip_prefix("subdirectory="))?;
    Some(PathBuf::from(subdirectory))
}

impl TryFrom<DisplaySafeUrl> for ParsedUrl {
    type Error = ParsedUrlError;

    fn try_from(url: DisplaySafeUrl) -> Result<Self, Self::Error> {
        if let Some((prefix, ..)) = url.scheme().split_once('+') {
            match prefix {
                "git" => Ok(Self::Git(ParsedGitUrl::try_from(url)?)),
                "bzr" => Err(ParsedUrlError::UnsupportedUrlPrefix {
                    prefix: prefix.to_string(),
                    url: url.to_string(),
                    message: "Bazaar is not supported",
                }),
                "hg" => Err(ParsedUrlError::UnsupportedUrlPrefix {
                    prefix: prefix.to_string(),
                    url: url.to_string(),
                    message: "Mercurial is not supported",
                }),
                "svn" => Err(ParsedUrlError::UnsupportedUrlPrefix {
                    prefix: prefix.to_string(),
                    url: url.to_string(),
                    message: "Subversion is not supported",
                }),
                _ => Err(ParsedUrlError::UnsupportedUrlPrefix {
                    prefix: prefix.to_string(),
                    url: url.to_string(),
                    message: "Unknown scheme",
                }),
            }
        } else if Path::new(url.path())
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("git"))
        {
            Ok(Self::Git(ParsedGitUrl::try_from(url)?))
        } else if url.scheme().eq_ignore_ascii_case("file") {
            let path = url
                .to_file_path()
                .map_err(|()| ParsedUrlError::InvalidFileUrl(url.to_string()))?;
            let is_dir = if let Ok(metadata) = path.metadata() {
                metadata.is_dir()
            } else {
                path.extension().is_none()
            };
            if is_dir {
                Ok(Self::Directory(ParsedDirectoryUrl {
                    url,
                    install_path: path.into_boxed_path(),
                    editable: None,
                    r#virtual: None,
                }))
            } else {
                Ok(Self::Path(ParsedPathUrl {
                    url,
                    ext: DistExtension::from_path(&path)
                        .map_err(|err| ParsedUrlError::MissingExtensionPath(path.clone(), err))?,
                    install_path: path.into_boxed_path(),
                }))
            }
        } else {
            Ok(Self::Archive(ParsedArchiveUrl::try_from(url)?))
        }
    }
}

impl From<&ParsedUrl> for DirectUrl {
    fn from(value: &ParsedUrl) -> Self {
        match value {
            ParsedUrl::Path(value) => Self::from(value),
            ParsedUrl::Directory(value) => Self::from(value),
            ParsedUrl::Git(value) => Self::from(value),
            ParsedUrl::Archive(value) => Self::from(value),
        }
    }
}

impl From<&ParsedPathUrl> for DirectUrl {
    fn from(value: &ParsedPathUrl) -> Self {
        Self::ArchiveUrl {
            url: value.url.to_string(),
            archive_info: ArchiveInfo {
                hash: None,
                hashes: None,
            },
            subdirectory: None,
        }
    }
}

impl From<&ParsedDirectoryUrl> for DirectUrl {
    fn from(value: &ParsedDirectoryUrl) -> Self {
        Self::LocalDirectory {
            url: value.url.to_string(),
            dir_info: DirInfo {
                editable: value.editable,
            },
            subdirectory: None,
        }
    }
}

impl From<&ParsedArchiveUrl> for DirectUrl {
    fn from(value: &ParsedArchiveUrl) -> Self {
        Self::ArchiveUrl {
            url: value.url.to_string(),
            archive_info: ArchiveInfo {
                hash: None,
                hashes: None,
            },
            subdirectory: value.subdirectory.clone(),
        }
    }
}

impl From<&ParsedGitUrl> for DirectUrl {
    fn from(value: &ParsedGitUrl) -> Self {
        Self::VcsUrl {
            url: value.url.repository().to_string(),
            vcs_info: VcsInfo {
                vcs: VcsKind::Git,
                commit_id: value.url.precise().as_ref().map(ToString::to_string),
                requested_revision: value.url.reference().as_str().map(ToString::to_string),
            },
            subdirectory: value.subdirectory.clone(),
        }
    }
}

impl From<ParsedUrl> for DisplaySafeUrl {
    fn from(value: ParsedUrl) -> Self {
        match value {
            ParsedUrl::Path(value) => value.into(),
            ParsedUrl::Directory(value) => value.into(),
            ParsedUrl::Git(value) => value.into(),
            ParsedUrl::Archive(value) => value.into(),
        }
    }
}

impl From<ParsedPathUrl> for DisplaySafeUrl {
    fn from(value: ParsedPathUrl) -> Self {
        value.url
    }
}

impl From<ParsedDirectoryUrl> for DisplaySafeUrl {
    fn from(value: ParsedDirectoryUrl) -> Self {
        value.url
    }
}

impl From<ParsedArchiveUrl> for DisplaySafeUrl {
    fn from(value: ParsedArchiveUrl) -> Self {
        let mut url = value.url;
        if let Some(subdirectory) = value.subdirectory {
            url.set_fragment(Some(&format!("subdirectory={}", subdirectory.display())));
        }
        url
    }
}

impl From<ParsedGitUrl> for DisplaySafeUrl {
    fn from(value: ParsedGitUrl) -> Self {
        let mut url = Self::parse(&format!("{}{}", "git+", Self::from(value.url).as_str()))
            .expect("Git URL is invalid");
        if let Some(subdirectory) = value.subdirectory {
            url.set_fragment(Some(&format!("subdirectory={}", subdirectory.display())));
        }
        url
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::parsed_url::ParsedUrl;
    use uv_redacted::DisplaySafeUrl;

    #[test]
    fn direct_url_from_url() -> Result<()> {
        let expected = DisplaySafeUrl::parse("git+https://github.com/pallets/flask.git")?;
        let actual = DisplaySafeUrl::from(ParsedUrl::try_from(expected.clone())?);
        assert_eq!(expected, actual);

        let expected =
            DisplaySafeUrl::parse("git+https://github.com/pallets/flask.git#subdirectory=pkg_dir")?;
        let actual = DisplaySafeUrl::from(ParsedUrl::try_from(expected.clone())?);
        assert_eq!(expected, actual);

        let expected = DisplaySafeUrl::parse("git+https://github.com/pallets/flask.git@2.0.0")?;
        let actual = DisplaySafeUrl::from(ParsedUrl::try_from(expected.clone())?);
        assert_eq!(expected, actual);

        let expected = DisplaySafeUrl::parse(
            "git+https://github.com/pallets/flask.git@2.0.0#subdirectory=pkg_dir",
        )?;
        let actual = DisplaySafeUrl::from(ParsedUrl::try_from(expected.clone())?);
        assert_eq!(expected, actual);

        // TODO(charlie): Preserve other fragments.
        let expected = DisplaySafeUrl::parse(
            "git+https://github.com/pallets/flask.git#egg=flask&subdirectory=pkg_dir",
        )?;
        let actual = DisplaySafeUrl::from(ParsedUrl::try_from(expected.clone())?);
        assert_ne!(expected, actual);

        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn direct_url_from_url_absolute() -> Result<()> {
        let expected = DisplaySafeUrl::parse("file:///path/to/directory")?;
        let actual = DisplaySafeUrl::from(ParsedUrl::try_from(expected.clone())?);
        assert_eq!(expected, actual);
        Ok(())
    }
}
