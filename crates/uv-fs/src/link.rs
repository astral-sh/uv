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
        mode => walk_and_link(src, dst, mode, options),
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

/// Whether the current linking strategy has been confirmed to work.
///
/// Some linking strategies (reflink, hardlink, symlink) might not be supported on a given
/// filesystem, but we can't always detect this ahead of time. We try the operation on the
/// first file — if it succeeds, we know later errors are real failures. If it fails, we
/// switch to the next fallback strategy for the rest of the operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkAttempt {
    /// The strategy has not yet been attempted on any file.
    Initial,
    /// The strategy succeeded for at least one file; continue using it.
    Subsequent,
}

/// Tracks the active linking strategy and whether it has been confirmed to work.
///
/// When the strategy's [`LinkAttempt`] is [`Initial`](LinkAttempt::Initial) and the first
/// file operation fails, [`next_mode`](Self::next_mode) transitions to the next fallback.
/// When the attempt is [`Subsequent`](LinkAttempt::Subsequent) and a file fails, it is
/// either a hard error (the strategy was confirmed to work, so this is a real failure) or,
/// for reflink, a transition to the next fallback.
#[derive(Debug, Clone, Copy)]
struct LinkState {
    /// The linking strategy currently in use.
    mode: LinkMode,
    /// Whether the strategy has been confirmed to work.
    attempt: LinkAttempt,
}

impl LinkState {
    /// Create a new state with the given mode, not yet confirmed to work.
    fn new(mode: LinkMode) -> Self {
        Self {
            mode,
            attempt: LinkAttempt::Initial,
        }
    }

    /// Mark the current strategy as confirmed working on this filesystem.
    fn mode_working(self) -> Self {
        Self {
            attempt: LinkAttempt::Subsequent,
            ..self
        }
    }

    /// Transition to the next fallback strategy in the chain.
    ///
    /// - `Clone` → `Hardlink`
    /// - `Hardlink` → `Copy`
    /// - `Symlink` → `Copy`
    /// - `Copy` → Failure
    fn next_mode(self) -> Self {
        debug_assert!(
            self.mode != LinkMode::Copy,
            "Copy is the terminal fallback strategy and has no next mode"
        );
        Self::new(match self.mode {
            LinkMode::Clone => LinkMode::Hardlink,
            LinkMode::Hardlink | LinkMode::Symlink | LinkMode::Copy => LinkMode::Copy,
        })
    }
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
/// On all platforms, falls through to [`walk_and_link`] for per-file linking with
/// automatic fallback.
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

    walk_and_link(src, dst, LinkMode::Clone, options)
}

/// Walk a directory tree and link each file using the given starting [`LinkMode`].
///
/// The [`LinkState`] tracks the active strategy and automatically falls back via
/// [`LinkState::next_mode`] as needed.
fn walk_and_link<F>(
    src: &Path,
    dst: &Path,
    mode: LinkMode,
    options: &LinkOptions<'_, F>,
) -> Result<LinkMode, LinkError>
where
    F: Fn(&Path) -> bool,
{
    let mut state = LinkState::new(mode);

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

        state = link_file(path, &target, state, options)?;
    }

    Ok(state.mode)
}

/// Dispatch a single file to the appropriate linking strategy based on the current state.
///
/// Returns the (possibly updated) state for the next file. When a strategy fails, it
/// transitions to [`LinkState::next_mode`] and re-dispatches through this function so the
/// fallback chain is followed automatically.
fn link_file<F>(
    path: &Path,
    target: &Path,
    state: LinkState,
    options: &LinkOptions<'_, F>,
) -> Result<LinkState, LinkError>
where
    F: Fn(&Path) -> bool,
{
    match state.mode {
        LinkMode::Clone => reflink_file_with_fallback(path, target, state, options),
        LinkMode::Hardlink => hardlink_file_with_fallback(path, target, state, options),
        LinkMode::Symlink => symlink_file_with_fallback(path, target, state, options),
        LinkMode::Copy => {
            if options.on_existing_directory == OnExistingDirectory::Merge {
                atomic_copy_overwrite(path, target, options)?;
            } else {
                copy_file(path, target, options)?;
            }
            Ok(state)
        }
    }
}

