use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use console::Term;
use owo_colors::{AnsiColors, OwoColorize};
use tokio::sync::Semaphore;
use tracing::{debug, info, trace};
use uv_auth::{Credentials, DEFAULT_TOLERANCE_SECS, PyxTokenStore};
use uv_cache::Cache;
use uv_client::{AuthIntegration, BaseClient, BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{KeyringProviderType, TrustedPublishing};
use uv_distribution_types::{IndexCapabilities, IndexLocations, IndexUrl};
use uv_pep508::VerbatimUrl;
use uv_publish::{
    CheckUrlClient, FormMetadata, PublishError, TrustedPublishResult, check_trusted_publishing,
    files_for_publishing, upload,
};
use uv_redacted::DisplaySafeUrl;
use uv_warnings::{warn_user_once, write_error_chain};

use crate::commands::reporters::PublishReporter;
use crate::commands::{ExitStatus, human_readable_bytes};
use crate::printer::Printer;

pub(crate) async fn publish(
    paths: Vec<String>,
    publish_url: DisplaySafeUrl,
    trusted_publishing: TrustedPublishing,
    keyring_provider: KeyringProviderType,
    client_builder: &BaseClientBuilder<'_>,
    username: Option<String>,
    password: Option<String>,
    check_url: Option<IndexUrl>,
    index: Option<String>,
    index_locations: IndexLocations,
    dry_run: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if client_builder.is_offline() {
        bail!("Unable to publish files in offline mode");
    }

    let token_store = PyxTokenStore::from_settings()?;

    let (publish_url, check_url) = if let Some(index_name) = index {
        // If the user provided an index by name, look it up.
        debug!("Publishing with index {index_name}");
        let index = index_locations
            .simple_indexes()
            .find(|index| {
                index
                    .name
                    .as_ref()
                    .is_some_and(|name| name.as_ref() == index_name)
            })
            .with_context(|| {
                let mut index_names: Vec<String> = index_locations
                    .simple_indexes()
                    .filter_map(|index| index.name.as_ref())
                    .map(ToString::to_string)
                    .collect();
                index_names.sort();
                if index_names.is_empty() {
                    format!("No indexes were found, can't use index: `{index_name}`")
                } else {
                    let index_names = index_names.join("`, `");
                    format!("Index not found: `{index_name}`. Found indexes: `{index_names}`")
                }
            })?;
        let publish_url = index
            .publish_url
            .clone()
            .with_context(|| format!("Index is missing a publish URL: `{index_name}`"))?;
        let check_url = index.url.clone();
        (publish_url, Some(check_url))
    } else if token_store.is_known_url(&publish_url) {
        // If the user is publishing to a known index, construct the check URL from the publish
        // URL.
        let check_url = check_url.or_else(|| {
            infer_check_url(&publish_url)
                .inspect(|check_url| debug!("Inferred check URL: {check_url}"))
        });
        (publish_url, check_url)
    } else {
        (publish_url, check_url)
    };

    let files = files_for_publishing(paths)?;
    match files.len() {
        0 => bail!("No files found to publish"),
        1 => {
            if dry_run {
                writeln!(printer.stderr(), "Checking 1 file against {publish_url}")?;
            } else {
                writeln!(printer.stderr(), "Publishing 1 file to {publish_url}")?;
            }
        }
        n => {
            if dry_run {
                writeln!(printer.stderr(), "Checking {n} files against {publish_url}")?;
            } else {
                writeln!(printer.stderr(), "Publishing {n} files to {publish_url}")?;
            }
        }
    }

    // * For the uploads themselves, we roll our own retries due to
    //   https://github.com/seanmonstar/reqwest/issues/2416, but for trusted publishing, we want
    //   the default retries. We set the retries to 0 here and manually construct the retry policy
    //   in the upload loop.
    // * We want to allow configuring TLS for the registry, while for trusted publishing we know the
    //   defaults are correct.
    // * For the uploads themselves, we know we need an authorization header and we can't nor
    //   shouldn't try cloning the request to make an unauthenticated request first, but we want
    //   keyring integration. For trusted publishing, we use an OIDC auth routine without keyring
    //   or other auth integration.
    let upload_client = client_builder
        .clone()
        .retries(0)
        .keyring(keyring_provider)
        // Don't try cloning the request to make an unauthenticated request first.
        .auth_integration(AuthIntegration::OnlyAuthenticated)
        // Set a very high timeout for uploads, connections are often 10x slower on upload than
        // download. 15 min is taken from the time a trusted publishing token is valid.
        .default_timeout(Duration::from_secs(15 * 60))
        .build();
    let oidc_client = client_builder
        .clone()
        .auth_integration(AuthIntegration::NoAuthMiddleware)
        .wrap_existing(&upload_client);

    // We're only checking a single URL and one at a time, so 1 permit is sufficient
    let download_concurrency = Arc::new(Semaphore::new(1));

    // Load credentials.
    let (publish_url, credentials) = gather_credentials(
        publish_url,
        username,
        password,
        trusted_publishing,
        keyring_provider,
        &token_store,
        &oidc_client,
        &upload_client,
        check_url.as_ref(),
        Prompt::Enabled,
        printer,
    )
    .await?;

    // Initialize the registry client.
    let check_url_client = if let Some(index_url) = &check_url {
        let registry_client_builder =
            RegistryClientBuilder::new(client_builder.clone(), cache.clone())
                .index_locations(index_locations)
                .keyring(keyring_provider);
        Some(CheckUrlClient {
            index_url: index_url.clone(),
            registry_client_builder,
            client: &upload_client,
            index_capabilities: IndexCapabilities::default(),
            cache,
        })
    } else {
        None
    };

    for (file, raw_filename, filename) in files {
        if let Some(check_url_client) = &check_url_client {
            if uv_publish::check_url(check_url_client, &file, &filename, &download_concurrency)
                .await?
            {
                writeln!(printer.stderr(), "File {filename} already exists, skipping")?;
                continue;
            }
        }

        let size = fs_err::metadata(&file)?.len();
        let (bytes, unit) = human_readable_bytes(size);
        if dry_run {
            writeln!(
                printer.stderr(),
                "{} {filename} {}",
                "Checking".bold().cyan(),
                format!("({bytes:.1}{unit})").dimmed()
            )?;
        } else {
            writeln!(
                printer.stderr(),
                "{} {filename} {}",
                "Uploading".bold().green(),
                format!("({bytes:.1}{unit})").dimmed()
            )?;
        }

        // Collect the metadata for the file.
        let form_metadata = FormMetadata::read_from_file(&file, &filename)
            .await
            .map_err(|err| PublishError::PublishPrepare(file.clone(), Box::new(err)))?;

        // Run validation checks on the file, but don't upload it (if possible).
        uv_publish::validate(
            &file,
            &form_metadata,
            &raw_filename,
            &publish_url,
            &token_store,
            &upload_client,
            &credentials,
        )
        .await?;

        if dry_run {
            continue;
        }

        let reporter = PublishReporter::single(printer);
        let uploaded = upload(
            &file,
            &form_metadata,
            &raw_filename,
            &filename,
            &publish_url,
            &upload_client,
            &credentials,
            check_url_client.as_ref(),
            &download_concurrency,
            // Needs to be an `Arc` because the reqwest `Body` static lifetime requirement
            Arc::new(reporter),
        )
        .await?; // Filename and/or URL are already attached, if applicable.
        info!("Upload succeeded");

        if !uploaded {
            writeln!(
                printer.stderr(),
                "{}",
                "File already exists, skipping".dimmed()
            )?;
        }
    }

    Ok(ExitStatus::Success)
}

/// Whether to allow prompting for username and password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prompt {
    Enabled,
    #[allow(dead_code)]
    Disabled,
}

