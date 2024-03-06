use std::process::Command;

use anyhow::{bail, Result};
use tracing::debug;
use url::Url;

#[derive(Clone, Debug, PartialEq)]
pub struct BasicAuthData {
    pub username: String,
    pub password: String,
}

pub fn get_keyring_auth(url: &Url) -> Result<BasicAuthData> {
    if let Some(_) = url.password() {
        bail!("Url already contains password - keyring not required")
    }
    let username = match url.username() {
        u if !u.is_empty() => u,
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
        Ok(output) if output.status.success() => {
            String::from_utf8(output.stdout).expect("Keyring output should be valid utf8")
        }
        Ok(output) => bail!(
            "Unable to get keyring password for {url}: {}",
            String::from_utf8(output.stderr)
                .unwrap_or(String::from("Unable to convert stderr to String")),
        ),
        Err(e) => bail!(e),
    };
    Ok(BasicAuthData {
        username: username.to_string(),
        password: output,
    })
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
