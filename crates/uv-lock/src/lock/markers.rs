use std::collections::BTreeMap;
use std::str::FromStr;

use uv_distribution_types::{RequiresPython, SimplifiedMarkerTree};
use uv_pep508::MarkerTree;
use uv_pypi_types::ConflictItem;
use uv_resolver_types::{ConflictMarker, ConflictMarkerError, UniversalMarker};

/// A lockfile marker expressed relative to the enclosing Python requirement.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, PartialOrd, Ord)]
pub(super) struct MarkerWire(MarkerTree);

impl MarkerWire {
    /// Restores the Python requirement omitted from the serialized marker.
    pub(super) fn into_marker(self, requires_python: &RequiresPython) -> MarkerTree {
        requires_python.complexify_markers(self.0)
    }

    /// Normalizes a dependency marker under the lockfile's Python requirement.
    pub(super) fn into_simplified(self, requires_python: &RequiresPython) -> SimplifiedMarkerTree {
        SimplifiedMarkerTree::new(requires_python, self.0)
    }
}

impl<'de> serde::Deserialize<'de> for MarkerWire {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        MarkerValue::deserialize(deserializer)?
            .into_marker()
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

/// Ordinary environment markers remain strings; conflicts use explicit Boolean clauses.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub(super) enum MarkerValue {
    String(String),
    Any(MarkerAlternatives),
    Clause(MarkerClause),
}

/// A disjunction of clauses, each preserving its own environment and conflict predicates.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MarkerAlternatives {
    any: Vec<MarkerClause>,
}

/// All predicates in a clause must hold for it to match.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MarkerClause {
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    enabled: Vec<ConflictItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    disabled: Vec<ConflictItem>,
}

impl MarkerValue {
    /// Separates encoded conflict predicates without losing their relationship to environments.
    pub(super) fn from_marker(
        marker: MarkerTree,
        version: u32,
    ) -> Result<Self, ConflictMarkerError> {
        if version < 2 || marker.without_extras() == marker {
            return Ok(Self::String(marker.try_to_string().unwrap_or_default()));
        }

        let mut clauses = BTreeMap::new();
        for conjunction in marker.to_dnf() {
            let conjunction = conjunction
                .into_iter()
                .map(MarkerTree::expression)
                .fold(MarkerTree::TRUE, MarkerTree::and);
            let (mut enabled, mut disabled) = UniversalMarker::from_combined(conjunction)
                .conflict()
                .filter_rules()?;
            enabled.sort();
            disabled.sort();
            let environment = conjunction.without_extras();
            clauses
                .entry((enabled, disabled))
                .and_modify(|marker: &mut MarkerTree| *marker = marker.or(environment))
                .or_insert(environment);
        }
        let mut clauses = clauses
            .into_iter()
            .map(|((enabled, disabled), environment)| MarkerClause {
                environment: environment.try_to_string(),
                enabled,
                disabled,
            })
            .collect::<Vec<_>>();
        Ok(if clauses.len() == 1 {
            Self::Clause(clauses.remove(0))
        } else {
            Self::Any(MarkerAlternatives { any: clauses })
        })
    }

    /// Reconstructs the combined marker consumed by resolution and installation.
    fn into_marker(self) -> Result<MarkerTree, String> {
        match self {
            Self::String(marker) => MarkerTree::from_str(&marker).map_err(|err| err.to_string()),
            Self::Clause(clause) => clause.into_marker(),
            Self::Any(alternatives) => alternatives
                .any
                .into_iter()
                .try_fold(MarkerTree::FALSE, |marker, clause| {
                    Ok(marker.or(clause.into_marker()?))
                }),
        }
    }
}

impl MarkerClause {
    /// Reconstructs a conjunction of environment, enabled-item, and disabled-item predicates.
    fn into_marker(self) -> Result<MarkerTree, String> {
        let environment = self
            .environment
            .map(|marker| MarkerTree::from_str(&marker).map_err(|err| err.to_string()))
            .transpose()?
            .unwrap_or(MarkerTree::TRUE);
        if environment.without_extras() != environment {
            return Err(
                "conflict conditions belong in enabled or disabled, not environment".to_owned(),
            );
        }
        let mut conflicts = ConflictMarker::TRUE;
        for item in &self.enabled {
            conflicts = conflicts.and(ConflictMarker::from_conflict_item(item));
        }
        for item in &self.disabled {
            conflicts = conflicts.and(ConflictMarker::from_conflict_item(item).negate());
        }
        Ok(UniversalMarker::new(environment, conflicts).combined())
    }
}
