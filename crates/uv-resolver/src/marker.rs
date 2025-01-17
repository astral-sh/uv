use pubgrub::Ranges;

use uv_pep440::Version;
use uv_pep508::{CanonicalMarkerValueVersion, MarkerTree, MarkerTreeKind};

use crate::requires_python::{LowerBound, RequiresPythonRange, UpperBound};

/// Returns the bounding Python versions that can satisfy the [`MarkerTree`], if it's constrained.
pub(crate) fn requires_python(tree: MarkerTree) -> Option<RequiresPythonRange> {
    /// Collect the Python version markers from the tree.
    ///
    /// Specifically, performs a DFS to collect all Python requirements on the path to every
    /// `MarkerTreeKind::True` node.
    fn collect_python_markers(
        tree: MarkerTree,
        markers: &mut Vec<Vec<Ranges<Version>>>,
        current_path: &mut Vec<Ranges<Version>>,
    ) {
        match tree.kind() {
            MarkerTreeKind::True => {
                markers.push(current_path.clone());
            }
            MarkerTreeKind::False => {}
            MarkerTreeKind::Version(marker) => match marker.key() {
                CanonicalMarkerValueVersion::PythonFullVersion => {
                    for (range, tree) in marker.edges() {
                        current_path.push(range.clone());
                        collect_python_markers(tree, markers, current_path);
                        current_path.pop();
                    }
                }
                CanonicalMarkerValueVersion::ImplementationVersion => {
                    for (_, tree) in marker.edges() {
                        collect_python_markers(tree, markers, current_path);
                    }
                }
            },
            MarkerTreeKind::String(marker) => {
                for (_, tree) in marker.children() {
                    collect_python_markers(tree, markers, current_path);
                }
            }
            MarkerTreeKind::In(marker) => {
                for (_, tree) in marker.children() {
                    collect_python_markers(tree, markers, current_path);
                }
            }
            MarkerTreeKind::Contains(marker) => {
                for (_, tree) in marker.children() {
                    collect_python_markers(tree, markers, current_path);
                }
            }
            MarkerTreeKind::Extra(marker) => {
                for (_, tree) in marker.children() {
                    collect_python_markers(tree, markers, current_path);
                }
            }
        }
    }

    let mut markers = Vec::new();
    collect_python_markers(tree, &mut markers, &mut Vec::new());

    // If there are no Python version markers, return `None`.
    if markers.iter().all(Vec::is_empty) {
        return None;
    }

    // Take the union of the intersections of the Python version markers.
    let range = markers
        .into_iter()
        .map(|ranges| {
            ranges
                .into_iter()
                .fold(Ranges::full(), |acc: Ranges<Version>, range| {
                    acc.intersection(&range)
                })
        })
        .fold(Ranges::empty(), |acc: Ranges<Version>, range| {
            acc.union(&range)
        });

    let (lower, upper) = range.bounding_range()?;

    Some(RequiresPythonRange::new(
        LowerBound::new(lower.cloned()),
        UpperBound::new(upper.cloned()),
    ))
}

#[cfg(test)]
mod tests {
    use std::ops::Bound;
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_requires_python() {
        // An exact version match.
        let tree = MarkerTree::from_str("python_full_version == '3.8.*'").unwrap();
        let range = requires_python(tree).unwrap();
        assert_eq!(
            *range.lower(),
            LowerBound::new(Bound::Included(Version::from_str("3.8").unwrap()))
        );
        assert_eq!(
            *range.upper(),
            UpperBound::new(Bound::Excluded(Version::from_str("3.9").unwrap()))
        );

        // A version range with exclusive bounds.
        let tree =
            MarkerTree::from_str("python_full_version > '3.8' and python_full_version < '3.9'")
                .unwrap();
        let range = requires_python(tree).unwrap();
        assert_eq!(
            *range.lower(),
            LowerBound::new(Bound::Excluded(Version::from_str("3.8").unwrap()))
        );
        assert_eq!(
            *range.upper(),
            UpperBound::new(Bound::Excluded(Version::from_str("3.9").unwrap()))
        );

        // A version range with inclusive bounds.
        let tree =
            MarkerTree::from_str("python_full_version >= '3.8' and python_full_version <= '3.9'")
                .unwrap();
        let range = requires_python(tree).unwrap();
        assert_eq!(
            *range.lower(),
            LowerBound::new(Bound::Included(Version::from_str("3.8").unwrap()))
        );
        assert_eq!(
            *range.upper(),
            UpperBound::new(Bound::Included(Version::from_str("3.9").unwrap()))
        );

        // A version with a lower bound.
        let tree = MarkerTree::from_str("python_full_version >= '3.8'").unwrap();
        let range = requires_python(tree).unwrap();
        assert_eq!(
            *range.lower(),
            LowerBound::new(Bound::Included(Version::from_str("3.8").unwrap()))
        );
        assert_eq!(*range.upper(), UpperBound::new(Bound::Unbounded));

        // A version with an upper bound.
        let tree = MarkerTree::from_str("python_full_version < '3.9'").unwrap();
        let range = requires_python(tree).unwrap();
        assert_eq!(*range.lower(), LowerBound::new(Bound::Unbounded));
        assert_eq!(
            *range.upper(),
            UpperBound::new(Bound::Excluded(Version::from_str("3.9").unwrap()))
        );

        // A disjunction with a non-Python marker (i.e., an unbounded range).
        let tree =
            MarkerTree::from_str("python_full_version > '3.8' or sys_platform == 'win32'").unwrap();
        let range = requires_python(tree).unwrap();
        assert_eq!(*range.lower(), LowerBound::new(Bound::Unbounded));
        assert_eq!(*range.upper(), UpperBound::new(Bound::Unbounded));

        // A complex mix of conjunctions and disjunctions.
        let tree = MarkerTree::from_str("(python_full_version >= '3.8' and python_full_version < '3.9') or (python_full_version >= '3.10' and python_full_version < '3.11')").unwrap();
        let range = requires_python(tree).unwrap();
        assert_eq!(
            *range.lower(),
            LowerBound::new(Bound::Included(Version::from_str("3.8").unwrap()))
        );
        assert_eq!(
            *range.upper(),
            UpperBound::new(Bound::Excluded(Version::from_str("3.11").unwrap()))
        );
    }
}
