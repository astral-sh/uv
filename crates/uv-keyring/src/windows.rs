/*!

# Windows Credential Manager credential store

This module uses Windows Generic credentials to store entries.
These are identified by a single string (called their _target name_).
They also have a number of non-identifying but manipulable attributes:
a _username_, a _comment_, and a _target alias_.

For a given <_service_, _username_> pair,
this module uses the concatenated string `username.service`
as the mapped credential's _target name_, and
fills the _username_ and _comment_ fields with appropriate strings.
(This convention allows multiple users to store passwords for the same service.)

Because the Windows credential manager doesn't support multiple collections of credentials,
and because many Windows programs use _only_ the service name as the credential _target name_,
the `Entry::new_with_target` call uses the `target` parameter as the credential's _target name_
rather than concatenating the username and service.
So if you have a custom algorithm you want to use for computing the Windows target name,
you can specify the target name directly.  (You still need to provide a service and username,
because they are used in the credential's metadata.)

The [`get_attributes`](crate::Entry::get_attributes)
call will return the values in the `username`, `comment`, and `target_alias` fields
(using those strings as the attribute names),
and the [`update_attributes`](crate::Entry::update_attributes)
call allows setting those fields.

## Caveat

Reads and writes of the same entry from multiple threads
are not guaranteed to be serialized by the Windows Credential Manager in
the order in which they were made.  Careful testing has
shown that modifying the same entry in the same (almost simultaneous) order from
different threads produces different results on different runs.
*/

#![allow(unsafe_code)]

use crate::credential::{Credential, CredentialApi, CredentialBuilder, CredentialBuilderApi};
use crate::error::{Error as ErrorCode, Result};
use byteorder::{ByteOrder, LittleEndian};
use std::collections::HashMap;
use std::iter::once;
use std::str;
use windows::Win32::Foundation::{
    ERROR_BAD_USERNAME, ERROR_INVALID_FLAGS, ERROR_INVALID_PARAMETER, ERROR_NO_SUCH_LOGON_SESSION,
    ERROR_NOT_FOUND, FILETIME, WIN32_ERROR,
};
use windows::Win32::Security::Credentials::{
    CRED_FLAGS, CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_MAX_GENERIC_TARGET_NAME_LENGTH,
    CRED_MAX_STRING_LENGTH, CRED_MAX_USERNAME_LENGTH, CRED_PERSIST_ENTERPRISE, CRED_TYPE_GENERIC,
    CREDENTIAL_ATTRIBUTEW, CREDENTIALW, CredDeleteW, CredFree, CredReadW, CredWriteW,
};
use windows::core::PWSTR;
use zeroize::Zeroize;

/// The representation of a Windows Generic credential.
///
/// See the module header for the meanings of these fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinCredential {
    pub username: String,
    pub target_name: String,
    pub target_alias: String,
    pub comment: String,
}

// Windows API type mappings:
// DWORD is u32
// LPCWSTR is *const u16
// BOOL is i32 (false = 0, true = 1)
// PCREDENTIALW = *mut CREDENTIALW

#[async_trait::async_trait]
impl CredentialApi for WinCredential {
    /// Create and write a credential with password for this entry.
    ///
    /// The new credential replaces any existing one in the store.
    /// Since there is only one credential with a given _target name_,
    /// there is no chance of ambiguity.
    async fn set_password(&self, password: &str) -> Result<()> {
        self.validate_attributes(None, Some(password))?;
        // Password strings are converted to UTF-16, because that's the native
        // charset for Windows strings.  This allows interoperability with native
        // Windows credential APIs.  But the storage for the credential is actually
        // a little-endian blob, because Windows credentials can contain anything.
        let mut blob_u16 = to_wstr_no_null(password);
        let mut blob = vec![0; blob_u16.len() * 2];
        LittleEndian::write_u16_into(&blob_u16, &mut blob);
        let result = self.set_secret(&blob).await;
        // make sure that the copies of the secret are erased
        blob_u16.zeroize();
        blob.zeroize();
        result
    }

