use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;
use std::{env, io};

use itertools::Itertools;
use reqwest::{Certificate, Identity};
use tracing::debug;

use uv_fs::Simplified;
use uv_static::EnvVars;
use uv_warnings::warn_user_once;

/// Bundled Mozilla root certificates, converted to reqwest's `Certificate` type.
static WEBPKI_ROOT_CERTIFICATES: LazyLock<Vec<Certificate>> = LazyLock::new(|| {
    webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .filter_map(|cert_der| Certificate::from_der(cert_der).ok())
        .collect()
});

/// A collection of TLS certificates.
#[derive(Debug, Clone, Default)]
pub(crate) struct Certificates(Vec<Certificate>);

impl Certificates {
    /// Load the bundled Mozilla root certificates.
    pub(crate) fn webpki_roots() -> Self {
        Self(WEBPKI_ROOT_CERTIFICATES.clone())
    }

    /// Load custom CA certificates from `SSL_CERT_FILE` and `SSL_CERT_DIR` environment variables.
    /// Load custom CA certificates from `SSL_CERT_FILE` and `SSL_CERT_DIR` environment variables.
    ///
    /// Returns `None` if no sources could be successfully read (env vars unset, paths don't
    /// exist, etc.), indicating the default certificate source should be used. Returns
    /// `Some(certs)` when at least one file or directory was successfully read, even if no
    /// certificates were parsed from it.
    pub(crate) fn from_env() -> Option<Self> {
        let mut certs = Self::default();
        let mut has_source = false;

        // Load from `SSL_CERT_FILE`.
        if let Ok(cert_file) = env::var(EnvVars::SSL_CERT_FILE) {
            let path = Path::new(&cert_file);
            match Self::from_file(path) {
                Ok(loaded) => {
                    debug!(
                        "Loaded {} certificate(s) from `SSL_CERT_FILE`: {}",
                        loaded.iter().count(),
                        path.simplified_display()
                    );
                    has_source = true;
                    certs.extend(loaded);
                }
                Err(err) => {
                    warn_user_once!(
                        "Ignoring invalid `SSL_CERT_FILE` ({}): {err}",
                        path.simplified_display().cyan()
                    );
                }
            }
        }

        // Load from `SSL_CERT_DIR`.
        if let Ok(cert_dirs) = env::var(EnvVars::SSL_CERT_DIR) {
            if !cert_dirs.is_empty() {
                let paths: Vec<_> = env::split_paths(&cert_dirs).collect();
                let (existing, missing): (Vec<_>, Vec<_>) = paths.iter().partition(|p| p.exists());

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
                } else {
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

                    for dir in existing {
                        match Self::from_dir(dir) {
                            Ok(loaded) => {
                                has_source = true;
                                certs.extend(loaded);
                            }
                            Err(err) => {
                                warn_user_once!(
                                    "Failed to read `SSL_CERT_DIR` ({}): {err}",
                                    dir.simplified_display().cyan()
                                );
                            }
                        }
                    }
                }
            }
        }

