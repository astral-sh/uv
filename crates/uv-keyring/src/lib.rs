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

    use crate::get_keyring_auth;

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
}
