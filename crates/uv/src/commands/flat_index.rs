use uv_cache::Cache;
use uv_client::{FlatIndexClient, FlatIndexError, RegistryClient};
use uv_distribution_types::{Index, IndexLocations};
use uv_resolver::FlatIndex;

/// Load the `--find-links` entries for a command's configured indexes.
pub(crate) async fn resolve_flat_index(
    client: &RegistryClient,
    cache: &Cache,
    index_locations: &IndexLocations,
) -> Result<FlatIndex, FlatIndexError> {
    let client = FlatIndexClient::new(client.cached_client(), client.connectivity(), cache);
    let entries = client
        .fetch_all(index_locations.flat_indexes().map(Index::url))
        .await?;
    Ok(FlatIndex::from_entries(entries))
}
