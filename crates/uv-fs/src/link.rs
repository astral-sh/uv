//! Utilities for linking a file or directory with various options and automated fallback (e.g., to
//! copying) when link methods are unsupported.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rustc_hash::FxHashMap;
use tracing::debug;
use uv_warnings::warn_user_once;
use walkdir::WalkDir;

/// The method to use when linking.
///
/// Defaults to [`Clone`](LinkMode::Clone) on macOS (since APFS supports copy-on-write), and
/// [`Hardlink`](LinkMode::Hardlink) on other platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(deny_unknown_fields, rename_all = "kebab-case")
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum LinkMode {
    /// Clone (i.e., copy-on-write) packages from the source into the destination.
    #[cfg_attr(feature = "serde", serde(alias = "reflink"))]
    #[cfg_attr(feature = "clap", value(alias = "reflink"))]
    Clone,
    /// Copy packages from the source into the destination.
    Copy,
    /// Hard link packages from the source into the destination.
    Hardlink,
    /// Symbolically link packages from the source into the destination.
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
    /// Returns `true` if the link mode is [`Symlink`](LinkMode::Symlink).
    pub fn is_symlink(&self) -> bool {
        matches!(self, Self::Symlink)
    }
}

/// Behavior when the destination directory already exists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OnExistingDirectory {
    /// Fail if the destination directory already exists.
    #[default]
    Fail,
    /// Merge into the existing directory, overwriting files atomically via temp-file renames.
    Merge,
}

/// Link a directory tree from `src` to `dst` using the mode in `options`.
///
/// Returns the [`LinkMode`] that was actually used, which may differ from the requested mode if a
/// fallback was needed, e.g., if hard linking was requested but the source and destination are on
/// different filesystems.
pub fn link_dir<F>(
    src: &Path,
    dst: &Path,
    options: &LinkOptions<'_, F>,
) -> Result<LinkMode, LinkError>
where
    F: Fn(&Path) -> bool,
{
    match options.mode {
        LinkMode::Clone => clone_dir(src, dst, options),
        LinkMode::Copy => copy_dir(src, dst, options),
        LinkMode::Hardlink => hardlink_dir(src, dst, options),
        LinkMode::Symlink => symlink_dir(src, dst, options),
    }
}

/// Directory-level locks for concurrent copy operations.
///
/// Copying is the only non-atomic [`LinkMode`]: it creates a file then writes bytes, so concurrent
/// copies to the same directory can produce corrupted files.
///
/// These locks are used whenever a file is physically copied, regardless of the requested
/// [`LinkMode`], as all modes can fallback to copying.
///
/// The intended pattern for usage is to create a [`CopyLocks`] instance then share it across all
/// [`link_dir`] invocations that may conflict via [`LinkOptions::with_copy_locks`].
#[derive(Debug, Default)]
pub struct CopyLocks {
    dir_locks: Mutex<FxHashMap<PathBuf, Arc<Mutex<()>>>>,
}

impl CopyLocks {
    /// Copy a file with directory-level synchronization.
    ///
    /// Acquires a lock on the parent directory before copying to prevent concurrent writes to the
    /// same directory from corrupting files.
    pub fn synchronized_copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        // Ensure we have a lock for the directory.
        // TODO(zanieb): This unwrap was copied from `uv-install-wheel`; consider propagating the
        // error instead of panicking if `to` has no parent.
        let dir_lock = {
            let mut locks_guard = self.dir_locks.lock().unwrap();
            locks_guard
                .entry(to.parent().unwrap().to_path_buf())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };

        // Acquire a lock on the directory.
        let _dir_guard = dir_lock.lock().unwrap();

        // Copy the file, which will also set its permissions.
        fs_err::copy(from, to)?;

        Ok(())
    }
}

/// Options for directory link operations.
#[derive(Debug)]
pub struct LinkOptions<'a, F = fn(&Path) -> bool> {
    /// The linking strategy to use.
    mode: LinkMode,
    /// Predicate that returns `true` for files that need a mutable (safe to
    /// write) copy. Only applied in hardlink and symlink modes.
    needs_mutable_copy: F,
    /// Optional locks for synchronized copying during concurrent operations.
    copy_locks: Option<&'a CopyLocks>,
    /// What to do when the destination directory already exists.
    on_existing_directory: OnExistingDirectory,
}

