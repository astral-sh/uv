use std::collections::VecDeque;
use std::convert::Infallible;
use std::io;
use std::ops::ControlFlow;
use std::path::{Component, Path, PathBuf};

use crate::Layout;

/// Resolve installation-scheme aliases without following package directory links into the cache.
pub(crate) struct LibraryDirectories {
    roots: Vec<(PathBuf, PathBuf)>,
}

impl LibraryDirectories {
    /// Remember both spellings of the library roots, including roots that do not exist yet.
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

    /// Visit directory components inside the libraries before following their symlinks.
    ///
    /// The visitor must either leave a real directory or stop traversal. Installers expand links
    /// under their directory lock; uninstallers stop at the first link and remove the link itself.
    /// Aliases outside the libraries are followed component by component, since an alias can point
    /// inside a package whose directory is still linked to the cache.
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
