use std::borrow::Cow;
use std::fmt::Display;
use std::str::FromStr;

use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::{Identifier, IdentifierParseError};

/// The name of an importable Python module.
///
/// This is a dotted sequence of Python identifiers, like `foo` or `foo.bar`.
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModuleName(Box<str>);

#[derive(Debug, Clone, Error)]
pub enum ModuleNameParseError {
    #[error("A module name must not be empty")]
    Empty,
    #[error("Invalid module name component `{component}` in `{module}`")]
    InvalidComponent {
        component: Box<str>,
        module: Box<str>,
        #[source]
        err: IdentifierParseError,
    },
}

impl ModuleName {
    pub fn new(module: impl Into<Box<str>>) -> Result<Self, ModuleNameParseError> {
        let module = module.into();
        if module.is_empty() {
            return Err(ModuleNameParseError::Empty);
        }

        for component in module.split('.') {
            Self::validate_component(&module, component)?;
        }

        Ok(Self(module))
    }

    pub fn from_components<'a>(
        components: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, ModuleNameParseError> {
        let components = components.into_iter().collect::<Vec<_>>();
        if components.is_empty() {
            return Err(ModuleNameParseError::Empty);
        }

        let module = components.join(".").into_boxed_str();
        for component in components {
            Self::validate_component(&module, component)?;
        }

        Ok(Self(module))
    }

    /// Iterate over this module and its parent modules.
    ///
    /// For example, `foo.bar.baz` yields `foo`, `foo.bar`, and `foo.bar.baz`.
    pub fn prefixes(&self) -> impl Iterator<Item = Self> + '_ {
        self.0
            .match_indices('.')
            .map(|(index, _)| Self(Box::from(&self.0[..index])))
            .chain(std::iter::once(self.clone()))
    }

    fn validate_component(module: &str, component: &str) -> Result<(), ModuleNameParseError> {
        Identifier::new(component.to_string()).map_err(|err| {
            ModuleNameParseError::InvalidComponent {
                component: component.to_string().into_boxed_str(),
                module: module.into(),
                err,
            }
        })?;
        Ok(())
    }
}

impl FromStr for ModuleName {
    type Err = ModuleNameParseError;

    fn from_str(module: &str) -> Result<Self, Self::Err> {
        Self::new(module)
    }
}

impl Display for ModuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for ModuleName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<'de> serde::de::Deserialize<'de> for ModuleName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = <Cow<'_, str>>::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Serialize for ModuleName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Serialize::serialize(&self.0, serializer)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for ModuleName {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("ModuleName")
    }

    fn json_schema(_generator: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": r"^[_\p{Alphabetic}][_0-9\p{Alphabetic}]*(\.[_\p{Alphabetic}][_0-9\p{Alphabetic}]*)*$",
            "description": "A dotted Python module name"
        })
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use insta::assert_snapshot;

    use super::ModuleName;

    #[test]
    fn valid() {
        for module_name in ["abc", "abc.def", "_abc", "férrîs", "package.안녕하세요"] {
            assert!(ModuleName::from_str(module_name).is_ok(), "{module_name}");
        }
    }

    #[test]
    fn invalid() {
        assert_snapshot!(
            ModuleName::from_str("foo-bar").unwrap_err(),
            @"Invalid module name component `foo-bar` in `foo-bar`"
        );
        assert_snapshot!(
            ModuleName::from_str("foo.").unwrap_err(),
            @"Invalid module name component `` in `foo.`"
        );
    }

    #[test]
    fn prefixes() {
        let prefixes = ModuleName::from_str("foo.bar.baz")
            .expect("valid module name")
            .prefixes()
            .map(|module| module.to_string())
            .collect::<Vec<_>>();

        assert_eq!(prefixes, ["foo", "foo.bar", "foo.bar.baz"]);
    }
}
