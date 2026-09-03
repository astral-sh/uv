use std::borrow::Cow;
use std::fmt::Write;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use console::Term;
use owo_colors::OwoColorize;
use tracing::{debug, info, trace};
use uv_auth::Credentials;
use uv_cache::Cache;
use uv_client::{
    AuthIntegration, BaseClient, BaseClientBuilder, RedirectPolicy, RegistryClientBuilder,
};
use uv_configuration::{KeyringProviderType, TrustedPublishing};
use uv_distribution_types::{IndexLocations, IndexUrl};
use uv_errors::{ErrorOptions, Hints, write_error_chain_with_options};
use uv_publish::{
    PreparedDistribution, PublishSession, TrustedPublishResult, TrustedPublishingToken,
    UploadOutcome, burn_trusted_publishing_token, check_trusted_publishing,
};
use uv_redacted::DisplaySafeUrl;
use uv_settings::EnvironmentOptions;
use uv_warnings::{warn_user, warn_user_once};

use crate::commands::ExitStatus;
use crate::commands::reporters::PublishReporter;
use crate::printer::Printer;

pub(crate) async fn publish(
    paths: Vec<String>,
    publish_url: DisplaySafeUrl,
    trusted_publishing: TrustedPublishing,
    keyring_provider: KeyringProviderType,
    environment: &EnvironmentOptions,
    client_builder: &BaseClientBuilder<'_>,
    username: Option<String>,
    password: Option<String>,
    check_url: Option<IndexUrl>,
    index: Option<String>,
    index_locations: IndexLocations,
    dry_run: bool,
    no_attestations: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if client_builder.is_offline() {
        bail!("Unable to publish files in offline mode");
    }

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
    } else {
        (publish_url, check_url)
    };

    let distributions = PublishSession::prepare(paths, no_attestations)?;
    match distributions.len() {
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
        // Disable automatic redirect, as the streaming publish request is not cloneable.
        // Rely on custom redirect logic instead.
        .redirect(RedirectPolicy::NoRedirect)
        .read_timeout(environment.http_read_timeout_upload)
        .connect_timeout(environment.http_connect_timeout)
        .client_name("upload")
        .build()?;
    // For OIDC (trusted publishing), we need retries (GitHub's networking is unreliable)
    // and default timeouts.
    let oidc_client = client_builder
        .clone()
        .auth_integration(AuthIntegration::NoAuthMiddleware)
        .client_name("oidc")
        .build()?;

    // Load credentials.
    let (publish_url, credentials) = gather_credentials(
        publish_url,
        username,
        password,
        trusted_publishing,
        keyring_provider,
        &oidc_client,
        check_url.as_ref(),
        Prompt::Enabled,
        printer,
    )
    .await?;
    let mut session = PublishSession::new(
        publish_url.clone(),
        credentials.as_credentials().into_owned(),
        &upload_client,
        client_builder.retry_policy(),
    );
    if let Some(index_url) = check_url {
        let registry_client_builder =
            RegistryClientBuilder::new(client_builder.clone(), cache.clone())
                .index_locations(index_locations)
                .keyring(keyring_provider);
        session = session.with_check_url(index_url, registry_client_builder, cache);
    }

    // Keep the publishing result so token revocation also runs after an upload or metadata error.
    let result = publish_files(distributions, &mut session, dry_run, printer).await;

    if let PublishingCredentials::TrustedPublishing(token) = &credentials
        && let Err(err) = burn_trusted_publishing_token(token, &publish_url, &oidc_client).await
    {
        warn_user!(
            "Failed to invalidate trusted publishing token. It will expire naturally. Cause: {err}"
        );
        debug!("Trusted publishing token revocation failed: {err:?}");
    }

    result
}

/// Publish each distribution, reporting all validation failures during a dry run.
async fn publish_files(
    distributions: Vec<PreparedDistribution>,
    session: &mut PublishSession<'_>,
    dry_run: bool,
    printer: Printer,
) -> Result<ExitStatus> {
    let mut error_count: usize = 0;

    for prepared in distributions {
        let reporter = Arc::new(PublishReporter::single(printer, dry_run));
        match publish_file(prepared, session, reporter, dry_run, printer).await {
            Ok(()) => {}
            Err(err) => {
                if !dry_run {
                    return Err(err);
                }
                write_error_chain_with_options(
                    err.as_ref(),
                    Hints::none(),
                    ErrorOptions::default().with_stream(printer.stderr()),
                )?;
                error_count += 1;
            }
        }
    }

    if error_count > 0 {
        let failed = if error_count == 1 { "file" } else { "files" };
        writeln!(printer.stderr(), "Found issues with {error_count} {failed}")?;
        return Ok(ExitStatus::Failure);
    }

    Ok(ExitStatus::Success)
}

