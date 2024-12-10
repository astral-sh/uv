use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::{Direction, Graph};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::collections::hash_map::Entry;

use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pypi_types::{ConflictItem, Conflicts};

use crate::resolution::ResolutionGraphNode;
use crate::universal_marker::UniversalMarker;

/// Determine the markers under which a package is reachable in the dependency tree.
///
/// The algorithm is a variant of Dijkstra's algorithm for not totally ordered distances:
/// Whenever we find a shorter distance to a node (a marker that is not a subset of the existing
/// marker), we re-queue the node and update all its children. This implicitly handles cycles,
/// whenever we re-reach a node through a cycle the marker we have is a more
/// specific marker/longer path, so we don't update the node and don't re-queue it.
pub(crate) fn marker_reachability<T>(
    graph: &Graph<T, UniversalMarker>,
    fork_markers: &[UniversalMarker],
) -> FxHashMap<NodeIndex, UniversalMarker> {
    // Note that we build including the virtual packages due to how we propagate markers through
    // the graph, even though we then only read the markers for base packages.
    let mut reachability = FxHashMap::with_capacity_and_hasher(graph.node_count(), FxBuildHasher);

    // Collect the root nodes.
    //
    // Besides the actual virtual root node, virtual dev dependencies packages are also root
    // nodes since the edges don't cover dev dependencies.
    let mut queue: Vec<_> = graph
        .node_indices()
        .filter(|node_index| {
            graph
                .edges_directed(*node_index, Direction::Incoming)
                .next()
                .is_none()
        })
        .collect();

    // The root nodes are always applicable, unless the user has restricted resolver
    // environments with `tool.uv.environments`.
    let root_markers = if fork_markers.is_empty() {
        UniversalMarker::TRUE
    } else {
        fork_markers
            .iter()
            .fold(UniversalMarker::FALSE, |mut acc, marker| {
                acc.or(*marker);
                acc
            })
    };
    for root_index in &queue {
        reachability.insert(*root_index, root_markers);
    }

    // Propagate all markers through the graph, so that the eventual marker for each node is the
    // union of the markers of each path we can reach the node by.
    while let Some(parent_index) = queue.pop() {
        let marker = reachability[&parent_index];
        for child_edge in graph.edges_directed(parent_index, Direction::Outgoing) {
            // The marker for all paths to the child through the parent.
            let mut child_marker = *child_edge.weight();
            child_marker.and(marker);
            match reachability.entry(child_edge.target()) {
                Entry::Occupied(mut existing) => {
                    // If the marker is a subset of the existing marker (A ⊆ B exactly if
                    // A ∪ B = A), updating the child wouldn't change child's marker.
                    child_marker.or(*existing.get());
                    if &child_marker != existing.get() {
                        existing.insert(child_marker);
                        queue.push(child_edge.target());
                    }
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(child_marker);
                    queue.push(child_edge.target());
                }
            }
        }
    }

    reachability
}

