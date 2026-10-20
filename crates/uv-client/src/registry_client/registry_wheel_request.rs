use http::HeaderValue;
use uv_distribution_types::{IndexLocations, IndexUrl, RegistryBuiltWheel};
use uv_redacted::SafeUrl;

use crate::{Error, ErrorKind};

/// A registry wheel request with metadata cache settings applied.
///
/// Construct this through [`Self::for_wheel`] before fetching registry wheel metadata.
#[derive(Debug, Clone)]
pub(super) struct RegistryWheelRequest<'a> {
    url: SafeUrl,
    effective_index: &'a IndexUrl,
    cache_control: Option<HeaderValue>,
}

impl<'a> RegistryWheelRequest<'a> {
    /// Resolve the wheel's artifact URL, effective index, and metadata cache override together.
    pub(super) fn for_wheel(
        wheel: &'a RegistryBuiltWheel,
        indexes: &'a IndexLocations,
    ) -> Result<Self, Error> {
        let url = wheel.file.url.to_url().map_err(ErrorKind::InvalidUrl)?;
        let effective_index = &wheel.index;
        let cache_control = indexes.artifact_cache_control_for(effective_index);
        Ok(Self {
            url,
            effective_index,
            cache_control,
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

    /// Consume the request into its URL, cache and capability index, and cache override.
    pub(super) fn into_parts(self) -> (SafeUrl, &'a IndexUrl, Option<HeaderValue>) {
        (self.url, self.effective_index, self.cache_control)
    }
}
