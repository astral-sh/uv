use std::path::PathBuf;

use anyhow::{Context, Error, Result};
use url::Url;

use uv_git::{GitSha, GitUrl};

#[derive(Debug)]
pub enum DirectUrl {
    /// The direct URL is a path to a local directory or file.
    LocalFile(LocalFileUrl),
    /// The direct URL is path to a Git repository.
    Git(DirectGitUrl),
    /// The direct URL is a URL to an archive.
    Archive(DirectArchiveUrl),
}

/// A local path url
///
/// Examples:
/// * `file:///home/ferris/my_project`
#[derive(Debug)]
pub struct LocalFileUrl {
    pub url: Url,
    pub editable: bool,
}

/// A git repository url
///
/// Examples:
/// * `git+https://git.example.com/MyProject.git`
/// * `git+https://git.example.com/MyProject.git@v1.0#egg=pkg&subdirectory=pkg_dir`
#[derive(Debug)]
pub struct DirectGitUrl {
    pub url: GitUrl,
    pub subdirectory: Option<PathBuf>,
}

/// An archive url
///
/// Examples:
/// * wheel: `https://download.pytorch.org/whl/torch-2.0.1-cp39-cp39-manylinux2014_aarch64.whl#sha256=423e0ae257b756bb45a4b49072046772d1ad0c592265c5080070e0767da4e490`
/// * source dist, correctly named: `https://files.pythonhosted.org/packages/62/06/d5604a70d160f6a6ca5fd2ba25597c24abd5c5ca5f437263d177ac242308/tqdm-4.66.1.tar.gz`
/// * source dist, only extension recognizable: `https://github.com/foo-labs/foo/archive/master.zip#egg=pkg&subdirectory=packages/bar`
#[derive(Debug)]
pub struct DirectArchiveUrl {
    pub url: Url,
    pub subdirectory: Option<PathBuf>,
}

impl TryFrom<&Url> for DirectGitUrl {
    type Error = Error;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        let subdirectory = get_subdirectory(url);

        let url = url
            .as_str()
            .strip_prefix("git+")
            .context("Missing git+ prefix for Git URL")?;
        let url = Url::parse(url)?;
        let url = GitUrl::try_from(url)?;
        Ok(Self { url, subdirectory })
    }
}

impl From<&Url> for DirectArchiveUrl {
    fn from(url: &Url) -> Self {
        Self {
            url: url.clone(),
            subdirectory: get_subdirectory(url),
        }
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

/// Return the Git reference of the given URL, if it exists.
pub fn git_reference(url: &Url) -> Result<Option<GitSha>, Error> {
    let DirectGitUrl { url, .. } = DirectGitUrl::try_from(url)?;
    Ok(url.precise())
}

impl TryFrom<&Url> for DirectUrl {
    type Error = Error;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        if let Some((prefix, ..)) = url.scheme().split_once('+') {
            match prefix {
                "git" => Ok(Self::Git(DirectGitUrl::try_from(url)?)),
                _ => Err(Error::msg(format!(
                    "Unsupported URL prefix `{prefix}` in URL: {url}",
                ))),
            }
        } else if url.scheme().eq_ignore_ascii_case("file") {
            Ok(Self::LocalFile(LocalFileUrl {
                url: url.clone(),
                editable: false,
            }))
        } else {
            Ok(Self::Archive(DirectArchiveUrl::from(url)))
        }
    }
}

impl TryFrom<&DirectUrl> for pypi_types::DirectUrl {
    type Error = Error;

    fn try_from(value: &DirectUrl) -> std::result::Result<Self, Self::Error> {
        match value {
            DirectUrl::LocalFile(value) => pypi_types::DirectUrl::try_from(value),
            DirectUrl::Git(value) => pypi_types::DirectUrl::try_from(value),
            DirectUrl::Archive(value) => pypi_types::DirectUrl::try_from(value),
        }
    }
}

impl TryFrom<&LocalFileUrl> for pypi_types::DirectUrl {
    type Error = Error;

