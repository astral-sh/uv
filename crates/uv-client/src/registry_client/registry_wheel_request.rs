use uv_distribution_types::{IndexLocations, IndexUrl, RegistryBuiltWheel};
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
    pub(super) fn for_wheel(
        wheel: &'a RegistryBuiltWheel,
        indexes: &'a IndexLocations,
    ) -> Result<Self, Error> {
        let (url, effective_index) = if let Some(route) = indexes.proxy_route_for(&wheel.index) {
            let url = route
                .artifact_url_for_request(&wheel.file.url)
                .map_err(ErrorKind::ProxyIndex)?;
            (url, route.effective_url())
        } else {
            let url = wheel.file.url.to_url().map_err(ErrorKind::InvalidUrl)?;
            (url, &wheel.index)
        };
        Ok(Self {
            url,
            effective_index,
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
