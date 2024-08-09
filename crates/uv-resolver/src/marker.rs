use pep508_rs::{MarkerTree, MarkerTreeKind, MarkerValueVersion};

use crate::RequiresPythonBound;

/// Returns the minimum Python version that can satisfy the [`MarkerTree`], if it's constrained.
pub(crate) fn requires_python(tree: &MarkerTree) -> Option<RequiresPythonBound> {
    fn collect_python_markers(tree: &MarkerTree, markers: &mut Vec<RequiresPythonBound>) {
        match tree.kind() {
            MarkerTreeKind::True | MarkerTreeKind::False => {}
            MarkerTreeKind::Version(marker) => match marker.key() {
                MarkerValueVersion::PythonVersion | MarkerValueVersion::PythonFullVersion => {
                    for (range, tree) in marker.edges() {
                        if !tree.is_false() {
                            // Extract the lower bound.
                            let (lower, _) = range.iter().next().unwrap();
                            markers.push(RequiresPythonBound::new(lower.clone()));
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
    markers.into_iter().min()
}
