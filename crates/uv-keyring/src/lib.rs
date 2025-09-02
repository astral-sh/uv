#![cfg_attr(docsrs, feature(doc_cfg))]
/*!

# Keyring

This is a cross-platform library that does storage and retrieval of passwords
(or other secrets) in an underlying platform-specific secure credential store.
A top-level introduction to the library's usage, as well as a small code sample,
may be found in [the library's entry on crates.io](https://crates.io/crates/keyring).
Currently supported platforms are
Linux,
FreeBSD,
OpenBSD,
Windows,
and macOS.

## Design

This crate implements a very simple, platform-independent concrete object called an _entry_.
Each entry is identified by a <_service name_, _user name_> pair of UTF-8 strings.
Entries support setting, getting, and forgetting (aka deleting) passwords (UTF-8 strings)
and binary secrets (byte arrays). Each created entry provides security and persistence
of its secret by wrapping a credential held in a platform-specific, secure credential store.

The cross-platform API for creating an _entry_ supports specifying an (optional)
UTF-8 _target_ attribute on entries, but the meaning of this
attribute is credential-store (and thus platform) specific,
and should not be thought of as part of the credential's identification. See the
documentation of each credential store to understand the
effect of specifying the _target_ attribute on entries in that store,
as well as which values are allowed for _target_ by that store.

The abstract behavior of entries and credential stores are captured
by two types (with associated traits):

- a _credential builder_, represented by the [`CredentialBuilder`] type
  (and [`CredentialBuilderApi`](credential::CredentialBuilderApi) trait).  Credential
  builders are given the identifying information (and target, if any)
  provided for an entry and map
  it to the identifying information for a platform-specific credential.
- a _credential_, represented by the [`Credential`] type
  (and [`CredentialApi`](credential::CredentialApi) trait).  The platform-specific credential
  identified by the builder for an entry is what provides the secure storage
  for that entry's password/secret.

## Crate-provided Credential Stores

This crate runs on several different platforms, and on each one
it provides (by default) an implementation of a default credential store used
on that platform (see [`default_credential_builder`]).
These implementations work by mapping the data used to identify an entry
to data used to identify platform-specific storage objects.
For example, on macOS, the service and user provided for an entry
are mapped to the service and user attributes that identify a
generic credential in the macOS keychain.

Typically, platform-specific credential stores (called _keystores_ in this crate)
have a richer model of a credential than
the one used by this crate to identify entries.
These keystores expose their specific model in the
concrete credential objects they use to implement the Credential trait.
In order to allow clients to access this richer model, the Credential trait
has an [`as_any`](credential::CredentialApi::as_any) method that returns a
reference to the underlying
concrete object typed as [`Any`](std::any::Any), so that it can be downgraded to
its concrete type.

### Credential store features

Each of the platform-specific credential stores is associated a feature.
This feature controls whether that store is included when the crate is built
for its specific platform.  For example, the macOS Keychain credential store
implementation is only included if the `"apple-native"` feature is specified and the crate
is built with a macOS target.

The available credential store features, listed here, are all included in the
default feature set:

- `apple-native`: Provides access to the Keychain credential store on macOS.

- `windows-native`: Provides access to the Windows Credential Store on Windows.

- `secret-service`: Provides access to Secret Service.

If you suppress the default feature set when building this crate, and you
don't separately specify one of the included keystore features for your platform,
then no keystore will be built in, and calls to [`Entry::new`] and [`Entry::new_with_target`]
will fail unless the client brings their own keystore (see next section).

## Client-provided Credential Stores

In addition to the keystores implemented by this crate, clients
are free to provide their own keystores and use those.  There are
two mechanisms provided for this:

- Clients can give their desired credential builder to the crate
  for use by the [`Entry::new`] and [`Entry::new_with_target`] calls.
  This is done by making a call to [`set_default_credential_builder`].
  The major advantage of this approach is that client code remains
  independent of the credential builder being used.

- Clients can construct their concrete credentials directly and
  then turn them into entries by using the [`Entry::new_with_credential`]
  call. The major advantage of this approach is that credentials
  can be identified however clients want, rather than being restricted
  to the simple model used by this crate.

## Mock Credential Store

In addition to the platform-specific credential stores, this crate
always provides a mock credential store that clients can use to
test their code in a platform independent way.  The mock credential
store allows for pre-setting errors as well as password values to
be returned from [`Entry`] method calls. If you want to use the mock
credential store as your default in tests, make this call:
```
uv_keyring::set_default_credential_builder(uv_keyring::mock::default_credential_builder())
```

## Interoperability with Third Parties

Each of the platform-specific credential stores provided by this crate uses
an underlying store that may also be used by modules written
in other languages.  If you want to interoperate with these third party
credential writers, then you will need to understand the details of how the
target, service, and user of this crate's generic model
are used to identify credentials in the platform-specific store.
These details are in the implementation of this crate's keystores,
and are documented in the headers of those modules.

(_N.B._ Since the included credential store implementations are platform-specific,
you may need to use the Platform drop-down on [docs.rs](https://docs.rs/keyring) to
view the storage module documentation for your desired platform.)

## Caveats

This module expects passwords to be UTF-8 encoded strings,
so if a third party has stored an arbitrary byte string
then retrieving that as a password will return a
[`BadEncoding`](Error::BadEncoding) error.
The returned error will have the raw bytes attached,
so you can access them, but you can also just fetch
them directly using [`get_secret`](Entry::get_secret) rather than
[`get_password`](Entry::get_password).

While this crate's code is thread-safe, the underlying credential
stores may not handle access from different threads reliably.
In particular, accessing the same credential
from multiple threads at the same time can fail, especially on
Windows and Linux, because the accesses may not be serialized in the same order
they are made. And for RPC-based credential stores such as the dbus-based Secret
Service, accesses from multiple threads (and even the same thread very quickly)
are not recommended, as they may cause the RPC mechanism to fail.
 */

