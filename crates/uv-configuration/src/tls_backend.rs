/// TLS backend to use for HTTPS connections.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum TlsBackend {
    /// Use rustls.
    #[default]
    Rustls,
    /// Use the platform's native TLS implementation.
    #[serde(alias = "native-tls")]
    #[cfg_attr(feature = "clap", value(alias = "native-tls"))]
    Native,
}

impl std::fmt::Display for TlsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rustls => write!(f, "rustls"),
            Self::Native => write!(f, "native"),
        }
    }
}
