use requirements_txt::EditableRequirement;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone)]
pub struct LocalEditable {
    pub requirement: EditableRequirement,
    /// Either the path to the editable or its checkout
    pub path: PathBuf,
}

impl LocalEditable {
    pub fn url(&self) -> Url {
        Url::from_directory_path(&self.path).expect("A valid path makes a valid url")
    }
}

impl Display for LocalEditable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.requirement.fmt(f)
    }
}
