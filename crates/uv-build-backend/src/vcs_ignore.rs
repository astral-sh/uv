use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use rustc_hash::FxHashMap;
use uv_fs::normalize_path;

use crate::Error;

/// Git ignore rules scoped to a source tree, loaded only for directories being visited.
pub(crate) struct VcsIgnore {
    root: PathBuf,
    enabled: bool,
    directories: FxHashMap<PathBuf, Arc<DirectoryIgnore>>,
}

impl VcsIgnore {
    pub(crate) fn new(root: &Path, enabled: bool) -> Self {
        Self {
            root: normalize_path(root).into_owned(),
            enabled,
            directories: FxHashMap::default(),
        }
    }

    pub(crate) fn is_ignored(&mut self, path: &Path, is_dir: bool) -> Result<bool, Error> {
        if !self.enabled {
            return Ok(false);
        }
        let path = normalize_path(path);
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return Ok(false);
        };
        // A relative source root such as `.` normalizes to an empty path. Do not interpret an
        // absolute path or a leading `..` as a path inside that source tree.
        if relative.as_os_str().is_empty() || relative.has_root() || relative.starts_with("..") {
            return Ok(false);
        }
        let parent = path.parent().expect("path is inside source tree");
        let directory = self.directory(parent)?;
        Ok(directory.ignored || directory.is_ignored(&path, is_dir))
    }

    /// Required package files must remain available when rebuilding from the source distribution.
    pub(crate) fn require(&mut self, relative: &Path) -> Result<(), Error> {
        if self.enabled && self.is_ignored(&self.root.join(relative), false)? {
            return Err(Error::RequiredFileVcsIgnored(relative.to_path_buf()));
        }
        Ok(())
    }

    fn directory(&mut self, path: &Path) -> Result<Arc<DirectoryIgnore>, Error> {
        if let Some(directory) = self.directories.get(path) {
            return Ok(Arc::clone(directory));
        }

        let parent = if path == self.root {
            None
        } else {
            Some(self.directory(path.parent().expect("directory is inside source tree"))?)
        };
        // Git does not descend into ignored directories, so nested negations cannot re-include
        // their contents. Check this even when a wheel walk starts inside a module or data root.
        let ignored = parent
            .as_ref()
            .is_some_and(|parent| parent.ignored || parent.is_ignored(path, true));
        let rules = if ignored {
            None
        } else {
            Self::read_rules(path)?
        };
        let directory = Arc::new(DirectoryIgnore {
            ignored,
            rules,
            parent,
        });
        self.directories
            .insert(path.to_path_buf(), Arc::clone(&directory));
        Ok(directory)
    }

    fn read_rules(directory: &Path) -> Result<Option<Gitignore>, Error> {
        let path = directory.join(".gitignore");
        match fs_err::symlink_metadata(&path) {
            // Git does not follow symbolic links when reading .gitignore files.
            Ok(metadata) if metadata.file_type().is_symlink() => return Ok(None),
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(Error::VcsIgnore(path, ignore::Error::Io(err))),
        }
        let mut builder = GitignoreBuilder::new(directory);
        if let Some(err) = builder.add(&path) {
            return Err(Error::VcsIgnore(path, err));
        }
        let rules = builder.build().map_err(|err| Error::VcsIgnore(path, err))?;
        Ok(Some(rules))
    }
}

struct DirectoryIgnore {
    ignored: bool,
    rules: Option<Gitignore>,
    parent: Option<Arc<Self>>,
}

impl DirectoryIgnore {
    fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        let mut directory = Some(self);
        while let Some(current) = directory {
            if let Some(rules) = &current.rules {
                match rules.matched(path, is_dir) {
                    Match::Ignore(_) => return true,
                    Match::Whitelist(_) => return false,
                    Match::None => {}
                }
            }
            directory = current.parent.as_deref();
        }
        false
    }
}
