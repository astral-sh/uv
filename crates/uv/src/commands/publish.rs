use crate::commands::reporters::PublishReporter;
use crate::commands::{human_readable_bytes, ExitStatus};
use crate::printer::Printer;
use anyhow::{bail, Context, Result};
use console::Term;
use owo_colors::OwoColorize;
use std::fmt::Write;
use std::iter;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;
use url::Url;
use uv_cache::Cache;
use uv_client::{AuthIntegration, BaseClientBuilder, Connectivity, RegistryClientBuilder};
use uv_configuration::{KeyringProviderType, TrustedHost, TrustedPublishing};
use uv_distribution_types::{Index, IndexCapabilities, IndexLocations, IndexUrl};
use uv_publish::{
    check_trusted_publishing, files_for_publishing, upload, CheckUrlClient, TrustedPublishResult,
};

pub(crate) async fn publish(
    paths: Vec<String>,
    publish_url: Url,
    trusted_publishing: TrustedPublishing,
    keyring_provider: KeyringProviderType,
    allow_insecure_host: &[TrustedHost],
    username: Option<String>,
    password: Option<String>,
    check_url: Option<IndexUrl>,
    cache: &Cache,
    connectivity: Connectivity,
    native_tls: bool,
    printer: Printer,
) -> Result<ExitStatus> {
    if connectivity.is_offline() {
        bail!("Unable to publish files in offline mode");
    }

    let files = files_for_publishing(paths)?;
    match files.len() {
        0 => bail!("No files found to publish"),
        1 => writeln!(printer.stderr(), "Publishing 1 file to {publish_url}")?,
        n => writeln!(printer.stderr(), "Publishing {n} files {publish_url}")?,
    }

    // * For the uploads themselves, we roll our own retries due to
    //   https://github.com/seanmonstar/reqwest/issues/2416, but for trusted publishing, we want
    //   the default retries.
    // * We want to allow configuring TLS for the registry, while for trusted publishing we know the
    //   defaults are correct.
    // * For the uploads themselves, we know we need an authorization header and we can't nor
    //   shouldn't try cloning the request to make an unauthenticated request first, but we want
    //   keyring integration. For trusted publishing, we use an OIDC auth routine without keyring
    //   or other auth integration.
    let upload_client = BaseClientBuilder::new()
        .retries(0)
        .keyring(keyring_provider)
        .native_tls(native_tls)
        .allow_insecure_host(allow_insecure_host.to_vec())
        // Don't try cloning the request to make an unauthenticated request first.
        .auth_integration(AuthIntegration::OnlyAuthenticated)
        // Set a very high timeout for uploads, connections are often 10x slower on upload than
        // download. 15 min is taken from the time a trusted publishing token is valid.
        .default_timeout(Duration::from_secs(15 * 60))
        .build();
    let oidc_client = BaseClientBuilder::new()
        .auth_integration(AuthIntegration::NoAuthMiddleware)
        .wrap_existing(&upload_client);

    // Initialize the registry client.
    let check_url_client = if let Some(index_url) = check_url {
        let index_urls = IndexLocations::new(
            vec![Index::from_index_url(index_url.clone())],
            Vec::new(),
            false,
        )
        .index_urls();
        let registry_client_builder = RegistryClientBuilder::new(cache.clone())
            .native_tls(native_tls)
            .connectivity(connectivity)
            .index_urls(index_urls)
            .keyring(keyring_provider)
            .allow_insecure_host(allow_insecure_host.to_vec());
        Some(CheckUrlClient {
            index_url,
            registry_client_builder,
            client: &upload_client,
            index_capabilities: IndexCapabilities::default(),
            cache,
        })
    } else {
        None
    };

    // If applicable, attempt obtaining a token for trusted publishing.
    let trusted_publishing_token = check_trusted_publishing(
        username.as_deref(),
        password.as_deref(),
        keyring_provider,
        trusted_publishing,
        &publish_url,
        &oidc_client,
    )
    .await?;

    let (username, password) =
        if let TrustedPublishResult::Configured(password) = &trusted_publishing_token {
            (Some("__token__".to_string()), Some(password.to_string()))
        } else {
            if username.is_none() && password.is_none() {
                prompt_username_and_password()?
            } else {
                (username, password)
            }
        };

    if password.is_some() && username.is_none() {
        bail!(
            "Attempted to publish with a password, but no username. Either provide a username \
            with `--user` (`UV_PUBLISH_USERNAME`), or use `--token` (`UV_PUBLISH_TOKEN`) instead \
            of a password."
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
            writeln!(
                printer.stderr(),
                "{}: {err}",
                "Trusted publishing error".red().bold()
            )?;
            for source in iter::successors(std::error::Error::source(&err), |&err| err.source()) {
                writeln!(
                    printer.stderr(),
                    "  {}: {}",
                    "Caused by".red().bold(),
                    source.to_string().trim()
                )?;
            }
        }
    }

    for (file, raw_filename, filename) in files {
        if let Some(check_url_client) = &check_url_client {
            if uv_publish::check_url(check_url_client, &file, &filename).await? {
                writeln!(printer.stderr(), "File {filename} already exists, skipping")?;
                continue;
            }
        }

        let size = fs_err::metadata(&file)?.len();
        let (bytes, unit) = human_readable_bytes(size);
        writeln!(
            printer.stderr(),
            "{} {filename} {}",
            "Uploading".bold().green(),
            format!("({bytes:.1}{unit})").dimmed()
        )?;
        let reporter = PublishReporter::single(printer);
        let uploaded = upload(
            &file,
            &raw_filename,
            &filename,
            &publish_url,
            &upload_client,
            username.as_deref(),
            password.as_deref(),
            check_url_client.as_ref(),
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
