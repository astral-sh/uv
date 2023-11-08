use std::path::PathBuf;

use anyhow::{Context, Error, Result};
use url::Url;

use puffin_git::GitUrl;
use crate::Distribution;

#[derive(Debug)]
pub enum DirectUrl {
    Git(DirectGitUrl),
    Archive(DirectArchiveUrl),
}

#[derive(Debug)]
pub struct DirectGitUrl {
    pub url: GitUrl,
    pub subdirectory: Option<PathBuf>,
}

#[derive(Debug)]
pub struct DirectArchiveUrl {
    pub url: Url,
    pub subdirectory: Option<PathBuf>,
}

impl TryFrom<&Url> for DirectGitUrl {
    type Error = Error;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        // If the URL points to a subdirectory, extract it, as in:
        //   `https://git.example.com/MyProject.git@v1.0#subdirectory=pkg_dir`
        //   `https://git.example.com/MyProject.git@v1.0#egg=pkg&subdirectory=pkg_dir`
        let subdirectory = url.fragment().and_then(|fragment| {
            fragment
                .split('&')
                .find_map(|fragment| fragment.strip_prefix("subdirectory=").map(PathBuf::from))
        });

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
        // If the URL points to a subdirectory, extract it, as in:
        //   `https://git.example.com/MyProject.git@v1.0#subdirectory=pkg_dir`
        //   `https://git.example.com/MyProject.git@v1.0#egg=pkg&subdirectory=pkg_dir`
        let subdirectory = url.fragment().and_then(|fragment| {
            fragment
                .split('&')
                .find_map(|fragment| fragment.strip_prefix("subdirectory=").map(PathBuf::from))
        });

        let url = url.clone();
        Self { url, subdirectory }
    }
}

impl TryFrom<&Url> for DirectUrl {
    type Error = Error;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        if url.scheme().starts_with("git+") {
            Ok(Self::Git(DirectGitUrl::try_from(url)?))
        } else {
            Ok(Self::Archive(DirectArchiveUrl::from(url)))
        }
    }
}

impl TryFrom<&DirectUrl> for pypi_types::DirectUrl {
    type Error = Error;

    fn try_from(value: &DirectUrl) -> std::result::Result<Self, Self::Error> {
        match value {
            DirectUrl::Git(value) => pypi_types::DirectUrl::try_from(value),
            DirectUrl::Archive(value) => pypi_types::DirectUrl::try_from(value),
        }
    }
}

impl TryFrom<&DirectArchiveUrl> for pypi_types::DirectUrl {
    type Error = Error;

    fn try_from(value: &DirectArchiveUrl) -> Result<Self, Self::Error> {
        Ok(pypi_types::DirectUrl::ArchiveUrl {
            url: value.url.to_string(),
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
            url: value.url.repository().to_string(),
            vcs_info: pypi_types::VcsInfo {
                vcs: pypi_types::VcsKind::Git,
                commit_id: value.url.precise().map(|oid| oid.to_string()),
                requested_revision: value.url.reference().map(ToString::to_string),
            },
            subdirectory: value.subdirectory.clone(),
        })
    }
}
