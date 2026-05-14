use std::borrow::Cow;
use std::sync::LazyLock;

use reqsign::aws::DefaultSigner as AwsDefaultSigner;
use reqsign::google::DefaultSigner as GcsDefaultSigner;
use tracing::debug;
use url::Url;

use uv_preview::{Preview, PreviewFeature};
use uv_static::EnvVars;
use uv_warnings::warn_user_once;

use crate::Credentials;
use crate::credentials::Token;
use crate::index::is_path_prefix;
use crate::realm::{Realm, RealmRef};

/// The [`Realm`] for the Hugging Face platform.
static HUGGING_FACE_REALM: LazyLock<Realm> = LazyLock::new(|| {
    let url = Url::parse("https://huggingface.co").expect("Failed to parse Hugging Face URL");
    Realm::from(&url)
});

/// The authentication token for the Hugging Face platform, if set.
static HUGGING_FACE_TOKEN: LazyLock<Option<Vec<u8>>> = LazyLock::new(|| {
    // Extract the Hugging Face token from the environment variable, if it exists.
    let hf_token = std::env::var(EnvVars::HF_TOKEN)
        .ok()
        .map(String::into_bytes)
        .filter(|token| !token.is_empty())?;

    if std::env::var_os(EnvVars::UV_NO_HF_TOKEN).is_some() {
        debug!("Ignoring Hugging Face token from environment due to `UV_NO_HF_TOKEN`");
        return None;
    }

    debug!("Found Hugging Face token in environment");
    Some(hf_token)
});

/// A provider for authentication credentials for the Hugging Face platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HuggingFaceProvider;

impl HuggingFaceProvider {
    /// Returns the credentials for the Hugging Face platform, if available.
    pub(crate) fn credentials_for(url: &Url) -> Option<Credentials> {
        if RealmRef::from(url) == *HUGGING_FACE_REALM {
            if let Some(token) = HUGGING_FACE_TOKEN.as_ref() {
                return Some(Credentials::Bearer {
                    token: Token::new(token.clone()),
                });
            }
        }
        None
    }
}

/// The [`Url`] for the S3 endpoint, if set.
static S3_ENDPOINT_URL: LazyLock<Option<Url>> = LazyLock::new(|| {
    let s3_endpoint_url = std::env::var(EnvVars::UV_S3_ENDPOINT_URL).ok()?;
    let url = Url::parse(&s3_endpoint_url).expect("Failed to parse S3 endpoint URL");
    Some(url)
});

/// A provider for authentication credentials for S3 endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct S3EndpointProvider;

impl S3EndpointProvider {
    /// Returns `true` if the URL matches the configured S3 endpoint.
    pub(crate) fn is_s3_endpoint(url: &Url, preview: Preview) -> bool {
        if let Some(s3_endpoint_url) = S3_ENDPOINT_URL.as_ref() {
            if !preview.is_enabled(PreviewFeature::S3Endpoint) {
                warn_user_once!(
                    "The `s3-endpoint` option is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
                    PreviewFeature::S3Endpoint
                );
            }

            // Treat any URL under the endpoint path on the same domain or subdomain as available
            // for S3 signing.
            if is_endpoint_url(url, s3_endpoint_url) {
                return true;
            }
        }
        false
    }

    /// Creates a new S3 signer with the configured region.
    ///
    /// This is potentially expensive as it may invoke credential helpers, so the result
    /// should be cached.
    pub(crate) fn create_signer() -> AwsDefaultSigner {
        // TODO(charlie): Can `reqsign` infer the region for us? Profiles, for example,
        // often have a region set already.
        let region = std::env::var(EnvVars::AWS_REGION)
            .map(Cow::Owned)
            .unwrap_or_else(|_| {
                std::env::var(EnvVars::AWS_DEFAULT_REGION)
                    .map(Cow::Owned)
                    .unwrap_or_else(|_| Cow::Borrowed("us-east-1"))
            });
        reqsign::aws::default_signer("s3", &region)
    }
}

