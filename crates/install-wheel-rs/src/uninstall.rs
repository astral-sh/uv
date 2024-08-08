use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use fs_err as fs;
use std::sync::{LazyLock, Mutex};
use tracing::debug;
use uv_fs::write_atomic_sync;

use crate::wheel::read_record_file;
use crate::Error;

/// Uninstall the wheel represented by the given `.dist-info` directory.
pub fn uninstall_wheel(dist_info: &Path) -> Result<Uninstall, Error> {
    let Some(site_packages) = dist_info.parent() else {
        return Err(Error::BrokenVenv(
            "dist-info directory is not in a site-packages directory".to_string(),
        ));
    };

    // Read the RECORD file.
    let record = {
        let record_path = dist_info.join("RECORD");
        let mut record_file = match fs::File::open(&record_path) {
            Ok(record_file) => record_file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::MissingRecord(record_path));
            }
            Err(err) => return Err(err.into()),
        };
        read_record_file(&mut record_file)?
    };

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

/// Uninstall the egg represented by the `.egg-info` directory.
///
/// See: <https://github.com/pypa/pip/blob/41587f5e0017bcd849f42b314dc8a34a7db75621/src/pip/_internal/req/req_uninstall.py#L483>
pub fn uninstall_egg(egg_info: &Path) -> Result<Uninstall, Error> {
    let mut file_count = 0usize;
    let mut dir_count = 0usize;

    let dist_location = egg_info
        .parent()
        .expect("egg-info directory is not in a site-packages directory");

    // Read the `namespace_packages.txt` file.
    let namespace_packages = {
        let namespace_packages_path = egg_info.join("namespace_packages.txt");
        match fs_err::read_to_string(namespace_packages_path) {
            Ok(namespace_packages) => namespace_packages
                .lines()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                vec![]
            }
            Err(err) => return Err(err.into()),
        }
    };

    // Read the `top_level.txt` file, ignoring anything in `namespace_packages.txt`.
    let top_level = {
        let top_level_path = egg_info.join("top_level.txt");
        match fs_err::read_to_string(&top_level_path) {
            Ok(top_level) => top_level
                .lines()
                .map(ToString::to_string)
                .filter(|line| !namespace_packages.contains(line))
                .collect::<Vec<_>>(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::MissingTopLevel(top_level_path));
            }
            Err(err) => return Err(err.into()),
        }
    };

    // Remove everything in `top_level.txt`.
    for entry in top_level {
        let path = dist_location.join(&entry);

        // Remove as a directory.
        match fs_err::remove_dir_all(&path) {
            Ok(()) => {
                debug!("Removed directory: {}", path.display());
                dir_count += 1;
                continue;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }

        // Remove as a `.py`, `.pyc`, or `.pyo` file.
        for extension in &["py", "pyc", "pyo"] {
            let path = path.with_extension(extension);
            match fs_err::remove_file(&path) {
                Ok(()) => {
                    debug!("Removed file: {}", path.display());
                    file_count += 1;
                    break;
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }
    }

    // Remove the `.egg-info` directory.
    match fs_err::remove_dir_all(egg_info) {
        Ok(()) => {
            debug!("Removed directory: {}", egg_info.display());
            dir_count += 1;
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err.into());
        }
    }

    Ok(Uninstall {
        file_count,
        dir_count,
    })
}

fn normcase(s: &str) -> String {
    if cfg!(windows) {
        s.replace('/', "\\").to_lowercase()
    } else {
        s.to_owned()
    }
}

static EASY_INSTALL_PTH: LazyLock<Mutex<i32>> = LazyLock::new(Mutex::default);

/// Uninstall the legacy editable represented by the `.egg-link` file.
///
/// See: <https://github.com/pypa/pip/blob/41587f5e0017bcd849f42b314dc8a34a7db75621/src/pip/_internal/req/req_uninstall.py#L534-L552>
pub fn uninstall_legacy_editable(egg_link: &Path) -> Result<Uninstall, Error> {
    let mut file_count = 0usize;

    // Find the target line in the `.egg-link` file.
    let contents = fs::read_to_string(egg_link)?;
    let target_line = contents
        .lines()
        .find_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                None
            } else {
                Some(line)
            }
        })
        .ok_or_else(|| Error::InvalidEggLink(egg_link.to_path_buf()))?;

    // This comes from `pkg_resources.normalize_path`
    let target_line = normcase(target_line);

    match fs::remove_file(egg_link) {
        Ok(()) => {
            debug!("Removed file: {}", egg_link.display());
            file_count += 1;
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }

    let site_package = egg_link.parent().ok_or(Error::BrokenVenv(
        "`.egg-link` file is not in a directory".to_string(),
    ))?;
    let easy_install = site_package.join("easy-install.pth");

    // Since uv has an environment lock, it's enough to add a mutex here to ensure we never
    // lose writes to `easy-install.pth` (this is the only place in uv where `easy-install.pth`
    // is modified).
    let _guard = EASY_INSTALL_PTH.lock().unwrap();

    let content = fs::read_to_string(&easy_install)?;
    let mut new_content = String::with_capacity(content.len());
    let mut removed = false;

    // https://github.com/pypa/pip/blob/41587f5e0017bcd849f42b314dc8a34a7db75621/src/pip/_internal/req/req_uninstall.py#L634
    for line in content.lines() {
        if !removed && line.trim() == target_line {
            removed = true;
        } else {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }
    if removed {
        write_atomic_sync(&easy_install, new_content)?;
        debug!("Removed line from `easy-install.pth`: {target_line}");
    }

    Ok(Uninstall {
        file_count,
        dir_count: 0usize,
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
