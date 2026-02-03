pub use base_client::{
    AuthIntegration, BaseClient, BaseClientBuilder, DEFAULT_CONNECT_TIMEOUT, DEFAULT_MAX_REDIRECTS,
    DEFAULT_READ_TIMEOUT, DEFAULT_RETRIES, DEFAULT_UPLOAD_TIMEOUT, ExtraMiddleware,
    RedirectClientWithMiddleware, RedirectPolicy, RequestBuilder, RetryParsingError, RetryState,
    UvRetryableStrategy,
};
pub use cached_client::{CacheControl, CachedClient, CachedClientError, DataWithCachePolicy};
pub use error::{Error, ErrorKind, ProblemDetails, WrappedReqwestError};
pub use flat_index::{FlatIndexClient, FlatIndexEntries, FlatIndexEntry, FlatIndexError};
pub use linehaul::LineHaul;
pub use registry_client::{
    Connectivity, MetadataFormat, RegistryClient, RegistryClientBuilder, SimpleDetailMetadata,
    SimpleDetailMetadatum, SimpleIndexMetadata, VersionFiles,
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
