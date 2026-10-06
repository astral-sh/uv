use std::borrow::Cow;
use std::path::{Component, Path};

use ignore::{IncrementalIgnore, WalkBuilder};

use crate::Error;

/// Project-local ignore rules, with hierarchy and caching managed by `ignore`.
pub(crate) struct VcsIgnore(Option<IncrementalIgnore>);

impl VcsIgnore {
    pub(crate) fn new(root: &Path, enabled: bool) -> Self {
        Self(enabled.then(|| {
            let mut builder = WalkBuilder::new(root);
            // Custom ignore files avoid consulting Git configuration, parents, or repository
            // boundaries while retaining `.gitignore` matching semantics.
            builder
                .standard_filters(false)
                .add_custom_ignore_filename(".gitignore");
            builder.build_matchers().pop().expect("one configured root")
        }))
    }

    pub(crate) fn is_ignored(&mut self, path: &Path, is_dir: bool) -> Result<bool, Error> {
        let Some(matcher) = &mut self.0 else {
            return Ok(false);
        };
        let relative = match path.strip_prefix(matcher.root()) {
            Ok(relative)
                if !relative
                    .components()
                    .any(|part| part == Component::ParentDir) =>
            {
                Cow::Borrowed(relative)
            }
            Ok(_) | Err(_) => {
                let Some(relative) = matcher.normalize(path) else {
                    return Ok(false);
                };
                Cow::Owned(relative)
            }
        };
        let (matched, error) = matcher.matched_with_errors(relative, is_dir);
        if let Some(error) = error {
            return Err(Error::VcsIgnore(error));
        }
        Ok(matched.is_ignore())
    }

    /// Required package files must remain available when rebuilding a source distribution.
    pub(crate) fn require(&mut self, relative: &Path) -> Result<(), Error> {
        let Some(matcher) = &self.0 else {
            return Ok(());
        };
        if self.is_ignored(&matcher.root().join(relative), false)? {
            return Err(Error::RequiredFileVcsIgnored(relative.to_path_buf()));
        }
        Ok(())
    }
}
