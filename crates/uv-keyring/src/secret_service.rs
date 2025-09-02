/*!

# secret-service credential store

Items in the secret-service are identified by an arbitrary collection
of attributes.  This implementation controls the following attributes:

- `target` (optional & taken from entry creation call, defaults to `default`)
- `service` (required & taken from entry creation call)
- `username` (required & taken from entry creation call's `user` parameter)

In addition, when creating a new credential, this implementation assigns
two additional attributes:

- `application` (set to `uv`)
- `label` (set to a string with the user, service, target, and keyring version at time of creation)

Client code is allowed to retrieve and to set all attributes _except_ the
three that are controlled by this implementation. (N.B. The `label` string
is not actually an attribute; it's a required element in every item and is used
by GUI tools as the name for the item. But this implementation treats the
label as if it were any other non-controlled attribute, with the caveat that
it will reject any attempt to set the label to an empty string.)

Existing items are always searched for at the service level, which
means all collections are searched. The search attributes used are
`target` (set from the entry target), `service` (set from the entry
service), and `username` (set from the entry user). Because earlier
versions of this crate did not set the `target` attribute on credentials
that were stored in the default collection, a fallback search is done
for items in the default collection with no `target` attribute *if
the original search for all three attributes returns no matches*.

New items are created in the default collection,
unless a target other than `default` is
specified for the entry, in which case the item
will be created in a collection (created if necessary)
that is labeled with the specified target.

Setting the password on an entry will always update the password on an
existing item in preference to creating a new item.
This provides better compatibility with 3rd party clients, as well as earlier
versions of this crate, that may already
have created items that match the entry, and thus reduces the chance
of ambiguity in later searches.

## Headless usage

If you must use the secret-service on a headless linux box,
be aware that there are known issues with getting
dbus and secret-service and the gnome keyring
to work properly in headless environments.
For a quick workaround, look at how this project's
[CI workflow](https://github.com/hwchen/keyring-rs/blob/master/.github/workflows/ci.yaml)
starts the Gnome keyring unlocked with a known password;
a similar solution is also documented in the
[Python Keyring docs](https://pypi.org/project/keyring/)
(search for "Using Keyring on headless Linux systems").
The following `bash` function may be helpful:

```shell
function unlock-keyring ()
{
    read -rsp "Password: " pass
    echo -n "$pass" | gnome-keyring-daemon --unlock
    unset pass
}
```

For an excellent treatment of all the headless dbus issues, see
[this answer on ServerFault](https://serverfault.com/a/906224/79617).

## Usage - not! - on Windows Subsystem for Linux

As noted in
[this issue on GitHub](https://github.com/hwchen/keyring-rs/issues/133),
there is no "default" collection defined under WSL.  So
this keystore doesn't work "out of the box" on WSL.  See the
issue for more details and possible workarounds.
 */

use std::collections::HashMap;

use secret_service::{Collection, EncryptionType, Error, Item, SecretService};

use crate::credential::{Credential, CredentialApi, CredentialBuilder, CredentialBuilderApi};
use crate::error::{Error as ErrorCode, Result, decode_password};

/// The representation of an item in the secret-service.
///
/// This structure has two roles. On the one hand, it captures all the
/// information a user specifies for an [`Entry`](crate::Entry)
/// and so is the basis for our search
/// (or creation) of an item for that entry.  On the other hand, when
/// a search is ambiguous, each item found is represented by a credential that
/// has the same attributes and label as the item.
#[derive(Debug, Clone)]
pub struct SsCredential {
    pub attributes: HashMap<String, String>,
    pub label: String,
    target: Option<String>,
}

#[async_trait::async_trait]
impl CredentialApi for SsCredential {
    /// Sets the password on a unique matching item, if it exists, or creates one if necessary.
    ///
    /// If there are multiple matches,
    /// returns an [`Ambiguous`](ErrorCode::Ambiguous) error with a credential for each
    /// matching item.
    ///
    /// When creating, the item is put into a collection named by the credential's `target`
    /// attribute.
    async fn set_password(&self, password: &str) -> Result<()> {
        self.set_secret(password.as_bytes()).await
    }

