pub(crate) mod http;
pub(crate) mod retry;

/// Configuration for `ssl-version` in `http` section
/// There are two ways to configure:
///
/// ```text
/// [http]
/// ssl-version = "tlsv1.3"
/// ```
///
/// ```text
/// [http]
/// ssl-version.min = "tlsv1.2"
/// ssl-version.max = "tlsv1.3"
/// ```
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SslVersionConfig {
    Single(String),
    Range(SslVersionConfigRange),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SslVersionConfigRange {
    pub min: Option<String>,
    pub max: Option<String>,
}
