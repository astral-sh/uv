use std::collections::{BTreeMap, btree_map::Entry};
use std::fmt;

use serde::de::{Error as _, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};

use crate::{FileLocation, UrlString};

/// A one-way mapping from physical proxy artifact URL prefixes to canonical URL prefixes.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ArtifactUrlMap(BTreeMap<DisplaySafeUrl, DisplaySafeUrl>);

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for ArtifactUrlMap {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("ArtifactUrlMap")
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "One-way physical proxy artifact URL prefixes (keys) to canonical lock artifact URL prefixes (values).",
            "type": "object",
            "minProperties": 1,
            "propertyNames": {
                "format": "uri"
            },
            "additionalProperties": {
                "type": "string",
                "format": "uri"
            }
        })
    }
}

impl<'de> Deserialize<'de> for ArtifactUrlMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ArtifactUrlMapVisitor;

        impl<'de> Visitor<'de> for ArtifactUrlMapVisitor {
            type Value = ArtifactUrlMap;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a physical-to-canonical artifact URL map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut mappings = BTreeMap::new();
                while let Some((physical, canonical)) = map.next_entry::<String, String>()? {
                    let physical = parse_artifact_url::<A::Error>(&physical)?;
                    let canonical = parse_artifact_url::<A::Error>(&canonical)?;
                    match mappings.entry(physical) {
                        Entry::Vacant(entry) => {
                            entry.insert(canonical);
                        }
                        Entry::Occupied(entry) => {
                            let physical = diagnostic_safe(entry.key());
                            return Err(A::Error::custom(format_args!(
                                "duplicate physical artifact URL prefix `{physical}`"
                            )));
                        }
                    }
                }
                Ok(ArtifactUrlMap(mappings))
            }
        }

        deserializer.deserialize_map(ArtifactUrlMapVisitor)
    }
}

impl ArtifactUrlMap {
    /// Create an artifact URL map from physical-to-canonical prefix mappings.
    #[cfg(test)]
    pub(crate) fn new(mappings: BTreeMap<DisplaySafeUrl, DisplaySafeUrl>) -> Self {
        Self(mappings)
    }

    /// Create an artifact URL map containing one physical-to-canonical prefix mapping.
    pub fn single(physical: DisplaySafeUrl, canonical: DisplaySafeUrl) -> Self {
        Self(BTreeMap::from([(physical, canonical)]))
    }

    pub(crate) fn validate(&self) -> Result<ValidatedArtifactUrlMap, ArtifactUrlMapError> {
        if self.0.is_empty() {
            return Err(ArtifactUrlMapError::Empty);
        }

        let mut mappings = Vec::<ArtifactUrlMapping>::with_capacity(self.0.len());
        for (physical, canonical) in &self.0 {
            validate_configured_prefix(physical)?;
            validate_configured_prefix(canonical)?;

            let physical = normalize_prefix(physical);
            let canonical = normalize_prefix(canonical);
            for mapping in &mappings {
                if same_origin(&mapping.physical, &physical)
                    && (path_suffix(mapping.physical.path(), physical.path()).is_some()
                        || path_suffix(physical.path(), mapping.physical.path()).is_some())
                {
                    return Err(ArtifactUrlMapError::OverlappingPhysicalPrefixes {
                        first: Box::new(mapping.physical.clone()),
                        second: Box::new(physical),
                    });
                }
            }
            mappings.push(ArtifactUrlMapping {
                physical,
                canonical,
            });
        }
        Ok(ValidatedArtifactUrlMap { mappings })
    }
}

