// TLS backend selection.
//
// TLS backend selection: `rustls-tls` (default) or `native-tls`, as additive Cargo features.
// `rustls-tls` wins when both are on (e.g. `--all-features`), so `native-tls` code is gated on
// `all(feature = "native-tls", not(feature = "rustls-tls"))`. With neither, the crate still builds
// (for `--no-default-features` checks) but has no working TLS.

/// A human-readable description of the TLS backend, for diagnostic logging (`uv -vv`).
pub fn tls_stack() -> String {
    #[cfg(feature = "rustls-tls")]
    {
        "rustls (aws-lc-rs)".to_string()
    }

    #[cfg(all(feature = "native-tls", not(feature = "rustls-tls")))]
    {
        native_tls_stack()
    }

    #[cfg(not(any(feature = "rustls-tls", feature = "native-tls")))]
    {
        "none".to_string()
    }
}

/// Describe the `native-tls` backend for the current target platform.
#[cfg(all(feature = "native-tls", not(feature = "rustls-tls")))]
fn native_tls_stack() -> String {
    // Apple/Windows use the OS TLS stack (no version); elsewhere OpenSSL (has one).
    #[cfg(target_vendor = "apple")]
    {
        "native-tls (SecureTransport)".to_string()
    }

    #[cfg(target_os = "windows")]
    {
        "native-tls (SChannel)".to_string()
    }

    #[cfg(not(any(target_vendor = "apple", target_os = "windows")))]
    {
        format!("native-tls ({})", openssl::version::version())
    }
}

#[cfg(any(feature = "rustls-tls", feature = "native-tls"))]
use std::io::{self, Read};

#[cfg(any(feature = "rustls-tls", feature = "native-tls"))]
use reqwest::Identity;

#[cfg(feature = "rustls-tls")]
mod rustls;

#[cfg(feature = "rustls-tls")]
pub use self::rustls::{CertificateFileError, Certificates};

#[cfg(any(feature = "rustls-tls", feature = "native-tls"))]
#[derive(thiserror::Error, Debug)]
pub(crate) enum CertificateError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Reqwest(reqwest::Error),
}

/// Return the [`Identity`] from the provided file.
///
/// The file is expected to contain a PEM-encoded certificate chain and private key.
#[cfg(any(feature = "rustls-tls", feature = "native-tls"))]
pub(crate) fn read_identity(
    ssl_client_cert: &std::ffi::OsStr,
) -> Result<Identity, CertificateError> {
    let mut buf = Vec::new();
    fs_err::File::open(ssl_client_cert)?.read_to_end(&mut buf)?;

    #[cfg(feature = "rustls-tls")]
    {
        Identity::from_pem(&buf).map_err(|tls_err| {
            debug_assert!(tls_err.is_builder(), "must be a rustls::Error internally");
            CertificateError::Reqwest(tls_err)
        })
    }

    #[cfg(all(feature = "native-tls", not(feature = "rustls-tls")))]
    {
        // `Identity::from_pkcs8_pem` requires the key argument to begin with the
        // PKCS#8 PEM header. The certificate argument is parsed by OpenSSL's
        // `X509::stack_from_pem`, which only extracts certificate blocks and ignores
        // private key blocks, so we can pass the full buffer as-is.
        const KEY_MARKER: &[u8] = b"-----BEGIN PRIVATE KEY-----";
        let key_start = buf
            .windows(KEY_MARKER.len())
            .position(|window| window == KEY_MARKER)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "no PKCS#8 private key found in client certificate file",
                )
            })?;
        Identity::from_pkcs8_pem(&buf, &buf[key_start..]).map_err(CertificateError::Reqwest)
    }
}
