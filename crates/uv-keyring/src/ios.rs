/*!

# iOS Keychain credential store

All credentials on iOS are stored in secure stores called _keychains_.
On iOS there is only one of these, and it has no name.  The target
attribute of an [Entry](crate::Entry), for consistency with macOS,
determines which keychain an entry's credential is created in
searched for.  On iOS, then, entries must have no target or use
the specially named target `default`.

For a given service/user pair, this module creates/searches for a credential
in the target keychain whose _account_ attribute holds the user
and whose _name_ attribute holds the service.
Because of a quirk in the iOS keychain services API, neither the _account_
nor the _name_ may be the empty string. (Empty strings are treated as
wildcards when looking up credentials by attribute value.)

Credentials on iOS can have a large number of _key/value_ attributes,
but this module controls the _account_ and _name_ attributes and
ignores all the others. so clients can't use it to access or update any attributes.
 */

use security_framework::base::Error;
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

use crate::credential::{Credential, CredentialApi, CredentialBuilder, CredentialBuilderApi};
use crate::error::{Error as ErrorCode, Result, decode_password};

/// The representation of a generic Keychain credential.
///
/// The actual credentials can have lots of attributes
/// not represented here.  There's no way to use this
/// module to get at those attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IosCredential {
    pub service: String,
    pub account: String,
}

impl CredentialApi for IosCredential {
    /// Create and write a credential with password for this entry.
    ///
    /// The new credential replaces any existing one in the store.
    /// Since there is only one credential with a given _account_ and _user_
    /// in any given keychain, there is no chance of ambiguity.
    fn set_password(&self, password: &str) -> Result<()> {
        self.set_secret(password.as_bytes())?;
        Ok(())
    }

    /// Create and write a credential with secret for this entry.
    ///
    /// The new credential replaces any existing one in the store.
    /// Since there is only one credential with a given _account_ and _user_
    /// in any given keychain, there is no chance of ambiguity.
    fn set_secret(&self, secret: &[u8]) -> Result<()> {
        set_generic_password(&self.service, &self.account, secret).map_err(decode_error)?;
        Ok(())
    }

    /// Look up the password for this entry, if any.
    ///
    /// Returns a [NoEntry](ErrorCode::NoEntry) error if there is no
    /// credential in the store.
    fn get_password(&self) -> Result<String> {
        let password_bytes = self.get_secret()?;
        decode_password(password_bytes)
    }

    /// Look up the secret for this entry, if any.
    ///
    /// Returns a [NoEntry](ErrorCode::NoEntry) error if there is no
    /// credential in the store.
    fn get_secret(&self) -> Result<Vec<u8>> {
        get_generic_password(&self.service, &self.account).map_err(decode_error)
    }

    /// Delete the underlying generic credential for this entry, if any.
    ///
    /// Returns a [NoEntry](ErrorCode::NoEntry) error if there is no
    /// credential in the store.
    fn delete_credential(&self) -> Result<()> {
        delete_generic_password(&self.service, &self.account).map_err(decode_error)?;
        Ok(())
    }

    /// Return the underlying concrete object with an `Any` type so that it can
    /// be downgraded to an [IosCredential] for platform-specific processing.
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// Expose the concrete debug formatter for use via the [Credential] trait
    fn debug_fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl IosCredential {
    /// Construct a credential from the underlying generic credential.
    ///
    /// On iOS, this is basically a no-op, because we represent any attributes
    /// other than the ones we use to find the generic credential.
    /// But at least this checks whether the underlying credential exists.
    pub fn get_credential(&self) -> Result<Self> {
        get_generic_password(&self.service, &self.account).map_err(decode_error)?;
        Ok(self.clone())
    }

    /// Create a credential representing an iOS keychain entry.
    ///
    /// The target string is ignored, because there's only one keychain.
    ///
    /// Creating a credential does not put anything into the keychain.
    /// The keychain entry will be created
    /// when [set_password](IosCredential::set_password) is
    /// called.
    ///
    /// This will fail if the service or user strings are empty,
    /// because empty attribute values act as wildcards in the
    /// Keychain Services API.
    pub fn new_with_target(target: Option<&str>, service: &str, user: &str) -> Result<Self> {
        if service.is_empty() {
            return Err(ErrorCode::Invalid(
                "service".to_string(),
                "cannot be empty".to_string(),
            ));
        }
        if user.is_empty() {
            return Err(ErrorCode::Invalid(
                "user".to_string(),
                "cannot be empty".to_string(),
            ));
        }
        if let Some(target) = target {
            if !target.eq_ignore_ascii_case("default") {
                return Err(ErrorCode::Invalid(
                    "target".to_string(),
                    "only 'default' is allowed".to_string(),
                ));
            }
        }
        Ok(Self {
            service: service.to_string(),
            account: user.to_string(),
        })
    }
}

/// The builder for iOS keychain credentials
pub struct IosCredentialBuilder {}

/// Returns an instance of the iOS credential builder.
///
/// On iOS,
/// this is called once when an entry is first created.
pub fn default_credential_builder() -> Box<CredentialBuilder> {
    Box::new(IosCredentialBuilder {})
}

impl CredentialBuilderApi for IosCredentialBuilder {
    /// Build an [IosCredential] for the given target, service, and user.
    fn build(&self, target: Option<&str>, service: &str, user: &str) -> Result<Box<Credential>> {
        Ok(Box::new(IosCredential::new_with_target(
            target, service, user,
        )?))
    }

