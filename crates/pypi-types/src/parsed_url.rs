use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

use thiserror::Error;
use url::{ParseError, Url};

use pep508_rs::{Pep508Url, UnnamedRequirementUrl, VerbatimUrl, VerbatimUrlError};
use uv_git::{GitReference, GitSha, GitUrl, OidParseError};

use crate::{ArchiveInfo, DirInfo, DirectUrl, VcsInfo, VcsKind};

#[derive(Debug, Error)]
pub enum ParsedUrlError {
    #[error("Unsupported URL prefix `{prefix}` in URL: `{url}` ({message})")]
    UnsupportedUrlPrefix {
        prefix: String,
        url: Url,
        message: &'static str,
    },
    #[error("Invalid path in file URL: `{0}`")]
    InvalidFileUrl(Url),
    #[error("Failed to parse Git reference from URL: `{0}`")]
    GitShaParse(Url, #[source] OidParseError),
    #[error("Not a valid URL: `{0}`")]
    UrlParse(String, #[source] ParseError),
    #[error(transparent)]
    VerbatimUrl(#[from] VerbatimUrlError),
}

#[derive(Debug, Clone, Hash, PartialEq, PartialOrd, Eq, Ord)]
pub struct VerbatimParsedUrl {
    pub parsed_url: ParsedUrl,
    pub verbatim: VerbatimUrl,
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
}

impl UnnamedRequirementUrl for VerbatimParsedUrl {
    fn parse_path(
        path: impl AsRef<Path>,
        working_dir: impl AsRef<Path>,
    ) -> Result<Self, Self::Err> {
        let verbatim = VerbatimUrl::parse_path(&path, &working_dir)?;
        let verbatim_path = verbatim.as_path()?;
        let is_dir = if let Ok(metadata) = verbatim_path.metadata() {
            metadata.is_dir()
        } else {
            verbatim_path.extension().is_none()
        };
        let parsed_url = if is_dir {
            ParsedUrl::Directory(ParsedDirectoryUrl {
                url: verbatim.to_url(),
                install_path: verbatim.as_path()?,
                lock_path: path.as_ref().to_path_buf(),
                editable: false,
            })
        } else {
            ParsedUrl::Path(ParsedPathUrl {
                url: verbatim.to_url(),
                install_path: verbatim.as_path()?,
                lock_path: path.as_ref().to_path_buf(),
            })
        };
        Ok(Self {
            parsed_url,
            verbatim,
        })
    }

    fn parse_absolute_path(path: impl AsRef<Path>) -> Result<Self, Self::Err> {
        let verbatim = VerbatimUrl::parse_absolute_path(&path)?;
        let verbatim_path = verbatim.as_path()?;
        let is_dir = if let Ok(metadata) = verbatim_path.metadata() {
            metadata.is_dir()
        } else {
            verbatim_path.extension().is_none()
        };
        let parsed_url = if is_dir {
            ParsedUrl::Directory(ParsedDirectoryUrl {
                url: verbatim.to_url(),
                install_path: verbatim.as_path()?,
                lock_path: path.as_ref().to_path_buf(),
                editable: false,
            })
        } else {
            ParsedUrl::Path(ParsedPathUrl {
                url: verbatim.to_url(),
                install_path: verbatim.as_path()?,
                lock_path: path.as_ref().to_path_buf(),
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

    fn with_given(self, given: impl Into<String>) -> Self {
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

impl TryFrom<VerbatimUrl> for VerbatimParsedUrl {
    type Error = ParsedUrlError;

    fn try_from(verbatim_url: VerbatimUrl) -> Result<Self, Self::Error> {
        let parsed_url = ParsedUrl::try_from(verbatim_url.to_url())?;
        Ok(Self {
            parsed_url,
            verbatim: verbatim_url,
        })
    }
}

impl serde::ser::Serialize for VerbatimParsedUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.verbatim.serialize(serializer)
    }
}

impl<'de> serde::de::Deserialize<'de> for VerbatimParsedUrl {
    fn deserialize<D>(deserializer: D) -> Result<VerbatimParsedUrl, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let verbatim_url = VerbatimUrl::deserialize(deserializer)?;
        Self::try_from(verbatim_url).map_err(serde::de::Error::custom)
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
            Self::Directory(ParsedDirectoryUrl { editable: true, .. })
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
    pub url: Url,
    /// The resolved, absolute path to the distribution which we use for installing.
    pub install_path: PathBuf,
    /// The absolute path or path relative to the workspace root pointing to the distribution
    /// which we use for locking. Unlike `given` on the verbatim URL all environment variables
    /// are resolved, and unlike the install path, we did not yet join it on the base directory.
    pub lock_path: PathBuf,
}

impl ParsedPathUrl {
    /// Construct a [`ParsedPathUrl`] from a path requirement source.
    pub fn from_source(install_path: PathBuf, lock_path: PathBuf, url: Url) -> Self {
        Self {
            url,
            install_path,
            lock_path,
        }
    }
}

/// A local path URL for a source directory.
///
/// Examples:
/// * `file:///home/ferris/my_project`
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Hash, Ord)]
pub struct ParsedDirectoryUrl {
    pub url: Url,
    /// The resolved, absolute path to the distribution which we use for installing.
    pub install_path: PathBuf,
    /// The absolute path or path relative to the workspace root pointing to the distribution
    /// which we use for locking. Unlike `given` on the verbatim URL all environment variables
    /// are resolved, and unlike the install path, we did not yet join it on the base directory.
    pub lock_path: PathBuf,
    pub editable: bool,
}

impl ParsedDirectoryUrl {
    /// Construct a [`ParsedDirectoryUrl`] from a path requirement source.
    pub fn from_source(
        install_path: PathBuf,
        lock_path: PathBuf,
        editable: bool,
        url: Url,
    ) -> Self {
        Self {
            url,
            install_path,
            lock_path,
            editable,
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
    pub subdirectory: Option<PathBuf>,
}

impl ParsedGitUrl {
    /// Construct a [`ParsedGitUrl`] from a Git requirement source.
    pub fn from_source(
        repository: Url,
        reference: GitReference,
        precise: Option<GitSha>,
        subdirectory: Option<PathBuf>,
    ) -> Self {
        let mut url = GitUrl::new(repository, reference);
        if let Some(precise) = precise {
            url = url.with_precise(precise);
        }
        Self { url, subdirectory }
    }
}

impl TryFrom<Url> for ParsedGitUrl {
    type Error = ParsedUrlError;

    /// Supports URLS with and without the `git+` prefix.
    ///
    /// When the URL includes a prefix, it's presumed to come from a PEP 508 requirement; when it's
    /// excluded, it's presumed to come from `tool.uv.sources`.
    fn try_from(url_in: Url) -> Result<Self, Self::Error> {
        let subdirectory = get_subdirectory(&url_in);

        let url = url_in
            .as_str()
            .strip_prefix("git+")
            .unwrap_or(url_in.as_str());
        let url = Url::parse(url).map_err(|err| ParsedUrlError::UrlParse(url.to_string(), err))?;
        let url = GitUrl::try_from(url)
            .map_err(|err| ParsedUrlError::GitShaParse(url_in.clone(), err))?;
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
    pub url: Url,
    pub subdirectory: Option<PathBuf>,
}

impl ParsedArchiveUrl {
    /// Construct a [`ParsedArchiveUrl`] from a URL requirement source.
    pub fn from_source(location: Url, subdirectory: Option<PathBuf>) -> Self {
        Self {
            url: location,
            subdirectory,
        }
    }
}

impl From<Url> for ParsedArchiveUrl {
    fn from(url: Url) -> Self {
        let subdirectory = get_subdirectory(&url);
        Self { url, subdirectory }
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

impl TryFrom<Url> for ParsedUrl {
    type Error = ParsedUrlError;

    fn try_from(url: Url) -> Result<Self, Self::Error> {
        if let Some((prefix, ..)) = url.scheme().split_once('+') {
            match prefix {
                "git" => Ok(Self::Git(ParsedGitUrl::try_from(url)?)),
                "bzr" => Err(ParsedUrlError::UnsupportedUrlPrefix {
                    prefix: prefix.to_string(),
                    url: url.clone(),
                    message: "Bazaar is not supported",
                }),
                "hg" => Err(ParsedUrlError::UnsupportedUrlPrefix {
                    prefix: prefix.to_string(),
                    url: url.clone(),
                    message: "Mercurial is not supported",
                }),
                "svn" => Err(ParsedUrlError::UnsupportedUrlPrefix {
                    prefix: prefix.to_string(),
                    url: url.clone(),
                    message: "Subversion is not supported",
                }),
                _ => Err(ParsedUrlError::UnsupportedUrlPrefix {
                    prefix: prefix.to_string(),
                    url: url.clone(),
                    message: "Unknown scheme",
                }),
            }
        } else if url.scheme().eq_ignore_ascii_case("file") {
            let path = url
                .to_file_path()
                .map_err(|()| ParsedUrlError::InvalidFileUrl(url.clone()))?;
            let is_dir = if let Ok(metadata) = path.metadata() {
                metadata.is_dir()
            } else {
                path.extension().is_none()
            };
            if is_dir {
                Ok(Self::Directory(ParsedDirectoryUrl {
                    url,
                    install_path: path.clone(),
                    lock_path: path,
                    editable: false,
                }))
            } else {
                Ok(Self::Path(ParsedPathUrl {
                    url,
                    install_path: path.clone(),
                    lock_path: path,
                }))
            }
        } else {
            Ok(Self::Archive(ParsedArchiveUrl::from(url)))
        }
    }
}

impl TryFrom<&ParsedUrl> for DirectUrl {
    type Error = ParsedUrlError;

    fn try_from(value: &ParsedUrl) -> Result<Self, Self::Error> {
        match value {
            ParsedUrl::Path(value) => Self::try_from(value),
            ParsedUrl::Directory(value) => Self::try_from(value),
            ParsedUrl::Git(value) => Self::try_from(value),
            ParsedUrl::Archive(value) => Self::try_from(value),
        }
    }
}

impl TryFrom<&ParsedPathUrl> for DirectUrl {
    type Error = ParsedUrlError;

    fn try_from(value: &ParsedPathUrl) -> Result<Self, Self::Error> {
        Ok(Self::ArchiveUrl {
            url: value.url.to_string(),
            archive_info: ArchiveInfo {
                hash: None,
                hashes: None,
            },
            subdirectory: None,
        })
    }
}

impl TryFrom<&ParsedDirectoryUrl> for DirectUrl {
    type Error = ParsedUrlError;

    fn try_from(value: &ParsedDirectoryUrl) -> Result<Self, Self::Error> {
        Ok(Self::LocalDirectory {
            url: value.url.to_string(),
            dir_info: DirInfo {
                editable: value.editable.then_some(true),
            },
        })
    }
}

impl TryFrom<&ParsedArchiveUrl> for DirectUrl {
    type Error = ParsedUrlError;

    fn try_from(value: &ParsedArchiveUrl) -> Result<Self, Self::Error> {
        Ok(Self::ArchiveUrl {
            url: value.url.to_string(),
            archive_info: ArchiveInfo {
                hash: None,
                hashes: None,
            },
            subdirectory: value.subdirectory.clone(),
        })
    }
}

impl TryFrom<&ParsedGitUrl> for DirectUrl {
    type Error = ParsedUrlError;

    fn try_from(value: &ParsedGitUrl) -> Result<Self, Self::Error> {
        Ok(Self::VcsUrl {
            url: value.url.repository().to_string(),
            vcs_info: VcsInfo {
                vcs: VcsKind::Git,
                commit_id: value.url.precise().as_ref().map(ToString::to_string),
                requested_revision: value.url.reference().as_str().map(ToString::to_string),
            },
            subdirectory: value.subdirectory.clone(),
        })
    }
}

impl From<ParsedUrl> for Url {
    fn from(value: ParsedUrl) -> Self {
        match value {
            ParsedUrl::Path(value) => value.into(),
            ParsedUrl::Directory(value) => value.into(),
            ParsedUrl::Git(value) => value.into(),
            ParsedUrl::Archive(value) => value.into(),
        }
    }
}

impl From<ParsedPathUrl> for Url {
    fn from(value: ParsedPathUrl) -> Self {
        value.url
    }
}

impl From<ParsedDirectoryUrl> for Url {
    fn from(value: ParsedDirectoryUrl) -> Self {
        value.url
    }
}

impl From<ParsedArchiveUrl> for Url {
    fn from(value: ParsedArchiveUrl) -> Self {
        let mut url = value.url;
        if let Some(subdirectory) = value.subdirectory {
            url.set_fragment(Some(&format!("subdirectory={}", subdirectory.display())));
        }
        url
    }
}

impl From<ParsedGitUrl> for Url {
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
    use url::Url;

    use crate::parsed_url::ParsedUrl;

    #[test]
    fn direct_url_from_url() -> Result<()> {
        let expected = Url::parse("git+https://github.com/pallets/flask.git")?;
        let actual = Url::from(ParsedUrl::try_from(expected.clone())?);
        assert_eq!(expected, actual);

        let expected = Url::parse("git+https://github.com/pallets/flask.git#subdirectory=pkg_dir")?;
        let actual = Url::from(ParsedUrl::try_from(expected.clone())?);
        assert_eq!(expected, actual);

        let expected = Url::parse("git+https://github.com/pallets/flask.git@2.0.0")?;
        let actual = Url::from(ParsedUrl::try_from(expected.clone())?);
        assert_eq!(expected, actual);

        let expected =
            Url::parse("git+https://github.com/pallets/flask.git@2.0.0#subdirectory=pkg_dir")?;
        let actual = Url::from(ParsedUrl::try_from(expected.clone())?);
        assert_eq!(expected, actual);

        // TODO(charlie): Preserve other fragments.
        let expected =
            Url::parse("git+https://github.com/pallets/flask.git#egg=flask&subdirectory=pkg_dir")?;
        let actual = Url::from(ParsedUrl::try_from(expected.clone())?);
        assert_ne!(expected, actual);

        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn direct_url_from_url_absolute() -> Result<()> {
        let expected = Url::parse("file:///path/to/directory")?;
        let actual = Url::from(ParsedUrl::try_from(expected.clone())?);
        assert_eq!(expected, actual);
        Ok(())
    }
}