/// Traverse the given dependency graph and propagate activated markers.
///
/// For example, given an edge like `foo[x1] -> bar`, then it is known that
/// `x1` is activated. This in turn can be used to simplify any downstream
/// conflict markers with `extra == "x1"` in them (by replacing `extra == "x1"`
/// with `true`).
pub(crate) fn simplify_conflict_markers(
    conflicts: &Conflicts,
    graph: &mut Graph<ResolutionGraphNode, UniversalMarker>,
) {
    /// An inference about whether a conflicting item is always included or
    /// excluded.
    ///
    /// We collect these for each node in the graph after determining which
    /// extras/groups are activated for each node. Once we know what's
    /// activated, we can infer what must also be *inactivated* based on what's
    /// conflicting with it. So for example, if we have a conflict marker like
    /// `extra == 'foo' and extra != 'bar'`, and `foo` and `bar` have been
    /// declared as conflicting, and we are in a part of the graph where we
    /// know `foo` must be activated, then it follows that `extra != 'bar'`
    /// must always be true. Because if it were false, it would imply both
    /// `foo` and `bar` were activated simultaneously, which uv guarantees
    /// won't happen.
    ///
    /// We then use these inferences to simplify the conflict markers.
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    struct Inference {
        item: ConflictItem,
        included: bool,
    }

    // Do nothing if there are no declared conflicts. Without any declared
    // conflicts, we know we have no conflict markers and thus nothing to
    // simplify by determining which extras are activated at different points
    // in the dependency graph.
    if conflicts.is_empty() {
        return;
    }

    // The set of activated extras and groups for each node. The ROOT nodes
    // don't have any extras/groups activated.
    let mut activated: FxHashMap<NodeIndex, Vec<FxHashSet<ConflictItem>>> = FxHashMap::default();

    // Collect the root nodes.
    //
    // Besides the actual virtual root node, virtual dev dependencies packages are also root
    // nodes since the edges don't cover dev dependencies.
    let mut queue: Vec<_> = graph
        .node_indices()
        .filter(|node_index| {
            graph
                .edges_directed(*node_index, Direction::Incoming)
                .next()
                .is_none()
        })
        .collect();

    let mut seen: FxHashSet<NodeIndex> = FxHashSet::default();
    while let Some(parent_index) = queue.pop() {
        if let Some((package, extra)) = graph[parent_index].package_extra_names() {
            for set in activated
                .entry(parent_index)
                .or_insert_with(|| vec![FxHashSet::default()])
            {
                set.insert(ConflictItem::from((package.clone(), extra.clone())));
            }
        }
        if let Some((package, group)) = graph[parent_index].package_group_names() {
            for set in activated
                .entry(parent_index)
                .or_insert_with(|| vec![FxHashSet::default()])
            {
                set.insert(ConflictItem::from((package.clone(), group.clone())));
            }
        }
        let sets = activated.get(&parent_index).cloned().unwrap_or_default();
        for child_edge in graph.edges_directed(parent_index, Direction::Outgoing) {
            let mut change = false;
            for set in sets.clone() {
                let existing = activated.entry(child_edge.target()).or_default();
                // This is doing a linear scan for testing membership, which
                // is non-ideal. But it's not actually clear that there's a
                // strictly better alternative without a real workload being
                // slow because of this. Namely, we are checking whether the
                // _set_ being inserted is equivalent to an existing set. So
                // instead of, say, `Vec<FxHashSet<ConflictItem>>`, we could
                // have `BTreeSet<BTreeSet<ConflictItem>>`. But this in turn
                // makes mutating the elements in each set (done above) more
                // difficult and likely require more allocations.
                //
                // So if this does result in a perf slowdown on some real
                // work-load, I think the first step would be to re-examine
                // whether we're doing more work than we need to be doing. If
                // we aren't, then we might want a more purpose-built data
                // structure for this.
                if !existing.contains(&set) {
                    existing.push(set);
                    change = true;
                }
            }
            if seen.insert(child_edge.target()) || change {
                queue.push(child_edge.target());
            }
        }
    }

    let mut inferences: FxHashMap<NodeIndex, Vec<FxHashSet<Inference>>> = FxHashMap::default();
    for (node_id, sets) in activated {
        let mut new_sets = vec![];
        for set in sets {
            let mut new_set = FxHashSet::default();
            for item in set {
                for conflict_set in conflicts.iter() {
                    if !conflict_set.contains(item.package(), item.as_ref().conflict()) {
                        continue;
                    }
                    for conflict_item in conflict_set.iter() {
                        if conflict_item == &item {
                            continue;
                        }
                        new_set.insert(Inference {
                            item: conflict_item.clone(),
                            included: false,
                        });
                    }
                }
                new_set.insert(Inference {
                    item,
                    included: true,
                });
            }
            new_sets.push(new_set);
        }
        inferences.insert(node_id, new_sets);
    }

    for edge_index in (0..graph.edge_count()).map(EdgeIndex::new) {
        let (from_index, _) = graph.edge_endpoints(edge_index).unwrap();
        let Some(inference_sets) = inferences.get(&from_index) else {
            continue;
        };
        // If not all possible paths (represented by our inferences)
        // satisfy the conflict marker on this edge, then we can't make any
        // simplifications. Namely, because it follows that out inferences
        // aren't always true. Some of them may sometimes be false.
        let all_paths_satisfied = inference_sets.iter().all(|set| {
            let extras = set
                .iter()
                .filter_map(|inf| {
                    if !inf.included {
                        return None;
                    }
                    Some((inf.item.package().clone(), inf.item.extra()?.clone()))
                })
                .collect::<Vec<(PackageName, ExtraName)>>();
            let groups = set
                .iter()
                .filter_map(|inf| {
                    if !inf.included {
                        return None;
                    }
                    Some((inf.item.package().clone(), inf.item.group()?.clone()))
                })
                .collect::<Vec<(PackageName, GroupName)>>();
            graph[edge_index].conflict().evaluate(&extras, &groups)
        });
        if !all_paths_satisfied {
            continue;
        }
        for set in inference_sets {
            for inf in set {
                if inf.included {
                    graph[edge_index].assume_conflict_item(&inf.item);
                } else {
                    graph[edge_index].assume_not_conflict_item(&inf.item);
                }
            }
        }
    }
}
