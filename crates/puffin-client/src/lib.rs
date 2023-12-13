pub use cached_client::{CachedClient, CachedClientError, DataWithCachePolicy};
pub use error::Error;
pub use registry_client::{
    read_metadata_async, RegistryClient, RegistryClientBuilder, SimpleMetadata,
};

mod cached_client;
mod error;
mod registry_client;
mod remote_metadata;
