mod trusted_publishing;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fmt, io};

use fs_err::tokio::File;
use futures::TryStreamExt;
use glob::{GlobError, PatternError, glob};
use itertools::Itertools;
use reqwest::header::{AUTHORIZATION, LOCATION, ToStrError};
use reqwest::multipart::Part;
use reqwest::{Body, Response, StatusCode};
use reqwest_retry::RetryError;
use reqwest_retry::policies::ExponentialBackoff;
use rustc_hash::FxHashMap;
use serde::Deserialize;
use thiserror::Error;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::sync::Semaphore;
use tokio_util::io::ReaderStream;
use tracing::{Level, debug, enabled, trace, warn};
use url::Url;

use uv_auth::{Credentials, PyxTokenStore, Realm};
use uv_cache::{Cache, Refresh};
use uv_client::{
    BaseClient, DEFAULT_MAX_REDIRECTS, MetadataFormat, OwnedArchive, RegistryClientBuilder,
    RequestBuilder, RetryParsingError, RetryState,
};
use uv_configuration::{KeyringProviderType, TrustedPublishing};
use uv_distribution_filename::{DistFilename, SourceDistExtension, SourceDistFilename};
use uv_distribution_types::{IndexCapabilities, IndexUrl};
use uv_extract::hash::{HashReader, Hasher};
use uv_fs::{ProgressReader, Simplified};
use uv_metadata::read_metadata_async_seek;
use uv_pypi_types::{HashAlgorithm, HashDigest, Metadata23, MetadataError};
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};
use uv_warnings::warn_user;

use crate::trusted_publishing::pypi::PyPIPublishingService;
use crate::trusted_publishing::pyx::PyxPublishingService;
use crate::trusted_publishing::{
    TrustedPublishingError, TrustedPublishingService, TrustedPublishingToken,
};

