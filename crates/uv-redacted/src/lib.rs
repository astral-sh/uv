#![expect(
    clippy::disallowed_types,
    reason = "`SafeUrl` wraps `url::Url` and exposes conversions for external APIs"
)]

use ref_cast::RefCast;
use serde::Deserialize;
use std::borrow::Cow;
use std::fmt::{Debug, Display};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::str::FromStr;
use thiserror::Error;
use url::Url;

pub mod policy;

use policy::{InternalSerializationPolicy, Raw, SerializationPolicy, Unserializable};

const SENSITIVE_QUERY_PARAMETERS: &[&str] = &[
    "sig",
    "X-Amz-Credential",
    "X-Amz-Security-Token",
    "X-Amz-Signature",
    "sig",
];

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SafeUrlError {
    /// Failed to parse a URL.
    #[error(transparent)]
    Url(#[from] url::ParseError),

    /// We parsed a URL, but couldn't disambiguate its authority
    /// component.
    #[error("ambiguous user/pass authority in URL (not percent-encoded?): {0}")]
    AmbiguousAuthority(String),
}

/// A URL with redacted display output and a type-level serialization policy.
///
/// The complete parsed URL, including credentials, remains available in memory.
/// [`Display`] and [`Debug`] redact credentials and recognized sensitive query
/// parameters for every policy. Policies affect serialization, not parsing or
/// the URL's contents.
///
/// A reference cannot implicitly coerce to `&Url`:
///
/// ```compile_fail
/// use uv_redacted::SafeUrl;
///
/// let url = SafeUrl::parse("https://example.com")?;
/// let _: &url::Url = &url;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// A mutable reference cannot implicitly coerce to `&mut Url`:
///
/// ```compile_fail
/// use uv_redacted::SafeUrl;
///
/// let mut url = SafeUrl::parse("https://example.com")?;
/// let _: &mut url::Url = &mut url;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// A [`SafeUrl`] with the default [`Unserializable`] policy does not implement
/// [`serde::Serialize`].
/// Choose [`policy::Stripped`] or [`policy::Raw`] explicitly
/// when a field needs to be serialized:
///
/// ```
/// use uv_redacted::{SafeUrl, policy::Stripped};
///
/// #[derive(serde::Serialize)]
/// struct Origin {
///     url: SafeUrl<Stripped>,
/// }
///
/// let url = SafeUrl::parse("https://user:password@example.com/file.whl?sig=signature")?;
/// let origin = Origin { url: url.with_policy(Stripped) };
/// assert_eq!(origin.url.password(), Some("password"));
/// assert_eq!(serde_json::to_string(&origin)?,
///     r#"{"url":"https://example.com/file.whl?sig=signature"}"#);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// Fields with the default policy cannot derive [`serde::Serialize`] without a custom serializer:
///
/// ```compile_fail
/// use uv_redacted::SafeUrl;
///
/// #[derive(serde::Serialize)]
/// struct Origin {
///     url: SafeUrl,
/// }
/// ```
///
/// `serialize` method calls also require a serializing policy:
///
/// ```compile_fail
/// use serde::Serialize;
/// use uv_redacted::SafeUrl;
///
/// let url = SafeUrl::parse("https://user:password@example.com")?;
/// url.serialize(serde_json::value::Serializer)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// Returning to the default policy removes the ordinary [`serde::Serialize`] implementation:
///
/// ```compile_fail
/// use uv_redacted::{SafeUrl, policy::{Unserializable, Raw}};
///
/// let url = SafeUrl::parse("https://example.com")?
///     .with_policy(Raw)
///     .with_policy(Unserializable);
/// serde_json::to_string(&url)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Eq, PartialEq, PartialOrd, Ord, Hash, RefCast)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "schemars",
    schemars(transparent, bound = "", description = "A URL.")
)]
#[repr(transparent)]
pub struct SafeUrl<P: SerializationPolicy = Unserializable>(Url, PhantomData<P>);

/// A guard for editing a [`SafeUrl`]'s path segments.
///
/// The underlying guard contains the complete URL, so this wrapper does not expose its debug output.
pub struct PathSegmentsMut<'a>(url::PathSegmentsMut<'a>);

