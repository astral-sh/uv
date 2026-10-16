//! Validate activation requirements independently of installation markers.
//!
//! Request nodes retain the declared extras before choosing a resolved package version. Reachability
//! remains symbolic until each activated item has a grounded request and every reached requirement
//! has exactly one target. Declared conflicts then constrain the result, which is projected onto
//! environment markers for installation. No stage enumerates DNF clauses or complete assignments.

use std::collections::BTreeMap;

use itertools::Itertools;
use petgraph::Graph;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use rustc_hash::{FxHashMap, FxHashSet};
use uv_normalize::ExtraName;
use uv_pep508::MarkerTree;
use uv_pypi_types::{ConflictItem, ConflictSet, Conflicts};

use crate::{ConflictMarker, UniversalMarker};

/// A package, dependency section, or requirement in the activation graph.
///
/// Requests are recorded before selecting a target version. A requirement with alternative
/// locked versions has `requires_choice` set; exactly one outgoing edge must be selected.
/// Ordinary package nodes can require all of their outgoing dependencies.
#[derive(Debug, Default)]
pub struct ActivationNode {
    pub requests: Vec<ConflictItem>,
    pub requires_choice: bool,
}

/// Conditional requests and version choices, before simplification under declared conflicts.
///
/// Each package version has a distinct node. An edge guard may depend on the final activated
/// extras. Request edges must not conjoin the global conflict constraint: invalid requests must
/// remain observable before a version is chosen. Roots are supplied explicitly so disconnected
/// cycles cannot activate themselves.
#[derive(Debug, Default)]
pub struct ActivationGraph {
    pub graph: Graph<ActivationNode, MarkerTree>,
}

/// A selection whose requests have a unique, supported interpretation in every environment.
#[derive(Debug)]
pub struct ValidatedSelection {
    reachability: Vec<MarkerTree>,
}

impl ValidatedSelection {
    /// Return the environments in which this node belongs to the validated selection.
    ///
    /// All conflict variables have been eliminated. The result can drive installation for a
    /// concrete environment or export across the validated environment range.
    pub fn marker(&self, node: NodeIndex) -> MarkerTree {
        self.reachability[node.index()]
    }
}

/// Environments in which the recorded requests cannot select one consistent solution.
#[derive(Debug, PartialEq, Eq)]
pub enum SelectionError {
    /// The requests require two items from the same declared conflict set.
    Conflict {
        first: ConflictItem,
        second: ConflictItem,
        environment: MarkerTree,
    },
    /// No assignment satisfies both the requests and the declared conflicts.
    Unsatisfiable(MarkerTree),
    /// Distinct supported assignments activate different conflict items.
    Ambiguous(MarkerTree),
    /// An edge refers to an item absent from the requests and conflict declarations.
    UnknownConflictItem(ExtraName),
}

