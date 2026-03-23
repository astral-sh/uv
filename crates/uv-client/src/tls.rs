use std::env;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use itertools::Itertools;
use reqwest::{Certificate, Identity};
use rustls_native_certs::{CertificateResult, load_certs_from_paths};
use rustls_pki_types::CertificateDer;
use tracing::debug;

use uv_fs::Simplified;
use uv_static::EnvVars;
use uv_warnings::warn_user_once;

/// A collection of TLS certificates in DER form.
#[derive(Debug, Clone, Default)]
pub(crate) struct Certificates(Vec<CertificateDer<'static>>);

impl Certificates {
    /// Load the bundled Mozilla root certificates.
    ///
    /// We use `webpki-root-certs` (which gives us [`CertificateDer`] values) rather than the more
    /// space-efficient `webpki-roots` (pre-parsed [`TrustAnchor`] values) because reqwest's
    /// [`ClientBuilder::tls_certs_only`] accepts [`Certificate`] values built from DER bytes. Using
    /// `webpki-roots` would require constructing a [`rustls::ClientConfig`] manually and passing it
    /// via the semver-unstable [`ClientBuilder::tls_backend_preconfigured`], which also means
    /// taking ownership of ALPN, SNI, certificate verification, and mTLS configuration that reqwest
    /// otherwise handles for us.
    pub(crate) fn webpki_roots() -> Self {
        // Each [`CertificateDer`] in [`webpki_root_certs::TLS_SERVER_ROOT_CERTS`] borrows from static
        // data, so cloning into the [`Vec`] only copies the fat pointer, not the certificate bytes.
        Self(webpki_root_certs::TLS_SERVER_ROOT_CERTS.to_vec())
    }

    /// Load custom CA certificates from `SSL_CERT_FILE` and `SSL_CERT_DIR` environment variables.
    ///
    /// Returns `None` if neither variable is set, if the referenced files or directories are
    /// missing or inaccessible, or if no valid certificates are found (with a warning in each
    /// case). Delegates path loading to [`rustls_native_certs::load_certs_from_paths`].
    pub(crate) fn from_env() -> Option<Self> {
        let mut certs = Self::default();
        let mut has_source = false;

        if let Some(ssl_cert_file) = env::var_os(EnvVars::SSL_CERT_FILE)
            && let Some(file_certs) = Self::from_ssl_cert_file(&ssl_cert_file)
        {
            has_source = true;
            certs.merge(file_certs);
        }

        if let Some(ssl_cert_dir) = env::var_os(EnvVars::SSL_CERT_DIR)
            && let Some(dir_certs) = Self::from_ssl_cert_dir(&ssl_cert_dir)
        {
            has_source = true;
            certs.merge(dir_certs);
        }

        if has_source { Some(certs) } else { None }
    }

    /// Load certificates from the value of `SSL_CERT_FILE`.
    ///
    /// Returns `None` if the value is empty, the path does not refer to an accessible file,
    /// or the file contains no valid certificates.
    fn from_ssl_cert_file(ssl_cert_file: &std::ffi::OsStr) -> Option<Self> {
        if ssl_cert_file.is_empty() {
            return None;
        }

        let file = PathBuf::from(ssl_cert_file);
        match file.metadata() {
            Ok(metadata) if metadata.is_file() => {
                let result = Self::from_paths(Some(&file), None);
                for err in &result.errors {
                    warn_user_once!(
                        "Failed to load `SSL_CERT_FILE` ({}): {err}",
                        file.simplified_display().cyan()
                    );
                }
                let certs = Self::from(result);
                if certs.0.is_empty() {
                    warn_user_once!(
                        "Ignoring `SSL_CERT_FILE`. No certificates found in: {}.",
                        file.simplified_display().cyan()
                    );
                    return None;
                }
                Some(certs)
            }
            Ok(_) => {
                warn_user_once!(
                    "Ignoring invalid `SSL_CERT_FILE`. Path is not a file: {}.",
                    file.simplified_display().cyan()
                );
                None
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                warn_user_once!(
                    "Ignoring invalid `SSL_CERT_FILE`. Path does not exist: {}.",
                    file.simplified_display().cyan()
                );
                None
            }
            Err(err) => {
                warn_user_once!(
                    "Ignoring invalid `SSL_CERT_FILE`. Path is not accessible: {} ({err}).",
                    file.simplified_display().cyan()
                );
                None
            }
        }
    }

    /// Load certificates from the value of `SSL_CERT_DIR`.
    ///
    /// The value may include multiple entries, separated by a platform-specific delimiter (`:` on
    /// Unix, `;` on Windows).
    ///
    /// Returns `None` if the value is empty, no listed directories exist, or no valid
    /// certificates are found.
    fn from_ssl_cert_dir(ssl_cert_dir: &std::ffi::OsStr) -> Option<Self> {
        if ssl_cert_dir.is_empty() {
            return None;
        }

        let (existing, missing): (Vec<_>, Vec<_>) =
            env::split_paths(ssl_cert_dir).partition(|path| path.exists());

        if existing.is_empty() {
            let end_note = if missing.len() == 1 {
                "The directory does not exist."
            } else {
                "The entries do not exist."
            };
            warn_user_once!(
                "Ignoring invalid `SSL_CERT_DIR`. {end_note}: {}.",
                missing
                    .iter()
                    .map(Simplified::simplified_display)
                    .join(", ")
                    .cyan()
            );
            return None;
        }

        if !missing.is_empty() {
            let end_note = if missing.len() == 1 {
                "The following directory does not exist:"
            } else {
                "The following entries do not exist:"
            };
            warn_user_once!(
                "Invalid entries in `SSL_CERT_DIR`. {end_note}: {}.",
                missing
                    .iter()
                    .map(Simplified::simplified_display)
                    .join(", ")
                    .cyan()
            );
        }

        let mut certs = Self::default();
        for dir in &existing {
            let result = Self::from_paths(None, Some(dir));
            for err in &result.errors {
                warn_user_once!(
                    "Failed to load `SSL_CERT_DIR` ({}): {err}",
                    dir.simplified_display().cyan()
                );
            }
            certs.merge(Self::from(result));
        }

        if certs.0.is_empty() {
            warn_user_once!(
                "Ignoring `SSL_CERT_DIR`. No certificates found in: {}.",
                existing
                    .iter()
                    .map(Simplified::simplified_display)
                    .join(", ")
                    .cyan()
            );
            return None;
        }

        Some(certs)
    }

    /// Load certificates from explicit file and directory paths.
    fn from_paths(file: Option<&Path>, dir: Option<&Path>) -> CertificateResult {
        load_certs_from_paths(file, dir)
    }

    /// Remove duplicate certificates, sorting by DER bytes.
    fn dedup(&mut self) {
        self.0
            .sort_unstable_by(|left, right| left.as_ref().cmp(right.as_ref()));
        self.0.dedup();
    }

    /// Merge another set of certificates into this one.
    ///
    /// After merging, duplicates are removed.
    fn merge(&mut self, other: Self) {
        self.0.extend(other.0);
        self.dedup();
    }

    /// Convert certificates to reqwest [`Certificate`] objects.
    pub(crate) fn to_reqwest_certs(&self) -> Vec<Certificate> {
        self.0
            .iter()
            // `Certificate::from_der` returns a `Result` for backend compatibility, but these
            // certificates come from `rustls-native-certs` and are already validated DER certs.
            // In our rustls-based client configuration this conversion is expected to succeed.
            .filter_map(|cert| match Certificate::from_der(cert) {
                Ok(certificate) => Some(certificate),
                Err(err) => {
                    debug!("Failed to convert DER certificate to reqwest certificate: {err}");
                    None
                }
            })
            .collect()
    }

    /// Iterate over raw DER certificates.
    #[cfg(test)]
    fn iter(&self) -> impl Iterator<Item = &CertificateDer<'static>> {
        self.0.iter()
    }
}

