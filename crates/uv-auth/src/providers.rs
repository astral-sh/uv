use std::borrow::Cow;
use std::error::Error as _;
use std::path::PathBuf;
#[cfg(not(windows))]
use std::process::Stdio;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use http::header::AUTHORIZATION;
use reqsign::aws::DefaultSigner as AwsDefaultSigner;
use reqsign::azure::DefaultSigner as AzureDefaultSigner;
use reqsign::google::Credential as GoogleCredential;
use reqsign::google::DefaultSigner as GoogleDefaultSigner;
use reqsign::{Context, ProvideCredential};
#[cfg(not(windows))]
use serde::Deserialize;
#[cfg(not(windows))]
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::debug;
use url::Url;

use uv_preview::{Preview, PreviewFeature};
use uv_static::EnvVars;
use uv_warnings::warn_user_once;

use crate::Credentials;
use crate::credentials::Token;
use crate::index::is_path_prefix;
use crate::realm::{Realm, RealmRef};

/// The username expected by Google Artifact Registry when using an `OAuth2` access token.
const GOOGLE_ARTIFACT_REGISTRY_USERNAME: &str = "oauth2accesstoken";

/// The environment variable containing the path to explicit Google Application Default
/// Credentials.
const GOOGLE_APPLICATION_CREDENTIALS: &str = "GOOGLE_APPLICATION_CREDENTIALS";

/// The environment variable containing the path to the Google Cloud SDK configuration directory.
const GOOGLE_CLOUD_SDK_CONFIG: &str = "CLOUDSDK_CONFIG";

/// Refresh Google Artifact Registry credentials periodically, since access tokens are short-lived.
const GOOGLE_ARTIFACT_REGISTRY_CACHE_DURATION: Duration = Duration::from_mins(1);

/// Avoid waiting indefinitely for Application Default Credentials from the metadata server.
const GOOGLE_ARTIFACT_REGISTRY_ADC_TIMEOUT: Duration = Duration::from_secs(10);

/// Avoid waiting indefinitely for credentials from the `gcloud` CLI.
#[cfg(not(windows))]
const GOOGLE_ARTIFACT_REGISTRY_GCLOUD_TIMEOUT: Duration = Duration::from_secs(10);

/// A provider for authentication credentials for Google Artifact Registry.
#[derive(Clone, Debug)]
pub struct ArtifactRegistryProvider {
    signer: Option<GoogleDefaultSigner>,
    credentials: Arc<Mutex<Option<CachedArtifactRegistryCredentials>>>,
}

#[derive(Clone, Debug)]
struct CachedArtifactRegistryCredentials {
    credentials: Option<Credentials>,
    expires_at: Instant,
}

#[cfg(not(windows))]
#[derive(Debug, Deserialize)]
struct GcloudConfig {
    credential: Option<GcloudCredential>,
}

#[cfg(not(windows))]
#[derive(Debug, Deserialize)]
struct GcloudCredential {
    access_token: Option<String>,
    token_expiry: Option<String>,
}

/// The shared Google Artifact Registry provider.
static GOOGLE_ARTIFACT_REGISTRY_PROVIDER: LazyLock<ArtifactRegistryProvider> =
    LazyLock::new(|| ArtifactRegistryProvider {
        signer: None,
        credentials: Arc::new(Mutex::new(None)),
    });

/// The shared Google Artifact Registry signer.
static GOOGLE_ARTIFACT_REGISTRY_SIGNER: LazyLock<GoogleDefaultSigner> = LazyLock::new(|| {
    reqsign::google::default_signer("artifactregistry.googleapis.com")
        .with_credential_provider(ArtifactRegistryCredentialProvider)
});

/// A Google Application Default Credentials provider that preserves the documented lookup order.
///
/// Unlike the default `reqsign` provider, this provider does not fall through to another identity
/// when a configured credentials file exists but cannot be loaded.
#[derive(Clone, Copy, Debug)]
struct ArtifactRegistryCredentialProvider;

impl ProvideCredential for ArtifactRegistryCredentialProvider {
    type Credential = GoogleCredential;