/// Attempt to reflink a single file, falling back via [`link_file`] on failure.
fn reflink_file_with_fallback<F>(
    path: &Path,
    target: &Path,
    state: LinkState,
    options: &LinkOptions<'_, F>,
) -> Result<LinkState, LinkError>
where
    F: Fn(&Path) -> bool,
{
    match state.attempt {
        LinkAttempt::Initial => match reflink_copy::reflink(path, target) {
            Ok(()) => Ok(state.mode_working()),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                if options.on_existing_directory == OnExistingDirectory::Merge {
                    // File exists, overwrite atomically via temp file
                    let parent = target.parent().unwrap();
                    let tempdir = tempfile::tempdir_in(parent)?;
                    let tempfile = tempdir.path().join(target.file_name().unwrap());
                    if reflink_copy::reflink(path, &tempfile).is_ok() {
                        fs_err::rename(&tempfile, target)?;
                        Ok(state.mode_working())
                    } else {
                        debug!(
                            "Failed to reflink `{}` to temp location, falling back",
                            path.display()
                        );
                        link_file(path, target, state.next_mode(), options)
                    }
                } else {
                    Err(LinkError::Reflink {
                        from: path.to_path_buf(),
                        to: target.to_path_buf(),
                        err,
                    })
                }
            }
            Err(err) => {
                debug!(
                    "Failed to reflink `{}` to `{}`: {}, falling back",
                    path.display(),
                    target.display(),
                    err
                );
                link_file(path, target, state.next_mode(), options)
            }
        },
        LinkAttempt::Subsequent => match reflink_copy::reflink(path, target) {
            Ok(()) => Ok(state),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                if options.on_existing_directory == OnExistingDirectory::Merge {
                    let parent = target.parent().unwrap();
                    let tempdir = tempfile::tempdir_in(parent)?;
                    let tempfile = tempdir.path().join(target.file_name().unwrap());
                    reflink_copy::reflink(path, &tempfile).map_err(|err| LinkError::Reflink {
                        from: path.to_path_buf(),
                        to: tempfile.clone(),
                        err,
                    })?;
                    fs_err::rename(&tempfile, target)?;
                    Ok(state)
                } else {
                    Err(LinkError::Reflink {
                        from: path.to_path_buf(),
                        to: target.to_path_buf(),
                        err,
                    })
                }
            }
            Err(err) => Err(LinkError::Reflink {
                from: path.to_path_buf(),
                to: target.to_path_buf(),
                err,
            }),
        },
    }
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

/// Attempt to hard link a single file, falling back via [`link_file`] on failure.
///
/// Files matching the [`LinkOptions::needs_mutable_copy`] predicate are always copied
/// to avoid mutating the source through a hard link.
fn hardlink_file_with_fallback<F>(
    path: &Path,
    target: &Path,
    state: LinkState,
    options: &LinkOptions<'_, F>,
) -> Result<LinkState, LinkError>
where
    F: Fn(&Path) -> bool,
{
    if (options.needs_mutable_copy)(path) {
        copy_file(path, target, options)?;
        return Ok(state);
    }

    match state.attempt {
        LinkAttempt::Initial => {
            if let Err(err) = try_hardlink_file(path, target) {
                if err.kind() == io::ErrorKind::AlreadyExists
                    && options.on_existing_directory == OnExistingDirectory::Merge
                {
                    atomic_hardlink_overwrite(path, target, state, options)
                } else {
                    debug!(
                        "Failed to hard link `{}` to `{}`: {}; falling back to copy",
                        path.display(),
                        target.display(),
                        err
                    );
                    warn_user_once!(
                        "Failed to hardlink files; falling back to full copy. This may lead to degraded performance.\n         \
                        If the cache and target directories are on different filesystems, hardlinking may not be supported.\n         \
                        If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning."
                    );
                    link_file(path, target, state.next_mode(), options)
                }
            } else {
                Ok(state.mode_working())
            }
        }
        LinkAttempt::Subsequent => {
            if let Err(err) = try_hardlink_file(path, target) {
                if err.kind() == io::ErrorKind::AlreadyExists
                    && options.on_existing_directory == OnExistingDirectory::Merge
                {
                    atomic_hardlink_overwrite(path, target, state, options)
                } else {
                    Err(LinkError::Io(err))
                }
            } else {
                Ok(state)
            }
        }
    }
}

/// Attempt to symlink a single file, falling back via [`link_file`] on failure.
///
/// Files matching the [`LinkOptions::needs_mutable_copy`] predicate are always copied
/// to avoid mutating the source through a symlink.
fn symlink_file_with_fallback<F>(
    path: &Path,
    target: &Path,
    state: LinkState,
    options: &LinkOptions<'_, F>,
) -> Result<LinkState, LinkError>
where
    F: Fn(&Path) -> bool,
{
    if (options.needs_mutable_copy)(path) {
        copy_file(path, target, options)?;
        return Ok(state);
    }

    match state.attempt {
        LinkAttempt::Initial => {
            if let Err(err) = create_symlink(path, target) {
                if err.kind() == io::ErrorKind::AlreadyExists
                    && options.on_existing_directory == OnExistingDirectory::Merge
                {
                    atomic_symlink_overwrite(path, target, state, options)
                } else {
                    debug!(
                        "Failed to symlink `{}` to `{}`: {}; falling back to copy",
                        path.display(),
                        target.display(),
                        err
                    );
                    warn_user_once!(
                        "Failed to symlink files; falling back to full copy. This may lead to degraded performance.\n         \
                        If the cache and target directories are on different filesystems, symlinking may not be supported.\n         \
                        If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning."
                    );
                    link_file(path, target, state.next_mode(), options)
                }
            } else {
                Ok(state.mode_working())
            }
        }
        LinkAttempt::Subsequent => {
            if let Err(err) = create_symlink(path, target) {
                if err.kind() == io::ErrorKind::AlreadyExists
                    && options.on_existing_directory == OnExistingDirectory::Merge
                {
                    atomic_symlink_overwrite(path, target, state, options)
                } else {
                    Err(LinkError::Symlink {
                        from: path.to_path_buf(),
                        to: target.to_path_buf(),
                        err,
                    })
                }
            } else {
                Ok(state)
            }
        }
    }
}

