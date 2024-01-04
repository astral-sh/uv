use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use fs_err as fs;
use tracing::debug;

use crate::{read_record_file, Error};

/// Uninstall the wheel represented by the given `dist_info` directory.
pub fn uninstall_wheel(dist_info: &Path) -> Result<Uninstall, Error> {
    let Some(site_packages) = dist_info.parent() else {
        return Err(Error::BrokenVenv(
            "dist-info directory is not in a site-packages directory".to_string(),
        ));
    };

    // Read the RECORD file.
    let record_path = dist_info.join("RECORD");
    let mut record_file = match fs::File::open(&record_path) {
        Ok(record_file) => record_file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::MissingRecord(record_path));
        }
        Err(err) => return Err(err.into()),
    };
    let record = read_record_file(&mut record_file)?;

    let mut file_count = 0usize;
    let mut dir_count = 0usize;

    // Uninstall the files, keeping track of any directories that are left empty.
    let mut visited = BTreeSet::new();
    for entry in &record {
        let path = site_packages.join(&entry.path);
        match fs::remove_file(&path) {
            Ok(()) => {
                debug!("Removed file: {}", path.display());
                file_count += 1;
                if let Some(parent) = path.parent() {
                    visited.insert(normalize_path(parent));
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => match fs::remove_dir_all(&path) {
                Ok(()) => {
                    debug!("Removed directory: {}", path.display());
                    dir_count += 1;
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(err.into()),
            },
        }
    }

    // If any directories were left empty, remove them. Iterate in reverse order such that we visit
    // the deepest directories first.
    for path in visited.iter().rev() {
        // No need to look at directories outside of `site-packages` (like `bin`).
        if !path.starts_with(site_packages) {
            continue;
        }

        // Iterate up the directory tree, removing any empty directories. It's insufficient to
        // rely on `visited` alone here, because we may end up removing a directory whose parent
        // directory doesn't contain any files, leaving the _parent_ directory empty.
        let mut path = path.as_path();
        loop {
            // If we reach the site-packages directory, we're done.
            if path == site_packages {
                break;
            }

            // If the directory contains a `__pycache__` directory, always remove it. `__pycache__`
            // may or may not be listed in the RECORD, but installers are expected to be smart
            // enough to remove it either way.
            let pycache = path.join("__pycache__");
            match fs::remove_dir_all(&pycache) {
                Ok(()) => {
                    debug!("Removed directory: {}", pycache.display());
                    dir_count += 1;
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }

            // Try to read from the directory. If it doesn't exist, assume we deleted it in a
            // previous iteration.
            let mut read_dir = match fs::read_dir(path) {
                Ok(read_dir) => read_dir,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
                Err(err) => return Err(err.into()),
            };

            // If the directory is not empty, we're done.
            if read_dir.next().is_some() {
                break;
            }

            fs::remove_dir(path)?;

            debug!("Removed directory: {}", path.display());
            dir_count += 1;

            if let Some(parent) = path.parent() {
                path = parent;
            } else {
                break;
            }
        }
    }

    Ok(Uninstall {
        file_count,
        dir_count,
    })
}

#[derive(Debug, Default)]
pub struct Uninstall {
    /// The number of files that were removed during the uninstallation.
    pub file_count: usize,
    /// The number of directories that were removed during the uninstallation.
    pub dir_count: usize,
}

/// Normalize a path, removing things like `.` and `..`.
///
/// Source: <https://github.com/rust-lang/cargo/blob/b48c41aedbd69ee3990d62a0e2006edbb506a480/crates/cargo-util/src/paths.rs#L76C1-L109C2>
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = path.components().peekable();
    let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek().copied() {
        components.next();
        PathBuf::from(c.as_os_str())
    } else {
        PathBuf::new()
    };

    for component in components {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {
                ret.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                ret.pop();
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
}
