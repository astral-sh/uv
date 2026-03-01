/// TLS backend to use for HTTPS connections.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum TlsBackend {
    /// Use rustls with bundled webpki-root-certs certificates.
    #[default]
    #[serde(alias = "webpki")]
    RustlsWebpki,
    /// Use rustls with rustls-platform-verifier (system certificate store).
    Rustls,
    /// Use the platform's native TLS implementation.
    NativeTls,
}

impl TlsBackend {
    /// Returns `true` if this is the native-tls backend.
    pub fn is_native_tls(&self) -> bool {
        matches!(self, Self::NativeTls)
    }

    /// Returns `true` if this is a rustls-based backend.
    pub fn is_rustls(&self) -> bool {
        matches!(self, Self::Rustls | Self::RustlsWebpki)
    }
}

impl std::fmt::Display for TlsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RustlsWebpki => write!(f, "rustls-webpki"),
            Self::Rustls => write!(f, "rustls"),
            Self::NativeTls => write!(f, "native-tls"),
        }
    }
}
