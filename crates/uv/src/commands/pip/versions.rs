use anyhow::Result;
use tokio::sync::Semaphore;
use uv_auth::KeyringProvider;
use uv_cache::Cache;
use uv_configuration::IndexStrategy;
use uv_distribution_types::{IndexCapabilities, IndexLocations};
use uv_normalize::PackageName;

use crate::commands::ExitStatus;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};

/// do pip index versions but with uv
pub(crate) async fn pip_index_versions(
    package_name: PackageName,
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
            for version_datum in archived_metadata.iter() {
                // TODO: unarchive the metadata so we can actually understand the output
                println!("Version: {:?}", version_datum.version);
            }
        }
    }

    return Ok(ExitStatus::Success);
}