impl PathSegmentsMut<'_> {
    /// Remove all path segments, leaving the leading slash.
    pub fn clear(&mut self) -> &mut Self {
        self.0.clear();
        self
    }

    /// Remove the last path segment if it is empty.
    pub fn pop_if_empty(&mut self) -> &mut Self {
        self.0.pop_if_empty();
        self
    }

    /// Remove the last path segment.
    pub fn pop(&mut self) -> &mut Self {
        self.0.pop();
        self
    }

    /// Append a path segment, percent-encoding it as needed.
    pub fn push(&mut self, segment: &str) -> &mut Self {
        self.0.push(segment);
        self
    }

    /// Append path segments, percent-encoding them as needed.
    pub fn extend<I>(&mut self, segments: I) -> &mut Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        self.0.extend(segments);
        self
    }
}

/// A guard for appending query pairs to a [`SafeUrl`].
pub struct QueryPairsMut<'a>(url::form_urlencoded::Serializer<'a, url::UrlQuery<'a>>);

impl QueryPairsMut<'_> {
    /// Append a name/value pair using form URL encoding.
    pub fn append_pair(&mut self, name: &str, value: &str) -> &mut Self {
        self.0.append_pair(name, value);
        self
    }
}

/// Check if a path or fragment contains a credential-like pattern (`:` followed by `@`).
///
/// This skips colons that are followed by `//`, as those indicate URL schemes (e.g., `https://`)
/// rather than credentials. This is important for handling nested URLs like proxy URLs:
/// `git+https://proxy.com/https://github.com/user/repo.git@branch`.
fn has_credential_like_pattern(s: &str) -> bool {
    let mut remaining = s;
    while let Some(colon_pos) = remaining.find(':') {
        let after_colon = &remaining[colon_pos + 1..];
        // If the colon is followed by "//", consider it a URL scheme.
        if after_colon.starts_with("//") {
            remaining = after_colon;
            continue;
        }
        // Check if there's an @ after this colon.
        if after_colon.contains('@') {
            return true;
        }
        remaining = after_colon;
    }
    false
}

impl SafeUrl {
    #[inline]
    pub fn parse(input: &str) -> Result<Self, SafeUrlError> {
        let url = Self::parse_relaxed(input)?;

        Self::reject_ambiguous_credentials(input, &url.0)?;

        Ok(url)
    }

    /// Parse a URL using [`Url::parse`] without the ambiguous-credential check.
    ///
    /// Unlike [`Self::parse`], this accepts ambiguous inputs whose path or fragment
    /// may contain credentials. Those parts are not redacted when formatted.
    #[inline]
    pub fn parse_relaxed(input: &str) -> Result<Self, url::ParseError> {
        Url::parse(input).map(Self::from_url)
    }

    /// Reject some ambiguous cases, e.g., `https://user/name:password@domain/a/b/c`
    ///
    /// In this case the user *probably* meant to have a username of "user/name", but both RFC
    /// 3986 and WHATWG URL expect the userinfo (RFC 3986) or authority (WHATWG) to not contain a
    /// non-percent-encoded slash or other special character.
    ///
    /// This ends up being moderately annoying to detect, since the above gets parsed into a
    /// "valid" WHATWG URL where the host is `used` and the pathname is
    /// `/name:password@domain/a/b/c` rather than causing a parse error.
    ///
    /// To detect it, we use a heuristic: if the password component is missing but the path or
    /// fragment contain a `:` followed by a `@`, then we assume the URL is ambiguous.
    fn reject_ambiguous_credentials(input: &str, url: &Url) -> Result<(), SafeUrlError> {
        // `git://`, `http://`, and `https://` URLs may carry credentials, while `file://` URLs
        // on Windows may contain both sigils, but it's always safe, e.g.
        // `file://C:/Users/ferris/project@home/workspace`. The same holds for VCS URLs that use a
        // file transport, such as `git+file://C:/Users/ferris/repo.git@v1.0`, which likewise carry
        // no network credentials but can pair a drive-letter `:` with an `@` revision.
        let scheme = url.scheme();
        if scheme == "file" || scheme.ends_with("+file") {
            return Ok(());
        }

        if url.password().is_some() {
            return Ok(());
        }

        // Check for the suspicious pattern.
        if !has_credential_like_pattern(url.path())
            && !url.fragment().is_some_and(has_credential_like_pattern)
        {
            return Ok(());
        }

        // If the previous check passed, we should always expect to find these in the given URL.
        let (Some(col_pos), Some(at_pos)) = (input.find(':'), input.rfind('@')) else {
            if cfg!(debug_assertions) {
                unreachable!(
                    "`:` or `@` sign missing in URL that was confirmed to contain them: {input}"
                );
            }
            return Ok(());
        };

        // Our ambiguous URL probably has credentials in it, so we don't want to blast it out in
        // the error message. We somewhat aggressively replace everything between the scheme's
        // ':' and the lastmost `@` with `***`.
        let redacted_path = format!("{}***{}", &input[0..=col_pos], &input[at_pos..]);
        Err(SafeUrlError::AmbiguousAuthority(redacted_path))
    }

