use ref_cast::RefCast;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display};
use std::ops::{Deref, DerefMut};
use std::str::FromStr;
use url::Url;

/// A [`Url`] wrapper that redacts credentials when displaying the URL.
///
/// `DisplaySafeUrl` wraps the standard [`url::Url`] type, providing functionality to mask
/// secrets by default when the URL is displayed or logged. This helps prevent accidental
/// exposure of sensitive information in logs and debug output.
///
/// # Examples
///
/// ```
/// use uv_redacted::DisplaySafeUrl;
/// use std::str::FromStr;
///
/// // Create a `DisplaySafeUrl` from a `&str`
/// let mut url = DisplaySafeUrl::parse("https://user:password@example.com").unwrap();
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
#[derive(Clone, Eq, PartialEq, PartialOrd, Ord, Hash, Serialize, Deserialize, RefCast)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(transparent))]
#[repr(transparent)]
pub struct DisplaySafeUrl(Url);

impl DisplaySafeUrl {
    #[inline]
    pub fn parse(input: &str) -> Result<Self, url::ParseError> {
        Ok(Self(Url::parse(input)?))
    }

    /// Cast a `&Url` to a `&DisplaySafeUrl` using ref-cast.
    #[inline]
    pub fn ref_cast(url: &Url) -> &Self {
        RefCast::ref_cast(url)
    }

    /// Parse a string as an URL, with this URL as the base URL.
    #[inline]
    pub fn join(&self, input: &str) -> Result<Self, url::ParseError> {
        self.0.join(input).map(DisplaySafeUrl::from)
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
        Url::deserialize_internal(deserializer).map(DisplaySafeUrl::from)
    }

    #[allow(clippy::result_unit_err)]
    pub fn from_file_path<P: AsRef<std::path::Path>>(path: P) -> Result<DisplaySafeUrl, ()> {
        Url::from_file_path(path).map(DisplaySafeUrl::from)
    }

    /// Remove the credentials from a URL, allowing the generic `git` username (without a password)
    /// in SSH URLs, as in, `ssh://git@github.com/...`.
    #[inline]
    pub fn remove_credentials(&mut self) {
        // For URLs that use the `git` convention (i.e., `ssh://git@github.com/...`), avoid dropping the
        // username.
        if is_ssh_git_username(&self.0) {
            return;
        }
        let _ = self.0.set_username("");
        let _ = self.0.set_password(None);
    }

    /// Returns [`Display`] implementation that doesn't mask credentials.
    #[inline]
    pub fn displayable_with_credentials(&self) -> impl Display {
        &self.0
    }
}

impl Deref for DisplaySafeUrl {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DisplaySafeUrl {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for DisplaySafeUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        display_with_redacted_credentials(&self.0, f)
    }
}

impl Debug for DisplaySafeUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let url = &self.0;
        // For URLs that use the `git` convention (i.e., `ssh://git@github.com/...`), avoid masking the
        // username.
        let (username, password) = if is_ssh_git_username(url) {
            (url.username(), None)
        } else if url.username() != "" && url.password().is_some() {
            (url.username(), Some("****"))
        } else if url.username() != "" {
            ("****", None)
        } else if url.password().is_some() {
            ("", Some("****"))
        } else {
            ("", None)
        };

        f.debug_struct("DisplaySafeUrl")
            .field("scheme", &url.scheme())
            .field("cannot_be_a_base", &url.cannot_be_a_base())
            .field("username", &username)
            .field("password", &password)
            .field("host", &url.host())
            .field("port", &url.port())
            .field("path", &url.path())
            .field("query", &url.query())
            .field("fragment", &url.fragment())
            .finish()
    }
}

impl From<Url> for DisplaySafeUrl {
    fn from(url: Url) -> Self {
        DisplaySafeUrl(url)
    }
}

impl From<DisplaySafeUrl> for Url {
    fn from(url: DisplaySafeUrl) -> Self {
        url.0
    }
}

impl FromStr for DisplaySafeUrl {
    type Err = url::ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Ok(Self(Url::from_str(input)?))
    }
}

fn is_ssh_git_username(url: &Url) -> bool {
    matches!(url.scheme(), "ssh" | "git+ssh") && url.username() == "git" && url.password().is_none()
}

