/*!

# macOS Keychain credential store

All credentials on macOS are stored in secure stores called _keychains_.
The OS automatically creates three of them that live on filesystem,
called _User_ (aka login), _Common_, and _System_. In addition, removable
media can contain a keychain which can be registered under the name _Dynamic_.

The target attribute of an [`Entry`](crate::Entry) determines (case-insensitive)
which keychain that entry's credential is created in or searched for.
If the entry has no target, or the specified target doesn't name (case-insensitive)
one of the keychains listed above, the 'User' keychain is used.

For a given service/user pair, this module creates/searches for a credential
in the target keychain whose _account_ attribute holds the user
and whose _name_ attribute holds the service.
Because of a quirk in the Mac keychain services API, neither the _account_
nor the _name_ may be the empty string. (Empty strings are treated as
wildcards when looking up credentials by attribute value.)

In the _Keychain Access_ UI on Mac, credentials created by this module
show up in the passwords area (with their _where_ field equal to their _name_).
What the Keychain Access lists under _Note_ entries on the Mac are
also generic credentials, so existing _notes_ created by third-party
applications can be accessed by this module if you know the value
of their _account_ attribute (which is not displayed by _Keychain Access_).

Credentials on macOS can have a large number of _key/value_ attributes,
but this module controls the _account_ and _name_ attributes and
ignores all the others. so clients can't use it to access or update any attributes.
 */
use crate::credential::{Credential, CredentialApi, CredentialBuilder, CredentialBuilderApi};
use crate::error::{Error as ErrorCode, Result, decode_password};
use security_framework::base::Error;
use security_framework::os::macos::keychain::{SecKeychain, SecPreferencesDomain};
use security_framework::os::macos::passwords::find_generic_password;

/// The representation of a generic Keychain credential.
///
/// The actual credentials can have lots of attributes
/// not represented here.  There's no way to use this
/// module to get at those attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacCredential {
    pub domain: MacKeychainDomain,
    pub service: String,
    pub account: String,
}

#[async_trait::async_trait]
impl CredentialApi for MacCredential {
    /// Create and write a credential with password for this entry.
    ///
    /// The new credential replaces any existing one in the store.
    /// Since there is only one credential with a given _account_ and _user_
    /// in any given keychain, there is no chance of ambiguity.
    async fn set_password(&self, password: &str) -> Result<()> {
        let service = self.service.clone();
        let account = self.account.clone();
        let domain = self.domain;
        let password = password.to_string();
        crate::blocking::spawn_blocking(move || {
            get_keychain(domain)?
                .set_generic_password(&service, &account, password.as_bytes())
                .map_err(decode_error)
        })
        .await?;
        Ok(())
    }

    /// Create and write a credential with secret for this entry.
    ///
    /// The new credential replaces any existing one in the store.
    /// Since there is only one credential with a given _account_ and _user_
    /// in any given keychain, there is no chance of ambiguity.
    async fn set_secret(&self, secret: &[u8]) -> Result<()> {
        let service = self.service.clone();
        let account = self.account.clone();
        let domain = self.domain;
        let secret = secret.to_vec();
        crate::blocking::spawn_blocking(move || {
            get_keychain(domain)?
                .set_generic_password(&service, &account, &secret)
                .map_err(decode_error)
        })
        .await?;
        Ok(())
    }

    /// Look up the password for this entry, if any.
    ///
    /// Returns a [`NoEntry`](ErrorCode::NoEntry) error if there is no
    /// credential in the store.
    async fn get_password(&self) -> Result<String> {
        let service = self.service.clone();
        let account = self.account.clone();
        let domain = self.domain;

        let password_bytes = crate::blocking::spawn_blocking(move || -> Result<Vec<u8>> {
            let keychain = get_keychain(domain)?;
            let (password_bytes, _) = find_generic_password(Some(&[keychain]), &service, &account)
                .map_err(decode_error)?;
            Ok(password_bytes.to_owned())
        })
        .await?;

        decode_password(password_bytes)
    }

    /// Look up the secret for this entry, if any.
    ///
    /// Returns a [`NoEntry`](ErrorCode::NoEntry) error if there is no
    /// credential in the store.
    async fn get_secret(&self) -> Result<Vec<u8>> {
        let service = self.service.clone();
        let account = self.account.clone();
        let domain = self.domain;

        let password_bytes = crate::blocking::spawn_blocking(move || -> Result<Vec<u8>> {
            let keychain = get_keychain(domain)?;
            let (password_bytes, _) = find_generic_password(Some(&[keychain]), &service, &account)
                .map_err(decode_error)?;
            Ok(password_bytes.to_owned())
        })
        .await?;

        Ok(password_bytes)
    }

