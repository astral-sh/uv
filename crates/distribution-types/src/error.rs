use pep508_rs::VerbatimUrl;
use url::Url;
use uv_normalize::PackageName;

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

    #[error("Unsupported scheme `{0}` on URL: {1} ({2})")]
    UnsupportedScheme(String, String, String),

    #[error("Requested package name `{0}` does not match `{1}` in the distribution filename: {2}")]
    PackageNameMismatch(PackageName, PackageName, String),

    // TODO(konsti): Show given if it exists
    #[error("Only directories can be installed as editable, not filenames: `{0}`")]
    EditableFile(VerbatimUrl),
}