    /// Create and write a credential with secret for this entry.
    ///
    /// The new credential replaces any existing one in the store.
    /// Since there is only one credential with a given _target name_,
    /// there is no chance of ambiguity.
    async fn set_secret(&self, secret: &[u8]) -> Result<()> {
        self.validate_attributes(Some(secret), None)?;
        self.save_credential(secret).await
    }

    /// Look up the password for this entry, if any.
    ///
    /// Returns a [`NoEntry`](ErrorCode::NoEntry) error if there is no
    /// credential in the store.
    async fn get_password(&self) -> Result<String> {
        self.extract_from_platform(extract_password).await
    }

    /// Look up the secret for this entry, if any.
    ///
    /// Returns a [`NoEntry`](ErrorCode::NoEntry) error if there is no
    /// credential in the store.
    async fn get_secret(&self) -> Result<Vec<u8>> {
        self.extract_from_platform(extract_secret).await
    }

    /// Get the attributes from the credential for this entry, if it exists.
    ///
    /// Returns a [`NoEntry`](ErrorCode::NoEntry) error if there is no
    /// credential in the store.
    async fn get_attributes(&self) -> Result<HashMap<String, String>> {
        let cred = self.extract_from_platform(Self::extract_credential).await?;
        let mut attributes: HashMap<String, String> = HashMap::new();
        attributes.insert("comment".to_string(), cred.comment.clone());
        attributes.insert("target_alias".to_string(), cred.target_alias.clone());
        attributes.insert("username".to_string(), cred.username.clone());
        Ok(attributes)
    }

    /// Update the attributes on the credential for this entry, if it exists.
    ///
    /// Returns a [`NoEntry`](ErrorCode::NoEntry) error if there is no
    /// credential in the store.
    async fn update_attributes(&self, attributes: &HashMap<&str, &str>) -> Result<()> {
        let secret = self.extract_from_platform(extract_secret).await?;
        let mut cred = self.extract_from_platform(Self::extract_credential).await?;
        if let Some(comment) = attributes.get(&"comment") {
            cred.comment = (*comment).to_string();
        }
        if let Some(target_alias) = attributes.get(&"target_alias") {
            cred.target_alias = (*target_alias).to_string();
        }
        if let Some(username) = attributes.get(&"username") {
            cred.username = (*username).to_string();
        }
        cred.validate_attributes(Some(&secret), None)?;
        cred.save_credential(&secret).await
    }

    /// Delete the underlying generic credential for this entry, if any.
    ///
    /// Returns a [`NoEntry`](ErrorCode::NoEntry) error if there is no
    /// credential in the store.
    async fn delete_credential(&self) -> Result<()> {
        self.validate_attributes(None, None)?;
        let mut target_name = to_wstr(&self.target_name);
        let cred_type = CRED_TYPE_GENERIC;
        crate::blocking::spawn_blocking(move || {
            // SAFETY: Calling Windows API
            unsafe {
                CredDeleteW(PWSTR(target_name.as_mut_ptr()), cred_type, None)
                    .map_err(|err| Error(err).into())
            }
        })
        .await
    }