    fn try_from(value: &LocalFileUrl) -> Result<Self, Self::Error> {
        Ok(pypi_types::DirectUrl::LocalDirectory {
            url: value.url.clone(),
            dir_info: pypi_types::DirInfo {
                editable: value.editable.then_some(true),
            },
        })
    }
}

impl TryFrom<&DirectArchiveUrl> for pypi_types::DirectUrl {
    type Error = Error;

    fn try_from(value: &DirectArchiveUrl) -> Result<Self, Self::Error> {
        Ok(pypi_types::DirectUrl::ArchiveUrl {
            url: value.url.clone(),
            archive_info: pypi_types::ArchiveInfo {
                hash: None,
                hashes: None,
            },
            subdirectory: value.subdirectory.clone(),
        })
    }
}

impl TryFrom<&DirectGitUrl> for pypi_types::DirectUrl {
    type Error = Error;

    fn try_from(value: &DirectGitUrl) -> Result<Self, Self::Error> {
        Ok(pypi_types::DirectUrl::VcsUrl {
            url: value.url.repository().clone(),
            vcs_info: pypi_types::VcsInfo {
                vcs: pypi_types::VcsKind::Git,
                commit_id: value.url.precise().as_ref().map(ToString::to_string),
                requested_revision: value.url.reference().map(ToString::to_string),
            },
            subdirectory: value.subdirectory.clone(),
        })
    }
}

impl From<DirectUrl> for Url {
    fn from(value: DirectUrl) -> Self {
        match value {
            DirectUrl::LocalFile(value) => value.into(),
            DirectUrl::Git(value) => value.into(),
            DirectUrl::Archive(value) => value.into(),
        }
    }
}

impl From<LocalFileUrl> for Url {
    fn from(value: LocalFileUrl) -> Self {
        value.url
    }
}

impl From<DirectArchiveUrl> for Url {
    fn from(value: DirectArchiveUrl) -> Self {
        let mut url = value.url;
        if let Some(subdirectory) = value.subdirectory {
            url.set_fragment(Some(&format!("subdirectory={}", subdirectory.display())));
        }
        url
    }
}

impl From<DirectGitUrl> for Url {
    fn from(value: DirectGitUrl) -> Self {
        let mut url = Url::parse(&format!("{}{}", "git+", Url::from(value.url).as_str()))
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

    use crate::direct_url::DirectUrl;

    #[test]
    fn direct_url_from_url() -> Result<()> {
        let expected = Url::parse("file:///path/to/directory")?;
        let actual = Url::from(DirectUrl::try_from(&expected)?);
        assert_eq!(expected, actual);

        let expected = Url::parse("git+https://github.com/pallets/flask.git")?;
        let actual = Url::from(DirectUrl::try_from(&expected)?);
        assert_eq!(expected, actual);

        let expected = Url::parse("git+https://github.com/pallets/flask.git#subdirectory=pkg_dir")?;
        let actual = Url::from(DirectUrl::try_from(&expected)?);
        assert_eq!(expected, actual);

        let expected = Url::parse("git+https://github.com/pallets/flask.git@2.0.0")?;
        let actual = Url::from(DirectUrl::try_from(&expected)?);
        assert_eq!(expected, actual);

        let expected =
            Url::parse("git+https://github.com/pallets/flask.git@2.0.0#subdirectory=pkg_dir")?;
        let actual = Url::from(DirectUrl::try_from(&expected)?);
        assert_eq!(expected, actual);

        // TODO(charlie): Preserve other fragments.
        let expected =
            Url::parse("git+https://github.com/pallets/flask.git#egg=flask&subdirectory=pkg_dir")?;
        let actual = Url::from(DirectUrl::try_from(&expected)?);
        assert_ne!(expected, actual);

        Ok(())
    }
}
