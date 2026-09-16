#[cfg(feature = "schemars")]
use std::borrow::Cow;
use std::fmt::Formatter;
use std::str::FromStr;

use serde::de::{MapAccess, SeqAccess, Visitor, value::MapAccessDeserializer};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use uv_pep508::MarkerTree;

use crate::MinimumLibcVersion;

/// An environment marker with optional Linux libc constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Environment {
    pub marker: MarkerTree,
    pub libc: Option<MinimumLibcVersion>,
}

impl From<MarkerTree> for Environment {
    fn from(marker: MarkerTree) -> Self {
        Self { marker, libc: None }
    }
}

impl Serialize for Environment {
    /// Keep unconstrained entries as strings; tables retain libc constraints even for a true marker.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let marker = self
            .marker
            .try_to_string()
            .unwrap_or_else(|| "python_version >= '0'".to_string());
        if let Some(version) = self.libc {
            let mut table = serializer.serialize_struct("Environment", 2)?;
            table.serialize_field("marker", &marker)?;
            table.serialize_field("libc", &version)?;
            table.end()
        } else {
            serializer.serialize_str(&marker)
        }
    }
}

impl<'de> Deserialize<'de> for Environment {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EnvironmentVisitor;

        impl<'de> Visitor<'de> for EnvironmentVisitor {
            type Value = Environment;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("an environment marker string or table")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(MarkerTree::from_str(value).map_err(E::custom)?.into())
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Table {
                    marker: MarkerTree,
                    libc: Option<MinimumLibcVersion>,
                }

                let table = Table::deserialize(MapAccessDeserializer::new(map))?;
                Ok(Environment {
                    marker: table.marker,
                    libc: table.libc,
                })
            }
        }

        deserializer.deserialize_any(EnvironmentVisitor)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for Environment {
    fn schema_name() -> Cow<'static, str> {
        "Environment".into()
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
                        "libc": generator.subschema_for::<MinimumLibcVersion>(),
                    },
                },
            ],
        })
    }
}

/// A single marker or a list of markers and environment tables.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Environments(Vec<Environment>);

impl Environments {
    pub fn from_environments(environments: Vec<Environment>) -> Self {
        Self(environments)
    }

    pub fn as_slice(&self) -> &[Environment] {
        &self.0
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Environment> {
        self.0.iter()
    }

    pub fn into_markers(self) -> Vec<MarkerTree> {
        self.0
            .into_iter()
            .map(|environment| environment.marker)
            .collect()
    }

    pub fn has_libc_constraints(&self) -> bool {
        self.0.iter().any(|environment| environment.libc.is_some())
    }
}

impl<'a> IntoIterator for &'a Environments {
    type Item = &'a Environment;
    type IntoIter = std::slice::Iter<'a, Environment>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'de> Deserialize<'de> for Environments {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EnvironmentsVisitor;

        impl<'de> Visitor<'de> for EnvironmentsVisitor {
            type Value = Environments;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("an environment marker string or a list of strings and tables")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Environments(vec![
                    MarkerTree::from_str(value).map_err(E::custom)?.into(),
                ]))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut environments = Vec::new();
                while let Some(environment) = seq.next_element()? {
                    environments.push(environment);
                }
                Ok(Environments(environments))
            }
        }

        deserializer.deserialize_any(EnvironmentsVisitor)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for Environments {
    fn schema_name() -> Cow<'static, str> {
        "Environments".into()
    }

    fn json_schema(generator: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "anyOf": [
                generator.subschema_for::<MarkerTree>(),
                generator.subschema_for::<Vec<Environment>>(),
            ],
        })
    }
}
