use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

use pep508_rs::VerbatimUrl;
use requirements_txt::EditableRequirement;

#[derive(Debug, Clone)]
pub struct LocalEditable {
    pub requirement: EditableRequirement,
    /// Either the path to the editable or its checkout
    pub path: PathBuf,
}

impl LocalEditable {
    pub fn url(&self) -> VerbatimUrl {
        self.requirement.url()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Display for LocalEditable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.requirement.fmt(f)
    }
}
