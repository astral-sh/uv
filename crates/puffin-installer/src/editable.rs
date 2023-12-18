use distribution_types::{CachedDist, LocalEditable};
use pypi_types::Metadata21;

#[derive(Debug, Clone)]
pub struct BuiltEditable {
    pub editable: LocalEditable,
    pub wheel: CachedDist,
    pub metadata: Metadata21,
}