/// Unify the different possible source for username and password information.
///
/// Possible credential sources are environment variables, the CLI, the URL, the keyring, trusted
/// publishing or a prompt.
///
/// The username can come from, in order:
///
/// - Mutually exclusive:
///   - `--username` or `UV_PUBLISH_USERNAME`. The CLI option overrides the environment variable
///   - The username field in the publish URL
///   - If `--token` or `UV_PUBLISH_TOKEN` are used, it is `__token__`. The CLI option
///     overrides the environment variable
/// - If trusted publishing is available, it is `__token__`
/// - (We currently do not read the username from the keyring)
/// - If stderr is a tty, prompt the user
///
/// The password can come from, in order:
///
/// - Mutually exclusive:
///   - `--password` or `UV_PUBLISH_PASSWORD`. The CLI option overrides the environment variable
///   - The password field in the publish URL
///   - If `--token` or `UV_PUBLISH_TOKEN` are used, it is the token value. The CLI option overrides
///     the environment variable
/// - If the keyring is enabled, the keyring entry for the URL and username
/// - If trusted publishing is available, the trusted publishing token
/// - If stderr is a tty, prompt the user
///
/// If no credentials are found, the auth middleware does a final check for cached credentials and
/// otherwise errors without sending the request.
///
/// Returns the publish URL, the username and the password.
async fn gather_credentials(
    mut publish_url: DisplaySafeUrl,
    mut username: Option<String>,
    mut password: Option<String>,
    trusted_publishing: TrustedPublishing,
    keyring_provider: KeyringProviderType,
    token_store: &PyxTokenStore,
    oidc_client: &BaseClient,
    base_client: &BaseClient,
    check_url: Option<&IndexUrl>,
    prompt: Prompt,
    printer: Printer,
) -> Result<(DisplaySafeUrl, Credentials)> {
    // Support reading username and password from the URL, for symmetry with the index API.
    if let Some(url_password) = publish_url.password() {
        if password.is_some_and(|password| password != url_password) {
            bail!("The password can't be set both in the publish URL and in the CLI");
        }
        password = Some(url_password.to_string());
        publish_url
            .set_password(None)
            .expect("Failed to clear publish URL password");
    }

    if !publish_url.username().is_empty() {
        if username.is_some_and(|username| username != publish_url.username()) {
            bail!("The username can't be set both in the publish URL and in the CLI");
        }
        username = Some(publish_url.username().to_string());
        publish_url
            .set_username("")
            .expect("Failed to clear publish URL username");
    }

    // If the user is publishing to pyx, load the credentials from the store.
    if username.is_none() && password.is_none() {
        if token_store.is_known_url(&publish_url) {
            if let Some(token) = token_store
                .access_token(
                    base_client.for_host(token_store.api()).raw_client(),
                    DEFAULT_TOLERANCE_SECS,
                )
                .await?
            {
                debug!("Using authentication token from the store");
                return Ok((publish_url, Credentials::from(token)));
            }
        }
    }

    // If applicable, attempt obtaining a token for trusted publishing.
    let trusted_publishing_token = check_trusted_publishing(
        username.as_deref(),
        password.as_deref(),
        keyring_provider,
        trusted_publishing,
        &publish_url,
        oidc_client,
    )
    .await?;

    let (username, mut password) =
        if let TrustedPublishResult::Configured(password) = &trusted_publishing_token {
            (Some("__token__".to_string()), Some(password.to_string()))
        } else {
            if username.is_none() && password.is_none() {
                match prompt {
                    Prompt::Enabled => prompt_username_and_password()?,
                    Prompt::Disabled => (None, None),
                }
            } else {
                (username, password)
            }
        };

    if password.is_some() && username.is_none() {
        bail!(
            "Attempted to publish with a password, but no username. Either provide a username \
            with `--username` (`UV_PUBLISH_USERNAME`), or use `--token` (`UV_PUBLISH_TOKEN`) \
            instead of a password."
        );
    }

    if username.is_none() && password.is_none() && keyring_provider == KeyringProviderType::Disabled
    {
        if let TrustedPublishResult::Ignored(err) = trusted_publishing_token {
            // The user has configured something incorrectly:
            // * The user forgot to configure credentials.
            // * The user forgot to forward the secrets as env vars (or used the wrong ones).
            // * The trusted publishing configuration is wrong.
            writeln!(
                printer.stderr(),
                "Note: Neither credentials nor keyring are configured, and there was an error \
                fetching the trusted publishing token. If you don't want to use trusted \
                publishing, you can ignore this error, but you need to provide credentials."
            )?;

            trace!("Error trace: {err:?}");
            write_error_chain(
                anyhow::Error::from(err)
                    .context("Trusted publishing failed")
                    .as_ref(),
                printer.stderr(),
                "error",
                AnsiColors::Red,
            )?;
        }
    }

    // If applicable, fetch the password from the keyring eagerly to avoid user confusion about
    // missing keyring entries later.
    if let Some(provider) = keyring_provider.to_provider() {
        if password.is_none() {
            if let Some(username) = &username {
                debug!("Fetching password from keyring");
                if let Some(keyring_password) = provider
                    .fetch(DisplaySafeUrl::ref_cast(&publish_url), Some(username))
                    .await
                    .as_ref()
                    .and_then(|credentials| credentials.password())
                {
                    password = Some(keyring_password.to_string());
                } else {
                    warn_user_once!(
                        "Keyring has no password for URL `{publish_url}` and username `{username}`"
                    );
                }
            }
        } else if check_url.is_none() {
            warn_user_once!(
                "Using `--keyring-provider` with a password or token and no check URL has no effect"
            );
        } else {
            // We may be using the keyring for the simple index.
        }
    }

    let credentials = Credentials::basic(username, password);

    Ok((publish_url, credentials))
}

