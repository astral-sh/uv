use uv_auth::{KeyringProvider, TextCredentialStore, TomlCredentialError};
use uv_configuration::KeyringProviderType;
use uv_fs::LockedFile;
use uv_preview::Preview;

pub(crate) mod dir;
pub(crate) mod login;
pub(crate) mod logout;
pub(crate) mod token;

/// The storage backend to use in `uv auth` commands.
enum AuthBackend {
    Keyring(KeyringProvider),
    TextStore(TextCredentialStore, LockedFile),
}

impl AuthBackend {
    fn from_settings(
        keyring: Option<&KeyringProviderType>,
        preview: Preview,
    ) -> Result<Self, TomlCredentialError> {
        // For keyring providers, we only support persistence via the native keyring right now
        if let Some(keyring) = match keyring {
            Some(provider @ KeyringProviderType::Native) => provider.to_provider(&preview),
            _ => None,
        } {
            return Ok(Self::Keyring(keyring));
        }

        // Otherwise, we'll use the plain text credential store
        let path = TextCredentialStore::default_file()?;
        match TextCredentialStore::read(&path) {
            Ok((store, lock)) => Ok(Self::TextStore(store, lock)),
            Err(TomlCredentialError::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self::TextStore(
                    TextCredentialStore::default(),
                    TextCredentialStore::lock(&path)?,
                ))
            }
            Err(err) => Err(err),
        }
    }
}
