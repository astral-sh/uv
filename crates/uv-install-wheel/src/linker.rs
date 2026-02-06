use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use fs_err as fs;
use fs_err::DirEntry;
use itertools::Itertools;
use reflink_copy as reflink;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use tempfile::tempdir_in;
use tracing::{debug, instrument, trace, warn};
use walkdir::WalkDir;

use uv_distribution_filename::WheelFilename;
use uv_fs::Simplified;
use uv_preview::{Preview, PreviewFeature};
use uv_warnings::{warn_user, warn_user_once};

use crate::Error;

/// Avoid and track conflicts between packages.
#[expect(clippy::struct_field_names)]
#[derive(Debug, Default)]
pub struct Locks {
    /// The parent directory of a file in a synchronized copy
    copy_dir_locks: Mutex<FxHashMap<PathBuf, Arc<Mutex<()>>>>,
    /// Top level files and directories in site-packages, stored as relative path, and wheels they
    /// are from, with the absolute paths in the unpacked wheel.
    site_packages_paths: Mutex<FxHashMap<PathBuf, BTreeSet<(WheelFilename, PathBuf)>>>,
    /// Preview settings for feature flags.
    preview: Preview,
}

impl Locks {
    /// Create a new Locks instance with the given preview settings.
    pub fn new(preview: Preview) -> Self {
        Self {
            copy_dir_locks: Mutex::new(FxHashMap::default()),
            site_packages_paths: Mutex::new(FxHashMap::default()),
            preview,
        }
    }

    /// Register which package installs which (top level) path.
    ///
    /// This is later used warn when different files at the same path exist in multiple packages.
    ///
    /// The first non-self argument is the target path relative to site-packages, the second is the
    /// source path in the unpacked wheel.
    fn register_installed_path(&self, relative: &Path, absolute: &Path, wheel: &WheelFilename) {
        debug_assert!(!relative.is_absolute());
        debug_assert!(absolute.is_absolute());

        // Only register top level entries, these are the only ones we have reliably as cloning
        // a directory on macOS traverses outside our code.
        if relative.components().count() != 1 {
            return;
        }

        self.site_packages_paths
            .lock()
            .unwrap()
            .entry(relative.to_path_buf())
            .or_default()
            .insert((wheel.clone(), absolute.to_path_buf()));
    }

    /// Warn when the same file with different contents exists in multiple packages.
    ///
    /// The intent is to detect different variants of the same package installed over each other,
    /// or different packages using the same top-level module name, which cause non-deterministic
    /// failures only surfacing at runtime. See <https://github.com/astral-sh/uv/pull/13437> for a
    /// list of cases.
    ///
    /// The check has some false negatives. It is rather too lenient than too strict, and includes
    /// support for namespace packages that include the same `__init__.py` file, e.g., gpu-a and
    /// gpu-b both including the same `gpu/__init__.py`.
    ///
    /// We assume that all wheels of a package have the same module(s), so a conflict between
    /// installing two unpacked wheels is a conflict between two packages.
    ///
    /// # Performance
    ///
    /// When there are no namespace packages, this method is a Mutex lock and a hash map iteration.
    ///
    /// When there are namespace packages, we only traverse into directories shared by at least two
    /// packages. For example, for namespace packages gpu-a, gpu-b, and gpu-c with
    /// `gpu/a/__init__.py`, `gpu/b/__init__.py`, and `gpu/c/__init__.py` respectively, we only
    /// need to read the `gpu` directory. If there is a deeper shared directory, we only recurse
    /// down to this directory. As packages without conflicts generally do not share many
    /// directories, we do not recurse far.
    ///
    /// For each directory, we analyze all packages sharing the directory at the same time, reading
    /// the directory in each unpacked wheel only once. Effectively, we perform a parallel directory
    /// walk with early exit.
    ///
    /// We avoid reading the actual file contents and assume they are the same when their file
    /// length matches. This also excludes the same empty `__init__.py` files being reported as
    /// conflicting.
    pub fn warn_package_conflicts(self) -> Result<(), io::Error> {
        // This warning is currently in preview.
        if !self
            .preview
            .is_enabled(PreviewFeature::DetectModuleConflicts)
        {
            return Ok(());
        }

        for (relative, wheels) in &*self.site_packages_paths.lock().unwrap() {
            // Fast path: Only one package is using this module name, no conflicts.
            let mut wheel_iter = wheels.iter();
            let Some(first_wheel) = wheel_iter.next() else {
                debug_assert!(false, "at least one wheel");
                continue;
            };
            if wheel_iter.next().is_none() {
                continue;
            }

            // TODO(konsti): This assumes a path is either a file or a directory in all wheels.
            let file_type = fs_err::metadata(&first_wheel.1)?.file_type();
            if file_type.is_file() {
                // Handle conflicts between files directly in site-packages without a module
                // directory enclosing them.
                let files: BTreeSet<(&WheelFilename, u64)> = wheels
                    .iter()
                    .map(|(wheel, absolute)| Ok((wheel, absolute.metadata()?.len())))
                    .collect::<Result<_, io::Error>>()?;
                Self::warn_file_conflict(relative, &files);
            } else if file_type.is_dir() {
                // Don't early return if the method returns true, so we show warnings for each
                // top-level module.
                Self::warn_directory_conflict(relative, wheels)?;
            } else {
                // We don't expect any other file type, but it's ok if this check has false
                // negatives.
            }
        }

        Ok(())
    }