fn display_with_redacted_credentials(
    url: &Url,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    if url.password().is_none() && url.username() == "" {
        return write!(f, "{url}");
    }

    // For URLs that use the `git` convention (i.e., `ssh://git@github.com/...`), avoid dropping the
    // username.
    if is_ssh_git_username(url) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_url_no_credentials() {
        let url_str = "https://pypi-proxy.fly.dev/basic-auth/simple";
        let url = Url::parse(url_str).unwrap();
        let log_safe_url = DisplaySafeUrl::from(url);
        assert_eq!(log_safe_url.username(), "");
        assert!(log_safe_url.password().is_none());
        assert_eq!(log_safe_url.to_string(), url_str);
    }

    #[test]
    fn from_url_username_and_password() {
        let url_str = "https://user:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let url = Url::parse(url_str).unwrap();
        let log_safe_url = DisplaySafeUrl::from(url);
        assert_eq!(log_safe_url.username(), "user");
        assert!(log_safe_url.password().is_some_and(|p| p == "pass"));
        assert_eq!(
            log_safe_url.to_string(),
            "https://user:****@pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn from_url_just_password() {
        let url_str = "https://:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let url = Url::parse(url_str).unwrap();
        let log_safe_url = DisplaySafeUrl::from(url);
        assert_eq!(log_safe_url.username(), "");
        assert!(log_safe_url.password().is_some_and(|p| p == "pass"));
        assert_eq!(
            log_safe_url.to_string(),
            "https://:****@pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn from_url_just_username() {
        let url_str = "https://user@pypi-proxy.fly.dev/basic-auth/simple";
        let url = Url::parse(url_str).unwrap();
        let log_safe_url = DisplaySafeUrl::from(url);
        assert_eq!(log_safe_url.username(), "user");
        assert!(log_safe_url.password().is_none());
        assert_eq!(
            log_safe_url.to_string(),
            "https://****@pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn from_url_git_username() {
        let ssh_str = "ssh://git@github.com/org/repo";
        let ssh_url = DisplaySafeUrl::parse(ssh_str).unwrap();
        assert_eq!(ssh_url.username(), "git");
        assert!(ssh_url.password().is_none());
        assert_eq!(ssh_url.to_string(), ssh_str);
        // Test again for the `git+ssh` scheme
        let git_ssh_str = "git+ssh://git@github.com/org/repo";
        let git_ssh_url = DisplaySafeUrl::parse(git_ssh_str).unwrap();
        assert_eq!(git_ssh_url.username(), "git");
        assert!(git_ssh_url.password().is_none());
        assert_eq!(git_ssh_url.to_string(), git_ssh_str);
    }

    #[test]
    fn parse_url_string() {
        let url_str = "https://user:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let log_safe_url = DisplaySafeUrl::parse(url_str).unwrap();
        assert_eq!(log_safe_url.username(), "user");
        assert!(log_safe_url.password().is_some_and(|p| p == "pass"));
        assert_eq!(
            log_safe_url.to_string(),
            "https://user:****@pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn remove_credentials() {
        let url_str = "https://user:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let mut log_safe_url = DisplaySafeUrl::parse(url_str).unwrap();
        log_safe_url.remove_credentials();
        assert_eq!(log_safe_url.username(), "");
        assert!(log_safe_url.password().is_none());
        assert_eq!(
            log_safe_url.to_string(),
            "https://pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn preserve_ssh_git_username_on_remove_credentials() {
        let ssh_str = "ssh://git@pypi-proxy.fly.dev/basic-auth/simple";
        let mut ssh_url = DisplaySafeUrl::parse(ssh_str).unwrap();
        ssh_url.remove_credentials();
        assert_eq!(ssh_url.username(), "git");
        assert!(ssh_url.password().is_none());
        assert_eq!(ssh_url.to_string(), ssh_str);
        // Test again for `git+ssh` scheme
        let git_ssh_str = "git+ssh://git@pypi-proxy.fly.dev/basic-auth/simple";
        let mut git_shh_url = DisplaySafeUrl::parse(git_ssh_str).unwrap();
        git_shh_url.remove_credentials();
        assert_eq!(git_shh_url.username(), "git");
        assert!(git_shh_url.password().is_none());
        assert_eq!(git_shh_url.to_string(), git_ssh_str);
    }

    #[test]
    fn displayable_with_credentials() {
        let url_str = "https://user:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let log_safe_url = DisplaySafeUrl::parse(url_str).unwrap();
        assert_eq!(
            log_safe_url.displayable_with_credentials().to_string(),
            url_str
        );
    }

    #[test]
    fn url_join() {
        let url_str = "https://token@example.com/abc/";
        let log_safe_url = DisplaySafeUrl::parse(url_str).unwrap();
        let foo_url = log_safe_url.join("foo").unwrap();
        assert_eq!(foo_url.to_string(), "https://****@example.com/abc/foo");
    }

    #[test]
    fn log_safe_url_ref() {
        let url_str = "https://user:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let url = Url::parse(url_str).unwrap();
        let log_safe_url = DisplaySafeUrl::ref_cast(&url);
        assert_eq!(log_safe_url.username(), "user");
        assert!(log_safe_url.password().is_some_and(|p| p == "pass"));
        assert_eq!(
            log_safe_url.to_string(),
            "https://user:****@pypi-proxy.fly.dev/basic-auth/simple"
        );
    }
}
