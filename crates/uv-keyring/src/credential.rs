/*!

# Platform-independent secure storage model

This module defines a plug and play model for platform-specific credential stores.
The model comprises two traits: [`CredentialBuilderApi`] for the underlying store
and [`CredentialApi`] for the entries in the store.  These traits must be implemented
in a thread-safe way, a requirement captured in the [`CredentialBuilder`] and
[`Credential`] types that wrap them.
 */
use std::any::Any;
use std::collections::HashMap;

use crate::Result;

/// The API that [credentials](Credential) implement.
#[async_trait::async_trait]
pub trait CredentialApi {
    /// Set the credential's password (a string).
    ///
    /// This will persist the password in the underlying store.
    async fn set_password(&self, password: &str) -> Result<()> {
        self.set_secret(password.as_bytes()).await
    }

    /// Set the credential's secret (a byte array).
    ///
    /// This will persist the secret in the underlying store.
    async fn set_secret(&self, password: &[u8]) -> Result<()>;

    /// Retrieve the password (a string) from the underlying credential.
    ///
    /// This has no effect on the underlying store. If there is no credential
    /// for this entry, a [`NoEntry`](crate::Error::NoEntry) error is returned.
    async fn get_password(&self) -> Result<String> {
        let secret = self.get_secret().await?;
        crate::error::decode_password(secret)
    }

    /// Retrieve a secret (a byte array) from the credential.
    ///
    /// This has no effect on the underlying store. If there is no credential
    /// for this entry, a [NoEntry](crate::Error::NoEntry) error is returned.
    async fn get_secret(&self) -> Result<Vec<u8>>;

    /// Get the secure store attributes on this entry's credential.
    ///
    /// Each credential store may support reading and updating different
    /// named attributes; see the documentation on each of the stores
    /// for details. Note that the keyring itself uses some of these
    /// attributes to map entries to their underlying credential; these
    /// _controlled_ attributes are not available for reading or updating.
    ///
    /// We provide a default (no-op) implementation of this method
    /// for backward compatibility with stores that don't implement it.
    async fn get_attributes(&self) -> Result<HashMap<String, String>> {
        // this should err in the same cases as get_secret, so first call that for effect
        self.get_secret().await?;
        // if we got this far, return success with no attributes
        Ok(HashMap::new())
    }

    /// Update the secure store attributes on this entry's credential.
    ///
    /// Each credential store may support reading and updating different
    /// named attributes; see the documentation on each of the stores
    /// for details. The implementation will ignore any attribute names
    /// that you supply that are not available for update. Because the
    /// names used by the different stores tend to be distinct, you can
    /// write cross-platform code that will work correctly on each platform.
    ///
    /// We provide a default no-op implementation of this method
    /// for backward compatibility with stores that don't implement it.
    async fn update_attributes(&self, _: &HashMap<&str, &str>) -> Result<()> {
        // this should err in the same cases as get_secret, so first call that for effect
        self.get_secret().await?;
        // if we got this far, return success after setting no attributes
        Ok(())
    }

    /// Delete the underlying credential, if there is one.
    ///
    /// This is not idempotent if the credential existed!
    /// A second call to `delete_credential` will return
    /// a [`NoEntry`](crate::Error::NoEntry) error.
    async fn delete_credential(&self) -> Result<()>;

    /// Return the underlying concrete object cast to [Any].
    ///
    /// This allows clients
    /// to downcast the credential to its concrete type so they
    /// can do platform-specific things with it (e.g.,
    /// query its attributes in the underlying store).
    fn as_any(&self) -> &dyn Any;

    /// The `Debug` trait call for the object.
    ///
    /// This is used to implement the `Debug` trait on this type; it
    /// allows generic code to provide debug printing as provided by
    /// the underlying concrete object.
    ///
    /// We provide a (useless) default implementation for backward
    /// compatibility with existing implementors who may have not
    /// implemented the `Debug` trait for their credential objects
    fn debug_fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_any(), f)
    }
}

/// A thread-safe implementation of the [Credential API](CredentialApi).
pub type Credential = dyn CredentialApi + Send + Sync;

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.debug_fmt(f)
    }
}

/// A descriptor for the lifetime of stored credentials, returned from
/// a credential store's [`persistence`](CredentialBuilderApi::persistence) call.
#[non_exhaustive]
pub enum CredentialPersistence {
    /// Credentials vanish when the entry vanishes (stored in the entry)
    EntryOnly,
    /// Credentials vanish when the process terminates (stored in process memory)
    ProcessOnly,
    /// Credentials persist until the machine reboots (stored in kernel memory)
    UntilReboot,
    /// Credentials persist until they are explicitly deleted (stored on disk)
    UntilDelete,
}

/// The API that [credential builders](CredentialBuilder) implement.
pub trait CredentialBuilderApi {
    /// Create a credential identified by the given target, service, and user.
    ///
    /// This typically has no effect on the content of the underlying store.
    /// A credential need not be persisted until its password is set.
    fn build(&self, target: Option<&str>, service: &str, user: &str) -> Result<Box<Credential>>;

    /// Return the underlying concrete object cast to [Any].
    ///
    /// Because credential builders need not have any internal structure,
    /// this call is not so much for clients
    /// as it is to allow automatic derivation of a Debug trait for builders.
    fn as_any(&self) -> &dyn Any;

    /// The lifetime of credentials produced by this builder.
    ///
    /// A default implementation is provided for backward compatibility,
    /// since this API was added in a minor release.  The default assumes
    /// that keystores use disk-based credential storage.
    fn persistence(&self) -> CredentialPersistence {
        CredentialPersistence::UntilDelete
    }
}

impl std::fmt::Debug for CredentialBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_any().fmt(f)
    }
}

/// A thread-safe implementation of the [`CredentialBuilder` API](CredentialBuilderApi).
pub type CredentialBuilder = dyn CredentialBuilderApi + Send + Sync;

struct NopCredentialBuilder;

impl CredentialBuilderApi for NopCredentialBuilder {
    fn build(&self, _: Option<&str>, _: &str, _: &str) -> Result<Box<Credential>> {
        Err(super::Error::NoDefaultCredentialBuilder)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn persistence(&self) -> CredentialPersistence {
        CredentialPersistence::EntryOnly
    }
}

// Return a credential builder that always fails. This is the builder
// used if none of the crate-supplied keystores were included in the build.
pub fn nop_credential_builder() -> Box<CredentialBuilder> {
    Box::new(NopCredentialBuilder)
}