    /// Analyze a directory for conflicts.
    ///
    /// If there are any non-identical files (checked by size) included in more than one wheel,
    /// report this file and return.
    ///
    /// If there are any directories included in more than one wheel, recurse to analyze whether
    /// the directories contain conflicting files.
    ///
    /// Returns `true` if a warning was emitted.
    fn warn_directory_conflict(
        directory: &Path,
        wheels: &BTreeSet<(WheelFilename, PathBuf)>,
    ) -> Result<bool, io::Error> {
        // The files in the directory, as paths relative to the site-packages, with their origin and
        // size.
        let mut files: BTreeMap<PathBuf, BTreeSet<(&WheelFilename, u64)>> = BTreeMap::default();
        // The directories in the directory, as paths relative to the site-packages, with their
        // origin and absolute path.
        let mut subdirectories: BTreeMap<PathBuf, BTreeSet<(WheelFilename, PathBuf)>> =
            BTreeMap::default();

        // Read the shared directory in each unpacked wheel.
        for (wheel, absolute) in wheels {
            for dir_entry in fs_err::read_dir(absolute)? {
                let dir_entry = dir_entry?;
                let relative = directory.join(dir_entry.file_name());
                let file_type = dir_entry.file_type()?;
                if file_type.is_file() {
                    files
                        .entry(relative)
                        .or_default()
                        .insert((wheel, dir_entry.metadata()?.len()));
                } else if file_type.is_dir() {
                    subdirectories
                        .entry(relative)
                        .or_default()
                        .insert((wheel.clone(), dir_entry.path()));
                } else {
                    // We don't expect any other file type, but it's ok if this check has false
                    // negatives.
                }
            }
        }

        for (file, file_wheels) in files {
            if Self::warn_file_conflict(&file, &file_wheels) {
                return Ok(true);
            }
        }

        for (subdirectory, subdirectory_wheels) in subdirectories {
            if subdirectory_wheels.len() == 1 {
                continue;
            }
            // If there are directories shared between multiple wheels, recurse to check them
            // for shared files.
            if Self::warn_directory_conflict(&subdirectory, &subdirectory_wheels)? {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check if all files are the same size, if so assume they are identical.
    ///
    /// It's unlikely that two modules overlap with different contents but their files all have
    /// the same length, so we use this heuristic in this performance critical path to avoid
    /// reading potentially large files.
    fn warn_file_conflict(file: &Path, file_wheels: &BTreeSet<(&WheelFilename, u64)>) -> bool {
        let Some((_, file_len)) = file_wheels.first() else {
            debug_assert!(false, "Always at least one element");
            return false;
        };
        if !file_wheels
            .iter()
            .any(|(_, file_len_other)| file_len_other != file_len)
        {
            return false;
        }

        let packages = file_wheels
            .iter()
            .map(|(wheel_filename, _file_len)| {
                format!("* {} ({})", wheel_filename.name, wheel_filename)
            })
            .join("\n");
        warn_user!(
            "The file `{}` is provided by more than one package, \
            which causes an install race condition and can result in a broken module. \
            Packages containing the file:\n{}",
            file.user_display(),
            packages
        );

        // Assumption: There is generally two packages that have a conflict. The output is
        // more helpful with a single message that calls out the packages
        // rather than being comprehensive about the conflicting files.
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum LinkMode {
    /// Clone (i.e., copy-on-write or reflink) packages from the wheel into the `site-packages` directory.
    #[serde(alias = "reflink")]
    #[cfg_attr(feature = "clap", value(alias = "reflink"))]
    Clone,
    /// Copy packages from the wheel into the `site-packages` directory.
    Copy,
    /// Hard link packages from the wheel into the `site-packages` directory.
    Hardlink,
    /// Symbolically link packages from the wheel into the `site-packages` directory.
    Symlink,
}

impl Default for LinkMode {
    fn default() -> Self {
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            Self::Clone
        } else {
            Self::Hardlink
        }
    }
}

impl LinkMode {
    /// Extract a wheel by linking all of its files into site packages.
    #[instrument(skip_all)]
    pub fn link_wheel_files(
        self,
        site_packages: impl AsRef<Path>,
        wheel: impl AsRef<Path>,
        locks: &Locks,
        filename: &WheelFilename,
    ) -> Result<usize, Error> {
        match self {
            Self::Clone => clone_wheel_files(site_packages, wheel, locks, filename),
            Self::Copy => copy_wheel_files(site_packages, wheel, locks, filename),
            Self::Hardlink => hardlink_wheel_files(site_packages, wheel, locks, filename),
            Self::Symlink => symlink_wheel_files(site_packages, wheel, locks, filename),
        }
    }

    /// Returns `true` if the link mode is [`LinkMode::Symlink`].
    pub fn is_symlink(&self) -> bool {
        matches!(self, Self::Symlink)
    }
}

/// Extract a wheel by cloning all of its files into site packages. The files will be cloned
/// via copy-on-write, which is similar to a hard link, but allows the files to be modified
/// independently (that is, the file is copied upon modification).
///
/// This method uses `clonefile` on macOS, and `reflink` on Linux. See [`clone_recursive`] for
/// details.
fn clone_wheel_files(
    site_packages: impl AsRef<Path>,
    wheel: impl AsRef<Path>,
    locks: &Locks,
    filename: &WheelFilename,
) -> Result<usize, Error> {
    let wheel = wheel.as_ref();
    let mut count = 0usize;
    let mut attempt = Attempt::default();

    for entry in fs::read_dir(wheel)? {
        let entry = entry?;
        locks.register_installed_path(
            entry
                .path()
                .strip_prefix(wheel)
                .expect("wheel path starts with wheel root"),
            &entry.path(),
            filename,
        );
        clone_recursive(site_packages.as_ref(), wheel, locks, &entry, &mut attempt)?;
        count += 1;
    }

    // The directory mtime is not updated when cloning and the mtime is used by CPython's
    // import mechanisms to determine if it should look for new packages in a directory.
    // Here, we force the mtime to be updated to ensure that packages are importable without
    // manual cache invalidation.
    //
    // <https://github.com/python/cpython/blob/8336cb2b6f428246803b02a4e97fce49d0bb1e09/Lib/importlib/_bootstrap_external.py#L1601>
    let now = SystemTime::now();

    match fs::File::open(site_packages.as_ref()) {
        Ok(dir) => {
            if let Err(err) = dir.set_modified(now) {
                debug!(
                    "Failed to update mtime for {}: {err}",
                    site_packages.as_ref().display()
                );
            }
        }
        Err(err) => debug!(
            "Failed to open {} to update mtime: {err}",
            site_packages.as_ref().display()
        ),
    }

    Ok(count)
}

// Hard linking / reflinking might not be supported but we (afaik) can't detect this ahead of time,
// so we'll try hard linking / reflinking the first file - if this succeeds we'll know later
// errors are not due to lack of os/fs support. If it fails, we'll switch to copying for the rest of the
// install.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Attempt {
    #[default]
    Initial,
    Subsequent,
    UseCopyFallback,
}

/// Recursively clone the contents of `from` into `to`.
///
/// Note the behavior here is platform-dependent.
///
/// On macOS, directories can be recursively copied with a single `clonefile` call. So we only
/// need to iterate over the top-level of the directory, and copy each file or subdirectory
/// unless the subdirectory exists already in which case we'll need to recursively merge its
/// contents with the existing directory.
///
/// On Linux, we need to always reflink recursively, as `FICLONE` ioctl does not support
/// directories. Also note, that reflink is only supported on certain filesystems (btrfs, xfs,
/// ...), and only when it does not cross filesystem boundaries.
///
/// On Windows, we also always need to reflink recursively, as `FSCTL_DUPLICATE_EXTENTS_TO_FILE`
/// ioctl is not supported on directories. Also, it is only supported on certain filesystems
/// (ReFS, SMB, ...).
fn clone_recursive(
    site_packages: &Path,
    wheel: &Path,
    locks: &Locks,
    entry: &DirEntry,
    attempt: &mut Attempt,
) -> Result<(), Error> {
    // Determine the existing and destination paths.
    let from = entry.path();
    let to = site_packages.join(
        from.strip_prefix(wheel)
            .expect("wheel path starts with wheel root"),
    );

    trace!("Cloning {} to {}", from.display(), to.display());

    if (cfg!(windows) || cfg!(target_os = "linux")) && from.is_dir() {
        fs::create_dir_all(&to)?;
        for entry in fs::read_dir(from)? {
            clone_recursive(site_packages, wheel, locks, &entry?, attempt)?;
        }
        return Ok(());
    }

    match attempt {
        Attempt::Initial => {
            if let Err(err) = reflink::reflink(&from, &to) {
                if err.kind() == std::io::ErrorKind::AlreadyExists {
                    // If cloning or copying fails and the directory exists already, it must be
                    // merged recursively.
                    if entry.file_type()?.is_dir() {
                        for entry in fs::read_dir(from)? {
                            clone_recursive(site_packages, wheel, locks, &entry?, attempt)?;
                        }
                    } else {
                        // If file already exists, overwrite it.
                        let tempdir = tempdir_in(site_packages)?;
                        let tempfile = tempdir.path().join(from.file_name().unwrap());
                        if reflink::reflink(&from, &tempfile).is_ok() {
                            fs::rename(&tempfile, to)?;
                        } else {
                            debug!(
                                "Failed to clone `{}` to temporary location `{}`, attempting to copy files as a fallback",
                                from.display(),
                                tempfile.display(),
                            );
                            *attempt = Attempt::UseCopyFallback;
                            synchronized_copy(&from, &to, locks)?;
                        }
                    }
                } else {
                    debug!(
                        "Failed to clone `{}` to `{}`, attempting to copy files as a fallback",
                        from.display(),
                        to.display()
                    );
                    // Fallback to copying
                    *attempt = Attempt::UseCopyFallback;
                    clone_recursive(site_packages, wheel, locks, entry, attempt)?;
                }
            }
        }
        Attempt::Subsequent => {
            if let Err(err) = reflink::reflink(&from, &to) {
                if err.kind() == std::io::ErrorKind::AlreadyExists {
                    // If cloning/copying fails and the directory exists already, it must be merged recursively.
                    if entry.file_type()?.is_dir() {
                        for entry in fs::read_dir(from)? {
                            clone_recursive(site_packages, wheel, locks, &entry?, attempt)?;
                        }
                    } else {
                        // If file already exists, overwrite it.
                        let tempdir = tempdir_in(site_packages)?;
                        let tempfile = tempdir.path().join(from.file_name().unwrap());
                        reflink::reflink(&from, &tempfile)?;
                        fs::rename(&tempfile, to)?;
                    }
                } else {
                    return Err(Error::Reflink { from, to, err });
                }
            }
        }
        Attempt::UseCopyFallback => {
            if entry.file_type()?.is_dir() {
                fs::create_dir_all(&to)?;
                for entry in fs::read_dir(from)? {
                    clone_recursive(site_packages, wheel, locks, &entry?, attempt)?;
                }
            } else {
                synchronized_copy(&from, &to, locks)?;
            }
            warn_user_once!(
                "Failed to clone files; falling back to full copy. This may lead to degraded performance.\n         If the cache and target directories are on different filesystems, reflinking may not be supported.\n         If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning."
            );
        }
    }

    if *attempt == Attempt::Initial {
        *attempt = Attempt::Subsequent;
    }
    Ok(())
}

/// Extract a wheel by copying all of its files into site packages.
fn copy_wheel_files(
    site_packages: impl AsRef<Path>,
    wheel: impl AsRef<Path>,
    locks: &Locks,
    filename: &WheelFilename,
) -> Result<usize, Error> {
    let mut count = 0usize;

    // Walk over the directory.
    for entry in WalkDir::new(&wheel) {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(&wheel).expect("walkdir starts with root");
        let out_path = site_packages.as_ref().join(relative);
        locks.register_installed_path(relative, path, filename);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        synchronized_copy(path, &out_path, locks)?;

        count += 1;
    }

    Ok(count)
}

/// Extract a wheel by hard-linking all of its files into site packages.
fn hardlink_wheel_files(
    site_packages: impl AsRef<Path>,
    wheel: impl AsRef<Path>,
    locks: &Locks,
    filename: &WheelFilename,
) -> Result<usize, Error> {
    let mut attempt = Attempt::default();
    let mut count = 0usize;

    // Walk over the directory.
    for entry in WalkDir::new(&wheel) {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(&wheel).expect("walkdir starts with root");
        let out_path = site_packages.as_ref().join(relative);

        locks.register_installed_path(relative, path, filename);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        // The `RECORD` file is modified during installation, so we copy it instead of hard-linking.
        if path.ends_with("RECORD") {
            synchronized_copy(path, &out_path, locks)?;
            count += 1;
            continue;
        }

        // Fallback to copying if hardlinks aren't supported for this installation.
        match attempt {
            Attempt::Initial => {
                // Once https://github.com/rust-lang/rust/issues/86442 is stable, use that.
                attempt = Attempt::Subsequent;
                if let Err(err) = fs::hard_link(path, &out_path) {
                    // If the file already exists, remove it and try again.
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        debug!(
                            "File already exists (initial attempt), overwriting: {}",
                            out_path.display()
                        );
                        // Removing and recreating would lead to race conditions.
                        let tempdir = tempdir_in(&site_packages)?;
                        let tempfile = tempdir.path().join(entry.file_name());
                        if fs::hard_link(path, &tempfile).is_ok() {
                            fs_err::rename(&tempfile, &out_path)?;
                        } else {
                            debug!(
                                "Failed to hardlink `{}` to `{}`, attempting to copy files as a fallback",
                                out_path.display(),
                                path.display()
                            );
                            synchronized_copy(path, &out_path, locks)?;
                            attempt = Attempt::UseCopyFallback;
                        }
                    } else {
                        debug!(
                            "Failed to hardlink `{}` to `{}`, attempting to copy files as a fallback",
                            out_path.display(),
                            path.display()
                        );
                        synchronized_copy(path, &out_path, locks)?;
                        attempt = Attempt::UseCopyFallback;
                    }
                }
            }
            Attempt::Subsequent => {
                if let Err(err) = fs::hard_link(path, &out_path) {
                    // If the file already exists, remove it and try again.
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        debug!(
                            "File already exists (subsequent attempt), overwriting: {}",
                            out_path.display()
                        );
                        // Removing and recreating would lead to race conditions.
                        let tempdir = tempdir_in(&site_packages)?;
                        let tempfile = tempdir.path().join(entry.file_name());
                        fs::hard_link(path, &tempfile)?;
                        fs_err::rename(&tempfile, &out_path)?;
                    } else {
                        return Err(err.into());
                    }
                }
            }
            Attempt::UseCopyFallback => {
                synchronized_copy(path, &out_path, locks)?;
                warn_user_once!(
                    "Failed to hardlink files; falling back to full copy. This may lead to degraded performance.\n         If the cache and target directories are on different filesystems, hardlinking may not be supported.\n         If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning."
                );
            }
        }

        count += 1;
    }

    Ok(count)
}

/// Extract a wheel by symbolically-linking all of its files into site packages.
fn symlink_wheel_files(
    site_packages: impl AsRef<Path>,
    wheel: impl AsRef<Path>,
    locks: &Locks,
    filename: &WheelFilename,
) -> Result<usize, Error> {
    let mut attempt = Attempt::default();
    let mut count = 0usize;

    // Walk over the directory.
    for entry in WalkDir::new(&wheel) {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(&wheel).unwrap();
        let out_path = site_packages.as_ref().join(relative);

        locks.register_installed_path(relative, path, filename);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        // The `RECORD` file is modified during installation, so we copy it instead of symlinking.
        if path.ends_with("RECORD") {
            synchronized_copy(path, &out_path, locks)?;
            count += 1;
            continue;
        }

        // Fallback to copying if symlinks aren't supported for this installation.
        match attempt {
            Attempt::Initial => {
                // Once https://github.com/rust-lang/rust/issues/86442 is stable, use that.
                attempt = Attempt::Subsequent;
                if let Err(err) = create_symlink(path, &out_path) {
                    // If the file already exists, remove it and try again.
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        debug!(
                            "File already exists (initial attempt), overwriting: {}",
                            out_path.display()
                        );
                        // Removing and recreating would lead to race conditions.
                        let tempdir = tempdir_in(&site_packages)?;
                        let tempfile = tempdir.path().join(entry.file_name());
                        if create_symlink(path, &tempfile).is_ok() {
                            fs::rename(&tempfile, &out_path)?;
                        } else {
                            debug!(
                                "Failed to symlink `{}` to `{}`, attempting to copy files as a fallback",
                                out_path.display(),
                                path.display()
                            );
                            synchronized_copy(path, &out_path, locks)?;
                            attempt = Attempt::UseCopyFallback;
                        }
                    } else {
                        debug!(
                            "Failed to symlink `{}` to `{}`, attempting to copy files as a fallback",
                            out_path.display(),
                            path.display()
                        );
                        synchronized_copy(path, &out_path, locks)?;
                        attempt = Attempt::UseCopyFallback;
                    }
                }
            }
            Attempt::Subsequent => {
                if let Err(err) = create_symlink(path, &out_path) {
                    // If the file already exists, remove it and try again.
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        debug!(
                            "File already exists (subsequent attempt), overwriting: {}",
                            out_path.display()
                        );
                        // Removing and recreating would lead to race conditions.
                        let tempdir = tempdir_in(&site_packages)?;
                        let tempfile = tempdir.path().join(entry.file_name());
                        create_symlink(path, &tempfile)?;
                        fs::rename(&tempfile, &out_path)?;
                    } else {
                        return Err(err.into());
                    }
                }
            }
            Attempt::UseCopyFallback => {
                synchronized_copy(path, &out_path, locks)?;
                warn_user_once!(
                    "Failed to symlink files; falling back to full copy. This may lead to degraded performance.\n         If the cache and target directories are on different filesystems, symlinking may not be supported.\n         If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning."
                );
            }
        }