use std::collections::HashMap;

pub use credential::{Credential, CredentialBuilder};
pub use error::{Error, Result};

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod blocking;
pub mod mock;

//
// pick the *nix keystore
//
#[cfg(all(
    any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"),
    feature = "secret-service"
))]
#[cfg_attr(
    docsrs,
    doc(cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")))
)]
pub mod secret_service;

//
// pick the Apple keystore
//
#[cfg(all(target_os = "macos", feature = "apple-native"))]
#[cfg_attr(docsrs, doc(cfg(target_os = "macos")))]
pub mod macos;

//
// pick the Windows keystore
//
#[cfg(all(target_os = "windows", feature = "windows-native"))]
#[cfg_attr(docsrs, doc(cfg(target_os = "windows")))]
pub mod windows;

pub mod credential;
pub mod error;

#[derive(Default, Debug)]
struct EntryBuilder {
    inner: Option<Box<CredentialBuilder>>,
}

static DEFAULT_BUILDER: std::sync::RwLock<EntryBuilder> =
    std::sync::RwLock::new(EntryBuilder { inner: None });

/// Set the credential builder used by default to create entries.
///
/// This is really meant for use by clients who bring their own credential
/// store and want to use it everywhere.  If you are using multiple credential
/// stores and want precise control over which credential is in which store,
/// then use [`new_with_credential`](Entry::new_with_credential).
///
/// This will block waiting for all other threads currently creating entries
/// to complete what they are doing. It's really meant to be called
/// at app startup before you start creating entries.
pub fn set_default_credential_builder(new: Box<CredentialBuilder>) {
    let mut guard = DEFAULT_BUILDER
        .write()
        .expect("Poisoned RwLock in keyring-rs: please report a bug!");
    guard.inner = Some(new);
}

pub fn default_credential_builder() -> Box<CredentialBuilder> {
    #[cfg(any(
        all(target_os = "linux", feature = "secret-service"),
        all(target_os = "freebsd", feature = "secret-service"),
        all(target_os = "openbsd", feature = "secret-service")
    ))]
    return secret_service::default_credential_builder();
    #[cfg(all(target_os = "macos", feature = "apple-native"))]
    return macos::default_credential_builder();
    #[cfg(all(target_os = "windows", feature = "windows-native"))]
    return windows::default_credential_builder();
    #[cfg(not(any(
        all(target_os = "linux", feature = "secret-service"),
        all(target_os = "freebsd", feature = "secret-service"),
        all(target_os = "openbsd", feature = "secret-service"),
        all(target_os = "macos", feature = "apple-native"),
        all(target_os = "windows", feature = "windows-native"),
    )))]
    credential::nop_credential_builder()
}

fn build_default_credential(target: Option<&str>, service: &str, user: &str) -> Result<Entry> {
    static DEFAULT: std::sync::LazyLock<Box<CredentialBuilder>> =
        std::sync::LazyLock::new(default_credential_builder);
    let guard = DEFAULT_BUILDER
        .read()
        .expect("Poisoned RwLock in keyring-rs: please report a bug!");
    let builder = guard.inner.as_ref().unwrap_or_else(|| &DEFAULT);
    let credential = builder.build(target, service, user)?;
    Ok(Entry { inner: credential })
}

