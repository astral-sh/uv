use std::borrow::Cow;
use std::path::{Path, PathBuf};

use url::Url;

use pep508_rs::VerbatimUrl;
use requirements_txt::EditableRequirement;

use crate::Verbatim;

#[derive(Debug, Clone)]
pub struct LocalEditable {
    pub requirement: EditableRequirement,
    /// Either the path to the editable or its checkout.
    pub path: PathBuf,
}

impl LocalEditable {
    /// Return the [`VerbatimUrl`] of the editable.
    pub fn url(&self) -> &VerbatimUrl {
        self.requirement.url()
    }

    /// Return the underlying [`Url`] of the editable.
    pub fn raw(&self) -> &Url {
        self.requirement.raw()
    }

    /// Return the resolved path to the editable.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Verbatim for LocalEditable {
    fn verbatim(&self) -> Cow<'_, str> {
        self.url().verbatim()
    }
}

impl std::fmt::Display for LocalEditable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.requirement.fmt(f)
    }
}
