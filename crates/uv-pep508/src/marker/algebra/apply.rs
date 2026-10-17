//! Bounded multiway conjunction, without materializing each pairwise intermediate.

use version_ranges::Ranges;

use super::{Edges, InternerGuard, NodeId, SmallVec, Variable, can_conjoin};

const EXPANSION_BUDGET: usize = 1024;
const CACHE_CAPACITY: usize = 4096;

impl InternerGuard<'_> {
    /// Conjoins four operands with bounded Shannon expansion and tuple memoization.
    ///
    /// Each batch permits at most 1,024 expansions and range intersections. The shared
    /// tuple cache retains at most 4,096 results and is cleared between batches when
    /// full; remaining work uses balanced binary conjunction. Multiple conflict-sensitive
    /// roots also use that path: its intermediate exclusion checks normalize markers.
    pub(crate) fn and_all(&mut self, operands: [NodeId; 4]) -> NodeId {
        if operands
            .iter()
            .filter(|node| {
                !node.is_true()
                    && !node.is_false()
                    && self.shared.node(**node).var.is_conflicting_variable()
            })
            .take(2)
            .count()
            == 2
        {
            return self.and_balanced(operands);
        }
        if self.state.cache_all.len() == CACHE_CAPACITY {
            self.state.cache_all.clear();
        }
        let mut budget = EXPANSION_BUDGET;
        self.and_all_cached(operands, &mut budget)
    }

    fn and_balanced(&mut self, [first, second, third, fourth]: [NodeId; 4]) -> NodeId {
        let left = self.and(first, second);
        let right = self.and(third, fourth);
        self.and(left, right)
    }

    /// Canonicalizes operand tuples before memoizing their joint expansion.
    fn and_all_cached(&mut self, mut operands: [NodeId; 4], budget: &mut usize) -> NodeId {
        operands.sort_unstable();
        let mut unique = [NodeId::TRUE; 4];
        let mut length = 0;
        for node in operands {
            if node.is_false() {
                return NodeId::FALSE;
            }
            if node.is_true() {
                continue;
            }
            if length > 0 {
                if unique[length - 1] == node {
                    continue;
                }
                if unique[length - 1] == node.not() {
                    return NodeId::FALSE;
                }
            }
            unique[length] = node;
            length += 1;
        }
        if length <= 2 {
            return self.and(unique[0], unique[1]);
        }
        if let Some(result) = self.state.cache_all.get(&unique) {
            return *result;
        }
        if *budget == 0 || self.state.cache_all.len() == CACHE_CAPACITY {
            return self.and_balanced(unique);
        }
        *budget -= 1;

        let shared = self.shared;
        let mut first = unique[0];
        for &node in &unique[1..length] {
            if shared.node(node).var < shared.node(first).var {
                first = node;
            }
        }
        let node = shared.node(first);
        let children = match &node.children {
            Edges::Boolean { .. } => {
                let mut high = unique;
                let mut low = unique;
                for (index, &parent) in unique[..length].iter().enumerate() {
                    let candidate = shared.node(parent);
                    if candidate.var == node.var
                        && let Edges::Boolean {
                            high: child_high,
                            low: child_low,
                        } = candidate.children
                    {
                        high[index] = child_high.negate(parent);
                        low[index] = child_low.negate(parent);
                    }
                }
                Some(Edges::Boolean {
                    high: self.and_all_cached(high, budget),
                    low: self.and_all_cached(low, budget),
                })
            }
            Edges::Version { .. } => self
                .and_all_ranges(unique, &node.var, budget, |edges| match edges {
                    Edges::Version { edges } => Some(edges),
                    Edges::String { .. } | Edges::Boolean { .. } => None,
                })
                .map(|edges| Edges::Version { edges }),
            Edges::String { .. } => self
                .and_all_ranges(unique, &node.var, budget, |edges| match edges {
                    Edges::String { edges } => Some(edges),
                    Edges::Version { .. } | Edges::Boolean { .. } => None,
                })
                .map(|edges| Edges::String { edges }),
        };
        let result = if let Some(children) = children {
            self.create_node(node.var.clone(), children)
        } else {
            self.and_balanced(unique)
        };
        if self.state.cache_all.len() < CACHE_CAPACITY {
            self.state.cache_all.insert(unique, result);
        }
        result
    }

    /// Refines matching range decisions into a joint partition before combining children.
    /// Restricted domains can have gaps; only intersections present in every operand survive.
    fn and_all_ranges<T: Clone + Ord>(
        &mut self,
        operands: [NodeId; 4],
        variable: &Variable,
        budget: &mut usize,
        ranges: impl Fn(&Edges) -> Option<&[(Ranges<T>, NodeId)]>,
    ) -> Option<SmallVec<(Ranges<T>, NodeId)>> {
        let mut partitions = SmallVec::new();
        partitions.push((Ranges::full(), operands));
        for (index, parent) in operands.into_iter().enumerate() {
            if parent.is_true() || self.shared.node(parent).var != *variable {
                continue;
            }
            let mut refined = SmallVec::new();
            for (range, children) in &partitions {
                for (candidate, child) in ranges(&self.shared.node(parent).children)? {
                    *budget = budget.checked_sub(1)?;
                    let intersection = range.intersection(candidate);
                    if !intersection.is_empty() {
                        let mut children = *children;
                        children[index] = child.negate(parent);
                        refined.push((intersection, children));
                    }
                }
            }
            partitions = refined;
        }

        let mut combined = SmallVec::new();
        for (range, children) in partitions {
            let child = self.and_all_cached(children, budget);
            if let Some((previous_range, previous)) = combined.last_mut()
                && *previous == child
                && can_conjoin(previous_range, &range)
            {
                *previous_range = previous_range.union(&range);
            } else {
                combined.push((range, child));
            }
        }
        Some(combined)
    }
}

#[cfg(test)]
mod tests {
    use super::super::INTERNER;
    use crate::MarkerTree;

    /// Running out of work while refining a range partition must finish the whole operation.
    #[test]
    fn bounded_apply_partial_partition_fallback() {
        let first: MarkerTree = "python_full_version < '3.11' or extra == 'budget-a'"
            .parse()
            .expect("valid marker");
        let second: MarkerTree = "python_full_version != '3.12' or extra == 'budget-b'"
            .parse()
            .expect("valid marker");
        let third: MarkerTree = "python_full_version >= '3.10' or extra == 'budget-c'"
            .parse()
            .expect("valid marker");
        let fourth: MarkerTree = "extra == 'budget-d'".parse().expect("valid marker");
        let expected = first.and(second).and(third.and(fourth));
        let mut budget = 2;
        let result = INTERNER
            .lock()
            .and_all_cached([first.0, second.0, third.0, fourth.0], &mut budget);
        assert_eq!(result, expected.0);
        assert_eq!(budget, 0);
    }
}
