#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    PypiTypes(#[from] pypi_types::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
}
