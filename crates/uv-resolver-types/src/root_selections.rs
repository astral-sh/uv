use std::collections::BTreeSet;

use uv_distribution_types::Requirement;
use uv_normalize::{ExtraName, GroupName};
use uv_pep508::MarkerTree;

/// PEP 751 selections and requirement-to-original-marker mappings.
#[derive(Debug, Clone, Default)]
pub struct RootSelections {
    pub extras: BTreeSet<ExtraName>,
    pub groups: BTreeSet<GroupName>,
    pub requirements: Vec<(Requirement, MarkerTree)>,
}

/// PEP 751 selections, Same set as RootSelections but with requirement-to-original-marker pairs
/// dropped, not strictly necessary
#[derive(Debug)]
pub struct SelectionNames {
    pub extras: BTreeSet<ExtraName>,
    pub groups: BTreeSet<GroupName>,
}
