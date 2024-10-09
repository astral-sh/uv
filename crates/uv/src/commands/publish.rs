use crate::commands::reporters::PublishReporter;
use crate::commands::{human_readable_bytes, ExitStatus};
use crate::printer::Printer;
use anyhow::{bail, Result};
use owo_colors::OwoColorize;
use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;
use url::Url;
use uv_client::{AuthIntegration, BaseClientBuilder, Connectivity, DEFAULT_RETRIES};
use uv_configuration::{KeyringProviderType, TrustedHost, TrustedPublishing};
use uv_publish::{check_trusted_publishing, files_for_publishing, upload};

pub(crate) async fn publish(
    paths: Vec<String>,
    publish_url: Url,
    trusted_publishing: TrustedPublishing,
    keyring_provider: KeyringProviderType,
    allow_insecure_host: Vec<TrustedHost>,
    username: Option<String>,
    password: Option<String>,
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
        .allow_insecure_host(allow_insecure_host)
        // Don't try cloning the request to make an unauthenticated request first.
        .auth_integration(AuthIntegration::OnlyAuthenticated)
        // Set a very high timeout for uploads, connections are often 10x slower on upload than
        // download. 15 min is taken from the time a trusted publishing token is valid.
        .default_timeout(Duration::from_secs(15 * 60))
        .build();
    let oidc_client = BaseClientBuilder::new()
        .auth_integration(AuthIntegration::NoAuthMiddleware)
        .wrap_existing(&upload_client);

    // If applicable, attempt obtaining a token for trusted publishing.
    let trusted_publishing_token = check_trusted_publishing(
        username.as_deref(),
        password.as_deref(),
        keyring_provider,
        trusted_publishing,
        &publish_url,
        &oidc_client.client(),
    )
    .await?;
    let (username, password) = if let Some(password) = trusted_publishing_token {
        (Some("__token__".to_string()), Some(password.into()))
    } else {
        (username, password)
    };

    for (file, filename) in files {
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
            &filename,
            &publish_url,
            &upload_client.client(),
            DEFAULT_RETRIES,
            username.as_deref(),
            password.as_deref(),
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
