use std::fmt::Display;
use std::fmt::Formatter;
use std::hash::BuildHasherDefault;
use std::sync::Arc;
use std::sync::RwLock;

use rustc_hash::{FxHashMap, FxHasher};
use tracing::trace;
use url::Url;

use uv_once_map::OnceMap;
use uv_redacted::DisplaySafeUrl;

use crate::Realm;
use crate::credentials::{Credentials, Username};

type FxOnceMap<K, V> = OnceMap<K, V, BuildHasherDefault<FxHasher>>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum FetchUrl {
    /// A full index URL
    Index(DisplaySafeUrl),
    /// A realm URL
    Realm(Realm),
}

impl Display for FetchUrl {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            Self::Index(index) => Display::fmt(index, f),
            Self::Realm(realm) => Display::fmt(realm, f),
        }
    }
}

pub struct CredentialsCache {
    /// A cache per realm and username
    realms: RwLock<FxHashMap<(Realm, Username), Arc<Credentials>>>,
    /// A cache tracking the result of realm or index URL fetches from external services
    pub(crate) fetches: FxOnceMap<(FetchUrl, Username), Option<Arc<Credentials>>>,
    /// A cache per URL, uses a trie for efficient prefix queries.
    urls: RwLock<UrlTrie>,
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
            fetches: FxOnceMap::default(),
            realms: RwLock::new(FxHashMap::default()),
            urls: RwLock::new(UrlTrie::new()),
        }
    }

    /// Return the credentials that should be used for a realm and username, if any.
    pub(crate) fn get_realm(&self, realm: Realm, username: Username) -> Option<Arc<Credentials>> {
        let realms = self.realms.read().unwrap();
        let given_username = username.is_some();
        let key = (realm, username);

        let Some(credentials) = realms.get(&key).cloned() else {
            trace!(
                "No credentials in cache for realm {}",
                RealmUsername::from(key)
            );
            return None;
        };

        if given_username && credentials.password().is_none() {
            // If given a username, don't return password-less credentials
            trace!(
                "No password in cache for realm {}",
                RealmUsername::from(key)
            );
            return None;
        }

        trace!(
            "Found cached credentials for realm {}",
            RealmUsername::from(key)
        );
        Some(credentials)
    }

    /// Return the cached credentials for a URL and username, if any.
    ///
    /// Note we do not cache per username, but if a username is passed we will confirm that the
    /// cached credentials have a username equal to the provided one — otherwise `None` is returned.
    /// If multiple usernames are used per URL, the realm cache should be queried instead.
    pub(crate) fn get_url(&self, url: &Url, username: &Username) -> Option<Arc<Credentials>> {
        let urls = self.urls.read().unwrap();
        let credentials = urls.get(url);
        if let Some(credentials) = credentials {
            if username.is_none() || username.as_deref() == credentials.username() {
                if username.is_some() && credentials.password().is_none() {
                    // If given a username, don't return password-less credentials
                    trace!("No password in cache for URL {url}");
                    return None;
                }
                trace!("Found cached credentials for URL {url}");
                return Some(credentials.clone());
            }
        }
        trace!("No credentials in cache for URL {url}");
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
            let realm = (Realm::from(url), username);
            self.insert_realm(realm, &credentials);
        }

        // Insert an entry for requests with no username
        self.insert_realm((Realm::from(url), Username::none()), &credentials);

        // Insert an entry for the URL
        let mut urls = self.urls.write().unwrap();
        urls.insert(url, credentials);
    }

    /// Private interface to update a realm cache entry.
    ///
    /// Returns replaced credentials, if any.
    fn insert_realm(
        &self,
        key: (Realm, Username),
        credentials: &Arc<Credentials>,
    ) -> Option<Arc<Credentials>> {
        // Do not cache empty credentials
        if credentials.is_empty() {
            return None;
        }

        let mut realms = self.realms.write().unwrap();

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
        let realm = Realm::from(url).to_string();
        for component in [realm.as_str()]
            .into_iter()
            .chain(url.path_segments().unwrap().filter(|item| !item.is_empty()))
        {
            state = self.states[state].get(component)?;
            if let Some(ref value) = self.states[state].value {
                return Some(value);
            }
        }
        self.states[state].value.as_ref()
    }

    fn insert(&mut self, url: &Url, value: Arc<Credentials>) {
        let mut state = 0;
        let realm = Realm::from(url).to_string();
        for component in [realm.as_str()]
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

#[derive(Debug)]
struct RealmUsername(Realm, Username);

impl std::fmt::Display for RealmUsername {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self(realm, username) = self;
        if let Some(username) = username.as_deref() {
            write!(f, "{username}@{realm}")
        } else {
            write!(f, "{realm}")
        }
    }
}

impl From<(Realm, Username)> for RealmUsername {
    fn from((realm, username): (Realm, Username)) -> Self {
        Self(realm, username)
    }
}

#[cfg(test)]
mod tests {
    use crate::credentials::Password;

    use super::*;

    #[test]
    fn test_trie() {
        let credentials1 = Arc::new(Credentials::basic(
            Some("username1".to_string()),
            Some("password1".to_string()),
        ));
        let credentials2 = Arc::new(Credentials::basic(
            Some("username2".to_string()),
            Some("password2".to_string()),
        ));
        let credentials3 = Arc::new(Credentials::basic(
            Some("username3".to_string()),
            Some("password3".to_string()),
        ));
        let credentials4 = Arc::new(Credentials::basic(
            Some("username4".to_string()),
            Some("password4".to_string()),
        ));

        let mut trie = UrlTrie::new();
        trie.insert(
            &Url::parse("https://burntsushi.net").unwrap(),
            credentials1.clone(),
        );
        trie.insert(
            &Url::parse("https://astral.sh").unwrap(),
            credentials2.clone(),
        );
        trie.insert(
            &Url::parse("https://example.com/foo").unwrap(),
            credentials3.clone(),
        );
        trie.insert(
            &Url::parse("https://example.com/bar").unwrap(),
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

    #[test]
    fn test_url_with_credentials() {
        let username = Username::new(Some(String::from("username")));
        let password = Password::new(String::from("password"));
        let credentials = Arc::new(Credentials::Basic {
            username: username.clone(),
            password: Some(password),
        });
        let cache = CredentialsCache::default();
        // Insert with URL with credentials and get with redacted URL.
        let url = Url::parse("https://username:password@example.com/foobar").unwrap();
        cache.insert(&url, credentials.clone());
        assert_eq!(cache.get_url(&url, &username), Some(credentials.clone()));
        // Insert with redacted URL and get with URL with credentials.
        let url = Url::parse("https://username:password@second-example.com/foobar").unwrap();
        cache.insert(&url, credentials.clone());
        assert_eq!(cache.get_url(&url, &username), Some(credentials.clone()));
    }
}