impl FromIterator<(DisplaySafeUrl, DisplaySafeUrl)> for ArtifactUrlMap {
    fn from_iter<T: IntoIterator<Item = (DisplaySafeUrl, DisplaySafeUrl)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub(crate) struct ValidatedArtifactUrlMap {
    mappings: Vec<ArtifactUrlMapping>,
}

impl ValidatedArtifactUrlMap {
    pub(crate) fn canonical_artifact_url(
        &self,
        location: &FileLocation,
        filename: &str,
    ) -> Result<FileLocation, ArtifactUrlMapError> {
        let mut physical = location
            .to_url()
            .map_err(|_| ArtifactUrlMapError::InvalidArtifactUrl)?;

        // Simple API hash fragments have already been captured in [`crate::File::hashes`].
        physical.set_fragment(None);

        let physical = DisplaySafeUrl::parse(physical.as_str())
            .map_err(|_| ArtifactUrlMapError::InvalidArtifactUrl)?;

        validate_http_url(&physical)?;
        if has_credentials(&physical) {
            return Err(ArtifactUrlMapError::Credentials {
                url: Box::new(diagnostic_safe(&physical)),
            });
        }
        if physical.query().is_some() {
            return Err(ArtifactUrlMapError::Query {
                url: Box::new(diagnostic_safe(&physical)),
            });
        }

        let mut matched = None;
        for mapping in &self.mappings {
            if same_origin(&mapping.physical, &physical)
                && let Some(suffix) = path_suffix(mapping.physical.path(), physical.path())
            {
                if matched.is_some() {
                    return Err(ArtifactUrlMapError::Ambiguous {
                        url: Box::new(physical),
                    });
                }
                matched = Some((mapping, suffix));
            }
        }
        let Some((mapping, suffix)) = matched else {
            return Err(ArtifactUrlMapError::Unmapped {
                url: Box::new(physical),
            });
        };

        let mut canonical = mapping.canonical.clone();
        canonical.set_path(&mapped_path(mapping.canonical.path(), suffix));
        let Some(encoded_filename) = canonical
            .path_segments()
            .and_then(|mut segments| segments.next_back())
        else {
            return Err(ArtifactUrlMapError::MissingFilename {
                url: Box::new(canonical),
            });
        };
        let mapped_filename = percent_encoding::percent_decode_str(encoded_filename)
            .decode_utf8()
            .map_err(|source| ArtifactUrlMapError::InvalidFilenameEncoding {
                url: Box::new(canonical.clone()),
                source,
            })?
            .into_owned();
        if mapped_filename != filename {
            return Err(ArtifactUrlMapError::FilenameChanged {
                url: Box::new(canonical),
                expected: filename.to_string(),
                actual: mapped_filename,
            });
        }

        Ok(FileLocation::AbsoluteUrl(UrlString::from(canonical)))
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct ArtifactUrlMapping {
    physical: DisplaySafeUrl,
    canonical: DisplaySafeUrl,
}

/// An invalid artifact URL mapping or application.
#[derive(Debug, Clone, Eq, PartialEq, Error)]
pub enum ArtifactUrlMapError {
    #[error("Artifact URL map must contain at least one physical-to-canonical prefix mapping")]
    Empty,
    #[error("Artifact URL `{url}` must use the HTTP or HTTPS scheme")]
    UnsupportedScheme { url: Box<DisplaySafeUrl> },
    #[error("Artifact URL `{url}` cannot be used as a URL base")]
    CannotBeABase { url: Box<DisplaySafeUrl> },
    #[error("Artifact URL `{url}` must not contain credentials")]
    Credentials { url: Box<DisplaySafeUrl> },
    #[error("Artifact URL `{url}` must not contain a query string")]
    Query { url: Box<DisplaySafeUrl> },
    #[error("Configured artifact URL prefix `{url}` must not contain a fragment")]
    Fragment { url: Box<DisplaySafeUrl> },
    #[error("Physical artifact URL prefixes `{first}` and `{second}` overlap")]
    OverlappingPhysicalPrefixes {
        first: Box<DisplaySafeUrl>,
        second: Box<DisplaySafeUrl>,
    },
    // Do not retain the failed parse input or source here. Unlike a [`DisplaySafeUrl`], both can
    // contain unredacted credentials or query parameters from an untrusted proxy response.
    #[error("Failed to resolve proxy artifact URL")]
    InvalidArtifactUrl,
    #[error("Artifact URL `{url}` does not match any configured physical prefix")]
    Unmapped { url: Box<DisplaySafeUrl> },
    #[error("Artifact URL `{url}` matches multiple configured physical prefixes")]
    Ambiguous { url: Box<DisplaySafeUrl> },
    #[error("Mapped artifact URL `{url}` does not contain a filename")]
    MissingFilename { url: Box<DisplaySafeUrl> },
    #[error("Mapped artifact URL `{url}` has an invalid percent-encoded filename")]
    InvalidFilenameEncoding {
        url: Box<DisplaySafeUrl>,
        #[source]
        source: std::str::Utf8Error,
    },
    #[error(
        "Mapped artifact URL `{url}` has filename `{actual}`, but the selected file is `{expected}`"
    )]
    FilenameChanged {
        url: Box<DisplaySafeUrl>,
        expected: String,
        actual: String,
    },
}

fn validate_configured_prefix(url: &DisplaySafeUrl) -> Result<(), ArtifactUrlMapError> {
    validate_http_url(url)?;
    if has_credentials(url) {
        return Err(ArtifactUrlMapError::Credentials {
            url: Box::new(diagnostic_safe(url)),
        });
    }
    if url.query().is_some() {
        return Err(ArtifactUrlMapError::Query {
            url: Box::new(diagnostic_safe(url)),
        });
    }
    if url.fragment().is_some() {
        return Err(ArtifactUrlMapError::Fragment {
            url: Box::new(diagnostic_safe(url)),
        });
    }
    Ok(())
}

fn validate_http_url(url: &DisplaySafeUrl) -> Result<(), ArtifactUrlMapError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ArtifactUrlMapError::UnsupportedScheme {
            url: Box::new(diagnostic_safe(url)),
        });
    }
    if url.cannot_be_a_base() {
        return Err(ArtifactUrlMapError::CannotBeABase {
            url: Box::new(diagnostic_safe(url)),
        });
    }
    Ok(())
}