    /// Create a new [`SafeUrl`] from a [`Url`].
    ///
    /// Unlike [`Self::parse`], this doesn't perform any ambiguity checks.
    /// That means that it's primarily useful for contexts where a human can't easily accidentally
    /// introduce an ambiguous URL, such as URLs being read from a request.
    pub fn from_url(url: Url) -> Self {
        Self(url, PhantomData)
    }

    /// Borrow a [`Url`] with the default [`Unserializable`] policy using ref-cast.
    #[inline]
    pub fn ref_cast(url: &Url) -> &Self {
        RefCast::ref_cast(url)
    }

    #[expect(clippy::result_unit_err)]
    pub fn from_file_path<P: AsRef<std::path::Path>>(path: P) -> Result<Self, ()> {
        Ok(Self(Url::from_file_path(path)?, PhantomData))
    }

    /// Convert an absolute directory path to a file URL with a trailing slash.
    #[expect(clippy::result_unit_err)]
    pub fn from_directory_path<P: AsRef<std::path::Path>>(path: P) -> Result<Self, ()> {
        Ok(Self(Url::from_directory_path(path)?, PhantomData))
    }
}

impl<P: SerializationPolicy> SafeUrl<P> {
    /// Change the serialization policy without modifying the parsed URL.
    #[must_use]
    pub fn with_policy<Q: SerializationPolicy>(self, _policy: Q) -> SafeUrl<Q> {
        SafeUrl(self.0, PhantomData)
    }

    /// Borrow the parsed URL with a different serialization policy.
    #[must_use]
    pub fn with_policy_ref<Q: SerializationPolicy>(&self, _policy: Q) -> &SafeUrl<Q> {
        RefCast::ref_cast(&self.0)
    }

    /// Borrow the URL with the default [`Unserializable`] policy.
    pub fn as_url(&self) -> &SafeUrl {
        self.with_policy_ref(Unserializable)
    }

    /// Return the URL with the default [`Unserializable`] policy.
    pub fn into_url(self) -> SafeUrl {
        self.with_policy(Unserializable)
    }

    /// Return the complete parsed URL for APIs that require [`Url`].
    ///
    /// The returned URL has no display redaction or serialization policy.
    /// Use this method to pass an owned [`Url`] to an API that requires it.
    /// Conversion using [`From`] is not supported:
    ///
    /// ```compile_fail
    /// use uv_redacted::SafeUrl;
    ///
    /// let url = SafeUrl::parse("https://example.com")?;
    /// let _: url::Url = url.into();
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn into_unsafe_url(self) -> Url {
        self.0
    }

    /// Return the complete URL string, including credentials.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Return the scheme, without the trailing colon.
    pub fn scheme(&self) -> &str {
        self.0.scheme()
    }

    /// Return the authority, including any credentials and port.
    pub fn authority(&self) -> &str {
        self.0.authority()
    }

    /// Return whether the URL has an opaque path instead of path segments.
    pub fn cannot_be_a_base(&self) -> bool {
        self.0.cannot_be_a_base()
    }

    /// Return the percent-encoded username.
    pub fn username(&self) -> &str {
        self.0.username()
    }

    /// Return the percent-encoded password, if present.
    pub fn password(&self) -> Option<&str> {
        self.0.password()
    }

    /// Return whether the URL has a host.
    pub fn has_host(&self) -> bool {
        self.0.has_host()
    }

    /// Return the host as a string, if present.
    pub fn host_str(&self) -> Option<&str> {
        self.0.host_str()
    }

    /// Return the parsed host, if present.
    pub fn host(&self) -> Option<url::Host<&str>> {
        self.0.host()
    }

    /// Return the explicit non-default port, if present.
    pub fn port(&self) -> Option<u16> {
        self.0.port()
    }

    /// Return the explicit port or the scheme's known default.
    pub fn port_or_known_default(&self) -> Option<u16> {
        self.0.port_or_known_default()
    }

    /// Return the percent-encoded path.
    pub fn path(&self) -> &str {
        self.0.path()
    }