impl LinkOptions<'static> {
    /// Create new link options with the given mode.
    pub fn new(mode: LinkMode) -> Self {
        Self {
            mode,
            needs_mutable_copy: |_| false,
            copy_locks: None,
            on_existing_directory: OnExistingDirectory::default(),
        }
    }
}

impl<'a, F> LinkOptions<'a, F> {
    /// Set a predicate for files that need to be writable after linking.
    ///
    /// Should be used for cases where the destination file will be mutated after linking and
    /// changes to the source file are undesirable.
    ///
    /// Files matching this predicate will use [`LinkMode::Copy`] instead when using
    /// [`LinkMode::Hardlink`] or [`LinkMode::Symlink`] are requested.
    ///
    /// Has no effect when using [`LinkMode::Copy`] or [`LinkMode::Clone`], since the linked file is
    /// already mutable without affecting source.
    pub fn with_mutable_copy_filter<G>(self, f: G) -> LinkOptions<'a, G>
    where
        G: Fn(&Path) -> bool,
    {
        LinkOptions {
            mode: self.mode,
            needs_mutable_copy: f,
            copy_locks: self.copy_locks,
            on_existing_directory: self.on_existing_directory,
        }
    }

    /// Set the locks for synchronized copying.
    ///
    /// When provided, file copy operations will acquire a directory-level lock before writing. This
    /// prevents corruption when multiple installations run concurrently.
    #[must_use]
    pub fn with_copy_locks(self, locks: &'a CopyLocks) -> Self {
        LinkOptions {
            mode: self.mode,
            needs_mutable_copy: self.needs_mutable_copy,
            copy_locks: Some(locks),
            on_existing_directory: self.on_existing_directory,
        }
    }

    /// Set the behavior when the destination directory already exists.
    #[must_use]
    pub fn with_on_existing_directory(self, on_existing_directory: OnExistingDirectory) -> Self {
        LinkOptions {
            mode: self.mode,
            needs_mutable_copy: self.needs_mutable_copy,
            copy_locks: self.copy_locks,
            on_existing_directory,
        }
    }

    /// Copy a file, using synchronized copy if locks are configured.
    fn copy_file(&self, from: &Path, to: &Path) -> io::Result<()>
    where
        F: Fn(&Path) -> bool,
    {
        if let Some(copy_locks) = self.copy_locks {
            copy_locks.synchronized_copy(from, to)
        } else {
            fs_err::copy(from, to)?;
            Ok(())
        }
    }
}

/// Tracks the state of linking attempts to handle fallback gracefully.
///
/// Hard linking / reflinking might not be supported but we can't detect this ahead of time,
/// so we'll try the operation on the first file - if this succeeds we'll know later
/// errors are not due to lack of OS/filesystem support. If it fails, we'll switch
/// to copying for the rest of the operation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Attempt {
    #[default]
    Initial,
    Subsequent,
    UseCopyFallback,
}

