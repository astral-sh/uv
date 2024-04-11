use std::borrow::Cow;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
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

/// A collection of [`LocalEditable`]s.
#[derive(Debug, Clone)]
pub struct LocalEditables(Vec<LocalEditable>);

impl LocalEditables {
    /// Merge and dedupe a list of [`LocalEditable`]s.
    ///
    /// This function will deduplicate any editables that point to identical paths, merging their
    /// extras.
    pub fn from_editables(editables: impl Iterator<Item = LocalEditable>) -> Self {
        let mut map = BTreeMap::new();
        for editable in editables {
            match map.entry(editable.path.clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(editable);
                }
                Entry::Occupied(mut entry) => {
                    let existing = entry.get_mut();
                    existing.extras.extend(editable.extras);
                }
            }
        }
        Self(map.into_values().collect())
    }

    /// Return the number of editables.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Return whether the editables are empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Return the editables as a vector.
    pub fn into_vec(self) -> Vec<LocalEditable> {
        self.0
    }
}

impl IntoIterator for LocalEditables {
    type Item = LocalEditable;
    type IntoIter = std::vec::IntoIter<LocalEditable>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
