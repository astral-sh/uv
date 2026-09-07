use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cap_primitives::ambient_authority;
use cap_primitives::fs::{
    DirOptions, OpenOptions, create_dir, open, open_ambient_dir, open_dir_nofollow, remove_dir,
    remove_dir_all, remove_file, rename,
};
use fs_err::File;
#[cfg(windows)]
use fs_err::os::windows::fs::{symlink_dir, symlink_file};

use crate::linker::needs_mutable_copy;

/// An opened uninstall directory. Full paths are used for diagnostics and source links, never deletion.
pub(super) struct Directory {
    pub(super) handle: Arc<File>,
    parent: Option<Arc<File>>,
}

impl Directory {
    pub(super) fn path(&self) -> &Path {
        self.handle.path()
    }

    /// Open a resolved scheme path without following links introduced after scheme resolution.
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        let path = std::path::absolute(path)?;
        let root = path
            .ancestors()
            .last()
            .ok_or_else(|| io::Error::other("Directory has no filesystem root"))?;
        let mut file = Arc::new(File::from_parts(
            open_ambient_dir(root, ambient_authority())?,
            root,
        ));
        let mut parent = None;
        for component in path
            .strip_prefix(root)
            .map_err(io::Error::other)?
            .components()
        {
            let name = Path::new(component.as_os_str());
            let child = Arc::new(File::from_parts(
                open_dir_nofollow(file.file(), name)?,
                file.path().join(name),
            ));
            parent = Some(file);
            file = child;
        }
        Ok(Self {
            handle: file,
            parent,
        })
    }

    /// Open one child without following symlinks, retaining its identity if an ancestor is renamed.
    pub(super) fn open_dir(&self, name: &Path) -> io::Result<Self> {
        Ok(Self {
            handle: Arc::new(File::from_parts(
                open_dir_nofollow(self.handle.file(), name)?,
                self.path().join(name),
            )),
            parent: Some(Arc::clone(&self.handle)),
        })
    }

    /// Remove an empty directory through its retained parent, without searching for its inode.
    pub(super) fn remove(self) -> io::Result<()> {
        let (Some(parent), Some(name)) = (self.parent, self.handle.path().file_name()) else {
            return Err(io::Error::other("Cannot remove a filesystem root"));
        };
        // Windows requires closing the child handle before removing the entry.
        let name = name.to_os_string();
        drop(self.handle);
        remove_dir(parent.file(), Path::new(&name))
    }

    /// Expand a shared directory link through its opened parent, then reopen without following links.
    pub(super) fn materialize(&self, name: &Path) -> io::Result<Self> {
        let source = fs_err::canonicalize(self.path().join(name))?;
        let temporary = PathBuf::from(format!(".uv-{:032x}", fastrand::u128(..)));
        create_dir(self.handle.file(), &temporary, &DirOptions::new())?;
        let result = (|| {
            copy_links(&source, &self.open_dir(&temporary)?)?;
            self.remove_symlink(name)?;
            if let Err(err) = rename(self.handle.file(), &temporary, self.handle.file(), name) {
                let _ = symlink(&source, self, name);
                return Err(err);
            }
            self.open_dir(name)
        })();
        let _ = remove_dir_all(self.handle.file(), &temporary);
        result
    }

    pub(super) fn remove_symlink(&self, name: &Path) -> io::Result<()> {
        #[cfg(not(windows))]
        {
            remove_file(self.handle.file(), name)
        }
        #[cfg(windows)]
        {
            remove_file(self.handle.file(), name)
                .or_else(|err| remove_dir(self.handle.file(), name).or(Err(err)))
        }
    }
}

/// Populate a private directory with file links, copying files that may be mutated after installation.
fn copy_links(source: &Path, destination: &Directory) -> io::Result<()> {
    for entry in fs_err::read_dir(source)? {
        let entry = entry?;
        let source = entry.path();
        let name = PathBuf::from(entry.file_name());
        if entry.file_type()?.is_dir() {
            create_dir(destination.handle.file(), &name, &DirOptions::new())?;
            copy_links(&source, &destination.open_dir(&name)?)?;
        } else if needs_mutable_copy(&source) || symlink(&source, destination, &name).is_err() {
            let mut source = fs_err::File::open(&source)?;
            let mut target = open(
                destination.handle.file(),
                &name,
                OpenOptions::new().write(true).create_new(true),
            )?;
            io::copy(&mut source, &mut target)?;
            target.set_permissions(source.metadata()?.permissions())?;
        }
    }
    Ok(())
}

/// Create a link relative to an opened parent, allowing absolute cache paths as its contents.
fn symlink(source: &Path, parent: &Directory, name: &Path) -> io::Result<()> {
    #[cfg(not(windows))]
    {
        cap_primitives::fs::symlink_contents(source, parent.handle.file(), name)
    }
    #[cfg(windows)]
    {
        // Directory handles are opened without FILE_SHARE_DELETE on Windows.
        if source.is_dir() {
            symlink_dir(source, parent.path().join(name))
        } else {
            symlink_file(source, parent.path().join(name))
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::path::Path;

    use assert_fs::prelude::*;
    use cap_primitives::fs::{remove_dir_all, remove_file};
    use fs_err::os::unix::fs::symlink;

    use super::Directory;

    #[test]
    fn remove_from_replaced_directory() -> anyhow::Result<()> {
        let temporary = assert_fs::TempDir::new()?;
        let package = temporary.child("package");
        package.child("nested/module.py").touch()?;
        package.child("nested/__pycache__/module.pyc").touch()?;
        let store = temporary.child("store");
        store.child("nested/module.py").write_str("keep source")?;
        store
            .child("nested/__pycache__/module.pyc")
            .write_str("keep bytecode")?;

        let root = Directory::open(&fs_err::canonicalize(temporary.path())?)?;
        let directory = root
            .open_dir(Path::new("package"))?
            .open_dir(Path::new("nested"))?;
        let renamed = temporary.child("renamed");
        fs_err::rename(package.path(), renamed.path())?;
        symlink(store.path(), package.path())?;

        // A replacement before opening must be rejected; a replacement after opening must not
        // redirect either individual file removal or recursive bytecode cleanup into the store.
        assert!(root.open_dir(Path::new("package")).is_err());
        remove_file(directory.handle.file(), Path::new("module.py"))?;
        remove_dir_all(directory.handle.file(), Path::new("__pycache__"))?;
        assert!(!renamed.child("nested/module.py").exists());
        assert!(!renamed.child("nested/__pycache__").exists());
        directory.remove()?;
        assert!(!renamed.child("nested").exists());
        assert!(package.is_symlink());
        assert_eq!(
            fs_err::read_to_string(store.child("nested/module.py"))?,
            "keep source"
        );
        assert_eq!(
            fs_err::read_to_string(store.child("nested/__pycache__/module.pyc"))?,
            "keep bytecode"
        );
        Ok(())
    }
}
