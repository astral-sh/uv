use std::collections::VecDeque;
use std::convert::Infallible;
use std::io;
use std::ops::ControlFlow;
use std::path::{Component, Path, PathBuf};

use uv_fs::link::{CopyLocks, materialize_symlink_dir};

use crate::linker::needs_mutable_copy;
use crate::{Error, Layout};

/// Resolve installation-scheme aliases without following package directory links into the cache.
///
/// Scheme aliases such as `lib64 -> lib` are followed. Links below a library root, such as
/// `site-packages/numpy -> <cache>/numpy`, are handled by the caller before traversal can enter
/// the cache. Scheme aliases must remain stable while this resolver is in use.
pub(crate) struct LibraryDirectories {
    /// Library roots as (logical path, resolved path) pairs.
    roots: Vec<(PathBuf, PathBuf)>,
}

impl LibraryDirectories {
    /// Cache both scheme and resolved root spellings so RECORD paths can skip repeated scheme
    /// traversal. Existing ancestors are resolved even when the library directory does not exist yet.
    pub(crate) fn new(layout: &Layout) -> io::Result<Self> {
        let directories = Self { roots: Vec::new() };
        let mut roots = Vec::with_capacity(2);
        for path in [&layout.scheme.purelib, &layout.scheme.platlib] {
            let path = std::path::absolute(path)?;
            let resolved = match directories.resolve(&path, |_| {
                Ok::<_, io::Error>(ControlFlow::<Infallible>::Continue(()))
            })? {
                ControlFlow::Continue(resolved) => resolved,
                ControlFlow::Break(never) => match never {},
            };
            roots.push((path, resolved));
        }
        Ok(Self { roots })
    }

    /// Whether a resolved path is strictly inside a library directory.
    pub(crate) fn contains(&self, path: &Path) -> bool {
        self.roots
            .iter()
            .any(|(_, root)| path != root && path.starts_with(root))
    }

    /// Create an installation directory, expanding package links before writing beneath them.
    ///
    /// Call this for a file's parent before publishing with the same [`CopyLocks`]. Top-level package
    /// directories are fully expanded under their lock before traversal reaches their children.
    pub(crate) fn prepare(&self, path: &Path, locks: &CopyLocks) -> Result<(), Error> {
        let resolved = self.resolve(path, |directory| {
            if !self
                .roots
                .iter()
                .any(|(_, root)| directory.parent() == Some(root.as_path()))
            {
                return Ok(ControlFlow::Continue(()));
            }
            locks.with_directory_lock(directory, || {
                materialize_symlink_dir(directory, needs_mutable_copy)?;
                fs_err::create_dir_all(directory)?;
                Ok::<_, Error>(ControlFlow::<Infallible>::Continue(()))
            })
        })?;
        match resolved {
            ControlFlow::Continue(directory) => fs_err::create_dir_all(directory)?,
            ControlFlow::Break(never) => match never {},
        }
        Ok(())
    }

    /// Resolve a directory path, visiting each component below library roots before following links.
    ///
    /// Visits run from parent to child. [`ControlFlow::Break`] stops before inspecting descendants.
    /// Callers handling files must pass the parent path, leaving the final filename unresolved.
    ///
    /// Scheme aliases are followed component by component: an alias may point below a package link,
    /// so canonicalizing the whole path could enter the cache before the visitor can handle it.
    pub(crate) fn resolve<B, E>(
        &self,
        path: &Path,
        mut visit: impl FnMut(&Path) -> Result<ControlFlow<B>, E>,
    ) -> Result<ControlFlow<B, PathBuf>, E>
    where
        E: From<io::Error>,
    {
        let path = std::path::absolute(path)?;
        // Most RECORD paths start with a known library root. Avoid inspecting the same scheme
        // directories for every file, while still visiting every package directory below them.
        let (mut resolved, relative) = self
            .roots
            .iter()
            .filter_map(|(logical, resolved)| {
                path.strip_prefix(logical)
                    .or_else(|_| path.strip_prefix(resolved))
                    .ok()
                    .map(|relative| (resolved.clone(), relative))
            })
            .min_by_key(|(_, relative)| relative.as_os_str().len())
            .unwrap_or_else(|| (PathBuf::new(), path.as_path()));
        let mut pending: VecDeque<_> = relative
            .components()
            .map(|component| component.as_os_str().to_owned())
            .collect();
        let mut followed_symlinks = 0;
        while let Some(component) = pending.pop_front() {
            match Path::new(&component).components().next() {
                Some(Component::ParentDir) => {
                    resolved.pop();
                }
                Some(Component::CurDir) | None => {}
                Some(Component::Prefix(_) | Component::RootDir) => resolved.push(component),
                Some(Component::Normal(_)) => {
                    resolved.push(component);
                    if self.contains(&resolved) {
                        if let ControlFlow::Break(value) = visit(&resolved)? {
                            return Ok(ControlFlow::Break(value));
                        }
                        continue;
                    }
                    match fs_err::symlink_metadata(&resolved) {
                        Ok(metadata) if metadata.file_type().is_symlink() => {
                            followed_symlinks += 1;
                            if followed_symlinks > 40 {
                                return Err(io::Error::other(format!(
                                    "Too many directory symlinks while resolving {}",
                                    path.display()
                                ))
                                .into());
                            }
                            let target = fs_err::read_link(&resolved)?;
                            resolved.pop();
                            for component in target.components().rev() {
                                pending.push_front(component.as_os_str().to_owned());
                            }
                        }
                        // This component is not a package link. Canonicalizing a real directory
                        // also recovers its spelling on case-insensitive filesystems before we
                        // compare descendants with the library roots.
                        Ok(metadata) if metadata.is_dir() => {
                            resolved = fs_err::canonicalize(&resolved)?;
                        }
                        Ok(_) => {}
                        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                        Err(err) => return Err(err.into()),
                    }
                }
            }
        }
        Ok(ControlFlow::Continue(resolved))
    }
}
