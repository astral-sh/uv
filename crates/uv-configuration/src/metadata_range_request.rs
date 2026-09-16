/// The behavior when wheel metadata cannot be fetched with HTTP range requests.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
pub enum MetadataRangeRequest {
    /// Download the entire wheel to read the metadata.
    #[default]
    Fallback,
    /// Fail instead of downloading the entire wheel.
    Require,
}

impl From<bool> for MetadataRangeRequest {
    fn from(require: bool) -> Self {
        if require {
            Self::Require
        } else {
            Self::Fallback
        }
    }
}
