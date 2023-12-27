pub use cached_client::{CachedClient, CachedClientError, DataWithCachePolicy};
pub use error::Error;
pub use registry_client::{
    read_metadata_async, RegistryClient, RegistryClientBuilder, SimpleMetadata, VersionFiles,
};

mod cached_client;
mod error;
mod html;
mod registry_client;
mod remote_metadata;