    async fn provide_credential(
        &self,
        context: &Context,
    ) -> reqsign::Result<Option<Self::Credential>> {
        if let Some(path) = context
            .env_var(GOOGLE_APPLICATION_CREDENTIALS)
            .filter(|path| !path.is_empty())
        {
            return reqsign::google::FileCredentialProvider::new(path)
                .provide_credential(context)
                .await;
        }

        if let Some(path) = google_cloud_sdk_adc_path(context) {
            match context.file_read(&path).await {
                Ok(content) => {
                    return reqsign::google::StaticCredentialProvider::new(
                        String::from_utf8_lossy(&content).into_owned(),
                    )
                    .provide_credential(context)
                    .await;
                }
                Err(err) if error_is_not_found(&err) => {}
                Err(err) => return Err(err),
            }
        }

        reqsign::google::VmMetadataCredentialProvider::new()
            .provide_credential(context)
            .await
    }
}

fn google_cloud_sdk_adc_path(context: &Context) -> Option<String> {
    let config_dir = if let Some(path) = context
        .env_var(GOOGLE_CLOUD_SDK_CONFIG)
        .filter(|path| !path.is_empty())
    {
        PathBuf::from(path)
    } else if let Some(path) = context.env_var("APPDATA").filter(|path| !path.is_empty()) {
        PathBuf::from(path).join("gcloud")
    } else if let Some(path) = context
        .env_var("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
    {
        PathBuf::from(path).join("gcloud")
    } else if let Some(path) = context.env_var("HOME").filter(|path| !path.is_empty()) {
        PathBuf::from(path).join(".config").join("gcloud")
    } else {
        return None;
    };

    Some(
        config_dir
            .join("application_default_credentials.json")
            .to_string_lossy()
            .into_owned(),
    )
}

fn error_is_not_found(err: &reqsign::Error) -> bool {
    let mut source = err.source();
    while let Some(err) = source {
        if err
            .downcast_ref::<std::io::Error>()
            .is_some_and(|err| err.kind() == std::io::ErrorKind::NotFound)
        {
            return true;
        }
        source = err.source();
    }
    false
}

impl Default for ArtifactRegistryProvider {
    fn default() -> Self {
        GOOGLE_ARTIFACT_REGISTRY_PROVIDER.clone()
    }
}

impl ArtifactRegistryProvider {
    /// Returns `true` if the URL is for Google Artifact Registry.
    pub fn is_artifact_registry(url: &Url) -> bool {
        url.scheme() == "https"
            && url
                .host_str()
                .is_some_and(|host| host.ends_with(".pkg.dev"))
    }

    /// Returns `true` if the username is compatible with Google Artifact Registry credentials.
    pub(crate) fn supports_username(username: Option<&str>) -> bool {
        username.is_none_or(|username| username == GOOGLE_ARTIFACT_REGISTRY_USERNAME)
    }

    /// Returns credentials for Google Artifact Registry, if available.
    ///
    /// This follows the lookup order of Google's `keyrings.google-artifactregistry-auth` package:
    /// Application Default Credentials are preferred, then active `gcloud` credentials on Unix.
    pub(crate) async fn credentials_for(&self, url: &Url) -> Option<Credentials> {
        if !Self::is_artifact_registry(url) {
            return None;
        }

        let mut cached_credentials = self.credentials.lock().await;
        if let Some(credentials) = cached_credentials
            .as_ref()
            .filter(|credentials| credentials.expires_at > Instant::now())
        {
            return credentials.credentials.clone();
        }

        let explicit_adc =
            std::env::var_os(GOOGLE_APPLICATION_CREDENTIALS).is_some_and(|path| !path.is_empty());
        let (credentials, cache_duration) = if let Some(credentials) =
            self.credentials_from_adc(url).await
        {
            debug!(
                "Found Google Artifact Registry credentials from Application Default Credentials"
            );
            (Some(credentials), GOOGLE_ARTIFACT_REGISTRY_CACHE_DURATION)
        } else if explicit_adc {
            debug!(
                "Skipping Google Artifact Registry credentials from gcloud because explicit Application Default Credentials are configured"
            );
            (None, GOOGLE_ARTIFACT_REGISTRY_CACHE_DURATION)
        } else if let Some((credentials, cache_duration)) = Self::credentials_from_gcloud().await {
            debug!("Found Google Artifact Registry credentials from gcloud");
            (Some(credentials), cache_duration)
        } else {
            debug!("No Google Artifact Registry credentials found");
            (None, GOOGLE_ARTIFACT_REGISTRY_CACHE_DURATION)
        };

        *cached_credentials = Some(CachedArtifactRegistryCredentials {
            credentials: credentials.clone(),
            expires_at: Instant::now() + cache_duration,
        });

        credentials
    }

    /// Returns `true` if credentials are available for Google Artifact Registry.
    pub async fn has_credentials_for(&self, url: &Url) -> bool {
        self.credentials_for(url).await.is_some()
    }

    async fn credentials_from_adc(&self, url: &Url) -> Option<Credentials> {
        let request = http::Request::get(url.as_str())
            .body(())
            .inspect_err(|err| {
                debug!("Failed to build Google Artifact Registry credential request: {err}");
            })
            .ok()?;
        let (mut parts, ()) = request.into_parts();
        let Ok(result) = tokio::time::timeout(
            GOOGLE_ARTIFACT_REGISTRY_ADC_TIMEOUT,
            self.signer
                .as_ref()
                .unwrap_or(&GOOGLE_ARTIFACT_REGISTRY_SIGNER)
                .sign(&mut parts, None),
        )
        .await
        else {
            debug!("Timed out retrieving Google Artifact Registry Application Default Credentials");
            return None;
        };
        result
            .inspect_err(|err| {
                debug!(
                    "Failed to retrieve Google Artifact Registry Application Default Credentials: {err}"
                );
            })
            .ok()?;

        let token = parts
            .headers
            .get(AUTHORIZATION)?
            .to_str()
            .ok()?
            .strip_prefix("Bearer ")?;
        Self::credentials_from_token(token.to_string())
    }

    #[cfg(not(windows))]
    async fn credentials_from_gcloud() -> Option<(Credentials, Duration)> {
        let mut command = Command::new("gcloud");
        command
            .args(["config", "config-helper", "--format=json(credential)"])
            .stdin(Stdio::null())
            .kill_on_drop(true);
        let output =
            tokio::time::timeout(GOOGLE_ARTIFACT_REGISTRY_GCLOUD_TIMEOUT, command.output())
                .await
                .inspect_err(|_| {
                    debug!(
                        "Timed out retrieving Google Artifact Registry credentials from `gcloud`"
                    );
                })
                .ok()?
                .inspect_err(|err| {
                    debug!("Failed to run `gcloud config config-helper`: {err}");
                })
                .ok()?;
        if !output.status.success() {
            debug!(
                "`gcloud config config-helper` exited with status {}",
                output.status
            );
            return None;
        }

        Self::credentials_from_gcloud_output(&output.stdout)
    }

    #[cfg(windows)]
    fn credentials_from_gcloud() -> std::future::Ready<Option<(Credentials, Duration)>> {
        // The Google Cloud SDK launcher on Windows is a `.cmd` script, which requires shell
        // execution. Keep Application Default Credentials support, but skip this fallback for now.
        debug!("Skipping Google Artifact Registry credentials from `gcloud` on Windows");
        std::future::ready(None)
    }

    #[cfg(not(windows))]
    fn credentials_from_gcloud_output(output: &[u8]) -> Option<(Credentials, Duration)> {
        let config = serde_json::from_slice::<GcloudConfig>(output)
            .inspect_err(|err| {
                debug!("Failed to parse credentials from `gcloud config config-helper`: {err}");
            })
            .ok()?;
        let credential = config.credential?;
        let token_expiry = credential
            .token_expiry?
            .parse::<jiff::Timestamp>()
            .inspect_err(|err| {
                debug!("Failed to parse credentials from `gcloud config config-helper`: {err}");
            })
            .ok()?;
        let now = jiff::Timestamp::now();
        if token_expiry <= now {
            debug!("Ignoring expired credentials from `gcloud config config-helper`");
            return None;
        }
        let cache_duration = token_expiry
            .duration_since(now)
            .unsigned_abs()
            .min(GOOGLE_ARTIFACT_REGISTRY_CACHE_DURATION);
        Some((
            Self::credentials_from_token(credential.access_token?)?,
            cache_duration,
        ))
    }

    fn credentials_from_token(token: String) -> Option<Credentials> {
        if token.is_empty() {
            return None;
        }

        Some(Credentials::basic(
            Some(GOOGLE_ARTIFACT_REGISTRY_USERNAME.to_string()),
            Some(token),
        ))
    }

    #[cfg(test)]
    pub(crate) fn with_signer(signer: GoogleDefaultSigner) -> Self {
        Self {
            signer: Some(signer),
            credentials: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    pub(crate) async fn cache_missing_credentials(&self) {
        *self.credentials.lock().await = Some(CachedArtifactRegistryCredentials {
            credentials: None,
            expires_at: Instant::now() + GOOGLE_ARTIFACT_REGISTRY_CACHE_DURATION,
        });
    }

    #[cfg(test)]
    pub(crate) async fn clear_cached_credentials(&self) {
        *self.credentials.lock().await = None;
    }
}

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
    pub(crate) fn create_signer() -> GoogleDefaultSigner {
        reqsign::google::default_signer("storage.googleapis.com")
    }
}

/// The [`Url`] for the Azure endpoint, if set.
static AZURE_ENDPOINT_URL: LazyLock<Option<Url>> = LazyLock::new(|| {
    let azure_endpoint_url = std::env::var(EnvVars::UV_AZURE_ENDPOINT_URL).ok()?;
    let url = Url::parse(&azure_endpoint_url).expect("Failed to parse Azure endpoint URL");
    Some(url)
});

/// A provider for authentication credentials for Azure endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AzureEndpointProvider;

impl AzureEndpointProvider {
    /// Returns `true` if the URL matches the configured Azure endpoint.
    pub(crate) fn is_azure_endpoint(url: &Url, preview: Preview) -> bool {
        if let Some(azure_endpoint_url) = AZURE_ENDPOINT_URL.as_ref() {
            if !preview.is_enabled(PreviewFeature::AzureEndpoint) {
                warn_user_once!(
                    "The `azure-endpoint` option is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
                    PreviewFeature::AzureEndpoint
                );
            }

            // Treat any URL under the endpoint path on the same domain or subdomain as available
            // for Azure signing.
            if is_endpoint_url(url, azure_endpoint_url) {
                return true;
            }
        }
        false
    }

    /// Creates a new Azure signer using the default Azure credential chain.
    ///
    /// This is potentially expensive as it may invoke credential helpers, so the result
    /// should be cached.
    pub(crate) fn create_signer() -> AzureDefaultSigner {
        reqsign::azure::default_signer()
    }
}

/// Returns `true` if `url` is within the configured S3, GCS, or Azure-compatible endpoint URL.
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
    use std::collections::HashMap;

    use reqsign::{FileRead, StaticEnv};

    use super::*;

    #[derive(Clone, Debug, Default)]
    struct TestFileRead {
        files: Arc<HashMap<String, Vec<u8>>>,
    }

    impl TestFileRead {
        fn new(files: HashMap<String, Vec<u8>>) -> Self {
            Self {
                files: Arc::new(files),
            }
        }
    }

    impl FileRead for TestFileRead {
        async fn file_read(&self, path: &str) -> reqsign::Result<Vec<u8>> {
            self.files.get(path).cloned().ok_or_else(|| {
                reqsign::Error::unexpected("test credential file not found").with_source(
                    std::io::Error::new(std::io::ErrorKind::NotFound, "test credential file"),
                )
            })
        }
    }

    fn service_account_credentials() -> Vec<u8> {
        br#"{
            "type": "service_account",
            "private_key": "-----BEGIN RSA PRIVATE KEY-----\ntest\n-----END RSA PRIVATE KEY-----",
            "client_email": "test@example.iam.gserviceaccount.com"
        }"#
        .to_vec()
    }

    fn cloud_sdk_credentials_path() -> String {
        PathBuf::from("/cloud-sdk")
            .join("application_default_credentials.json")
            .to_string_lossy()
            .into_owned()
    }

    #[tokio::test]
    async fn test_artifact_registry_credentials_from_adc() {
        let provider = ArtifactRegistryProvider::with_signer(
            reqsign::google::default_signer("artifactregistry.googleapis.com")
                .with_credential_provider(reqsign::google::TokenCredentialProvider::new(
                    "test-token",
                )),
        );

        assert_eq!(
            provider
                .credentials_for(
                    &Url::parse("https://us-central1-python.pkg.dev/project/index/simple").unwrap()
                )
                .await,
            Some(Credentials::basic(
                Some("oauth2accesstoken".to_string()),
                Some("test-token".to_string())
            ))
        );
    }

    #[tokio::test]
    async fn test_artifact_registry_credentials_ignores_other_hosts() {
        let provider = ArtifactRegistryProvider::with_signer(
            reqsign::google::default_signer("artifactregistry.googleapis.com")
                .with_credential_provider(reqsign::google::TokenCredentialProvider::new(
                    "test-token",
                )),
        );

        assert_eq!(
            provider
                .credentials_for(&Url::parse("https://python.pkg.dev.example.com/simple").unwrap())
                .await,
            None
        );
        assert_eq!(
            provider
                .credentials_for(
                    &Url::parse("http://us-central1-python.pkg.dev/project/index/simple").unwrap()
                )
                .await,
            None
        );
    }

    #[tokio::test]
    async fn test_artifact_registry_credentials_caches_missing_credentials() {
        let provider = ArtifactRegistryProvider::with_signer(
            reqsign::google::default_signer("artifactregistry.googleapis.com")
                .with_credential_provider(reqsign::google::TokenCredentialProvider::new(
                    "test-token",
                )),
        );
        provider.cache_missing_credentials().await;

        assert_eq!(
            provider
                .credentials_for(
                    &Url::parse("https://us-central1-python.pkg.dev/project/index/simple").unwrap()
                )
                .await,
            None
        );
    }

    #[tokio::test]
    async fn test_artifact_registry_credentials_fail_closed_for_explicit_adc() {
        let context = Context::new()
            .with_env(StaticEnv {
                envs: HashMap::from([
                    (
                        GOOGLE_APPLICATION_CREDENTIALS.to_string(),
                        "/missing/credentials.json".to_string(),
                    ),
                    (
                        GOOGLE_CLOUD_SDK_CONFIG.to_string(),
                        "/cloud-sdk".to_string(),
                    ),
                ]),
                home_dir: None,
            })
            .with_file_read(TestFileRead::new(HashMap::from([(
                cloud_sdk_credentials_path(),
                service_account_credentials(),
            )])));

        assert!(
            ArtifactRegistryCredentialProvider
                .provide_credential(&context)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_artifact_registry_credentials_respect_cloud_sdk_config() {
        let context = Context::new()
            .with_env(StaticEnv {
                envs: HashMap::from([(
                    GOOGLE_CLOUD_SDK_CONFIG.to_string(),
                    "/cloud-sdk".to_string(),
                )]),
                home_dir: None,
            })
            .with_file_read(TestFileRead::new(HashMap::from([(
                cloud_sdk_credentials_path(),
                service_account_credentials(),
            )])));

        let credentials = ArtifactRegistryCredentialProvider
            .provide_credential(&context)
            .await
            .expect("Credentials should load")
            .expect("Credentials should exist");
        assert_eq!(
            credentials
                .service_account
                .expect("Credentials should contain service account")
                .client_email,
            "test@example.iam.gserviceaccount.com"
        );
    }

    #[test]
    fn test_artifact_registry_credentials_supports_username() {
        assert!(ArtifactRegistryProvider::supports_username(None));
        assert!(ArtifactRegistryProvider::supports_username(Some(
            "oauth2accesstoken"
        )));
        assert!(!ArtifactRegistryProvider::supports_username(Some("user")));
    }

    #[cfg(not(windows))]
    #[test]
    fn test_artifact_registry_credentials_from_gcloud_output() {
        assert_eq!(
            ArtifactRegistryProvider::credentials_from_gcloud_output(
                br#"{"credential":{"access_token":"test-token","token_expiry":"2099-05-29T00:00:00Z"}}"#
            ),
            Some((
                Credentials::basic(
                    Some("oauth2accesstoken".to_string()),
                    Some("test-token".to_string())
                ),
                GOOGLE_ARTIFACT_REGISTRY_CACHE_DURATION
            ))
        );
        assert_eq!(
            ArtifactRegistryProvider::credentials_from_gcloud_output(
                br#"{"credential":{"access_token":"test-token"}}"#
            ),
            None
        );
        assert_eq!(
            ArtifactRegistryProvider::credentials_from_gcloud_output(
                br#"{"credential":{"access_token":"test-token","token_expiry":"2000-05-29T00:00:00Z"}}"#
            ),
            None
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_artifact_registry_credentials_from_gcloud_unsupported_on_windows() {
        assert_eq!(
            ArtifactRegistryProvider::credentials_from_gcloud().await,
            None
        );
    }

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
