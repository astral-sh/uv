use std::str::FromStr;

use uv_normalize::{ExtraName, PackageName};
use uv_pep508::MarkerTree;
use uv_pypi_types::ConflictItem;
use uv_resolver_types::{ConflictMarker, UniversalMarker};

#[test]
fn conjunction_does_not_report_an_eliminated_conflict() {
    let package = PackageName::from_str("demo").expect("valid package name");
    let extra = ExtraName::from_str("feature").expect("valid extra name");
    let item = ConflictItem::from((package, extra));
    let linux = MarkerTree::from_str("sys_platform == 'linux'").expect("valid marker");
    let conflict = ConflictMarker::from_conflict_item(&item);

    let mut positive = UniversalMarker::new(MarkerTree::TRUE, conflict);
    positive.or(UniversalMarker::from_combined(linux));
    let mut negative = UniversalMarker::new(MarkerTree::TRUE, conflict.negate());
    negative.or(UniversalMarker::from_combined(linux));
    assert!(positive.has_conflict_marker());
    assert!(negative.has_conflict_marker());

    // (linux OR feature) AND (linux OR NOT feature) depends only on linux.
    positive.and(negative);
    assert_eq!(positive.combined(), linux);
    assert!(!positive.has_conflict_marker());
}