/// Copy a single file, using synchronized copying if [`CopyLocks`] are configured.
fn copy_file<F>(path: &Path, target: &Path, options: &LinkOptions<'_, F>) -> Result<(), LinkError>
where
    F: Fn(&Path) -> bool,
{
    options
        .copy_file(path, target)
        .map_err(|err| LinkError::Copy {
            to: target.to_path_buf(),
            err,
        })
}

/// Try to create a hard link, returning the `io::Error` on failure.
fn try_hardlink_file(src: &Path, dst: &Path) -> io::Result<()> {
    fs_err::hard_link(src, dst)
}

/// Atomically overwrite an existing file with a hard link.
fn atomic_hardlink_overwrite<F>(
    src: &Path,
    dst: &Path,
    state: LinkState,
    options: &LinkOptions<'_, F>,
) -> Result<LinkState, LinkError>
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
        Ok(state.mode_working())
    } else {
        debug!(
            "Failed to hardlink `{}` to temp location, falling back to copy",
            src.display()
        );
        warn_user_once!(
            "Failed to hardlink files; falling back to full copy. This may lead to degraded performance.\n         \
            If the cache and target directories are on different filesystems, hardlinking may not be supported.\n         \
            If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning."
        );
        let state = state.next_mode();
        atomic_copy_overwrite(src, dst, options)?;
        Ok(state)
    }
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

/// Atomically overwrite an existing file with a symlink.
fn atomic_symlink_overwrite<F>(
    src: &Path,
    dst: &Path,
    state: LinkState,
    options: &LinkOptions<'_, F>,
) -> Result<LinkState, LinkError>
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
        Ok(state.mode_working())
    } else {
        debug!(
            "Failed to symlink `{}` to temp location, falling back to copy",
            src.display()
        );
        warn_user_once!(
            "Failed to symlink files; falling back to full copy. This may lead to degraded performance.\n         \
            If the cache and target directories are on different filesystems, symlinking may not be supported.\n         \
            If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning."
        );
        let state = state.next_mode();
        atomic_copy_overwrite(src, dst, options)?;
        Ok(state)
    }
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

