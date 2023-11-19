use std::path::PathBuf;

use anyhow::{Context, Error, Result};
use url::Url;

use puffin_git::GitUrl;

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
        if let Some((prefix, ..)) = url.scheme().split_once('+') {
            match prefix {
                "git" => Ok(Self::Git(DirectGitUrl::try_from(url)?)),
                _ => Err(Error::msg(format!(
                    "Unsupported URL prefix `{prefix}` in URL: {url}",
                ))),
            }
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

impl From<DirectUrl> for Url {
    fn from(value: DirectUrl) -> Self {
        match value {
            DirectUrl::Git(value) => value.into(),
            DirectUrl::Archive(value) => value.into(),
        }
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