fn prompt_username_and_password() -> Result<(Option<String>, Option<String>)> {
    let term = Term::stderr();
    if !term.is_term() {
        return Ok((None, None));
    }
    let username_prompt = "Enter username ('__token__' if using a token): ";
    let password_prompt = "Enter password: ";
    let username = uv_console::input(username_prompt, &term).context("Failed to read username")?;
    let password =
        uv_console::password(password_prompt, &term).context("Failed to read password")?;
    Ok((Some(username), Some(password)))
}

/// Construct a Simple Index URL from a publish URL, if possible.
///
/// Matches against a publish URL of the form `/v1/upload/{workspace}/{registry}` and returns
/// `/simple/{workspace}/{registry}`.
fn infer_check_url(publish_url: &DisplaySafeUrl) -> Option<IndexUrl> {
    let mut segments = publish_url.path_segments()?;

    let v1 = segments.next()?;
    if v1 != "v1" {
        return None;
    }

    let upload = segments.next()?;
    if upload != "upload" {
        return None;
    }

    let workspace = segments.next()?;
    if workspace.is_empty() {
        return None;
    }

    let registry = segments.next()?;
    if registry.is_empty() {
        return None;
    }

    // Skip any empty segments (trailing slash handling)
    for remaining in segments {
        if !remaining.is_empty() {
            return None;
        }
    }

    // Reconstruct the URL with `/simple/{workspace}/{registry}`.
    let mut check_url = publish_url.clone();
    {
        let mut segments = check_url.path_segments_mut().ok()?;
        segments.clear();
        segments.push("simple").push(workspace).push(registry);
    }
    Some(IndexUrl::from(VerbatimUrl::from(check_url)))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use insta::assert_snapshot;

    use uv_redacted::DisplaySafeUrl;

    async fn get_credentials(
        url: DisplaySafeUrl,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<(DisplaySafeUrl, Credentials)> {
        let client = BaseClientBuilder::default().build();
        let token_store = PyxTokenStore::from_settings()?;
        gather_credentials(
            url,
            username,
            password,
            TrustedPublishing::Never,
            KeyringProviderType::Disabled,
            &token_store,
            &client,
            &client,
            None,
            Prompt::Disabled,
            Printer::Quiet,
        )
        .await
    }

    #[tokio::test]
    async fn username_password_sources() {
        let example_url = DisplaySafeUrl::from_str("https://example.com").unwrap();
        let example_url_username = DisplaySafeUrl::from_str("https://ferris@example.com").unwrap();
        let example_url_username_password =
            DisplaySafeUrl::from_str("https://ferris:f3rr1s@example.com").unwrap();

        let (publish_url, credentials) = get_credentials(example_url.clone(), None, None)
            .await
            .unwrap();
        assert_eq!(publish_url, example_url);
        assert_eq!(credentials.username(), None);
        assert_eq!(credentials.password(), None);

        let (publish_url, credentials) = get_credentials(example_url_username.clone(), None, None)
            .await
            .unwrap();
        assert_eq!(publish_url, example_url);
        assert_eq!(credentials.username(), Some("ferris"));
        assert_eq!(credentials.password(), None);

        let (publish_url, credentials) =
            get_credentials(example_url_username_password.clone(), None, None)
                .await
                .unwrap();
        assert_eq!(publish_url, example_url);
        assert_eq!(credentials.username(), Some("ferris"));
        assert_eq!(credentials.password(), Some("f3rr1s"));

        // Ok: The username is the same between CLI/env vars and URL
        let (publish_url, credentials) = get_credentials(
            example_url_username_password.clone(),
            Some("ferris".to_string()),
            None,
        )
        .await
        .unwrap();
        assert_eq!(publish_url, example_url);
        assert_eq!(credentials.username(), Some("ferris"));
        assert_eq!(credentials.password(), Some("f3rr1s"));

        // Err: There are two different usernames between CLI/env vars and URL
        let err = get_credentials(
            example_url_username_password.clone(),
            Some("packaging-platypus".to_string()),
            None,
        )
        .await
        .unwrap_err();
        assert_snapshot!(
            err.to_string(),
            @"The username can't be set both in the publish URL and in the CLI"
        );

        // Ok: The username and password are the same between CLI/env vars and URL
        let (publish_url, credentials) = get_credentials(
            example_url_username_password.clone(),
            Some("ferris".to_string()),
            Some("f3rr1s".to_string()),
        )
        .await
        .unwrap();
        assert_eq!(publish_url, example_url);
        assert_eq!(credentials.username(), Some("ferris"));
        assert_eq!(credentials.password(), Some("f3rr1s"));

        // Err: There are two different passwords between CLI/env vars and URL
        let err = get_credentials(
            example_url_username_password.clone(),
            Some("ferris".to_string()),
            Some("secret".to_string()),
        )
        .await
        .unwrap_err();
        assert_snapshot!(
            err.to_string(),
            @"The password can't be set both in the publish URL and in the CLI"
        );
    }

    #[test]
    fn test_infer_check_url() {
        let url =
            DisplaySafeUrl::from_str("https://example.com/v1/upload/workspace/registry").unwrap();
        let check_url = infer_check_url(&url);
        assert_eq!(
            check_url,
            Some(IndexUrl::from_str("https://example.com/simple/workspace/registry").unwrap())
        );

        let url =
            DisplaySafeUrl::from_str("https://example.com/v1/upload/workspace/registry/").unwrap();
        let check_url = infer_check_url(&url);
        assert_eq!(
            check_url,
            Some(IndexUrl::from_str("https://example.com/simple/workspace/registry").unwrap())
        );

        let url =
            DisplaySafeUrl::from_str("https://example.com/upload/workspace/registry").unwrap();
        let check_url = infer_check_url(&url);
        assert_eq!(check_url, None);

        let url = DisplaySafeUrl::from_str("https://example.com/upload/workspace/registry/package")
            .unwrap();
        let check_url = infer_check_url(&url);
        assert_eq!(check_url, None);
    }
}