    /// Sets the secret on a unique matching item, if it exists, or creates one if necessary.
    ///
    /// If there are multiple matches,
    /// returns an [`Ambiguous`](ErrorCode::Ambiguous) error with a credential for each
    /// matching item.
    ///
    /// When creating, the item is put into a collection named by the credential's `target`
    /// attribute.
    async fn set_secret(&self, secret: &[u8]) -> Result<()> {
        // first try to find a unique, existing, matching item and set its password
        let secret_vec = secret.to_vec();
        match self
            .map_matching_items(async move |i| set_item_secret(i, &secret_vec).await, true)
            .await
        {
            Ok(_) => return Ok(()),
            Err(ErrorCode::NoEntry) => {}
            Err(err) => return Err(err),
        }
        // if there is no existing item, create one for this credential.  In order to create
        // an item, the credential must have an explicit target.  All entries created with
        // the [`new`] or [`new_with_target`] commands will have explicit targets.  But entries
        // created to wrap 3rd-party items that don't have `target` attributes may not.
        let ss = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(platform_failure)?;
        let name = self.target.as_ref().ok_or_else(empty_target)?;
        let collection = match get_collection(&ss, name).await {
            Ok(collection) => collection,
            Err(_) => create_collection(&ss, name).await?,
        };
        collection
            .create_item(
                self.label.as_str(),
                self.all_attributes(),
                secret,
                true, // replace
                "text/plain",
            )
            .await
            .map_err(platform_failure)?;
        Ok(())
    }

    /// Gets the password on a unique matching item, if it exists.
    ///
    /// If there are no
    /// matching items, returns a [`NoEntry`](ErrorCode::NoEntry) error.
    /// If there are multiple matches,
    /// returns an [`Ambiguous`](ErrorCode::Ambiguous)
    /// error with a credential for each matching item.
    async fn get_password(&self) -> Result<String> {
        Ok(self
            .map_matching_items(get_item_password, true)
            .await?
            .remove(0))
    }

    /// Gets the secret on a unique matching item, if it exists.
    ///
    /// If there are no
    /// matching items, returns a [`NoEntry`](ErrorCode::NoEntry) error.
    /// If there are multiple matches,
    /// returns an [`Ambiguous`](ErrorCode::Ambiguous)
    /// error with a credential for each matching item.
    async fn get_secret(&self) -> Result<Vec<u8>> {
        Ok(self
            .map_matching_items(get_item_secret, true)
            .await?
            .remove(0))
    }

    /// Get attributes on a unique matching item, if it exists
    async fn get_attributes(&self) -> Result<HashMap<String, String>> {
        let attributes: Vec<HashMap<String, String>> =
            self.map_matching_items(get_item_attributes, true).await?;
        Ok(attributes.into_iter().next().unwrap())
    }

    /// Update attributes on a unique matching item, if it exists
    async fn update_attributes(&self, attributes: &HashMap<&str, &str>) -> Result<()> {
        // Convert to owned data to avoid lifetime issues
        let attributes_owned: HashMap<String, String> = attributes
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();

        self.map_matching_items(
            async move |item| {
                let attrs_ref: HashMap<&str, &str> = attributes_owned
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                update_item_attributes(item, &attrs_ref).await
            },
            true,
        )
        .await?;
        Ok(())
    }

    /// Deletes the unique matching item, if it exists.
    ///
    /// If there are no
    /// matching items, returns a [`NoEntry`](ErrorCode::NoEntry) error.
    /// If there are multiple matches,
    /// returns an [`Ambiguous`](ErrorCode::Ambiguous)
    /// error with a credential for each matching item.
    async fn delete_credential(&self) -> Result<()> {
        self.map_matching_items(delete_item, true).await?;
        Ok(())
    }

