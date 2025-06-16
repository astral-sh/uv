use std::ops::Deref;

use http::StatusCode;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use url::Url;

use crate::{IndexCapabilities, IndexUrl};

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub enum IndexStatusCodeStrategy {
    #[default]
    Default,
    IgnoreErrorCodes {
        status_codes: FxHashSet<StatusCode>,
    },
}

impl IndexStatusCodeStrategy {
    /// Derive a strategy from an index URL. We special-case PyTorch. Otherwise,
    /// we follow the default strategy.
    pub fn from_index_url(url: &Url) -> Self {
        if url
            .host_str()
            .is_some_and(|host| host.ends_with("pytorch.org"))
        {
            // The PyTorch registry returns a 403 when a package is not found, so
            // we ignore them when deciding whether to search other indexes.
            Self::IgnoreErrorCodes {
                status_codes: FxHashSet::from_iter([StatusCode::FORBIDDEN]),
            }
        } else {
            Self::Default
        }
    }

    /// Derive a strategy from a list of status codes to ignore.
    pub fn from_ignored_error_codes(status_codes: &[SerializableStatusCode]) -> Self {
        Self::IgnoreErrorCodes {
            status_codes: status_codes
                .iter()
                .map(SerializableStatusCode::deref)
                .copied()
                .collect::<FxHashSet<_>>(),
        }
    }

    /// Derive a strategy for ignoring authentication error codes.
    pub fn ignore_authentication_error_codes() -> Self {
        Self::IgnoreErrorCodes {
            status_codes: FxHashSet::from_iter([
                StatusCode::UNAUTHORIZED,
                StatusCode::FORBIDDEN,
                StatusCode::NETWORK_AUTHENTICATION_REQUIRED,
                StatusCode::PROXY_AUTHENTICATION_REQUIRED,
            ]),
        }
    }

    /// Based on the strategy, decide whether to continue searching the next index
    /// based on the status code returned by this one.
    pub fn handle_status_code(
        &self,
        status_code: StatusCode,
        index_url: &IndexUrl,
        capabilities: &IndexCapabilities,
    ) -> IndexStatusCodeDecision {
        match self {
            IndexStatusCodeStrategy::Default => match status_code {
                StatusCode::NOT_FOUND => IndexStatusCodeDecision::Ignore,
                StatusCode::UNAUTHORIZED => {
                    capabilities.set_unauthorized(index_url.clone());
                    IndexStatusCodeDecision::Fail(status_code)
                }
                StatusCode::FORBIDDEN => {
                    capabilities.set_forbidden(index_url.clone());
                    IndexStatusCodeDecision::Fail(status_code)
                }
                _ => IndexStatusCodeDecision::Fail(status_code),
            },
            IndexStatusCodeStrategy::IgnoreErrorCodes { status_codes } => {
                if status_codes.contains(&status_code) {
                    IndexStatusCodeDecision::Ignore
                } else {
                    IndexStatusCodeStrategy::Default.handle_status_code(
                        status_code,
                        index_url,
                        capabilities,
                    )
                }
            }
        }
    }
}

/// Decision on whether to continue searching the next index.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum IndexStatusCodeDecision {
    Ignore,
    Fail(StatusCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SerializableStatusCode(StatusCode);

impl Deref for SerializableStatusCode {
    type Target = StatusCode;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Serialize for SerializableStatusCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(self.0.as_u16())
    }
}