impl ActivationGraph {
    /// Resolve requests to a least reachable closure, then validate their conflict assignments.
    ///
    /// Edge guards remain symbolic throughout propagation. An assumed extra must be requested
    /// by a reachable node, and every requested extra must be assumed active. This prevents
    /// negative guards from treating an undiscovered extra as disabled. Requests must be justified
    /// without assuming their own activation, so positive cycles cannot enable themselves.
    ///
    /// `environment` and root conditions must contain only environment expressions. The graph
    /// must retain the original request conditions and distinguish alternative package versions.
    pub fn select(
        &self,
        roots: impl IntoIterator<Item = (NodeIndex, MarkerTree)>,
        environment: MarkerTree,
        conflicts: &Conflicts,
    ) -> Result<ValidatedSelection, SelectionError> {
        let mut requested = BTreeMap::new();
        for item in conflicts.iter().flat_map(ConflictSet::iter) {
            requested.insert(item, MarkerTree::FALSE);
        }
        for node in self.graph.node_weights() {
            for item in &node.requests {
                requested.insert(item, MarkerTree::FALSE);
            }
        }
        let mut variables = Vec::new();
        for item in requested.keys() {
            conflict_variable(item).visit_extras(|_, extra| {
                variables.push((extra.clone(), *item));
            });
        }
        let names: FxHashSet<_> = variables.iter().map(|(name, _)| name).collect();
        for edge in self.graph.edge_weights() {
            let mut unknown = None;
            edge.visit_extras(|_, extra| {
                if !names.contains(extra) {
                    unknown = Some(extra.clone());
                }
            });
            if let Some(extra) = unknown {
                return Err(SelectionError::UnknownConflictItem(extra));
            }
        }

        let mut reachability = vec![MarkerTree::FALSE; self.graph.node_count()];
        let mut queue = Vec::new();
        for (node, marker) in roots {
            let combined = reachability[node.index()].or(marker.and(environment));
            if combined != reachability[node.index()] {
                reachability[node.index()] = combined;
                queue.push(node);
            }
        }
        while let Some(parent) = queue.pop() {
            for edge in self.graph.edges(parent) {
                let child = edge.target();
                let marker = reachability[parent.index()].and(*edge.weight());
                let combined = reachability[child.index()].or(marker);
                if combined != reachability[child.index()] {
                    reachability[child.index()] = combined;
                    queue.push(child);
                }
            }
        }
        for node in self.graph.node_indices() {
            for item in &self.graph[node].requests {
                let condition = requested.entry(item).or_insert(MarkerTree::FALSE);
                *condition = condition.or(reachability[node.index()]);
            }
        }

        // Merge all paths before deriving activations: a requirement shared by every version
        // is justified without first choosing a version, even when the choice needs that extra.
        let mut justified: BTreeMap<_, _> = requested
            .keys()
            .map(|item| (*item, MarkerTree::FALSE))
            .collect();
        loop {
            let bounds: Vec<_> = variables
                .iter()
                .map(|(_, item)| (justified[item], conflict_variable(item)))
                .collect();
            let mut cache = FxHashMap::default();
            let mut changed = false;
            for (item, condition) in &requested {
                let marker = supported_guard(*condition, &bounds, 0, &mut cache);
                let existing = justified.entry(item).or_insert(MarkerTree::FALSE);
                let combined = existing.or(marker);
                changed |= combined != *existing;
                *existing = combined;
            }
            if !changed {
                break;
            }
        }

        // An activation determined by the environment needs no symbolic assignment. Substitute
        // it before constructing constraints, leaving only mutually dependent activations to solve.
        let mut unresolved = Vec::new();
        let mut activations = BTreeMap::new();
        for (item, condition) in &justified {
            let mut symbolic = false;
            condition.visit_extras(|_, _| symbolic = true);
            let activation = if symbolic {
                unresolved.push(*item);
                conflict_variable(item)
            } else {
                *condition
            };
            activations.insert(*item, activation);
        }
        let substitutions: Vec<_> = variables
            .iter()
            .map(|(name, item)| (name.clone(), activations[item]))
            .collect();
        let mut projected = FxHashMap::default();
        let mut project = |marker: MarkerTree| {
            *projected
                .entry(marker)
                .or_insert_with(|| marker.resolve_extras(substitutions.iter().cloned()))
        };
        for marker in &mut reachability {
            *marker = project(*marker);
        }
        let mut valid = environment;
        for node in self.graph.node_indices() {
            if self.graph[node].requires_choice {
                let mut covered = MarkerTree::FALSE;
                let mut overlap = MarkerTree::FALSE;
                for edge in self.graph.edges(node) {
                    let marker = project(*edge.weight());
                    overlap = overlap.or(covered.and(marker));
                    covered = covered.or(marker);
                }
                valid =
                    valid.and(reachability[node.index()].implies(covered.and(overlap.negate())));
            }
        }
        for item in &unresolved {
            let condition = project(justified[item]);
            let variable = conflict_variable(item);
            valid = valid.and(variable.implies(condition));
            valid = valid.and(condition.implies(variable));
        }
        // Enforce at most one active item per set within the supported assignments. Accumulating
        // prior activations avoids constructing a separate constraint for every pair.
        let mut constrained = valid;
        for set in conflicts.iter() {
            let mut active = MarkerTree::FALSE;
            for item in set.iter() {
                let activation = activations[item];
                constrained = constrained.and(active.and(activation).negate());
                active = active.or(constrained.and(activation));
            }
        }
        let invalid = environment.and(constrained.without_extras().negate());
        if !invalid.is_false() {
            for set in conflicts.iter() {
                for (first, second) in set.iter().tuple_combinations() {
                    let condition = valid
                        .and(invalid)
                        .and(activations[first])
                        .and(activations[second])
                        .without_extras();
                    if !condition.is_false() {
                        return Err(SelectionError::Conflict {
                            first: first.clone(),
                            second: second.clone(),
                            environment: condition,
                        });
                    }
                }
            }
            return Err(SelectionError::Unsatisfiable(invalid));
        }
        let valid = constrained;
        let mut ambiguous = MarkerTree::FALSE;
        for item in &unresolved {
            let variable = conflict_variable(item);
            let enabled = valid.and(variable).without_extras();
            let disabled = valid.and(variable.negate()).without_extras();
            ambiguous = ambiguous.or(enabled.and(disabled));
        }
        if !ambiguous.is_false() {
            return Err(SelectionError::Ambiguous(ambiguous));
        }
        Ok(ValidatedSelection {
            reachability: reachability
                .into_iter()
                .map(|marker| marker.and(valid).without_extras())
                .collect(),
        })
    }
}

/// Require a guard to hold for every completion between justified and assumed activations.
///
/// A positive extra cannot justify its own request. A negative extra can be used when the
/// candidate assignment excludes it; consistency is checked after reaching the fixed point.
/// Substitution leaves assumed variables symbolic, so all candidate assignments are handled
/// together without enumerating selections or expanding the diagram into DNF.
fn supported_guard(
    marker: MarkerTree,
    bounds: &[(MarkerTree, MarkerTree)],
    index: usize,
    cache: &mut FxHashMap<(MarkerTree, usize), MarkerTree>,
) -> MarkerTree {
    let Some((justified, assumed)) = bounds.get(index) else {
        return marker;
    };
    if marker.is_true() || marker.is_false() {
        return marker;
    }
    if let Some(result) = cache.get(&(marker, index)) {
        return *result;
    }
    let low = marker.restrict(assumed.negate());
    let high = marker.restrict(*assumed);
    let result = if low == high {
        supported_guard(low, bounds, index + 1, cache)
    } else {
        let low = supported_guard(low, bounds, index + 1, cache);
        let high = supported_guard(high, bounds, index + 1, cache);
        justified.or(low).and(assumed.negate().or(high))
    };
    cache.insert((marker, index), result);
    result
}

/// Encode an activation item in the shared decision-diagram variable namespace.
fn conflict_variable(item: &ConflictItem) -> MarkerTree {
    UniversalMarker::new(MarkerTree::TRUE, ConflictMarker::from_conflict_item(item)).combined()
}

#[cfg(test)]
mod tests;
