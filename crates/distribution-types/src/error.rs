use url::Url;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    WheelFilename(#[from] distribution_filename::WheelFilenameError),

    #[error("Unable to extract filename from URL: {0}")]
    UrlFilename(Url),

    #[error("Unable to locate distribution at: {0}")]
    NotFound(Url, #[source] std::io::Error),
}
