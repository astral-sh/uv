use std::str::FromStr;

use pep508_rs::PackageName;

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

        impl<'de> serde::de::Visitor<'de> for Visitor {
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
    fn schema_name() -> String {
        "PackageNameSpecifier".to_string()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            string: Some(Box::new(schemars::schema::StringValidation {
                // See: https://packaging.python.org/en/latest/specifications/name-normalization/#name-format
                pattern: Some(
                    r"^(:none:|:all:|([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9._-]*[a-zA-Z0-9]))$"
                        .to_string(),
                ),
                ..schemars::schema::StringValidation::default()
            })),
            metadata: Some(Box::new(schemars::schema::Metadata {
                description: Some("The name of a package, or `:all:` or `:none:` to select or omit all packages, respectively.".to_string()),
              ..schemars::schema::Metadata::default()
            })),
            ..schemars::schema::SchemaObject::default()
        }
        .into()
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
