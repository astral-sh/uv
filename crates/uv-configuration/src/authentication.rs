use uv_auth::{self, KeyringProvider};
use uv_preview::{Preview, PreviewFeatures};
use uv_warnings::warn_user_once;

/// Keyring provider type to use for credential lookup.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum KeyringProviderType {
    /// Do not use keyring for credential lookup.
    #[default]
    Disabled,
    /// Use a native integration with the system keychain for credential lookup.
    Native,
    /// Use the `keyring` command for credential lookup.
    Subprocess,
    // /// Not yet implemented
    // Auto,
    // /// Not implemented yet. Maybe use <https://docs.rs/keyring/latest/keyring/> for this?
    // Import,
}
// See <https://pip.pypa.io/en/stable/topics/authentication/#keyring-support> for details.

impl KeyringProviderType {
    pub fn to_provider(&self, preview: &Preview) -> Option<KeyringProvider> {
        match self {
            Self::Disabled => None,
            Self::Native => {
                if !preview.is_enabled(PreviewFeatures::NATIVE_KEYRING) {
                    warn_user_once!(
                        "The native keyring provider is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
                        PreviewFeatures::NATIVE_KEYRING
                    );
                }
                Some(KeyringProvider::native())
            }
            Self::Subprocess => Some(KeyringProvider::subprocess()),
        }
    }
}

impl std::fmt::Display for KeyringProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "disabled"),
            Self::Native => write!(f, "native"),
            Self::Subprocess => write!(f, "subprocess"),
        }
    }
}

