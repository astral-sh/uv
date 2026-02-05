use reqwest::Identity;
use std::ffi::OsStr;
use std::io::Read;

#[derive(thiserror::Error, Debug)]
pub(crate) enum CertificateError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Reqwest(reqwest::Error),
    #[error("No certificate found in PEM file")]
    NoCertificate,
    #[error("No private key found in PEM file")]
    NoPrivateKey,
}

/// Return the `Identity` from the provided file (for native-tls backend).
///
/// Uses `Identity::from_pkcs8_pem` which creates a native-tls-compatible identity.
/// The PEM file should contain both certificate(s) and a private key.
pub(crate) fn read_identity_native_tls(
    ssl_client_cert: &OsStr,
) -> Result<Identity, CertificateError> {
    let mut buf = Vec::new();
    fs_err::File::open(ssl_client_cert)?.read_to_end(&mut buf)?;

    // Split the PEM file into certificate and key parts
    let (cert_pem, key_pem) = split_pem_cert_and_key(&buf)?;

    Identity::from_pkcs8_pem(&cert_pem, &key_pem).map_err(CertificateError::Reqwest)
}

/// Split a combined PEM file into certificate and private key parts.
fn split_pem_cert_and_key(pem_data: &[u8]) -> Result<(Vec<u8>, Vec<u8>), CertificateError> {
    let pem_str = std::str::from_utf8(pem_data).map_err(|_| CertificateError::NoCertificate)?;

    let mut cert_parts = Vec::new();
    let mut key_part = None;

    // Simple PEM parsing - look for BEGIN/END markers
    let mut current_section = String::new();
    let mut in_section = false;
    let mut section_type = String::new();

    for line in pem_str.lines() {
        if line.starts_with("-----BEGIN ") {
            in_section = true;
            section_type = line
                .strip_prefix("-----BEGIN ")
                .and_then(|s| s.strip_suffix("-----"))
                .unwrap_or("")
                .to_string();
            current_section = format!("{line}\n");
        } else if line.starts_with("-----END ") {
            current_section.push_str(line);
            current_section.push('\n');
            in_section = false;

            // Determine if this is a cert or key
            if section_type.contains("CERTIFICATE") {
                cert_parts.push(current_section.clone());
            } else if section_type.contains("PRIVATE KEY") {
                key_part = Some(current_section.clone());
            }
            current_section.clear();
        } else if in_section {
            current_section.push_str(line);
            current_section.push('\n');
        }
    }

    if cert_parts.is_empty() {
        return Err(CertificateError::NoCertificate);
    }

    let key_pem = key_part.ok_or(CertificateError::NoPrivateKey)?;
    let cert_pem = cert_parts.join("");

    Ok((cert_pem.into_bytes(), key_pem.into_bytes()))
}
