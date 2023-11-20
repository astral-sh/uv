#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    PypiTypes(#[from] pypi_types::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error("Unable to read .dist-info directory for: {0}")]
    DistInfo(String, #[source] install_wheel_rs::Error),
    #[error("Unable to parse wheel filename for: {0}")]
    FilenameParse(String, #[source] anyhow::Error),
}