    /// Return the underlying credential object with an `Any` type so that it can
    /// be downgraded to an [`SsCredential`] for platform-specific processing.
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// Expose the concrete debug formatter for use via the [`Credential`] trait
    fn debug_fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl SsCredential {
    /// Create a credential for the given target, service, and user.
    ///
    /// The target defaults to `default` (the default secret-service collection).
    ///
    /// Creating this credential does not create a matching item.
    /// If there isn't already one there, it will be created only
    /// when [`set_password`](SsCredential::set_password) is
    /// called.
    pub fn new_with_target(target: Option<&str>, service: &str, user: &str) -> Result<Self> {
        if let Some("") = target {
            return Err(empty_target());
        }
        let target = target.unwrap_or("default");

        let attributes = HashMap::from([
            ("service".to_string(), service.to_string()),
            ("username".to_string(), user.to_string()),
            ("target".to_string(), target.to_string()),
            ("application".to_string(), "uv".to_string()),
        ]);
        Ok(Self {
            attributes,
            label: format!(
                "{user}@{service}:{target} (uv v{})",
                env!("CARGO_PKG_VERSION"),
            ),
            target: Some(target.to_string()),
        })
    }

    /// Create a credential that has *no* target and the given service and user.
    ///
    /// This emulates what keyring v1 did, and can be very handy when you need to
    /// access an old v1 credential that's in your secret service default collection.
    pub fn new_with_no_target(service: &str, user: &str) -> Result<Self> {
        let attributes = HashMap::from([
            ("service".to_string(), service.to_string()),
            ("username".to_string(), user.to_string()),
            ("application".to_string(), "uv".to_string()),
        ]);
        Ok(Self {
            attributes,
            label: format!(
                "uv v{} for no target, service '{service}', user '{user}'",
                env!("CARGO_PKG_VERSION"),
            ),
            target: None,
        })
    }

    /// Create a credential from an underlying item.
    ///
    /// The created credential will have all the attributes and label
    /// of the underlying item, so you can examine them.
    pub async fn new_from_item(item: &Item<'_>) -> Result<Self> {
        let attributes = item.get_attributes().await.map_err(decode_error)?;
        let target = attributes.get("target").cloned();
        Ok(Self {
            attributes,
            label: item.get_label().await.map_err(decode_error)?,
            target,
        })
    }

    /// Construct a credential for this credential's underlying matching item,
    /// if there is exactly one.
    pub async fn new_from_matching_item(&self) -> Result<Self> {
        Ok(self
            .map_matching_items(Self::new_from_item, true)
            .await?
            .remove(0))
    }

    /// If there are multiple matching items for this credential, get all of their passwords.
    ///
    /// (This is useful if [`get_password`](SsCredential::get_password)
    /// returns an [`Ambiguous`](ErrorCode::Ambiguous) error.)
    pub async fn get_all_passwords(&self) -> Result<Vec<String>> {
        self.map_matching_items(get_item_password, false).await
    }

    /// If there are multiple matching items for this credential, delete all of them.
    ///
    /// (This is useful if [`delete_credential`](SsCredential::delete_credential)
    /// returns an [`Ambiguous`](ErrorCode::Ambiguous) error.)
    pub async fn delete_all_passwords(&self) -> Result<()> {
        self.map_matching_items(delete_item, false).await?;
        Ok(())
    }

    /// Map an async function over the items matching this credential.
    ///
    /// Items are unlocked before the function is applied.
    ///
    /// If `require_unique` is true, and there are no matching items, then
    /// a [`NoEntry`](ErrorCode::NoEntry) error is returned.
    /// If `require_unique` is true, and there are multiple matches,
    /// then an [`Ambiguous`](ErrorCode::Ambiguous) error is returned
    /// with a vector containing one
    /// credential for each of the matching items.
    async fn map_matching_items<F, T>(&self, f: F, require_unique: bool) -> Result<Vec<T>>
    where
        F: AsyncFn(&Item<'_>) -> Result<T>,
        T: Sized,
    {
        let ss = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(platform_failure)?;
        let attributes: HashMap<&str, &str> = self.search_attributes(false).into_iter().collect();
        let search = ss.search_items(attributes).await.map_err(decode_error)?;
        let count = search.locked.len() + search.unlocked.len();
        if count == 0 {
            if let Some("default") = self.target.as_deref() {
                return self.map_matching_legacy_items(&ss, f, require_unique).await;
            }
        }
        if require_unique {
            if count == 0 {
                return Err(ErrorCode::NoEntry);
            } else if count > 1 {
                let mut creds: Vec<Box<Credential>> = vec![];
                for item in search.locked.iter().chain(search.unlocked.iter()) {
                    let cred = Self::new_from_item(item).await?;
                    creds.push(Box::new(cred));
                }
                return Err(ErrorCode::Ambiguous(creds));
            }
        }
        let mut results: Vec<T> = vec![];
        for item in &search.unlocked {
            results.push(f(item).await?);
        }
        for item in &search.locked {
            item.unlock().await.map_err(decode_error)?;
            results.push(f(item).await?);
        }
        Ok(results)
    }

    /// Map an async function over items that older versions of keyring
    /// would have matched against this credential.
    ///
    /// Keyring v1 created secret service items that had no target attribute, and it was
    /// only able to create items in the default collection. Keyring v2, and Keyring v3.1,
    /// in order to be able to find items set by keyring v1, would first look for items
    /// everywhere independent of target attribute, and then filter those found by the value
    /// of the target attribute. But this matching behavior overgeneralized when the keyring
    /// was locked at the time of the search (see
    /// [issue #204](https://github.com/hwchen/keyring-rs/issues/204) for details).
    ///
    /// As of keyring v3.2, the service-wide search behavior was changed to require a
    /// matching target on items. But, as pointed out in
    /// [issue #207](https://github.com/hwchen/keyring-rs/issues/207),
    /// this meant that items set by keyring v1 (or by 3rd party tools that didn't set
    /// the target attribute) would not be found, even if they were in the default
    /// collection.
    ///
    /// So with keyring v3.2.1, if the service-wide search fails to find any matching
    /// credential, and the credential being searched for has the default target, we fall back and search the default collection for a v1-style credential.
    /// That preserves the legacy behavior at the cost of a second round-trip through
    /// the secret service for the collection search.
    pub async fn map_matching_legacy_items<F, T>(
        &self,
        ss: &SecretService<'_>,
        f: F,
        require_unique: bool,
    ) -> Result<Vec<T>>
    where
        F: AsyncFn(&Item<'_>) -> Result<T>,
        T: Sized,
    {
        let collection = ss.get_default_collection().await.map_err(decode_error)?;
        let attributes = self.search_attributes(true);
        let search = collection
            .search_items(attributes)
            .await
            .map_err(decode_error)?;
        if require_unique {
            if search.is_empty() && require_unique {
                return Err(ErrorCode::NoEntry);
            } else if search.len() > 1 {
                let mut creds: Vec<Box<Credential>> = vec![];
                for item in &search {
                    let cred = Self::new_from_item(item).await?;
                    creds.push(Box::new(cred));
                }
                return Err(ErrorCode::Ambiguous(creds));
            }
        }
        let mut results: Vec<T> = vec![];
        for item in &search {
            results.push(f(item).await?);
        }
        Ok(results)
    }

    /// Using strings in the credential map makes managing the lifetime
    /// of the credential much easier.  But since the secret service expects
    /// a map from &str to &str, we have this utility to transform the
    /// credential's map into one of the right form.
    fn all_attributes(&self) -> HashMap<&str, &str> {
        self.attributes
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }

    /// Similar to [`all_attributes`](SsCredential::all_attributes),
    /// but this just selects the ones we search on
    fn search_attributes(&self, omit_target: bool) -> HashMap<&str, &str> {
        let mut result: HashMap<&str, &str> = HashMap::new();
        if self.target.is_some() && !omit_target {
            result.insert("target", self.attributes["target"].as_str());
        }
        result.insert("service", self.attributes["service"].as_str());
        result.insert("username", self.attributes["username"].as_str());
        result
    }
}

/// The builder for secret-service credentials
#[derive(Debug, Default)]
pub struct SsCredentialBuilder;

/// Returns an instance of the secret-service credential builder.
///
/// If secret-service is the default credential store,
/// this is called once when an entry is first created.
pub fn default_credential_builder() -> Box<CredentialBuilder> {
    Box::new(SsCredentialBuilder {})
}

impl CredentialBuilderApi for SsCredentialBuilder {
    /// Build an [`SsCredential`] for the given target, service, and user.
    fn build(&self, target: Option<&str>, service: &str, user: &str) -> Result<Box<Credential>> {
        Ok(Box::new(SsCredential::new_with_target(
            target, service, user,
        )?))
    }

    /// Return the underlying builder object with an `Any` type so that it can
    /// be downgraded to an [`SsCredentialBuilder`] for platform-specific processing.
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

//
// Secret Service utilities
//

/// Find the secret service collection whose label is the given name.
///
/// The name `default` is treated specially and is interpreted as naming
/// the default collection regardless of its label (which might be different).
pub async fn get_collection<'a>(ss: &'a SecretService<'_>, name: &str) -> Result<Collection<'a>> {
    let collection = if name.eq("default") {
        ss.get_default_collection().await.map_err(decode_error)?
    } else {
        let all = ss.get_all_collections().await.map_err(decode_error)?;
        let mut found = None;
        for c in all {
            if c.get_label().await.map_err(decode_error)?.eq(name) {
                found = Some(c);
                break;
            }
        }
        found.ok_or(ErrorCode::NoEntry)?
    };
    if collection.is_locked().await.map_err(decode_error)? {
        collection.unlock().await.map_err(decode_error)?;
    }
    Ok(collection)
}

/// Create a secret service collection labeled with the given name.
///
/// If a collection with that name already exists, it is returned.
///
/// The name `default` is specially interpreted to mean the default collection.
pub async fn create_collection<'a>(
    ss: &'a SecretService<'_>,
    name: &str,
) -> Result<Collection<'a>> {
    let collection = if name.eq("default") {
        ss.get_default_collection().await.map_err(decode_error)?
    } else {
        ss.create_collection(name, "").await.map_err(decode_error)?
    };
    Ok(collection)
}

/// Given an existing item, set its secret.
pub async fn set_item_secret(item: &Item<'_>, secret: &[u8]) -> Result<()> {
    item.set_secret(secret, "text/plain")
        .await
        .map_err(decode_error)
}

/// Given an existing item, retrieve and decode its password.
pub async fn get_item_password(item: &Item<'_>) -> Result<String> {
    let bytes = item.get_secret().await.map_err(decode_error)?;
    decode_password(bytes)
}

/// Given an existing item, retrieve its secret.
pub async fn get_item_secret(item: &Item<'_>) -> Result<Vec<u8>> {
    let secret = item.get_secret().await.map_err(decode_error)?;
    Ok(secret)
}

/// Given an existing item, retrieve its non-controlled attributes.
pub async fn get_item_attributes(item: &Item<'_>) -> Result<HashMap<String, String>> {
    let mut attributes = item.get_attributes().await.map_err(decode_error)?;
    attributes.remove("target");
    attributes.remove("service");
    attributes.remove("username");
    attributes.insert(
        "label".to_string(),
        item.get_label().await.map_err(decode_error)?,
    );
    Ok(attributes)
}

/// Given an existing item, retrieve its non-controlled attributes.
pub async fn update_item_attributes(
    item: &Item<'_>,
    attributes: &HashMap<&str, &str>,
) -> Result<()> {
    let existing = item.get_attributes().await.map_err(decode_error)?;
    let mut updated: HashMap<&str, &str> = HashMap::new();
    for (k, v) in &existing {
        updated.insert(k, v);
    }
    for (k, v) in attributes {
        if k.eq(&"target") || k.eq(&"service") || k.eq(&"username") {
            continue;
        }
        if k.eq(&"label") {
            if v.is_empty() {
                return Err(ErrorCode::Invalid(
                    "label".to_string(),
                    "cannot be empty".to_string(),
                ));
            }
            item.set_label(v).await.map_err(decode_error)?;
            if updated.contains_key("label") {
                updated.insert("label", v);
            }
        } else {
            updated.insert(k, v);
        }
    }
    item.set_attributes(updated).await.map_err(decode_error)?;
    Ok(())
}

// Given an existing item, delete it.
pub async fn delete_item(item: &Item<'_>) -> Result<()> {
    item.delete().await.map_err(decode_error)
}

//
// Error utilities
//

/// Map underlying secret-service errors to crate errors with
/// appropriate annotation.
pub fn decode_error(err: Error) -> ErrorCode {
    match err {
        Error::Locked => no_access(err),
        Error::NoResult => no_access(err),
        Error::Prompt => no_access(err),
        _ => platform_failure(err),
    }
}

fn empty_target() -> ErrorCode {
    ErrorCode::Invalid("target".to_string(), "cannot be empty".to_string())
}

fn platform_failure(err: Error) -> ErrorCode {
    ErrorCode::PlatformFailure(wrap(err))
}

fn no_access(err: Error) -> ErrorCode {
    ErrorCode::NoStorageAccess(wrap(err))
}

fn wrap(err: Error) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(err)
}

#[cfg(feature = "native-auth")]
#[cfg(test)]
mod tests {
    use crate::credential::CredentialPersistence;
    use crate::secret_service::{EncryptionType, SecretService, SsCredential};
    use crate::{Entry, Error, default_credential_builder, tests::generate_random_string};
    use std::collections::HashMap;

