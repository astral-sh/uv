use http::StatusCode;
use rustc_hash::FxHashSet;
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
            .is_some_and(|host| host.contains("pytorch.org"))
        {
            // The PyTorch registry returns a 403 when a package is not found, so
            // we ignore them when deciding whether to search other indexes.
            Self::IgnoreErrorCodes {
                status_codes: FxHashSet::from_iter(vec![StatusCode::FORBIDDEN]),
            }
        } else {
            Self::Default
        }
    }

    /// Derive a strategy from a list of status codes to ignore.
    pub fn from_ignored_error_codes(status_codes: &[u16]) -> Self {
        let status_codes = status_codes
            .iter()
            .map(|code| StatusCode::from_u16(*code))
            .collect::<Result<FxHashSet<_>, _>>()
            .expect("Status codes should be valid.");
        Self::IgnoreErrorCodes { status_codes }
    }

    /// Based on the strategy, decide whether to continue searching the next index
    /// based on the status code returned by this one.
    pub fn handle_status_code(
        &self,
        status_code: StatusCode,
        index_url: &IndexUrl,
        capabilities: &IndexCapabilities,
    ) -> IndexStatusCodeDecision {
        capabilities.set_by_status_code(status_code, index_url);
        match self {
            IndexStatusCodeStrategy::Default => match status_code {
                StatusCode::NOT_FOUND => IndexStatusCodeDecision::Continue,
                StatusCode::UNAUTHORIZED => IndexStatusCodeDecision::Stop,
                StatusCode::FORBIDDEN => IndexStatusCodeDecision::Stop,
                _ => IndexStatusCodeDecision::Stop,
            },
            IndexStatusCodeStrategy::IgnoreErrorCodes { status_codes } => {
                if status_codes.contains(&status_code) {
                    IndexStatusCodeDecision::Continue
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
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum IndexStatusCodeDecision {
    Continue,
    Stop,
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use std::str::FromStr;

    use url::Url;

    use super::*;

    #[test]
    fn test_strategy_normal_registry() -> Result<()> {
        let url = Url::from_str("https://internal-registry.com/simple")?;
        assert_eq!(
            IndexStatusCodeStrategy::from_index_url(&url),
            IndexStatusCodeStrategy::Default
        );
        Ok(())
    }

    #[test]
    fn test_strategy_pytorch_registry() -> Result<()> {
        let status_codes = std::iter::once(StatusCode::FORBIDDEN).collect::<FxHashSet<_>>();
        let url = Url::from_str("https://download.pytorch.org/whl/cu118")?;
        assert_eq!(
            IndexStatusCodeStrategy::from_index_url(&url),
            IndexStatusCodeStrategy::IgnoreErrorCodes { status_codes }
        );
        Ok(())
    }

    #[test]
    fn test_strategy_custom_error_codes() {
        let status_codes =
            FxHashSet::from_iter(vec![StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN]);
        assert_eq!(
            IndexStatusCodeStrategy::from_ignored_error_codes(&[401, 403]),
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
        assert_eq!(decision, IndexStatusCodeDecision::Stop);
    }

    #[test]
    fn test_decision_default_401() {
        let strategy = IndexStatusCodeStrategy::Default;
        let status_code = StatusCode::UNAUTHORIZED;
        let index_url = IndexUrl::parse("https://internal-registry.com/simple", None).unwrap();
        let capabilities = IndexCapabilities::default();
        let decision = strategy.handle_status_code(status_code, &index_url, &capabilities);
        assert_eq!(decision, IndexStatusCodeDecision::Stop);
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
        assert_eq!(decision, IndexStatusCodeDecision::Stop);
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
        assert_eq!(decision, IndexStatusCodeDecision::Continue);
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
        assert_eq!(decision, IndexStatusCodeDecision::Continue);
        // Test we stop on 401 for PyTorch registry.
        let status_code = StatusCode::UNAUTHORIZED;
        let decision = strategy.handle_status_code(status_code, &index_url, &capabilities);
        assert_eq!(decision, IndexStatusCodeDecision::Stop);
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
            assert_eq!(decision, IndexStatusCodeDecision::Continue);
        }
        // Test a status code that's not ignored
        let other_status_code = StatusCode::FORBIDDEN;
        let decision = strategy.handle_status_code(other_status_code, &index_url, &capabilities);
        assert_eq!(decision, IndexStatusCodeDecision::Stop);
    }
}
