pub use base_client::{
    AuthIntegration, BaseClient, BaseClientBuilder, ClientBuildError, DEFAULT_CONNECT_TIMEOUT,
    DEFAULT_MAX_REDIRECTS, DEFAULT_READ_TIMEOUT, DEFAULT_READ_TIMEOUT_UPLOAD, DEFAULT_RETRIES,
    ExtraMiddleware, RedirectClientWithMiddleware, RedirectPolicy, RequestBuilder,
    RetryParsingError, fetch_with_url_fallback,
};
pub use cached_client::{CacheControl, CachedClient, CachedClientError, DataWithCachePolicy};
pub use error::{Error, ErrorKind, ProblemDetails, WrappedReqwestError};
pub use flat_index::{FlatIndexClient, FlatIndexEntries, FlatIndexEntry, FlatIndexError};
pub use linehaul::LineHaul;
pub use registry_client::{
    Connectivity, MetadataFormat, RegistryClient, RegistryClientBuilder, SimpleDetailMetadata,
    SimpleDetailMetadatum, SimpleIndexMetadata, VersionFiles,
};
pub use retry::{RetriableError, RetryState, UvRetryableStrategy, retryable_on_request_failure};
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
mod retry;
mod rkyvutil;
mod tls;
