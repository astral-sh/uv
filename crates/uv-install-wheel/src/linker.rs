use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use fs_err as fs;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use tracing::{debug, instrument};
use walkdir::WalkDir;

use uv_distribution_filename::WheelFilename;
use uv_fs::Simplified;
use uv_fs::link::{CopyLocks, LinkOptions, OnExistingDirectory, link_dir};
use uv_preview::{Preview, PreviewFeature};
use uv_warnings::warn_user;

use crate::Error;

pub use uv_fs::link::LinkMode;

/// Shared state for concurrent wheel installations.
#[derive(Debug, Default)]
pub struct InstallState {
    /// Directory-level locks to prevent concurrent write corruption.
    locks: CopyLocks,
    /// Top level files and directories in site-packages, stored as relative path, and wheels they
    /// are from, with the absolute paths in the unpacked wheel.
    site_packages_paths: Mutex<FxHashMap<PathBuf, BTreeSet<(WheelFilename, PathBuf)>>>,
    /// Preview settings for feature flags.
    preview: Preview,
}

impl InstallState {
    /// Create a new `InstallState` with the given preview settings.
    pub fn new(preview: Preview) -> Self {
        Self {
            locks: CopyLocks::default(),
            site_packages_paths: Mutex::new(FxHashMap::default()),
            preview,
        }
    }

    /// Get the underlying copy locks for use with [`uv_fs::link::link_dir`] functions.
    pub fn copy_locks(&self) -> &CopyLocks {
        &self.locks
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

/// Extract a wheel by linking all of its files into site packages.
///
/// Returns the number of files extracted.
#[instrument(skip_all)]
pub fn link_wheel_files(
    link_mode: LinkMode,
    site_packages: impl AsRef<Path>,
    wheel: impl AsRef<Path>,
    state: &InstallState,
    filename: &WheelFilename,
) -> Result<usize, Error> {
    let wheel = wheel.as_ref();
    let site_packages = site_packages.as_ref();
    let count = register_installed_paths(wheel, state, filename)?;

    // The `RECORD` file is modified during installation, so it needs a real
    // copy rather than a link back to the cache.
    let options = LinkOptions::new(link_mode)
        .with_mutable_copy_filter(|p: &Path| p.ends_with("RECORD"))
        .with_copy_locks(state.copy_locks())
        .with_on_existing_directory(OnExistingDirectory::Merge);
    let used_link_mode = link_dir(wheel, site_packages, &options)?;

    if used_link_mode == LinkMode::Clone {
        // The directory mtime is not updated when cloning and the mtime is
        // used by CPython's import mechanisms to determine if it should look
        // for new packages in a directory. Force an update so packages are
        // importable without manual cache invalidation.
        //
        // <https://github.com/python/cpython/blob/8336cb2b6f428246803b02a4e97fce49d0bb1e09/Lib/importlib/_bootstrap_external.py#L1601>
        update_site_packages_mtime(site_packages);
    }

    Ok(count)
}

/// Update the mtime of the site-packages directory to the current time.
fn update_site_packages_mtime(site_packages: &Path) {
    let now = SystemTime::now();
    match fs::File::open(site_packages) {
        Ok(dir) => {
            if let Err(err) = dir.set_modified(now) {
                debug!(
                    "Failed to update mtime for {}: {err}",
                    site_packages.display()
                );
            }
        }
        Err(err) => debug!(
            "Failed to open {} to update mtime: {err}",
            site_packages.display()
        ),
    }
}

/// Walk the wheel directory and register all paths for conflict detection.
///
/// Returns the number of files (not directories) in the wheel.
fn register_installed_paths(
    wheel: &Path,
    state: &InstallState,
    filename: &WheelFilename,
) -> Result<usize, Error> {
    let mut count = 0;
    for entry in WalkDir::new(wheel) {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(wheel).expect("walkdir starts with root");
        state.register_installed_path(relative, path, filename);
        if entry.file_type().is_file() {
            count += 1;
        }
    }
    Ok(count)
}
