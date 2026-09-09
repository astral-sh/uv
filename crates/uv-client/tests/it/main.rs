mod cached_client;
mod http_util;
mod proxy;
mod remote_metadata;
// The certificate tests exercise uv's custom-certificate handling and a rustls-based test server,
// both of which are only available with the `rustls-tls` backend.
#[cfg(feature = "rustls-tls")]
mod ssl_certs;
mod user_agent_version;
