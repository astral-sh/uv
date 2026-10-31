use uv_distribution_types::{IndexUrl, RegistryBuiltWheel};
use uv_redacted::SafeUrl;

use crate::{Error, ErrorKind};

/// The artifact URL and effective index for a registry wheel metadata request.
///
/// Construct this through [`Self::for_wheel`] before fetching registry wheel metadata.
#[derive(Debug, Clone)]
pub(super) struct RegistryWheelRequest<'a> {
    url: SafeUrl,
    effective_index: &'a IndexUrl,
}

impl<'a> RegistryWheelRequest<'a> {
    /// Resolve the wheel's artifact URL and effective index together.
    pub(super) fn for_wheel(wheel: &'a RegistryBuiltWheel) -> Result<Self, Error> {
        Ok(Self {
            url: wheel.file.url.to_url().map_err(ErrorKind::InvalidUrl)?,
            effective_index: &wheel.index,
        })
    }

    /// Borrow the URL used for requests.
    pub(super) fn as_url(&self) -> &SafeUrl {
        &self.url
    }

    /// Consume the prepared URL when making a request.
    pub(super) fn into_url(self) -> SafeUrl {
        self.url
    }

    /// Consume the request into its URL and effective index.
    pub(super) fn into_parts(self) -> (SafeUrl, &'a IndexUrl) {
        (self.url, self.effective_index)
    }
}