        count += 1;
    }

    Ok(count)
}

/// Copy from `from` to `to`, ensuring that the parent directory is locked. Avoids simultaneous
/// writes to the same file, which can lead to corruption.
///
/// See: <https://github.com/astral-sh/uv/issues/4831>
fn synchronized_copy(from: &Path, to: &Path, locks: &Locks) -> std::io::Result<()> {
    // Ensure we have a lock for the directory.
    let dir_lock = {
        let mut locks_guard = locks.copy_dir_locks.lock().unwrap();
        locks_guard
            .entry(to.parent().unwrap().to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    };

    // Acquire a lock on the directory.
    let _dir_guard = dir_lock.lock().unwrap();

    // Copy the file, which will also set its permissions.
    fs::copy(from, to)?;

    Ok(())
}

#[cfg(unix)]
fn create_symlink<P: AsRef<Path>, Q: AsRef<Path>>(original: P, link: Q) -> std::io::Result<()> {
    fs_err::os::unix::fs::symlink(original, link)
}

#[cfg(windows)]
fn create_symlink<P: AsRef<Path>, Q: AsRef<Path>>(original: P, link: Q) -> std::io::Result<()> {
    if original.as_ref().is_dir() {
        fs_err::os::windows::fs::symlink_dir(original, link)
    } else {
        fs_err::os::windows::fs::symlink_file(original, link)
    }
}