#[derive(Debug)]
pub struct Entry {
    inner: Box<Credential>,
}

impl Entry {
    /// Create an entry for the given service and user.
    ///
    /// The default credential builder is used.
    ///
    /// # Errors
    ///
    /// This function will return an [`Error`] if the `service` or `user` values are invalid.
    /// The specific reasons for invalidity are platform-dependent, but include length constraints.
    ///
    /// # Panics
    ///
    /// In the very unlikely event that the internal credential builder's `RwLock` is poisoned, this function
    /// will panic. If you encounter this, and especially if you can reproduce it, please report a bug with the
    /// details (and preferably a backtrace) so the developers can investigate.
    pub fn new(service: &str, user: &str) -> Result<Self> {
        let entry = build_default_credential(None, service, user)?;
        Ok(entry)
    }

    /// Create an entry for the given target, service, and user.
    ///
    /// The default credential builder is used.
    pub fn new_with_target(target: &str, service: &str, user: &str) -> Result<Self> {
        let entry = build_default_credential(Some(target), service, user)?;
        Ok(entry)
    }

    /// Create an entry from a credential that may be in any credential store.
    pub fn new_with_credential(credential: Box<Credential>) -> Self {
        Self { inner: credential }
    }

    /// Set the password for this entry.
    ///
    /// Can return an [`Ambiguous`](Error::Ambiguous) error
    /// if there is more than one platform credential
    /// that matches this entry.  This can only happen
    /// on some platforms, and then only if a third-party
    /// application wrote the ambiguous credential.
    pub async fn set_password(&self, password: &str) -> Result<()> {
        self.inner.set_password(password).await
    }

    /// Set the secret for this entry.
    ///
    /// Can return an [`Ambiguous`](Error::Ambiguous) error
    /// if there is more than one platform credential
    /// that matches this entry.  This can only happen
    /// on some platforms, and then only if a third-party
    /// application wrote the ambiguous credential.
    pub async fn set_secret(&self, secret: &[u8]) -> Result<()> {
        self.inner.set_secret(secret).await
    }

    /// Retrieve the password saved for this entry.
    ///
    /// Returns a [`NoEntry`](Error::NoEntry) error if there isn't one.
    ///
    /// Can return an [`Ambiguous`](Error::Ambiguous) error
    /// if there is more than one platform credential
    /// that matches this entry.  This can only happen
    /// on some platforms, and then only if a third-party
    /// application wrote the ambiguous credential.
    pub async fn get_password(&self) -> Result<String> {
        self.inner.get_password().await
    }

    /// Retrieve the secret saved for this entry.
    ///
    /// Returns a [`NoEntry`](Error::NoEntry) error if there isn't one.
    ///
    /// Can return an [`Ambiguous`](Error::Ambiguous) error
    /// if there is more than one platform credential
    /// that matches this entry.  This can only happen
    /// on some platforms, and then only if a third-party
    /// application wrote the ambiguous credential.
    pub async fn get_secret(&self) -> Result<Vec<u8>> {
        self.inner.get_secret().await
    }

    /// Get the attributes on the underlying credential for this entry.
    ///
    /// Some of the underlying credential stores allow credentials to have named attributes
    /// that can be set to string values. See the documentation for each credential store
    /// for a list of which attribute names are supported by that store.
    ///
    /// Returns a [`NoEntry`](Error::NoEntry) error if there isn't a credential for this entry.
    ///
    /// Can return an [`Ambiguous`](Error::Ambiguous) error
    /// if there is more than one platform credential
    /// that matches this entry.  This can only happen
    /// on some platforms, and then only if a third-party
    /// application wrote the ambiguous credential.
    pub async fn get_attributes(&self) -> Result<HashMap<String, String>> {
        self.inner.get_attributes().await
    }

    /// Update the attributes on the underlying credential for this entry.
    ///
    /// Some of the underlying credential stores allow credentials to have named attributes
    /// that can be set to string values. See the documentation for each credential store
    /// for a list of which attribute names can be given values by this call. To support
    /// cross-platform use, each credential store ignores (without error) any specified attributes
    /// that aren't supported by that store.
    ///
    /// Returns a [`NoEntry`](Error::NoEntry) error if there isn't a credential for this entry.
    ///
    /// Can return an [`Ambiguous`](Error::Ambiguous) error
    /// if there is more than one platform credential
    /// that matches this entry.  This can only happen
    /// on some platforms, and then only if a third-party
    /// application wrote the ambiguous credential.
    pub async fn update_attributes(&self, attributes: &HashMap<&str, &str>) -> Result<()> {
        self.inner.update_attributes(attributes).await
    }

