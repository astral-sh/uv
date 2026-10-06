use std::path::Path;

use crate::Error;
use ignore::{DirEntry, WalkBuilder};

/// Walk a selected subtree, inheriting ignore rules only from within the project.
pub(crate) fn walk_tree(
    source_tree: &Path,
    subtree: &Path,
    respect_gitignore: bool,
    filter: impl Fn(&Path) -> bool + Send + Sync + 'static,
) -> Result<impl Iterator<Item = Result<DirEntry, Error>>, Error> {
    // A project-root walk would otherwise silently skip a missing selected subtree.
    if respect_gitignore {
        fs_err::symlink_metadata(subtree)?;
    }
    // Starting at the project root lets the walker handle ignored ancestors and nested negations.
    let root = if respect_gitignore {
        source_tree
    } else {
        subtree
    };
    let mut builder = WalkBuilder::from_iter(filter(subtree).then_some(root));
    builder.standard_filters(false).sort_by_file_name(Ord::cmp);
    if respect_gitignore {
        // Use project-local rules without consulting Git configuration or repository boundaries.
        builder.add_custom_ignore_filename(".gitignore");
    }
    let selected = subtree.to_path_buf();
    builder.filter_entry(move |entry| {
        if entry.path().starts_with(&selected) {
            filter(entry.path())
        } else {
            selected.starts_with(entry.path())
        }
    });
    let subtree = subtree.to_path_buf();
    let root = root.to_path_buf();
    Ok(builder.build().filter_map(move |result| {
        let entry = match result {
            Ok(entry) => entry,
            Err(err) => {
                return Some(Err(Error::IgnoreWalk {
                    root: root.clone(),
                    err,
                }));
            }
        };
        if let Some(err) = entry.error() {
            return Some(Err(Error::VcsIgnore(err.clone())));
        }
        entry.path().starts_with(&subtree).then_some(Ok(entry))
    }))
}

/// Required package files must survive filtering so a source distribution remains buildable.
pub(crate) fn require_included(
    source_tree: &Path,
    relative: &Path,
    respect_gitignore: bool,
) -> Result<(), Error> {
    if !respect_gitignore {
        return Ok(());
    }
    let mut builder = WalkBuilder::new(source_tree);
    builder
        .standard_filters(false)
        .add_custom_ignore_filename(".gitignore");
    let mut matcher = builder.build_matchers().pop().expect("one configured root");
    let Some(path) = matcher.normalize(source_tree.join(relative)) else {
        return Ok(());
    };
    let (matched, error) = matcher.matched_with_errors(path, false);
    if let Some(error) = error {
        return Err(Error::VcsIgnore(error));
    }
    if matched.is_ignore() {
        return Err(Error::RequiredFileVcsIgnored(relative.to_path_buf()));
    }
    Ok(())
}
