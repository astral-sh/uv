use std::collections::HashMap;
use std::fmt::Write;
use std::io::Read;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use url::Url;

use uv_auth::{AuthBackend, Credentials, PyxTokenStore};
use uv_client::BaseClientBuilder;
use uv_preview::Preview;
use uv_redacted::DisplaySafeUrl;

use crate::{commands::ExitStatus, printer::Printer, settings::NetworkSettings};

/// Request format for the Bazel credential helper protocol.
#[derive(Debug, Deserialize)]
struct CredentialRequest {
    uri: String,
}

/// Response format for the Bazel credential helper protocol.
#[derive(Debug, Serialize)]
struct CredentialResponse {
    headers: HashMap<String, Vec<String>>,
}

async fn credentials_for_url(
    url: &DisplaySafeUrl,
    preview: Preview,
    network_settings: &NetworkSettings,
) -> Result<Option<Credentials>> {
    let pyx_store = PyxTokenStore::from_settings()?;

    // Use only the username from the URL, if present - discarding the password
    let url_credentials = Credentials::from_url(url);
    let url_username = url_credentials.as_ref().and_then(|c| c.username());
    let username = url_username.map(ToString::to_string);

    if pyx_store.is_known_domain(url) {
        if username.is_some() {
            bail!(
                "Cannot specify a username for URLs under {}",
                url.host()
                    .map(|host| host.to_string())
                    .unwrap_or("this host".to_owned())
            );
        }
        let client = BaseClientBuilder::new(
            network_settings.connectivity,
            network_settings.native_tls,
            network_settings.allow_insecure_host.clone(),
            preview,
            network_settings.timeout,
            network_settings.retries,
        )
        .auth_integration(uv_client::AuthIntegration::NoAuthMiddleware)
        .build();
        let maybe_token = pyx_store
            .access_token(client.for_host(pyx_store.api()).raw_client(), 0)
            .await
            .context("Authentication failure")?;
        let token = maybe_token.ok_or_else(|| anyhow::anyhow!("No access token found"))?;
        return Ok(Some(Credentials::bearer(token.into_bytes())));
    }
    let backend = AuthBackend::from_settings(preview)?;
    let credentials = match &backend {
        AuthBackend::System(provider) => provider.fetch(url, username.as_deref()).await,
        AuthBackend::TextStore(store, _lock) => {
            store.get_credentials(url, username.as_deref()).cloned()
        }
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
pub(crate) async fn helper(
    preview: Preview,
    network_settings: &NetworkSettings,
    printer: Printer,
) -> Result<ExitStatus> {
    // Read CredentialRequest from stdin
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .context("Failed to read from stdin")?;

    let request: CredentialRequest =
        serde_json::from_str(&buffer).context("Failed to parse credential request as JSON")?;

    let url = Url::parse(&request.uri).context("Invalid URI in credential request")?;
    let safe_url = DisplaySafeUrl::from_url(url);

    // Convert credentials to HTTP headers
    let mut headers = HashMap::new();

    let credentials = credentials_for_url(&safe_url, preview, network_settings).await?;

    if let Some(creds) = credentials {
        // Only include the Authorization header if credentials are authenticated
        // (i.e., not just a username without password)
        if creds.is_authenticated() {
            let header_value = creds.to_header_value();

            // Convert HeaderValue to String
            let header_str = header_value
                .to_str()
                .context("Failed to convert header value to string")?
                .to_string();

            headers.insert("Authorization".to_string(), vec![header_str]);
        }
    }

    let response = serde_json::to_string(&CredentialResponse { headers })
        .context("Failed to serialize response as JSON")?;
    writeln!(printer.stdout(), "{response}")?;
    Ok(ExitStatus::Success)
}