    /// Return the percent-encoded path segments, unless the path is opaque.
    pub fn path_segments(&self) -> Option<std::str::Split<'_, char>> {
        self.0.path_segments()
    }

    /// Return the decoded query name/value pairs.
    pub fn query_pairs(&self) -> url::form_urlencoded::Parse<'_> {
        self.0.query_pairs()
    }

    /// Return the fragment, without the leading hash, if present.
    pub fn fragment(&self) -> Option<&str> {
        self.0.fragment()
    }

    /// Set or remove the fragment, using [`Url::set_fragment`] encoding rules.
    pub fn set_fragment(&mut self, fragment: Option<&str>) {
        self.0.set_fragment(fragment);
    }

    /// Set or remove the query, using [`Url::set_query`] encoding rules.
    pub fn set_query(&mut self, query: Option<&str>) {
        self.0.set_query(query);
    }

    /// Set the path, using [`Url::set_path`] encoding rules.
    pub fn set_path(&mut self, path: &str) {
        self.0.set_path(path);
    }

    /// Set or remove the password, if the URL supports credentials.
    #[expect(clippy::result_unit_err)]
    pub fn set_password(&mut self, password: Option<&str>) -> Result<(), ()> {
        self.0.set_password(password)
    }

    /// Set the username, if the URL supports credentials.
    #[expect(clippy::result_unit_err)]
    pub fn set_username(&mut self, username: &str) -> Result<(), ()> {
        self.0.set_username(username)
    }

    /// Convert a file URL to a filesystem path.
    #[expect(clippy::result_unit_err)]
    pub fn to_file_path(&self) -> Result<PathBuf, ()> {
        self.0.to_file_path()
    }

    /// Edit the path segments without exposing the underlying URL.
    #[expect(clippy::result_unit_err)]
    pub fn path_segments_mut(&mut self) -> Result<PathSegmentsMut<'_>, ()> {
        self.0.path_segments_mut().map(PathSegmentsMut)
    }

    /// Append query pairs with form URL encoding.
    ///
    /// The guard restores the fragment when dropped. It cannot return a raw URL:
    ///
    /// ```compile_fail
    /// use uv_redacted::SafeUrl;
    ///
    /// let mut url = SafeUrl::parse("https://example.com")?;
    /// let _: &mut url::Url = url.query_pairs_mut().finish();
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn query_pairs_mut(&mut self) -> QueryPairsMut<'_> {
        QueryPairsMut(self.0.query_pairs_mut())
    }

    /// Parse a string as an URL, with this URL as the base URL.
    #[inline]
    pub fn join(&self, input: &str) -> Result<Self, SafeUrlError> {
        Ok(Self(self.0.join(input)?, PhantomData))
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

    /// Return the URL with sensitive userinfo removed, retaining its serialization policy.
    ///
    /// The complete query is retained; display and debug formatting redact recognized credentials.
    /// Removing userinfo does not make an [`Unserializable`] URL serializable:
    ///
    /// ```compile_fail
    /// use uv_redacted::SafeUrl;
    ///
    /// let url = SafeUrl::parse("https://user:password@example.com/file.whl")?;
    /// serde_json::to_string(&url.without_credentials())?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn without_credentials(&self) -> Cow<'_, Self> {
        if self.0.password().is_none() && self.0.username() == "" {
            return Cow::Borrowed(self);
        }

        // For URLs that use the `git` convention (i.e., `ssh://git@github.com/...`), avoid dropping the
        // username.
        if is_ssh_git_username(&self.0) {
            return Cow::Borrowed(self);
        }

        let mut url = self.clone();
        url.remove_credentials();
        Cow::Owned(url)
    }

    /// Returns [`Display`] implementation that doesn't mask credentials.
    #[inline]
    pub fn displayable_with_credentials(&self) -> impl Display {
        &self.0
    }

    /// Redact all occurrences of this URL in a message.
    ///
    /// This is useful for errors from external tools, which may include the credentialed URL in
    /// their command or output instead of using the URL's [`Display`] implementation.
    pub fn redact_in(&self, message: &str) -> String {
        message.replace(self.0.as_str(), &self.to_string())
    }

    /// Serialize the complete URL using the internal representation of the `Url` struct.
    ///
    /// A field using this serializer must declare the [`Raw`] policy:
    ///
    /// ```compile_fail
    /// use uv_redacted::{SafeUrl, policy::Raw};
    ///
    /// #[derive(serde::Serialize)]
    /// struct CachedUrl {
    ///     #[serde(serialize_with = "SafeUrl::<Raw>::serialize_internal")]
    ///     url: SafeUrl,
    /// }
    /// ```
    ///
    /// `serialize_internal` method calls require [`Raw`] as well:
    ///
    /// ```compile_fail
    /// use uv_redacted::SafeUrl;
    ///
    /// let url = SafeUrl::parse("https://user:password@example.com")?;
    /// url.serialize_internal(serde_json::value::Serializer)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// ```compile_fail
    /// use uv_redacted::{SafeUrl, policy::Stripped};
    ///
    /// let url = SafeUrl::parse("https://user:password@example.com")?
    ///     .with_policy(Stripped);
    /// url.serialize_internal(serde_json::value::Serializer)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[inline]
    pub fn serialize_internal<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        P: InternalSerializationPolicy,
        S: serde::Serializer,
    {
        self.0.serialize_internal(serializer)
    }
}