    /// Return the underlying builder object with an `Any` type so that it can
    /// be downgraded to an [IosCredentialBuilder] for platform-specific processing.
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Map an iOS API error to a crate error with appropriate annotation
///
/// The iOS error code values used here are from
/// [this reference](https://opensource.apple.com/source/libsecurity_keychain/libsecurity_keychain-78/lib/SecBase.h.auto.html)
fn decode_error(err: Error) -> ErrorCode {
    match err.code() {
        -25291 => ErrorCode::NoStorageAccess(Box::new(err)), // errSecNotAvailable
        -25292 => ErrorCode::NoStorageAccess(Box::new(err)), // errSecReadOnly
        -25300 => ErrorCode::NoEntry,                        // errSecItemNotFound
        _ => ErrorCode::PlatformFailure(Box::new(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::{IosCredential, default_credential_builder};
    use crate::credential::CredentialPersistence;
    use crate::{Entry, Error, tests::generate_random_string};

    #[test]
    fn test_persistence() {
        assert!(matches!(
            default_credential_builder().persistence(),
            CredentialPersistence::UntilDelete
        ))
    }

    fn entry_new(service: &str, user: &str) -> Entry {
        crate::tests::entry_from_constructor(IosCredential::new_with_target, service, user)
    }

    #[test]
    fn test_invalid_parameter() {
        let credential = IosCredential::new_with_target(None, "", "user");
        assert!(
            matches!(credential, Err(Error::Invalid(_, _))),
            "Created credential with empty service"
        );
        let credential = IosCredential::new_with_target(None, "service", "");
        assert!(
            matches!(credential, Err(Error::Invalid(_, _))),
            "Created entry with empty user"
        );
        let credential = IosCredential::new_with_target(Some(""), "service", "user");
        assert!(
            matches!(credential, Err(Error::Invalid(_, _))),
            "Created entry with empty target"
        );
    }

    #[test]
    fn test_missing_entry() {
        crate::tests::test_missing_entry(entry_new);
    }

    #[test]
    fn test_empty_password() {
        crate::tests::test_empty_password(entry_new);
    }

    #[test]
    fn test_round_trip_ascii_password() {
        crate::tests::test_round_trip_ascii_password(entry_new);
    }

    #[test]
    fn test_round_trip_non_ascii_password() {
        crate::tests::test_round_trip_non_ascii_password(entry_new);
    }

    #[test]
    fn test_round_trip_random_secret() {
        crate::tests::test_round_trip_random_secret(entry_new);
    }

    #[test]
    fn test_update() {
        crate::tests::test_update(entry_new);
    }

    #[test]
    fn test_get_credential() {
        let name = generate_random_string();
        let entry = entry_new(&name, &name);
        let credential: &IosCredential = entry
            .get_credential()
            .downcast_ref()
            .expect("Not a mac credential");
        assert!(
            credential.get_credential().is_err(),
            "Platform credential shouldn't exist yet!"
        );
        entry
            .set_password("test get_credential")
            .expect("Can't set password for get_credential");
        assert!(credential.get_credential().is_ok());
        entry
            .delete_credential()
            .expect("Couldn't delete after get_credential");
        assert!(matches!(entry.get_password(), Err(Error::NoEntry)));
    }

    #[test]
    fn test_get_update_attributes() {
        crate::tests::test_noop_get_update_attributes(entry_new);
    }
}
