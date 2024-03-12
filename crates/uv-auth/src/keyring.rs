use std::process::Command;

use thiserror::Error;
use tracing::debug;
use url::Url;

use crate::store::{BasicAuthData, Credential};

/// Keyring provider to use for authentication
///
/// See <https://pip.pypa.io/en/stable/topics/authentication/#keyring-support>
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
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
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Url is not valid Keyring target: {0}")]
    NotKeyringTarget(String),
    #[error(transparent)]
    CliFailure(#[from] std::io::Error),
    #[error(transparent)]
    ParseFailed(#[from] std::string::FromUtf8Error),
}

/// Get credentials from keyring for given url
///
/// See `pip`'s KeyringCLIProvider
/// <https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/network/auth.py#L102>
pub fn get_keyring_subprocess_auth(url: &Url) -> Result<Option<Credential>, Error> {
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
    let username = match url.username() {
        u if !u.is_empty() => u,
        // this is the username keyring.get_credentials returns as username for GCP registry
        _ => "oauth2accesstoken",
    };
    debug!(
        "Running `keyring get` for `{}` with username `{}`",
        url.to_string(),
        username
    );
    let output = match Command::new("keyring")
        .arg("get")
        .arg(url.to_string())
        .arg(username)
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

    output.map(|password| {
        password.map(|password| {
            Credential::Basic(BasicAuthData {
                username: username.to_string(),
                password: Some(password),
            })
        })
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn hostless_url_should_err() {
        let url = Url::parse("file:/etc/bin/").unwrap();
        let res = get_keyring_subprocess_auth(&url);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(),
                Error::NotKeyringTarget(s) if s == "Should only use keyring for urls with host"));
    }

    #[test]
    fn passworded_url_should_err() {
        let url = Url::parse("https://u:p@example.com").unwrap();
        let res = get_keyring_subprocess_auth(&url);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(),
                Error::NotKeyringTarget(s) if s == "Url already contains password - keyring not required"));
    }
}
