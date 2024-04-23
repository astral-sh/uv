use std::path::PathBuf;

use thiserror::Error;
use url::Url;

use pep508_rs::VerbatimUrl;
use uv_git::GitReference;

/// We support three types of URLs for distributions:
/// * The path to a file or directory (`file://`)
/// * A git repository (`git+https://` or `git+ssh://`), optionally with a subdirectory and/or
///   string to checkout.
/// * A remote archive (`https://`), optional with a subdirectory (source dist only)
/// A URL in a requirement `foo @ <url>` must be one of the above.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum DecomposedUrl {
    /// The direct URL is a path to a local directory or file.
    LocalFile(DecomposedFileUrl),
    /// The direct URL is path to a Git repository.
    Git(DecomposedGitUrl),
    /// The direct URL is a URL to an archive.
    Archive(DecomposedArchiveUrl),
}

#[derive(Debug, Error)]
pub enum DecomposedUrlParseError {
    #[error("Unsupported URL prefix `{prefix}` in URL: `{url}`")]
    UnsupportedUrlPrefix { prefix: String, url: Url },
    #[error("Invalid path in file URL: `{0}`")]
    InvalidFileUrl(Url),
    #[error("Failed to parse Git reference from URL: `{0}`")]
    GitShaParse(Url, #[source] git2::Error),
    #[error("Not a valid URL: `{0}`")]
    UrlParse(String, #[source] url::ParseError),
}

impl TryFrom<VerbatimUrl> for DecomposedUrl {
    type Error = DecomposedUrlParseError;

    fn try_from(url: VerbatimUrl) -> Result<Self, Self::Error> {
        if let Some((prefix, rest)) = url.scheme().split_once('+') {
            if prefix != "git" {
                return Err(Self::Error::UnsupportedUrlPrefix {
                    prefix: prefix.to_string(),
                    url: url.to_url(),
                });
            }
            let mut repository = Url::parse(rest).expect("TODO(konsti)");
            let subdirectory = get_subdirectory(&repository);

            // Remove any query parameters and fragments.
            repository.set_fragment(None);
            repository.set_query(None);

            // If the URL ends with a reference, like `https://git.example.com/MyProject.git@v1.0`,
            // extract it.
            let mut reference = GitReference::DefaultBranch;
            if let Some((prefix, suffix)) = url
                .path()
                .rsplit_once('@')
                .map(|(prefix, suffix)| (prefix.to_string(), suffix.to_string()))
            {
                reference = GitReference::from_rev(&suffix);
                repository.set_path(&prefix);
            }

            Ok(Self::Git(DecomposedGitUrl {
                url: repository,
                subdirectory,
                reference,
                given: url.given().map(ToString::to_string),
            }))
        } else if url.scheme().eq_ignore_ascii_case("file") {
            // TODO(konsti): Store canonicalized path?
            let path = url
                .to_url()
                .to_file_path()
                .map_err(|()| Self::Error::InvalidFileUrl(url.to_url()))?;
            Ok(Self::LocalFile(DecomposedFileUrl {
                url: url.to_url(),
                path,
                subdirectory: get_subdirectory(&url),
                given: url.given().map(ToString::to_string),
            }))
        } else {
            Ok(Self::Archive(DecomposedArchiveUrl {
                url: url.to_url(),
                subdirectory: get_subdirectory(&url),
                given: url.given().map(ToString::to_string),
            }))
        }
    }
}

impl std::fmt::Display for DecomposedUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.to_verbatim_url(), f)
    }
}

impl DecomposedUrl {
    pub fn to_verbatim_url(&self) -> VerbatimUrl {
        match self {
            DecomposedUrl::LocalFile(file_url) => file_url.to_verbatim_url(),
            DecomposedUrl::Git(git_url) => {
                let mut url = git_url.url.clone();
                if let Some(rev) = git_url.reference.as_str() {
                    url.set_path(&format!("{}@{}", url.path(), rev));
                }
                if let Some(subdirectory) = &git_url.subdirectory {
                    url.set_fragment(Some(&format!("subdirectory={subdirectory}")));
                }
                VerbatimUrl::new(url, git_url.given.clone())
            }
            DecomposedUrl::Archive(archive_url) => {
                VerbatimUrl::new(archive_url.url.clone(), archive_url.given.clone())
            }
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct DecomposedFileUrl {
    pub url: Url,
    pub path: PathBuf,
    pub subdirectory: Option<String>,
    pub given: Option<String>,
}

impl DecomposedFileUrl {
    pub fn to_verbatim_url(&self) -> VerbatimUrl {
        VerbatimUrl::new(self.url.clone(), self.given.clone())
    }
}

impl std::fmt::Display for DecomposedFileUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.to_verbatim_url(), f)
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct DecomposedGitUrl {
    pub url: Url,
    pub subdirectory: Option<String>,
    pub reference: GitReference,
    pub given: Option<String>,
}

impl DecomposedGitUrl {
    pub fn to_verbatim_url(&self) -> VerbatimUrl {
        let mut url = self.url.clone();
        if let Some(rev) = self.reference.as_str() {
            url.set_path(&format!("{}@{}", url.path(), rev));
        }
        if let Some(subdirectory) = &self.subdirectory {
            url.set_fragment(Some(&format!("subdirectory={subdirectory}")));
        }
        VerbatimUrl::new(url, self.given.clone())
    }
}

impl std::fmt::Display for DecomposedGitUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.to_verbatim_url(), f)
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct DecomposedArchiveUrl {
    pub url: Url,
    pub subdirectory: Option<String>,
    pub given: Option<String>,
}

impl DecomposedArchiveUrl {
    pub fn to_verbatim_url(&self) -> VerbatimUrl {
        VerbatimUrl::new(self.url.clone(), self.given.clone())
    }
}

impl std::fmt::Display for DecomposedArchiveUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.to_verbatim_url(), f)
    }
}

/// If the URL points to a subdirectory, extract it, as in (git):
///   `git+https://git.example.com/MyProject.git@v1.0#subdirectory=pkg_dir`
///   `git+https://git.example.com/MyProject.git@v1.0#egg=pkg&subdirectory=pkg_dir`
/// or (direct archive url):
///   `https://github.com/foo-labs/foo/archive/master.zip#subdirectory=packages/bar`
///   `https://github.com/foo-labs/foo/archive/master.zip#egg=pkg&subdirectory=packages/bar`
fn get_subdirectory(url: &Url) -> Option<String> {
    let fragment = url.fragment()?;
    let subdirectory = fragment
        .split('&')
        .find_map(|fragment| fragment.strip_prefix("subdirectory="))?;
    Some(subdirectory.to_string())
}
