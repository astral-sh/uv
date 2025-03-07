use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;

use uv_normalize::GroupName;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DependencyGroups(BTreeMap<GroupName, Vec<DependencyGroupSpecifier>>);

impl DependencyGroups {
    /// Returns the names of the dependency groups.
    pub fn keys(&self) -> impl Iterator<Item = &GroupName> {
        self.0.keys()
    }

    /// Returns the dependency group with the given name.
    pub fn get(&self, group: &GroupName) -> Option<&Vec<DependencyGroupSpecifier>> {
        self.0.get(group)
    }

    /// Returns `true` if the dependency group is in the list.
    pub fn contains_key(&self, group: &GroupName) -> bool {
        self.0.contains_key(group)
    }

    /// Returns an iterator over the dependency groups.
    pub fn iter(&self) -> impl Iterator<Item = (&GroupName, &Vec<DependencyGroupSpecifier>)> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a DependencyGroups {
    type Item = (&'a GroupName, &'a Vec<DependencyGroupSpecifier>);
    type IntoIter = std::collections::btree_map::Iter<'a, GroupName, Vec<DependencyGroupSpecifier>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// Ensure that all keys in the TOML table are unique.
impl<'de> serde::de::Deserialize<'de> for DependencyGroups {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct GroupVisitor;

        impl<'de> serde::de::Visitor<'de> for GroupVisitor {
            type Value = DependencyGroups;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a table with unique dependency group names")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut sources = BTreeMap::new();
                while let Some((key, value)) =
                    access.next_entry::<GroupName, Vec<DependencyGroupSpecifier>>()?
                {
                    match sources.entry(key) {
                        std::collections::btree_map::Entry::Occupied(entry) => {
                            return Err(serde::de::Error::custom(format!(
                                "duplicate dependency group: `{}`",
                                entry.key()
                            )));
                        }
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(value);
                        }
                    }
                }
                Ok(DependencyGroups(sources))
            }
        }

        deserializer.deserialize_map(GroupVisitor)
    }
}

/// A specifier item in a [PEP 735](https://peps.python.org/pep-0735/) Dependency Group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DependencyGroupSpecifier {
    /// A PEP 508-compatible requirement string.
    Requirement(String),
    /// A reference to another dependency group.
    IncludeGroup {
        /// The name of the group to include.
        include_group: GroupName,
    },
    /// A Dependency Object Specifier.
    Object(BTreeMap<String, String>),
}

impl<'de> Deserialize<'de> for DependencyGroupSpecifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = DependencyGroupSpecifier;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or a map with the `include-group` key")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(DependencyGroupSpecifier::Requirement(value.to_owned()))
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut map_data = BTreeMap::new();
                while let Some((key, value)) = map.next_entry()? {
                    map_data.insert(key, value);
                }

                if map_data.is_empty() {
                    return Err(serde::de::Error::custom("missing field `include-group`"));
                }

                if let Some(include_group) = map_data
                    .get("include-group")
                    .map(String::as_str)
                    .map(GroupName::from_str)
                    .transpose()
                    .map_err(serde::de::Error::custom)?
                {
                    Ok(DependencyGroupSpecifier::IncludeGroup { include_group })
                } else {
                    Ok(DependencyGroupSpecifier::Object(map_data))
                }
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}
