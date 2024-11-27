use uv_normalize::PackageName;
use uv_pep508::Requirement;

use crate::VerbatimParsedUrl;

/// The `build-system.requires` field in a `pyproject.toml` file.
///
/// See: <https://peps.python.org/pep-0518/>
#[derive(Debug, Clone)]
pub struct BuildRequires {
    pub name: Option<PackageName>,
    pub requires_dist: Vec<Requirement<VerbatimParsedUrl>>,
}