    /// Delete the underlying generic credential for this entry, if any.
    ///
    /// Returns a [`NoEntry`](ErrorCode::NoEntry) error if there is no
    /// credential in the store.
    async fn delete_credential(&self) -> Result<()> {
        let service = self.service.clone();
        let account = self.account.clone();
        let domain = self.domain;

        crate::blocking::spawn_blocking(move || {
            let keychain = get_keychain(domain)?;
            let (_, item) = find_generic_password(Some(&[keychain]), &service, &account)
                .map_err(decode_error)?;
            item.delete();
            Ok(())
        })
        .await?;
        Ok(())
    }

    /// Return the underlying concrete object with an `Any` type so that it can
    /// be downgraded to a [`MacCredential`] for platform-specific processing.
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// Expose the concrete debug formatter for use via the [Credential] trait
    fn debug_fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl MacCredential {
    /// Construct a credential from the underlying generic credential.
    ///
    /// On Mac, this is basically a no-op, because we represent any attributes
    /// other than the ones we use to find the generic credential.
    /// But at least this checks whether the underlying credential exists.
    pub async fn get_credential(&self) -> Result<Self> {
        let service = self.service.clone();
        let account = self.account.clone();
        let domain = self.domain;
        let keychain = get_keychain(domain)?;
        crate::blocking::spawn_blocking(move || -> Result<()> {
            let (_, _) = find_generic_password(Some(&[keychain]), &service, &account)
                .map_err(decode_error)?;
            Ok(())
        })
        .await?;
        Ok(self.clone())
    }

    /// Create a credential representing a Mac keychain entry.
    ///
    /// Creating a credential does not put anything into the keychain.
    /// The keychain entry will be created
    /// when [`set_password`](MacCredential::set_password) is
    /// called.
    ///
    /// This will fail if the service or user strings are empty,
    /// because empty attribute values act as wildcards in the
    /// Keychain Services API.
    pub fn new_with_target(
        target: Option<MacKeychainDomain>,
        service: &str,
        user: &str,
    ) -> Result<Self> {
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
        let domain = if let Some(target) = target {
            target
        } else {
            MacKeychainDomain::User
        };
        Ok(Self {
            domain,
            service: service.to_string(),
            account: user.to_string(),
        })
    }
}

/// The builder for Mac keychain credentials
pub struct MacCredentialBuilder;

/// Returns an instance of the Mac credential builder.
///
/// On Mac, with default features enabled,
/// this is called once when an entry is first created.
pub fn default_credential_builder() -> Box<CredentialBuilder> {
    Box::new(MacCredentialBuilder {})
}

impl CredentialBuilderApi for MacCredentialBuilder {
    /// Build a [`MacCredential`] for the given target, service, and user.
    ///
    /// If a target is specified but not recognized as a keychain name,
    /// the User keychain is selected.
    fn build(&self, target: Option<&str>, service: &str, user: &str) -> Result<Box<Credential>> {
        let domain: MacKeychainDomain = if let Some(target) = target {
            target.parse().unwrap_or(MacKeychainDomain::User)
        } else {
            MacKeychainDomain::User
        };
        Ok(Box::new(MacCredential::new_with_target(
            Some(domain),
            service,
            user,
        )?))
    }

    /// Return the underlying builder object with an `Any` type so that it can
    /// be downgraded to a [`MacCredentialBuilder`] for platform-specific processing.
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
/// The four pre-defined Mac keychains.
pub enum MacKeychainDomain {
    User,
    System,
    Common,
    Dynamic,
    Protected,
}

impl std::fmt::Display for MacKeychainDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => "User".fmt(f),
            Self::System => "System".fmt(f),
            Self::Common => "Common".fmt(f),
            Self::Dynamic => "Dynamic".fmt(f),
            Self::Protected => "Protected".fmt(f),
        }
    }
}

impl std::str::FromStr for MacKeychainDomain {
    type Err = ErrorCode;

