use lazy_static::lazy_static;
use std::{collections::HashMap, process::Command, sync::Mutex};

use anyhow::{anyhow, bail, Result};
use tracing::debug;
use url::Url;

lazy_static! {
    static ref PASSWORDS: Mutex<HashMap<String, Option<BasicAuthData>>> =
        Mutex::new(HashMap::new());
}

#[derive(Clone, Debug, PartialEq)]
pub struct BasicAuthData {
    pub username: String,
    pub password: String,
}

pub fn get_keyring_auth(url: &Url) -> Result<BasicAuthData> {
    let host = url.host_str();
    if host.is_none() {
        bail!("Should only use keyring for urls with host");
    }
    let host = host.unwrap();
    if url.password().is_some() {
        bail!("Url already contains password - keyring not required")
    }
    let mut passwords = PASSWORDS.lock().unwrap();
    if passwords.contains_key(host) {
        return passwords
            .get(host)
            .unwrap()
            .clone()
            .ok_or(anyhow!("Previously failed to find keyring password"));
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
            .expect("Keyring output should be valid utf8")
            .trim_end()
            .to_owned()),
        Ok(output) => Err(anyhow!(
            "Unable to get keyring password for {url}: {}",
            String::from_utf8(output.stderr)
                .unwrap_or(String::from("Unable to convert stderr to String")),
        )),
        Err(e) => Err(anyhow!(e)),
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

    use crate::{get_keyring_auth, BasicAuthData, PASSWORDS};

    #[test]
    fn hostless_url_should_err() {
        let url = Url::parse("file:/etc/bin/").unwrap();
        let res = get_keyring_auth(&url);
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err().to_string(),
            "Should only use keyring for urls with host"
        );
    }

    #[test]
    fn passworded_url_should_err() {
        let url = Url::parse("https://u:p@example.com").unwrap();
        let res = get_keyring_auth(&url);
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err().to_string(),
            "Url already contains password - keyring not required"
        );
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
        assert_eq!(
            not_found_res.unwrap_err().to_string(),
            "Previously failed to find keyring password"
        );
    }
}