/// Error type for copy operations.
#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("Failed to read directory `{}`", path.display())]
    WalkDir {
        path: PathBuf,
        #[source]
        err: walkdir::Error,
    },
    #[error("Failed to copy to `{}`", to.display())]
    Copy {
        to: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to create directory `{}`", path.display())]
    CreateDir {
        path: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to clone `{}` to `{}`", from.display(), to.display())]
    Reflink {
        from: PathBuf,
        to: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to create symlink from `{}` to `{}`", from.display(), to.display())]
    Symlink {
        from: PathBuf,
        to: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Clone a directory tree using copy-on-write.
///
/// On macOS with APFS, tries to clone the entire directory in a single syscall.
///
/// On all platforms, attempts to reflink individual files. If reflinking is not supported,
/// falls back to hard linking, then copying.
fn clone_dir<F>(src: &Path, dst: &Path, options: &LinkOptions<'_, F>) -> Result<LinkMode, LinkError>
where
    F: Fn(&Path) -> bool,
{
    // On macOS, try to clone the entire directory in one syscall.
    #[cfg(target_os = "macos")]
    {
        match try_clone_dir_recursive(src, dst, options) {
            Ok(()) => return Ok(LinkMode::Clone),
            Err(e) => {
                debug!(
                    "Failed to clone `{}` to `{}`: {}, falling back to per-file reflink",
                    src.display(),
                    dst.display(),
                    e
                );
            }
        }
    }

    // Try per-file reflinking, with fallback to hardlink then copy
    reflink_dir(src, dst, options)
}

/// Reflink individual files in a directory tree.
///
/// Attempts to reflink each file. If reflinking fails (e.g., unsupported filesystem),
/// falls back to hard linking, then copying.
fn reflink_dir<F>(
    src: &Path,
    dst: &Path,
    options: &LinkOptions<'_, F>,
) -> Result<LinkMode, LinkError>
where
    F: Fn(&Path) -> bool,
{
    let mut attempt = Attempt::Initial;

    for entry in WalkDir::new(src) {
        let entry = entry.map_err(|err| LinkError::WalkDir {
            path: src.to_path_buf(),
            err,
        })?;

        let path = entry.path();
        let relative = path.strip_prefix(src).expect("walkdir starts with root");
        let target = dst.join(relative);

        if entry.file_type().is_dir() {
            fs_err::create_dir_all(&target).map_err(|err| LinkError::CreateDir {
                path: target.clone(),
                err,
            })?;
            continue;
        }

        match attempt {
            Attempt::Initial => {
                match reflink_copy::reflink(path, &target) {
                    Ok(()) => {
                        attempt = Attempt::Subsequent;
                    }
                    Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                        if options.on_existing_directory == OnExistingDirectory::Merge {
                            // File exists, overwrite atomically via temp file
                            let parent = target.parent().unwrap();
                            let tempdir = tempfile::tempdir_in(parent)?;
                            let tempfile = tempdir.path().join(target.file_name().unwrap());
                            if reflink_copy::reflink(path, &tempfile).is_ok() {
                                fs_err::rename(&tempfile, &target)?;
                                attempt = Attempt::Subsequent;
                            } else {
                                // Reflink to temp failed, fallback to hardlink
                                debug!(
                                    "Failed to reflink `{}` to temp location, falling back to hardlink",
                                    path.display()
                                );
                                return hardlink_dir(src, dst, options);
                            }
                        } else {
                            return Err(LinkError::Reflink {
                                from: path.to_path_buf(),
                                to: target,
                                err,
                            });
                        }
                    }
                    Err(err) => {
                        debug!(
                            "Failed to reflink `{}` to `{}`: {}, falling back to hardlink",
                            path.display(),
                            target.display(),
                            err
                        );
                        // Fallback to hardlinking (hardlink_dir handles AlreadyExists for
                        // files we already reflinked when in Merge mode)
                        return hardlink_dir(src, dst, options);
                    }
                }
            }
            Attempt::Subsequent => match reflink_copy::reflink(path, &target) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                    if options.on_existing_directory == OnExistingDirectory::Merge {
                        let parent = target.parent().unwrap();
                        let tempdir = tempfile::tempdir_in(parent)?;
                        let tempfile = tempdir.path().join(target.file_name().unwrap());
                        reflink_copy::reflink(path, &tempfile).map_err(|err| {
                            LinkError::Reflink {
                                from: path.to_path_buf(),
                                to: tempfile.clone(),
                                err,
                            }
                        })?;
                        fs_err::rename(&tempfile, &target)?;
                    } else {
                        return Err(LinkError::Reflink {
                            from: path.to_path_buf(),
                            to: target,
                            err,
                        });
                    }
                }
                Err(err) => {
                    return Err(LinkError::Reflink {
                        from: path.to_path_buf(),
                        to: target,
                        err,
                    });
                }
            },
            Attempt::UseCopyFallback => {
                // We've fallen back to hardlinking; this is handled by returning
                // early to hardlink_dir, so this branch should not be reached.
                unreachable!(
                    "reflink_dir should return to hardlink_dir before reaching UseCopyFallback"
                );
            }
        }
    }

    Ok(LinkMode::Clone)
}

/// Try to clone a directory, handling `merge_directories` option.
#[cfg(target_os = "macos")]
fn try_clone_dir_recursive<F>(
    src: &Path,
    dst: &Path,
    options: &LinkOptions<'_, F>,
) -> Result<(), LinkError>
where
    F: Fn(&Path) -> bool,
{
    match reflink_copy::reflink(src, dst) {
        Ok(()) => {
            debug!(
                "Cloned directory `{}` to `{}`",
                src.display(),
                dst.display()
            );
            Ok(())
        }
        Err(err)
            if err.kind() == io::ErrorKind::AlreadyExists
                && options.on_existing_directory == OnExistingDirectory::Merge =>
        {
            // Directory exists, need to merge recursively
            clone_dir_merge(src, dst, options)
        }
        Err(err) => Err(LinkError::Reflink {
            from: src.to_path_buf(),
            to: dst.to_path_buf(),
            err,
        }),
    }
}

/// Clone a directory by merging into an existing destination.
#[cfg(target_os = "macos")]
fn clone_dir_merge<F>(
    src: &Path,
    dst: &Path,
    _options: &LinkOptions<'_, F>,
) -> Result<(), LinkError>
where
    F: Fn(&Path) -> bool,
{
    for entry in fs_err::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if entry.file_type()?.is_dir() {
            // Try to clone the directory directly first; if it already exists, merge recursively
            match reflink_copy::reflink(&src_path, &dst_path) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                    clone_dir_merge(&src_path, &dst_path, _options)?;
                }
                Err(err) => {
                    return Err(LinkError::Reflink {
                        from: src_path,
                        to: dst_path,
                        err,
                    });
                }
            }
        } else {
            // Try to clone the file
            match reflink_copy::reflink(&src_path, &dst_path) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                    // File exists, overwrite atomically via temp file
                    let tempdir = tempfile::tempdir_in(dst)?;
                    let tempfile = tempdir.path().join(entry.file_name());
                    reflink_copy::reflink(&src_path, &tempfile).map_err(|err| {
                        LinkError::Reflink {
                            from: src_path.clone(),
                            to: tempfile.clone(),
                            err,
                        }
                    })?;
                    fs_err::rename(&tempfile, &dst_path)?;
                }
                Err(err) => {
                    return Err(LinkError::Reflink {
                        from: src_path,
                        to: dst_path,
                        err,
                    });
                }
            }
        }
    }
    Ok(())
}