    #[test]
    fn test_persistence() {
        assert!(matches!(
            default_credential_builder().persistence(),
            CredentialPersistence::UntilDelete
        ));
    }

    fn entry_new(service: &str, user: &str) -> Entry {
        crate::tests::entry_from_constructor(SsCredential::new_with_target, service, user)
    }

    #[test]
    fn test_invalid_parameter() {
        let credential = SsCredential::new_with_target(Some(""), "service", "user");
        assert!(
            matches!(credential, Err(Error::Invalid(_, _))),
            "Created entry with empty target"
        );
    }

    #[tokio::test]
    async fn test_missing_entry() {
        crate::tests::test_missing_entry(entry_new).await;
    }

    #[tokio::test]
    async fn test_empty_password() {
        crate::tests::test_empty_password(entry_new).await;
    }

    #[tokio::test]
    async fn test_round_trip_ascii_password() {
        crate::tests::test_round_trip_ascii_password(entry_new).await;
    }

    #[tokio::test]
    async fn test_round_trip_non_ascii_password() {
        crate::tests::test_round_trip_non_ascii_password(entry_new).await;
    }

    #[tokio::test]
    async fn test_round_trip_random_secret() {
        crate::tests::test_round_trip_random_secret(entry_new).await;
    }

    #[tokio::test]
    async fn test_update() {
        crate::tests::test_update(entry_new).await;
    }

