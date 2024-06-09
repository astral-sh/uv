use reqwest::Identity;
use std::ffi::OsStr;
use std::io::Read;

#[derive(thiserror::Error, Debug)]
pub(crate) enum CertificateError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
}

/// Return the `Identity` from the provided file.
pub(crate) fn read_identity(ssl_client_cert: &OsStr) -> Result<Identity, CertificateError> {
    let mut buf = Vec::new();
    fs_err::File::open(ssl_client_cert)?.read_to_end(&mut buf)?;
    Ok(Identity::from_pem(&buf)?)
}