    /// Delete the underlying credential for this entry.
    ///
    /// Returns a [`NoEntry`](Error::NoEntry) error if there isn't one.
    ///
    /// Can return an [`Ambiguous`](Error::Ambiguous) error
    /// if there is more than one platform credential
    /// that matches this entry.  This can only happen
    /// on some platforms, and then only if a third-party
    /// application wrote the ambiguous credential.
    ///
    /// Note: This does _not_ affect the lifetime of the [Entry]
    /// structure, which is controlled by Rust.  It only
    /// affects the underlying credential store.
    pub async fn delete_credential(&self) -> Result<()> {
        self.inner.delete_credential().await
    }

    /// Return a reference to this entry's wrapped credential.
    ///
    /// The reference is of the [Any](std::any::Any) type, so it can be
    /// downgraded to a concrete credential object.  The client must know
    /// what type of concrete object to cast to.
    pub fn get_credential(&self) -> &dyn std::any::Any {
        self.inner.as_any()
    }
}

#[cfg(doctest)]
doc_comment::doctest!("../README.md", readme);

#[cfg(test)]
/// There are no actual tests in this module.
/// Instead, it contains generics that each keystore invokes in their tests,
/// passing their store-specific parameters for the generic ones.
mod tests {
    use super::{Entry, Error};
    #[cfg(feature = "native-auth")]
    use super::{Result, credential::CredentialApi};
    use std::collections::HashMap;

    /// Create a platform-specific credential given the constructor, service, and user
    #[cfg(feature = "native-auth")]
    pub(crate) fn entry_from_constructor<F, T>(f: F, service: &str, user: &str) -> Entry
    where
        F: FnOnce(Option<&str>, &str, &str) -> Result<T>,
        T: 'static + CredentialApi + Send + Sync,
    {
        match f(None, service, user) {
            Ok(credential) => Entry::new_with_credential(Box::new(credential)),
            Err(err) => {
                panic!("Couldn't create entry (service: {service}, user: {user}): {err:?}")
            }
        }
    }

    async fn test_round_trip_no_delete(case: &str, entry: &Entry, in_pass: &str) {
        entry
            .set_password(in_pass)
            .await
            .unwrap_or_else(|err| panic!("Can't set password for {case}: {err:?}"));
        let out_pass = entry
            .get_password()
            .await
            .unwrap_or_else(|err| panic!("Can't get password for {case}: {err:?}"));
        assert_eq!(
            in_pass, out_pass,
            "Passwords don't match for {case}: set='{in_pass}', get='{out_pass}'",
        );
    }

    /// A basic round-trip unit test given an entry and a password.
    pub(crate) async fn test_round_trip(case: &str, entry: &Entry, in_pass: &str) {
        test_round_trip_no_delete(case, entry, in_pass).await;
        entry
            .delete_credential()
            .await
            .unwrap_or_else(|err| panic!("Can't delete password for {case}: {err:?}"));
        let password = entry.get_password().await;
        assert!(
            matches!(password, Err(Error::NoEntry)),
            "Read deleted password for {case}",
        );
    }

    /// A basic round-trip unit test given an entry and a password.
    pub(crate) async fn test_round_trip_secret(case: &str, entry: &Entry, in_secret: &[u8]) {
        entry
            .set_secret(in_secret)
            .await
            .unwrap_or_else(|err| panic!("Can't set secret for {case}: {err:?}"));
        let out_secret = entry
            .get_secret()
            .await
            .unwrap_or_else(|err| panic!("Can't get secret for {case}: {err:?}"));
        assert_eq!(
            in_secret, &out_secret,
            "Passwords don't match for {case}: set='{in_secret:?}', get='{out_secret:?}'",
        );
        entry
            .delete_credential()
            .await
            .unwrap_or_else(|err| panic!("Can't delete password for {case}: {err:?}"));
        let password = entry.get_secret().await;
        assert!(
            matches!(password, Err(Error::NoEntry)),
            "Read deleted password for {case}",
        );
    }

