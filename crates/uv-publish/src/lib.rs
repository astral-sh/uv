mod trusted_publishing;

use crate::trusted_publishing::TrustedPublishingError;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use fs_err::File;
use futures::TryStreamExt;
use glob::{glob, GlobError, PatternError};
use itertools::Itertools;
use reqwest::header::AUTHORIZATION;
use reqwest::multipart::Part;
use reqwest::{Body, Response, StatusCode};
use reqwest_middleware::RequestBuilder;
use reqwest_retry::{Retryable, RetryableStrategy};
use rustc_hash::FxHashSet;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{env, fmt, io};
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;
use tracing::{debug, enabled, trace, Level};
use url::Url;
use uv_client::{BaseClient, UvRetryableStrategy};
use uv_configuration::{KeyringProviderType, TrustedPublishing};
use uv_distribution_filename::{DistFilename, SourceDistExtension, SourceDistFilename};
use uv_fs::{ProgressReader, Simplified};
use uv_metadata::read_metadata_async_seek;
use uv_pypi_types::{Metadata23, MetadataError};
use uv_static::EnvVars;
use uv_warnings::{warn_user, warn_user_once};

pub use trusted_publishing::TrustedPublishingToken;

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
    fn on_download_start(&self, name: &str, size: Option<u64>) -> usize;
    fn on_download_progress(&self, id: usize, inc: u64);
    fn on_download_complete(&self, id: usize);
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
            if filename == ".gitignore" {
                continue;
            }
            let dist_filename = DistFilename::try_from_normalized_filename(&filename)
                .ok_or_else(|| PublishError::InvalidFilename(dist.clone()))?;
            files.push((dist, filename, dist_filename));
        }
    }
    // TODO(konsti): Should we sort those files, e.g. wheels before sdists because they are more
    // certain to have reliable metadata, even though the metadata in the upload API is unreliable
    // in general?
    Ok(files)
}

