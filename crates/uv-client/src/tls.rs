use std::env;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::LazyLock;

use reqwest::{Certificate, Identity};
use rustls_native_certs::load_certs_from_paths;
use rustls_pki_types::CertificateDer;

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
    ///
    /// Delegates to [`rustls_native_certs::load_certs_from_paths`] for the actual certificate
    /// loading, which handles PEM parsing, symlink resolution, OpenSSL hash-named files, and
    /// deduplication.
    ///
    /// Returns `None` if neither environment variable is set. Returns `Some(certs)` when either
    /// variable is set, even if no certificates were successfully loaded, so the environment
    /// override remains authoritative.
    pub(crate) fn from_env() -> Option<Self> {
        let cert_file = env::var_os(EnvVars::SSL_CERT_FILE).map(PathBuf::from);
        let cert_dirs = env::var_os(EnvVars::SSL_CERT_DIR);

        if cert_file.is_none() && cert_dirs.is_none() {
            return None;
        }

        let mut der_certs: Vec<CertificateDer<'static>> = Vec::new();

        // Load from `SSL_CERT_FILE`.
        if let Some(ref file) = cert_file {
            let result = load_certs_from_paths(Some(file), None);
            for err in &result.errors {
                warn_user_once!(
                    "Failed to load `SSL_CERT_FILE` ({}): {err}",
                    file.simplified_display().cyan()
                );
            }
            der_certs.extend(result.certs);
        }

        // Load from `SSL_CERT_DIR` (colon-separated on Unix, semicolon on Windows).
        if let Some(ref dirs) = cert_dirs {
            for dir in env::split_paths(dirs) {
                let result = load_certs_from_paths(None, Some(&dir));
                for err in &result.errors {
                    warn_user_once!(
                        "Failed to load `SSL_CERT_DIR` ({}): {err}",
                        dir.simplified_display().cyan()
                    );
                }
                der_certs.extend(result.certs);
            }
        }

        // Deduplicate, matching rustls-native-certs behavior.
        der_certs.sort_unstable_by(|a, b| a.as_ref().cmp(b.as_ref()));
        der_certs.dedup();

        // Convert to reqwest certificates.
        let certs = der_certs
            .into_iter()
            .filter_map(|der| Certificate::from_der(&der).ok())
            .collect();

        Some(Self(certs))
    }

    /// Load certificates from a PEM file (single cert or bundle).
    #[cfg(test)]
    fn from_file(path: &std::path::Path) -> Result<Self, CertificateError> {
        let cert_data = fs_err::read(path)?;
        let certs = Self::load(&cert_data).map_err(CertificateError::Reqwest)?;
        Ok(Self(certs))
    }

    /// Load all certificate files from a directory.
    ///
    /// Any regular file in the directory is considered, including OpenSSL hash-based filenames
    /// like `fd3003c5.0`. Individual file errors are logged and skipped.
    #[cfg(test)]
    fn from_dir(dir: &std::path::Path) -> Result<Self, CertificateError> {
        let mut certs = Vec::new();
        for entry in fs_err::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            let metadata = match fs_err::metadata(&path) {
                Ok(metadata) => metadata,
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    // Skip dangling symlinks.
                    continue;
                }
                Err(err) => return Err(err.into()),
            };

            if !metadata.is_file() {
                continue;
            }

            if let Ok(loaded) = Self::from_file(&path) {
                certs.extend(loaded.0);
            }
        }

        Ok(Self(certs))
    }

    /// Iterate over the certificates.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Certificate> {
        self.0.iter()
    }

    /// Extend with certificates from another collection.
    #[cfg(test)]
    fn extend(&mut self, other: Self) {
        self.0.extend(other.0);
    }

    /// Load certificates from PEM data, supporting both bundles and single certs.
    #[cfg(test)]
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
    use std::path::Path;

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
    fn test_from_dir_accepts_hash_named_files() {
        let dir = tempfile::tempdir().unwrap();
        fs_err::write(dir.path().join("5d30f3c5.3"), generate_cert_pem()).unwrap();
        fs_err::write(dir.path().join("fd3003c5.0"), generate_cert_pem()).unwrap();

        let certs = Certificates::from_dir(dir.path()).unwrap();
        assert!(certs.iter().count() > 0);
    }

    #[test]
    fn test_from_dir_ignores_invalid_files() {
        let dir = tempfile::tempdir().unwrap();
        fs_err::write(dir.path().join("cert.txt"), "not a certificate").unwrap();
        fs_err::write(dir.path().join("cert"), "still not a certificate").unwrap();

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