        // TODO(zanieb): For consistency with other tools, return `Some` when the variable is
        // set to a non-empty value, even if no sources could be read.
        // See https://github.com/astral-sh/uv/issues/18526
        if has_source { Some(certs) } else { None }
    }

    /// Load certificates from a PEM file (single cert or bundle).
    fn from_file(path: &Path) -> Result<Self, CertificateError> {
        let cert_data = fs_err::read(path)?;
        let certs = Self::load(&cert_data).map_err(CertificateError::Reqwest)?;
        Ok(Self(certs))
    }

    /// Load all certificate files from a directory.
    ///
    /// Only files with `.crt`, `.pem`, or `.cer` extensions are loaded.
    /// Individual file errors are logged and skipped.
    fn from_dir(dir: &Path) -> Result<Self, CertificateError> {
        let mut certs = Vec::new();
        for entry in fs_err::read_dir(dir)?.flatten() {
            let path = entry.path();

            // Only process files with .crt, .pem, or .cer extensions.
            if !matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("crt" | "pem" | "cer")
            ) {
                continue;
            }

            match Self::from_file(&path) {
                Ok(loaded) => {
                    debug!(
                        "Loaded {} certificate(s) from {}",
                        loaded.iter().count(),
                        path.simplified_display()
                    );
                    certs.extend(loaded.0);
                }
                Err(err) => {
                    debug!("Skipping {}: {err}", path.simplified_display());
                }
            }
        }

        Ok(Self(certs))
    }

    /// Iterate over the certificates.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Certificate> {
        self.0.iter()
    }

    /// Extend with certificates from another collection.
    pub(crate) fn extend(&mut self, other: Self) {
        self.0.extend(other.0);
    }

    /// Load certificates from PEM data, supporting both bundles and single certs.
    fn load(cert_data: &[u8]) -> Result<Vec<Certificate>, reqwest::Error> {
        let certs = Certificate::from_pem_bundle(cert_data)
            .or_else(|_| Certificate::from_pem(cert_data).map(|cert| vec![cert]))?;

        Ok(certs)
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
    use super::*;

    /// Generate a self-signed certificate and return the PEM-encoded public cert.
    fn generate_cert_pem() -> String {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .expect("Failed to generate self-signed cert");
        cert.cert.pem()
    }

    #[test]
    fn test_from_file_nonexistent() {
        let result = Certificates::from_file(Path::new("/nonexistent/path/cert.pem"));
        assert!(result.is_err());
    }

    #[test]
    fn test_from_file_invalid_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.pem");
        fs_err::write(&path, "not a certificate").unwrap();

        // Invalid PEM content is not an IO error — it parses successfully but yields no certs.
        let certs = Certificates::from_file(&path).unwrap();
        assert_eq!(certs.iter().count(), 0);
    }

    #[test]
    fn test_from_file_valid_cert() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cert.pem");
        fs_err::write(&path, generate_cert_pem()).unwrap();

        let certs = Certificates::from_file(&path).unwrap();
        assert!(certs.iter().count() > 0);
    }

    #[test]
    fn test_from_file_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bundle.pem");
        let bundle = format!("{}\n{}", generate_cert_pem(), generate_cert_pem());
        fs_err::write(&path, bundle).unwrap();

        let certs = Certificates::from_file(&path).unwrap();
        assert!(certs.iter().count() > 0);
    }

    #[test]
    fn test_from_dir_nonexistent() {
        let result = Certificates::from_dir(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_from_dir_empty() {
        let dir = tempfile::tempdir().unwrap();
        let certs = Certificates::from_dir(dir.path()).unwrap();
        assert_eq!(certs.iter().count(), 0);
    }

    #[test]
    fn test_from_dir_with_certs() {
        let dir = tempfile::tempdir().unwrap();
        fs_err::write(dir.path().join("cert.pem"), generate_cert_pem()).unwrap();
        fs_err::write(dir.path().join("cert.crt"), generate_cert_pem()).unwrap();

        let certs = Certificates::from_dir(dir.path()).unwrap();
        assert!(certs.iter().count() > 0);
    }

    #[test]
    fn test_from_dir_ignores_wrong_extensions() {
        let dir = tempfile::tempdir().unwrap();
        fs_err::write(dir.path().join("cert.txt"), generate_cert_pem()).unwrap();
        fs_err::write(dir.path().join("cert"), generate_cert_pem()).unwrap();

        let certs = Certificates::from_dir(dir.path()).unwrap();
        assert_eq!(certs.iter().count(), 0);
    }

    #[test]
    fn test_webpki_roots_not_empty() {
        let certs = Certificates::webpki_roots();
        assert!(certs.iter().count() > 0);
    }

    #[test]
    fn test_extend_combines_certs() {
        let dir = tempfile::tempdir().unwrap();
        let path1 = dir.path().join("a.pem");
        let path2 = dir.path().join("b.pem");
        fs_err::write(&path1, generate_cert_pem()).unwrap();
        fs_err::write(&path2, generate_cert_pem()).unwrap();

        let mut certs = Certificates::from_file(&path1).unwrap();
        let count_before = certs.iter().count();
        certs.extend(Certificates::from_file(&path2).unwrap());
        assert!(certs.iter().count() > count_before);
    }
}
