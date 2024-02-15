use rustc_hash::FxHashSet;
use url::Url;

#[derive(Debug, Default)]
pub(crate) struct AllowedUrls(FxHashSet<cache_key::CanonicalUrl>);

impl AllowedUrls {
    pub(crate) fn contains(&self, url: &Url) -> bool {
        self.0.contains(&cache_key::CanonicalUrl::new(url))
    }
}

impl<'a> FromIterator<&'a Url> for AllowedUrls {
    fn from_iter<T: IntoIterator<Item = &'a Url>>(iter: T) -> Self {
        Self(iter.into_iter().map(cache_key::CanonicalUrl::new).collect())
    }
}
