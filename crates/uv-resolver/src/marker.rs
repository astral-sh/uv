use crate::requires_python::{LowerBound, RequiresPythonRange, UpperBound};
use pep440_rs::Version;
use pep508_rs::{MarkerTree, MarkerTreeKind, MarkerValueVersion};
use pubgrub::Range;

/// Returns the bounding Python versions that can satisfy the [`MarkerTree`], if it's constrained.
pub(crate) fn requires_python(tree: &MarkerTree) -> Option<RequiresPythonRange> {
    fn collect_python_markers(tree: &MarkerTree, markers: &mut Vec<Range<Version>>) {
        match tree.kind() {
            MarkerTreeKind::True | MarkerTreeKind::False => {}
            MarkerTreeKind::Version(marker) => match marker.key() {
                MarkerValueVersion::PythonVersion | MarkerValueVersion::PythonFullVersion => {
                    for (range, tree) in marker.edges() {
                        if !tree.is_false() {
                            markers.push(range.clone());
                        }
                    }
                }
                MarkerValueVersion::ImplementationVersion => {
                    for (_, tree) in marker.edges() {
                        collect_python_markers(&tree, markers);
                    }
                }
            },
            MarkerTreeKind::String(marker) => {
                for (_, tree) in marker.children() {
                    collect_python_markers(&tree, markers);
                }
            }
            MarkerTreeKind::In(marker) => {
                for (_, tree) in marker.children() {
                    collect_python_markers(&tree, markers);
                }
            }
            MarkerTreeKind::Contains(marker) => {
                for (_, tree) in marker.children() {
                    collect_python_markers(&tree, markers);
                }
            }
            MarkerTreeKind::Extra(marker) => {
                for (_, tree) in marker.children() {
                    collect_python_markers(&tree, markers);
                }
            }
        }
    }

    let mut markers = Vec::new();
    collect_python_markers(tree, &mut markers);

    // Take the union of all Python version markers.
    let range = markers
        .into_iter()
        .fold(None, |acc: Option<Range<Version>>, range| {
            Some(match acc {
                Some(acc) => acc.union(&range),
                None => range.clone(),
            })
        })?;

    let (lower, upper) = range.bounding_range()?;

    Some(RequiresPythonRange::new(
        LowerBound::new(lower.cloned()),
        UpperBound::new(upper.cloned()),
    ))
}