    /// Convert a target specification string to a keychain domain.
    ///
    /// We accept any case in the string,
    /// but the value has to match a known keychain domain name
    /// or else we assume the login keychain is meant.
    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "user" => Ok(Self::User),
            "system" => Ok(Self::System),
            "common" => Ok(Self::Common),
            "dynamic" => Ok(Self::Dynamic),
            "protected" => Ok(Self::Protected),
            "data protection" => Ok(Self::Protected),
            _ => Err(ErrorCode::Invalid(
                "target".to_string(),
                format!("'{s}' is not User, System, Common, Dynamic, or Protected"),
            )),
        }
    }
}

fn get_keychain(domain: MacKeychainDomain) -> Result<SecKeychain> {
    let domain = match domain {
        MacKeychainDomain::User => SecPreferencesDomain::User,
        MacKeychainDomain::System => SecPreferencesDomain::System,
        MacKeychainDomain::Common => SecPreferencesDomain::Common,
        MacKeychainDomain::Dynamic => SecPreferencesDomain::Dynamic,
        MacKeychainDomain::Protected => panic!("Protected is not a keychain domain on macOS"),
    };
    match SecKeychain::default_for_domain(domain) {
        Ok(keychain) => Ok(keychain),
        Err(err) => Err(decode_error(err)),
    }
}

/// Map a Mac API error to a crate error with appropriate annotation
///
/// The macOS error code values used here are from
/// [this reference](https://opensource.apple.com/source/libsecurity_keychain/libsecurity_keychain-78/lib/SecBase.h.auto.html)
pub fn decode_error(err: Error) -> ErrorCode {
    match err.code() {
        -25291 => ErrorCode::NoStorageAccess(Box::new(err)), // errSecNotAvailable
        -25292 => ErrorCode::NoStorageAccess(Box::new(err)), // errSecReadOnly
        -25294 => ErrorCode::NoStorageAccess(Box::new(err)), // errSecNoSuchKeychain
        -25295 => ErrorCode::NoStorageAccess(Box::new(err)), // errSecInvalidKeychain
        -25300 => ErrorCode::NoEntry,                        // errSecItemNotFound
        _ => ErrorCode::PlatformFailure(Box::new(err)),
    }
}

#[cfg(feature = "native-auth")]
#[cfg(not(miri))]
#[cfg(test)]
mod tests {
    use crate::credential::CredentialPersistence;
    use crate::{Entry, Error, tests::generate_random_string};

    use super::{MacCredential, default_credential_builder};

    #[test]
    fn test_persistence() {
        assert!(matches!(
            default_credential_builder().persistence(),
            CredentialPersistence::UntilDelete
        ));
    }

    fn entry_new(service: &str, user: &str) -> Entry {
        crate::tests::entry_from_constructor(
            |_, s, u| MacCredential::new_with_target(None, s, u),
            service,
            user,
        )
    }

    #[test]
    fn test_invalid_parameter() {
        let credential = MacCredential::new_with_target(None, "", "user");
        assert!(
            matches!(credential, Err(Error::Invalid(_, _))),
            "Created credential with empty service"
        );
        let credential = MacCredential::new_with_target(None, "service", "");
        assert!(
            matches!(credential, Err(Error::Invalid(_, _))),
            "Created entry with empty user"
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
        let credential: &MacCredential = entry
            .get_credential()
            .downcast_ref()
            .expect("Not a mac credential");
        assert!(
            credential.get_credential().await.is_err(),
            "Platform credential shouldn't exist yet!"
        );
        entry
            .set_password("test get_credential")
            .await
            .expect("Can't set password for get_credential");
        assert!(credential.get_credential().await.is_ok());
        entry
            .delete_credential()
            .await
            .expect("Couldn't delete after get_credential");
        assert!(matches!(entry.get_password().await, Err(Error::NoEntry)));
    }

    #[tokio::test]
    async fn test_get_update_attributes() {
        crate::tests::test_noop_get_update_attributes(entry_new).await;
    }

    #[test]
    fn test_select_keychain() {
        for name in ["unknown", "user", "common", "system", "dynamic"] {
            let cred = Entry::new_with_target(name, name, name)
                .expect("couldn't create credential")
                .inner;
            let mac_cred: &MacCredential = cred
                .as_any()
                .downcast_ref()
                .expect("credential not a MacCredential");
            if name == "unknown" {
                assert!(
                    matches!(mac_cred.domain, super::MacKeychainDomain::User),
                    "wrong domain for unknown specifier"
                );
            }
        }
    }
}
