use anyhow::Result;
use itertools::Itertools;
use rkyv::rancor::Error;
use tokio::sync::Semaphore;
use uv_cache::Cache;
use uv_configuration::IndexStrategy;
use uv_distribution_types::{IndexCapabilities, IndexLocations};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_resolver::PrereleaseMode;

use crate::commands::ExitStatus;
use uv_client::{BaseClientBuilder, RegistryClientBuilder, SimpleDetailMetadatum};

/// do pip index versions but with uv
pub(crate) async fn pip_index_versions(
    package_name: PackageName,
    prerelease: bool,
    json: bool,
    client_builder: &BaseClientBuilder<'_>,
    cache: Cache,
    index_locations: IndexLocations,
    index_strategy: IndexStrategy,
    // TODO: take more arguments for the client and query
) -> Result<ExitStatus> {
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
                    println!("{} ({})", package_name.as_str(), max_version.to_string());
                    print!("Available versions: ");
                    println!("{}", versions.iter().format(", "))
                }
                true => {
                    unimplemented!("This is next on my list!")
                }
            }
        }
    }

    return Ok(ExitStatus::Success);
}
