use std::collections::BTreeSet;

use uv_distribution_types::Requirement;
use uv_normalize::{ExtraName, GroupName};
use uv_pep508::MarkerTree;

/// PEP 751 selections and root requirements with selection conditions
#[derive(Debug, Clone, Default)]
pub struct RootSelections {
    pub extras: BTreeSet<ExtraName>,
    pub groups: BTreeSet<GroupName>,
    pub requirements: Vec<(Requirement, MarkerTree)>,
}

/// PEP 751 selections with dropped requirements (just to simplify)
#[derive(Debug)]
pub struct SelectionNames {
    pub extras: BTreeSet<ExtraName>,
    pub groups: BTreeSet<GroupName>,
}
