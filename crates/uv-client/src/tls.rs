use rustls::ClientConfig;
use tracing::warn;

#[derive(thiserror::Error, Debug)]
pub(crate) enum TlsError {
    #[error(transparent)]
    Rustls(#[from] rustls::Error),
    #[error("zero valid certificates found in native root store")]
    ZeroCertificates,
    #[error("failed to load native root certificates")]
    NativeCertificates(#[source] std::io::Error),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Roots {
    /// Use reqwest's `rustls-tls-webpki-roots` behavior for loading root certificates.
    Webpki,
    /// Use reqwest's `rustls-tls-native-roots` behavior for loading root certificates.
    Native,
}

/// Initialize a TLS configuration for the client.
///
/// This is equivalent to the TLS initialization `reqwest` when `rustls-tls` is enabled,
/// with two notable changes:
///
/// 1. It enables _either_ the `webpki-roots` or the `native-certs` feature, but not both.
/// 2. It assumes the following builder settings (which match the defaults):
///    - `root_certs: vec![]`
///    - `min_tls_version: None`
///    - `max_tls_version: None`
///    - `identity: None`
///    - `certs_verification: false`
///    - `tls_sni: true`
///    - `http_version_pref: HttpVersionPref::All`
///
/// See: <https://github.com/seanmonstar/reqwest/blob/e3192638518d577759dd89da489175b8f992b12f/src/async_impl/client.rs#L498>
pub(crate) fn load(roots: Roots) -> Result<ClientConfig, TlsError> {
    // Set root certificates.
    let mut root_cert_store = rustls::RootCertStore::empty();

    match roots {
        Roots::Webpki => {
            // Use `rustls-tls-webpki-roots`
            use rustls::OwnedTrustAnchor;

            let trust_anchors = webpki_roots::TLS_SERVER_ROOTS.iter().map(|trust_anchor| {
                OwnedTrustAnchor::from_subject_spki_name_constraints(
                    trust_anchor.subject,
                    trust_anchor.spki,
                    trust_anchor.name_constraints,
                )
            });

            root_cert_store.add_trust_anchors(trust_anchors);
        }
        Roots::Native => {
            // Use: `rustls-tls-native-roots`
            let mut valid_count = 0;
            let mut invalid_count = 0;
            for cert in
                rustls_native_certs::load_native_certs().map_err(TlsError::NativeCertificates)?
            {
                let cert = rustls::Certificate(cert.0);
                // Continue on parsing errors, as native stores often include ancient or syntactically
                // invalid certificates, like root certificates without any X509 extensions.
                // Inspiration: https://github.com/rustls/rustls/blob/633bf4ba9d9521a95f68766d04c22e2b01e68318/rustls/src/anchors.rs#L105-L112
                match root_cert_store.add(&cert) {
                    Ok(_) => valid_count += 1,
                    Err(err) => {
                        invalid_count += 1;
                        warn!(
                            "rustls failed to parse DER certificate {:?} {:?}",
                            &err, &cert
                        );
                    }
                }
            }
            if valid_count == 0 && invalid_count > 0 {
                return Err(TlsError::ZeroCertificates);
            }
        }
    }

    // Build TLS config
    let config_builder = ClientConfig::builder()
        .with_safe_default_cipher_suites()
        .with_safe_default_kx_groups()
        .with_protocol_versions(rustls::ALL_VERSIONS)?
        .with_root_certificates(root_cert_store);

    // Finalize TLS config
    let mut tls = config_builder.with_no_client_auth();

    // Enable SNI
    tls.enable_sni = true;

    // ALPN protocol
    tls.alpn_protocols = vec!["h2".into(), "http/1.1".into()];

    Ok(tls)
}