/// If applicable, attempt obtaining a token for trusted publishing.
pub async fn check_trusted_publishing(
    username: Option<&str>,
    password: Option<&str>,
    keyring_provider: KeyringProviderType,
    trusted_publishing: TrustedPublishing,
    registry: &Url,
    client: &BaseClient,
) -> Result<Option<TrustedPublishingToken>, PublishError> {
    match trusted_publishing {
        TrustedPublishing::Automatic => {
            // If the user provided credentials, use those.
            if username.is_some()
                || password.is_some()
                || keyring_provider != KeyringProviderType::Disabled
            {
                return Ok(None);
            }
            // If we aren't in GitHub Actions, we can't use trusted publishing.
            if env::var(EnvVars::GITHUB_ACTIONS) != Ok("true".to_string()) {
                return Ok(None);
            }
            // We could check for credentials from the keyring or netrc the auth middleware first, but
            // given that we are in GitHub Actions we check for trusted publishing first.
            debug!("Running on GitHub Actions without explicit credentials, checking for trusted publishing");
            match trusted_publishing::get_token(registry, client.for_host(registry)).await {
                Ok(token) => Ok(Some(token)),
                Err(err) => {
                    // TODO(konsti): It would be useful if we could differentiate between actual errors
                    // such as connection errors and warn for them while ignoring errors from trusted
                    // publishing not being configured.
                    debug!("Could not obtain trusted publishing credentials, skipping: {err}");
                    Ok(None)
                }
            }
        }
        TrustedPublishing::Always => {
            debug!("Using trusted publishing for GitHub Actions");
            if env::var(EnvVars::GITHUB_ACTIONS) != Ok("true".to_string()) {
                warn_user_once!(
                    "Trusted publishing was requested, but you're not in GitHub Actions."
                );
            }

            let token = trusted_publishing::get_token(registry, client.for_host(registry)).await?;
            Ok(Some(token))
        }
        TrustedPublishing::Never => Ok(None),
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
    retries: u32,
    username: Option<&str>,
    password: Option<&str>,
    reporter: Arc<impl Reporter>,
) -> Result<bool, PublishError> {
    let form_metadata = form_metadata(file, filename)
        .await
        .map_err(|err| PublishError::PublishPrepare(file.to_path_buf(), Box::new(err)))?;

    // Retry loop
    let mut attempt = 0;
    loop {
        attempt += 1;
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
        if attempt < retries && UvRetryableStrategy.handle(&result) == Some(Retryable::Transient) {
            reporter.on_download_complete(idx);
            warn_user!("Transient request failure for {}, retrying", registry);
            continue;
        }

        let response = result.map_err(|err| {
            PublishError::PublishSend(
                file.to_path_buf(),
                registry.clone(),
                PublishSendError::ReqwestMiddleware(err),
            )
        })?;

        return handle_response(registry, response)
            .await
            .map_err(|err| PublishError::PublishSend(file.to_path_buf(), registry.clone(), err));
    }
}

/// Calculate the SHA256 of a file.
fn hash_file(path: impl AsRef<Path>) -> Result<String, io::Error> {
    // Ideally, this would be async, but in case we actually want to make parallel uploads we should
    // use `spawn_blocking` since sha256 is cpu intensive.
    let mut file = BufReader::new(File::open(path.as_ref())?);
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

// Not in `uv-metadata` because we only support tar files here.
async fn source_dist_pkg_info(file: &Path) -> Result<Vec<u8>, PublishPrepareError> {
    let file = fs_err::tokio::File::open(&file).await?;
    let reader = tokio::io::BufReader::new(file);
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
            let file = fs_err::tokio::File::open(&file).await?;
            let reader = tokio::io::BufReader::new(file);
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
    let hash_hex = hash_file(file)?;

    let metadata = metadata(file, filename).await?;

    let mut form_metadata = vec![
        (":action", "file_upload".to_string()),
        ("sha256_digest", hash_hex),
        ("protocol_version", "1".to_string()),
        ("metadata_version", metadata.metadata_version.clone()),
        // Twine transforms the name with `re.sub("[^A-Za-z0-9.]+", "-", name)`
        // * <https://github.com/pypa/twine/issues/743>
        // * <https://github.com/pypa/twine/blob/5bf3f38ff3d8b2de47b7baa7b652c697d7a64776/twine/package.py#L57-L65>
        // warehouse seems to call `packaging.utils.canonicalize_name` nowadays and has a separate
        // `normalized_name`, so we'll start with this and we'll readjust if there are user reports.
        ("name", metadata.name.clone()),
        ("version", metadata.version.clone()),
        ("filetype", filename.filetype().to_string()),
    ];

    if let DistFilename::WheelFilename(wheel) = filename {
        form_metadata.push(("pyversion", wheel.python_tag.join(".")));
    } else {
        form_metadata.push(("pyversion", "source".to_string()));
    }

    let mut add_option = |name, value: Option<String>| {
        if let Some(some) = value.clone() {
            form_metadata.push((name, some));
        }
    };

    add_option("summary", metadata.summary);
    add_option("description", metadata.description);
    add_option(
        "description_content_type",
        metadata.description_content_type,
    );
    add_option("author", metadata.author);
    add_option("author_email", metadata.author_email);
    add_option("maintainer", metadata.maintainer);
    add_option("maintainer_email", metadata.maintainer_email);
    add_option("license", metadata.license);
    add_option("keywords", metadata.keywords);
    add_option("home_page", metadata.home_page);
    add_option("download_url", metadata.download_url);

    // The GitLab PyPI repository API implementation requires this metadata field and twine always
    // includes it in the request, even when it's empty.
    form_metadata.push((
        "requires_python",
        metadata.requires_python.unwrap_or(String::new()),
    ));

    let mut add_vec = |name, values: Vec<String>| {
        for i in values {
            form_metadata.push((name, i.clone()));
        }
    };

    add_vec("classifiers", metadata.classifiers);
    add_vec("platform", metadata.platforms);
    add_vec("requires_dist", metadata.requires_dist);
    add_vec("provides_dist", metadata.provides_dist);
    add_vec("obsoletes_dist", metadata.obsoletes_dist);
    add_vec("requires_external", metadata.requires_external);
    add_vec("project_urls", metadata.project_urls);

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

    let file = fs_err::tokio::File::open(file).await?;
    let idx = reporter.on_download_start(&filename.to_string(), Some(file.metadata().await?.len()));
    let reader = ProgressReader::new(file, move |read| {
        reporter.on_download_progress(idx, read as u64);
    });
    // Stream wrapping puts a static lifetime requirement on the reader (so the request doesn't have
    // a lifetime) -> callback needs to be static -> reporter reference needs to be Arc'd.
    let file_reader = Body::wrap_stream(ReaderStream::new(reader));
    // See [`files_for_publishing`] on `raw_filename`
    let part = Part::stream(file_reader).file_name(raw_filename.to_string());
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

/// Returns `true` if the file was newly uploaded and `false` if it already existed.
async fn handle_response(registry: &Url, response: Response) -> Result<bool, PublishSendError> {
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
        return Ok(true);
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

    // Detect existing file errors the way twine does.
    // https://github.com/pypa/twine/blob/c512bbf166ac38239e58545a39155285f8747a7b/twine/commands/upload.py#L34-L72
    if status_code == StatusCode::FORBIDDEN {
        if upload_error.contains("overwrite artifact") {
            // Artifactory (https://jfrog.com/artifactory/)
            Ok(false)
        } else {
            Err(PublishSendError::PermissionDenied(
                status_code,
                PublishSendError::extract_error_message(
                    upload_error.to_string(),
                    content_type.as_deref(),
                ),
            ))
        }
    } else if status_code == StatusCode::CONFLICT {
        // conflict, pypiserver (https://pypi.org/project/pypiserver)
        Ok(false)
    } else if status_code == StatusCode::BAD_REQUEST
        && (upload_error.contains("updating asset") || upload_error.contains("already been taken"))
    {
        // Nexus Repository OSS (https://www.sonatype.com/nexus-repository-oss)
        // and Gitlab Enterprise Edition (https://about.gitlab.com)
        Ok(false)
    } else {
        Err(PublishSendError::Status(
            status_code,
            PublishSendError::extract_error_message(
                upload_error.to_string(),
                content_type.as_deref(),
            ),
        ))
    }
}

#[cfg(test)]
mod tests;