/// Hard link or copy a directory tree from `src` to `dst`.
///
/// Tries hard linking first for efficiency, falling back to copying if hard links
/// are not supported (e.g., cross-filesystem operations).
fn hardlink_dir<F>(
    src: &Path,
    dst: &Path,
    options: &LinkOptions<'_, F>,
) -> Result<LinkMode, LinkError>
where
    F: Fn(&Path) -> bool,
{
    let mut attempt = Attempt::Initial;

    for entry in WalkDir::new(src) {
        let entry = entry.map_err(|err| LinkError::WalkDir {
            path: src.to_path_buf(),
            err,
        })?;

        let path = entry.path();
        let relative = path.strip_prefix(src).expect("walkdir starts with root");
        let target = dst.join(relative);

        if entry.file_type().is_dir() {
            fs_err::create_dir_all(&target).map_err(|err| LinkError::CreateDir {
                path: target.clone(),
                err,
            })?;
            continue;
        }

        if (options.needs_mutable_copy)(path) {
            options
                .copy_file(path, &target)
                .map_err(|err| LinkError::Copy {
                    to: target.clone(),
                    err,
                })?;
            continue;
        }

        match attempt {
            Attempt::Initial => {
                if let Err(err) = try_hardlink_file(path, &target) {
                    if err.kind() == io::ErrorKind::AlreadyExists
                        && options.on_existing_directory == OnExistingDirectory::Merge
                    {
                        // File exists, try atomic overwrite
                        atomic_hardlink_overwrite(path, &target, &mut attempt, options)?;
                    } else {
                        debug!(
                            "Failed to hard link `{}` to `{}`: {}; falling back to copy",
                            path.display(),
                            target.display(),
                            err
                        );
                        attempt = Attempt::UseCopyFallback;
                        options
                            .copy_file(path, &target)
                            .map_err(|err| LinkError::Copy {
                                to: target.clone(),
                                err,
                            })?;
                        warn_user_once!(
                            "Failed to hardlink files; falling back to full copy. This may lead to degraded performance.\n         \
                            If the cache and target directories are on different filesystems, hardlinking may not be supported.\n         \
                            If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning."
                        );
                    }
                } else {
                    attempt = Attempt::Subsequent;
                }
            }
            Attempt::Subsequent => {
                if let Err(err) = try_hardlink_file(path, &target) {
                    if err.kind() == io::ErrorKind::AlreadyExists
                        && options.on_existing_directory == OnExistingDirectory::Merge
                    {
                        atomic_hardlink_overwrite(path, &target, &mut attempt, options)?;
                    } else {
                        return Err(LinkError::Io(err));
                    }
                }
            }
            Attempt::UseCopyFallback => {
                if options.on_existing_directory == OnExistingDirectory::Merge {
                    atomic_copy_overwrite(path, &target, options)?;
                } else {
                    options
                        .copy_file(path, &target)
                        .map_err(|err| LinkError::Copy {
                            to: target.clone(),
                            err,
                        })?;
                }
            }
        }
    }

    if attempt == Attempt::UseCopyFallback {
        Ok(LinkMode::Copy)
    } else {
        Ok(LinkMode::Hardlink)
    }
}

