//! Services that implement PyPI's Trusted Publishing interfaces.

use reqwest_middleware::ClientWithMiddleware;
use tracing::{debug, trace};
use url::Url;
use uv_client::BaseClient;
use uv_redacted::DisplaySafeUrl;

use crate::trusted_publishing::{
    Audience, MintTokenRequest, PublishToken, TrustedPublishingError, TrustedPublishingService,
    decode_oidc_token,
};

pub(crate) struct PyPIPublishingService<'a> {
    client: &'a ClientWithMiddleware,
    registry: &'a DisplaySafeUrl,
}

impl<'a> PyPIPublishingService<'a> {
    pub(crate) fn new(registry: &'a DisplaySafeUrl, client: &'a BaseClient) -> Self {
        Self {
            client: client.for_host(registry).raw_client(),
            registry,
        }
    }
}

impl TrustedPublishingService for PyPIPublishingService<'_> {
    fn client(&self) -> &ClientWithMiddleware {
        self.client
    }

    async fn audience(&self) -> Result<String, super::TrustedPublishingError> {
        // `pypa/gh-action-pypi-publish` uses `netloc` (RFC 1808), which is deprecated for authority
        // (RFC 3986).
        // Prefer HTTPS for OIDC discovery; allow HTTP only in test builds
        let scheme: &str = if cfg!(feature = "test") {
            self.registry.scheme()
        } else {
            "https"
        };
        let audience_url = DisplaySafeUrl::parse(&format!(
            "{}://{}/_/oidc/audience",
            scheme,
            self.registry.authority()
        ))?;
        debug!("Querying the trusted publishing audience from {audience_url}");
        let response = self
            .client
            .get(Url::from(audience_url.clone()))
            .send()
            .await
            .map_err(|err| TrustedPublishingError::ReqwestMiddleware(audience_url.clone(), err))?;
        let audience = response
            .error_for_status()
            .map_err(|err| TrustedPublishingError::Reqwest(audience_url.clone(), err))?
            .json::<Audience>()
            .await
            .map_err(|err| TrustedPublishingError::Reqwest(audience_url.clone(), err))?;
        trace!("The audience is `{}`", &audience.audience);
        Ok(audience.audience)
    }

    async fn exchange_token(
        &self,
        oidc_token: ambient_id::IdToken,
    ) -> Result<super::TrustedPublishingToken, super::TrustedPublishingError> {
        // Prefer HTTPS for OIDC minting; allow HTTP only in test builds
        let scheme: &str = if cfg!(feature = "test") {
            self.registry.scheme()
        } else {
            "https"
        };
        let mint_token_url = DisplaySafeUrl::parse(&format!(
            "{}://{}/_/oidc/mint-token",
            scheme,
            self.registry.authority()
        ))?;
        debug!("Querying the trusted publishing upload token from {mint_token_url}");
        let mint_token_payload = MintTokenRequest {
            token: oidc_token.reveal().to_string(),
        };
        let response = self
            .client
            .post(Url::from(mint_token_url.clone()))
            .body(serde_json::to_vec(&mint_token_payload)?)
            .send()
            .await
            .map_err(|err| {
                TrustedPublishingError::ReqwestMiddleware(mint_token_url.clone(), err)
            })?;

        // reqwest's implementation of `.json()` also goes through `.bytes()`
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|err| TrustedPublishingError::Reqwest(mint_token_url.clone(), err))?;

        if status.is_success() {
            let publish_token: PublishToken = serde_json::from_slice(&body)?;
            Ok(publish_token.token)
        } else {
            match decode_oidc_token(oidc_token.reveal()) {
                Some(claims) => {
                    // An error here means that something is misconfigured, e.g. a typo in the PyPI
                    // configuration, so we're showing the body and the JWT claims for more context, see
                    // https://docs.pypi.org/trusted-publishers/troubleshooting/#token-minting
                    // for what the body can mean.
                    Err(TrustedPublishingError::TokenRejected(
                        status,
                        String::from_utf8_lossy(&body).to_string(),
                        claims,
                    ))
                }
                None => {
                    // This is not a user configuration error, the OIDC token should always have a valid
                    // format.
                    Err(TrustedPublishingError::InvalidOidcToken(
                        status,
                        String::from_utf8_lossy(&body).to_string(),
                    ))
                }
            }
        }
    }
}
