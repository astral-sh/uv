use std::borrow::Cow;

use url::Url;

/// Return a version of the URL with redacted credentials, allowing the generic `git` username (without a password)
/// in SSH URLs, as in, `ssh://git@github.com/...`.
pub fn redacted_url(url: &Url) -> Cow<'_, Url> {
    if url.username().is_empty() && url.password().is_none() {
        return Cow::Borrowed(url);
    }
    if url.scheme() == "ssh" && url.username() == "git" && url.password().is_none() {
        return Cow::Borrowed(url);
    }

    let mut url = url.clone();
    let _ = url.set_username("");
    let _ = url.set_password(None);
    Cow::Owned(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_url_no_credentials() {
        let url = Url::parse("https://pypi-proxy.fly.dev/basic-auth/simple").unwrap();
        let redacted = redacted_url(&url);
        assert_eq!(redacted.username(), "");
        assert!(redacted.password().is_none());
        assert_eq!(
            format!("{redacted}"),
            "https://pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn from_url_username_and_password() {
        let url = Url::parse("https://user:pass@pypi-proxy.fly.dev/basic-auth/simple").unwrap();
        let redacted = redacted_url(&url);
        assert_eq!(redacted.username(), "");
        assert!(redacted.password().is_none());
        assert_eq!(
            format!("{redacted}"),
            "https://pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn from_url_just_password() {
        let url = Url::parse("https://:pass@pypi-proxy.fly.dev/basic-auth/simple").unwrap();
        let redacted = redacted_url(&url);
        assert_eq!(redacted.username(), "");
        assert!(redacted.password().is_none());
        assert_eq!(
            format!("{redacted}"),
            "https://pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn from_url_just_username() {
        let url = Url::parse("https://user@pypi-proxy.fly.dev/basic-auth/simple").unwrap();
        let redacted = redacted_url(&url);
        assert_eq!(redacted.username(), "");
        assert!(redacted.password().is_none());
        assert_eq!(
            format!("{redacted}"),
            "https://pypi-proxy.fly.dev/basic-auth/simple"
        );
    }
}
