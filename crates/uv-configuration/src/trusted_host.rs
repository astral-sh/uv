use serde::{Deserialize, Serialize};
use url::Url;

/// A trusted host, which could be a host or a host-port pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedHost {
    scheme: Option<String>,
    host: String,
    port: Option<u16>,
}

impl TrustedHost {
    /// Returns `true` if the [`Url`] matches this trusted host.
    pub fn matches(&self, url: &Url) -> bool {
        if self
            .scheme
            .as_ref()
            .is_some_and(|scheme| scheme != url.scheme())
        {
            return false;
        }

        if self.port.is_some_and(|port| url.port() != Some(port)) {
            return false;
        }

        if Some(self.host.as_ref()) != url.host_str() {
            return false;
        }

        true
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
        // Detect scheme.
        let (scheme, s) = if let Some(s) = s.strip_prefix("https://") {
            (Some("https".to_string()), s)
        } else if let Some(s) = s.strip_prefix("http://") {
            (Some("http".to_string()), s)
        } else {
            (None, s)
        };

        let mut parts = s.splitn(2, ':');

        // Detect host.
        let host = parts
            .next()
            .and_then(|host| host.split('/').next())
            .map(ToString::to_string)
            .ok_or_else(|| TrustedHostError::MissingHost(s.to_string()))?;

        // Detect port.
        let port = parts
            .next()
            .map(str::parse)
            .transpose()
            .map_err(|_| TrustedHostError::InvalidPort(s.to_string()))?;

        Ok(Self { scheme, host, port })
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
            super::TrustedHost {
                scheme: None,
                host: "example.com".to_string(),
                port: None
            }
        );

        assert_eq!(
            "example.com:8080".parse::<super::TrustedHost>().unwrap(),
            super::TrustedHost {
                scheme: None,
                host: "example.com".to_string(),
                port: Some(8080)
            }
        );

        assert_eq!(
            "https://example.com".parse::<super::TrustedHost>().unwrap(),
            super::TrustedHost {
                scheme: Some("https".to_string()),
                host: "example.com".to_string(),
                port: None
            }
        );

        assert_eq!(
            "https://example.com/hello/world"
                .parse::<super::TrustedHost>()
                .unwrap(),
            super::TrustedHost {
                scheme: Some("https".to_string()),
                host: "example.com".to_string(),
                port: None
            }
        );
    }
}
