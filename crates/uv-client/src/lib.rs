pub use error::{Error, ErrorKind};
pub use flat_index::{FlatIndexClient, FlatIndexEntries, FlatIndexEntry, FlatIndexError};
pub use registry_client::{
    MetadataFormat, RegistryClient, RegistryClientBuilder, SimpleDetailMetadata,
    SimpleDetailMetadatum, SimpleIndexMetadata, VersionFiles,
};
pub use uv_http::{
    AuthIntegration, BaseClient, BaseClientBuilder, CacheControl, CachedClient, CachedClientError,
    CertificateFileError, Certificates, ClientBuildError, Connectivity, DEFAULT_CONNECT_TIMEOUT,
    DEFAULT_MAX_REDIRECTS, DEFAULT_READ_TIMEOUT, DEFAULT_READ_TIMEOUT_UPLOAD, DEFAULT_RETRIES,
    DataWithCachePolicy, ExtraMiddleware, MetadataRangeRequest, OwnedArchive, ProblemDetails,
    RedirectClientWithMiddleware, RedirectPolicy, RequestBuilder, RetriableError,
    RetryParsingError, RetryState, WrappedReqwestError, fetch_with_url_fallback,
    retryable_on_request_failure,
};

mod error;
mod flat_index;
mod html;
mod registry_client;
mod remote_metadata;
