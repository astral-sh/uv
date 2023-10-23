use url::Url;

/// The index URLs to use for fetching packages.
#[derive(Debug, Clone)]
pub(crate) struct IndexUrls {
    pub(crate) index: Option<Url>,
    pub(crate) extra_index: Vec<Url>,
}

impl IndexUrls {
    /// Determine the index URLs to use for fetching packages.
    pub(crate) fn from_args(
        index: Option<Url>,
        extra_index: Vec<Url>,
        no_index: bool,
    ) -> Option<Self> {
        (!no_index).then_some(Self { index, extra_index })
    }
}
