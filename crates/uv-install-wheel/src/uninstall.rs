use std::collections::{BTreeSet, HashSet};
use std::fmt::Display;
use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};

use tracing::trace;

use uv_fs::write_atomic_sync;
use uv_warnings::warn_user;

use crate::wheel::read_record;
use crate::{Error, Layout};

/// Uninstall the wheel represented by the given `.dist-info` directory.
pub fn uninstall_wheel(
    dist_info: &Path,
    distribution: impl Display,
    layout: &Layout,
) -> Result<Uninstall, Error> {
    let Some(site_packages) = dist_info.parent() else {
        return Err(Error::BrokenVenv(
            "dist-info directory is not in a site-packages directory".to_string(),
        ));
    };

    // Read the RECORD file.
    let record = {
        let record_path = dist_info.join("RECORD");
        let mut record_file = match fs_err::File::open(&record_path) {
            Ok(record_file) => record_file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::MissingRecord(record_path));
            }
            Err(err) => return Err(err.into()),
        };
        read_record(&mut record_file)?
    };

    // Build the set of paths that are also claimed by some other installed package's
    // `RECORD`. Some package pairs (e.g. `opencv-python` and `opencv-contrib-python`,
    // which both ship a top-level `cv2/` package) overlap on disk. When the user
    // removes one of them, we must NOT delete files that the remaining package
    // also claims, otherwise `import cv2` (or the equivalent) breaks until the
    // user re-installs from scratch. See issue #19412.
    let shared_paths = collect_shared_paths(site_packages, dist_info);

    let mut file_count = 0usize;
    let mut dir_count = 0usize;

    #[cfg(windows)]
    let itself = std::env::current_exe().ok();

    // Uninstall the files, keeping track of any directories that are left empty.
    let mut visited = BTreeSet::new();
    for entry in &record {
        let path = site_packages.join(&entry.path);

        if !is_path_in_scheme(&entry.path, site_packages, &distribution, layout) {
            continue;
        }

        // Skip files that another installed package's `RECORD` also claims; the
        // remaining package still needs them. See issue #19412.
        if shared_paths.contains(&path) {
            trace!(
                "Skipping shared file (also owned by another installed package): {}",
                path.display()
            );
            continue;
        }

        // On Windows, deleting the current executable is a special case.
        #[cfg(windows)]
        if let Some(itself) = itself.as_ref() {
            if itself
                .file_name()
                .is_some_and(|itself| path.file_name().is_some_and(|path| itself == path))
            {
                if same_file::is_same_file(itself, &path).unwrap_or(false) {
                    tracing::debug!("Detected self-delete of executable: {}", path.display());
                    match self_replace::self_delete_outside_path(site_packages) {
                        Ok(()) => {
                            trace!("Removed file: {}", path.display());
                            file_count += 1;
                            if let Some(parent) = path.parent() {
                                visited.insert(normalize_path(parent));
                            }
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                        Err(err) => return Err(err.into()),
                    }
                    continue;
                }
            }
        }

        match fs_err::remove_file(&path) {
            Ok(()) => {
                trace!("Removed file: {}", path.display());
                file_count += 1;
                if let Some(parent) = path.parent() {
                    visited.insert(normalize_path(parent));
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => match fs_err::remove_dir_all(&path) {
                Ok(()) => {
                    trace!("Removed directory: {}", path.display());
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
            match fs_err::remove_dir_all(&pycache) {
                Ok(()) => {
                    trace!("Removed directory: {}", pycache.display());
                    dir_count += 1;
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }

            // Try to read from the directory. If it doesn't exist, assume we deleted it in a
            // previous iteration.
            let mut read_dir = match fs_err::read_dir(path) {
                Ok(read_dir) => read_dir,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
                Err(err) => return Err(err.into()),
            };

            // If the directory is not empty, we're done.
            if read_dir.next().is_some() {
                break;
            }

            fs_err::remove_dir(path)?;

            trace!("Removed directory: {}", path.display());
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

static WARNED_FOR_RECORD_ENTRY_PACKAGE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static WARNED_FOR_EGG_TOP_LEVEL_PACKAGE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Check if the path is inside the venv or a system interpreter path, and warn if it isn't.
///
/// Returns `false` is a path is outside the paths that files from a wheel can be installed into,
/// so that the caller can reject RECORD entries that escape site-packages via path traversal (e.g.,
/// `../../../etc/passwd`). A malicious wheel could otherwise include such entries to cause deletion
/// of arbitrary files on uninstall.
fn is_path_in_scheme(
    path: &str,
    site_packages: &Path,
    distribution: impl Display,
    layout: &Layout,
) -> bool {
    let normalized = normalize_path(&site_packages.join(path));

    // `purelib` or `platlib` are site-packages (depending on `Root-Is-Purelib`). As
    // `.data/*` goes into the directories of `scheme`, `.dist-info` goes into site-packages
    // and all other content goes into site-packages, the condition below covers all valid
    // directories, in venvs, system interpreters and custom installation schemes.
    //
    // For a venv, `data` is the venv root: A wheel can write into the entire venv through
    // `.data/data`. For a system environment, wheels are allowed to write to
    // whole system directories, for example `data` is `/usr/local` for system Python on
    // Ubuntu 24.04.
    if normalized.starts_with(&layout.scheme.data)
        || normalized.starts_with(&layout.scheme.purelib)
        || normalized.starts_with(&layout.scheme.platlib)
        || normalized.starts_with(&layout.scheme.scripts)
        || normalized.starts_with(&layout.scheme.include)
    {
        true
    } else {
        // A package that does this is malformed to the point of being a risk to the user, be
        // annoying about it, but only once per package.
        if WARNED_FOR_RECORD_ENTRY_PACKAGE
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .expect("The mutex is broken, did some other thread panic?")
            .insert(distribution.to_string())
        {
            warn_user!(
                "Invalid RECORD entry in {} that escapes the Python environment, skipping: {}",
                distribution,
                path
            );
        }
        false
    }
}

/// Check that a `top_level.txt` entry names a single top-level module or package.
///
/// Unlike wheel `RECORD` entries, egg `top_level.txt` entries refer to direct children of the
/// egg's base location, not arbitrary paths. Treating them as paths can make uninstall delete
/// directories outside `site-packages`.
fn is_valid_top_level_entry(entry: &str, distribution: impl Display) -> bool {
    if is_safe_top_level_entry(entry) {
        true
    } else {
        if WARNED_FOR_EGG_TOP_LEVEL_PACKAGE
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .expect("The mutex is broken, did some other thread panic?")
            .insert(distribution.to_string())
        {
            warn_user!(
                "Invalid `top_level.txt` entry in {} that is not a top-level module or package, skipping: {}",
                distribution,
                entry
            );
        }
        false
    }
}

fn is_safe_top_level_entry(entry: &str) -> bool {
    !entry.is_empty() && entry != "." && entry != ".." && !entry.contains(['/', '\\'])
}

/// Collect the set of paths under `site_packages` that are listed in some OTHER
/// installed package's `RECORD`, excluding the one we are about to uninstall
/// (`dist_info_being_removed`).
///
/// This walks every `*.dist-info` directory in `site_packages` other than the
/// one being removed and reads each `RECORD`. Any `RECORD` we cannot read (e.g.
/// missing, malformed, or a path we cannot resolve) is silently skipped, since
/// the existing uninstall logic already tolerates missing-on-disk entries via
/// `ErrorKind::NotFound` and the worst case from skipping a malformed `RECORD`
/// here is reverting to the pre-fix behavior for those specific files. This is
/// strictly an additive safety check.
///
/// Returns an empty set if `site_packages` cannot be read at all.
fn collect_shared_paths(site_packages: &Path, dist_info_being_removed: &Path) -> HashSet<PathBuf> {
    let mut shared = HashSet::new();
    let Ok(entries) = fs_err::read_dir(site_packages) else {
        return shared;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path == dist_info_being_removed {
            continue;
        }
        // Only consider sibling `.dist-info` directories.
        if !entry_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".dist-info"))
        {
            continue;
        }
        let other_record_path = entry_path.join("RECORD");
        let Ok(mut other_record_file) = fs_err::File::open(&other_record_path) else {
            continue;
        };
        let Ok(other_record) = read_record(&mut other_record_file) else {
            continue;
        };
        for other_entry in other_record {
            shared.insert(site_packages.join(&other_entry.path));
        }
    }
    shared
}

/// Uninstall the egg represented by the `.egg-info` directory.
///
/// See: <https://github.com/pypa/pip/blob/41587f5e0017bcd849f42b314dc8a34a7db75621/src/pip/_internal/req/req_uninstall.py#L483>
pub fn uninstall_egg(egg_info: &Path, distribution: impl Display) -> Result<Uninstall, Error> {
    let mut file_count = 0usize;
    let mut dir_count = 0usize;

    let dist_location = egg_info
        .parent()
        .expect("egg-info directory is not in a site-packages directory");

    // Read the `namespace_packages.txt` file, skipping empty or whitespace-only entries.
    let namespace_packages = {
        let namespace_packages_path = egg_info.join("namespace_packages.txt");
        match fs_err::read_to_string(namespace_packages_path) {
            Ok(namespace_packages) => namespace_packages
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                vec![]
            }
            Err(err) => return Err(err.into()),
        }
    };

    // Read the `top_level.txt` file, ignoring anything in `namespace_packages.txt`.
    //
    // Empty or whitespace-only entries are skipped: legacy setuptools writes `top_level.txt`
    // with a trailing newline even when the package has no top-level modules, which
    // `str::lines` yields as an empty string. Joining that onto `dist_location` would
    // resolve back to `dist_location` itself (site-packages), and a subsequent
    // `remove_dir_all` would wipe out every installed package.
    let top_level = {
        let top_level_path = egg_info.join("top_level.txt");
        match fs_err::read_to_string(&top_level_path) {
            Ok(top_level) => top_level
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .filter(|line| !namespace_packages.iter().any(|ns| ns.as_str() == *line))
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::MissingTopLevel(top_level_path));
            }
            Err(err) => return Err(err.into()),
        }
    };

    // Remove everything in `top_level.txt`.
    for entry in top_level {
        if !is_valid_top_level_entry(&entry, &distribution) {
            continue;
        }

        let path = dist_location.join(&entry);

        // Remove as a directory.
        match fs_err::remove_dir_all(&path) {
            Ok(()) => {
                trace!("Removed directory: {}", path.display());
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
                    trace!("Removed file: {}", path.display());
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
            trace!("Removed directory: {}", egg_info.display());
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
    let contents = fs_err::read_to_string(egg_link)?;
    let target_line = contents
        .lines()
        .find_map(|line| {
            let line = line.trim();
            if line.is_empty() { None } else { Some(line) }
        })
        .ok_or_else(|| Error::InvalidEggLink(egg_link.to_path_buf()))?;

    // This comes from `pkg_resources.normalize_path`
    let target_line = normcase(target_line);

    match fs_err::remove_file(egg_link) {
        Ok(()) => {
            trace!("Removed file: {}", egg_link.display());
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

    let content = fs_err::read_to_string(&easy_install)?;
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
        trace!("Removed line from `easy-install.pth`: {target_line}");
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use assert_fs::prelude::*;

    use uv_pypi_types::Scheme;

    use crate::Layout;
    use crate::uninstall::{is_safe_top_level_entry, uninstall_egg, uninstall_wheel};

    #[test]
    fn test_top_level_entry_safe_name() {
        assert!(is_safe_top_level_entry("package"));

        assert!(!is_safe_top_level_entry(""));
        assert!(!is_safe_top_level_entry("."));
        assert!(!is_safe_top_level_entry(".."));
        assert!(!is_safe_top_level_entry("../package"));
        assert!(!is_safe_top_level_entry("package/name"));
        assert!(!is_safe_top_level_entry(r"package\name"));
    }

    /// Uninstall must not remove files outside the install scheme.
    #[test]
    fn test_uninstall_record_path_traversal() {
        let venv = assert_fs::TempDir::new().unwrap();
        let site_packages = venv.child("lib/python3.12/site-packages");
        let outside_dir = assert_fs::TempDir::new().unwrap();

        // Create a file outside site-packages that a malicious RECORD might target.
        let target_file = outside_dir.child("traversal_target.txt");
        target_file.write_str("I should not be deleted").unwrap();

        // Build a relative traversal path from site-packages to the target file.
        let dist_info = site_packages.child("evilpkg-0.1.0.dist-info");
        dist_info.create_dir_all().unwrap();
        let target_path = pathdiff::diff_paths(target_file.path(), site_packages.path()).unwrap();
        assert!(site_packages.join(&target_path).exists());

        // Add the invalid path to the RECORD.
        let record_content = format!(
            "evilpkg/__init__.py,,0\n\
             evilpkg-0.1.0.dist-info/METADATA,,0\n\
             evilpkg-0.1.0.dist-info/RECORD,,\n\
             {},,0\n",
            target_path.display()
        );
        dist_info
            .child("RECORD")
            .write_str(&record_content)
            .unwrap();

        // Also create the legitimate files so uninstall can remove them.
        let init_py = site_packages.child("evilpkg/__init__.py");
        init_py.touch().unwrap();
        let metadata = dist_info.child("METADATA");
        metadata.touch().unwrap();

        // Something that looks sufficiently like a Unix environment.
        let layout = Layout {
            sys_executable: venv.path().join("bin/python"),
            python_version: (3, 13),
            os_name: "posix".to_string(),
            scheme: Scheme {
                purelib: site_packages.to_path_buf(),
                platlib: site_packages.to_path_buf(),
                scripts: venv.path().join("bin"),
                data: venv.path().to_path_buf(),
                include: venv.path().join("include/python3.12"),
            },
        };

        uninstall_wheel(dist_info.path(), "evilpkg 0.1.0", &layout).unwrap();

        // The regular package files have been removed, while the file outside the scheme still
        // exists.
        assert!(target_file.exists());
        assert!(!metadata.exists());
        assert!(!init_py.exists());
    }

    #[test]
    fn test_uninstall_egg_info_path_traversal() {
        let venv = assert_fs::TempDir::new().unwrap();
        let site_packages = venv.child("lib/python3.12/site-packages");

        // Create directories outside site-packages, but inside the environment. Egg uninstall should
        // still reject them, even though wheel RECORD entries may target other install-scheme
        // directories.
        let target_dir = venv.child("traversal_target");
        let target_file = target_dir.child("secret.txt");
        target_file.write_str("I should not be deleted").unwrap();
        // Build a relative traversal path from site-packages to the target directory.
        let egg_info = site_packages.child("evilpkg-0.1.0.egg-info");
        egg_info.create_dir_all().unwrap();
        let target_path = pathdiff::diff_paths(target_dir.path(), site_packages.path()).unwrap();
        assert!(site_packages.join(&target_path).exists());

        // Create a fake egg-info directory with a path traversal entry in `top_level.txt`.
        egg_info
            .child("top_level.txt")
            .write_str(&format!("evilpkg\n{}\n", target_path.display()))
            .unwrap();

        // Also create the legitimate package directory so uninstall can remove it.
        let init_py = site_packages.child("evilpkg").child("__init__.py");
        init_py.touch().unwrap();

        uninstall_egg(egg_info.path(), "evilpkg 0.1.0").unwrap();

        // The regular package directory has been removed, while the directory outside
        // site-packages still exists.
        assert!(target_dir.exists());
        assert!(target_file.exists());
        assert!(!init_py.exists());
        assert!(!egg_info.exists());
    }

    /// Regression test for <https://github.com/astral-sh/uv/issues/19113>.
    ///
    /// Legacy setuptools writes a `top_level.txt` that contains just a newline when the
    /// distribution has no top-level modules. Previously, [`uninstall_egg`] parsed that as a
    /// single empty entry, joined it onto `site-packages`, and called `remove_dir_all` on the
    /// result, wiping out every other package in the environment. Uninstalling such a package
    /// must leave its siblings untouched.
    #[test]
    fn test_uninstall_egg_info_empty_top_level() {
        let venv = assert_fs::TempDir::new().unwrap();
        let site_packages = venv.child("lib/python3.12/site-packages");
        site_packages.create_dir_all().unwrap();

        // A sibling package that must survive the uninstall.
        let sibling_init = site_packages.child("sibling").child("__init__.py");
        sibling_init.touch().unwrap();
        let sibling_dist_info = site_packages.child("sibling-1.0.0.dist-info");
        sibling_dist_info.create_dir_all().unwrap();

        // The egg-info for the package we're uninstalling, with a `top_level.txt` that
        // contains only a newline (as legacy setuptools writes for an empty package).
        let egg_info = site_packages.child("emptypkg-0.1.0.egg-info");
        egg_info.create_dir_all().unwrap();
        egg_info.child("top_level.txt").write_str("\n").unwrap();

        uninstall_egg(egg_info.path(), "emptypkg 0.1.0").unwrap();

        // The egg-info is gone, but the rest of site-packages (including the sibling
        // package) survives.
        assert!(!egg_info.exists());
        assert!(
            site_packages.exists(),
            "uninstall must not remove site-packages itself"
        );
        assert!(sibling_init.exists(), "sibling package must not be removed");
        assert!(
            sibling_dist_info.exists(),
            "sibling dist-info must not be removed"
        );
    }

    /// Same bug shape as #19113, but triggered by a blank or whitespace-only line embedded
    /// between valid entries in `top_level.txt`. Exercises the filter in combination with
    /// real entries to make sure they're still honored after skipping empties.
    #[test]
    fn test_uninstall_egg_info_blank_lines_in_top_level() {
        let venv = assert_fs::TempDir::new().unwrap();
        let site_packages = venv.child("lib/python3.12/site-packages");
        site_packages.create_dir_all().unwrap();

        // A sibling package that must survive.
        let sibling_init = site_packages.child("sibling").child("__init__.py");
        sibling_init.touch().unwrap();

        // Two real top-level modules that should be removed.
        let pkg_a_init = site_packages.child("pkg_a").child("__init__.py");
        pkg_a_init.touch().unwrap();
        let pkg_b_init = site_packages.child("pkg_b").child("__init__.py");
        pkg_b_init.touch().unwrap();

        // `top_level.txt` with a leading blank line, a whitespace-only line between the two
        // valid entries, a trailing blank line, and `\r\n` line endings mixed in.
        let egg_info = site_packages.child("mixedpkg-0.1.0.egg-info");
        egg_info.create_dir_all().unwrap();
        egg_info
            .child("top_level.txt")
            .write_str("\npkg_a\n   \r\npkg_b\n\n")
            .unwrap();

        uninstall_egg(egg_info.path(), "mixedpkg 0.1.0").unwrap();

        // The two named packages are gone, the egg-info is gone, and site-packages plus
        // the sibling survive.
        assert!(!egg_info.exists());
        assert!(!pkg_a_init.exists(), "pkg_a must be removed");
        assert!(!pkg_b_init.exists(), "pkg_b must be removed");
        assert!(
            site_packages.exists(),
            "uninstall must not remove site-packages itself"
        );
        assert!(sibling_init.exists(), "sibling package must not be removed");
    }

    /// Build a layout whose install scheme points at `site_packages`.
    fn layout_for(venv: &Path, site_packages: &Path) -> Layout {
        Layout {
            sys_executable: venv.join("bin/python"),
            python_version: (3, 12),
            os_name: "posix".to_string(),
            scheme: Scheme {
                purelib: site_packages.to_path_buf(),
                platlib: site_packages.to_path_buf(),
                scripts: venv.join("bin"),
                data: venv.to_path_buf(),
                include: venv.join("include/python3.12"),
            },
        }
    }

    /// Two installed wheels sharing a file (the `opencv-python` / `opencv-contrib-python`
    /// pattern reported in #19412): uninstalling one must not delete the shared file.
    #[test]
    fn shared_files_are_preserved_when_one_overlapping_wheel_is_uninstalled() {
        let venv = assert_fs::TempDir::new().unwrap();
        let site_packages = venv.child("lib/python3.12/site-packages");
        site_packages.create_dir_all().unwrap();

        let shared = site_packages.child("cv2/__init__.py");
        shared.touch().unwrap();
        let pkg_a_only = site_packages.child("pkg_a_only.py");
        pkg_a_only.touch().unwrap();
        let pkg_b_only = site_packages.child("pkg_b_only.py");
        pkg_b_only.touch().unwrap();

        let dist_a = site_packages.child("pkg_a-1.0.dist-info");
        dist_a.create_dir_all().unwrap();
        dist_a
            .child("RECORD")
            .write_str(
                "cv2/__init__.py,,\n\
                 pkg_a_only.py,,\n\
                 pkg_a-1.0.dist-info/RECORD,,\n",
            )
            .unwrap();

        let dist_b = site_packages.child("pkg_b-1.0.dist-info");
        dist_b.create_dir_all().unwrap();
        dist_b
            .child("RECORD")
            .write_str(
                "cv2/__init__.py,,\n\
                 pkg_b_only.py,,\n\
                 pkg_b-1.0.dist-info/RECORD,,\n",
            )
            .unwrap();

        uninstall_wheel(
            dist_a.path(),
            "pkg_a 1.0",
            &layout_for(venv.path(), site_packages.path()),
        )
        .unwrap();

        assert!(!pkg_a_only.exists());
        assert!(pkg_b_only.exists());
        assert!(
            shared.exists(),
            "shared cv2/__init__.py was deleted when uninstalling pkg_a, but pkg_b still needs it"
        );
    }

    /// A wheel with no overlapping dist-info should still have its files removed normally.
    #[test]
    fn unique_files_are_removed_when_no_overlap_exists() {
        let venv = assert_fs::TempDir::new().unwrap();
        let site_packages = venv.child("lib/python3.12/site-packages");
        site_packages.create_dir_all().unwrap();

        let solo = site_packages.child("solo/__init__.py");
        solo.touch().unwrap();

        let dist = site_packages.child("solo-1.0.dist-info");
        dist.create_dir_all().unwrap();
        dist.child("RECORD")
            .write_str(
                "solo/__init__.py,,\n\
                 solo-1.0.dist-info/RECORD,,\n",
            )
            .unwrap();

        uninstall_wheel(
            dist.path(),
            "solo 1.0",
            &layout_for(venv.path(), site_packages.path()),
        )
        .unwrap();

        assert!(!solo.exists());
    }
}
