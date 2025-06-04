use serde::{Deserialize, Deserializer};
use std::str::FromStr;
use url::Url;

/// A host specification (wildcard, or host, with optional scheme and/or port) for which
/// certificates are not verified when making HTTPS requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustedHost {
    Wildcard,
    Host {
        scheme: Option<String>,
        host: String,
        port: Option<u16>,
    },
}

impl TrustedHost {
    /// Returns `true` if the [`Url`] matches this trusted host.
    pub fn matches(&self, url: &Url) -> bool {
        match self {
            TrustedHost::Wildcard => true,
            TrustedHost::Host { scheme, host, port } => {
                if scheme.as_ref().is_some_and(|scheme| scheme != url.scheme()) {
                    return false;
                }

                if port.is_some_and(|port| url.port() != Some(port)) {
                    return false;
                }

                if Some(host.as_str()) != url.host_str() {
                    return false;
                }

                true
            }
        }
    }
}

impl<'de> Deserialize<'de> for TrustedHost {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Inner {
            scheme: Option<String>,
            host: String,
            port: Option<u16>,
        }

        serde_untagged::UntaggedEnumVisitor::new()
            .string(|string| TrustedHost::from_str(string).map_err(serde::de::Error::custom))
            .map(|map| {
                map.deserialize::<Inner>().map(|inner| TrustedHost::Host {
                    scheme: inner.scheme,
                    host: inner.host,
                    port: inner.port,
                })
            })
            .deserialize(deserializer)
    }
}

impl serde::Serialize for TrustedHost {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let s = self.to_string();
        serializer.serialize_str(&s)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TrustedHostError {
    #[error("missing host for `--trusted-host`: `{0}`")]
    MissingHost(String),
    #[error("invalid port for `--trusted-host`: `{0}`")]
    InvalidPort(String),
}

impl FromStr for TrustedHost {
    type Err = TrustedHostError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "*" {
            return Ok(Self::Wildcard);
        }

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

        Ok(Self::Host { scheme, host, port })
    }
}

impl std::fmt::Display for TrustedHost {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TrustedHost::Wildcard => {
                write!(f, "*")?;
            }
            TrustedHost::Host { scheme, host, port } => {
                if let Some(scheme) = &scheme {
                    write!(f, "{scheme}://{host}")?;
                } else {
                    write!(f, "{host}")?;
                }

                if let Some(port) = port {
                    write!(f, ":{port}")?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for TrustedHost {
    fn schema_name() -> String {
        "TrustedHost".to_string()
    }

    fn json_schema(_gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
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
            "*".parse::<super::TrustedHost>().unwrap(),
            super::TrustedHost::Wildcard
        );

        assert_eq!(
            "example.com".parse::<super::TrustedHost>().unwrap(),
            super::TrustedHost::Host {
                scheme: None,
                host: "example.com".to_string(),
                port: None
            }
        );

        assert_eq!(
            "example.com:8080".parse::<super::TrustedHost>().unwrap(),
            super::TrustedHost::Host {
                scheme: None,
                host: "example.com".to_string(),
                port: Some(8080)
            }
        );

        assert_eq!(
            "https://example.com".parse::<super::TrustedHost>().unwrap(),
            super::TrustedHost::Host {
                scheme: Some("https".to_string()),
                host: "example.com".to_string(),
                port: None
            }
        );

        assert_eq!(
            "https://example.com/hello/world"
                .parse::<super::TrustedHost>()
                .unwrap(),
            super::TrustedHost::Host {
                scheme: Some("https".to_string()),
                host: "example.com".to_string(),
                port: None
            }
        );
    }
}