    /// Return the underlying concrete object with an `Any` type so that it can
    /// be downgraded to a [`WinCredential`] for platform-specific processing.
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// Expose the concrete debug formatter for use via the [`Credential`] trait
    fn debug_fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl WinCredential {
    fn validate_attributes(&self, secret: Option<&[u8]>, password: Option<&str>) -> Result<()> {
        if self.username.len() > CRED_MAX_USERNAME_LENGTH as usize {
            return Err(ErrorCode::TooLong(
                String::from("user"),
                CRED_MAX_USERNAME_LENGTH,
            ));
        }
        if self.target_name.is_empty() {
            return Err(ErrorCode::Invalid(
                "target".to_string(),
                "cannot be empty".to_string(),
            ));
        }
        if self.target_name.len() > CRED_MAX_GENERIC_TARGET_NAME_LENGTH as usize {
            return Err(ErrorCode::TooLong(
                String::from("target"),
                CRED_MAX_GENERIC_TARGET_NAME_LENGTH,
            ));
        }
        if self.target_alias.len() > CRED_MAX_STRING_LENGTH as usize {
            return Err(ErrorCode::TooLong(
                String::from("target alias"),
                CRED_MAX_STRING_LENGTH,
            ));
        }
        if self.comment.len() > CRED_MAX_STRING_LENGTH as usize {
            return Err(ErrorCode::TooLong(
                String::from("comment"),
                CRED_MAX_STRING_LENGTH,
            ));
        }
        if let Some(secret) = secret {
            if secret.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
                return Err(ErrorCode::TooLong(
                    String::from("secret"),
                    CRED_MAX_CREDENTIAL_BLOB_SIZE,
                ));
            }
        }
        if let Some(password) = password {
            // We're going to store the password as UTF-16, so first transform it to UTF-16,
            // count its runes, and then multiply by 2 to get the number of bytes needed.
            if password.encode_utf16().count() * 2 > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
                return Err(ErrorCode::TooLong(
                    String::from("password encoded as UTF-16"),
                    CRED_MAX_CREDENTIAL_BLOB_SIZE,
                ));
            }
        }
        Ok(())
    }

    /// Write this credential into the underlying store as a Generic credential
    ///
    /// You must always have validated attributes before you call this!
    #[allow(clippy::cast_possible_truncation)]
    async fn save_credential(&self, secret: &[u8]) -> Result<()> {
        let mut username = to_wstr(&self.username);
        let mut target_name = to_wstr(&self.target_name);
        let mut target_alias = to_wstr(&self.target_alias);
        let mut comment = to_wstr(&self.comment);
        let mut blob = secret.to_vec();
        let blob_len = blob.len() as u32;
        crate::blocking::spawn_blocking(move || {
            let flags = CRED_FLAGS::default();
            let cred_type = CRED_TYPE_GENERIC;
            let persist = CRED_PERSIST_ENTERPRISE;
            // Ignored by CredWriteW
            let last_written = FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            };
            let attribute_count = 0;
            let attributes: *mut CREDENTIAL_ATTRIBUTEW = std::ptr::null_mut();
            let credential = CREDENTIALW {
                Flags: flags,
                Type: cred_type,
                TargetName: PWSTR(target_name.as_mut_ptr()),
                Comment: PWSTR(comment.as_mut_ptr()),
                LastWritten: last_written,
                CredentialBlobSize: blob_len,
                CredentialBlob: blob.as_mut_ptr(),
                Persist: persist,
                AttributeCount: attribute_count,
                Attributes: attributes,
                TargetAlias: PWSTR(target_alias.as_mut_ptr()),
                UserName: PWSTR(username.as_mut_ptr()),
            };
            // SAFETY: Calling Windows API
            let result =
                unsafe { CredWriteW(&raw const credential, 0) }.map_err(|err| Error(err).into());
            // erase the copy of the secret
            blob.zeroize();
            result
        })
        .await
    }

    /// Construct a credential from this credential's underlying Generic credential.
    ///
    /// This can be useful for seeing modifications made by a third party.
    pub async fn get_credential(&self) -> Result<Self> {
        self.extract_from_platform(Self::extract_credential).await
    }

    async fn extract_from_platform<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&CREDENTIALW) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        self.validate_attributes(None, None)?;
        let mut target_name = to_wstr(&self.target_name);
        crate::blocking::spawn_blocking(move || {
            let mut p_credential = std::ptr::null_mut();
            // at this point, p_credential is just a pointer to nowhere.
            // The allocation happens in the `CredReadW` call below.
            let cred_type = CRED_TYPE_GENERIC;
            // SAFETY: Calling windows API
            unsafe {
                CredReadW(
                    PWSTR(target_name.as_mut_ptr()),
                    cred_type,
                    None,
                    &raw mut p_credential,
                )
            }
            .map_err(Error)?;
            // SAFETY: `CredReadW` succeeded, so p_credential points at an allocated credential. Apply
            // the passed extractor function to it.
            let ref_cred: &mut CREDENTIALW = unsafe { &mut *p_credential };
            let result = f(ref_cred);
            // Finally, we erase the secret and free the allocated credential.
            erase_secret(ref_cred);
            let p_credential = p_credential;
            // SAFETY: `CredReadW` succeeded, so p_credential points at an allocated credential.
            // Free the allocation.
            unsafe { CredFree(p_credential.cast()) }
            result
        })
        .await
    }

    #[allow(clippy::unnecessary_wraps)]
    fn extract_credential(w_credential: &CREDENTIALW) -> Result<Self> {
        Ok(Self {
            username: unsafe { from_wstr(w_credential.UserName.as_ptr()) },
            target_name: unsafe { from_wstr(w_credential.TargetName.as_ptr()) },
            target_alias: unsafe { from_wstr(w_credential.TargetAlias.as_ptr()) },
            comment: unsafe { from_wstr(w_credential.Comment.as_ptr()) },
        })
    }

    /// Create a credential for the given target, service, and user.
    ///
    /// Creating a credential does not create a matching Generic credential
    /// in the Windows Credential Manager.
    /// If there isn't already one there, it will be created only
    /// when [`set_password`](WinCredential::set_password) is
    /// called.
    pub fn new_with_target(target: Option<&str>, service: &str, user: &str) -> Result<Self> {
        const VERSION: &str = env!("CARGO_PKG_VERSION");
        let credential = if let Some(target) = target {
            Self {
                // On Windows, the target name is all that's used to
                // search for the credential, so we allow clients to
                // specify it if they want a different convention.
                username: user.to_string(),
                target_name: target.to_string(),
                target_alias: String::new(),
                comment: format!("{user}@{service}:{target} (keyring v{VERSION})"),
            }
        } else {
            Self {
                // Note: default concatenation of user and service name is
                // used because windows uses target_name as sole identifier.
                // See the module docs for more rationale.  Also see this issue
                // for Python: https://github.com/jaraco/keyring/issues/47
                //
                // Note that it's OK to have an empty user or service name,
                // because the format for the target name will not be empty.
                // But it's certainly not recommended.
                username: user.to_string(),
                target_name: format!("{user}.{service}"),
                target_alias: String::new(),
                comment: format!("{user}@{service}:{user}.{service} (keyring v{VERSION})"),
            }
        };
        credential.validate_attributes(None, None)?;
        Ok(credential)
    }
}

