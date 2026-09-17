use std::fmt::Formatter;

use serde::Deserialize;
use serde::de::{MapAccess, Visitor, value::MapAccessDeserializer};

use uv_pep440::{Version, VersionSpecifier};
use uv_pep508::{MarkerExpression, MarkerTree, MarkerValueVersion};

/// An environment marker with optional libc coverage requirements.
#[derive(Debug)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(untagged, deny_unknown_fields))]
pub enum RequiredEnvironment {
    Marker(#[cfg_attr(feature = "schemars", schemars(with = "String"))] MarkerTree),
    Table {
        #[cfg_attr(feature = "schemars", schemars(with = "String"))]
        marker: MarkerTree,
        libc: Option<LibcVersions>,
    },
}

impl RequiredEnvironment {
    /// Lower each libc baseline into a separate coverage requirement for the same environment.
    pub(crate) fn into_markers(self) -> Vec<MarkerTree> {
        let (marker, libc) = match self {
            Self::Marker(marker) | Self::Table { marker, libc: None } => return vec![marker],
            Self::Table {
                marker,
                libc: Some(libc),
            } => (marker, libc),
        };

        [
            (
                MarkerValueVersion::GlibcVersion,
                MarkerValueVersion::MuslVersion,
                libc.glibc,
            ),
            (
                MarkerValueVersion::MuslVersion,
                MarkerValueVersion::GlibcVersion,
                libc.musl,
            ),
        ]
        .into_iter()
        .filter_map(|(key, other, version)| {
            let version = version?;
            // The other family is absent, not unconstrained. This keeps glibc and musl
            // requirements disjoint, while requiring the selected package to cover both.
            Some(
                marker
                    .and(MarkerTree::expression(MarkerExpression::Version {
                        key,
                        specifier: VersionSpecifier::equals_version(version.0),
                    }))
                    .and(MarkerTree::expression(MarkerExpression::Version {
                        key: other,
                        specifier: VersionSpecifier::equals_version(Version::new([0])),
                    })),
            )
        })
        .collect()
    }
}

impl<'de> Deserialize<'de> for RequiredEnvironment {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EnvironmentVisitor;

        impl<'de> Visitor<'de> for EnvironmentVisitor {
            type Value = RequiredEnvironment;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("an environment marker string or table")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                value
                    .parse()
                    .map(RequiredEnvironment::Marker)
                    .map_err(E::custom)
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Table {
                    marker: MarkerTree,
                    libc: Option<LibcVersions>,
                }

                let Table { marker, libc } = Table::deserialize(MapAccessDeserializer::new(map))?;
                Ok(RequiredEnvironment::Table { marker, libc })
            }
        }

        deserializer.deserialize_any(EnvironmentVisitor)
    }
}

/// The oldest required release of each specified libc implementation.
#[derive(Debug)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(deny_unknown_fields, extend("minProperties" = 1)))]
pub struct LibcVersions {
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    glibc: Option<LibcVersion>,
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    musl: Option<LibcVersion>,
}

impl<'de> Deserialize<'de> for LibcVersions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            glibc: Option<LibcVersion>,
            musl: Option<LibcVersion>,
        }

        let Wire { glibc, musl } = Wire::deserialize(deserializer)?;
        if glibc.is_none() && musl.is_none() {
            return Err(serde::de::Error::custom(
                "at least one of `glibc` or `musl` must be specified",
            ));
        }
        Ok(Self { glibc, musl })
    }
}

/// A major.minor libc release with a nonzero major; zero denotes an absent implementation.
#[derive(Debug, Deserialize)]
#[serde(try_from = "String")]
struct LibcVersion(Version);

impl TryFrom<String> for LibcVersion {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let invalid =
            "expected a libc version in the form `<major>.<minor>` (e.g., `2.31` or `1.2`)";
        let Some((major, minor)) = value.split_once('.') else {
            return Err(invalid);
        };
        if !major.bytes().all(|byte| byte.is_ascii_digit())
            || !minor.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(invalid);
        }
        let major = major.parse::<u16>().map_err(|_| invalid)?;
        let minor = minor.parse::<u16>().map_err(|_| invalid)?;
        if major == 0 {
            return Err(invalid);
        }
        Ok(Self(Version::new([u64::from(major), u64::from(minor)])))
    }
}