/// Try to create a hard link, returning the `io::Error` on failure.
fn try_hardlink_file(src: &Path, dst: &Path) -> io::Result<()> {
    fs_err::hard_link(src, dst)
}

/// Atomically overwrite an existing file with a hard link.
fn atomic_hardlink_overwrite<F>(
    src: &Path,
    dst: &Path,
    attempt: &mut Attempt,
    options: &LinkOptions<'_, F>,
) -> Result<(), LinkError>
where
    F: Fn(&Path) -> bool,
{
    // TODO(zanieb): These unwraps were copied from `uv-install-wheel`; consider propagating errors
    // instead of panicking if `dst` has no parent or file name.
    let parent = dst.parent().unwrap();
    let tempdir = tempfile::tempdir_in(parent)?;
    let tempfile = tempdir.path().join(dst.file_name().unwrap());

    if fs_err::hard_link(src, &tempfile).is_ok() {
        fs_err::rename(&tempfile, dst)?;
    } else {
        // Hard link to temp failed, fallback to copy
        debug!(
            "Failed to hardlink `{}` to temp location, falling back to copy",
            src.display()
        );
        *attempt = Attempt::UseCopyFallback;
        atomic_copy_overwrite(src, dst, options)?;
        warn_user_once!(
            "Failed to hardlink files; falling back to full copy. This may lead to degraded performance.\n         \
            If the cache and target directories are on different filesystems, hardlinking may not be supported.\n         \
            If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning."
        );
    }
    Ok(())
}

/// Atomically overwrite an existing file with a copy.
fn atomic_copy_overwrite<F>(
    src: &Path,
    dst: &Path,
    options: &LinkOptions<'_, F>,
) -> Result<(), LinkError>
where
    F: Fn(&Path) -> bool,
{
    // TODO(zanieb): These unwraps were copied from `uv-install-wheel`; consider propagating errors
    // instead of panicking if `dst` has no parent or file name.
    let parent = dst.parent().unwrap();
    let tempdir = tempfile::tempdir_in(parent)?;
    let tempfile = tempdir.path().join(dst.file_name().unwrap());

    options
        .copy_file(src, &tempfile)
        .map_err(|err| LinkError::Copy {
            to: tempfile.clone(),
            err,
        })?;
    fs_err::rename(&tempfile, dst)?;
    Ok(())
}