impl SafeUrl<Raw> {
    /// Deserialize the internal representation of the `Url` struct.
    #[inline]
    pub fn deserialize_internal<'de, D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(Url::deserialize_internal(deserializer)?, PhantomData))
    }
}

impl<P: SerializationPolicy> Display for SafeUrl<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        display_with_redacted_credentials(&self.0, f)
    }
}

impl<P: SerializationPolicy> Debug for SafeUrl<P> {
    #[inline(never)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let url = &self.0;
        let (username, password) = redacted_credentials(url);

        f.debug_struct("SafeUrl")
            .field("scheme", &url.scheme())
            .field("cannot_be_a_base", &url.cannot_be_a_base())
            .field("username", &username)
            .field("password", &password)
            .field("host", &url.host())
            .field("port", &url.port())
            .field("path", &url.path())
            .field(
                "query",
                &url.query()
                    .map(|query| redacted_query(query, url.query_pairs())),
            )
            .field("fragment", &url.fragment())
            .finish()
    }
}

impl From<Url> for SafeUrl {
    fn from(url: Url) -> Self {
        Self(url, PhantomData)
    }
}

impl FromStr for SafeUrl {
    type Err = SafeUrlError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input)
    }
}

impl<'de, P: SerializationPolicy> Deserialize<'de> for SafeUrl<P> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Policies do not select parsing rules. Use `SafeUrl::parse` to check human input.
        Url::deserialize(deserializer).map(|url| Self(url, PhantomData))
    }
}

fn is_ssh_git_username(url: &Url) -> bool {
    matches!(url.scheme(), "ssh" | "git+ssh" | "git+https")
        && url.username() == "git"
        && url.password().is_none()
}

/// Returns the URL's username and password with sensitive values redacted for display.
fn redacted_credentials(url: &Url) -> (&str, Option<&str>) {
    match (url.username(), url.password()) {
        (username, Some(_)) => (username, Some("****")),
        ("", None) => ("", None),
        // The generic Git username is not sensitive.
        (username, None) if is_ssh_git_username(url) => (username, None),
        // Other standalone usernames may be tokens.
        (_, None) => ("****", None),
    }
}

fn is_sensitive_query_parameter(key: &str) -> bool {
    SENSITIVE_QUERY_PARAMETERS
        .iter()
        .any(|sensitive| key.eq_ignore_ascii_case(sensitive))
}

fn redacted_query<'a>(
    query: &'a str,
    query_pairs: impl Iterator<Item = (Cow<'a, str>, Cow<'a, str>)>,
) -> Cow<'a, str> {
    let mut redacted = false;
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in query_pairs {
        if is_sensitive_query_parameter(&key) {
            serializer.append_pair(&key, "****");
            redacted = true;
        } else {
            serializer.append_pair(&key, &value);
        }
    }

    if redacted {
        Cow::Owned(serializer.finish())
    } else {
        Cow::Borrowed(query)
    }
}