/// Upload a prepared distribution unless this is a dry run.
async fn publish_file(
    prepared: PreparedDistribution,
    session: &mut PublishSession<'_>,
    reporter: Arc<PublishReporter>,
    dry_run: bool,
    printer: Printer,
) -> Result<()> {
    let normalized_filename = prepared.filename().to_string();
    if prepared.raw_filename() != normalized_filename {
        warn_user_once!(
            "`{}` has a non-normalized filename (expected `{normalized_filename}`), skipping",
            prepared.raw_filename()
        );
        return Ok(());
    }

    if session.check_existing(&prepared, reporter.clone()).await? {
        writeln!(
            printer.stderr(),
            "File {} already exists, skipping",
            prepared.filename()
        )?;
        return Ok(());
    }

    if dry_run {
        session.dry_run(&prepared, reporter).await?;
        return Ok(());
    }

    let uploaded = session.upload(prepared, reporter).await?;
    info!("Upload succeeded");

    match uploaded {
        UploadOutcome::Uploaded => {}
        UploadOutcome::AlreadyExists => {
            writeln!(
                printer.stderr(),
                "{}",
                "File already exists, skipping".dimmed()
            )?;
        }
    }

    Ok(())
}

/// Credentials for publishing, including whether they require revocation after use.
enum PublishingCredentials {
    /// Credentials supplied by the user or resolved by the authentication middleware.
    Supplied(Credentials),
    /// A short-lived token obtained through trusted publishing.
    TrustedPublishing(TrustedPublishingToken),
}

impl PublishingCredentials {
    /// Return the HTTP credentials to use for uploads.
    fn as_credentials(&self) -> Cow<'_, Credentials> {
        match self {
            Self::Supplied(credentials) => Cow::Borrowed(credentials),
            Self::TrustedPublishing(token) => Cow::Owned(Credentials::basic(
                Some("__token__".to_string()),
                Some(token.to_string()),
            )),
        }
    }
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
/// Returns the publish URL and [`PublishingCredentials`].
async fn gather_credentials(
    mut publish_url: DisplaySafeUrl,
    mut username: Option<String>,
    mut password: Option<String>,
    trusted_publishing: TrustedPublishing,
    keyring_provider: KeyringProviderType,
    oidc_client: &BaseClient,
    check_url: Option<&IndexUrl>,
    prompt: Prompt,
    printer: Printer,
) -> Result<(DisplaySafeUrl, PublishingCredentials)> {
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

    // If applicable, attempt obtaining a token for trusted publishing.
    let trusted_publishing_status = match check_trusted_publishing(
        username.as_deref(),
        password.as_deref(),
        keyring_provider,
        trusted_publishing,
        &publish_url,
        oidc_client,
    )
    .await?
    {
        TrustedPublishResult::Configured(token) => {
            return Ok((publish_url, PublishingCredentials::TrustedPublishing(token)));
        }
        TrustedPublishResult::Skipped => None,
        TrustedPublishResult::Ignored(err) => Some(err),
    };

    let (username, mut password) = if username.is_none() && password.is_none() {
        match prompt {
            Prompt::Enabled => prompt_username_and_password()?,
            Prompt::Disabled => (None, None),
        }
    } else {
        (username, password)
    };

    if password.is_some() && username.is_none() {
        bail!(
            "Attempted to publish with a password, but no username. Either provide a username \
            with `--username` (`UV_PUBLISH_USERNAME`), or use `--token` (`UV_PUBLISH_TOKEN`) \
            instead of a password."
        );
    }

    if username.is_none()
        && password.is_none()
        && keyring_provider == KeyringProviderType::Disabled
        && let Some(err) = trusted_publishing_status
    {
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
        write_error_chain_with_options(
            anyhow::Error::from(err)
                .context("Trusted publishing failed")
                .as_ref(),
            Hints::none(),
            ErrorOptions::default().with_stream(printer.stderr()),
        )?;
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

    Ok((publish_url, PublishingCredentials::Supplied(credentials)))
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
        let client = BaseClientBuilder::default().build()?;
        gather_credentials(
            url,
            username,
            password,
            TrustedPublishing::Never,
            KeyringProviderType::Disabled,
            &client,
            None,
            Prompt::Disabled,
            Printer::Quiet,
        )
        .await
        .map(|(publish_url, credentials)| (publish_url, credentials.as_credentials().into_owned()))
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
}