impl<'de> Deserialize<'de> for SerializableStatusCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let code = u16::deserialize(deserializer)?;
        StatusCode::from_u16(code)
            .map(SerializableStatusCode)
            .map_err(|_| {
                serde::de::Error::custom(format!("{code} is not a valid HTTP status code"))
            })
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for SerializableStatusCode {
    fn schema_name() -> String {
        "StatusCode".to_string()
    }

    fn json_schema(r#gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        let mut schema = r#gen.subschema_for::<u16>().into_object();
        schema.metadata().description = Some("HTTP status code (100-599)".to_string());
        schema.number().minimum = Some(100.0);
        schema.number().maximum = Some(599.0);

        schema.into()
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use url::Url;

    use super::*;

    #[test]
    fn test_strategy_normal_registry() {
        let url = Url::from_str("https://internal-registry.com/simple").unwrap();
        assert_eq!(
            IndexStatusCodeStrategy::from_index_url(&url),
            IndexStatusCodeStrategy::Default
        );
    }

    #[test]
    fn test_strategy_pytorch_registry() {
        let status_codes = std::iter::once(StatusCode::FORBIDDEN).collect::<FxHashSet<_>>();
        let url = Url::from_str("https://download.pytorch.org/whl/cu118").unwrap();
        assert_eq!(
            IndexStatusCodeStrategy::from_index_url(&url),
            IndexStatusCodeStrategy::IgnoreErrorCodes { status_codes }
        );
    }

    #[test]
    fn test_strategy_custom_error_codes() {
        let status_codes = FxHashSet::from_iter([StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN]);
        let serializable_status_codes = status_codes
            .iter()
            .map(|code| SerializableStatusCode(*code))
            .collect::<Vec<_>>();
        assert_eq!(
            IndexStatusCodeStrategy::from_ignored_error_codes(&serializable_status_codes),
            IndexStatusCodeStrategy::IgnoreErrorCodes { status_codes }
        );
    }

    #[test]
    fn test_decision_default_400() {
        let strategy = IndexStatusCodeStrategy::Default;
        let status_code = StatusCode::BAD_REQUEST;
        let index_url = IndexUrl::parse("https://internal-registry.com/simple", None).unwrap();
        let capabilities = IndexCapabilities::default();
        let decision = strategy.handle_status_code(status_code, &index_url, &capabilities);
        assert_eq!(
            decision,
            IndexStatusCodeDecision::Fail(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn test_decision_default_401() {
        let strategy = IndexStatusCodeStrategy::Default;
        let status_code = StatusCode::UNAUTHORIZED;
        let index_url = IndexUrl::parse("https://internal-registry.com/simple", None).unwrap();
        let capabilities = IndexCapabilities::default();
        let decision = strategy.handle_status_code(status_code, &index_url, &capabilities);
        assert_eq!(
            decision,
            IndexStatusCodeDecision::Fail(StatusCode::UNAUTHORIZED)
        );
        assert!(capabilities.unauthorized(&index_url));
        assert!(!capabilities.forbidden(&index_url));
    }

    #[test]
    fn test_decision_default_403() {
        let strategy = IndexStatusCodeStrategy::Default;
        let status_code = StatusCode::FORBIDDEN;
        let index_url = IndexUrl::parse("https://internal-registry.com/simple", None).unwrap();
        let capabilities = IndexCapabilities::default();
        let decision = strategy.handle_status_code(status_code, &index_url, &capabilities);
        assert_eq!(
            decision,
            IndexStatusCodeDecision::Fail(StatusCode::FORBIDDEN)
        );
        assert!(capabilities.forbidden(&index_url));
        assert!(!capabilities.unauthorized(&index_url));
    }

    #[test]
    fn test_decision_default_404() {
        let strategy = IndexStatusCodeStrategy::Default;
        let status_code = StatusCode::NOT_FOUND;
        let index_url = IndexUrl::parse("https://internal-registry.com/simple", None).unwrap();
        let capabilities = IndexCapabilities::default();
        let decision = strategy.handle_status_code(status_code, &index_url, &capabilities);
        assert_eq!(decision, IndexStatusCodeDecision::Ignore);
        assert!(!capabilities.forbidden(&index_url));
        assert!(!capabilities.unauthorized(&index_url));
    }

    #[test]
    fn test_decision_pytorch() {
        let index_url = IndexUrl::parse("https://download.pytorch.org/whl/cu118", None).unwrap();
        let strategy = IndexStatusCodeStrategy::from_index_url(&index_url);
        let capabilities = IndexCapabilities::default();
        // Test we continue on 403 for PyTorch registry.
        let status_code = StatusCode::FORBIDDEN;
        let decision = strategy.handle_status_code(status_code, &index_url, &capabilities);
        assert_eq!(decision, IndexStatusCodeDecision::Ignore);
        // Test we stop on 401 for PyTorch registry.
        let status_code = StatusCode::UNAUTHORIZED;
        let decision = strategy.handle_status_code(status_code, &index_url, &capabilities);
        assert_eq!(
            decision,
            IndexStatusCodeDecision::Fail(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn test_decision_multiple_ignored_status_codes() {
        let status_codes = vec![
            StatusCode::UNAUTHORIZED,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
        ];
        let strategy = IndexStatusCodeStrategy::IgnoreErrorCodes {
            status_codes: status_codes.iter().copied().collect::<FxHashSet<_>>(),
        };
        let index_url = IndexUrl::parse("https://internal-registry.com/simple", None).unwrap();
        let capabilities = IndexCapabilities::default();
        // Test each ignored status code
        for status_code in status_codes {
            let decision = strategy.handle_status_code(status_code, &index_url, &capabilities);
            assert_eq!(decision, IndexStatusCodeDecision::Ignore);
        }
        // Test a status code that's not ignored
        let other_status_code = StatusCode::FORBIDDEN;
        let decision = strategy.handle_status_code(other_status_code, &index_url, &capabilities);
        assert_eq!(
            decision,
            IndexStatusCodeDecision::Fail(StatusCode::FORBIDDEN)
        );
    }
}