fn display_with_redacted_credentials(
    url: &Url,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    write!(f, "{}:", url.scheme())?;

    if url.has_authority() {
        write!(f, "//")?;

        match redacted_credentials(url) {
            ("", None) => {}
            (username, Some(password)) => write!(f, "{username}:{password}@")?,
            (username, None) => write!(f, "{username}@")?,
        }

        write!(f, "{}", url.host_str().unwrap_or(""))?;

        if let Some(port) = url.port() {
            write!(f, ":{port}")?;
        }
    }

    write!(f, "{}", url.path())?;
    if let Some(query) = url.query() {
        write!(f, "?{}", redacted_query(query, url.query_pairs()))?;
    }
    if let Some(fragment) = url.fragment() {
        write!(f, "#{fragment}")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use indoc::{formatdoc, indoc};
    use insta::assert_debug_snapshot;

    use super::*;

    #[test]
    fn parse_relaxed() -> Result<(), url::ParseError> {
        let input = "https://user/name:password@example.com/project";
        let url = SafeUrl::parse_relaxed(input)?;
        assert_eq!(url.host_str(), Some("user"));
        assert_eq!(url.path(), "/name:password@example.com/project");
        insta::assert_snapshot!(SafeUrl::parse(input).unwrap_err(), @"ambiguous user/pass authority in URL (not percent-encoded?): https:***@example.com/project");
        assert_eq!(
            SafeUrl::parse_relaxed("https://").unwrap_err(),
            url::ParseError::EmptyHost
        );
        Ok(())
    }

    #[test]
    fn from_url_no_credentials() {
        let url_str = "https://pypi-proxy.fly.dev/basic-auth/simple";
        let log_safe_url = SafeUrl::parse("https://pypi-proxy.fly.dev/basic-auth/simple").unwrap();
        assert_eq!(log_safe_url.username(), "");
        assert!(log_safe_url.password().is_none());
        assert_eq!(log_safe_url.to_string(), url_str);
    }

    #[test]
    fn from_url_username_and_password() {
        let log_safe_url =
            SafeUrl::parse("https://user:pass@pypi-proxy.fly.dev/basic-auth/simple").unwrap();
        assert_eq!(log_safe_url.username(), "user");
        assert!(log_safe_url.password().is_some_and(|p| p == "pass"));
        assert_eq!(
            log_safe_url.to_string(),
            "https://user:****@pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn from_url_just_password() {
        let log_safe_url =
            SafeUrl::parse("https://:pass@pypi-proxy.fly.dev/basic-auth/simple").unwrap();
        assert_eq!(log_safe_url.username(), "");
        assert!(log_safe_url.password().is_some_and(|p| p == "pass"));
        assert_eq!(
            log_safe_url.to_string(),
            "https://:****@pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn from_url_just_username() {
        let log_safe_url =
            SafeUrl::parse("https://user@pypi-proxy.fly.dev/basic-auth/simple").unwrap();
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
        let ssh_url = SafeUrl::parse(ssh_str).unwrap();
        assert_eq!(ssh_url.username(), "git");
        assert!(ssh_url.password().is_none());
        assert_eq!(ssh_url.to_string(), ssh_str);
        // Test again for the `git+ssh` scheme
        let git_ssh_str = "git+ssh://git@github.com/org/repo";
        let git_ssh_url = SafeUrl::parse(git_ssh_str).unwrap();
        assert_eq!(git_ssh_url.username(), "git");
        assert!(git_ssh_url.password().is_none());
        assert_eq!(git_ssh_url.to_string(), git_ssh_str);
    }

    #[test]
    fn parse_url_string() {
        let url_str = "https://user:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let log_safe_url = SafeUrl::parse(url_str).unwrap();
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
        let mut log_safe_url = SafeUrl::parse(url_str).unwrap();
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
        let mut ssh_url = SafeUrl::parse(ssh_str).unwrap();
        ssh_url.remove_credentials();
        assert_eq!(ssh_url.username(), "git");
        assert!(ssh_url.password().is_none());
        assert_eq!(ssh_url.to_string(), ssh_str);
        // Test again for `git+ssh` scheme
        let git_ssh_str = "git+ssh://git@pypi-proxy.fly.dev/basic-auth/simple";
        let mut git_shh_url = SafeUrl::parse(git_ssh_str).unwrap();
        git_shh_url.remove_credentials();
        assert_eq!(git_shh_url.username(), "git");
        assert!(git_shh_url.password().is_none());
        assert_eq!(git_shh_url.to_string(), git_ssh_str);
    }

    #[test]
    fn displayable_with_credentials() {
        let url_str = "https://user:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let log_safe_url = SafeUrl::parse(url_str).unwrap();
        assert_eq!(
            log_safe_url.displayable_with_credentials().to_string(),
            url_str
        );
    }

    #[test]
    fn redact_url_in_message() {
        let url = SafeUrl::parse("https://user:pass@example.com/org/repo.git").unwrap();
        let message = formatdoc! {"
            process didn't exit successfully: `git fetch '{}'`
            --- stderr
            fatal: Authentication failed for '{}'
        ",
            url.as_str(),
            url.as_str()
        };

        assert_eq!(
            url.redact_in(&message),
            indoc! {"
                process didn't exit successfully: `git fetch 'https://user:****@example.com/org/repo.git'`
                --- stderr
                fatal: Authentication failed for 'https://user:****@example.com/org/repo.git'
            "}
        );
    }

    #[test]
    fn redact_presigned_url_in_message() {
        let url = SafeUrl::parse(
            "https://bucket.s3.amazonaws.com/dist.whl?X-Amz%2DSignature=signature&X-Amz-Credential=credential&X-Amz-Security-Token=token&safe=value",
        )
        .unwrap();
        let message = format!("failed to fetch '{}'", url.as_str());

        assert_eq!(
            url.redact_in(&message),
            "failed to fetch 'https://bucket.s3.amazonaws.com/dist.whl?X-Amz-Signature=****&X-Amz-Credential=****&X-Amz-Security-Token=****&safe=value'"
        );
    }

    #[test]
    fn redact_aws_presigned_query_values() {
        let log_safe_url = SafeUrl::parse(
            "https://bucket.s3.amazonaws.com/dist.whl?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=credential&X-Amz-Date=20260424T120000Z&X-Amz-Expires=300&X-Amz-SignedHeaders=host&X-Amz-Signature=signature&X-Amz-Security-Token=token",
        )
        .unwrap();

        assert_eq!(
            log_safe_url.to_string(),
            "https://bucket.s3.amazonaws.com/dist.whl?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=****&X-Amz-Date=20260424T120000Z&X-Amz-Expires=300&X-Amz-SignedHeaders=host&X-Amz-Signature=****&X-Amz-Security-Token=****"
        );
    }

    #[test]
    fn redact_azure_shared_access_signature() -> Result<(), SafeUrlError> {
        let url = SafeUrl::parse(
            "https://example.blob.core.windows.net/dist.whl?sv=2026-01-01&sig=signature&sp=r",
        )?;
        assert_eq!(
            url.to_string(),
            "https://example.blob.core.windows.net/dist.whl?sv=2026-01-01&sig=****&sp=r"
        );
        assert_eq!(
            url.redact_in(&format!("failed to fetch '{}'", url.as_str())),
            "failed to fetch 'https://example.blob.core.windows.net/dist.whl?sv=2026-01-01&sig=****&sp=r'"
        );
        // Formatting must not alter the signature used in actual requests.
        assert_eq!(
            url.as_str(),
            "https://example.blob.core.windows.net/dist.whl?sv=2026-01-01&sig=signature&sp=r"
        );
        Ok(())
    }

    #[test]
    fn redact_aws_presigned_query_values_case_insensitive() {
        let log_safe_url = SafeUrl::parse(
            "https://bucket.s3.amazonaws.com/dist.whl?x-amz-credential=credential&x-amz-signature=signature&x-amz-security-token=token",
        )
        .unwrap();

        assert_eq!(
            log_safe_url.to_string(),
            "https://bucket.s3.amazonaws.com/dist.whl?x-amz-credential=****&x-amz-signature=****&x-amz-security-token=****"
        );
    }

    #[test]
    fn redact_aws_presigned_query_values_with_percent_encoded_keys() {
        let log_safe_url = SafeUrl::parse(
            "https://bucket.s3.amazonaws.com/dist.whl?X-Amz%2DSignature=signature&safe=value",
        )
        .unwrap();

        assert_eq!(
            log_safe_url.to_string(),
            "https://bucket.s3.amazonaws.com/dist.whl?X-Amz-Signature=****&safe=value"
        );
    }

    #[test]
    fn redact_aws_presigned_query_values_in_debug() {
        let log_safe_url = SafeUrl::parse(
            "https://bucket.s3.amazonaws.com/dist.whl?X-Amz-Credential=credential&X-Amz-Signature=signature",
        )
        .unwrap();

        let debug = format!("{log_safe_url:?}");
        assert!(debug.contains(r#"query: Some("X-Amz-Credential=****&X-Amz-Signature=****")"#));
        assert!(!debug.contains("credential"));
        assert!(!debug.contains("signature"));
    }

    #[test]
    fn redact_azure_sas_query_signature() {
        let urls = [
            "https://account.blob.core.windows.net/container/dist.whl?sv=2024-11-04&sr=b&sig=signature&sp=r",
            "https://account.blob.core.windows.net/container/dist.whl?SIG=signature&safe=value",
        ]
        .map(|url| SafeUrl::parse(url).unwrap().to_string());

        assert_debug_snapshot!(urls, @r#"
        [
            "https://account.blob.core.windows.net/container/dist.whl?sv=2024-11-04&sr=b&sig=****&sp=r",
            "https://account.blob.core.windows.net/container/dist.whl?SIG=****&safe=value",
        ]
        "#);
    }

    #[test]
    fn does_not_redact_unknown_query_values() {
        let log_safe_url =
            SafeUrl::parse("https://bucket.s3.amazonaws.com/dist.whl?token=secret").unwrap();

        assert_eq!(
            log_safe_url.to_string(),
            "https://bucket.s3.amazonaws.com/dist.whl?token=secret"
        );
    }

    #[test]
    fn does_not_add_authority_to_urls_without_authority() {
        let log_safe_url = SafeUrl::parse("c:/home/ferris/projects/foo").unwrap();

        assert_eq!(log_safe_url.to_string(), "c:/home/ferris/projects/foo");
    }

    #[test]
    fn redacts_query_values_in_urls_without_authority() {
        let log_safe_url =
            SafeUrl::parse("c:/home/ferris/projects/foo?X-Amz-Signature=signature").unwrap();

        assert_eq!(
            log_safe_url.to_string(),
            "c:/home/ferris/projects/foo?X-Amz-Signature=****"
        );
    }

    #[test]
    fn redacts_query_values_in_cannot_be_a_base_urls() {
        let log_safe_url =
            SafeUrl::parse("mailto:ferris@example.com?X-Amz-Signature=signature").unwrap();

        assert!(log_safe_url.cannot_be_a_base());
        assert_eq!(
            log_safe_url.to_string(),
            "mailto:ferris@example.com?X-Amz-Signature=****"
        );
    }

    #[test]
    fn url_join() {
        let url_str = "https://token@example.com/abc/";
        let log_safe_url = SafeUrl::parse(url_str).unwrap();
        let foo_url = log_safe_url.join("foo").unwrap();
        assert_eq!(foo_url.to_string(), "https://****@example.com/abc/foo");
    }

    #[test]
    fn log_safe_url_ref() {
        let url_str = "https://user:pass@pypi-proxy.fly.dev/basic-auth/simple";
        let url = SafeUrl::parse(url_str).unwrap();
        let log_safe_url = SafeUrl::ref_cast(&url.0);
        assert_eq!(log_safe_url.username(), "user");
        assert!(log_safe_url.password().is_some_and(|p| p == "pass"));
        assert_eq!(
            log_safe_url.to_string(),
            "https://user:****@pypi-proxy.fly.dev/basic-auth/simple"
        );
    }

    #[test]
    fn parse_url_ambiguous() {
        for url in &[
            "https://user/name:password@domain/a/b/c",
            "https://user\\name:password@domain/a/b/c",
            "https://user#name:password@domain/a/b/c",
            "https://user.com/name:password@domain/a/b/c",
        ] {
            let err = SafeUrl::parse(url).unwrap_err();
            match err {
                SafeUrlError::AmbiguousAuthority(redacted) => {
                    assert!(redacted.starts_with("https:***@domain/a/b/c"));
                }
                SafeUrlError::Url(_) => panic!("expected AmbiguousAuthority error"),
            }
        }
    }

    #[test]
    fn parse_url_not_ambiguous() {
        for url in &[
            // https://github.com/astral-sh/uv/issues/16756
            "file:///C:/jenkins/ython_Environment_Manager_PR-251@2/venv%201/workspace",
            // https://github.com/astral-sh/uv/issues/17214
            // Git proxy URLs with nested schemes should not trigger the ambiguity check
            "git+https://githubproxy.cc/https://github.com/user/repo.git@branch",
            "git+https://proxy.example.com/https://github.com/org/project@v1.0.0",
            "git+https://proxy.example.com/https://github.com/org/project@refs/heads/main",
            // https://github.com/astral-sh/uv/issues/19887
            // Windows `git+file://` URLs pair a drive-letter `:` with an `@` revision, but use a
            // file transport and so carry no credentials.
            "git+file:///C:/Users/ferris/repo.git@v1.0",
            "git+file:///C:/Users/ferris/repo.git@10c049896212932ad5f7b19456d90bc604eeca53",
            "hg+file:///C:/Users/ferris/repo@default",
        ] {
            SafeUrl::parse(url).unwrap();
        }
    }

    #[test]
    fn credential_like_pattern() {
        assert!(!has_credential_like_pattern(
            "/https://github.com/user/repo.git@branch"
        ));
        assert!(!has_credential_like_pattern("/http://example.com/path@ref"));

        assert!(has_credential_like_pattern("/name:password@domain/a/b/c"));
        assert!(has_credential_like_pattern(":password@domain"));
    }
}
