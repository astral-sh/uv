use std::{
    collections::{btree_map::Entry, BTreeMap},
    str::FromStr,
};

#[derive(Debug, Clone)]
pub struct ConfigSettingEntry {
    /// The key of the setting. For example, given `key=value`, this would be `key`.
    key: String,
    /// The value of the setting. For example, given `key=value`, this would be `value`.
    value: String,
}

impl FromStr for ConfigSettingEntry {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((key, value)) = s.split_once('=') else {
            return Err(format!(
                "Invalid config setting: {s} (expected `KEY=VALUE`)"
            ));
        };
        Ok(Self {
            key: key.trim().to_string(),
            value: value.trim().to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
enum ConfigSettingValue {
    /// The value consists of a single string.
    String(String),
    /// The value consists of a list of strings.
    List(Vec<String>),
}

impl serde::Serialize for ConfigSettingValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            ConfigSettingValue::String(value) => serializer.serialize_str(value),
            ConfigSettingValue::List(values) => serializer.collect_seq(values.iter()),
        }
    }
}

impl<'de> serde::Deserialize<'de> for ConfigSettingValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ConfigSettingValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or list of strings")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(ConfigSettingValue::String(value.to_string()))
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element()? {
                    values.push(value);
                }
                Ok(ConfigSettingValue::List(values))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// Settings to pass to a PEP 517 build backend, structured as a map from (string) key to string or
/// list of strings.
///
/// See: <https://peps.python.org/pep-0517/#config-settings>
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ConfigSettings(BTreeMap<String, ConfigSettingValue>);

impl FromIterator<ConfigSettingEntry> for ConfigSettings {
    fn from_iter<T: IntoIterator<Item = ConfigSettingEntry>>(iter: T) -> Self {
        let mut config = BTreeMap::default();
        for entry in iter {
            match config.entry(entry.key) {
                Entry::Vacant(vacant) => {
                    vacant.insert(ConfigSettingValue::String(entry.value));
                }
                Entry::Occupied(mut occupied) => match occupied.get_mut() {
                    ConfigSettingValue::String(existing) => {
                        let existing = existing.clone();
                        occupied.insert(ConfigSettingValue::List(vec![existing, entry.value]));
                    }
                    ConfigSettingValue::List(existing) => {
                        existing.push(entry.value);
                    }
                },
            }
        }
        Self(config)
    }
}

impl ConfigSettings {
    /// Convert the settings to a string that can be passed directly to a PEP 517 build backend.
    pub fn escape_for_python(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize config settings")
    }

    /// Merge two sets of config settings, with the values in `self` taking precedence.
    #[must_use]
    pub fn merge(self, other: ConfigSettings) -> ConfigSettings {
        let mut config = self.0;
        for (key, value) in other.0 {
            match config.entry(key) {
                Entry::Vacant(vacant) => {
                    vacant.insert(value);
                }
                Entry::Occupied(mut occupied) => match occupied.get_mut() {
                    ConfigSettingValue::String(existing) => {
                        let existing = existing.clone();
                        match value {
                            ConfigSettingValue::String(value) => {
                                occupied.insert(ConfigSettingValue::List(vec![existing, value]));
                            }
                            ConfigSettingValue::List(mut values) => {
                                values.insert(0, existing);
                                occupied.insert(ConfigSettingValue::List(values));
                            }
                        }
                    }
                    ConfigSettingValue::List(existing) => match value {
                        ConfigSettingValue::String(value) => {
                            existing.push(value);
                        }
                        ConfigSettingValue::List(values) => {
                            existing.extend(values);
                        }
                    },
                },
            }
        }
        Self(config)
    }
}

impl serde::Serialize for ConfigSettings {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for ConfigSettings {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ConfigSettings;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map from string to string or list of strings")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut config = BTreeMap::default();
                while let Some((key, value)) = map.next_entry()? {
                    config.insert(key, value);
                }
                Ok(ConfigSettings(config))
            }
        }

        deserializer.deserialize_map(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_config_settings() {
        let settings: ConfigSettings = vec![
            ConfigSettingEntry {
                key: "key".to_string(),
                value: "value".to_string(),
            },
            ConfigSettingEntry {
                key: "key".to_string(),
                value: "value2".to_string(),
            },
            ConfigSettingEntry {
                key: "list".to_string(),
                value: "value3".to_string(),
            },
            ConfigSettingEntry {
                key: "list".to_string(),
                value: "value4".to_string(),
            },
        ]
        .into_iter()
        .collect();
        assert_eq!(
            settings.0.get("key"),
            Some(&ConfigSettingValue::List(vec![
                "value".to_string(),
                "value2".to_string()
            ]))
        );
        assert_eq!(
            settings.0.get("list"),
            Some(&ConfigSettingValue::List(vec![
                "value3".to_string(),
                "value4".to_string()
            ]))
        );
    }

    #[test]
    fn escape_for_python() {
        let mut settings = ConfigSettings::default();
        settings.0.insert(
            "key".to_string(),
            ConfigSettingValue::String("value".to_string()),
        );
        settings.0.insert(
            "list".to_string(),
            ConfigSettingValue::List(vec!["value1".to_string(), "value2".to_string()]),
        );
        assert_eq!(
            settings.escape_for_python(),
            r#"{"key":"value","list":["value1","value2"]}"#
        );

        let mut settings = ConfigSettings::default();
        settings.0.insert(
            "key".to_string(),
            ConfigSettingValue::String("Hello, \"world!\"".to_string()),
        );
        settings.0.insert(
            "list".to_string(),
            ConfigSettingValue::List(vec!["'value1'".to_string()]),
        );
        assert_eq!(
            settings.escape_for_python(),
            r#"{"key":"Hello, \"world!\"","list":["'value1'"]}"#
        );

        let mut settings = ConfigSettings::default();
        settings.0.insert(
            "key".to_string(),
            ConfigSettingValue::String("val\\1 {}value".to_string()),
        );
        assert_eq!(settings.escape_for_python(), r#"{"key":"val\\1 {}value"}"#);
    }
}