    #[tokio::test]
    async fn test_get_credential() {
        let name = generate_random_string();
        let entry = entry_new(&name, &name);
        entry
            .set_password("test get credential")
            .await
            .expect("Can't set password for get_credential");
        let credential: &SsCredential = entry
            .get_credential()
            .downcast_ref()
            .expect("Not a secret service credential");
        let actual = credential
            .new_from_matching_item()
            .await
            .expect("Can't read credential");
        assert_eq!(actual.label, credential.label, "Labels don't match");
        for (key, value) in &credential.attributes {
            assert_eq!(
                actual.attributes.get(key).expect("Missing attribute"),
                value,
                "Attribute mismatch"
            );
        }
        entry
            .delete_credential()
            .await
            .expect("Couldn't delete get-credential");
        assert!(matches!(entry.get_password().await, Err(Error::NoEntry)));
    }

    #[tokio::test]
    async fn test_get_update_attributes() {
        let name = generate_random_string();
        let credential = SsCredential::new_with_target(None, &name, &name)
            .expect("Can't create credential for attribute test");
        let create_label = credential.label.clone();
        let entry = Entry::new_with_credential(Box::new(credential));
        assert!(
            matches!(entry.get_attributes().await, Err(Error::NoEntry)),
            "Read missing credential in attribute test",
        );
        let mut in_map: HashMap<&str, &str> = HashMap::new();
        in_map.insert("label", "test label value");
        in_map.insert("test attribute name", "test attribute value");
        in_map.insert("target", "ignored target value");
        in_map.insert("service", "ignored service value");
        in_map.insert("username", "ignored username value");
        assert!(
            matches!(entry.update_attributes(&in_map).await, Err(Error::NoEntry)),
            "Updated missing credential in attribute test",
        );
        // create the credential and test again
        entry
            .set_password("test password for attributes")
            .await
            .unwrap_or_else(|err| panic!("Can't set password for attribute test: {err:?}"));
        let out_map = entry
            .get_attributes()
            .await
            .expect("Can't get attributes after create");
        assert_eq!(out_map["label"], create_label);
        assert_eq!(out_map["application"], "uv");
        assert!(!out_map.contains_key("target"));
        assert!(!out_map.contains_key("service"));
        assert!(!out_map.contains_key("username"));
        assert!(
            matches!(entry.update_attributes(&in_map).await, Ok(())),
            "Couldn't update attributes in attribute test",
        );
        let after_map = entry
            .get_attributes()
            .await
            .expect("Can't get attributes after update");
        assert_eq!(after_map["label"], in_map["label"]);
        assert_eq!(
            after_map["test attribute name"],
            in_map["test attribute name"]
        );
        assert_eq!(out_map["application"], "uv");
        in_map.insert("label", "");
        assert!(
            matches!(
                entry.update_attributes(&in_map).await,
                Err(Error::Invalid(_, _))
            ),
            "Was able to set empty label in attribute test",
        );
        entry
            .delete_credential()
            .await
            .unwrap_or_else(|err| panic!("Can't delete credential for attribute test: {err:?}"));
        assert!(
            matches!(entry.get_attributes().await, Err(Error::NoEntry)),
            "Read deleted credential in attribute test",
        );
    }