#[derive(Error, Debug)]
pub enum PublishError {
    #[error("The publish path is not a valid glob pattern: `{0}`")]
    Pattern(String, #[source] PatternError),
    /// [`GlobError`] is a wrapped io error.
    #[error(transparent)]
    Glob(#[from] GlobError),
    #[error("Path patterns didn't match any wheels or source distributions")]
    NoFiles,
    #[error(transparent)]
    Fmt(#[from] fmt::Error),
    #[error("File is neither a wheel nor a source distribution: `{}`", _0.user_display())]
    InvalidFilename(PathBuf),
    #[error("Failed to publish: `{}`", _0.user_display())]
    PublishPrepare(PathBuf, #[source] Box<PublishPrepareError>),
    #[error("Failed to publish `{}` to {}", _0.user_display(), _1)]
    PublishSend(
        PathBuf,
        Box<DisplaySafeUrl>,
        #[source] Box<PublishSendError>,
    ),
    #[error("Validation failed for `{}` on {}", _0.user_display(), _1)]
    Validate(
        PathBuf,
        Box<DisplaySafeUrl>,
        #[source] Box<PublishSendError>,
    ),
    #[error("Failed to obtain token for trusted publishing")]
    TrustedPublishing(#[from] Box<TrustedPublishingError>),
    #[error("{0} are not allowed when using trusted publishing")]
    MixedCredentials(String),
    #[error("Failed to query check URL")]
    CheckUrlIndex(#[source] uv_client::Error),
    #[error(
        "Local file and index file do not match for {filename}. \
        Local: {hash_algorithm}={local}, Remote: {hash_algorithm}={remote}"
    )]
    HashMismatch {
        filename: Box<DistFilename>,
        hash_algorithm: HashAlgorithm,
        local: String,
        remote: String,
    },
    #[error("Hash is missing in index for {0}")]
    MissingHash(Box<DistFilename>),
    #[error(transparent)]
    RetryParsing(#[from] RetryParsingError),
    #[error("Failed to reserve upload slot for `{}`", _0.user_display())]
    Reserve(PathBuf, #[source] Box<PublishSendError>),
    #[error("Failed to upload to S3 for `{}`", _0.user_display())]
    S3Upload(PathBuf, #[source] Box<PublishSendError>),
    #[error("Failed to finalize upload for `{}`", _0.user_display())]
    Finalize(PathBuf, #[source] Box<PublishSendError>),
}

/// Failure to get the metadata for a specific file.
#[derive(Error, Debug)]
pub enum PublishPrepareError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Failed to read metadata")]
    Metadata(#[from] uv_metadata::Error),
    #[error("Failed to read metadata")]
    Metadata23(#[from] MetadataError),
    #[error("Only files ending in `.tar.gz` are valid source distributions: `{0}`")]
    InvalidExtension(SourceDistFilename),
    #[error("No PKG-INFO file found")]
    MissingPkgInfo,
    #[error("Multiple PKG-INFO files found: `{0}`")]
    MultiplePkgInfo(String),
    #[error("Failed to read: `{0}`")]
    Read(String, #[source] io::Error),
    #[error("Invalid PEP 740 attestation (not JSON): `{0}`")]
    InvalidAttestation(PathBuf, #[source] serde_json::Error),
}

/// Failure in or after (HTTP) transport for a specific file.
#[derive(Error, Debug)]
pub enum PublishSendError {
    #[error("Failed to send POST request")]
    ReqwestMiddleware(#[source] reqwest_middleware::Error),
    #[error("Server returned status code {0}")]
    StatusNoBody(StatusCode, #[source] reqwest::Error),
    #[error("Server returned status code {0}. Server says: {1}")]
    Status(StatusCode, String),
    #[error("Server returned status code {0}. {1}")]
    StatusProblemDetails(StatusCode, String),
    #[error(
        "POST requests are not supported by the endpoint, are you using the simple index URL instead of the upload URL?"
    )]
    MethodNotAllowedNoBody,
    #[error(
        "POST requests are not supported by the endpoint, are you using the simple index URL instead of the upload URL? Server says: {0}"
    )]
    MethodNotAllowed(String),
    /// The registry returned a "403 Forbidden".
    #[error("Permission denied (status code {0}): {1}")]
    PermissionDenied(StatusCode, String),
    #[error("Too many redirects, only {0} redirects are allowed")]
    TooManyRedirects(u32),
    #[error("Redirected URL is not in the same realm. Redirected to: {0}")]
    RedirectRealmMismatch(String),
    #[error("Request was redirected, but no location header was provided")]
    RedirectNoLocation,
    #[error("Request was redirected, but location header is not a UTF-8 string")]
    RedirectLocationInvalidStr(#[source] ToStrError),
    #[error("Request was redirected, but location header is not a URL")]
    RedirectInvalidLocation(#[source] DisplaySafeUrlError),
}

pub trait Reporter: Send + Sync + 'static {
    fn on_progress(&self, name: &str, id: usize);
    fn on_upload_start(&self, name: &str, size: Option<u64>) -> usize;
    fn on_upload_progress(&self, id: usize, inc: u64);
    fn on_upload_complete(&self, id: usize);
}

/// Context for using a fresh registry client for check URL requests.
pub struct CheckUrlClient<'a> {
    pub index_url: IndexUrl,
    pub registry_client_builder: RegistryClientBuilder<'a>,
    pub client: &'a BaseClient,
    pub index_capabilities: IndexCapabilities,
    pub cache: &'a Cache,
}

impl PublishSendError {
    /// Extract `code` from the PyPI json error response, if any.
    ///
    /// The error response from PyPI contains crucial context, such as the difference between
    /// "Invalid or non-existent authentication information" and "The user 'konstin' isn't allowed
    /// to upload to project 'dummy'".
    ///
    /// Twine uses the HTTP status reason for its error messages. In HTTP 2.0 and onward this field
    /// is abolished, so reqwest doesn't expose it, see
    /// <https://docs.rs/reqwest/0.12.7/reqwest/struct.StatusCode.html#method.canonical_reason>.
    /// PyPI does respect the content type for error responses and can return an error display as
    /// HTML, JSON and plain. Since HTML and plain text are both overly verbose, we show the JSON
    /// response. Examples are shown below, line breaks were inserted for readability. Of those,
    /// the `code` seems to be the most helpful message, so we return it. If the response isn't a
    /// JSON document with `code` we return the regular body.
    ///
    /// ```json
    /// {"message": "The server could not comply with the request since it is either malformed or
    /// otherwise incorrect.\n\n\nError: Use 'source' as Python version for an sdist.\n\n",
    /// "code": "400 Error: Use 'source' as Python version for an sdist.",
    /// "title": "Bad Request"}
    /// ```
    ///
    /// ```json
    /// {"message": "Access was denied to this resource.\n\n\nInvalid or non-existent authentication
    /// information. See https://test.pypi.org/help/#invalid-auth for more information.\n\n",
    /// "code": "403 Invalid or non-existent authentication information. See
    /// https://test.pypi.org/help/#invalid-auth for more information.",
    /// "title": "Forbidden"}
    /// ```
    /// ```json
    /// {"message": "Access was denied to this resource.\n\n\n\n\n",
    /// "code": "403 Username/Password authentication is no longer supported. Migrate to API
    /// Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and
    /// https://test.pypi.org/help/#trusted-publishers",
    /// "title": "Forbidden"}
    /// ```
    ///
    /// For context, for the last case twine shows:
    /// ```text
    /// WARNING  Error during upload. Retry with the --verbose option for more details.
    /// ERROR    HTTPError: 403 Forbidden from https://test.pypi.org/legacy/
    ///          Username/Password authentication is no longer supported. Migrate to API
    ///          Tokens or Trusted Publishers instead. See
    ///          https://test.pypi.org/help/#apitoken and
    ///          https://test.pypi.org/help/#trusted-publishers
    /// ```
    ///
    /// ```text
    /// INFO     Response from https://test.pypi.org/legacy/:
    ///          403 Username/Password authentication is no longer supported. Migrate to
    ///          API Tokens or Trusted Publishers instead. See
    ///          https://test.pypi.org/help/#apitoken and
    ///          https://test.pypi.org/help/#trusted-publishers
    /// INFO     <html>
    ///           <head>
    ///            <title>403 Username/Password authentication is no longer supported.
    ///          Migrate to API Tokens or Trusted Publishers instead. See
    ///          https://test.pypi.org/help/#apitoken and
    ///          https://test.pypi.org/help/#trusted-publishers</title>
    ///           </head>
    ///          <body>
    ///           <h1>403 Username/Password authentication is no longer supported.
    ///         Migrate to API Tokens or Trusted Publishers instead. See
    ///          https://test.pypi.org/help/#apitoken and
    ///          https://test.pypi.org/help/#trusted-publishers</h1>
    ///            Access was denied to this resource.<br/><br/>
    /// ```
    ///
    /// In comparison, we now show (line-wrapped for readability):
    ///
    /// ```text
    /// error: Failed to publish `dist/astral_test_1-0.1.0-py3-none-any.whl` to `https://test.pypi.org/legacy/`
    ///   Caused by: Incorrect credentials (status code 403 Forbidden): 403 Username/Password
    ///     authentication is no longer supported. Migrate to API Tokens or Trusted Publishers
    ///     instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
    /// ```
    fn extract_error_message(body: String, content_type: Option<&str>) -> String {
        if content_type == Some("application/json") {
            #[derive(Deserialize)]
            struct ErrorBody {
                code: String,
            }

            if let Ok(structured) = serde_json::from_str::<ErrorBody>(&body) {
                structured.code
            } else {
                body
            }
        } else {
            body
        }
    }
}

/// Represents a single "to-be-uploaded" distribution, along with zero
/// or more attestations that will be uploaded alongside it.
#[derive(Debug)]
pub struct UploadDistribution {
    /// The path to the main distribution file to upload.
    pub file: PathBuf,
    /// The raw filename of the main distribution file.
    pub raw_filename: String,
    /// The parsed filename of the main distribution file.
    pub filename: DistFilename,
    /// Zero or more paths to PEP 740 attestations for the distribution.
    pub attestations: Vec<PathBuf>,
}

/// Given a list of paths (which may contain globs), unroll them into
/// a flat, unique list of files. Files are returned in a stable
/// but unspecified order.
fn unroll_paths(paths: Vec<String>) -> Result<Vec<PathBuf>, PublishError> {
    let mut files = BTreeSet::default();
    for path in paths {
        for file in glob(&path).map_err(|err| PublishError::Pattern(path.clone(), err))? {
            let file = file?;
            if !file.is_file() {
                continue;
            }

            files.insert(file);
        }
    }

    Ok(files.into_iter().collect())
}

/// Given a flat list of input files, merge them into a list of [`UploadDistribution`]s.
fn group_files(files: Vec<PathBuf>, no_attestations: bool) -> Vec<UploadDistribution> {
    let mut groups = FxHashMap::default();
    let mut attestations_by_dist = FxHashMap::default();
    for file in files {
        let Some(filename) = file
            .file_name()
            .and_then(|filename| filename.to_str())
            .map(ToString::to_string)
        else {
            continue;
        };

        // Attestations are named as `<dist>.<type>.attestation`, e.g.
        // `foo-1.2.3.tar.gz.publish.attestation`.
        // We use this to build up a map of `dist -> [attestations]`
        // for subsequent merging.
        let mut filename_parts = filename.rsplitn(3, '.');
        if filename_parts.next() == Some("attestation")
            && let Some(_) = filename_parts.next()
            && let Some(dist_name) = filename_parts.next()
        {
            debug!(
                "Found attestation for distribution: `{}` -> `{}`",
                file.user_display(),
                dist_name
            );

            attestations_by_dist
                .entry(dist_name.to_string())
                .or_insert_with(Vec::new)
                .push(file);
        } else {
            let Some(dist_filename) = DistFilename::try_from_normalized_filename(&filename) else {
                debug!("Not a distribution filename: `{filename}`");
                // I've never seen these in upper case
                #[expect(clippy::case_sensitive_file_extension_comparisons)]
                if filename.ends_with(".whl")
                    || filename.ends_with(".zip")
                    // Catch all compressed tar variants, e.g., `.tar.gz`
                    || filename
                        .split_once(".tar.")
                        .is_some_and(|(_, ext)| ext.chars().all(char::is_alphanumeric))
                {
                    warn_user!(
                        "Skipping file that looks like a distribution, \
                        but is not a valid distribution filename: `{}`",
                        file.user_display()
                    );
                }
                continue;
            };

            groups.insert(
                filename.clone(),
                UploadDistribution {
                    file,
                    raw_filename: filename,
                    filename: dist_filename,
                    attestations: Vec::new(),
                },
            );
        }
    }

    if no_attestations {
        debug!("Not merging attestations with distributions per user request");
    } else {
        // Merge attestations into their respective upload groups.
        for (dist_name, attestations) in attestations_by_dist {
            if let Some(group) = groups.get_mut(&dist_name) {
                group.attestations = attestations;
                group.attestations.sort();
            }
        }
    }

    groups.into_values().collect()
}

/// Collect the source distributions and wheels for publishing.
///
/// Returns an [`UploadGroup`] for each distribution to be published.
/// This group contains the path, the raw filename and the parsed filename. The raw filename is a fixup for
/// <https://github.com/astral-sh/uv/issues/8030> caused by
/// <https://github.com/pypa/setuptools/issues/3777> in combination with
/// <https://github.com/pypi/warehouse/blob/50a58f3081e693a3772c0283050a275e350004bf/warehouse/forklift/legacy.py#L1133-L1155>
pub fn group_files_for_publishing(
    paths: Vec<String>,
    no_attestations: bool,
) -> Result<Vec<UploadDistribution>, PublishError> {
    Ok(group_files(unroll_paths(paths)?, no_attestations))
}

pub enum TrustedPublishResult {
    /// We didn't check for trusted publishing.
    Skipped,
    /// We checked for trusted publishing and found a token.
    Configured(TrustedPublishingToken),
    /// We checked for optional trusted publishing, but it didn't succeed.
    Ignored(TrustedPublishingError),
}

/// If applicable, attempt obtaining a token for trusted publishing.
pub async fn check_trusted_publishing(
    username: Option<&str>,
    password: Option<&str>,
    keyring_provider: KeyringProviderType,
    token_store: &PyxTokenStore,
    trusted_publishing: TrustedPublishing,
    registry: &DisplaySafeUrl,
    client: &BaseClient,
) -> Result<TrustedPublishResult, PublishError> {
    match trusted_publishing {
        TrustedPublishing::Automatic => {
            // If the user provided credentials, use those.
            if username.is_some()
                || password.is_some()
                || keyring_provider != KeyringProviderType::Disabled
            {
                return Ok(TrustedPublishResult::Skipped);
            }

            debug!("Attempting to get a token for trusted publishing");

            // Attempt to get a token for trusted publishing.
            let token = if token_store.is_known_url(registry) {
                debug!("Using trusted publishing flow for pyx");
                PyxPublishingService::new(registry, client)
                    .get_token()
                    .await
            } else {
                debug!("Using trusted publishing flow for PyPI");
                PyPIPublishingService::new(registry, client)
                    .get_token()
                    .await
            };

            match token {
                // Success: we have a token for trusted publishing.
                Ok(Some(token)) => Ok(TrustedPublishResult::Configured(token)),
                // Failed to discover an ambient OIDC token.
                Ok(None) => Ok(TrustedPublishResult::Ignored(
                    TrustedPublishingError::NoToken,
                )),
                // Hard failure during OIDC discovery or token exchange.
                Err(err) => Ok(TrustedPublishResult::Ignored(err)),
            }
        }
        TrustedPublishing::Always => {
            debug!("Using trusted publishing for GitHub Actions");

            let mut conflicts = Vec::new();
            if username.is_some() {
                conflicts.push("a username");
            }
            if password.is_some() {
                conflicts.push("a password");
            }
            if keyring_provider != KeyringProviderType::Disabled {
                conflicts.push("the keyring");
            }
            if !conflicts.is_empty() {
                return Err(PublishError::MixedCredentials(conflicts.join(" and ")));
            }

            // Attempt to get a token for trusted publishing.
            let token = if token_store.is_known_url(registry) {
                debug!("Using trusted publishing flow for pyx");
                PyxPublishingService::new(registry, client)
                    .get_token()
                    .await
                    .map_err(Box::new)?
            } else {
                debug!("Using trusted publishing flow for PyPI");
                PyPIPublishingService::new(registry, client)
                    .get_token()
                    .await
                    .map_err(Box::new)?
            };

            let Some(token) = token else {
                return Err(PublishError::TrustedPublishing(
                    TrustedPublishingError::NoToken.into(),
                ));
            };

            Ok(TrustedPublishResult::Configured(token))
        }
        TrustedPublishing::Never => Ok(TrustedPublishResult::Skipped),
    }
}

/// Upload a file to a registry.
///
/// Returns `true` if the file was newly uploaded and `false` if it already existed.
///
/// Implements a custom retry flow since the request isn't cloneable.
pub async fn upload(
    group: &UploadDistribution,
    form_metadata: &FormMetadata,
    registry: &DisplaySafeUrl,
    client: &BaseClient,
    retry_policy: ExponentialBackoff,
    credentials: &Credentials,
    check_url_client: Option<&CheckUrlClient<'_>>,
    download_concurrency: &Semaphore,
    reporter: Arc<impl Reporter>,
) -> Result<bool, PublishError> {
    let mut n_past_redirections = 0;
    let max_redirects = DEFAULT_MAX_REDIRECTS;
    let mut current_registry = registry.clone();
    let mut retry_state = RetryState::start(retry_policy, registry.clone());

    loop {
        let (request, idx) = build_upload_request(
            group,
            &current_registry,
            client,
            credentials,
            form_metadata,
            reporter.clone(),
        )
        .await
        .map_err(|err| PublishError::PublishPrepare(group.file.clone(), Box::new(err)))?;

        let result = request.send().await;
        let response = match result {
            Ok(response) => {
                // When the user accidentally uses https://test.pypi.org/legacy (no slash) as publish URL, we
                // get a redirect to https://test.pypi.org/legacy/ (the canonical index URL).
                // In the above case we get 308, where reqwest or `RedirectClientWithMiddleware` would try
                // cloning the streaming body, which is not possible.
                // For https://test.pypi.org/simple (no slash), we get 301, which means we should make a GET request:
                // https://fetch.spec.whatwg.org/#http-redirect-fetch).
                // Reqwest doesn't support redirect policies conditional on the HTTP
                // method (https://github.com/seanmonstar/reqwest/issues/1777#issuecomment-2303386160), so we're
                // implementing our custom redirection logic.
                if response.status().is_redirection() {
                    if n_past_redirections >= max_redirects {
                        return Err(PublishError::PublishSend(
                            group.file.clone(),
                            current_registry.clone().into(),
                            PublishSendError::TooManyRedirects(n_past_redirections).into(),
                        ));
                    }
                    let location = response
                        .headers()
                        .get(LOCATION)
                        .ok_or_else(|| {
                            PublishError::PublishSend(
                                group.file.clone(),
                                current_registry.clone().into(),
                                PublishSendError::RedirectNoLocation.into(),
                            )
                        })?
                        .to_str()
                        .map_err(|err| {
                            PublishError::PublishSend(
                                group.file.clone(),
                                current_registry.clone().into(),
                                PublishSendError::RedirectLocationInvalidStr(err).into(),
                            )
                        })?;
                    current_registry = DisplaySafeUrl::parse(location).map_err(|err| {
                        PublishError::PublishSend(
                            group.file.clone(),
                            current_registry.clone().into(),
                            PublishSendError::RedirectInvalidLocation(err).into(),
                        )
                    })?;
                    if Realm::from(&current_registry) != Realm::from(registry) {
                        return Err(PublishError::PublishSend(
                            group.file.clone(),
                            current_registry.clone().into(),
                            PublishSendError::RedirectRealmMismatch(current_registry.to_string())
                                .into(),
                        ));
                    }
                    debug!("Redirecting the request to: {}", current_registry);
                    n_past_redirections += 1;
                    continue;
                }
                reporter.on_upload_complete(idx);
                response
            }
            Err(err) => {
                let middleware_retries = if let Some(RetryError::WithRetries { retries, .. }) =
                    (&err as &dyn std::error::Error).downcast_ref::<RetryError>()
                {
                    *retries
                } else {
                    0
                };
                if let Some(backoff) = retry_state.should_retry(&err, middleware_retries) {
                    retry_state.sleep_backoff(backoff).await;
                    continue;
                }
                return Err(PublishError::PublishSend(
                    group.file.clone(),
                    current_registry.clone().into(),
                    PublishSendError::ReqwestMiddleware(err).into(),
                ));
            }
        };

        return match handle_response(&current_registry, response).await {
            Ok(()) => {
                // Upload successful; for PyPI this can also mean a hash match in a raced upload
                // (but it doesn't tell us), for other registries it should mean a fresh upload.
                Ok(true)
            }
            Err(err) => {
                if matches!(
                    err,
                    PublishSendError::Status(..) | PublishSendError::StatusNoBody(..)
                ) {
                    if let Some(check_url_client) = &check_url_client {
                        if check_url(
                            check_url_client,
                            &group.file,
                            &group.filename,
                            download_concurrency,
                        )
                        .await?
                        {
                            // There was a raced upload of the same file, so even though our upload failed,
                            // the right file now exists in the registry.
                            return Ok(false);
                        }
                    }
                }
                Err(PublishError::PublishSend(
                    group.file.clone(),
                    current_registry.clone().into(),
                    err.into(),
                ))
            }
        };
    }
}

/// Validate a distribution before uploading.
///
/// Returns `true` if the file should be uploaded, `false` if it already exists on the server.
pub async fn validate(
    file: &Path,
    form_metadata: &FormMetadata,
    raw_filename: &str,
    registry: &DisplaySafeUrl,
    store: &PyxTokenStore,
    client: &BaseClient,
    credentials: &Credentials,
) -> Result<bool, PublishError> {
    if store.is_known_url(registry) {
        debug!("Performing validation request for {registry}");

        let mut validation_url = registry.clone();
        validation_url
            .path_segments_mut()
            .expect("URL must have path segments")
            .push("validate");

        let request = build_metadata_request(
            raw_filename,
            &validation_url,
            client,
            credentials,
            form_metadata,
        );

        let response = request.send().await.map_err(|err| {
            PublishError::Validate(
                file.to_path_buf(),
                registry.clone().into(),
                PublishSendError::ReqwestMiddleware(err).into(),
            )
        })?;

        let status_code = response.status();
        debug!("Response code for {validation_url}: {status_code}");

        if status_code.is_success() {
            #[derive(Deserialize)]
            struct ValidateResponse {
                exists: bool,
            }

            // Check if the file already exists.
            match response.text().await {
                Ok(body) => {
                    trace!("Response content for {validation_url}: {body}");
                    if let Ok(response) = serde_json::from_str::<ValidateResponse>(&body) {
                        if response.exists {
                            debug!("File already uploaded: {raw_filename}");
                            return Ok(false);
                        }
                    }
                }
                Err(err) => {
                    trace!("Failed to read response content for {validation_url}: {err}");
                }
            }
            return Ok(true);
        }

        // Handle error response.
        handle_response(&validation_url, response)
            .await
            .map_err(|err| {
                PublishError::Validate(file.to_path_buf(), registry.clone().into(), err.into())
            })?;

        Ok(true)
    } else {
        debug!("Skipping validation request for unsupported publish URL: {registry}");
        Ok(true)
    }
}

/// Upload a file using the two-phase upload protocol for pyx.
///
/// This is a more efficient upload method that:
/// 1. Reserves an upload slot and gets a pre-signed S3 URL.
/// 2. Uploads the file directly to S3.
/// 3. Finalizes the upload with the registry.
///
/// Returns `true` if the file was newly uploaded and `false` if it already existed.
pub async fn upload_two_phase(
    group: &UploadDistribution,
    form_metadata: &FormMetadata,
    registry: &DisplaySafeUrl,
    client: &BaseClient,
    s3_client: &BaseClient,
    retry_policy: ExponentialBackoff,
    credentials: &Credentials,
    reporter: Arc<impl Reporter>,
) -> Result<bool, PublishError> {
    #[derive(Debug, Deserialize)]
    struct ReserveResponse {
        upload_url: Option<String>,
        upload_headers: Option<FxHashMap<String, String>>,
    }

    // Step 1: Reserve an upload slot.
    let mut reserve_url = registry.clone();
    reserve_url
        .path_segments_mut()
        .expect("URL must have path segments")
        .push("reserve");

    debug!("Reserving upload slot at {reserve_url}");

    let reserve_request = build_metadata_request(
        &group.raw_filename,
        &reserve_url,
        client,
        credentials,
        form_metadata,
    );

    let response = reserve_request.send().await.map_err(|err| {
        PublishError::Reserve(
            group.file.clone(),
            PublishSendError::ReqwestMiddleware(err).into(),
        )
    })?;

    let status = response.status();

    let reserve_response: ReserveResponse = match status {
        StatusCode::OK => {
            debug!("File already uploaded: {}", group.raw_filename);
            return Ok(false);
        }
        StatusCode::CREATED => {
            let body = response.text().await.map_err(|err| {
                PublishError::Reserve(
                    group.file.clone(),
                    PublishSendError::StatusNoBody(status, err).into(),
                )
            })?;
            serde_json::from_str(&body).map_err(|_| {
                PublishError::Reserve(
                    group.file.clone(),
                    PublishSendError::Status(status, format!("Invalid JSON response: {body}"))
                        .into(),
                )
            })?
        }
        _ => {
            let body = response.text().await.unwrap_or_default();
            return Err(PublishError::Reserve(
                group.file.clone(),
                PublishSendError::Status(status, body).into(),
            ));
        }
    };

    // Step 2: Upload the file directly to S3 (if needed).
    // When upload_url is None, the file already exists on S3 with the correct hash.
    if let Some(upload_url) = reserve_response.upload_url {
        let s3_url = DisplaySafeUrl::parse(&upload_url).map_err(|_| {
            PublishError::S3Upload(
                group.file.clone(),
                PublishSendError::Status(
                    StatusCode::BAD_REQUEST,
                    "Invalid S3 URL in reserve response".to_string(),
                )
                .into(),
            )
        })?;

        debug!("Got pre-signed URL for upload: {s3_url}");

        // Use a custom retry loop since streaming uploads can't be retried by the middleware.
        let file_size = fs_err::tokio::metadata(&group.file)
            .await
            .map_err(|err| {
                PublishError::PublishPrepare(
                    group.file.clone(),
                    Box::new(PublishPrepareError::Io(err)),
                )
            })?
            .len();

        let mut retry_state = RetryState::start(retry_policy, s3_url.clone());
        loop {
            let file = File::open(&group.file).await.map_err(|err| {
                PublishError::PublishPrepare(
                    group.file.clone(),
                    Box::new(PublishPrepareError::Io(err)),
                )
            })?;

            let idx = reporter.on_upload_start(&group.filename.to_string(), Some(file_size));
            let reporter_clone = reporter.clone();
            let reader = ProgressReader::new(file, move |read| {
                reporter_clone.on_upload_progress(idx, read as u64);
            });
            let file_reader = Body::wrap_stream(ReaderStream::new(reader));

            let mut request = s3_client
                .for_host(&s3_url)
                .raw_client()
                .put(Url::from(s3_url.clone()))
                .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
                .header(reqwest::header::CONTENT_LENGTH, file_size);

            // Add any required headers from the reserve response (e.g., x-amz-tagging).
            if let Some(headers) = &reserve_response.upload_headers {
                for (key, value) in headers {
                    request = request.header(key, value);
                }
            }

            let result = request.body(file_reader).send().await;

            let response = match result {
                Ok(response) => {
                    reporter.on_upload_complete(idx);
                    response
                }
                Err(err) => {
                    let middleware_retries =
                        if let Some(RetryError::WithRetries { retries, .. }) =
                            (&err as &dyn std::error::Error).downcast_ref::<RetryError>()
                        {
                            *retries
                        } else {
                            0
                        };
                    if let Some(backoff) = retry_state.should_retry(&err, middleware_retries) {
                        retry_state.sleep_backoff(backoff).await;
                        continue;
                    }
                    return Err(PublishError::S3Upload(
                        group.file.clone(),
                        PublishSendError::ReqwestMiddleware(err).into(),
                    ));
                }
            };

            if response.status().is_success() {
                break;
            }

            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(PublishError::S3Upload(
                group.file.clone(),
                PublishSendError::Status(status, format!("S3 upload failed: {body}")).into(),
            ));
        }

        debug!("S3 upload complete for {}", group.raw_filename);
    } else {
        debug!(
            "File already exists on S3, skipping upload: {}",
            group.raw_filename
        );
    }

    // Step 3: Finalize the upload.
    let mut finalize_url = registry.clone();
    finalize_url
        .path_segments_mut()
        .expect("URL must have path segments")
        .push("finalize");

    debug!("Finalizing upload at {finalize_url}");

    let finalize_request = build_metadata_request(
        &group.raw_filename,
        &finalize_url,
        client,
        credentials,
        form_metadata,
    );

    let response = finalize_request.send().await.map_err(|err| {
        PublishError::Finalize(
            group.file.clone(),
            PublishSendError::ReqwestMiddleware(err).into(),
        )
    })?;

    handle_response(&finalize_url, response)
        .await
        .map_err(|err| PublishError::Finalize(group.file.clone(), err.into()))?;

    debug!("Upload finalized for {}", group.raw_filename);

    Ok(true)
}

/// Check whether we should skip the upload of a file because it already exists on the index.
pub async fn check_url(
    check_url_client: &CheckUrlClient<'_>,
    file: &Path,
    filename: &DistFilename,
    download_concurrency: &Semaphore,
) -> Result<bool, PublishError> {
    let CheckUrlClient {
        index_url,
        registry_client_builder,
        client,
        index_capabilities,
        cache,
    } = check_url_client;

    // Avoid using the PyPI 10min default cache.
    let cache_refresh = (*cache)
        .clone()
        .with_refresh(Refresh::from_args(None, vec![filename.name().clone()]));
    let registry_client = registry_client_builder
        .clone()
        .cache(cache_refresh)
        .wrap_existing(client);

    debug!("Checking for {filename} in the registry");
    let response = match registry_client
        .simple_detail(
            filename.name(),
            Some(index_url.into()),
            index_capabilities,
            download_concurrency,
        )
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return match err.kind() {
                uv_client::ErrorKind::RemotePackageNotFound(_) => {
                    // The package doesn't exist, so we can't have uploaded it.
                    warn!(
                        "Package not found in the registry; skipping upload check for {filename}"
                    );
                    Ok(false)
                }
                _ => Err(PublishError::CheckUrlIndex(err)),
            };
        }
    };
    let [(_, MetadataFormat::Simple(simple_metadata))] = response.as_slice() else {
        unreachable!("We queried a single index, we must get a single response");
    };
    let simple_metadata = OwnedArchive::deserialize(simple_metadata);
    let Some(metadatum) = simple_metadata
        .iter()
        .find(|metadatum| &metadatum.version == filename.version())
    else {
        return Ok(false);
    };

    let archived_file = match filename {
        DistFilename::SourceDistFilename(source_dist) => metadatum
            .files
            .source_dists
            .iter()
            .find(|entry| &entry.name == source_dist)
            .map(|entry| &entry.file),
        DistFilename::WheelFilename(wheel) => metadatum
            .files
            .wheels
            .iter()
            .find(|entry| &entry.name == wheel)
            .map(|entry| &entry.file),
    };
    let Some(archived_file) = archived_file else {
        return Ok(false);
    };

    // TODO(konsti): Do we have a preference for a hash here?
    if let Some(remote_hash) = archived_file.hashes.first() {
        // We accept the risk for TOCTOU errors here, since we already read the file once before the
        // streaming upload to compute the hash for the form metadata.
        let local_hash = &hash_file(file, vec![Hasher::from(remote_hash.algorithm)])
            .await
            .map_err(|err| {
                PublishError::PublishPrepare(
                    file.to_path_buf(),
                    Box::new(PublishPrepareError::Io(err)),
                )
            })?[0];
        if local_hash.digest == remote_hash.digest {
            debug!(
                "Found {filename} in the registry with matching hash {}",
                remote_hash.digest
            );
            Ok(true)
        } else {
            Err(PublishError::HashMismatch {
                filename: Box::new(filename.clone()),
                hash_algorithm: remote_hash.algorithm,
                local: local_hash.digest.to_string(),
                remote: remote_hash.digest.to_string(),
            })
        }
    } else {
        Err(PublishError::MissingHash(Box::new(filename.clone())))
    }
}

/// Calculate the requested hashes of a file.
async fn hash_file(
    path: impl AsRef<Path>,
    hashers: Vec<Hasher>,
) -> Result<Vec<HashDigest>, io::Error> {
    debug!("Hashing {}", path.as_ref().display());
    let file = BufReader::new(File::open(path.as_ref()).await?);
    let mut hashers = hashers;
    HashReader::new(file, &mut hashers).finish().await?;

    Ok(hashers
        .into_iter()
        .map(HashDigest::from)
        .collect::<Vec<_>>())
}

// Not in `uv-metadata` because we only support tar files here.
async fn source_dist_pkg_info(file: &Path) -> Result<Vec<u8>, PublishPrepareError> {
    let reader = BufReader::new(File::open(&file).await?);
    let decoded = async_compression::tokio::bufread::GzipDecoder::new(reader);
    let mut archive = tokio_tar::Archive::new(decoded);
    let mut pkg_infos: Vec<(PathBuf, Vec<u8>)> = archive
        .entries()?
        .map_err(PublishPrepareError::from)
        .try_filter_map(async |mut entry| {
            let path = entry
                .path()
                .map_err(PublishPrepareError::from)?
                .to_path_buf();
            let mut components = path.components();
            let Some(_top_level) = components.next() else {
                return Ok(None);
            };
            let Some(pkg_info) = components.next() else {
                return Ok(None);
            };
            if components.next().is_some() || pkg_info.as_os_str() != "PKG-INFO" {
                return Ok(None);
            }
            let mut buffer = Vec::new();
            // We have to read while iterating or the entry is empty as we're beyond it in the file.
            entry.read_to_end(&mut buffer).await.map_err(|err| {
                PublishPrepareError::Read(path.to_string_lossy().to_string(), err)
            })?;
            Ok(Some((path, buffer)))
        })
        .try_collect()
        .await?;
    match pkg_infos.len() {
        0 => Err(PublishPrepareError::MissingPkgInfo),
        1 => Ok(pkg_infos.remove(0).1),
        _ => Err(PublishPrepareError::MultiplePkgInfo(
            pkg_infos
                .iter()
                .map(|(path, _buffer)| path.to_string_lossy())
                .join(", "),
        )),
    }
}

async fn metadata(file: &Path, filename: &DistFilename) -> Result<Metadata23, PublishPrepareError> {
    let contents = match filename {
        DistFilename::SourceDistFilename(source_dist) => {
            if source_dist.extension != SourceDistExtension::TarGz {
                // See PEP 625. While we support installing legacy source distributions, we don't
                // support creating and uploading them.
                return Err(PublishPrepareError::InvalidExtension(source_dist.clone()));
            }
            source_dist_pkg_info(file).await?
        }
        DistFilename::WheelFilename(wheel) => {
            let reader = BufReader::new(File::open(&file).await?);
            read_metadata_async_seek(wheel, reader).await?
        }
    };
    Ok(Metadata23::parse(&contents)?)
}

#[derive(Debug, Clone)]
pub struct FormMetadata(Vec<(&'static str, String)>);

impl FormMetadata {
    /// Collect the non-file fields for the multipart request from the package METADATA.
    ///
    /// Reference implementation: <https://github.com/pypi/warehouse/blob/d2c36d992cf9168e0518201d998b2707a3ef1e72/warehouse/forklift/legacy.py#L1376-L1430>
    pub async fn read_from_file(
        file: &Path,
        filename: &DistFilename,
    ) -> Result<Self, PublishPrepareError> {
        let hashes = hash_file(
            file,
            vec![
                Hasher::from(HashAlgorithm::Sha256),
                Hasher::from(HashAlgorithm::Blake2b),
            ],
        )
        .await?;

        let sha256_hash = hashes
            .iter()
            .find(|hash| hash.algorithm == HashAlgorithm::Sha256)
            .unwrap();

        let blake2b_hash = hashes
            .iter()
            .find(|hash| hash.algorithm == HashAlgorithm::Blake2b)
            .unwrap();

        let Metadata23 {
            metadata_version,
            name,
            version,
            platforms,
            // Not used by PyPI legacy upload
            supported_platforms: _,
            summary,
            description,
            description_content_type,
            keywords,
            home_page,
            download_url,
            author,
            author_email,
            maintainer,
            maintainer_email,
            license,
            license_expression,
            license_files,
            classifiers,
            requires_dist,
            provides_dist,
            obsoletes_dist,
            requires_python,
            requires_external,
            project_urls,
            provides_extra,
            dynamic,
        } = metadata(file, filename).await?;

        let mut form_metadata = vec![
            (":action", "file_upload".to_string()),
            ("sha256_digest", sha256_hash.digest.to_string()),
            ("blake2_256_digest", blake2b_hash.digest.to_string()),
            ("protocol_version", "1".to_string()),
            ("metadata_version", metadata_version.clone()),
            // Twine transforms the name with `re.sub("[^A-Za-z0-9.]+", "-", name)`
            // * <https://github.com/pypa/twine/issues/743>
            // * <https://github.com/pypa/twine/blob/5bf3f38ff3d8b2de47b7baa7b652c697d7a64776/twine/package.py#L57-L65>
            // warehouse seems to call `packaging.utils.canonicalize_name` nowadays and has a separate
            // `normalized_name`, so we'll start with this and we'll readjust if there are user reports.
            ("name", name.clone()),
            ("version", version.clone()),
            ("filetype", filename.filetype().to_string()),
        ];

        if let DistFilename::WheelFilename(wheel) = filename {
            form_metadata.push(("pyversion", wheel.python_tags().iter().join(".")));
        } else {
            form_metadata.push(("pyversion", "source".to_string()));
        }

        let mut add_option = |name, value: Option<String>| {
            if let Some(some) = value.clone() {
                form_metadata.push((name, some));
            }
        };

        add_option("author", author);
        add_option("author_email", author_email);
        add_option("description", description);
        add_option("description_content_type", description_content_type);
        add_option("download_url", download_url);
        add_option("home_page", home_page);
        add_option("keywords", keywords.map(|keywords| keywords.as_metadata()));
        add_option("license", license);
        add_option("license_expression", license_expression);
        add_option("maintainer", maintainer);
        add_option("maintainer_email", maintainer_email);
        add_option("summary", summary);

        // The GitLab PyPI repository API implementation requires this metadata field and twine always
        // includes it in the request, even when it's empty.
        form_metadata.push(("requires_python", requires_python.unwrap_or(String::new())));

        let mut add_vec = |name, values: Vec<String>| {
            for i in values {
                form_metadata.push((name, i.clone()));
            }
        };

        add_vec("classifiers", classifiers);
        add_vec("dynamic", dynamic);
        add_vec("license_file", license_files);
        add_vec("obsoletes_dist", obsoletes_dist);
        add_vec("platform", platforms);
        add_vec("project_urls", project_urls.to_vec_str());
        add_vec("provides_dist", provides_dist);
        add_vec("provides_extra", provides_extra);
        add_vec("requires_dist", requires_dist);
        add_vec("requires_external", requires_external);

        Ok(Self(form_metadata))
    }

    /// Returns an iterator over the metadata fields.
    fn iter(&self) -> std::slice::Iter<'_, (&'static str, String)> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a FormMetadata {
    type Item = &'a (&'a str, String);
    type IntoIter = std::slice::Iter<'a, (&'a str, String)>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Build the upload request.
///
/// Returns the [`RequestBuilder`] and the reporter progress bar ID.
async fn build_upload_request<'a>(
    group: &UploadDistribution,
    registry: &DisplaySafeUrl,
    client: &'a BaseClient,
    credentials: &Credentials,
    form_metadata: &FormMetadata,
    reporter: Arc<impl Reporter>,
) -> Result<(RequestBuilder<'a>, usize), PublishPrepareError> {
    let mut form = reqwest::multipart::Form::new();
    for (key, value) in form_metadata.iter() {
        form = form.text(*key, value.clone());
    }

    let file = File::open(&group.file).await?;
    let file_size = file.metadata().await?.len();
    let idx = reporter.on_upload_start(&group.filename.to_string(), Some(file_size));
    let reader = ProgressReader::new(file, move |read| {
        reporter.on_upload_progress(idx, read as u64);
    });
    // Stream wrapping puts a static lifetime requirement on the reader (so the request doesn't have
    // a lifetime) -> callback needs to be static -> reporter reference needs to be Arc'd.
    let file_reader = Body::wrap_stream(ReaderStream::new(reader));
    // See [`files_for_publishing`] on `raw_filename`
    let part =
        Part::stream_with_length(file_reader, file_size).file_name(group.raw_filename.clone());
    form = form.part("content", part);

    let mut attestations = vec![];
    for attestation_path in &group.attestations {
        let contents = fs_err::read_to_string(attestation_path)?;
        // NOTE: We don't currently validate the interior structure of an attestation beyond being
        // valid JSON. We could validate it pretty easily in the future.
        let raw_attestation = serde_json::from_str::<serde_json::Value>(&contents)
            .map_err(|err| PublishPrepareError::InvalidAttestation(attestation_path.into(), err))?;
        attestations.push(raw_attestation);
    }

    if !attestations.is_empty() {
        // PEP 740 specifies the `attestations` field as a JSON array of attestation objects.
        let attestations_json =
            serde_json::to_string(&attestations).expect("Round-trip of PEP 740 attestation failed");
        form = form.text("attestations", attestations_json);
    }

    // If we have a username but no password, attach the username to the URL so the authentication
    // middleware can find the matching password.
    let url = if let Some(username) = credentials
        .username()
        .filter(|_| credentials.password().is_none())
    {
        let mut url = registry.clone();
        let _ = url.set_username(username);
        url
    } else {
        registry.clone()
    };

    let mut request = client
        .for_host(&url)
        .post(Url::from(url))
        .multipart(form)
        // Ask PyPI for a structured error messages instead of HTML-markup error messages.
        // For other registries, we ask them to return plain text over HTML. See
        // [`PublishSendError::extract_remote_error`].
        .header(
            reqwest::header::ACCEPT,
            "application/json;q=0.9, text/plain;q=0.8, text/html;q=0.7",
        );

    match credentials {
        Credentials::Basic { password, .. } => {
            if password.is_some() {
                debug!("Using HTTP Basic authentication");
                request = request.header(AUTHORIZATION, credentials.to_header_value());
            }
        }
        Credentials::Bearer { .. } => {
            debug!("Using Bearer token authentication");
            request = request.header(AUTHORIZATION, credentials.to_header_value());
        }
    }

    Ok((request, idx))
}

/// Build a request with form metadata but without the file content.
fn build_metadata_request<'a>(
    raw_filename: &str,
    registry: &DisplaySafeUrl,
    client: &'a BaseClient,
    credentials: &Credentials,
    form_metadata: &FormMetadata,
) -> RequestBuilder<'a> {
    let mut form = reqwest::multipart::Form::new();
    for (key, value) in form_metadata.iter() {
        form = form.text(*key, value.clone());
    }
    form = form.text("filename", raw_filename.to_owned());

    // If we have a username but no password, attach the username to the URL so the authentication
    // middleware can find the matching password.
    let url = if let Some(username) = credentials
        .username()
        .filter(|_| credentials.password().is_none())
    {
        let mut url = registry.clone();
        let _ = url.set_username(username);
        url
    } else {
        registry.clone()
    };

    let mut request = client
        .for_host(&url)
        .post(Url::from(url))
        .multipart(form)
        // Ask PyPI for a structured error messages instead of HTML-markup error messages.
        // For other registries, we ask them to return plain text over HTML. See
        // [`PublishSendError::extract_remote_error`].
        .header(
            reqwest::header::ACCEPT,
            "application/json;q=0.9, text/plain;q=0.8, text/html;q=0.7",
        );

    match credentials {
        Credentials::Basic { password, .. } => {
            if password.is_some() {
                debug!("Using HTTP Basic authentication");
                request = request.header(AUTHORIZATION, credentials.to_header_value());
            }
        }
        Credentials::Bearer { .. } => {
            debug!("Using Bearer token authentication");
            request = request.header(AUTHORIZATION, credentials.to_header_value());
        }
    }

    request
}

/// Log response information and map response to an error variant if not successful.
async fn handle_response(
    registry: &DisplaySafeUrl,
    response: Response,
) -> Result<(), PublishSendError> {
    let status_code = response.status();
    debug!("Response code for {registry}: {status_code}");
    trace!("Response headers for {registry}: {response:?}");

    if status_code.is_success() {
        if enabled!(Level::TRACE) {
            match response.text().await {
                Ok(response_content) => {
                    trace!("Response content for {registry}: {response_content}");
                }
                Err(err) => {
                    trace!("Failed to read response content for {registry}: {err}");
                }
            }
        }
        return Ok(());
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|content_type| content_type.to_str().ok())
        .map(ToString::to_string);
    let upload_error = response.bytes().await.map_err(|err| {
        if status_code == StatusCode::METHOD_NOT_ALLOWED {
            PublishSendError::MethodNotAllowedNoBody
        } else {
            PublishSendError::StatusNoBody(status_code, err)
        }
    })?;
    let upload_error = String::from_utf8_lossy(&upload_error);

    trace!("Response content for non-200 response for {registry}: {upload_error}");

    debug!("Upload error response: {upload_error}");

    // That's most likely the simple index URL, not the upload URL.
    if status_code == StatusCode::METHOD_NOT_ALLOWED {
        return Err(PublishSendError::MethodNotAllowed(
            PublishSendError::extract_error_message(
                upload_error.to_string(),
                content_type.as_deref(),
            ),
        ));
    }

    // Try to parse as RFC 9457 Problem Details (e.g., from pyx).
    if content_type.as_deref() == Some(uv_client::ProblemDetails::CONTENT_TYPE)
        && let Some(problem) =
            uv_client::ProblemDetails::try_from_response_body(upload_error.as_bytes())
        && let Some(description) = problem.description()
    {
        return Err(PublishSendError::StatusProblemDetails(
            status_code,
            description,
        ));
    }

    // Raced uploads of the same file are handled by the caller.
    Err(PublishSendError::Status(
        status_code,
        PublishSendError::extract_error_message(upload_error.to_string(), content_type.as_deref()),
    ))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use insta::{allow_duplicates, assert_debug_snapshot, assert_snapshot};
    use itertools::Itertools;
    use uv_auth::Credentials;
    use uv_client::{AuthIntegration, BaseClientBuilder, RedirectPolicy};
    use uv_distribution_filename::DistFilename;
    use uv_redacted::DisplaySafeUrl;

    use crate::{
        FormMetadata, PublishError, Reporter, UploadDistribution, build_upload_request,
        group_files, upload,
    };
    use tokio::sync::Semaphore;
    use uv_warnings::owo_colors::AnsiColors;
    use uv_warnings::write_error_chain;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct DummyReporter;

    impl Reporter for DummyReporter {
        fn on_progress(&self, _name: &str, _id: usize) {}
        fn on_upload_start(&self, _name: &str, _size: Option<u64>) -> usize {
            0
        }
        fn on_upload_progress(&self, _id: usize, _inc: u64) {}
        fn on_upload_complete(&self, _id: usize) {}
    }

    async fn mock_server_upload(mock_server: &MockServer) -> Result<bool, PublishError> {
        let raw_filename = "tqdm-4.66.1-py3-none-manylinux_2_12_x86_64.manylinux2010_x86_64.musllinux_1_1_x86_64.whl";
        let file = PathBuf::from("../../test/links/").join(raw_filename);
        let filename = DistFilename::try_from_normalized_filename(raw_filename).unwrap();

        let group = UploadDistribution {
            file,
            raw_filename: raw_filename.to_string(),
            filename,
            attestations: vec![],
        };

        let form_metadata = FormMetadata::read_from_file(&group.file, &group.filename)
            .await
            .unwrap();

        let client = BaseClientBuilder::default()
            .redirect(RedirectPolicy::NoRedirect)
            .retries(0)
            .auth_integration(AuthIntegration::NoAuthMiddleware)
            .build();

        let download_concurrency = Arc::new(Semaphore::new(1));
        let registry = DisplaySafeUrl::parse(&format!("{}/final", &mock_server.uri())).unwrap();
        upload(
            &group,
            &form_metadata,
            &registry,
            &client,
            client.retry_policy(),
            &Credentials::basic(Some("ferris".to_string()), Some("F3RR!S".to_string())),
            None,
            &download_concurrency,
            Arc::new(DummyReporter),
        )
        .await
    }

    #[test]
    fn test_group_files() {
        // Fisher-Yates shuffle.
        fn shuffle<T>(vec: &mut [T]) {
            let n: usize = vec.len();
            for i in 0..(n - 1) {
                let j = (fastrand::usize(..)) % (n - i) + i;
                vec.swap(i, j);
            }
        }

        let valid_sdist = "dist/acme-1.2.3.tar.gz";
        let valid_sdist_publish_attestation = format!("{valid_sdist}.publish.attestation");
        let valid_sdist_build_attestation = format!("{valid_sdist}.build.attestation");
        let valid_sdist_frob_attestation = format!("{valid_sdist}.frob.attestation");

        let valid_wheel = "dist/acme-1.2.3-py3-none-any.whl";
        let valid_wheel_publish_attestation = format!("{valid_wheel}.publish.attestation");
        let valid_wheel_build_attestation = format!("{valid_wheel}.build.attestation");
        let valid_wheel_frob_attestation = format!("{valid_wheel}.frob.attestation");

        let invalid_sdist = "dist/nudnik.tar.gz";
        let invalid_wheel = "dist/nudnik.whl";
        let valid_sdist_invalid_attestation = format!("{valid_sdist}.attestation");
        let invalid_attestation = "dist/nudnik.attestation";

        // Valid sdists/wheels without attestations
        {
            let dists = [valid_sdist, valid_wheel];

            let mut groups = group_files(dists.iter().map(PathBuf::from).collect(), false);
            groups.sort_by_key(|group| group.raw_filename.clone());

            assert_debug_snapshot!(groups, @r#"
            [
                UploadDistribution {
                    file: "dist/acme-1.2.3-py3-none-any.whl",
                    raw_filename: "acme-1.2.3-py3-none-any.whl",
                    filename: WheelFilename(
                        WheelFilename {
                            name: PackageName(
                                "acme",
                            ),
                            version: "1.2.3",
                            tags: Small {
                                small: WheelTagSmall {
                                    python_tag: Python {
                                        major: 3,
                                        minor: None,
                                    },
                                    abi_tag: None,
                                    platform_tag: Any,
                                },
                            },
                        },
                    ),
                    attestations: [],
                },
                UploadDistribution {
                    file: "dist/acme-1.2.3.tar.gz",
                    raw_filename: "acme-1.2.3.tar.gz",
                    filename: SourceDistFilename(
                        SourceDistFilename {
                            name: PackageName(
                                "acme",
                            ),
                            version: "1.2.3",
                            extension: TarGz,
                        },
                    ),
                    attestations: [],
                },
            ]
            "#);
        }

        // Valid sdists/wheels with attestations in various orders.
        {
            let mut dists = vec![
                valid_sdist,
                &valid_sdist_publish_attestation,
                &valid_sdist_build_attestation,
                &valid_sdist_frob_attestation,
                valid_wheel,
                &valid_wheel_build_attestation,
                &valid_wheel_publish_attestation,
                &valid_wheel_frob_attestation,
            ];

            allow_duplicates! {
                for _ in 0..5 {
                    shuffle(&mut dists);

                    let mut groups =
                        group_files(dists.iter().map(PathBuf::from).collect(), false);
                    groups.sort_by_key(|group| group.raw_filename.clone());

                    assert_debug_snapshot!(groups, @r#"
                    [
                        UploadDistribution {
                            file: "dist/acme-1.2.3-py3-none-any.whl",
                            raw_filename: "acme-1.2.3-py3-none-any.whl",
                            filename: WheelFilename(
                                WheelFilename {
                                    name: PackageName(
                                        "acme",
                                    ),
                                    version: "1.2.3",
                                    tags: Small {
                                        small: WheelTagSmall {
                                            python_tag: Python {
                                                major: 3,
                                                minor: None,
                                            },
                                            abi_tag: None,
                                            platform_tag: Any,
                                        },
                                    },
                                },
                            ),
                            attestations: [
                                "dist/acme-1.2.3-py3-none-any.whl.build.attestation",
                                "dist/acme-1.2.3-py3-none-any.whl.frob.attestation",
                                "dist/acme-1.2.3-py3-none-any.whl.publish.attestation",
                            ],
                        },
                        UploadDistribution {
                            file: "dist/acme-1.2.3.tar.gz",
                            raw_filename: "acme-1.2.3.tar.gz",
                            filename: SourceDistFilename(
                                SourceDistFilename {
                                    name: PackageName(
                                        "acme",
                                    ),
                                    version: "1.2.3",
                                    extension: TarGz,
                                },
                            ),
                            attestations: [
                                "dist/acme-1.2.3.tar.gz.build.attestation",
                                "dist/acme-1.2.3.tar.gz.frob.attestation",
                                "dist/acme-1.2.3.tar.gz.publish.attestation",
                            ],
                        },
                    ]
                    "#);
                }
            }
        }

        // Valid sdists/wheels with attestations in various orders, but
        // attestations are disabled while grouping.
        {
            let mut dists = vec![
                valid_sdist,
                &valid_sdist_publish_attestation,
                &valid_sdist_build_attestation,
                &valid_sdist_frob_attestation,
                valid_wheel,
                &valid_wheel_build_attestation,
                &valid_wheel_publish_attestation,
                &valid_wheel_frob_attestation,
            ];

            allow_duplicates! {
                for _ in 0..5 {
                    shuffle(&mut dists);

                    let mut groups =
                        group_files(dists.iter().map(PathBuf::from).collect(), true);
                    groups.sort_by_key(|group| group.raw_filename.clone());

                    assert_debug_snapshot!(groups, @r#"
                    [
                        UploadDistribution {
                            file: "dist/acme-1.2.3-py3-none-any.whl",
                            raw_filename: "acme-1.2.3-py3-none-any.whl",
                            filename: WheelFilename(
                                WheelFilename {
                                    name: PackageName(
                                        "acme",
                                    ),
                                    version: "1.2.3",
                                    tags: Small {
                                        small: WheelTagSmall {
                                            python_tag: Python {
                                                major: 3,
                                                minor: None,
                                            },
                                            abi_tag: None,
                                            platform_tag: Any,
                                        },
                                    },
                                },
                            ),
                            attestations: [],
                        },
                        UploadDistribution {
                            file: "dist/acme-1.2.3.tar.gz",
                            raw_filename: "acme-1.2.3.tar.gz",
                            filename: SourceDistFilename(
                                SourceDistFilename {
                                    name: PackageName(
                                        "acme",
                                    ),
                                    version: "1.2.3",
                                    extension: TarGz,
                                },
                            ),
                            attestations: [],
                        },
                    ]
                    "#);
                }
            }
        }

        // Invalid dist/attestation filenames get ignored.
        {
            let dists = [
                valid_sdist,
                &valid_sdist_frob_attestation,
                valid_wheel,
                &valid_wheel_build_attestation,
                invalid_sdist,
                invalid_wheel,
                &valid_sdist_invalid_attestation,
                invalid_attestation,
            ];

            let groups = group_files(dists.iter().map(PathBuf::from).collect(), false);
            assert_debug_snapshot!(groups, @r#"
            [
                UploadDistribution {
                    file: "dist/acme-1.2.3-py3-none-any.whl",
                    raw_filename: "acme-1.2.3-py3-none-any.whl",
                    filename: WheelFilename(
                        WheelFilename {
                            name: PackageName(
                                "acme",
                            ),
                            version: "1.2.3",
                            tags: Small {
                                small: WheelTagSmall {
                                    python_tag: Python {
                                        major: 3,
                                        minor: None,
                                    },
                                    abi_tag: None,
                                    platform_tag: Any,
                                },
                            },
                        },
                    ),
                    attestations: [
                        "dist/acme-1.2.3-py3-none-any.whl.build.attestation",
                    ],
                },
                UploadDistribution {
                    file: "dist/acme-1.2.3.tar.gz",
                    raw_filename: "acme-1.2.3.tar.gz",
                    filename: SourceDistFilename(
                        SourceDistFilename {
                            name: PackageName(
                                "acme",
                            ),
                            version: "1.2.3",
                            extension: TarGz,
                        },
                    ),
                    attestations: [
                        "dist/acme-1.2.3.tar.gz.frob.attestation",
                    ],
                },
            ]
            "#);
        }
    }

    /// Snapshot the data we send for an upload request for a source distribution.
    #[tokio::test]
    async fn upload_request_source_dist() {
        let group = {
            let raw_filename = "tqdm-999.0.0.tar.gz";
            let file = PathBuf::from("../../test/links/").join(raw_filename);
            let filename = DistFilename::try_from_normalized_filename(raw_filename).unwrap();

            UploadDistribution {
                file,
                raw_filename: raw_filename.to_string(),
                filename,
                attestations: vec![],
            }
        };

        let form_metadata = FormMetadata::read_from_file(&group.file, &group.filename)
            .await
            .unwrap();

        let formatted_metadata = form_metadata
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .join("\n");
        assert_snapshot!(&formatted_metadata, @"
        :action: file_upload
        sha256_digest: 89fa05cffa7f457658373b85de302d24d0c205ceda2819a8739e324b75e9430b
        blake2_256_digest: 40ab79b48c4e289e4990f7e689177adae4096c07a634034eb1d10c0b6700e4d2
        protocol_version: 1
        metadata_version: 2.3
        name: tqdm
        version: 999.0.0
        filetype: sdist
        pyversion: source
        author_email: Charlie Marsh <charlie.r.marsh@gmail.com>
        description: # tqdm

        [![PyPI - Version](https://img.shields.io/pypi/v/tqdm.svg)](https://pypi.org/project/tqdm)
        [![PyPI - Python Version](https://img.shields.io/pypi/pyversions/tqdm.svg)](https://pypi.org/project/tqdm)

        -----

        **Table of Contents**

        - [Installation](#installation)
        - [License](#license)

        ## Installation

        ```console
        pip install tqdm
        ```

        ## License

        `tqdm` is distributed under the terms of the [MIT](https://spdx.org/licenses/MIT.html) license.

        description_content_type: text/markdown
        license_expression: MIT
        requires_python: >=3.8
        classifiers: Development Status :: 4 - Beta
        classifiers: Programming Language :: Python
        classifiers: Programming Language :: Python :: 3.8
        classifiers: Programming Language :: Python :: 3.9
        classifiers: Programming Language :: Python :: 3.10
        classifiers: Programming Language :: Python :: 3.11
        classifiers: Programming Language :: Python :: 3.12
        classifiers: Programming Language :: Python :: Implementation :: CPython
        classifiers: Programming Language :: Python :: Implementation :: PyPy
        license_file: LICENSE.txt
        project_urls: Documentation, https://github.com/unknown/tqdm#readme
        project_urls: Issues, https://github.com/unknown/tqdm/issues
        project_urls: Source, https://github.com/unknown/tqdm
        ");

        let client = BaseClientBuilder::default().build();
        let (request, _) = build_upload_request(
            &group,
            &DisplaySafeUrl::parse("https://example.org/upload").unwrap(),
            &client,
            &Credentials::basic(Some("ferris".to_string()), Some("F3RR!S".to_string())),
            &form_metadata,
            Arc::new(DummyReporter),
        )
        .await
        .unwrap();

        insta::with_settings!({
            filters => [("boundary=[0-9a-f-]+", "boundary=[...]")],
        }, {
            assert_debug_snapshot!(&request.raw_builder(), @r#"
            RequestBuilder {
                inner: RequestBuilder {
                    method: POST,
                    url: Url {
                        scheme: "https",
                        cannot_be_a_base: false,
                        username: "",
                        password: None,
                        host: Some(
                            Domain(
                                "example.org",
                            ),
                        ),
                        port: None,
                        path: "/upload",
                        query: None,
                        fragment: None,
                    },
                    headers: {
                        "content-type": "multipart/form-data; boundary=[...]",
                        "content-length": "7000",
                        "accept": "application/json;q=0.9, text/plain;q=0.8, text/html;q=0.7",
                        "authorization": Sensitive,
                    },
                },
                ..
            }
            "#);
        });
    }

    /// Snapshot the data we send for an upload request for a wheel.
    #[tokio::test]
    async fn upload_request_wheel() {
        let group = {
            let raw_filename = "tqdm-4.66.1-py3-none-manylinux_2_12_x86_64.manylinux2010_x86_64.musllinux_1_1_x86_64.whl";
            let file = PathBuf::from("../../test/links/").join(raw_filename);
            let filename = DistFilename::try_from_normalized_filename(raw_filename).unwrap();

            UploadDistribution {
                file,
                raw_filename: raw_filename.to_string(),
                filename,
                attestations: vec![],
            }
        };

        let form_metadata = FormMetadata::read_from_file(&group.file, &group.filename)
            .await
            .unwrap();

        let formatted_metadata = form_metadata
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .join("\n");
        assert_snapshot!(&formatted_metadata, @r#"
        :action: file_upload
        sha256_digest: 0d88ca657bc6b64995ca416e0c59c71af85cc10015d940fa446c42a8b485ee1c
        blake2_256_digest: 33d4e92517a16e3fa0c0893de0c7e4d46a2c38adab148dd2ff66eb47481d19cd
        protocol_version: 1
        metadata_version: 2.1
        name: tqdm
        version: 4.66.1
        filetype: bdist_wheel
        pyversion: py3
        description_content_type: text/x-rst
        keywords: progressbar,progressmeter,progress,bar,meter,rate,eta,console,terminal,time
        license: MPL-2.0 AND MIT
        maintainer_email: tqdm developers <devs@tqdm.ml>
        summary: Fast, Extensible Progress Meter
        requires_python: >=3.7
        classifiers: Development Status :: 5 - Production/Stable
        classifiers: Environment :: Console
        classifiers: Environment :: MacOS X
        classifiers: Environment :: Other Environment
        classifiers: Environment :: Win32 (MS Windows)
        classifiers: Environment :: X11 Applications
        classifiers: Framework :: IPython
        classifiers: Framework :: Jupyter
        classifiers: Intended Audience :: Developers
        classifiers: Intended Audience :: Education
        classifiers: Intended Audience :: End Users/Desktop
        classifiers: Intended Audience :: Other Audience
        classifiers: Intended Audience :: System Administrators
        classifiers: License :: OSI Approved :: MIT License
        classifiers: License :: OSI Approved :: Mozilla Public License 2.0 (MPL 2.0)
        classifiers: Operating System :: MacOS
        classifiers: Operating System :: MacOS :: MacOS X
        classifiers: Operating System :: Microsoft
        classifiers: Operating System :: Microsoft :: MS-DOS
        classifiers: Operating System :: Microsoft :: Windows
        classifiers: Operating System :: POSIX
        classifiers: Operating System :: POSIX :: BSD
        classifiers: Operating System :: POSIX :: BSD :: FreeBSD
        classifiers: Operating System :: POSIX :: Linux
        classifiers: Operating System :: POSIX :: SunOS/Solaris
        classifiers: Operating System :: Unix
        classifiers: Programming Language :: Python
        classifiers: Programming Language :: Python :: 3
        classifiers: Programming Language :: Python :: 3.7
        classifiers: Programming Language :: Python :: 3.8
        classifiers: Programming Language :: Python :: 3.9
        classifiers: Programming Language :: Python :: 3.10
        classifiers: Programming Language :: Python :: 3.11
        classifiers: Programming Language :: Python :: 3 :: Only
        classifiers: Programming Language :: Python :: Implementation
        classifiers: Programming Language :: Python :: Implementation :: IronPython
        classifiers: Programming Language :: Python :: Implementation :: PyPy
        classifiers: Programming Language :: Unix Shell
        classifiers: Topic :: Desktop Environment
        classifiers: Topic :: Education :: Computer Aided Instruction (CAI)
        classifiers: Topic :: Education :: Testing
        classifiers: Topic :: Office/Business
        classifiers: Topic :: Other/Nonlisted Topic
        classifiers: Topic :: Software Development :: Build Tools
        classifiers: Topic :: Software Development :: Libraries
        classifiers: Topic :: Software Development :: Libraries :: Python Modules
        classifiers: Topic :: Software Development :: Pre-processors
        classifiers: Topic :: Software Development :: User Interfaces
        classifiers: Topic :: System :: Installation/Setup
        classifiers: Topic :: System :: Logging
        classifiers: Topic :: System :: Monitoring
        classifiers: Topic :: System :: Shells
        classifiers: Topic :: Terminals
        classifiers: Topic :: Utilities
        license_file: LICENCE
        project_urls: homepage, https://tqdm.github.io
        project_urls: repository, https://github.com/tqdm/tqdm
        project_urls: changelog, https://tqdm.github.io/releases
        project_urls: wiki, https://github.com/tqdm/tqdm/wiki
        provides_extra: dev
        provides_extra: notebook
        provides_extra: slack
        provides_extra: telegram
        requires_dist: colorama ; platform_system == "Windows"
        requires_dist: pytest >=6 ; extra == 'dev'
        requires_dist: pytest-cov ; extra == 'dev'
        requires_dist: pytest-timeout ; extra == 'dev'
        requires_dist: pytest-xdist ; extra == 'dev'
        requires_dist: ipywidgets >=6 ; extra == 'notebook'
        requires_dist: slack-sdk ; extra == 'slack'
        requires_dist: requests ; extra == 'telegram'
        "#);

        let client = BaseClientBuilder::default().build();
        let (request, _) = build_upload_request(
            &group,
            &DisplaySafeUrl::parse("https://example.org/upload").unwrap(),
            &client,
            &Credentials::basic(Some("ferris".to_string()), Some("F3RR!S".to_string())),
            &form_metadata,
            Arc::new(DummyReporter),
        )
        .await
        .unwrap();

        insta::with_settings!({
            filters => [("boundary=[0-9a-f-]+", "boundary=[...]")],
        }, {
            assert_debug_snapshot!(&request.raw_builder(), @r#"
            RequestBuilder {
                inner: RequestBuilder {
                    method: POST,
                    url: Url {
                        scheme: "https",
                        cannot_be_a_base: false,
                        username: "",
                        password: None,
                        host: Some(
                            Domain(
                                "example.org",
                            ),
                        ),
                        port: None,
                        path: "/upload",
                        query: None,
                        fragment: None,
                    },
                    headers: {
                        "content-type": "multipart/form-data; boundary=[...]",
                        "content-length": "19527",
                        "accept": "application/json;q=0.9, text/plain;q=0.8, text/html;q=0.7",
                        "authorization": Sensitive,
                    },
                },
                ..
            }
            "#);
        });
    }

    #[tokio::test]
    async fn upload_redirect_308() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/final"))
            .respond_with(
                ResponseTemplate::new(308)
                    .insert_header("Location", format!("{}/final/", &mock_server.uri())),
            )
            .mount(&mock_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/final/"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        assert!(mock_server_upload(&mock_server).await.unwrap());
    }

    #[tokio::test]
    async fn upload_infinite_redirects() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/final"))
            .respond_with(
                ResponseTemplate::new(308)
                    .insert_header("Location", format!("{}/final/", &mock_server.uri())),
            )
            .mount(&mock_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/final/"))
            .respond_with(
                ResponseTemplate::new(308)
                    .insert_header("Location", format!("{}/final", &mock_server.uri())),
            )
            .mount(&mock_server)
            .await;

        let err = mock_server_upload(&mock_server).await.unwrap_err();

        let mut capture = String::new();
        write_error_chain(&err, &mut capture, "error", AnsiColors::Red).unwrap();

        let capture = capture.replace(&mock_server.uri(), "[SERVER]");
        let capture = anstream::adapter::strip_str(&capture);
        assert_snapshot!(
            &capture,
            @"
        error: Failed to publish `../../test/links/tqdm-4.66.1-py3-none-manylinux_2_12_x86_64.manylinux2010_x86_64.musllinux_1_1_x86_64.whl` to [SERVER]/final
          Caused by: Too many redirects, only 10 redirects are allowed
        "
        );
    }

    #[tokio::test]
    async fn upload_redirect_different_realm() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/final"))
            .respond_with(
                ResponseTemplate::new(308)
                    .insert_header("Location", "https://different.auth.tld/final/"),
            )
            .mount(&mock_server)
            .await;

        let err = mock_server_upload(&mock_server).await.unwrap_err();

        let mut capture = String::new();
        write_error_chain(&err, &mut capture, "error", AnsiColors::Red).unwrap();

        let capture = capture.replace(&mock_server.uri(), "[SERVER]");
        let capture = anstream::adapter::strip_str(&capture);
        assert_snapshot!(
            &capture,
            @"
        error: Failed to publish `../../test/links/tqdm-4.66.1-py3-none-manylinux_2_12_x86_64.manylinux2010_x86_64.musllinux_1_1_x86_64.whl` to https://different.auth.tld/final/
          Caused by: Redirected URL is not in the same realm. Redirected to: https://different.auth.tld/final/
        "
        );
    }

    /// PyPI returns `application/json` with a `code` field.
    #[tokio::test]
    async fn upload_error_pypi_json() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/final"))
            .respond_with(
                ResponseTemplate::new(400)
                    .insert_header("content-type", "application/json")
                    .set_body_raw(
                        r#"{"message": "The server could not comply with the request since it is either malformed or otherwise incorrect.\n\n\nError: Use 'source' as Python version for an sdist.\n\n", "code": "400 Error: Use 'source' as Python version for an sdist.", "title": "Bad Request"}"#,
                        "application/json",
                    ),
            )
            .mount(&mock_server)
            .await;

        let err = mock_server_upload(&mock_server).await.unwrap_err();

        let mut capture = String::new();
        write_error_chain(&err, &mut capture, "error", AnsiColors::Red).unwrap();

        let capture = capture.replace(&mock_server.uri(), "[SERVER]");
        let capture = anstream::adapter::strip_str(&capture);
        assert_snapshot!(
            &capture,
            @"
        error: Failed to publish `../../test/links/tqdm-4.66.1-py3-none-manylinux_2_12_x86_64.manylinux2010_x86_64.musllinux_1_1_x86_64.whl` to [SERVER]/final
          Caused by: Server returned status code 400 Bad Request. Server says: 400 Error: Use 'source' as Python version for an sdist.
        "
        );
    }

    /// pyx returns `application/problem+json` with RFC 9457 Problem Details.
    #[tokio::test]
    async fn upload_error_problem_details() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/final"))
            .respond_with(
                ResponseTemplate::new(400)
                    .insert_header(
                        "content-type",
                        uv_client::ProblemDetails::CONTENT_TYPE,
                    )
                    .set_body_raw(
                        r#"{"type": "about:blank", "status": 400, "title": "Bad Request", "detail": "Missing required field `name`"}"#,
                        uv_client::ProblemDetails::CONTENT_TYPE,
                    ),
            )
            .mount(&mock_server)
            .await;

        let err = mock_server_upload(&mock_server).await.unwrap_err();

        let mut capture = String::new();
        write_error_chain(&err, &mut capture, "error", AnsiColors::Red).unwrap();

        let capture = capture.replace(&mock_server.uri(), "[SERVER]");
        let capture = anstream::adapter::strip_str(&capture);
        assert_snapshot!(
            &capture,
            @"
        error: Failed to publish `../../test/links/tqdm-4.66.1-py3-none-manylinux_2_12_x86_64.manylinux2010_x86_64.musllinux_1_1_x86_64.whl` to [SERVER]/final
          Caused by: Server returned status code 400 Bad Request. Server message: Bad Request, Missing required field `name`
        "
        );
    }
}
