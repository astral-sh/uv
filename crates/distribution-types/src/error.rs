use url::Url;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    WheelFilename(#[from] distribution_filename::WheelFilenameError),

    #[error("Unable to extract filename from URL: {0}")]
    UrlFilename(Url),

    #[error("Distribution not found at: {0}")]
    NotFound(Url),
}