fn diagnostic_safe(url: &DisplaySafeUrl) -> DisplaySafeUrl {
    let mut sanitized = url.clone();
    sanitized.remove_credentials();
    sanitized.set_query(None);
    sanitized.set_fragment(None);
    sanitized
}

fn parse_artifact_url<E: serde::de::Error>(value: &str) -> Result<DisplaySafeUrl, E> {
    DisplaySafeUrl::parse(value).map_err(|error| match error {
        DisplaySafeUrlError::AmbiguousAuthority(_) => {
            E::custom("ambiguous user/pass authority in artifact URL (not percent-encoded?)")
        }
        DisplaySafeUrlError::Url(error) => E::custom(error),
    })
}

fn has_credentials(url: &DisplaySafeUrl) -> bool {
    !url.username().is_empty() || url.password().is_some()
}

fn normalize_prefix(url: &DisplaySafeUrl) -> DisplaySafeUrl {
    let mut normalized = url.clone();
    let path = normalized_path(url.path());
    normalized.set_path(&path);
    normalized
}

fn normalized_path(path: &str) -> String {
    if path.is_empty() {
        return "/".to_string();
    }
    path.strip_suffix('/')
        .filter(|prefix| !prefix.is_empty() && !prefix.ends_with('/'))
        .unwrap_or(path)
        .to_string()
}

fn same_origin(left: &DisplaySafeUrl, right: &DisplaySafeUrl) -> bool {
    left.origin() == right.origin()
}

fn path_suffix<'url>(prefix: &str, path: &'url str) -> Option<&'url str> {
    if prefix == "/" {
        return path.strip_prefix('/');
    }
    let suffix = path.strip_prefix(prefix)?;
    if suffix.is_empty() || prefix.ends_with('/') {
        Some(suffix)
    } else {
        suffix.strip_prefix('/')
    }
}

