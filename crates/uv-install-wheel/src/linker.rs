use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use fs_err as fs;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use tracing::{debug, instrument};

use uv_distribution_filename::WheelFilename;
use uv_fs::Simplified;
use uv_fs::link::{CopyLocks, LinkOptions, OnExistingDirectory, link_dir, link_file};
use uv_preview::{Preview, PreviewFeature};
use uv_warnings::warn_user;

use crate::ArchiveFileManifest;
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
    /// Files omitted from the unpacked wheel and stored in the shared archive-file bucket.
    archive_file_paths: Mutex<BTreeMap<PathBuf, BTreeSet<(WheelFilename, PathBuf)>>>,
    /// Preview settings for feature flags.
    preview: Preview,
}

impl InstallState {
    /// Create a new `InstallState` with the given preview settings.
    pub fn new(preview: Preview) -> Self {
        Self {
            locks: CopyLocks::default(),
            site_packages_paths: Mutex::new(FxHashMap::default()),
            archive_file_paths: Mutex::new(BTreeMap::default()),
            preview,
        }
    }

    /// Get the underlying copy locks for use with [`uv_fs::link::link_dir`] functions.
    fn copy_locks(&self) -> &CopyLocks {
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

    /// Register an installed file stored in the shared archive-file bucket.
    fn register_archive_file_path(&self, relative: &Path, absolute: &Path, wheel: &WheelFilename) {
        debug_assert!(!relative.is_absolute());
        debug_assert!(absolute.is_absolute());

        self.archive_file_paths
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

        let site_packages_paths = self.site_packages_paths.lock().unwrap();
        let mut warned_top_level_paths = BTreeSet::new();
        for (relative, wheels) in &*site_packages_paths {
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
                if Self::warn_file_conflict(relative, &files) {
                    warned_top_level_paths.insert(relative.clone());
                }
            } else if file_type.is_dir() {
                // Don't early return if the method returns true, so we show warnings for each
                // top-level module.
                if Self::warn_directory_conflict(relative, wheels)? {
                    warned_top_level_paths.insert(relative.clone());
                }
            } else {
                // We don't expect any other file type, but it's ok if this check has false
                // negatives.
            }
        }

        for (relative, archive_files) in &*self.archive_file_paths.lock().unwrap() {
            let Some(top_level) = relative.components().next() else {
                continue;
            };
            let top_level = Path::new(top_level.as_os_str());
            if warned_top_level_paths.contains(top_level) {
                continue;
            }

            let mut files: BTreeSet<(&WheelFilename, u64)> = archive_files
                .iter()
                .map(|(wheel, absolute)| Ok((wheel, absolute.metadata()?.len())))
                .collect::<Result<_, io::Error>>()?;

            let archive_wheels = archive_files
                .iter()
                .map(|(wheel, _)| wheel)
                .collect::<BTreeSet<_>>();
            if let Some(wheels) = site_packages_paths.get(top_level) {
                let Ok(remainder) = relative.strip_prefix(top_level) else {
                    continue;
                };
                for (wheel, absolute) in wheels {
                    if archive_wheels.contains(wheel) {
                        continue;
                    }
                    let absolute = absolute.join(remainder);
                    match absolute.metadata() {
                        Ok(metadata) if metadata.is_file() => {
                            files.insert((wheel, metadata.len()));
                        }
                        Ok(_) => {}
                        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                        Err(err) => return Err(err),
                    }
                }
            }

            if Self::warn_file_conflict(relative, &files) {
                warned_top_level_paths.insert(top_level.to_path_buf());
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
#[instrument(skip_all)]
pub(crate) fn link_wheel_files(
    link_mode: Option<LinkMode>,
    site_packages: impl AsRef<Path>,
    wheel: impl AsRef<Path>,
    archive_metadata: Option<&Path>,
    archive_files: Option<&Path>,
    state: &InstallState,
    filename: &WheelFilename,
) -> Result<(), Error> {
    let wheel = wheel.as_ref();
    let site_packages = site_packages.as_ref();
    let archive_file_manifest = read_archive_file_manifest(wheel, archive_metadata)?;
    register_installed_paths(wheel, state, filename)?;
    // Preserve the existing directory-linking default for ordinary wheel files.
    let directory_link_mode = link_mode.unwrap_or_default();

    // The `RECORD` file is modified during installation, so it needs a real
    // copy rather than a link back to the cache.
    let options = LinkOptions::new(directory_link_mode)
        .with_mutable_copy_filter(|p: &Path| p.ends_with("RECORD"))
        .with_copy_locks(state.copy_locks())
        .with_on_existing_directory(OnExistingDirectory::Merge);
    let used_link_mode = link_dir(wheel, site_packages, &options)?;

    if let (Some(archive_file_manifest), Some(archive_files)) =
        (archive_file_manifest.as_ref(), archive_files)
    {
        link_archive_file_manifest_entries(
            site_packages,
            archive_files,
            archive_file_manifest,
            archive_file_link_mode(link_mode, used_link_mode),
            state.copy_locks(),
            state,
            filename,
        )?;
    }

    if used_link_mode == LinkMode::Clone {
        // The directory mtime is not updated when cloning and the mtime is
        // used by CPython's import mechanisms to determine if it should look
        // for new packages in a directory. Force an update so packages are
        // importable without manual cache invalidation.
        //
        // <https://github.com/python/cpython/blob/8336cb2b6f428246803b02a4e97fce49d0bb1e09/Lib/importlib/_bootstrap_external.py#L1601>
        update_site_packages_mtime(site_packages);
    }

    Ok(())
}

fn archive_file_link_mode(
    requested_link_mode: Option<LinkMode>,
    used_link_mode: LinkMode,
) -> LinkMode {
    if requested_link_mode.is_none() && used_link_mode == LinkMode::Clone {
        LinkMode::Hardlink
    } else {
        used_link_mode
    }
}

/// Read the archive-file manifest for a cached archive directory.
fn read_archive_file_manifest(
    wheel: &Path,
    archive_metadata: Option<&Path>,
) -> Result<Option<ArchiveFileManifest>, Error> {
    let Some(archive_metadata) = archive_metadata else {
        return Ok(None);
    };
    let Some(archive_id) = wheel.file_name() else {
        return Ok(None);
    };

    Ok(ArchiveFileManifest::read_from_metadata(
        &archive_metadata.join(archive_id),
    )?)
}

/// Replace installed payloads with links to their shared archive-file objects.
fn link_archive_file_manifest_entries(
    site_packages: &Path,
    archive_files: &Path,
    archive_file_manifest: &ArchiveFileManifest,
    link_mode: LinkMode,
    copy_locks: &CopyLocks,
    state: &InstallState,
    filename: &WheelFilename,
) -> Result<(), Error> {
    let options = LinkOptions::new(link_mode)
        .with_copy_locks(copy_locks)
        .with_on_existing_directory(OnExistingDirectory::Merge);

    for entry in archive_file_manifest.files() {
        if !is_relative_path(entry.path()) || !is_relative_path(entry.object()) {
            return Err(Error::InvalidWheel(format!(
                "archive-file manifest contains an unsafe path: {}",
                entry.path().display()
            )));
        }

        let source = archive_files.join(entry.object());
        let target = site_packages.join(entry.path());
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }

        link_file(&source, &target, &options)?;
        state.register_archive_file_path(entry.path(), &source, filename);
    }

    Ok(())
}

/// Return whether a path can be joined below a trusted root.
fn is_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
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

/// Register top-level wheel paths for conflict detection.
fn register_installed_paths(
    wheel: &Path,
    state: &InstallState,
    filename: &WheelFilename,
) -> Result<(), Error> {
    for entry in fs::read_dir(wheel)? {
        let entry = entry?;
        let path = entry.path();
        let relative = PathBuf::from(entry.file_name());
        state.register_installed_path(&relative, &path, filename);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{LinkMode, archive_file_link_mode};

    #[test]
    fn archive_file_link_mode_uses_hardlinks_after_default_clone() {
        assert_eq!(
            archive_file_link_mode(None, LinkMode::Clone),
            LinkMode::Hardlink
        );
        assert_eq!(
            archive_file_link_mode(Some(LinkMode::Clone), LinkMode::Clone),
            LinkMode::Clone
        );
    }

    #[test]
    fn archive_file_link_mode_preserves_copy_fallback() {
        assert_eq!(archive_file_link_mode(None, LinkMode::Copy), LinkMode::Copy);
    }
}
