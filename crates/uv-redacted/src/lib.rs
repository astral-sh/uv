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
    if url.password().is_some() {
        let _ = url.set_password(Some("****"));
    // A username on its own might be a secret token.
    } else if url.username() != "" {
        let _ = url.set_username("****");
    }
    Cow::Owned(url)
}
