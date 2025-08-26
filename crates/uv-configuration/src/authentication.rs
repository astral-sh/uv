use uv_auth::{self, KeyringProvider};

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
    pub fn to_provider(&self) -> Option<KeyringProvider> {
        match self {
            Self::Disabled => None,
            Self::Native => Some(KeyringProvider::native()),
            Self::Subprocess => Some(KeyringProvider::subprocess()),
        }
    }
}
