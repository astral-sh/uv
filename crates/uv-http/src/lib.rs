pub use base_client::{
    AuthIntegration, BaseClient, BaseClientBuilder, CertificateSource, ClientBuildError,
    DEFAULT_CONNECT_TIMEOUT, DEFAULT_MAX_REDIRECTS, DEFAULT_READ_TIMEOUT,
    DEFAULT_READ_TIMEOUT_UPLOAD, DEFAULT_RETRIES, ExtraMiddleware, RedirectClientWithMiddleware,
    RedirectPolicy, RequestBuilder, RetryParsingError, fetch_with_url_fallback,
};
pub use cached_client::{
    CacheControl, Cacheable, CachedClient, CachedClientError, DataWithCachePolicy,
};
pub use connectivity::Connectivity;
pub use error::{Error, ErrorKind, ProblemDetails, WrappedReqwestError};
pub(crate) use retry::UvRetryableStrategy;
pub use retry::{RetriableError, RetryState, retryable_on_request_failure};
pub use rkyvutil::OwnedArchive;
pub use tls::{CertificateFileError, Certificates};
pub use uv_configuration::MetadataRangeRequest;

mod base_client;
mod cached_client;
mod connectivity;
mod error;
mod httpcache;
mod linehaul;
mod middleware;
mod retry;
mod rkyvutil;
mod tls;
