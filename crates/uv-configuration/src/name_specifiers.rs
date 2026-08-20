use std::borrow::Cow;
use std::str::FromStr;

use uv_normalize::PackageName;

/// A specifier used for (e.g.) pip's `--no-binary` flag.
///
/// This is a superset of the package name format, allowing for special values `:all:` and `:none:`.
#[derive(Debug, Clone)]
pub enum PackageNameSpecifier {
    All,
    None,
    Package(PackageName),
}

impl FromStr for PackageNameSpecifier {
    type Err = uv_normalize::InvalidNameError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            ":all:" => Ok(Self::All),
            ":none:" => Ok(Self::None),
            _ => Ok(Self::Package(PackageName::from_str(name)?)),
        }
    }
}

impl<'de> serde::Deserialize<'de> for PackageNameSpecifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = PackageNameSpecifier;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a package name or `:all:` or `:none:`")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                // Accept the special values `:all:` and `:none:`.
                match value {
                    ":all:" => Ok(PackageNameSpecifier::All),
                    ":none:" => Ok(PackageNameSpecifier::None),
                    _ => {
                        // Otherwise, parse the value as a package name.
                        match PackageName::from_str(value) {
                            Ok(name) => Ok(PackageNameSpecifier::Package(name)),
                            Err(err) => Err(E::custom(err)),
                        }
                    }
                }
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for PackageNameSpecifier {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("PackageNameSpecifier")
    }

    fn json_schema(_gen: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": r"^(:none:|:all:|([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9._-]*[a-zA-Z0-9]))$",
            "description": "The name of a package, or `:all:` or `:none:` to select or omit all packages, respectively.",
        })
    }
}

/// A repeated specifier used for (e.g.) pip's `--no-binary` flag.
///
/// This is a superset of the package name format, allowing for special values `:all:` and `:none:`.
#[derive(Debug, Clone)]
pub enum PackageNameSpecifiers {
    All,
    None,
    Packages(Vec<PackageName>),
}

impl PackageNameSpecifiers {
    pub(crate) fn from_iter(specifiers: impl Iterator<Item = PackageNameSpecifier>) -> Self {
        let mut packages = Vec::new();
        let mut all: bool = false;

        for specifier in specifiers {
            match specifier {
                PackageNameSpecifier::None => {
                    packages.clear();
                    all = false;
                }
                PackageNameSpecifier::All => {
                    all = true;
                }
                PackageNameSpecifier::Package(name) => {
                    packages.push(name);
                }
            }
        }

        if all {
            Self::All
        } else if packages.is_empty() {
            Self::None
        } else {
            Self::Packages(packages)
        }
    }
}

/// A package selector for `--only-binary`, including conditional wheel-only selection.
#[derive(Debug, Clone)]
pub enum OnlyBinarySpecifier {
    /// Apply an existing package, `:all:`, or `:none:` selector.
    Package(PackageNameSpecifier),
    /// Require wheels only when the selected version provides compatible wheels.
    IfAvailable,
}

impl OnlyBinarySpecifier {
    /// Return the underlying package selector, if this is not a conditional selector.
    pub fn into_package_specifier(self) -> Option<PackageNameSpecifier> {
        match self {
            Self::Package(specifier) => Some(specifier),
            Self::IfAvailable => None,
        }
    }
}

impl From<PackageNameSpecifier> for OnlyBinarySpecifier {
    fn from(specifier: PackageNameSpecifier) -> Self {
        Self::Package(specifier)
    }
}

impl FromStr for OnlyBinarySpecifier {
    type Err = uv_normalize::InvalidNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == ":if-available:" {
            Ok(Self::IfAvailable)
        } else {
            PackageNameSpecifier::from_str(value).map(Self::Package)
        }
    }
}

impl<'de> serde::Deserialize<'de> for OnlyBinarySpecifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <Cow<'de, str> as serde::Deserialize>::deserialize(deserializer)?;
        Self::from_str(&value).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for OnlyBinarySpecifier {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("OnlyBinarySpecifier")
    }

    fn json_schema(_generator: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": r"^(:none:|:all:|:if-available:|([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9._-]*[a-zA-Z0-9]))$",
            "description": "The name of a package, `:all:`, `:none:`, or `:if-available:` to require wheels when they are available.",
        })
    }
}
