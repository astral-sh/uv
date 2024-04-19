use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};

use crate::credentials::{Credentials, Username};
use crate::NetLoc;

use tracing::trace;
use url::Url;

pub struct CredentialsCache {
    realms: Mutex<HashMap<(NetLoc, Username), Arc<Credentials>>>,
    #[allow(clippy::type_complexity)]
    urls: Mutex<UrlTrie>,
}

impl Default for CredentialsCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialsCache {
    /// Create a new cache.
    pub fn new() -> Self {
        Self {
            realms: Mutex::new(HashMap::new()),
            urls: Mutex::new(UrlTrie::new()),
        }
    }

    /// Return the credentials that should be used for a realm, if any.
    pub(crate) fn get_realm(&self, netloc: NetLoc, username: Username) -> Option<Arc<Credentials>> {
        let realms = self.realms.lock().unwrap();
        let name = if let Some(username) = username.as_deref() {
            format!("{username}@{netloc}")
        } else {
            netloc.to_string()
        };
        let key = (netloc, username);

        realms
            .get(&key)
            .cloned()
            .map(Some)
            .inspect(|_| trace!("Found cached credentials for realm {name}"))
            .unwrap_or_else(|| {
                trace!("No credentials in cache for realm {name}");
                None
            })
    }

    /// Return the cached credentials for a URL, if any.
    ///
    /// Note we do not cache per URL and username, but if a username is passed we will confirm that the
    /// cached username is equal to the provided one otherwise `None` is returned.
    pub(crate) fn get_url(&self, url: &Url, username: Username) -> Option<Arc<Credentials>> {
        let urls = self.urls.lock().unwrap();
        let credentials = urls.get(url);
        if let Some(credentials) = credentials {
            if username.is_none() || username.as_deref() == credentials.username() {
                trace!("Found cached credentials for URL {url}");
                return Some(credentials.clone());
            }
        }
        trace!("No credentials in URL cache for {url}");
        None
    }

    /// Update the cache with the given credentials.
    pub(crate) fn insert(&self, url: &Url, credentials: Arc<Credentials>) {
        // Do not cache empty credentials
        if credentials.is_empty() {
            return;
        }

        // Insert an entry for requests including the username
        let username = credentials.to_username();
        if username.is_some() {
            let realm = (NetLoc::from(url), username.clone());
            self.insert_realm(realm, credentials.clone());
        }

        // Insert an entry for requests with no username
        self.insert_realm((NetLoc::from(url), Username::none()), credentials.clone());

        // Insert an entry for the URL
        let mut urls = self.urls.lock().unwrap();
        urls.insert(url.clone(), credentials.clone());
    }

    /// Private interface to update a realm cache entry.
    ///
    /// Returns replaced credentials, if any.
    fn insert_realm(
        &self,
        key: (NetLoc, Username),
        credentials: Arc<Credentials>,
    ) -> Option<Arc<Credentials>> {
        // Do not cache empty credentials
        if credentials.is_empty() {
            return None;
        }

        let mut realms = self.realms.lock().unwrap();

        // Always replace existing entries if we have a password
        if credentials.password().is_some() {
            return realms.insert(key, credentials.clone());
        }

        // If we only have a username, add a new entry or replace an existing entry if it doesn't have a password
        let existing = realms.get(&key);
        if existing.is_none()
            || existing.is_some_and(|credentials| credentials.password().is_none())
        {
            return realms.insert(key, credentials.clone());
        }

        None
    }
}

#[derive(Debug)]
struct UrlTrie {
    states: Vec<TrieState>,
}

#[derive(Debug, Default)]
struct TrieState {
    children: Vec<(String, usize)>,
    value: Option<Arc<Credentials>>,
}

impl UrlTrie {
    fn new() -> UrlTrie {
        let mut trie = UrlTrie { states: vec![] };
        trie.alloc();
        trie
    }

