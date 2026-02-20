use indexmap::IndexMap;
use reqwest_middleware::ClientWithMiddleware;
use uv_redacted::DisplaySafeUrl;

use crate::{
    service::VulnerabilityService,
    types::{Dependency, Finding},
};

const API_BASE: &str = "https://api.osv.dev/";

/// Errors during OSV service interactions.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest_middleware::Error),
}

/// Represents [OSV](https://osv.dev/), an open-source vulnerability database.
pub struct Osv {
    base_url: DisplaySafeUrl,
    client: ClientWithMiddleware,
}

impl Default for Osv {
    fn default() -> Self {
        Self {
            base_url: DisplaySafeUrl::parse(API_BASE).expect("impossible: embedded URL is invalid"),
            client: Default::default(),
        }
    }
}

#[async_trait::async_trait]
impl VulnerabilityService for Osv {
    type Error = Error;

    async fn query<'a>(
        &self,
        dependency: &Dependency<'a>,
    ) -> Result<Vec<Finding<'a>>, Self::Error> {
        todo!()
    }

    async fn query_batch<'a>(
        &self,
        dependencies: &[Dependency<'a>],
    ) -> Result<IndexMap<Dependency<'a>, Vec<Finding<'a>>>, Self::Error> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::API_BASE;
    use super::Osv;

    /// Ensures that the default OSV client is configured with our default OSV API base URL.
    #[test]
    fn test_osv_default() {
        let osv = Osv::default();
        assert_eq!(osv.base_url.as_str(), API_BASE);
    }
}