#[cfg(test)]
#[allow(clippy::print_stderr)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create a test directory structure with some files.
    fn create_test_tree(root: &Path) {
        fs_err::create_dir_all(root.join("subdir")).unwrap();
        fs_err::write(root.join("file1.txt"), "content1").unwrap();
        fs_err::write(root.join("file2.txt"), "content2").unwrap();
        fs_err::write(root.join("subdir/nested.txt"), "nested content").unwrap();
    }

    /// Verify the destination has the expected structure and content.
    fn verify_test_tree(root: &Path) {
        assert!(root.join("file1.txt").exists());
        assert!(root.join("file2.txt").exists());
        assert!(root.join("subdir/nested.txt").exists());
        assert_eq!(
            fs_err::read_to_string(root.join("file1.txt")).unwrap(),
            "content1"
        );
        assert_eq!(
            fs_err::read_to_string(root.join("file2.txt")).unwrap(),
            "content2"
        );
        assert_eq!(
            fs_err::read_to_string(root.join("subdir/nested.txt")).unwrap(),
            "nested content"
        );
    }

    #[test]
    fn test_copy_dir_basic() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        let options = LinkOptions::new(LinkMode::Copy);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        assert_eq!(result, LinkMode::Copy);
        verify_test_tree(dst_dir.path());
    }

    #[test]
    fn test_hardlink_dir_basic() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        let options = LinkOptions::new(LinkMode::Hardlink);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        // May fall back to copy on some filesystems
        assert!(result == LinkMode::Hardlink || result == LinkMode::Copy);
        verify_test_tree(dst_dir.path());

        // If hardlink succeeded, verify files share the same inode
        #[cfg(unix)]
        if result == LinkMode::Hardlink {
            use std::os::unix::fs::MetadataExt;
            let src_meta = fs_err::metadata(src_dir.path().join("file1.txt")).unwrap();
            let dst_meta = fs_err::metadata(dst_dir.path().join("file1.txt")).unwrap();
            assert_eq!(src_meta.ino(), dst_meta.ino());
        }
    }

    #[test]
    #[cfg(unix)] // Symlinks require special permissions on Windows
    fn test_symlink_dir_basic() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        let options = LinkOptions::new(LinkMode::Symlink);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        // May fall back to copy on some filesystems
        assert!(result == LinkMode::Symlink || result == LinkMode::Copy);
        verify_test_tree(dst_dir.path());

        // If symlink succeeded, verify files are symlinks
        if result == LinkMode::Symlink {
            assert!(dst_dir.path().join("file1.txt").is_symlink());
        }
    }

    #[test]
    fn test_clone_dir_basic() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        let options = LinkOptions::new(LinkMode::Clone);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        // Clone may fall back to hardlink or copy depending on filesystem
        assert!(
            result == LinkMode::Clone || result == LinkMode::Hardlink || result == LinkMode::Copy
        );
        verify_test_tree(dst_dir.path());
    }

    /// Check if reflink is supported by attempting to reflink a test file.
    /// Returns true if reflink is supported on this filesystem.
    fn reflink_supported(dir: &Path) -> bool {
        let src = dir.join("reflink_test_src");
        let dst = dir.join("reflink_test_dst");
        fs_err::write(&src, "test").unwrap();
        let supported = reflink_copy::reflink(&src, &dst).is_ok();
        let _ = fs_err::remove_file(&src);
        let _ = fs_err::remove_file(&dst);
        supported
    }

    #[test]
    fn test_reflink_file_when_supported() {
        let tmp_dir = TempDir::new().unwrap();

        if !reflink_supported(tmp_dir.path()) {
            eprintln!("Skipping test: reflink not supported on this filesystem");
            return;
        }

        // Create source file
        let src = tmp_dir.path().join("src.txt");
        let dst = tmp_dir.path().join("dst.txt");
        fs_err::write(&src, "reflink content").unwrap();

        // Reflink should succeed
        reflink_copy::reflink(&src, &dst).unwrap();

        assert_eq!(fs_err::read_to_string(&dst).unwrap(), "reflink content");

        // Modifying dst should not affect src (copy-on-write)
        fs_err::write(&dst, "modified").unwrap();
        assert_eq!(fs_err::read_to_string(&src).unwrap(), "reflink content");
        assert_eq!(fs_err::read_to_string(&dst).unwrap(), "modified");
    }

    #[test]
    fn test_clone_dir_reflink_when_supported() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        if !reflink_supported(src_dir.path()) {
            eprintln!("Skipping test: reflink not supported on this filesystem");
            return;
        }

        create_test_tree(src_dir.path());

        let options = LinkOptions::new(LinkMode::Clone);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        // On supported filesystems, clone should succeed
        assert_eq!(result, LinkMode::Clone);
        verify_test_tree(dst_dir.path());

        // Verify copy-on-write: modifying dst should not affect src
        fs_err::write(dst_dir.path().join("file1.txt"), "modified").unwrap();
        assert_eq!(
            fs_err::read_to_string(src_dir.path().join("file1.txt")).unwrap(),
            "content1"
        );
    }

    #[test]
    fn test_clone_merge_when_supported() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        if !reflink_supported(src_dir.path()) {
            eprintln!("Skipping test: reflink not supported on this filesystem");
            return;
        }

        create_test_tree(src_dir.path());

        // Pre-create destination with some existing content
        fs_err::create_dir_all(dst_dir.path()).unwrap();
        fs_err::write(dst_dir.path().join("file1.txt"), "old content").unwrap();
        fs_err::write(dst_dir.path().join("extra.txt"), "extra").unwrap();

        let options = LinkOptions::new(LinkMode::Clone)
            .with_on_existing_directory(OnExistingDirectory::Merge);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        assert_eq!(result, LinkMode::Clone);

        // Source files should overwrite destination
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("file1.txt")).unwrap(),
            "content1"
        );
        // Extra file should remain
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("extra.txt")).unwrap(),
            "extra"
        );
    }

    #[test]
    fn test_reflink_fallback_to_hardlink() {
        // This test verifies the fallback behavior when reflink fails.
        // We can't easily force reflink to fail on a supporting filesystem,
        // so we just verify the clone path works and returns a valid mode.
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        let options = LinkOptions::new(LinkMode::Clone);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        // Should succeed with one of the valid modes
        assert!(matches!(
            result,
            LinkMode::Clone | LinkMode::Hardlink | LinkMode::Copy
        ));
        verify_test_tree(dst_dir.path());
    }

    #[test]
    fn test_merge_overwrites_existing_files() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create source
        create_test_tree(src_dir.path());

        // Create destination with different content
        fs_err::create_dir_all(dst_dir.path().join("subdir")).unwrap();
        fs_err::write(dst_dir.path().join("file1.txt"), "old content").unwrap();
        fs_err::write(dst_dir.path().join("existing.txt"), "should remain").unwrap();

        let options =
            LinkOptions::new(LinkMode::Copy).with_on_existing_directory(OnExistingDirectory::Merge);
        link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        // Verify source files overwrote destination
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("file1.txt")).unwrap(),
            "content1"
        );
        // Verify existing file that wasn't in source remains
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("existing.txt")).unwrap(),
            "should remain"
        );
    }

    #[test]
    fn test_fail_mode_errors_on_existing_hardlink() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        // Create conflicting file in destination
        fs_err::write(dst_dir.path().join("file1.txt"), "existing").unwrap();

        // Hardlink mode with Fail should error when target exists
        let options = LinkOptions::new(LinkMode::Hardlink)
            .with_on_existing_directory(OnExistingDirectory::Fail);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options);

        // Should either fail with AlreadyExists error, or fall back to copy
        // (which overwrites). The key is it doesn't do atomic overwrite.
        // On filesystems where hardlink works, this should fail.
        // We can't guarantee the error because hardlink might fall back to copy.
        if result.is_ok() {
            // If it succeeded, hardlink must have fallen back to copy
            // which overwrites the file
            assert_eq!(
                fs_err::read_to_string(dst_dir.path().join("file1.txt")).unwrap(),
                "content1"
            );
        }
    }

    #[test]
    fn test_copy_mode_overwrites_in_fail_mode() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        // Create conflicting file in destination
        fs_err::write(dst_dir.path().join("file1.txt"), "existing").unwrap();

        // Copy mode always overwrites, even in Fail mode
        // (Fail mode only affects link operations that naturally fail on AlreadyExists)
        let options =
            LinkOptions::new(LinkMode::Copy).with_on_existing_directory(OnExistingDirectory::Fail);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options);

        assert!(result.is_ok());
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("file1.txt")).unwrap(),
            "content1"
        );
    }

    #[test]
    fn test_mutable_copy_filter() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());
        // Add a RECORD file that should be copied, not linked
        fs_err::write(src_dir.path().join("RECORD"), "record content").unwrap();

        let options = LinkOptions::new(LinkMode::Hardlink)
            .with_mutable_copy_filter(|p: &Path| p.ends_with("RECORD"));
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        // Verify RECORD exists
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("RECORD")).unwrap(),
            "record content"
        );

        // If hardlink succeeded, RECORD should NOT be a hardlink (different inode)
        if result == LinkMode::Hardlink {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let src_meta = fs_err::metadata(src_dir.path().join("RECORD")).unwrap();
                let dst_meta = fs_err::metadata(dst_dir.path().join("RECORD")).unwrap();
                // RECORD should be copied, not hardlinked
                assert_ne!(src_meta.ino(), dst_meta.ino());

                // But regular files should be hardlinked
                let src_file_meta = fs_err::metadata(src_dir.path().join("file1.txt")).unwrap();
                let dst_file_meta = fs_err::metadata(dst_dir.path().join("file1.txt")).unwrap();
                assert_eq!(src_file_meta.ino(), dst_file_meta.ino());
            }
        }
    }

    #[test]
    fn test_synchronized_copy() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        let locks = CopyLocks::default();
        let options = LinkOptions::new(LinkMode::Copy).with_copy_locks(&locks);

        link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        verify_test_tree(dst_dir.path());
    }

    #[test]
    fn test_empty_directory() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create empty subdirectory
        fs_err::create_dir_all(src_dir.path().join("empty_subdir")).unwrap();

        let options = LinkOptions::new(LinkMode::Copy);
        link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        assert!(dst_dir.path().join("empty_subdir").is_dir());
    }

    #[test]
    fn test_nested_directories() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create deeply nested structure
        let deep_path = src_dir.path().join("a/b/c/d/e");
        fs_err::create_dir_all(&deep_path).unwrap();
        fs_err::write(deep_path.join("deep.txt"), "deep content").unwrap();

        let options = LinkOptions::new(LinkMode::Copy);
        link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("a/b/c/d/e/deep.txt")).unwrap(),
            "deep content"
        );
    }

    #[test]
    fn test_hardlink_merge_with_existing() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        // Pre-create destination with existing file
        fs_err::create_dir_all(dst_dir.path()).unwrap();
        fs_err::write(dst_dir.path().join("file1.txt"), "old").unwrap();

        let options = LinkOptions::new(LinkMode::Hardlink)
            .with_on_existing_directory(OnExistingDirectory::Merge);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        assert!(result == LinkMode::Hardlink || result == LinkMode::Copy);

        // Content should be overwritten
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("file1.txt")).unwrap(),
            "content1"
        );
    }

    #[test]
    fn test_copy_locks_synchronization() {
        use std::sync::Arc;
        use std::thread;

        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create a file to copy
        fs_err::write(src_dir.path().join("file.txt"), "content").unwrap();

        let locks = Arc::new(CopyLocks::default());
        let src = src_dir.path().to_path_buf();
        let dst = dst_dir.path().to_path_buf();

        // Spawn multiple threads that try to copy concurrently
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let locks = Arc::clone(&locks);
                let src = src.clone();
                let dst = dst.clone();
                thread::spawn(move || {
                    let options = LinkOptions::new(LinkMode::Copy)
                        .with_copy_locks(&locks)
                        .with_on_existing_directory(OnExistingDirectory::Merge);
                    link_dir(&src, &dst, &options)
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap().unwrap();
        }

        // Verify file is intact
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("file.txt")).unwrap(),
            "content"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_symlink_merge_with_existing() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        // Pre-create destination with existing file
        fs_err::create_dir_all(dst_dir.path()).unwrap();
        fs_err::write(dst_dir.path().join("file1.txt"), "old").unwrap();

        let options = LinkOptions::new(LinkMode::Symlink)
            .with_on_existing_directory(OnExistingDirectory::Merge);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        assert!(result == LinkMode::Symlink || result == LinkMode::Copy);

        // Content should come from source (via symlink or copy)
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("file1.txt")).unwrap(),
            "content1"
        );

        // If symlink succeeded, verify it's a symlink
        if result == LinkMode::Symlink {
            assert!(dst_dir.path().join("file1.txt").is_symlink());
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_symlink_mutable_copy_filter() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());
        fs_err::write(src_dir.path().join("RECORD"), "record content").unwrap();

        let options = LinkOptions::new(LinkMode::Symlink)
            .with_mutable_copy_filter(|p: &Path| p.ends_with("RECORD"));
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        // Verify RECORD exists and has correct content
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("RECORD")).unwrap(),
            "record content"
        );

        // If symlink succeeded, RECORD should NOT be a symlink (it was copied)
        if result == LinkMode::Symlink {
            assert!(!dst_dir.path().join("RECORD").is_symlink());
            // But regular files should be symlinks
            assert!(dst_dir.path().join("file1.txt").is_symlink());
        }
    }

    #[test]
    fn test_source_not_found() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Don't create any files in src_dir, just use a non-existent path
        let nonexistent = src_dir.path().join("nonexistent");

        let options = LinkOptions::new(LinkMode::Copy);
        let result = link_dir(&nonexistent, dst_dir.path(), &options);

        assert!(result.is_err());
    }

    #[test]
    fn test_clone_mutable_copy_filter_ignored() {
        // The mutable_copy filter only applies to hardlink/symlink modes.
        // For clone/copy modes, all files are already mutable (copy-on-write or full copy).
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());
        fs_err::write(src_dir.path().join("RECORD"), "record content").unwrap();

        // Even with filter, clone mode should work (filter is ignored)
        let options = LinkOptions::new(LinkMode::Clone)
            .with_mutable_copy_filter(|p: &Path| p.ends_with("RECORD"));
        let result = link_dir(src_dir.path(), dst_dir.path(), &options);

        assert!(result.is_ok());
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("RECORD")).unwrap(),
            "record content"
        );
    }

    #[test]
    fn test_copy_mutable_copy_filter_ignored() {
        // For copy mode, all files are already mutable, so filter is ignored
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());
        fs_err::write(src_dir.path().join("RECORD"), "record content").unwrap();

        let options = LinkOptions::new(LinkMode::Copy)
            .with_mutable_copy_filter(|p: &Path| p.ends_with("RECORD"));
        let result = link_dir(src_dir.path(), dst_dir.path(), &options);

        assert!(result.is_ok());
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("RECORD")).unwrap(),
            "record content"
        );
    }

    #[test]
    fn test_special_characters_in_filenames() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create files with special characters (that are valid on most filesystems)
        fs_err::write(src_dir.path().join("file with spaces.txt"), "spaces").unwrap();
        fs_err::write(src_dir.path().join("file-with-dashes.txt"), "dashes").unwrap();
        fs_err::write(
            src_dir.path().join("file_with_underscores.txt"),
            "underscores",
        )
        .unwrap();
        fs_err::write(src_dir.path().join("file.multiple.dots.txt"), "dots").unwrap();

        let options = LinkOptions::new(LinkMode::Copy);
        link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("file with spaces.txt")).unwrap(),
            "spaces"
        );
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("file-with-dashes.txt")).unwrap(),
            "dashes"
        );
    }

    #[test]
    fn test_hidden_files() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create hidden files (dotfiles)
        fs_err::write(src_dir.path().join(".hidden"), "hidden content").unwrap();
        fs_err::write(src_dir.path().join(".gitignore"), "*.pyc").unwrap();
        fs_err::create_dir_all(src_dir.path().join(".hidden_dir")).unwrap();
        fs_err::write(src_dir.path().join(".hidden_dir/file.txt"), "nested hidden").unwrap();

        let options = LinkOptions::new(LinkMode::Copy);
        link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join(".hidden")).unwrap(),
            "hidden content"
        );
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join(".gitignore")).unwrap(),
            "*.pyc"
        );
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join(".hidden_dir/file.txt")).unwrap(),
            "nested hidden"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_macos_clone_directory_recursive() {
        // Test the macOS-specific directory cloning via clonefile
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        // On macOS with APFS, this should use clonefile for entire directories
        let options = LinkOptions::new(LinkMode::Clone);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        // On APFS, should succeed with Clone mode
        assert_eq!(result, LinkMode::Clone);
        verify_test_tree(dst_dir.path());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_macos_clone_dir_merge_nested() {
        // Test the macOS clone_dir_merge with nested directory structure
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create nested structure in source
        fs_err::create_dir_all(src_dir.path().join("a/b/c")).unwrap();
        fs_err::write(src_dir.path().join("a/file1.txt"), "a1").unwrap();
        fs_err::write(src_dir.path().join("a/b/file2.txt"), "b2").unwrap();
        fs_err::write(src_dir.path().join("a/b/c/file3.txt"), "c3").unwrap();

        // Pre-create partial destination structure to force merge
        fs_err::create_dir_all(dst_dir.path().join("a/b")).unwrap();
        fs_err::write(dst_dir.path().join("a/existing.txt"), "existing").unwrap();

        let options = LinkOptions::new(LinkMode::Clone)
            .with_on_existing_directory(OnExistingDirectory::Merge);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        assert_eq!(result, LinkMode::Clone);

        // Source files should be cloned
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("a/file1.txt")).unwrap(),
            "a1"
        );
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("a/b/file2.txt")).unwrap(),
            "b2"
        );
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("a/b/c/file3.txt")).unwrap(),
            "c3"
        );
        // Existing file should remain
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("a/existing.txt")).unwrap(),
            "existing"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_macos_clone_merge_overwrites_files() {
        // Test that clone merge properly overwrites existing files on macOS
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        fs_err::write(src_dir.path().join("file.txt"), "new content").unwrap();

        // Create existing file with different content
        fs_err::write(dst_dir.path().join("file.txt"), "old content").unwrap();

        let options = LinkOptions::new(LinkMode::Clone)
            .with_on_existing_directory(OnExistingDirectory::Merge);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        assert_eq!(result, LinkMode::Clone);
        assert_eq!(
            fs_err::read_to_string(dst_dir.path().join("file.txt")).unwrap(),
            "new content"
        );
    }

    #[test]
    fn test_clone_fail_mode_on_existing() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        // Pre-create destination with existing file
        fs_err::write(dst_dir.path().join("file1.txt"), "existing").unwrap();

        let options =
            LinkOptions::new(LinkMode::Clone).with_on_existing_directory(OnExistingDirectory::Fail);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options);

        // Clone in Fail mode should error when destination exists
        // (may fall back to hardlink which also fails, or to copy which overwrites)
        // The behavior depends on filesystem support
        if result.is_ok() {
            // If it succeeded, it fell back to copy which overwrites
            assert_eq!(
                fs_err::read_to_string(dst_dir.path().join("file1.txt")).unwrap(),
                "content1"
            );
        }
        // If it failed, that's the expected Fail mode behavior
    }

    #[test]
    #[cfg(unix)]
    fn test_symlink_fail_mode_on_existing() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        // Pre-create destination with existing file
        fs_err::write(dst_dir.path().join("file1.txt"), "existing").unwrap();

        let options = LinkOptions::new(LinkMode::Symlink)
            .with_on_existing_directory(OnExistingDirectory::Fail);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options);

        // Symlink in Fail mode should error or fall back to copy
        if result.is_ok() {
            // Fell back to copy which overwrites
            assert_eq!(
                fs_err::read_to_string(dst_dir.path().join("file1.txt")).unwrap(),
                "content1"
            );
        }
    }

    #[test]
    fn test_clone_fallback_when_reflink_unsupported() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        if reflink_supported(src_dir.path()) {
            eprintln!("Skipping test: reflink is supported on this filesystem");
            return;
        }

        create_test_tree(src_dir.path());

        let options = LinkOptions::new(LinkMode::Clone);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        // When reflink is not supported, should fall back to hardlink or copy
        assert!(
            result == LinkMode::Hardlink || result == LinkMode::Copy,
            "Expected fallback to Hardlink or Copy, got {result:?}"
        );
        verify_test_tree(dst_dir.path());
    }

    #[test]
    #[cfg(windows)]
    fn test_windows_symlink_file_vs_dir() {
        // Test that Windows correctly uses symlink_file for files and symlink_dir for directories
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create a file and a directory
        fs_err::write(src_dir.path().join("file.txt"), "content").unwrap();
        fs_err::create_dir_all(src_dir.path().join("subdir")).unwrap();
        fs_err::write(src_dir.path().join("subdir/nested.txt"), "nested").unwrap();

        let options = LinkOptions::new(LinkMode::Symlink);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options);

        // Symlinks may require elevated permissions on Windows
        if let Ok(mode) = result {
            if mode == LinkMode::Symlink {
                // Verify the files are accessible through symlinks
                assert_eq!(
                    fs_err::read_to_string(dst_dir.path().join("file.txt")).unwrap(),
                    "content"
                );
                assert_eq!(
                    fs_err::read_to_string(dst_dir.path().join("subdir/nested.txt")).unwrap(),
                    "nested"
                );
            }
        }
        // If symlink failed (permissions), that's expected on Windows without elevation
    }

    #[test]
    fn test_link_state_new() {
        let state = LinkState::new(LinkMode::Clone);
        assert_eq!(state.mode, LinkMode::Clone);
        assert_eq!(state.attempt, LinkAttempt::Initial);
    }

    #[test]
    fn test_link_state_mode_working() {
        let state = LinkState::new(LinkMode::Hardlink).mode_working();
        assert_eq!(state.mode, LinkMode::Hardlink);
        assert_eq!(state.attempt, LinkAttempt::Subsequent);
    }

    #[test]
    fn test_link_state_next_mode_clone_to_hardlink() {
        let state = LinkState::new(LinkMode::Clone).next_mode();
        assert_eq!(state.mode, LinkMode::Hardlink);
        assert_eq!(state.attempt, LinkAttempt::Initial);
    }

    #[test]
    fn test_link_state_next_mode_hardlink_to_copy() {
        let state = LinkState::new(LinkMode::Hardlink).next_mode();
        assert_eq!(state.mode, LinkMode::Copy);
        assert_eq!(state.attempt, LinkAttempt::Initial);
    }

    #[test]
    fn test_link_state_next_mode_symlink_to_copy() {
        let state = LinkState::new(LinkMode::Symlink).next_mode();
        assert_eq!(state.mode, LinkMode::Copy);
        assert_eq!(state.attempt, LinkAttempt::Initial);
    }

    #[test]
    fn test_link_state_full_fallback_chain() {
        // Clone → Hardlink → Copy
        let state = LinkState::new(LinkMode::Clone);
        let state = state.next_mode();
        assert_eq!(state.mode, LinkMode::Hardlink);
        let state = state.next_mode();
        assert_eq!(state.mode, LinkMode::Copy);
    }

    #[test]
    fn test_link_state_mode_working_resets_on_next_mode() {
        // Confirming a mode as working, then transitioning resets to Initial
        let state = LinkState::new(LinkMode::Clone).mode_working();
        assert_eq!(state.attempt, LinkAttempt::Subsequent);
        let state = state.next_mode();
        assert_eq!(state.mode, LinkMode::Hardlink);
        assert_eq!(state.attempt, LinkAttempt::Initial);
    }

    #[test]
    fn test_hardlink_merge_confirms_mode_working() {
        // When hardlink succeeds through the atomic overwrite path during merge,
        // the mode should be confirmed as working
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        // Pre-create all destination files so every hardlink hits AlreadyExists
        fs_err::create_dir_all(dst_dir.path().join("subdir")).unwrap();
        fs_err::write(dst_dir.path().join("file1.txt"), "old1").unwrap();
        fs_err::write(dst_dir.path().join("file2.txt"), "old2").unwrap();
        fs_err::write(dst_dir.path().join("subdir/nested.txt"), "old nested").unwrap();

        let options = LinkOptions::new(LinkMode::Hardlink)
            .with_on_existing_directory(OnExistingDirectory::Merge);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        // Should succeed (hardlink or copy fallback)
        assert!(result == LinkMode::Hardlink || result == LinkMode::Copy);
        verify_test_tree(dst_dir.path());
    }

    #[test]
    fn test_clone_mode_returns_hardlink_or_copy_when_reflink_unsupported() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        create_test_tree(src_dir.path());

        if reflink_supported(src_dir.path()) {
            eprintln!("Skipping test: reflink is supported on this filesystem");
            return;
        }

        // When reflink is not supported, clone mode should fall back through
        // hardlink before reaching copy
        let options = LinkOptions::new(LinkMode::Clone);
        let result = link_dir(src_dir.path(), dst_dir.path(), &options).unwrap();

        assert!(
            result == LinkMode::Hardlink || result == LinkMode::Copy,
            "Expected Hardlink or Copy fallback, got {result:?}"
        );
        verify_test_tree(dst_dir.path());
    }
}
