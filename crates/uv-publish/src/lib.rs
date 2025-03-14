mod trusted_publishing;

use crate::trusted_publishing::TrustedPublishingError;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use fs_err::tokio::File;
use futures::TryStreamExt;
use glob::{glob, GlobError, PatternError};
use itertools::Itertools;
use reqwest::header::AUTHORIZATION;
use reqwest::multipart::Part;
use reqwest::{Body, Response, StatusCode};
use reqwest_middleware::RequestBuilder;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{RetryPolicy, Retryable, RetryableStrategy};
use rustc_hash::FxHashSet;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use std::{env, fmt, io};
use thiserror::Error;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::sync::Semaphore;
use tokio_util::io::ReaderStream;
use tracing::{debug, enabled, trace, warn, Level};
use trusted_publishing::TrustedPublishingToken;
use url::Url;
use uv_cache::{Cache, Refresh};
use uv_client::{
    BaseClient, OwnedArchive, RegistryClientBuilder, UvRetryableStrategy, DEFAULT_RETRIES,
};
use uv_configuration::{KeyringProviderType, TrustedPublishing};
use uv_distribution_filename::{DistFilename, SourceDistExtension, SourceDistFilename};
use uv_distribution_types::{IndexCapabilities, IndexUrl};
use uv_extract::hash::{HashReader, Hasher};
use uv_fs::{ProgressReader, Simplified};
use uv_metadata::read_metadata_async_seek;
use uv_pypi_types::{HashAlgorithm, HashDigest, Metadata23, MetadataError};
use uv_static::EnvVars;
use uv_warnings::{warn_user, warn_user_once};

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
    PublishSend(PathBuf, Url, #[source] PublishSendError),
    #[error("Failed to obtain token for trusted publishing")]
    TrustedPublishing(#[from] TrustedPublishingError),
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
}

/// Failure in or after (HTTP) transport for a specific file.
#[derive(Error, Debug)]
pub enum PublishSendError {
    #[error("Failed to send POST request")]
    ReqwestMiddleware(#[source] reqwest_middleware::Error),
    #[error("Upload failed with status {0}")]
    StatusNoBody(StatusCode, #[source] reqwest::Error),
    #[error("Upload failed with status code {0}. Server says: {1}")]
    Status(StatusCode, String),
    #[error("POST requests are not supported by the endpoint, are you using the simple index URL instead of the upload URL?")]
    MethodNotAllowedNoBody,
    #[error("POST requests are not supported by the endpoint, are you using the simple index URL instead of the upload URL? Server says: {0}")]
    MethodNotAllowed(String),
    /// The registry returned a "403 Forbidden".
    #[error("Permission denied (status code {0}): {1}")]
    PermissionDenied(StatusCode, String),
    /// See inline comment.
    #[error("The request was redirected, but redirects are not allowed when publishing, please use the canonical URL: `{0}`")]
    RedirectError(Url),
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

/// Collect the source distributions and wheels for publishing.
///
/// Returns the path, the raw filename and the parsed filename. The raw filename is a fixup for
/// <https://github.com/astral-sh/uv/issues/8030> caused by
/// <https://github.com/pypa/setuptools/issues/3777> in combination with
/// <https://github.com/pypi/warehouse/blob/50a58f3081e693a3772c0283050a275e350004bf/warehouse/forklift/legacy.py#L1133-L1155>
pub fn files_for_publishing(
    paths: Vec<String>,
) -> Result<Vec<(PathBuf, String, DistFilename)>, PublishError> {
    let mut seen = FxHashSet::default();
    let mut files = Vec::new();
    for path in paths {
        for dist in glob(&path).map_err(|err| PublishError::Pattern(path, err))? {
            let dist = dist?;
            if !dist.is_file() {
                continue;
            }
            if !seen.insert(dist.clone()) {
                continue;
            }
            let Some(filename) = dist
                .file_name()
                .and_then(|filename| filename.to_str())
                .map(ToString::to_string)
            else {
                continue;
            };
            let Some(dist_filename) = DistFilename::try_from_normalized_filename(&filename) else {
                debug!("Not a distribution filename: `{filename}`");
                // I've never seen these in upper case
                #[allow(clippy::case_sensitive_file_extension_comparisons)]
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
                        dist.user_display()
                    );
                }
                continue;
            };
            files.push((dist, filename, dist_filename));
        }
    }
    // TODO(konsti): Should we sort those files, e.g. wheels before sdists because they are more
    // certain to have reliable metadata, even though the metadata in the upload API is unreliable
    // in general?
    Ok(files)
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
    trusted_publishing: TrustedPublishing,
    registry: &Url,
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
            // If we aren't in GitHub Actions, we can't use trusted publishing.
            if env::var(EnvVars::GITHUB_ACTIONS) != Ok("true".to_string()) {
                return Ok(TrustedPublishResult::Skipped);
            }
            // We could check for credentials from the keyring or netrc the auth middleware first, but
            // given that we are in GitHub Actions we check for trusted publishing first.
            debug!("Running on GitHub Actions without explicit credentials, checking for trusted publishing");
            match trusted_publishing::get_token(registry, client.for_host(registry)).await {
                Ok(token) => Ok(TrustedPublishResult::Configured(token)),
                Err(err) => {
                    // TODO(konsti): It would be useful if we could differentiate between actual errors
                    // such as connection errors and warn for them while ignoring errors from trusted
                    // publishing not being configured.
                    debug!("Could not obtain trusted publishing credentials, skipping: {err}");
                    Ok(TrustedPublishResult::Ignored(err))
                }
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

            if env::var(EnvVars::GITHUB_ACTIONS) != Ok("true".to_string()) {
                warn_user_once!(
                    "Trusted publishing was requested, but you're not in GitHub Actions."
                );
            }

            let token = trusted_publishing::get_token(registry, client.for_host(registry)).await?;
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
    file: &Path,
    raw_filename: &str,
    filename: &DistFilename,
    registry: &Url,
    client: &BaseClient,
    username: Option<&str>,
    password: Option<&str>,
    check_url_client: Option<&CheckUrlClient<'_>>,
    download_concurrency: &Semaphore,
    reporter: Arc<impl Reporter>,
) -> Result<bool, PublishError> {
    let form_metadata = form_metadata(file, filename)
        .await
        .map_err(|err| PublishError::PublishPrepare(file.to_path_buf(), Box::new(err)))?;

    let mut n_past_retries = 0;
    let start_time = SystemTime::now();
    // N.B. We cannot use the client policy here because it is set to zero retries
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(DEFAULT_RETRIES);
    loop {
        let (request, idx) = build_request(
            file,
            raw_filename,
            filename,
            registry,
            client,
            username,
            password,
            &form_metadata,
            reporter.clone(),
        )
        .await
        .map_err(|err| PublishError::PublishPrepare(file.to_path_buf(), Box::new(err)))?;

        let result = request.send().await;
        if UvRetryableStrategy.handle(&result) == Some(Retryable::Transient) {
            let retry_decision = retry_policy.should_retry(start_time, n_past_retries);
            if let reqwest_retry::RetryDecision::Retry { execute_after } = retry_decision {
                warn_user!("Transient failure while handling response for {registry}; retrying...");
                reporter.on_upload_complete(idx);
                let duration = execute_after
                    .duration_since(SystemTime::now())
                    .unwrap_or_else(|_| Duration::default());
                tokio::time::sleep(duration).await;
                n_past_retries += 1;
                continue;
            }
        }

        let response = result.map_err(|err| {
            PublishError::PublishSend(
                file.to_path_buf(),
                registry.clone(),
                PublishSendError::ReqwestMiddleware(err),
            )
        })?;

        return match handle_response(registry, response).await {
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
                        if check_url(check_url_client, file, filename, download_concurrency).await?
                        {
                            // There was a raced upload of the same file, so even though our upload failed,
                            // the right file now exists in the registry.
                            return Ok(false);
                        }
                    }
                }
                Err(PublishError::PublishSend(
                    file.to_path_buf(),
                    registry.clone(),
                    err,
                ))
            }
        };
    }
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
        .simple(
            filename.name(),
            Some(index_url),
            index_capabilities,
            download_concurrency,
        )
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return match err.into_kind() {
                uv_client::ErrorKind::PackageNotFound(_) => {
                    // The package doesn't exist, so we can't have uploaded it.
                    warn!(
                        "Package not found in the registry; skipping upload check for {filename}"
                    );
                    Ok(false)
                }
                kind => Err(PublishError::CheckUrlIndex(kind.into())),
            };
        }
    };
    let [(_, simple_metadata)] = response.as_slice() else {
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
        let local_hash = hash_file(file, Hasher::from(remote_hash.algorithm))
            .await
            .map_err(|err| {
                PublishError::PublishPrepare(
                    file.to_path_buf(),
                    Box::new(PublishPrepareError::Io(err)),
                )
            })?;
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

/// Calculate the SHA256 of a file.
async fn hash_file(path: impl AsRef<Path>, hasher: Hasher) -> Result<HashDigest, io::Error> {
    debug!("Hashing {}", path.as_ref().display());
    let file = BufReader::new(File::open(path.as_ref()).await?);
    let mut hashers = vec![hasher];
    HashReader::new(file, &mut hashers).finish().await?;
    Ok(HashDigest::from(hashers.remove(0)))
}

// Not in `uv-metadata` because we only support tar files here.
async fn source_dist_pkg_info(file: &Path) -> Result<Vec<u8>, PublishPrepareError> {
    let reader = BufReader::new(File::open(&file).await?);
    let decoded = async_compression::tokio::bufread::GzipDecoder::new(reader);
    let mut archive = tokio_tar::Archive::new(decoded);
    let mut pkg_infos: Vec<(PathBuf, Vec<u8>)> = archive
        .entries()?
        .map_err(PublishPrepareError::from)
        .try_filter_map(|mut entry| async move {
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

/// Collect the non-file fields for the multipart request from the package METADATA.
///
/// Reference implementation: <https://github.com/pypi/warehouse/blob/d2c36d992cf9168e0518201d998b2707a3ef1e72/warehouse/forklift/legacy.py#L1376-L1430>
async fn form_metadata(
    file: &Path,
    filename: &DistFilename,
) -> Result<Vec<(&'static str, String)>, PublishPrepareError> {
    let hash_hex = hash_file(file, Hasher::from(HashAlgorithm::Sha256)).await?;

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
        provides_extras,
        dynamic,
    } = metadata(file, filename).await?;

    let mut form_metadata = vec![
        (":action", "file_upload".to_string()),
        ("sha256_digest", hash_hex.digest.to_string()),
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
    add_option("keywords", keywords);
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
    add_vec("project_urls", project_urls);
    add_vec("provides_dist", provides_dist);
    add_vec("provides_extra", provides_extras);
    add_vec("requires_dist", requires_dist);
    add_vec("requires_external", requires_external);

    Ok(form_metadata)
}

/// Build the upload request.
///
/// Returns the request and the reporter progress bar id.
async fn build_request(
    file: &Path,
    raw_filename: &str,
    filename: &DistFilename,
    registry: &Url,
    client: &BaseClient,
    username: Option<&str>,
    password: Option<&str>,
    form_metadata: &[(&'static str, String)],
    reporter: Arc<impl Reporter>,
) -> Result<(RequestBuilder, usize), PublishPrepareError> {
    let mut form = reqwest::multipart::Form::new();
    for (key, value) in form_metadata {
        form = form.text(*key, value.clone());
    }

    let file = File::open(file).await?;
    let file_size = file.metadata().await?.len();
    let idx = reporter.on_upload_start(&filename.to_string(), Some(file_size));
    let reader = ProgressReader::new(file, move |read| {
        reporter.on_upload_progress(idx, read as u64);
    });
    // Stream wrapping puts a static lifetime requirement on the reader (so the request doesn't have
    // a lifetime) -> callback needs to be static -> reporter reference needs to be Arc'd.
    let file_reader = Body::wrap_stream(ReaderStream::new(reader));
    // See [`files_for_publishing`] on `raw_filename`
    let part = Part::stream_with_length(file_reader, file_size).file_name(raw_filename.to_string());
    form = form.part("content", part);

    let url = if let Some(username) = username {
        if password.is_none() {
            // Attach the username to the URL so the authentication middleware can find the matching
            // password.
            let mut url = registry.clone();
            let _ = url.set_username(username);
            url
        } else {
            // We set the authorization header below.
            registry.clone()
        }
    } else {
        registry.clone()
    };

    let mut request = client
        .for_host(&url)
        .post(url)
        .multipart(form)
        // Ask PyPI for a structured error messages instead of HTML-markup error messages.
        // For other registries, we ask them to return plain text over HTML. See
        // [`PublishSendError::extract_remote_error`].
        .header(
            reqwest::header::ACCEPT,
            "application/json;q=0.9, text/plain;q=0.8, text/html;q=0.7",
        );
    if let (Some(username), Some(password)) = (username, password) {
        debug!("Using username/password basic auth");
        let credentials = BASE64_STANDARD.encode(format!("{username}:{password}"));
        request = request.header(AUTHORIZATION, format!("Basic {credentials}"));
    }
    Ok((request, idx))
}

/// Log response information and map response to an error variant if not successful.
async fn handle_response(registry: &Url, response: Response) -> Result<(), PublishSendError> {
    let status_code = response.status();
    debug!("Response code for {registry}: {status_code}");
    trace!("Response headers for {registry}: {response:?}");

    // When the user accidentally uses https://test.pypi.org/simple (no slash) as publish URL, we
    // get a redirect to https://test.pypi.org/simple/ (the canonical index URL), while changing the
    // method to GET (see https://en.wikipedia.org/wiki/Post/Redirect/Get and
    // https://fetch.spec.whatwg.org/#http-redirect-fetch). The user gets a 200 OK while we actually
    // didn't upload anything! Reqwest doesn't support redirect policies conditional on the HTTP
    // method (https://github.com/seanmonstar/reqwest/issues/1777#issuecomment-2303386160), so we're
    // checking after the fact.
    if response.url() != registry {
        return Err(PublishSendError::RedirectError(response.url().clone()));
    }

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

    // Raced uploads of the same file are handled by the caller.
    Err(PublishSendError::Status(
        status_code,
        PublishSendError::extract_error_message(upload_error.to_string(), content_type.as_deref()),
    ))
}

#[cfg(test)]
mod tests {
    use crate::{build_request, form_metadata, Reporter};
    use insta::{assert_debug_snapshot, assert_snapshot};
    use itertools::Itertools;
    use std::path::PathBuf;
    use std::sync::Arc;
    use url::Url;
    use uv_client::BaseClientBuilder;
    use uv_distribution_filename::DistFilename;

    struct DummyReporter;

    impl Reporter for DummyReporter {
        fn on_progress(&self, _name: &str, _id: usize) {}
        fn on_upload_start(&self, _name: &str, _size: Option<u64>) -> usize {
            0
        }
        fn on_upload_progress(&self, _id: usize, _inc: u64) {}
        fn on_upload_complete(&self, _id: usize) {}
    }

    /// Snapshot the data we send for an upload request for a source distribution.
    #[tokio::test]
    async fn upload_request_source_dist() {
        let raw_filename = "tqdm-999.0.0.tar.gz";
        let file = PathBuf::from("../../scripts/links/").join(raw_filename);
        let filename = DistFilename::try_from_normalized_filename(raw_filename).unwrap();

        let form_metadata = form_metadata(&file, &filename).await.unwrap();

        let formatted_metadata = form_metadata
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .join("\n");
        assert_snapshot!(&formatted_metadata, @r###"
        :action: file_upload
        sha256_digest: 89fa05cffa7f457658373b85de302d24d0c205ceda2819a8739e324b75e9430b
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
        "###);

        let (request, _) = build_request(
            &file,
            raw_filename,
            &filename,
            &Url::parse("https://example.org/upload").unwrap(),
            &BaseClientBuilder::new().build(),
            Some("ferris"),
            Some("F3RR!S"),
            &form_metadata,
            Arc::new(DummyReporter),
        )
        .await
        .unwrap();

        insta::with_settings!({
            filters => [("boundary=[0-9a-f-]+", "boundary=[...]")],
        }, {
            assert_debug_snapshot!(&request, @r###"
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
                    "content-length": "6803",
                    "accept": "application/json;q=0.9, text/plain;q=0.8, text/html;q=0.7",
                    "authorization": "Basic ZmVycmlzOkYzUlIhUw==",
                },
            },
            ..
        }
        "###);
        });
    }

    /// Snapshot the data we send for an upload request for a wheel.
    #[tokio::test]
    async fn upload_request_wheel() {
        let raw_filename =
            "tqdm-4.66.1-py3-none-manylinux_2_12_x86_64.manylinux2010_x86_64.musllinux_1_1_x86_64.whl";
        let file = PathBuf::from("../../scripts/links/").join(raw_filename);
        let filename = DistFilename::try_from_normalized_filename(raw_filename).unwrap();

        let form_metadata = form_metadata(&file, &filename).await.unwrap();

        let formatted_metadata = form_metadata
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .join("\n");
        assert_snapshot!(&formatted_metadata, @r###"
        :action: file_upload
        sha256_digest: 0d88ca657bc6b64995ca416e0c59c71af85cc10015d940fa446c42a8b485ee1c
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
        "###);

        let (request, _) = build_request(
            &file,
            raw_filename,
            &filename,
            &Url::parse("https://example.org/upload").unwrap(),
            &BaseClientBuilder::new().build(),
            Some("ferris"),
            Some("F3RR!S"),
            &form_metadata,
            Arc::new(DummyReporter),
        )
        .await
        .unwrap();

        insta::with_settings!({
            filters => [("boundary=[0-9a-f-]+", "boundary=[...]")],
        }, {
            assert_debug_snapshot!(&request, @r###"
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
                    "content-length": "19330",
                    "accept": "application/json;q=0.9, text/plain;q=0.8, text/html;q=0.7",
                    "authorization": "Basic ZmVycmlzOkYzUlIhUw==",
                },
            },
            ..
        }
        "###);
        });
    }
}
