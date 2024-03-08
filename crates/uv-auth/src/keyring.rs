use lazy_static::lazy_static;
use std::{collections::HashMap, process::Command, sync::Mutex};

use thiserror::Error;
use tracing::debug;
use url::Url;

// TODO - migrate to AuthenticationStore used in middleware
lazy_static! {
    static ref PASSWORDS: Mutex<HashMap<String, Option<BasicAuthData>>> =
        Mutex::new(HashMap::new());
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Url is not valid Keyring target: {0}")]
    NotKeyringTarget(String),
    #[error("Keyring did not resolve password: {0}")]
    NotFound(String),
    #[error(transparent)]
    CLIError(#[from] std::io::Error),
    #[error(transparent)]
    ParseError(#[from] std::string::FromUtf8Error),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BasicAuthData {
    pub username: String,
    pub password: String,
}

pub fn get_keyring_auth(url: &Url) -> Result<BasicAuthData, Error> {
    let host = url.host_str();
    if host.is_none() {
        return Err(Error::NotKeyringTarget(
            "Should only use keyring for urls with host".to_string(),
        ));
    }
    let host = host.unwrap();
    if url.password().is_some() {
        return Err(Error::NotKeyringTarget(
            "Url already contains password - keyring not required".to_string(),
        ));
    }
    let mut passwords = PASSWORDS.lock().unwrap();
    if let Some(password) = passwords.get(host) {
        return password.clone().ok_or(Error::NotFound(
            "Previously failed to find keyring password".to_string(),
        ));
    }
    let username = match url.username() {
        u if !u.is_empty() => u,
        // this is the username keyring.get_credentials returns as username for GCP registry
        _ => "oauth2accesstoken",
    };
    debug!(
        "Running `keyring get` for `{:?}` with username `{}`",
        url.to_string(),
        username
    );
    let output = match Command::new("keyring")
        .arg("get")
        .arg(url.to_string())
        .arg(username)
        .output()
    {
        Ok(output) if output.status.success() => Ok(String::from_utf8(output.stdout)
            .map_err(|e| Error::ParseError(e))?
            .trim_end()
            .to_owned()),
        Ok(output) => Err(Error::NotFound(
            String::from_utf8(output.stderr).map_err(|e| Error::ParseError(e))?,
        )),
        Err(e) => Err(Error::CLIError(e)),
    };
    let output = output.map(|password| BasicAuthData {
        username: username.to_string(),
        password,
    });
    passwords.insert(host.to_string(), output.as_ref().ok().cloned());
    output
}

#[cfg(test)]
mod test {
    use url::Url;

    use super::{get_keyring_auth, BasicAuthData, Error, PASSWORDS};

    #[test]
    fn hostless_url_should_err() {
        let url = Url::parse("file:/etc/bin/").unwrap();
        let res = get_keyring_auth(&url);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(),
                Error::NotKeyringTarget(s) if s == "Should only use keyring for urls with host"));
    }

    #[test]
    fn passworded_url_should_err() {
        let url = Url::parse("https://u:p@example.com").unwrap();
        let res = get_keyring_auth(&url);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(),
                Error::NotKeyringTarget(s) if s == "Url already contains password - keyring not required"));
    }

    #[test]
    fn memo_return_works() {
        let found_first_url = Url::parse("https://example1.com/simple/first/").unwrap();
        let not_found_first_url = Url::parse("https://example2.com/simple/first/").unwrap();

        {
            // simulate output memoization from keyring CLI result
            let mut passwords = PASSWORDS.lock().unwrap();
            passwords.insert(
                found_first_url.host_str().unwrap().to_string(),
                Some(BasicAuthData {
                    username: "u".to_string(),
                    password: "p".to_string(),
                }),
            );
            passwords.insert(not_found_first_url.host_str().unwrap().to_string(), None);
        }

        let found_second_url = Url::parse("https://example1.com/simple/second/").unwrap();
        let not_found_second_url = Url::parse("https://example2.com/simple/second/").unwrap();

        let found_res = get_keyring_auth(&found_second_url);
        assert!(found_res.is_ok());
        let found_res = found_res.unwrap();
        assert_eq!(found_res.username, "u");
        assert_eq!(found_res.password, "p");

        let not_found_res = get_keyring_auth(&not_found_second_url);
        assert!(not_found_res.is_err());
        assert!(matches!(not_found_res.unwrap_err(),
                Error::NotFound(s) if s == "Previously failed to find keyring password"));
    }
}
