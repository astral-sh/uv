use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::LazyLock;

use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use uv_small_str::SmallString;

use crate::{
    InvalidNameError, InvalidPipGroupError, InvalidPipGroupPathError, validate_and_normalize_ref,
};

/// The normalized name of a dependency group.
///
/// See:
/// - <https://peps.python.org/pep-0735/>
/// - <https://packaging.python.org/en/latest/specifications/name-normalization/>
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GroupName(SmallString);

impl GroupName {
    /// Create a validated, normalized group name.
    ///
    /// At present, this is no more efficient than calling [`GroupName::from_str`].
    #[allow(clippy::needless_pass_by_value)]
    pub fn from_owned(name: String) -> Result<Self, InvalidNameError> {
        validate_and_normalize_ref(&name).map(Self)
    }
}

impl FromStr for GroupName {
    type Err = InvalidNameError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        validate_and_normalize_ref(name).map(Self)
    }
}

impl<'de> Deserialize<'de> for GroupName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = GroupName;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                GroupName::from_str(v).map_err(serde::de::Error::custom)
            }

            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                GroupName::from_owned(v).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl Serialize for GroupName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl std::fmt::Display for GroupName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for GroupName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// The pip-compatible variant of a [`GroupName`].
///
/// Either <groupname> or <path>:<groupname>.
/// If <path> is omitted it defaults to "pyproject.toml".
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PipGroupName {
    pub path: Option<PathBuf>,
    pub name: GroupName,
}

impl PipGroupName {
    /// Gets the path to use, applying the default if it's missing
    pub fn path(&self) -> &Path {
        if let Some(path) = &self.path {
            path
        } else {
            Path::new("pyproject.toml")
        }
    }
}

impl FromStr for PipGroupName {
    type Err = InvalidPipGroupError;

    fn from_str(path_and_name: &str) -> Result<Self, Self::Err> {
        // The syntax is `<path>:<name>`.
        //
        // `:` isn't valid as part of a dependency-group name, but it can appear in a path.
        // Therefore we look for the first `:` starting from the end to find the delimiter.
        // If there is no `:` then there's no path and we use the default one.
        if let Some((path, name)) = path_and_name.rsplit_once(':') {
            // pip hard errors if the path does not end with pyproject.toml
            if !path.ends_with("pyproject.toml") {
                Err(InvalidPipGroupPathError(path.to_owned()))?;
            }

            let name = GroupName::from_str(name)?;
            let path = Some(PathBuf::from(path));
            Ok(Self { path, name })
        } else {
            let name = GroupName::from_str(path_and_name)?;
            let path = None;
            Ok(Self { path, name })
        }
    }
}

impl<'de> Deserialize<'de> for PipGroupName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Serialize for PipGroupName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let string = self.to_string();
        string.serialize(serializer)
    }
}

impl Display for PipGroupName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "{}:{}", path.display(), self.name)
        } else {
            self.name.fmt(f)
        }
    }
}

/// Either the literal "all" or a list of groups
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DefaultGroups {
    /// All groups are defaulted
    All,
    /// A list of groups
    List(Vec<GroupName>),
}

/// Serialize a [`DefaultGroups`] struct into a list of marker strings.
impl serde::Serialize for DefaultGroups {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            DefaultGroups::All => serializer.serialize_str("all"),
            DefaultGroups::List(groups) => {
                let mut seq = serializer.serialize_seq(Some(groups.len()))?;
                for group in groups {
                    seq.serialize_element(&group)?;
                }
                seq.end()
            }
        }
    }
}

/// Deserialize a "all" or list of [`GroupName`] into a [`DefaultGroups`] enum.
impl<'de> serde::Deserialize<'de> for DefaultGroups {
    fn deserialize<D>(deserializer: D) -> Result<DefaultGroups, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StringOrVecVisitor;

        impl<'de> serde::de::Visitor<'de> for StringOrVecVisitor {
            type Value = DefaultGroups;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(r#"the string "all" or a list of strings"#)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value != "all" {
                    return Err(serde::de::Error::custom(
                        r#"default-groups must be "all" or a ["list", "of", "groups"]"#,
                    ));
                }
                Ok(DefaultGroups::All)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut groups = Vec::new();

                while let Some(elem) = seq.next_element::<GroupName>()? {
                    groups.push(elem);
                }

                Ok(DefaultGroups::List(groups))
            }
        }

        deserializer.deserialize_any(StringOrVecVisitor)
    }
}

impl Default for DefaultGroups {
    /// Note this is an "empty" default unlike other contexts where `["dev"]` is the default
    fn default() -> Self {
        DefaultGroups::List(Vec::new())
    }
}

/// The name of the global `dev-dependencies` group.
///
/// Internally, we model dependency groups as a generic concept; but externally, we only expose the
/// `dev-dependencies` group.
pub static DEV_DEPENDENCIES: LazyLock<GroupName> =
    LazyLock::new(|| GroupName::from_str("dev").unwrap());
