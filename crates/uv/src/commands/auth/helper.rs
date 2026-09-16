use std::collections::HashMap;
use std::fmt::Write;
use std::io::Read;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

use uv_auth::{AuthBackend, Credentials};
use uv_preview::{Preview, PreviewFeature};
use uv_redacted::DisplaySafeUrl;
use uv_warnings::warn_user;

use crate::{commands::ExitStatus, printer::Printer};

/// Request format for the Bazel credential helper protocol.
#[derive(Debug, Deserialize)]
struct BazelCredentialRequest {
    uri: DisplaySafeUrl,
}

impl BazelCredentialRequest {
    fn from_str(s: &str) -> Result<Self> {
        serde_json::from_str(s).context("Failed to parse credential request as JSON")
    }

    fn from_stdin() -> Result<Self> {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read from stdin")?;

        Self::from_str(&buffer)
    }
}

/// Response format for the Bazel credential helper protocol.
#[derive(Debug, Serialize, Default)]
struct BazelCredentialResponse {
    headers: HashMap<String, Vec<String>>,
}

impl TryFrom<Credentials> for BazelCredentialResponse {
    fn try_from(creds: Credentials) -> Result<Self> {
        let header_str = creds
            .to_header_value()
            .context("Invalid authorization header")?
            .to_str()
            // TODO: this is infallible in practice
            .context("Failed to convert header value to string")?
            .to_owned();

        Ok(Self {
            headers: HashMap::from([("Authorization".to_owned(), vec![header_str])]),
        })
    }

    type Error = anyhow::Error;
}

async fn credentials_for_url(
    url: &DisplaySafeUrl,
    preview: Preview,
) -> Result<Option<Credentials>> {
    // Use only the username from the URL, if present - discarding the password
    let url_credentials = Credentials::from_url(url)?;
    let username = url_credentials.as_ref().and_then(|c| c.username());
    if url_credentials
        .as_ref()
        .is_some_and(|credentials| credentials.password().is_some())
    {
        debug!("URL '{url}' contain a password; ignoring");
    }

    let backend = AuthBackend::from_settings(preview).await?;
    let credentials = match &backend {
        AuthBackend::System(provider) => provider.fetch(url, username).await,
        AuthBackend::TextStore(store, _lock) => store.get_credentials(url, username)?.cloned(),
    };
    Ok(credentials)
}

/// Implement the Bazel credential helper protocol.
///
/// Reads a JSON request from stdin containing a URI, looks up credentials
/// for that URI using uv's authentication backends, and writes a JSON response
/// to stdout containing HTTP headers (if credentials are found).
///
/// Protocol specification TLDR:
/// - Input (stdin): `{"uri": "https://example.com/path"}`
/// - Output (stdout): `{"headers": {"Authorization": ["Basic ..."]}}` or `{"headers": {}}`
/// - Errors: Written to stderr with non-zero exit code
///
/// Full spec is [available here](https://github.com/bazelbuild/proposals/blob/main/designs/2022-06-07-bazel-credential-helpers.md)
pub(crate) async fn helper(preview: Preview, printer: Printer) -> Result<ExitStatus> {
    if !preview.is_enabled(PreviewFeature::AuthHelper) {
        warn_user!(
            "The `uv auth helper` command is experimental and may change without warning. Pass `--preview-features {}` to disable this warning",
            PreviewFeature::AuthHelper
        );
    }

    let request = BazelCredentialRequest::from_stdin()?;

    // TODO: make this logic generic over the protocol by providing `request.uri` from a
    // trait - that should help with adding new protocols
    let credentials = credentials_for_url(&request.uri, preview).await?;

    let response = serde_json::to_string(
        &credentials
            .map(BazelCredentialResponse::try_from)
            .unwrap_or_else(|| Ok(BazelCredentialResponse::default()))?,
    )
    .context("Failed to serialize response as JSON")?;
    writeln!(printer.stdout_important(), "{response}")?;
    Ok(ExitStatus::Success)
}
