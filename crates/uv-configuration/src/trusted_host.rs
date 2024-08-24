use serde::{Deserialize, Serialize};
use url::Url;

/// A trusted host, which could be a host or a host-port pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustedHost {
    Host(String),
    HostPort(String, u16),
}

impl TrustedHost {
    pub fn matches(&self, url: &Url) -> bool {
        match self {
            Self::Host(host) => url.host_str() == Some(host.as_str()),
            Self::HostPort(host, port) => {
                url.host_str() == Some(host.as_str()) && url.port() == Some(*port)
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TrustedHostError {
    #[error("missing host for `--trusted-host`: `{0}`")]
    MissingHost(String),
    #[error("invalid port for `--trusted-host`: `{0}`")]
    InvalidPort(String),
}

impl std::str::FromStr for TrustedHost {
    type Err = TrustedHostError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Strip `http://` or `https://`.
        let s = s
            .strip_prefix("https://")
            .unwrap_or_else(|| s.strip_prefix("http://").unwrap_or(s));

        // Split into host and scheme.
        let mut parts = s.splitn(2, ':');
        let host = parts
            .next()
            .ok_or_else(|| TrustedHostError::MissingHost(s.to_string()))?;
        let port = parts
            .next()
            .map(str::parse)
            .transpose()
            .map_err(|_| TrustedHostError::InvalidPort(s.to_string()))?;

        match port {
            Some(port) => Ok(TrustedHost::HostPort(host.to_string(), port)),
            None => Ok(TrustedHost::Host(host.to_string())),
        }
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for TrustedHost {
    fn schema_name() -> String {
        "TrustedHost".to_string()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            metadata: Some(Box::new(schemars::schema::Metadata {
                description: Some("A host or host-port pair.".to_string()),
                ..schemars::schema::Metadata::default()
            })),
            ..schemars::schema::SchemaObject::default()
        }
        .into()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse() {
        assert_eq!(
            "example.com".parse::<super::TrustedHost>().unwrap(),
            super::TrustedHost::Host("example.com".to_string())
        );

        assert_eq!(
            "example.com:8080".parse::<super::TrustedHost>().unwrap(),
            super::TrustedHost::HostPort("example.com".to_string(), 8080)
        );

        assert_eq!(
            "https://example.com".parse::<super::TrustedHost>().unwrap(),
            super::TrustedHost::Host("example.com".to_string())
        );
    }
}