/// Copy a directory tree from `src` to `dst`.
///
/// Always copies files (no linking). Supports synchronized copying and
/// directory merging via options.
fn copy_dir<F>(src: &Path, dst: &Path, options: &LinkOptions<'_, F>) -> Result<LinkMode, LinkError>
where
    F: Fn(&Path) -> bool,
{
    for entry in WalkDir::new(src) {
        let entry = entry.map_err(|err| LinkError::WalkDir {
            path: src.to_path_buf(),
            err,
        })?;

        let path = entry.path();
        let relative = path.strip_prefix(src).expect("walkdir starts with root");
        let target = dst.join(relative);

        if entry.file_type().is_dir() {
            fs_err::create_dir_all(&target).map_err(|err| LinkError::CreateDir {
                path: target.clone(),
                err,
            })?;
            continue;
        }

        if options.on_existing_directory == OnExistingDirectory::Merge {
            atomic_copy_overwrite(path, &target, options)?;
        } else {
            options
                .copy_file(path, &target)
                .map_err(|err| LinkError::Copy {
                    to: target.clone(),
                    err,
                })?;
        }
    }

    Ok(LinkMode::Copy)
}

/// Symbolically link a directory tree from `src` to `dst`.
///
/// Tries creating symlinks first, falling back to copying if symlinks are not supported.
fn symlink_dir<F>(
    src: &Path,
    dst: &Path,
    options: &LinkOptions<'_, F>,
) -> Result<LinkMode, LinkError>
where
    F: Fn(&Path) -> bool,
{
    let mut attempt = Attempt::Initial;

    for entry in WalkDir::new(src) {
        let entry = entry.map_err(|err| LinkError::WalkDir {
            path: src.to_path_buf(),
            err,
        })?;

        let path = entry.path();
        let relative = path.strip_prefix(src).expect("walkdir starts with root");
        let target = dst.join(relative);

        if entry.file_type().is_dir() {
            fs_err::create_dir_all(&target).map_err(|err| LinkError::CreateDir {
                path: target.clone(),
                err,
            })?;
            continue;
        }

        if (options.needs_mutable_copy)(path) {
            options
                .copy_file(path, &target)
                .map_err(|err| LinkError::Copy {
                    to: target.clone(),
                    err,
                })?;
            continue;
        }

        match attempt {
            Attempt::Initial => {
                if let Err(err) = create_symlink(path, &target) {
                    if err.kind() == io::ErrorKind::AlreadyExists
                        && options.on_existing_directory == OnExistingDirectory::Merge
                    {
                        atomic_symlink_overwrite(path, &target, &mut attempt, options)?;
                    } else {
                        debug!(
                            "Failed to symlink `{}` to `{}`: {}; falling back to copy",
                            path.display(),
                            target.display(),
                            err
                        );
                        attempt = Attempt::UseCopyFallback;
                        options
                            .copy_file(path, &target)
                            .map_err(|err| LinkError::Copy {
                                to: target.clone(),
                                err,
                            })?;
                        warn_user_once!(
                            "Failed to symlink files; falling back to full copy. This may lead to degraded performance.\n         \
                            If the cache and target directories are on different filesystems, symlinking may not be supported.\n         \
                            If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning."
                        );
                    }
                } else {
                    attempt = Attempt::Subsequent;
                }
            }
            Attempt::Subsequent => {
                if let Err(err) = create_symlink(path, &target) {
                    if err.kind() == io::ErrorKind::AlreadyExists
                        && options.on_existing_directory == OnExistingDirectory::Merge
                    {
                        atomic_symlink_overwrite(path, &target, &mut attempt, options)?;
                    } else {
                        return Err(LinkError::Symlink {
                            from: path.to_path_buf(),
                            to: target,
                            err,
                        });
                    }
                }
            }
            Attempt::UseCopyFallback => {
                if options.on_existing_directory == OnExistingDirectory::Merge {
                    atomic_copy_overwrite(path, &target, options)?;
                } else {
                    options
                        .copy_file(path, &target)
                        .map_err(|err| LinkError::Copy {
                            to: target.clone(),
                            err,
                        })?;
                }
            }
        }
    }

    if attempt == Attempt::UseCopyFallback {
        Ok(LinkMode::Copy)
    } else {
        Ok(LinkMode::Symlink)
    }
}

/// Atomically overwrite an existing file with a symlink.
fn atomic_symlink_overwrite<F>(
    src: &Path,
    dst: &Path,
    attempt: &mut Attempt,
    options: &LinkOptions<'_, F>,
) -> Result<(), LinkError>
where
    F: Fn(&Path) -> bool,
{
    // TODO(zanieb): These unwraps were copied from `uv-install-wheel`; consider propagating errors
    // instead of panicking if `dst` has no parent or file name.
    let parent = dst.parent().unwrap();
    let tempdir = tempfile::tempdir_in(parent)?;
    let tempfile = tempdir.path().join(dst.file_name().unwrap());

    if create_symlink(src, &tempfile).is_ok() {
        fs_err::rename(&tempfile, dst)?;
    } else {
        // Symlink to temp failed, fallback to copy
        debug!(
            "Failed to symlink `{}` to temp location, falling back to copy",
            src.display()
        );
        *attempt = Attempt::UseCopyFallback;
        atomic_copy_overwrite(src, dst, options)?;
        warn_user_once!(
            "Failed to symlink files; falling back to full copy. This may lead to degraded performance.\n         \
            If the cache and target directories are on different filesystems, symlinking may not be supported.\n         \
            If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning."
        );
    }
    Ok(())
}

/// Create a symbolic link.
#[cfg(unix)]
fn create_symlink(original: &Path, link: &Path) -> io::Result<()> {
    fs_err::os::unix::fs::symlink(original, link)
}

/// Create a symbolic link.
#[cfg(windows)]
fn create_symlink(original: &Path, link: &Path) -> io::Result<()> {
    if original.is_dir() {
        fs_err::os::windows::fs::symlink_dir(original, link)
    } else {
        fs_err::os::windows::fs::symlink_file(original, link)
    }
}