    fn get(&self, url: &Url) -> Option<&Arc<Credentials>> {
        let mut state = 0;
        let netloc = NetLoc::from(url).to_string();
        for component in [netloc.as_str()]
            .into_iter()
            .chain(url.path_segments().unwrap().filter(|item| !item.is_empty()))
        {
            dbg!(component);
            state = self.states[state].get(component)?;
            if let Some(ref value) = self.states[state].value {
                return Some(value);
            }
        }
        self.states[state].value.as_ref()
    }

    fn insert(&mut self, url: Url, value: Arc<Credentials>) {
        let mut state = 0;
        let netloc = NetLoc::from(&url).to_string();
        for component in [netloc.as_str()]
            .into_iter()
            .chain(url.path_segments().unwrap().filter(|item| !item.is_empty()))
        {
            match self.states[state].index(component) {
                Ok(i) => state = self.states[state].children[i].1,
                Err(i) => {
                    let new_state = self.alloc();
                    self.states[state]
                        .children
                        .insert(i, (component.to_string(), new_state));
                    state = new_state;
                }
            }
        }
        self.states[state].value = Some(value);
    }

    fn alloc(&mut self) -> usize {
        let id = self.states.len();
        self.states.push(TrieState::default());
        id
    }
}

impl TrieState {
    fn get(&self, component: &str) -> Option<usize> {
        let i = self.index(component).ok()?;
        Some(self.children[i].1)
    }

    fn index(&self, component: &str) -> Result<usize, usize> {
        self.children
            .binary_search_by(|(label, _)| label.as_str().cmp(component))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trie() {
        let credentials1 = Arc::new(Credentials::new(
            Some("username1".to_string()),
            Some("password1".to_string()),
        ));
        let credentials2 = Arc::new(Credentials::new(
            Some("username2".to_string()),
            Some("password2".to_string()),
        ));
        let credentials3 = Arc::new(Credentials::new(
            Some("username3".to_string()),
            Some("password3".to_string()),
        ));
        let credentials4 = Arc::new(Credentials::new(
            Some("username4".to_string()),
            Some("password4".to_string()),
        ));

        let mut trie = UrlTrie::new();
        trie.insert(
            Url::parse("https://burntsushi.net").unwrap(),
            credentials1.clone(),
        );
        trie.insert(
            Url::parse("https://astral.sh").unwrap(),
            credentials2.clone(),
        );
        trie.insert(
            Url::parse("https://example.com/foo").unwrap(),
            credentials3.clone(),
        );
        trie.insert(
            Url::parse("https://example.com/bar").unwrap(),
            credentials4.clone(),
        );

        let url = Url::parse("https://burntsushi.net/regex-internals").unwrap();
        assert_eq!(trie.get(&url), Some(&credentials1));

        let url = Url::parse("https://burntsushi.net/").unwrap();
        assert_eq!(trie.get(&url), Some(&credentials1));

        let url = Url::parse("https://astral.sh/about").unwrap();
        assert_eq!(trie.get(&url), Some(&credentials2));

        let url = Url::parse("https://example.com/foo").unwrap();
        assert_eq!(trie.get(&url), Some(&credentials3));

        let url = Url::parse("https://example.com/foo/").unwrap();
        assert_eq!(trie.get(&url), Some(&credentials3));

        let url = Url::parse("https://example.com/foo/bar").unwrap();
        assert_eq!(trie.get(&url), Some(&credentials3));

        let url = Url::parse("https://example.com/bar").unwrap();
        assert_eq!(trie.get(&url), Some(&credentials4));

        let url = Url::parse("https://example.com/bar/").unwrap();
        assert_eq!(trie.get(&url), Some(&credentials4));

        let url = Url::parse("https://example.com/bar/foo").unwrap();
        assert_eq!(trie.get(&url), Some(&credentials4));

        let url = Url::parse("https://example.com/about").unwrap();
        assert_eq!(trie.get(&url), None);

        let url = Url::parse("https://example.com/foobar").unwrap();
        assert_eq!(trie.get(&url), None);
    }
}