    /// When tests fail, they leave keys behind, and those keys
    /// have to be cleaned up before the tests can be run again
    /// in order to avoid bad results.  So it's a lot easier just
    /// to have tests use a random string for key names to avoid
    /// the conflicts, and then do any needed cleanup once everything
    /// is working correctly.  So we export this function for tests to use.
    pub(crate) fn generate_random_string_of_len(len: usize) -> String {
        use fastrand;
        use std::iter::repeat_with;
        repeat_with(fastrand::alphanumeric).take(len).collect()
    }

    pub(crate) fn generate_random_string() -> String {
        generate_random_string_of_len(30)
    }

    fn generate_random_bytes_of_len(len: usize) -> Vec<u8> {
        use fastrand;
        use std::iter::repeat_with;
        repeat_with(|| fastrand::u8(..)).take(len).collect()
    }

    pub(crate) async fn test_missing_entry<F>(f: F)
    where
        F: FnOnce(&str, &str) -> Entry,
    {
        let name = generate_random_string();
        let entry = f(&name, &name);
        assert!(
            matches!(entry.get_password().await, Err(Error::NoEntry)),
            "Missing entry has password"
        );
    }

    pub(crate) async fn test_empty_password<F>(f: F)
    where
        F: FnOnce(&str, &str) -> Entry,
    {
        let name = generate_random_string();
        let entry = f(&name, &name);
        test_round_trip("empty password", &entry, "").await;
    }

    pub(crate) async fn test_round_trip_ascii_password<F>(f: F)
    where
        F: FnOnce(&str, &str) -> Entry,
    {
        let name = generate_random_string();
        let entry = f(&name, &name);
        test_round_trip("ascii password", &entry, "test ascii password").await;
    }

    pub(crate) async fn test_round_trip_non_ascii_password<F>(f: F)
    where
        F: FnOnce(&str, &str) -> Entry,
    {
        let name = generate_random_string();
        let entry = f(&name, &name);
        test_round_trip("non-ascii password", &entry, "このきれいな花は桜です").await;
    }

    pub(crate) async fn test_round_trip_random_secret<F>(f: F)
    where
        F: FnOnce(&str, &str) -> Entry,
    {
        let name = generate_random_string();
        let entry = f(&name, &name);
        let secret = generate_random_bytes_of_len(24);
        test_round_trip_secret("non-ascii password", &entry, secret.as_slice()).await;
    }

    pub(crate) async fn test_update<F>(f: F)
    where
        F: FnOnce(&str, &str) -> Entry,
    {
        let name = generate_random_string();
        let entry = f(&name, &name);
        test_round_trip_no_delete("initial ascii password", &entry, "test ascii password").await;
        test_round_trip(
            "updated non-ascii password",
            &entry,
            "このきれいな花は桜です",
        )
        .await;
    }

    pub(crate) async fn test_noop_get_update_attributes<F>(f: F)
    where
        F: FnOnce(&str, &str) -> Entry,
    {
        let name = generate_random_string();
        let entry = f(&name, &name);
        assert!(
            matches!(entry.get_attributes().await, Err(Error::NoEntry)),
            "Read missing credential in attribute test",
        );
        let mut map: HashMap<&str, &str> = HashMap::new();
        map.insert("test attribute name", "test attribute value");
        assert!(
            matches!(entry.update_attributes(&map).await, Err(Error::NoEntry)),
            "Updated missing credential in attribute test",
        );
        // create the credential and test again
        entry
            .set_password("test password for attributes")
            .await
            .unwrap_or_else(|err| panic!("Can't set password for attribute test: {err:?}"));
        match entry.get_attributes().await {
            Err(err) => panic!("Couldn't get attributes: {err:?}"),
            Ok(attrs) if attrs.is_empty() => {}
            Ok(attrs) => panic!("Unexpected attributes: {attrs:?}"),
        }
        assert!(
            matches!(entry.update_attributes(&map).await, Ok(())),
            "Couldn't update attributes in attribute test",
        );
        match entry.get_attributes().await {
            Err(err) => panic!("Couldn't get attributes after update: {err:?}"),
            Ok(attrs) if attrs.is_empty() => {}
            Ok(attrs) => panic!("Unexpected attributes after update: {attrs:?}"),
        }
        entry
            .delete_credential()
            .await
            .unwrap_or_else(|err| panic!("Can't delete credential for attribute test: {err:?}"));
        assert!(
            matches!(entry.get_attributes().await, Err(Error::NoEntry)),
            "Read deleted credential in attribute test",
        );
    }
}
