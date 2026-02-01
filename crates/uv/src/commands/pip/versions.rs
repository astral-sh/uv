use crate::printer::Printer;
use anyhow::Result;
use itertools::Itertools;
use rkyv::rancor::Error;
use serde::Serialize;
use std::fmt::Write;
use tokio::sync::Semaphore;
use uv_cache::Cache;
use uv_configuration::IndexStrategy;
use uv_distribution_types::{IndexCapabilities, IndexLocations, InstalledDist};
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

    let installed = site_packages.get_packages(&package_name);
    let installed_version: Option<Version>;
    if installed.is_empty() {
        installed_version = None;
    } else {
        // TODO: don't assume the first version is the right one.
        installed_version = Some(installed[0].version().to_owned());
    }

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
            &Semaphore::new(1),
        )
        .await?;

    if simple_detail.is_empty() {
        println!("No versions found");
        return Ok(ExitStatus::Failure);
    }

    let (_index_url, metadata_format) = &simple_detail[0];

    match metadata_format {
        uv_client::MetadataFormat::Flat(_) => return Ok(ExitStatus::Error), // TODO: handle flat metadata
        uv_client::MetadataFormat::Simple(archived_metadata) => {
            let versions: Vec<Version> = archived_metadata
                .iter()
                .map(|archived_metadatum| {
                    rkyv::deserialize::<SimpleDetailMetadatum, Error>(archived_metadatum).unwrap() // TODO: don't unwrap, do this properly
                })
                .filter(|metadatum| match prerelease_mode {
                    PrereleaseMode::Allow => true,
                    PrereleaseMode::Disallow => !metadatum.version.is_pre(),
                    _ => unreachable!("The only possible PrereleaseModes are Allow and Disallow"),
                })
                .map(|metadatum| metadatum.version)
                // TODO: we need to ensure they are in descending order
                .collect();

            let max_version = versions.iter().max().unwrap(); // TODO: this panics when there are no versions - the simple_detail.is_empty() above doesn't prevent this.

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
                    if installed_version.is_some() {
                        writeln!(
                            printer.stdout(),
                            "INSTALLED: {}",
                            installed_version.unwrap()
                        )?;
                        writeln!(printer.stdout(), "LATEST: {}", max_version)?;
                    }
                }
                true => {
                    let output = PipIndexVersionsJsonOutput {
                        name: package_name.to_string(),
                        // TODO: why do I need to own these?
                        versions: versions.to_owned(),
                        latest: max_version.to_owned(),
                        installed_version,
                    };
                    writeln!(
                        printer.stdout(),
                        "{:}",
                        serde_json::to_string(&output).unwrap()
                    )?;
                }
            }
        }
    }

    return Ok(ExitStatus::Success);
}

#[derive(Serialize, Debug)]
struct PipIndexVersionsJsonOutput {
    name: String,
    versions: Vec<Version>,
    latest: Version,
    installed_version: Option<Version>,
}
