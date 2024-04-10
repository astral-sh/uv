use std::process::Command;

use thiserror::Error;
use tracing::debug;
use url::Url;

use crate::credentials::Credentials;

/// Keyring provider to use for authentication
///
/// See <https://pip.pypa.io/en/stable/topics/authentication/#keyring-support>
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "clap", not(test)), derive(clap::ValueEnum))]
#[cfg_attr(not(test), derive(Copy))]
pub enum KeyringProvider {
    /// Will not use keyring for authentication
    #[default]
    Disabled,
    /// Will use keyring CLI command for authentication
    Subprocess,
    // /// Not yet implemented
    // Auto,
    // /// Not implemented yet.  Maybe use <https://docs.rs/keyring/latest/keyring/> for this?
    // Import,
    /// A provider that returns preset credentials.
    /// Only available during testing of this crate.
    #[cfg(test)]
    Dummy(std::collections::HashMap<Url, &'static str>),
}

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("Url is not valid Keyring target: {0}")]
    NotKeyringTarget(String),
    #[error(transparent)]
    CliFailure(#[from] std::io::Error),
    #[error(transparent)]
    ParseFailed(#[from] std::string::FromUtf8Error),
}

impl KeyringProvider {
    /// Fetch credentials for the given [`Url`] from the keyring.
    ///
    /// Returns `None` if no password was found, even if a username is present on the URL.
    pub(crate) fn fetch(&self, url: &Url) -> Result<Option<Credentials>, Error> {
        let host = url.host_str();
        if host.is_none() {
            return Err(Error::NotKeyringTarget(
                "Should only use keyring for urls with host".to_string(),
            ));
        }

        if url.password().is_some() {
            return Err(Error::NotKeyringTarget(
                "Url already contains password - keyring not required".to_string(),
            ));
        }

        debug!("Checking keyring for credentials for `{}`", url.to_string(),);
        let password = match self {
            Self::Disabled => Ok(None),
            Self::Subprocess => self.fetch_subprocess(url),
            #[cfg(test)]
            Self::Dummy(provider) => self.fetch_dummy(provider, url),
        }?;

        Ok(password.map(|password| Credentials::new(url.username().to_string(), Some(password))))
    }

    /// Fetch from the `keyring` subprocess.
    ///
    /// See pip's implementation
    /// <https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/network/auth.py#L102>
    fn fetch_subprocess(&self, url: &Url) -> Result<Option<String>, Error> {
        let output = match Command::new("keyring")
            .arg("get")
            .arg(url.to_string())
            .arg(url.username())
            .output()
        {
            Ok(output) if output.status.success() => Ok(Some(
                String::from_utf8(output.stdout)
                    .map_err(Error::ParseFailed)?
                    .trim_end()
                    .to_owned(),
            )),
            Ok(_) => Ok(None),
            Err(e) => Err(Error::CliFailure(e)),
        };

        output
    }

    #[cfg(test)]
    fn fetch_dummy(
        &self,
        provider: &std::collections::HashMap<Url, &'static str>,
        url: &Url,
    ) -> Result<Option<String>, Error> {
        Ok(provider.get(url).map(|password| password.to_string()))
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn fetch_url_no_host() {
        let url = Url::parse("file:/etc/bin/").unwrap();
        let res = KeyringProvider::Dummy(HashMap::default()).fetch(&url);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(),
                Error::NotKeyringTarget(s) if s == "Should only use keyring for urls with host"));
    }

    #[test]
    fn fetch_url_with_password() {
        let url = Url::parse("https://user:password@example.com").unwrap();
        let res = KeyringProvider::Dummy(HashMap::default()).fetch(&url);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(),
                Error::NotKeyringTarget(s) if s == "Url already contains password - keyring not required"));
    }

    #[test]
    fn fetch_url_with_password_and_no_username() {
        let url = Url::parse("https://:password@example.com").unwrap();
        let res = KeyringProvider::Dummy(HashMap::default()).fetch(&url);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(),
                Error::NotKeyringTarget(s) if s == "Url already contains password - keyring not required"));
    }

    #[test]
    fn fetch_url_no_auth() -> Result<(), Error> {
        let url = Url::parse("https://example.com").unwrap();
        let credentials = KeyringProvider::Dummy(HashMap::default()).fetch(&url)?;
        assert!(credentials.is_none());
        Ok(())
    }

    #[test]
    fn fetch_url() -> Result<(), Error> {
        let url = Url::parse("https://example.com").unwrap();
        let credentials =
            KeyringProvider::Dummy(HashMap::from_iter([(url.clone(), "password")])).fetch(&url)?;
        assert_eq!(
            credentials,
            Some(Credentials::new(
                "".to_string(),
                Some("password".to_string())
            ))
        );
        Ok(())
    }

    #[test]
    fn fetch_url_no_match() -> Result<(), Error> {
        let url = Url::parse("https://example.com").unwrap();
        let credentials = KeyringProvider::Dummy(HashMap::from_iter([(
            Url::parse("https://other.com").unwrap(),
            "password",
        )]))
        .fetch(&url)?;
        assert_eq!(credentials, None);
        Ok(())
    }

    #[test]
    fn fetch_url_username() -> Result<(), Error> {
        let url = Url::parse("https://user@example.com").unwrap();
        let credentials =
            KeyringProvider::Dummy(HashMap::from_iter([(url.clone(), "password")])).fetch(&url)?;
        assert_eq!(
            credentials,
            Some(Credentials::new(
                "user".to_string(),
                Some("password".to_string())
            ))
        );
        Ok(())
    }

    #[test]
    fn fetch_url_username_no_match() -> Result<(), Error> {
        let foo_url = Url::parse("https://foo@example.com").unwrap();
        let bar_url = Url::parse("https://bar@example.com").unwrap();
        let credentials =
            KeyringProvider::Dummy(HashMap::from_iter([(foo_url.clone(), "password")]))
                .fetch(&bar_url)?;
        assert_eq!(credentials, None,);
        Ok(())
    }
}
