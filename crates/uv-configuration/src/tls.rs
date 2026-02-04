use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// The TLS backend to use for HTTPS connections.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum TlsBackend {
    /// Use the system's native TLS implementation (e.g., `Security.framework` on macOS,
    /// `SChannel` on Windows, `OpenSSL` on Linux).
    ///
    /// This backend uses the system's certificate store and may provide better
    /// compatibility with corporate proxies and custom CA certificates.
    NativeTls,

    /// Use rustls with webpki-roots (Mozilla's root certificates).
    ///
    /// This is the default backend, providing consistent behavior across platforms
    /// using a bundled set of trusted root certificates.
    #[default]
    Rustls,
}

impl Display for TlsBackend {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NativeTls => write!(f, "native-tls"),
            Self::Rustls => write!(f, "rustls"),
        }
    }
}

impl FromStr for TlsBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "native-tls" | "native_tls" | "nativetls" => Ok(Self::NativeTls),
            "rustls" => Ok(Self::Rustls),
            _ => Err(format!(
                "Invalid TLS backend: `{s}`. Expected `native-tls` or `rustls`."
            )),
        }
    }
}