    #[tokio::test]
    #[ignore = "can't be run headless, because it needs to prompt"]
    async fn test_create_new_target_collection() {
        let name = generate_random_string();
        let credential = SsCredential::new_with_target(Some(&name), &name, &name)
            .expect("Can't create credential for new collection");
        let entry = Entry::new_with_credential(Box::new(credential));
        let password = "password in new collection";
        entry
            .set_password(password)
            .await
            .expect("Can't set password for new collection entry");
        let actual = entry
            .get_password()
            .await
            .expect("Can't get password for new collection entry");
        assert_eq!(actual, password);
        entry
            .delete_credential()
            .await
            .expect("Couldn't delete password for new collection entry");
        assert!(matches!(entry.get_password().await, Err(Error::NoEntry)));
        delete_collection(&name).await;
    }

    #[tokio::test]
    #[ignore = "can't be run headless, because it needs to prompt"]
    async fn test_separate_targets_dont_interfere() {
        let name1 = generate_random_string();
        let name2 = generate_random_string();
        let credential1 = SsCredential::new_with_target(Some(&name1), &name1, &name1)
            .expect("Can't create credential1 with new collection");
        let entry1 = Entry::new_with_credential(Box::new(credential1));
        let credential2 = SsCredential::new_with_target(Some(&name2), &name1, &name1)
            .expect("Can't create credential2 with new collection");
        let entry2 = Entry::new_with_credential(Box::new(credential2));
        let entry3 = Entry::new(&name1, &name1).expect("Can't create entry in default collection");
        let password1 = "password for collection 1";
        let password2 = "password for collection 2";
        let password3 = "password for default collection";
        entry1
            .set_password(password1)
            .await
            .expect("Can't set password for collection 1");
        entry2
            .set_password(password2)
            .await
            .expect("Can't set password for collection 2");
        entry3
            .set_password(password3)
            .await
            .expect("Can't set password for default collection");
        let actual1 = entry1
            .get_password()
            .await
            .expect("Can't get password for collection 1");
        assert_eq!(actual1, password1);
        let actual2 = entry2
            .get_password()
            .await
            .expect("Can't get password for collection 2");
        assert_eq!(actual2, password2);
        let actual3 = entry3
            .get_password()
            .await
            .expect("Can't get password for default collection");
        assert_eq!(actual3, password3);
        entry1
            .delete_credential()
            .await
            .expect("Couldn't delete password for collection 1");
        assert!(matches!(entry1.get_password().await, Err(Error::NoEntry)));
        entry2
            .delete_credential()
            .await
            .expect("Couldn't delete password for collection 2");
        assert!(matches!(entry2.get_password().await, Err(Error::NoEntry)));
        entry3
            .delete_credential()
            .await
            .expect("Couldn't delete password for default collection");
        assert!(matches!(entry3.get_password().await, Err(Error::NoEntry)));
        delete_collection(&name1).await;
        delete_collection(&name2).await;
    }

