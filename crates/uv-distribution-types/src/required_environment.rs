#[cfg(feature = "schemars")]
use std::borrow::Cow;
use std::fmt::Formatter;
use std::str::FromStr;

use serde::de::{MapAccess, SeqAccess, Visitor, value::MapAccessDeserializer};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use uv_pep508::MarkerTree;

use crate::MinimumLibcVersion;

/// An environment that must have compatible artifacts, with optional Linux libc constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequiredEnvironment {
    pub marker: MarkerTree,
    pub minimum_libc_version: Option<MinimumLibcVersion>,
}

impl From<MarkerTree> for RequiredEnvironment {
    fn from(marker: MarkerTree) -> Self {
        Self {
            marker,
            minimum_libc_version: None,
        }
    }
}

impl Serialize for RequiredEnvironment {
    /// Keep unconstrained entries as strings; tables retain libc constraints even for a true marker.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let marker = self.marker.try_to_string().unwrap_or_default();
        if let Some(version) = self.minimum_libc_version {
            let mut table = serializer.serialize_struct("RequiredEnvironment", 2)?;
            table.serialize_field("marker", &marker)?;
            table.serialize_field("minimum-libc-version", &version)?;
            table.end()
        } else {
            serializer.serialize_str(&marker)
        }
    }
}

impl<'de> Deserialize<'de> for RequiredEnvironment {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EnvironmentVisitor;

        impl<'de> Visitor<'de> for EnvironmentVisitor {
            type Value = RequiredEnvironment;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("an environment marker string or table")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(MarkerTree::from_str(value).map_err(E::custom)?.into())
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields, rename_all = "kebab-case")]
                struct Table {
                    marker: MarkerTree,
                    minimum_libc_version: Option<MinimumLibcVersion>,
                }

                let table = Table::deserialize(MapAccessDeserializer::new(map))?;
                Ok(RequiredEnvironment {
                    marker: table.marker,
                    minimum_libc_version: table.minimum_libc_version,
                })
            }
        }

        deserializer.deserialize_any(EnvironmentVisitor)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for RequiredEnvironment {
    fn schema_name() -> Cow<'static, str> {
        "RequiredEnvironment".into()
    }

    fn json_schema(generator: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "anyOf": [
                generator.subschema_for::<MarkerTree>(),
                {
                    "type": "object",
                    "required": ["marker"],
                    "additionalProperties": false,
                    "properties": {
                        "marker": generator.subschema_for::<MarkerTree>(),
                        "minimum-libc-version": generator.subschema_for::<MinimumLibcVersion>(),
                    },
                },
            ],
        })
    }
}

/// Required environments accept a single marker or a list of markers and environment tables.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct RequiredEnvironments(Vec<RequiredEnvironment>);

impl RequiredEnvironments {
    pub fn from_environments(environments: Vec<RequiredEnvironment>) -> Self {
        Self(environments)
    }

    pub fn as_slice(&self) -> &[RequiredEnvironment] {
        &self.0
    }

    pub fn iter(&self) -> std::slice::Iter<'_, RequiredEnvironment> {
        self.0.iter()
    }

    pub fn has_libc_constraints(&self) -> bool {
        self.0
            .iter()
            .any(|environment| environment.minimum_libc_version.is_some())
    }
}

impl<'a> IntoIterator for &'a RequiredEnvironments {
    type Item = &'a RequiredEnvironment;
    type IntoIter = std::slice::Iter<'a, RequiredEnvironment>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'de> Deserialize<'de> for RequiredEnvironments {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EnvironmentsVisitor;

        impl<'de> Visitor<'de> for EnvironmentsVisitor {
            type Value = RequiredEnvironments;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("an environment marker string or a list of strings and tables")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(RequiredEnvironments(vec![
                    MarkerTree::from_str(value).map_err(E::custom)?.into(),
                ]))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut environments = Vec::new();
                while let Some(environment) = seq.next_element()? {
                    environments.push(environment);
                }
                Ok(RequiredEnvironments(environments))
            }
        }

        deserializer.deserialize_any(EnvironmentsVisitor)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for RequiredEnvironments {
    fn schema_name() -> Cow<'static, str> {
        "RequiredEnvironments".into()
    }

    fn json_schema(generator: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "anyOf": [
                generator.subschema_for::<MarkerTree>(),
                generator.subschema_for::<Vec<RequiredEnvironment>>(),
            ],
        })
    }
}