/// The [`Url`] for the GCS endpoint, if set.
static GCS_ENDPOINT_URL: LazyLock<Option<Url>> = LazyLock::new(|| {
    let gcs_endpoint_url = std::env::var(EnvVars::UV_GCS_ENDPOINT_URL).ok()?;
    let url = Url::parse(&gcs_endpoint_url).expect("Failed to parse GCS endpoint URL");
    Some(url)
});

/// A provider for authentication credentials for GCS endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GcsEndpointProvider;

impl GcsEndpointProvider {
    /// Returns `true` if the URL matches the configured GCS endpoint.
    pub(crate) fn is_gcs_endpoint(url: &Url, preview: Preview) -> bool {
        if let Some(gcs_endpoint_url) = GCS_ENDPOINT_URL.as_ref() {
            if !preview.is_enabled(PreviewFeature::GcsEndpoint) {
                warn_user_once!(
                    "The `gcs-endpoint` option is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
                    PreviewFeature::GcsEndpoint
                );
            }

            // Treat any URL under the endpoint path on the same domain or subdomain as available
            // for GCS signing.
            if is_endpoint_url(url, gcs_endpoint_url) {
                return true;
            }
        }
        false
    }

    /// Creates a new GCS signer.
    ///
    /// This is potentially expensive as it may invoke credential helpers, so the result
    /// should be cached.
    pub(crate) fn create_signer() -> GcsDefaultSigner {
        reqsign::google::default_signer("storage.googleapis.com")
    }
}

/// Returns `true` if `url` is within the configured S3 or GCS-compatible endpoint URL.
///
/// The URL must be in the same realm, or a subdomain of the endpoint realm, and must be under the
/// endpoint path using complete path-segment prefix matching.
fn is_endpoint_url(url: &Url, endpoint_url: &Url) -> bool {
    let endpoint_realm = RealmRef::from(endpoint_url);
    let realm = RealmRef::from(url);
    if realm != endpoint_realm && !realm.is_subdomain_of(endpoint_realm) {
        return false;
    }

    is_path_prefix(endpoint_url.path(), url.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_url_matches_path_prefix() {
        let endpoint_url = Url::parse("https://example.com/private").unwrap();

        for url in [
            "https://example.com/private",
            "https://example.com/private/",
            "https://example.com/private/packages/anyio.whl",
        ] {
            assert!(
                is_endpoint_url(&Url::parse(url).unwrap(), &endpoint_url),
                "Failed to match endpoint URL prefix: {url}"
            );
        }
    }

    #[test]
    fn test_endpoint_url_rejects_partial_path_segments() {
        let endpoint_url = Url::parse("https://example.com/private").unwrap();

        for url in [
            "https://example.com/public",
            "https://example.com/private-bucket",
            "https://example.com/privatebucket",
        ] {
            assert!(
                !is_endpoint_url(&Url::parse(url).unwrap(), &endpoint_url),
                "Should not match URL outside endpoint path: {url}"
            );
        }
    }

    #[test]
    fn test_endpoint_url_matches_subdomain_with_path_prefix() {
        let endpoint_url = Url::parse("https://example.com/private").unwrap();

        assert!(is_endpoint_url(
            &Url::parse("https://bucket.example.com/private/package.whl").unwrap(),
            &endpoint_url
        ));
        assert!(!is_endpoint_url(
            &Url::parse("https://bucket.example.com/public/package.whl").unwrap(),
            &endpoint_url
        ));
    }

    #[test]
    fn test_endpoint_url_root_path_matches_all_paths() {
        let endpoint_url = Url::parse("https://example.com").unwrap();

        for url in [
            "https://example.com/package.whl",
            "https://bucket.example.com/package.whl",
        ] {
            assert!(
                is_endpoint_url(&Url::parse(url).unwrap(), &endpoint_url),
                "Failed to match URL under endpoint root: {url}"
            );
        }
    }
}
