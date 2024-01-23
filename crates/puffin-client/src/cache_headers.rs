use http::HeaderValue;
use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;

/// Cache headers from an HTTP response.
#[derive(Debug, Default)]
pub(crate) struct CacheHeaders(FxHashMap<Box<str>, Option<Box<str>>>);

impl CacheHeaders {
    /// Parse the `Cache-Control` header from an HTTP response.
    ///
    /// See: <https://github.com/kornelski/rusty-http-cache-semantics/blob/8fba3b9a3ddf01ba24f2d1a7994f4e9500644c1a/src/lib.rs#L51>
    pub(crate) fn from_response<'a>(
        headers: impl IntoIterator<Item = &'a HeaderValue>,
    ) -> CacheHeaders {
        let mut cc = FxHashMap::<Box<str>, Option<Box<str>>>::default();
        let mut is_valid = true;

        for h in headers.into_iter().filter_map(|v| v.to_str().ok()) {
            for part in h.split(',') {
                // TODO: lame parsing
                if part.trim().is_empty() {
                    continue;
                }
                let mut kv = part.splitn(2, '=');
                let k = kv.next().unwrap().trim();
                if k.is_empty() {
                    continue;
                }
                let v = kv.next().map(str::trim);
                match cc.entry(k.into()) {
                    Entry::Occupied(e) => {
                        // When there is more than one value present for a given directive (e.g., two Expires header fields, multiple Cache-Control: max-age directives),
                        // the directive's value is considered invalid. Caches are encouraged to consider responses that have invalid freshness information to be stale
                        if e.get().as_deref() != v {
                            is_valid = false;
                        }
                    }
                    Entry::Vacant(e) => {
                        e.insert(v.map(|v| v.trim_matches('"')).map(From::from));
                        // TODO: bad unquoting
                    }
                }
            }
        }
        if !is_valid {
            cc.insert("must-revalidate".into(), None);
        }
        Self(cc)
    }

    /// Returns `true` if the response has an `immutable` directive.
    pub(crate) fn is_immutable(&self) -> bool {
        self.0.contains_key("immutable")
    }
}
