pub use base_client::{
    AuthIntegration, BaseClient, BaseClientBuilder, UvRetryableStrategy, DEFAULT_RETRIES,
};
pub use cached_client::{CacheControl, CachedClient, CachedClientError, DataWithCachePolicy};
pub use error::{Error, ErrorKind, WrappedReqwestError};
pub use flat_index::{FlatIndexClient, FlatIndexEntries, FlatIndexError};
pub use linehaul::LineHaul;
pub use registry_client::{
    Connectivity, RegistryClient, RegistryClientBuilder, SimpleMetadata, SimpleMetadatum,
    VersionFiles,
};
pub use rkyvutil::{Deserializer, OwnedArchive, Serializer, Validator};

mod base_client;
mod cached_client;
mod error;
mod flat_index;
mod html;
mod httpcache;
mod linehaul;
mod middleware;
mod registry_client;
mod remote_metadata;
mod rkyvutil;
mod tls;
