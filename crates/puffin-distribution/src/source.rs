use std::path::PathBuf;

use anyhow::{Context, Error, Result};
use url::Url;

use puffin_git::GitUrl;

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

impl TryFrom<&Url> for DirectArchiveUrl {
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

        let url = url.clone();
        Ok(Self { url, subdirectory })
    }
}

impl From<DirectArchiveUrl> for Url {
    fn from(value: DirectArchiveUrl) -> Self {
        value.url
    }
}

// impl From<DirectGitUrl> for Url {
//     fn try_from(value: &DirectGitUrl) -> Result<Self, Self::Error> {
//         let mut url = Url::parse(&format!("{}{}", "git+", Url::from(&value.url).as_str()))?;
//         if let Some(subdirectory) = &value.subdirectory {
//             url.set_fragment(Some(&format!("subdirectory={}", subdirectory.display())));
//         }
//         Ok(url)
//     }
// }

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

// STOPSHIP(charlie): Add logic for converting to and from URL.