fn mapped_path(prefix: &str, suffix: &str) -> String {
    if suffix.is_empty() {
        prefix.to_string()
    } else if prefix.ends_with('/') {
        format!("{prefix}{suffix}")
    } else {
        format!("{prefix}/{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn validates_required_http_prefixes() -> Result<(), Box<dyn Error>> {
        assert_eq!(
            ArtifactUrlMap::new(BTreeMap::new()).validate(),
            Err(ArtifactUrlMapError::Empty)
        );

        let invalid = [
            (
                "ftp://proxy.example/files",
                ArtifactUrlMapError::UnsupportedScheme {
                    url: Box::new(url("ftp://proxy.example/files")?),
                },
            ),
            (
                "https://username-token@proxy.example/files",
                ArtifactUrlMapError::Credentials {
                    url: Box::new(url("https://proxy.example/files")?),
                },
            ),
            (
                "https://proxy.example/files?token=secret",
                ArtifactUrlMapError::Query {
                    url: Box::new(url("https://proxy.example/files")?),
                },
            ),
            (
                "ftp://proxy.example/files?token=secret",
                ArtifactUrlMapError::UnsupportedScheme {
                    url: Box::new(url("ftp://proxy.example/files")?),
                },
            ),
            (
                "https://username-token@proxy.example/files?token=secret",
                ArtifactUrlMapError::Credentials {
                    url: Box::new(url("https://proxy.example/files")?),
                },
            ),
            (
                "https://proxy.example/files#fragment_token=secret",
                ArtifactUrlMapError::Fragment {
                    url: Box::new(url("https://proxy.example/files")?),
                },
            ),
        ];
        for (physical, expected) in invalid {
            let map =
                ArtifactUrlMap::single(url(physical)?, url("https://canonical.example/packages")?);
            assert_eq!(map.validate(), Err(expected));
        }
        let configured_query = ArtifactUrlMap::single(
            url("https://proxy.example/files?token=secret")?,
            url("https://canonical.example/packages")?,
        )
        .validate()
        .expect_err("configured queries should be rejected");
        assert!(
            configured_query
                .to_string()
                .contains("https://proxy.example/files")
        );
        assert!(!configured_query.to_string().contains("token=secret"));

        let invalid_canonical = ArtifactUrlMap::single(
            url("https://proxy.example/files")?,
            url("https://canonical.example/packages#fragment")?,
        );
        assert!(matches!(
            invalid_canonical.validate(),
            Err(ArtifactUrlMapError::Fragment { .. })
        ));
        let configured_fragment = ArtifactUrlMap::single(
            url("https://proxy.example/files#fragment_token=secret")?,
            url("https://canonical.example/packages")?,
        )
        .validate()
        .expect_err("configured fragments should be rejected");
        assert!(
            configured_fragment
                .to_string()
                .contains("proxy.example/files")
        );
        assert!(!configured_fragment.to_string().contains("secret"));
        assert!(!configured_fragment.to_string().contains("fragment_token"));
        Ok(())
    }

    #[test]
    fn rejects_normalized_duplicate_and_overlapping_prefixes() -> Result<(), Box<dyn Error>> {
        let duplicate = artifact_url_map([
            (
                "https://PROXY.example:443/pypi/",
                "https://canonical.example/one",
            ),
            (
                "https://proxy.example/pypi",
                "https://canonical.example/two",
            ),
        ])?;
        assert!(matches!(
            duplicate.validate(),
            Err(ArtifactUrlMapError::OverlappingPhysicalPrefixes { .. })
        ));

        let parent = artifact_url_map([
            (
                "https://proxy.example/pypi",
                "https://canonical.example/one",
            ),
            (
                "https://proxy.example/pypi/files",
                "https://canonical.example/two",
            ),
        ])?;
        assert!(matches!(
            parent.validate(),
            Err(ArtifactUrlMapError::OverlappingPhysicalPrefixes { .. })
        ));

        let segment_boundary = artifact_url_map([
            (
                "https://proxy.example/pypi",
                "https://canonical.example/shared",
            ),
            (
                "https://proxy.example/pypi-other",
                "https://canonical.example/shared/",
            ),
        ])?;
        assert!(segment_boundary.validate().is_ok());
        Ok(())
    }

    #[test]
    fn deserialization_rejects_normalized_duplicate_keys_and_round_trips()
    -> Result<(), Box<dyn Error>> {
        let duplicate = toml::from_str::<ArtifactUrlMap>(
            r#"
"https://PROXY.example:443/pypi" = "https://canonical.example/one"
"https://proxy.example/pypi" = "https://canonical.example/two"
"#,
        )
        .expect_err("normalized duplicate physical prefixes must be rejected");
        assert!(
            duplicate
                .to_string()
                .contains("duplicate physical artifact URL prefix")
        );
        let duplicate_query = toml::from_str::<ArtifactUrlMap>(
            r#"
"https://PROXY.example:443/pypi?token=secret" = "https://canonical.example/one"
"https://proxy.example/pypi?token=secret" = "https://canonical.example/two"
"#,
        )
        .expect_err("normalized duplicate query-bearing prefixes must be rejected");
        assert!(
            duplicate_query
                .to_string()
                .contains("https://proxy.example/pypi")
        );
        assert!(!duplicate_query.to_string().contains("token=secret"));
        let duplicate_fragment = toml::from_str::<ArtifactUrlMap>(
            r#"
"https://PROXY.example:443/pypi#fragment_token=secret" = "https://canonical.example/one"
"https://proxy.example/pypi#fragment_token=secret" = "https://canonical.example/two"
"#,
        )
        .expect_err("normalized duplicate fragment-bearing prefixes must be rejected");
        assert!(
            duplicate_fragment
                .to_string()
                .contains("https://proxy.example/pypi")
        );
        assert!(!duplicate_fragment.to_string().contains("secret"));
        assert!(!duplicate_fragment.to_string().contains("fragment_token"));

        let map = toml::from_str::<ArtifactUrlMap>(
            r#"
"https://proxy.example/pypi" = "https://canonical.example/packages"
"#,
        )?;
        let serialized = toml::to_string(&map)?;
        assert_eq!(toml::from_str::<ArtifactUrlMap>(&serialized)?, map);
        Ok(())
    }

    #[test]
    fn deserialization_rejects_ambiguous_authority() {
        for input in [
            r#"
"https://user/name:password@domain/files#fragment_token=secret" = "https://canonical.example/packages"
"#,
            r#"
"https://proxy.example/pypi" = "https://user/name:password@domain/files#fragment_token=secret"
"#,
        ] {
            let error = toml::from_str::<ArtifactUrlMap>(input)
                .expect_err("ambiguous URL authorities must be rejected");
            assert!(error.to_string().contains("ambiguous user/pass authority"));
            assert!(!error.to_string().contains("secret"));
            assert!(!error.to_string().contains("fragment_token"));
        }
    }

    #[test]
    fn maps_physical_prefix_and_preserves_encoded_suffix() -> Result<(), Box<dyn Error>> {
        let map = ArtifactUrlMap::single(
            url("https://proxy.example/pypi/")?,
            url("https://canonical.example/packages/")?,
        )
        .validate()?;
        let physical = FileLocation::new(
            "https://proxy.example/pypi/%E2%82%AC.whl#sha256=abc".into(),
            &"".into(),
        );

        let canonical = map.canonical_artifact_url(&physical, "€.whl")?;
        assert_eq!(
            canonical.to_url()?.as_str(),
            "https://canonical.example/packages/%E2%82%AC.whl"
        );
        assert!(matches!(canonical, FileLocation::AbsoluteUrl(_)));

        let relative = FileLocation::new(
            "nested/example.whl".into(),
            &"https://proxy.example/pypi/".into(),
        );
        assert_eq!(
            map.canonical_artifact_url(&relative, "example.whl")?
                .to_url()?
                .as_str(),
            "https://canonical.example/packages/nested/example.whl"
        );

        let reverse = FileLocation::new(
            "https://canonical.example/packages/%E2%82%AC.whl".into(),
            &"".into(),
        );
        assert!(matches!(
            map.canonical_artifact_url(&reverse, "€.whl"),
            Err(ArtifactUrlMapError::Unmapped { .. })
        ));
        Ok(())
    }

    #[test]
    fn requires_path_segment_boundary() -> Result<(), Box<dyn Error>> {
        let map = ArtifactUrlMap::single(
            url("https://proxy.example/pypi")?,
            url("https://canonical.example/packages")?,
        )
        .validate()?;
        let physical = FileLocation::new(
            "https://proxy.example/pypi-other/example.whl".into(),
            &"".into(),
        );
        assert!(matches!(
            map.canonical_artifact_url(&physical, "example.whl"),
            Err(ArtifactUrlMapError::Unmapped { .. })
        ));
        Ok(())
    }

    #[test]
    fn preserves_empty_path_segments() -> Result<(), Box<dyn Error>> {
        let root_map = ArtifactUrlMap::single(
            url("https://proxy.example//")?,
            url("https://canonical.example/packages//")?,
        )
        .validate()?;
        let root_artifact =
            FileLocation::new("https://proxy.example/artifact.whl".into(), &"".into());
        assert!(matches!(
            root_map.canonical_artifact_url(&root_artifact, "artifact.whl"),
            Err(ArtifactUrlMapError::Unmapped { .. })
        ));

        let empty_segment_artifact =
            FileLocation::new("https://proxy.example//artifact.whl".into(), &"".into());
        assert_eq!(
            root_map
                .canonical_artifact_url(&empty_segment_artifact, "artifact.whl")?
                .to_url()?
                .as_str(),
            "https://canonical.example/packages//artifact.whl"
        );

        let nested_map = ArtifactUrlMap::single(
            url("https://proxy.example/pypi//")?,
            url("https://canonical.example/packages//")?,
        )
        .validate()?;
        let single_separator =
            FileLocation::new("https://proxy.example/pypi/artifact.whl".into(), &"".into());
        assert!(matches!(
            nested_map.canonical_artifact_url(&single_separator, "artifact.whl"),
            Err(ArtifactUrlMapError::Unmapped { .. })
        ));
        let double_separator = FileLocation::new(
            "https://proxy.example/pypi//artifact.whl".into(),
            &"".into(),
        );
        assert_eq!(
            nested_map
                .canonical_artifact_url(&double_separator, "artifact.whl")?
                .to_url()?
                .as_str(),
            "https://canonical.example/packages//artifact.whl"
        );
        Ok(())
    }

    #[test]
    fn maps_to_root_without_duplicate_separator() -> Result<(), Box<dyn Error>> {
        let map = ArtifactUrlMap::single(
            url("https://proxy.example/pypi/")?,
            url("https://canonical.example/")?,
        )
        .validate()?;
        let physical =
            FileLocation::new("https://proxy.example/pypi/example.whl".into(), &"".into());
        assert_eq!(
            map.canonical_artifact_url(&physical, "example.whl")?
                .to_url()?
                .as_str(),
            "https://canonical.example/example.whl"
        );
        Ok(())
    }

    #[test]
    fn rejects_selected_query_credentials_and_changed_filename() -> Result<(), Box<dyn Error>> {
        let map = ArtifactUrlMap::single(
            url("https://proxy.example/files")?,
            url("https://canonical.example/packages")?,
        )
        .validate()?;

        let query = FileLocation::new(
            "https://proxy.example/files/example.whl?token=secret#fragment_token=secret".into(),
            &"".into(),
        );
        let query_error = map
            .canonical_artifact_url(&query, "example.whl")
            .expect_err("selected queries should be rejected");
        assert!(matches!(&query_error, ArtifactUrlMapError::Query { .. }));
        assert!(
            query_error
                .to_string()
                .contains("https://proxy.example/files/example.whl")
        );
        assert!(!query_error.to_string().contains("token=secret"));
        assert!(!query_error.to_string().contains("fragment_token"));

        let credentials_and_query = FileLocation::new(
            "https://username-token:password-token@proxy.example/files/example.whl?token=secret#fragment_token=secret"
                .into(),
            &"".into(),
        );
        let credentials_and_query_error = map
            .canonical_artifact_url(&credentials_and_query, "example.whl")
            .expect_err("selected credentials should be rejected before mapping");
        assert!(matches!(
            &credentials_and_query_error,
            ArtifactUrlMapError::Credentials { .. }
        ));
        let ArtifactUrlMapError::Credentials { url } = &credentials_and_query_error else {
            return Err("expected credentials error".into());
        };
        assert_eq!(url.as_str(), "https://proxy.example/files/example.whl");
        assert!(
            credentials_and_query_error
                .to_string()
                .contains("https://proxy.example/files/example.whl")
        );
        for diagnostic in [
            credentials_and_query_error.to_string(),
            format!("{credentials_and_query_error:?}"),
        ] {
            assert!(!diagnostic.contains("username-token"));
            assert!(!diagnostic.contains("password-token"));
            assert!(!diagnostic.contains("token=secret"));
            assert!(!diagnostic.contains("fragment_token"));
        }

        let credentials = FileLocation::new(
            "https://user:password@proxy.example/files/example.whl".into(),
            &"".into(),
        );
        assert!(matches!(
            map.canonical_artifact_url(&credentials, "example.whl"),
            Err(ArtifactUrlMapError::Credentials { .. })
        ));

        let local = FileLocation::new(
            "file:///tmp/example.whl#fragment_token=secret".into(),
            &"".into(),
        );
        let local_error = map
            .canonical_artifact_url(&local, "example.whl")
            .expect_err("non-HTTP artifact URLs should be rejected");
        assert!(matches!(
            &local_error,
            ArtifactUrlMapError::UnsupportedScheme { .. }
        ));
        assert!(!local_error.to_string().contains("secret"));
        assert!(!local_error.to_string().contains("fragment_token"));

        let filename = FileLocation::new(
            "https://proxy.example/files/different.whl".into(),
            &"".into(),
        );
        assert!(matches!(
            map.canonical_artifact_url(&filename, "example.whl"),
            Err(ArtifactUrlMapError::FilenameChanged { .. })
        ));
        Ok(())
    }

    #[test]
    fn invalid_artifact_url_errors_do_not_retain_unparsed_secrets() -> Result<(), Box<dyn Error>> {
        let map = ArtifactUrlMap::single(
            url("https://proxy.example/files")?,
            url("https://canonical.example/packages")?,
        )
        .validate()?;
        let locations = [
            FileLocation::new(
                "https://[invalid]/example.whl?absolute_token=secret".into(),
                &"".into(),
            ),
            FileLocation::new(
                "example.whl".into(),
                &"https://[invalid]/files?base_token=secret".into(),
            ),
            FileLocation::RelativeUrl(
                "https://proxy.example/files/".into(),
                "https://[invalid]/example.whl?join_token=secret".into(),
            ),
            FileLocation::new(
                "//username-token/name:password-secret@proxy.example/files/example.whl".into(),
                &"https://proxy.example/simple/".into(),
            ),
        ];

        for location in locations {
            let error = map
                .canonical_artifact_url(&location, "example.whl")
                .expect_err("unparsable artifact URLs should be rejected");
            assert_eq!(error, ArtifactUrlMapError::InvalidArtifactUrl);
            let chain = error_chain(&error);
            assert!(!chain.contains("secret"));
            assert!(!chain.contains("token"));
        }
        Ok(())
    }

    #[test]
    fn detects_ambiguous_match_defensively() -> Result<(), Box<dyn Error>> {
        let map = ValidatedArtifactUrlMap {
            mappings: vec![
                ArtifactUrlMapping {
                    physical: url("https://proxy.example/")?,
                    canonical: url("https://canonical.example/root")?,
                },
                ArtifactUrlMapping {
                    physical: url("https://proxy.example/files")?,
                    canonical: url("https://canonical.example/files")?,
                },
            ],
        };
        let physical =
            FileLocation::new("https://proxy.example/files/example.whl".into(), &"".into());
        assert!(matches!(
            map.canonical_artifact_url(&physical, "example.whl"),
            Err(ArtifactUrlMapError::Ambiguous { .. })
        ));
        Ok(())
    }

    fn artifact_url_map<const N: usize>(
        mappings: [(&str, &str); N],
    ) -> Result<ArtifactUrlMap, uv_redacted::DisplaySafeUrlError> {
        mappings
            .into_iter()
            .map(|(physical, canonical)| Ok((url(physical)?, url(canonical)?)))
            .collect()
    }

    fn url(value: &str) -> Result<DisplaySafeUrl, uv_redacted::DisplaySafeUrlError> {
        DisplaySafeUrl::parse(value)
    }

    fn error_chain(mut error: &(dyn Error + 'static)) -> String {
        let mut messages = Vec::new();
        loop {
            messages.push(error.to_string());
            let Some(source) = error.source() else {
                break;
            };
            error = source;
        }
        messages.join(": ")
    }
}
