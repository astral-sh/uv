use std::borrow::Cow;
use std::path::{Component, Path, PathBuf, absolute};

use ignore::{IncrementalIgnore, WalkBuilder};

use crate::Error;

/// Git ignore rules, with hierarchy and caching managed by `ignore`.
pub(crate) struct VcsIgnore {
    root: PathBuf,
    matcher: Option<IncrementalIgnore>,
}

impl VcsIgnore {
    pub(crate) fn new(root: &Path, enabled: bool) -> Result<Self, Error> {
        let matcher = if enabled {
            let absolute_root = absolute(root)?;
            let ignore_root = absolute_root
                .ancestors()
                .find(|ancestor| ancestor.join(".git").exists())
                .unwrap_or(&absolute_root);
            let mut builder = WalkBuilder::new(ignore_root);
            // Root the matcher at the repository so ancestor rules retain their path semantics.
            // Custom ignore files avoid consulting Git configuration or rules above that root.
            builder
                .standard_filters(false)
                .add_custom_ignore_filename(".gitignore");
            Some(builder.build_matchers().pop().expect("one configured root"))
        } else {
            None
        };
        Ok(Self {
            root: root.to_path_buf(),
            matcher,
        })
    }

    pub(crate) fn is_ignored(&mut self, path: &Path, is_dir: bool) -> Result<bool, Error> {
        let Some(matcher) = &mut self.matcher else {
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
        if self.matcher.is_none() {
            return Ok(());
        }
        if self.is_ignored(&self.root.join(relative), false)? {
            return Err(Error::RequiredFileVcsIgnored(relative.to_path_buf()));
        }
        Ok(())
    }
}