/// The builder for Windows Generic credentials.
pub struct WinCredentialBuilder;

/// Returns an instance of the Windows credential builder.
///
/// On Windows, with the default feature set,
/// this is called once when an entry is first created.
pub fn default_credential_builder() -> Box<CredentialBuilder> {
    Box::new(WinCredentialBuilder {})
}

impl CredentialBuilderApi for WinCredentialBuilder {
    /// Build a [`WinCredential`] for the given target, service, and user.
    fn build(&self, target: Option<&str>, service: &str, user: &str) -> Result<Box<Credential>> {
        Ok(Box::new(WinCredential::new_with_target(
            target, service, user,
        )?))
    }

    /// Return the underlying builder object with an `Any` type so that it can
    /// be downgraded to a [`WinCredentialBuilder`] for platform-specific processing.
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn extract_password(credential: &CREDENTIALW) -> Result<String> {
    let mut blob = extract_secret(credential)?;
    // 3rd parties may write credential data with an odd number of bytes,
    // so we make sure that we don't try to decode those as utf16
    if blob.len() % 2 != 0 {
        return Err(ErrorCode::BadEncoding(blob));
    }
    // This should be a UTF-16 string, so convert it to
    // a UTF-16 vector and then try to decode it.
    let mut blob_u16 = vec![0; blob.len() / 2];
    LittleEndian::read_u16_into(&blob, &mut blob_u16);
    let result = match String::from_utf16(&blob_u16) {
        Err(_) => Err(ErrorCode::BadEncoding(blob)),
        Ok(s) => {
            // we aren't returning the blob, so clear it
            blob.zeroize();
            Ok(s)
        }
    };
    // we aren't returning the utf16 blob, so clear it
    blob_u16.zeroize();
    result
}

#[allow(clippy::unnecessary_wraps)]
fn extract_secret(credential: &CREDENTIALW) -> Result<Vec<u8>> {
    let blob_pointer: *const u8 = credential.CredentialBlob;
    let blob_len: usize = credential.CredentialBlobSize as usize;
    if blob_len == 0 {
        return Ok(Vec::new());
    }
    let blob = unsafe { std::slice::from_raw_parts(blob_pointer, blob_len) };
    Ok(blob.to_vec())
}

fn erase_secret(credential: &mut CREDENTIALW) {
    let blob_pointer: *mut u8 = credential.CredentialBlob;
    let blob_len: usize = credential.CredentialBlobSize as usize;
    if blob_len == 0 {
        return;
    }
    let blob = unsafe { std::slice::from_raw_parts_mut(blob_pointer, blob_len) };
    blob.zeroize();
}

fn to_wstr(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(once(0)).collect()
}

fn to_wstr_no_null(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

#[allow(clippy::maybe_infinite_iter)]
unsafe fn from_wstr(ws: *const u16) -> String {
    // null pointer case, return empty string
    if ws.is_null() {
        return String::new();
    }
    // this code from https://stackoverflow.com/a/48587463/558006
    let len = (0..).take_while(|&i| unsafe { *ws.offset(i) != 0 }).count();
    if len == 0 {
        return String::new();
    }
    let slice = unsafe { std::slice::from_raw_parts(ws, len) };
    String::from_utf16_lossy(slice)
}

/// Windows error codes are `DWORDS` which are 32-bit unsigned ints.
#[derive(Debug)]
pub struct Error(windows::core::Error);

impl From<WIN32_ERROR> for Error {
    fn from(error: WIN32_ERROR) -> Self {
        Self(windows::core::Error::from(error))
    }
}

impl From<Error> for ErrorCode {
    fn from(err: Error) -> Self {
        if err.0 == ERROR_NOT_FOUND.into() {
            Self::NoEntry
        } else if err.0 == ERROR_NO_SUCH_LOGON_SESSION.into() {
            Self::NoStorageAccess(Box::new(err))
        } else {
            Self::PlatformFailure(Box::new(err))
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.0 == ERROR_NO_SUCH_LOGON_SESSION.into() {
            write!(f, "Windows ERROR_NO_SUCH_LOGON_SESSION")
        } else if self.0 == ERROR_NOT_FOUND.into() {
            write!(f, "Windows ERROR_NOT_FOUND")
        } else if self.0 == ERROR_BAD_USERNAME.into() {
            write!(f, "Windows ERROR_BAD_USERNAME")
        } else if self.0 == ERROR_INVALID_FLAGS.into() {
            write!(f, "Windows ERROR_INVALID_FLAGS")
        } else if self.0 == ERROR_INVALID_PARAMETER.into() {
            write!(f, "Windows ERROR_INVALID_PARAMETER")
        } else {
            write!(f, "Windows error code {}", self.0)
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

#[cfg(feature = "native-auth")]
#[cfg(test)]
mod tests {
    use super::*;

    use crate::Entry;
    use crate::credential::CredentialPersistence;
    use crate::tests::{generate_random_string, generate_random_string_of_len};

    #[test]
    fn test_persistence() {
        assert!(matches!(
            default_credential_builder().persistence(),
            CredentialPersistence::UntilDelete
        ));
    }

    fn entry_new(service: &str, user: &str) -> Entry {
        crate::tests::entry_from_constructor(WinCredential::new_with_target, service, user)
    }

    #[allow(clippy::cast_possible_truncation)]
    #[test]
    fn test_bad_password() {
        fn make_platform_credential(password: &mut Vec<u8>) -> CREDENTIALW {
            let last_written = FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            };
            let attribute_count = 0;
            let attributes: *mut CREDENTIAL_ATTRIBUTEW = std::ptr::null_mut();
            CREDENTIALW {
                Flags: CRED_FLAGS(0),
                Type: CRED_TYPE_GENERIC,
                TargetName: PWSTR::null(),
                Comment: PWSTR::null(),
                LastWritten: last_written,
                CredentialBlobSize: password.len() as u32,
                CredentialBlob: password.as_mut_ptr(),
                Persist: CRED_PERSIST_ENTERPRISE,
                AttributeCount: attribute_count,
                Attributes: attributes,
                TargetAlias: PWSTR::null(),
                UserName: PWSTR::null(),
            }
        }
        // the first malformed sequence can't be UTF-16 because it has an odd number of bytes.
        // the second malformed sequence has a first surrogate marker (0xd800) without a matching
        // companion (it's taken from the String::fromUTF16 docs).
        let mut odd_bytes = b"1".to_vec();
        let malformed_utf16 = [0xD834, 0xDD1E, 0x006d, 0x0075, 0xD800, 0x0069, 0x0063];
        let mut malformed_bytes: Vec<u8> = vec![0; malformed_utf16.len() * 2];
        LittleEndian::write_u16_into(&malformed_utf16, &mut malformed_bytes);
        for bytes in [&mut odd_bytes, &mut malformed_bytes] {
            let credential = make_platform_credential(bytes);
            match extract_password(&credential) {
                Err(ErrorCode::BadEncoding(str)) => assert_eq!(&str, bytes),
                Err(other) => panic!("Bad password ({bytes:?}) decode gave wrong error: {other}"),
                Ok(s) => panic!("Bad password ({bytes:?}) decode gave results: {s:?}"),
            }
        }
    }

    #[test]
    fn test_validate_attributes() {
        fn validate_attribute_too_long(result: Result<()>, attr: &str, len: u32) {
            match result {
                Err(ErrorCode::TooLong(arg, val)) => {
                    if attr == "password" {
                        assert_eq!(
                            &arg, "password encoded as UTF-16",
                            "Error names wrong attribute"
                        );
                    } else {
                        assert_eq!(&arg, attr, "Error names wrong attribute");
                    }
                    assert_eq!(val, len, "Error names wrong limit");
                }
                Err(other) => panic!("Error is not '{attr} too long': {other}"),
                Ok(()) => panic!("No error when {attr} too long"),
            }
        }
        let cred = WinCredential {
            username: "username".to_string(),
            target_name: "target_name".to_string(),
            target_alias: "target_alias".to_string(),
            comment: "comment".to_string(),
        };
        for (attr, len) in [
            ("user", CRED_MAX_USERNAME_LENGTH),
            ("target", CRED_MAX_GENERIC_TARGET_NAME_LENGTH),
            ("target alias", CRED_MAX_STRING_LENGTH),
            ("comment", CRED_MAX_STRING_LENGTH),
            ("password", CRED_MAX_CREDENTIAL_BLOB_SIZE),
            ("secret", CRED_MAX_CREDENTIAL_BLOB_SIZE),
        ] {
            let long_string = generate_random_string_of_len(1 + len as usize);
            let mut bad_cred = cred.clone();
            match attr {
                "user" => bad_cred.username = long_string.clone(),
                "target" => bad_cred.target_name = long_string.clone(),
                "target alias" => bad_cred.target_alias = long_string.clone(),
                "comment" => bad_cred.comment = long_string.clone(),
                _ => (),
            }
            let validate = |r| validate_attribute_too_long(r, attr, len);
            match attr {
                "password" => {
                    let password = generate_random_string_of_len((len / 2) as usize + 1);
                    validate(bad_cred.validate_attributes(None, Some(&password)));
                }
                "secret" => {
                    let secret: Vec<u8> = vec![255u8; len as usize + 1];
                    validate(bad_cred.validate_attributes(Some(&secret), None));
                }
                _ => validate(bad_cred.validate_attributes(None, None)),
            }
        }
    }

    #[test]
    fn test_password_valid_only_after_conversion_to_utf16() {
        let cred = WinCredential {
            username: "username".to_string(),
            target_name: "target_name".to_string(),
            target_alias: "target_alias".to_string(),
            comment: "comment".to_string(),
        };

        let len = CRED_MAX_CREDENTIAL_BLOB_SIZE / 2;
        let password: String = (0..len).map(|_| "笑").collect();

        assert!(password.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize);
        cred.validate_attributes(None, Some(&password))
            .expect("Password of appropriate length in UTF16 was invalid");
    }

    #[test]
    fn test_invalid_parameter() {
        let credential = WinCredential::new_with_target(Some(""), "service", "user");
        assert!(
            matches!(credential, Err(ErrorCode::Invalid(_, _))),
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
    async fn test_get_update_attributes() {
        let name = generate_random_string();
        let cred = WinCredential::new_with_target(None, &name, &name)
            .expect("Can't create credential for attribute test");
        let entry = Entry::new_with_credential(Box::new(cred.clone()));
        assert!(
            matches!(entry.get_attributes().await, Err(ErrorCode::NoEntry)),
            "Read missing credential in attribute test",
        );
        let mut in_map: HashMap<&str, &str> = HashMap::new();
        in_map.insert("label", "ignored label value");
        in_map.insert("attribute name", "ignored attribute value");
        in_map.insert("target_alias", "target alias value");
        in_map.insert("comment", "comment value");
        in_map.insert("username", "username value");
        assert!(
            matches!(
                entry.update_attributes(&in_map).await,
                Err(ErrorCode::NoEntry)
            ),
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
        assert_eq!(out_map["target_alias"], cred.target_alias);
        assert_eq!(out_map["comment"], cred.comment);
        assert_eq!(out_map["username"], cred.username);
        assert!(
            matches!(entry.update_attributes(&in_map).await, Ok(())),
            "Couldn't update attributes in attribute test",
        );
        let after_map = entry
            .get_attributes()
            .await
            .expect("Can't get attributes after update");
        assert_eq!(after_map["target_alias"], in_map["target_alias"]);
        assert_eq!(after_map["comment"], in_map["comment"]);
        assert_eq!(after_map["username"], in_map["username"]);
        assert!(!after_map.contains_key("label"));
        assert!(!after_map.contains_key("attribute name"));
        entry
            .delete_credential()
            .await
            .unwrap_or_else(|err| panic!("Can't delete credential for attribute test: {err:?}"));
        assert!(
            matches!(entry.get_attributes().await, Err(ErrorCode::NoEntry)),
            "Read deleted credential in attribute test",
        );
    }

    #[tokio::test]
    async fn test_get_credential() {
        let name = generate_random_string();
        let entry = entry_new(&name, &name);
        let password = "test get password";
        entry
            .set_password(password)
            .await
            .expect("Can't set test get password");
        let credential: &WinCredential = entry
            .get_credential()
            .downcast_ref()
            .expect("Not a windows credential");
        let actual = credential
            .get_credential()
            .await
            .expect("Can't read credential");
        assert_eq!(
            actual.username, credential.username,
            "Usernames don't match"
        );
        assert_eq!(
            actual.target_name, credential.target_name,
            "Target names don't match"
        );
        assert_eq!(
            actual.target_alias, credential.target_alias,
            "Target aliases don't match"
        );
        assert_eq!(actual.comment, credential.comment, "Comments don't match");
        entry
            .delete_credential()
            .await
            .expect("Couldn't delete get-credential");
        assert!(matches!(
            entry.get_password().await,
            Err(ErrorCode::NoEntry)
        ));
    }
}
