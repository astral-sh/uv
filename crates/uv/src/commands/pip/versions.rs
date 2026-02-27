use crate::printer::Printer;
use anyhow::{Ok, Result};
use itertools::Itertools;
use rkyv::rancor::Error;
use serde::Serialize;
use std::fmt::Write;
use tokio::sync::Semaphore;
use uv_cache::Cache;
use uv_configuration::{Concurrency, IndexStrategy};
use uv_distribution_types::{IndexCapabilities, IndexLocations};
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_preview::Preview;
use uv_python::{EnvironmentPreference, PythonEnvironment, PythonPreference, PythonRequest};
use uv_resolver::PrereleaseMode;

use crate::commands::{ExitStatus, pip::operations::report_target_environment};
use uv_client::{BaseClientBuilder, RegistryClientBuilder, SimpleDetailMetadatum};

/// do pip index versions but with uv
pub(crate) async fn pip_index_versions(
    package_name: PackageName,
    prerelease: bool,
    json: bool,
    python: Option<&str>,
    client_builder: &BaseClientBuilder<'_>,
    cache: Cache,
    index_locations: IndexLocations,
    index_strategy: IndexStrategy,
    concurrency: Concurrency,
    system: bool,
    printer: Printer,
    preview: Preview,
    // TODO: take more arguments for the client and query
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let environment = PythonEnvironment::find(
        &python.map(PythonRequest::parse).unwrap_or_default(),
        EnvironmentPreference::from_system_flag(system, false),
        PythonPreference::default().with_system_flag(system),
        &cache,
        preview,
    )?;

    report_target_environment(&environment, &cache, printer)?;

    // Build the installed index.
    let site_packages = SitePackages::from_environment(&environment)?;

    let installed_packages = site_packages.get_packages(&package_name);

    let installed_version = if installed_packages.is_empty() {
        None
    } else {
        // TODO: how do we know which one to pick if multiple are installed?
        //  what does it mean to have multiple installed?
        Some(installed_packages[0].version().to_owned())
    };

    let client = RegistryClientBuilder::new(client_builder.clone(), cache)
        .index_locations(index_locations)
        .index_strategy(index_strategy)
        .build();

    let prerelease_mode = if prerelease {
        PrereleaseMode::Allow
    } else {
        PrereleaseMode::Disallow
    };

    let simple_detail = client
        .simple_detail(
            &package_name,
            None,
            &IndexCapabilities::default(),
            &Semaphore::new(concurrency.downloads),
        )
        .await?;

    // TODO: when could this be empty?
    //  if the package doesn't exist we get an error before this point
    if simple_detail.is_empty() {
        writeln!(printer.stderr(), "No data returned from indexes")?;
        return Ok(ExitStatus::Failure);
    }

    // TODO: we should handle multiple index urls here by flattening the results
    let (_index_url, metadata_format) = &simple_detail[0];

    match metadata_format {
        uv_client::MetadataFormat::Flat(_) => return Ok(ExitStatus::Error), // TODO: handle flat metadata
        uv_client::MetadataFormat::Simple(archived_metadata) => {
            let mut versions: Vec<Version> = archived_metadata
                .iter()
                .map(|archived_metadatum| {
                    rkyv::deserialize::<SimpleDetailMetadatum, Error>(archived_metadatum)
                        .expect("archived version files always deserializes")
                })
                .filter(|metadatum| match prerelease_mode {
                    PrereleaseMode::Allow => true,
                    PrereleaseMode::Disallow => !metadatum.version.is_pre(),
                    _ => unreachable!("The only possible PrereleaseModes are Allow and Disallow"),
                })
                .map(|metadatum| metadatum.version)
                .collect();

            versions.sort();
            versions.reverse();

            let max_version = match versions.iter().max() {
                None => {
                    writeln!(
                        printer.stderr(),
                        "ERROR: No matching distribution found for {}",
                        package_name.as_str()
                    )?;
                    return Ok(ExitStatus::Failure);
                }
                Some(max_version) => max_version,
            };

            match json {
                false => {
                    writeln!(
                        printer.stdout(),
                        "{} ({})",
                        package_name.as_str(),
                        max_version.to_string()
                    )?;
                    write!(printer.stdout(), "Available versions: ")?;
                    writeln!(printer.stdout(), "{}", versions.iter().format(", "))?;

                    if let Some(installed_version) = installed_version {
                        writeln!(printer.stdout(), "INSTALLED: {}", installed_version)?;
                        writeln!(printer.stdout(), "LATEST: {}", max_version)?;
                    }
                }
                true => {
                    let output = PipIndexVersionsJsonOutput {
                        name: package_name.to_string(),
                        versions: &versions,
                        latest: max_version,
                        installed_version,
                    };
                    writeln!(printer.stdout(), "{}", serde_json::to_string(&output)?)?;
                }
            }
        }
    }

    return Ok(ExitStatus::Success);
}

#[derive(Serialize, Debug)]
struct PipIndexVersionsJsonOutput<'a> {
    name: String,
    versions: &'a Vec<Version>,
    latest: &'a Version,
    #[serde(skip_serializing_if = "Option::is_none")]
    installed_version: Option<Version>,
}
