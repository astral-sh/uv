use std::str::FromStr;

use uv_normalize::PackageName;
use uv_pep440::Version;

/// A `<name>-<version>-variants.json` file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VariantsJson {
    pub name: PackageName,
    pub version: Version,
}

impl VariantsJson {
    /// Parse a `<name>-<version>-variants.json` filename.
    ///
    /// name and version must be normalized, i.e., they don't contain dashes.
    pub fn try_from_normalized_filename(filename: &str) -> Option<Self> {
        let stem = filename.strip_suffix("-variants.json")?;

        let (name, version) = stem.split_once('-')?;
        let name = PackageName::from_str(name).ok()?;
        let version = Version::from_str(version).ok()?;

        Some(VariantsJson { name, version })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_json_parsing() {
        let variant =
            VariantsJson::try_from_normalized_filename("numpy-1.21.0-variants.json").unwrap();
        assert_eq!(variant.name.as_str(), "numpy");
        assert_eq!(variant.version.to_string(), "1.21.0");
    }
}
