use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};
use std::str::FromStr;
use url::Url;

/// A [`Url`] wrapper that redacts credentials when displaying the URL.
///
/// `LogSafeUrl` wraps the standard [`url::Url`] type, providing functionality to mask
/// secrets by default when the URL is displayed or logged. This helps prevent accidental
/// exposure of sensitive information in logs and debug output.
///
/// # Examples
///
/// ```
/// use uv_redacted::LogSafeUrl;
/// use std::str::FromStr;
///
/// // Create a `LogSafeUrl` from a `&str`
/// let mut url = LogSafeUrl::parse("https://user:password@example.com").unwrap();
///
/// // Display will mask secrets
/// assert_eq!(url.to_string(), "https://user:****@example.com/");
///
/// // You can still access the username and password
/// assert_eq!(url.username(), "user");
/// assert_eq!(url.password(), Some("password"));
///
/// // And you can still update the username and password
/// let _ = url.set_username("new_user");
/// let _ = url.set_password(Some("new_password"));
/// assert_eq!(url.username(), "new_user");
/// assert_eq!(url.password(), Some("new_password"));
///
/// // It is also possible to remove the credentials entirely
/// url.remove_credentials();
/// assert_eq!(url.username(), "");
/// assert_eq!(url.password(), None);
/// ```
#[derive(Clone, Eq, PartialEq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LogSafeUrl(Url);

impl LogSafeUrl {
    #[inline]
    pub fn parse(input: &str) -> Result<Self, url::ParseError> {
        Ok(Self(Url::parse(input)?))
    }

    /// Parse a string as an URL, with this URL as the base URL.
    #[inline]
    pub fn join(&self, input: &str) -> Result<Self, url::ParseError> {
        self.0.join(input).map(LogSafeUrl::from)
    }

    /// Serialize with Serde using the internal representation of the `Url` struct.
    #[inline]
    pub fn serialize_internal<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize_internal(serializer)
    }

    /// Serialize with Serde using the internal representation of the `Url` struct.
    #[inline]
    pub fn deserialize_internal<'de, D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Url::deserialize_internal(deserializer).map(LogSafeUrl::from)
    }

    #[allow(clippy::result_unit_err)]
    pub fn from_file_path<P: AsRef<std::path::Path>>(path: P) -> Result<LogSafeUrl, ()> {
        Url::from_file_path(path).map(LogSafeUrl::from)
    }

    /// Remove the credentials from a URL, allowing the generic `git` username (without a password)
    /// in SSH URLs, as in, `ssh://git@github.com/...`.
    #[inline]
    pub fn remove_credentials(&mut self) {
        // For URLs that use the `git` convention (i.e., `ssh://git@github.com/...`), avoid dropping the
        // username.
        if self.0.scheme() == "ssh" && self.0.username() == "git" && self.0.password().is_none() {
            return;
        }
        let _ = self.0.set_username("");
        let _ = self.0.set_password(None);
    }

    /// Returns string representation without masking credentials.
    #[inline]
    pub fn to_string_with_credentials(&self) -> String {
        self.0.to_string()
    }
}

impl Deref for LogSafeUrl {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for LogSafeUrl {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::fmt::Display for LogSafeUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let url = &self.0;

        if url.password().is_none() && url.username() == "" {
            return write!(f, "{url}");
        }

        write!(f, "{}://", url.scheme())?;

        if url.username() != "" && url.password().is_some() {
            write!(f, "{}", url.username())?;
            write!(f, ":****@")?;
        } else if url.username() != "" {
            write!(f, "****@")?;
        } else if url.password().is_some() {
            write!(f, ":****@")?;
        }

        write!(f, "{}", url.host_str().unwrap_or(""))?;

        if let Some(port) = url.port() {
            write!(f, ":{port}")?;
        }

        write!(f, "{}", url.path())?;
        if let Some(query) = url.query() {
            write!(f, "?{query}")?;
        }
        if let Some(fragment) = url.fragment() {
            write!(f, "#{fragment}")?;
        }

        Ok(())
    }
}

impl std::fmt::Debug for LogSafeUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

impl From<Url> for LogSafeUrl {
    fn from(url: Url) -> Self {
        LogSafeUrl(url)
    }
}

impl From<LogSafeUrl> for Url {
    fn from(url: LogSafeUrl) -> Self {
        url.0
    }
}

impl FromStr for LogSafeUrl {
    type Err = url::ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Ok(Self(Url::from_str(input)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_url_no_credentials() {
        let url_str = "https://pypi-proxy.fly.dev/basic-auth/simple";
        let url = Url::parse(url_str).unwrap();
        let log_safe_url = LogSafeUrl::from(url);
        assert_eq!(log_safe_url.username(), "");
        assert!(log_safe_url.password().is_none());
        assert_eq!(format!("{log_safe_url}"), url_str);
    }

    #[test]
    fn from_url_username_and_password() {
        let url_str = "https://user:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let url = Url::parse(url_str).unwrap();
        let log_safe_url = LogSafeUrl::from(url);
        assert_eq!(log_safe_url.username(), "user");
        assert!(log_safe_url.password().is_some_and(|p| p == "pass"));
        assert_eq!(
            format!("{log_safe_url}"),
            "https://user:****@pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn from_url_just_password() {
        let url_str = "https://:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let url = Url::parse(url_str).unwrap();
        let log_safe_url = LogSafeUrl::from(url);
        assert_eq!(log_safe_url.username(), "");
        assert!(log_safe_url.password().is_some_and(|p| p == "pass"));
        assert_eq!(
            format!("{log_safe_url}"),
            "https://:****@pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn from_url_just_username() {
        let url_str = "https://user@pypi-proxy.fly.dev/basic-auth/simple";
        let url = Url::parse(url_str).unwrap();
        let log_safe_url = LogSafeUrl::from(url);
        assert_eq!(log_safe_url.username(), "user");
        assert!(log_safe_url.password().is_none());
        assert_eq!(
            format!("{log_safe_url}"),
            "https://****@pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn parse_url_string() {
        let url_str = "https://user:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let log_safe_url = LogSafeUrl::parse(url_str).unwrap();
        assert_eq!(log_safe_url.username(), "user");
        assert!(log_safe_url.password().is_some_and(|p| p == "pass"));
        assert_eq!(
            format!("{log_safe_url}"),
            "https://user:****@pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn remove_credentials() {
        let url_str = "https://user:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let mut log_safe_url = LogSafeUrl::parse(url_str).unwrap();
        log_safe_url.remove_credentials();
        assert_eq!(log_safe_url.username(), "");
        assert!(log_safe_url.password().is_none());
        assert_eq!(
            format!("{log_safe_url}"),
            "https://pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn to_string_with_credentials() {
        let url_str = "https://user:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let log_safe_url = LogSafeUrl::parse(url_str).unwrap();
        assert_eq!(&log_safe_url.to_string_with_credentials(), url_str);
    }

    #[test]
    fn url_join() {
        let url_str = "https://token@example.com/abc/";
        let log_safe_url = LogSafeUrl::parse(url_str).unwrap();
        let foo_url = log_safe_url.join("foo").unwrap();
        assert_eq!(format!("{foo_url}"), "https://****@example.com/abc/foo");
    }
}
