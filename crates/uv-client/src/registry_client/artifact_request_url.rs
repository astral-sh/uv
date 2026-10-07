use uv_distribution_types::{IndexLocations, RegistryBuiltWheel};
use uv_redacted::DisplaySafeUrl;

use crate::{Error, ErrorKind};

/// A registry wheel URL with any configured proxy mapping applied.
///
/// Construct this through [`Self::for_wheel`] before fetching registry wheel metadata.
#[derive(Debug, Clone)]
pub(super) struct ArtifactRequestUrl(DisplaySafeUrl);

impl ArtifactRequestUrl {
    /// Resolve the wheel's artifact location and apply its index's proxy mapping, if any.
    pub(super) fn for_wheel(
        wheel: &RegistryBuiltWheel,
        indexes: &IndexLocations,
    ) -> Result<Self, Error> {
        let url = if let Some(route) = indexes.proxy_route_for(&wheel.index) {
            route
                .artifact_url_for_request(&wheel.file.url)
                .map_err(ErrorKind::ProxyIndex)?
        } else {
            wheel.file.url.to_url().map_err(ErrorKind::InvalidUrl)?
        };
        Ok(Self(url))
    }

    /// Borrow the URL used for requests.
    pub(super) fn as_url(&self) -> &DisplaySafeUrl {
        &self.0
    }

    /// Consume the prepared URL when making a request.
    pub(super) fn into_url(self) -> DisplaySafeUrl {
        self.0
    }
}
