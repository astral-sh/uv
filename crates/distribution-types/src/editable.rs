use std::borrow::Cow;
use std::path::PathBuf;

use url::Url;

use pep508_rs::VerbatimUrl;
use uv_normalize::ExtraName;

use crate::Verbatim;

#[derive(Debug, Clone)]
pub struct LocalEditable {
    /// The underlying [`EditableRequirement`] from the `requirements.txt` file.
    pub url: VerbatimUrl,
    /// Either the path to the editable or its checkout.
    pub path: PathBuf,
    /// The extras that should be installed.
    pub extras: Vec<ExtraName>,
}

impl LocalEditable {
    /// Return the editable as a [`Url`].
    pub fn url(&self) -> &VerbatimUrl {
        &self.url
    }

    /// Return the resolved path to the editable.
    pub fn raw(&self) -> &Url {
        self.url.raw()
    }
}

impl Verbatim for LocalEditable {
    fn verbatim(&self) -> Cow<'_, str> {
        self.url.verbatim()
    }
}

impl std::fmt::Display for LocalEditable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.url, f)
    }
}