impl From<CertificateResult> for Certificates {
    fn from(result: CertificateResult) -> Self {
        Self(result.certs)
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum CertificateError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Reqwest(reqwest::Error),
}

/// Return the `Identity` from the provided file.
pub(crate) fn read_identity(
    ssl_client_cert: &std::ffi::OsStr,
) -> Result<Identity, CertificateError> {
    let mut buf = Vec::new();
    fs_err::File::open(ssl_client_cert)?.read_to_end(&mut buf)?;
    Identity::from_pem(&buf).map_err(|tls_err| {
        debug_assert!(tls_err.is_builder(), "must be a rustls::Error internally");
        CertificateError::Reqwest(tls_err)
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    fn generate_cert_pem() -> String {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        cert.cert.pem()
    }

    #[test]
    fn test_from_ssl_cert_file_nonexistent_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let missing_file = dir.path().join("missing.pem");

        let certs = Certificates::from_ssl_cert_file(missing_file.as_os_str());
        assert!(certs.is_none());
    }

    #[test]
    fn test_from_ssl_cert_file_empty_value_returns_none() {
        let certs = Certificates::from_ssl_cert_file(OsString::new().as_os_str());
        assert!(certs.is_none());
    }

    #[test]
    fn test_from_ssl_cert_file_no_valid_certs_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let cert_path = dir.path().join("empty.pem");
        fs_err::write(&cert_path, "not a certificate").unwrap();

        let certs = Certificates::from_ssl_cert_file(cert_path.as_os_str());
        assert!(certs.is_none());
    }

    #[test]
    fn test_from_ssl_cert_dir_empty_value_returns_none() {
        let certs = Certificates::from_ssl_cert_dir(OsString::new().as_os_str());
        assert!(certs.is_none());
    }

    #[test]
    fn test_from_ssl_cert_dir_nonexistent_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let missing_dir = dir.path().join("missing-dir");
        let cert_dirs = std::env::join_paths([&missing_dir]).unwrap();

        let certs = Certificates::from_ssl_cert_dir(cert_dirs.as_os_str());
        assert!(certs.is_none());
    }

    #[test]
    fn test_from_ssl_cert_dir_empty_existing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let cert_dirs = std::env::join_paths([dir.path()]).unwrap();

        let certs = Certificates::from_ssl_cert_dir(cert_dirs.as_os_str());
        assert!(certs.is_none());
    }

    #[test]
    fn test_merge_deduplicates() {
        let dir = tempfile::tempdir().unwrap();
        let cert_path = dir.path().join("cert.pem");
        fs_err::write(&cert_path, generate_cert_pem()).unwrap();

        let first = Certificates::from(Certificates::from_paths(Some(&cert_path), None));
        let second = Certificates::from(Certificates::from_paths(Some(&cert_path), None));

        let mut merged = first;
        merged.merge(second);

        assert_eq!(merged.iter().count(), 1);
    }

    #[test]
    fn test_webpki_roots_not_empty() {
        let certs = Certificates::webpki_roots();
        assert!(certs.iter().count() > 0);
    }
}
