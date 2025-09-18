use std::borrow::Cow;
use std::sync::LazyLock;

use tracing::debug;
use url::Url;

use uv_static::EnvVars;

use crate::Credentials;
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
                    token: token.clone(),
                });
            }
        }
        None
    }
}

/// The [`Url`] for the S3 endpoint, if set.
static S3_ENDPOINT_URL: LazyLock<Option<Url>> = LazyLock::new(|| {
    let s3_endpoint_url = std::env::var(EnvVars::UV_S3_ENDPOINT).ok()?;
    let url = Url::parse(&s3_endpoint_url).expect("Failed to parse S3 endpoint URL");
    Some(url)
});

/// A provider for authentication credentials for S3 endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct S3EndpointProvider;

impl S3EndpointProvider {
    /// Returns the credentials for the S3 endpoint, if available.
    pub(crate) fn credentials_for(url: &Url) -> Option<Credentials> {
        if let Some(s3_endpoint_url) = S3_ENDPOINT_URL.as_ref() {
            if url.scheme() == s3_endpoint_url.scheme()
                && url.port() == s3_endpoint_url.port()
                && url.domain().is_some_and(|subdomain| {
                    s3_endpoint_url.domain().is_some_and(|domain| {
                        subdomain == domain
                            || subdomain
                                .strip_suffix(domain)
                                .is_some_and(|prefix| prefix.ends_with('.'))
                    })
                })
            {
                let region = std::env::var(EnvVars::AWS_REGION)
                    .map(Cow::Owned)
                    .unwrap_or_else(|_| {
                        std::env::var("AWS_DEFAULT_REGION")
                            .map(Cow::Owned)
                            .unwrap_or_else(|_| Cow::Borrowed("us-east-1"))
                    });
                let signer = reqsign::aws::default_signer("s3", &region);
                return Some(Credentials::AwsSignatureV4 { signer });
            }
        }
        None
    }
}
