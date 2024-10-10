use crate::{LenientRequirement, MetadataError, VerbatimParsedUrl};
use serde::Deserialize;
use std::io::BufRead;
use std::str::FromStr;
use uv_normalize::ExtraName;
use uv_pep508::marker::MarkerValueExtra;
use uv_pep508::{ExtraOperator, MarkerExpression, MarkerTree, Requirement};

/// `requires.txt` metadata as defined in <https://setuptools.pypa.io/en/latest/deprecated/python_eggs.html#dependency-metadata>.
///
/// This is a subset of the full metadata specification, and only includes the fields that are
/// included in the legacy `requires.txt` file.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct RequiresTxt {
    pub requires_dist: Vec<Requirement<VerbatimParsedUrl>>,
    pub provides_extras: Vec<ExtraName>,
}

impl RequiresTxt {
    /// Parse the [`RequiresTxt`] from a `requires.txt` file, as included in an `egg-info`.
    ///
    /// See: <https://setuptools.pypa.io/en/latest/deprecated/python_eggs.html#dependency-metadata>
    pub fn parse(content: &[u8]) -> Result<Self, MetadataError> {
        let mut requires_dist = vec![];
        let mut provides_extras = vec![];
        let mut current_marker = MarkerTree::default();

        for line in content.lines() {
            let line = line.map_err(MetadataError::RequiresTxtContents)?;

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // When encountering a new section, parse the extra and marker from the header, e.g.,
            // `[:sys_platform == "win32"]` or `[dev]`.
            if line.starts_with('[') {
                let line = line.trim_start_matches('[').trim_end_matches(']');

                // Split into extra and marker, both of which can be empty.
                let (extra, marker) = {
                    let (extra, marker) = match line.split_once(':') {
                        Some((extra, marker)) => (Some(extra), Some(marker)),
                        None => (Some(line), None),
                    };
                    let extra = extra.filter(|extra| !extra.is_empty());
                    let marker = marker.filter(|marker| !marker.is_empty());
                    (extra, marker)
                };

                // Parse the extra.
                let extra = if let Some(extra) = extra {
                    if let Ok(extra) = ExtraName::from_str(extra) {
                        provides_extras.push(extra.clone());
                        Some(MarkerValueExtra::Extra(extra))
                    } else {
                        Some(MarkerValueExtra::Arbitrary(extra.to_string()))
                    }
                } else {
                    None
                };

                // Parse the marker.
                let marker = marker.map(MarkerTree::parse_str).transpose()?;

                // Create the marker tree.
                match (extra, marker) {
                    (Some(extra), Some(mut marker)) => {
                        marker.and(MarkerTree::expression(MarkerExpression::Extra {
                            operator: ExtraOperator::Equal,
                            name: extra,
                        }));
                        current_marker = marker;
                    }
                    (Some(extra), None) => {
                        current_marker = MarkerTree::expression(MarkerExpression::Extra {
                            operator: ExtraOperator::Equal,
                            name: extra,
                        });
                    }
                    (None, Some(marker)) => {
                        current_marker = marker;
                    }
                    (None, None) => {
                        current_marker = MarkerTree::default();
                    }
                }

                continue;
            }

            // Parse the requirement.
            let requirement =
                Requirement::<VerbatimParsedUrl>::from(LenientRequirement::from_str(line)?);

            // Add the markers and extra, if necessary.
            requires_dist.push(Requirement {
                marker: current_marker.clone(),
                ..requirement
            });
        }

        Ok(Self {
            requires_dist,
            provides_extras,
        })
    }
}

#[cfg(test)]
mod tests;
