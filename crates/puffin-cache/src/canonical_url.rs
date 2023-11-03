use url::Url;

use crate::cache_key::{CacheKey, CacheKeyHasher};

/// A wrapper around `Url` which represents a "canonical" version of an
/// original URL.
///
/// A "canonical" url is only intended for internal comparison purposes. It's
/// to help paper over mistakes such as depending on `github.com/foo/bar` vs.
/// `github.com/foo/bar.git`. This is **only** for internal purposes and
/// provides no means to actually read the underlying string value of the `Url`
/// it contains. This is intentional, because all fetching should still happen
/// within the context of the original URL.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct CanonicalUrl(Url);

impl CanonicalUrl {
    pub fn new(url: &Url) -> CanonicalUrl {
        let mut url = url.clone();

        // Strip a trailing slash.
        if url.path().ends_with('/') {
            url.path_segments_mut().unwrap().pop_if_empty();
        }

        // If a URL starts with a kind (like `git+`), remove it.
        if let Some(suffix) = url.as_str().strip_prefix("git+") {
            // If a Git URL ends in a reference (like a branch, tag, or commit), remove it.
            if let Some((prefix, _)) = suffix.rsplit_once('@') {
                url = prefix.parse().unwrap();
            } else {
                url = suffix.parse().unwrap();
            }
        }

        // For GitHub URLs specifically, just lower-case everything. GitHub
        // treats both the same, but they hash differently, and we're gonna be
        // hashing them. This wants a more general solution, and also we're
        // almost certainly not using the same case conversion rules that GitHub
        // does. (See issue #84)
        if url.host_str() == Some("github.com") {
            url = format!("https{}", &url[url::Position::AfterScheme..])
                .parse()
                .unwrap();
            let path = url.path().to_lowercase();
            url.set_path(&path);
        }

        // Repos can generally be accessed with or without `.git` extension.
        let needs_chopping = std::path::Path::new(url.path())
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("git"));
        if needs_chopping {
            let last = {
                let last = url.path_segments().unwrap().next_back().unwrap();
                last[..last.len() - 4].to_owned()
            };
            url.path_segments_mut().unwrap().pop().push(&last);
        }

        CanonicalUrl(url)
    }
}

impl CacheKey for CanonicalUrl {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        // `as_str` gives the serialisation of a url (which has a spec) and so insulates against
        // possible changes in how the URL crate does hashing.
        self.0.as_str().cache_key(state);
    }
}