    #[tokio::test]
    async fn test_legacy_entry() {
        let name = generate_random_string();
        let pw = "test password";
        let v3_entry = Entry::new(&name, &name).expect("Can't create v3 entry");
        let _ = v3_entry.get_password().await.expect_err("Found v3 entry");
        create_v1_entry(&name, pw).await;
        let password = v3_entry.get_password().await.expect("Can't find v1 entry");
        assert_eq!(password, pw);
        v3_entry
            .delete_credential()
            .await
            .expect("Can't delete v1 entry");
        let _ = v3_entry
            .get_password()
            .await
            .expect_err("Got password for v1 entry after delete");
    }

    async fn delete_collection(name: &str) {
        let ss = SecretService::connect(EncryptionType::Dh)
            .await
            .expect("Can't connect to secret service");
        let collection = super::get_collection(&ss, name)
            .await
            .expect("Can't find collection to delete");
        collection.delete().await.expect("Can't delete collection");
    }

    async fn create_v1_entry(name: &str, password: &str) {
        use secret_service::{EncryptionType, SecretService};

        let cred = SsCredential::new_with_no_target(name, name)
            .expect("Can't create credential with no target");
        let ss = SecretService::connect(EncryptionType::Dh)
            .await
            .expect("Can't connect to secret service");
        let collection = ss
            .get_default_collection()
            .await
            .expect("Can't get default collection");
        collection
            .create_item(
                cred.label.as_str(),
                cred.all_attributes(),
                password.as_bytes(),
                true, // replace
                "text/plain",
            )
            .await
            .expect("Can't create item with no target in default collection");
    }
}
