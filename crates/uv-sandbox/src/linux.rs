//! Linux sandboxing using user namespaces, mount namespaces, and network namespaces.
//!
//! Applied inside a `pre_exec` hook (after fork, before exec), so the parent
//! process is never affected.
//!
//! ## Strategy
//!
//! 1. `unshare(CLONE_NEWUSER | CLONE_NEWNS | CLONE_NEWNET?)` — create new
//!    user, mount, and optionally network namespaces.
//! 2. Map the current UID/GID to root inside the user namespace so we have
//!    permission to manipulate mounts.
//! 3. Create a tmpfs at a temporary location as the new root.
//! 4. Bind-mount each allowed path into the new root with appropriate flags
//!    (read-only, noexec, etc.).
//! 5. `pivot_root` to the new root and unmount the old one.
//! 6. `setsid` to create a new session (prevents signal injection from
//!    outside the sandbox).
//! 7. `set_pdeathsig(SIGKILL)` so the sandboxed process is killed if its
//!    parent dies (prevents orphaned sandbox processes).
//! 8. `set_no_new_privs(true)` to prevent privilege escalation via suid.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::ffi::CString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::sched::{CloneFlags, unshare};
use nix::sys::prctl::{set_no_new_privs, set_pdeathsig};
use nix::sys::signal::Signal;
use nix::unistd::{getegid, geteuid, pivot_root, setsid};

use crate::spec::SandboxSpec;

/// New root path inside the mount namespace.
const NEW_ROOT: &str = "/tmp/.uv-sandbox-root";

/// Build the Linux sandbox in the current (forked child) process.
///
/// # Safety
///
/// Must be called from a single-threaded context (after `fork`, before `exec`).
pub fn apply_sandbox(spec: &SandboxSpec) -> io::Result<()> {
    let euid = geteuid();
    let egid = getegid();

    // Collect symlink overrides *before* entering namespaces (we need full
    // filesystem access to resolve symlink targets). This detects symlinks
    // inside writable directories that point into denied paths — a common
    // sandbox escape technique.
    let all_deny: Vec<PathBuf> = spec
        .deny_read
        .iter()
        .chain(spec.deny_write.iter())
        .chain(spec.deny_execute.iter())
        .cloned()
        .collect();
    let symlink_overrides = find_symlink_escapes(&spec.allow_write, &all_deny);

    // Enter new user + mount namespaces. Optionally enter a new network namespace.
    let mut ns_flags = CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWNS;
    if !spec.allow_net {
        ns_flags |= CloneFlags::CLONE_NEWNET;
    }
    unshare(ns_flags).map_err(nix_to_io)?;

    // Map our UID/GID to 0 inside the user namespace so we can manipulate mounts.
    map_ids(euid.as_raw(), egid.as_raw())?;

    // Build the set of bind mounts from the spec.
    let bind_mounts = build_bind_mounts(spec)?;

    // Collect deny paths with their restriction kind for overlay enforcement.
    // deny-read paths are fully hidden (empty overlay). deny-write and
    // deny-execute paths that are NOT also deny-read are made read-only
    // (preserving content visibility).
    let deny_read_set: HashSet<PathBuf> = spec
        .deny_read
        .iter()
        .map(|p| dunce::canonicalize(p).unwrap_or_else(|_| p.clone()))
        .collect();

    let mut deny_entries: Vec<DenyEntry> = Vec::new();

    for p in &spec.deny_read {
        let canonical = dunce::canonicalize(p).unwrap_or_else(|_| p.clone());
        deny_entries.push(DenyEntry {
            path: canonical,
            kind: DenyKind::HideContent,
        });
    }
    for p in spec.deny_write.iter().chain(spec.deny_execute.iter()) {
        let canonical = dunce::canonicalize(p).unwrap_or_else(|_| p.clone());
        if !deny_read_set.contains(&canonical) {
            deny_entries.push(DenyEntry {
                path: canonical,
                kind: DenyKind::ReadOnly,
            });
        }
    }

    // Deduplicate by path, keeping the stricter kind (HideContent wins over
    // ReadOnly). We sort by path for grouping, then use `dedup_by` which
    // keeps `b` (the first element) and removes `a` (the later duplicate).
    // Before removing `a`, we upgrade `b` to HideContent if either entry
    // requires it, so the result is always the strictest restriction.
    deny_entries.sort_by(|a, b| a.path.cmp(&b.path));
    deny_entries.dedup_by(|a, b| {
        if a.path == b.path {
            // Upgrade to HideContent if either entry requires it.
            if a.kind == DenyKind::HideContent {
                b.kind = DenyKind::HideContent;
            }
            true
        } else {
            false
        }
    });

    // Set up the new filesystem root with only the allowed paths visible.
    setup_mount_namespace(&bind_mounts, &symlink_overrides, &deny_entries)?;

    // Create a new session so processes outside the sandbox cannot send
    // signals to the sandboxed process (mirrors bwrap's `--new-session`).
    setsid().map_err(nix_to_io)?;

    // Kill the sandboxed process if its parent (uv) dies. This prevents
    // orphaned sandbox processes from continuing to run unsupervised
    // (mirrors bwrap's `--die-with-parent`).
    set_pdeathsig(Signal::SIGKILL).map_err(nix_to_io)?;

    // Prevent suid/sgid escalation.
    set_no_new_privs().map_err(nix_to_io)?;

    Ok(())
}

/// Convert a `nix::Error` (which is `Errno`) to `io::Error`.
fn nix_to_io(e: nix::Error) -> io::Error {
    io::Error::from_raw_os_error(e as i32)
}

/// A single bind mount with permission flags.
struct BindMount {
    /// Source path on the real filesystem (canonicalized).
    source: PathBuf,
    /// Mount attribute flags (RDONLY, NOEXEC, NOSUID).
    flags: u64,
}

/// Mount attribute flags for `mount_setattr`.
mod mount_attr {
    pub const RDONLY: u64 = 0x0000_0001;
    pub const NOSUID: u64 = 0x0000_0002;
    pub const NOEXEC: u64 = 0x0000_0008;
}

/// How a deny path should be enforced in the mount namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DenyKind {
    /// Completely hide the content (empty overlay). Used for `deny-read` paths.
    HideContent,
    /// Make the path read-only but preserve content. Used for `deny-write`
    /// and `deny-execute` paths that are not also denied for reading.
    ReadOnly,
}

/// A deny path with its enforcement kind.
#[derive(Debug, Clone)]
struct DenyEntry {
    path: PathBuf,
    kind: DenyKind,
}

/// Find symlinks within writable directories that resolve to denied paths.
///
/// Returns a list of symlink paths that should be bind-mounted over with
/// `/dev/null` to prevent sandbox escapes. A sandboxed process with write
/// access to a directory could replace a regular file with a symlink pointing
/// to a protected path, then read/write through the symlink. By detecting
/// existing symlinks before entering the sandbox, we can block this vector.
///
/// This mirrors Claude Code's `vPD` function which walks deny-path components
/// looking for symlinks within writable directories.
fn find_symlink_escapes(writable_paths: &[PathBuf], deny_paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut overrides = Vec::new();

    for deny_path in deny_paths {
        let deny_canonical = dunce::canonicalize(deny_path).unwrap_or_else(|_| deny_path.clone());

        // Walk each component of the deny path looking for symlinks.
        let mut current = PathBuf::new();
        for component in deny_path.components() {
            current.push(component);

            // Only check if this intermediate path is a symlink.
            let Ok(metadata) = current.symlink_metadata() else {
                break;
            };

            if !metadata.file_type().is_symlink() {
                continue;
            }

            // Check if this symlink lives inside a writable directory.
            let is_in_writable = writable_paths.iter().any(|w| {
                let w_canonical = dunce::canonicalize(w).unwrap_or_else(|_| w.clone());
                current.starts_with(&w_canonical)
            });

            if !is_in_writable {
                continue;
            }

            // This symlink is inside a writable dir. Check if its target
            // resolves to something within a denied path.
            if let Ok(target) = dunce::canonicalize(&current) {
                if target.starts_with(&deny_canonical) || deny_canonical.starts_with(&target) {
                    overrides.push(current.clone());
                }
            }
        }
    }

    overrides.sort();
    overrides.dedup();
    overrides
}

/// Check whether a path is denied by any entry in a deny list.
///
/// Both the input path and deny list entries should already be canonicalized.
/// A path is denied if it starts with (i.e., is equal to or is a child of) any
/// denied path.
fn is_denied(canonical: &Path, deny_list: &[PathBuf]) -> bool {
    deny_list.iter().any(|d| canonical.starts_with(d))
}

/// Canonicalize each path in a list, falling back to the original on error.
fn canonicalize_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .map(|p| dunce::canonicalize(p).unwrap_or_else(|_| p.clone()))
        .collect()
}

/// Build the list of bind mounts from a [`SandboxSpec`].
///
/// Each allowed path gets a bind mount with appropriate permission flags.
/// Deny paths are handled by simply not mounting them (they won't exist in the
/// new root).
fn build_bind_mounts(spec: &SandboxSpec) -> io::Result<Vec<BindMount>> {
    let mut mounts: Vec<BindMount> = Vec::new();

    // Helper: add or update a mount for a canonical path.
    let mut add_mount = |path: &Path, readonly: bool, noexec: bool, deny_list: &[PathBuf]| {
        // Canonicalize to resolve symlinks. Fall back to the original if it
        // doesn't exist yet.
        let canonical = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

        // Check if this path is denied by the relevant deny list.
        if is_denied(&canonical, deny_list) {
            return;
        }

        // Look for an existing mount for this path.
        if let Some(existing) = mounts.iter_mut().find(|m| m.source == canonical) {
            // Widen permissions: remove RDONLY if write is requested, remove
            // NOEXEC if execute is requested.
            if !readonly {
                existing.flags &= !mount_attr::RDONLY;
            }
            if !noexec {
                existing.flags &= !mount_attr::NOEXEC;
            }
        } else {
            let mut flags = mount_attr::NOSUID;
            if readonly {
                flags |= mount_attr::RDONLY;
            }
            if noexec {
                flags |= mount_attr::NOEXEC;
            }
            mounts.push(BindMount {
                source: canonical,
                flags,
            });
        }
    };

    // Pre-compute and canonicalize combined deny lists to avoid
    // re-allocating and re-canonicalizing per path.
    let read_deny = canonicalize_paths(&spec.deny_read);
    let write_deny: Vec<PathBuf> = canonicalize_paths(
        &spec
            .deny_read
            .iter()
            .chain(spec.deny_write.iter())
            .cloned()
            .collect::<Vec<_>>(),
    );
    let execute_deny: Vec<PathBuf> = canonicalize_paths(
        &spec
            .deny_read
            .iter()
            .chain(spec.deny_execute.iter())
            .cloned()
            .collect::<Vec<_>>(),
    );

    // Read-only paths (denied by deny_read).
    for path in &spec.allow_read {
        add_mount(path, true, true, &read_deny);
    }

    // Writable paths (denied by deny_read or deny_write).
    for path in &spec.allow_write {
        add_mount(path, false, true, &write_deny);
    }

    // Executable paths (denied by deny_read or deny_execute).
    for path in &spec.allow_execute {
        add_mount(path, true, false, &execute_deny);
    }

    // Always mount /dev/null, /dev/urandom, /dev/random, /dev/tty for basic operation.
    // Note: /dev/stdout, /dev/stderr, /dev/stdin are symlinks to /proc/self/fd/*
    // and are available through the /proc mount, so we don't bind-mount them
    // (bind-mounting symlinks returns EINVAL).
    for dev_path in &[
        "/dev/null",
        "/dev/urandom",
        "/dev/random",
        "/dev/zero",
        "/dev/tty",
    ] {
        let p = Path::new(dev_path);
        if p.exists() {
            add_mount(p, false, false, &[]);
        }
    }

    // Always mount /proc (needed for Python's multiprocessing, os.getpid, etc.).
    // We'll handle /proc specially in setup_mount_namespace.

    // Sort by path depth so parents are mounted before children.
    mounts.sort_unstable_by(|a, b| {
        let a_depth = a.source.components().count();
        let b_depth = b.source.components().count();
        match a_depth.cmp(&b_depth) {
            Ordering::Equal => a.source.cmp(&b.source),
            ord => ord,
        }
    });

    Ok(mounts)
}

/// Set up the mount namespace: create a new root, bind mount allowed paths,
/// pivot_root, and unmount the old root.
///
/// `symlink_overrides` is a list of symlink paths (within writable dirs) that
/// should be bind-mounted over with `/dev/null` to prevent sandbox escapes.
fn setup_mount_namespace(
    bind_mounts: &[BindMount],
    symlink_overrides: &[PathBuf],
    deny_entries: &[DenyEntry],
) -> io::Result<()> {
    // Save the current working directory before pivot_root invalidates it.
    let saved_cwd = std::env::current_dir().ok();

    let new_root = PathBuf::from(NEW_ROOT);

    // Create the new root directory.
    fs::create_dir_all(&new_root)?;

    // Mount a tmpfs as the new root.
    mount_tmpfs(&new_root)?;

    // Create a staging directory for deny-file overlays. We use a separate
    // tmpfs with an empty regular file that we can bind-mount over denied
    // paths. Using a regular file (instead of /dev/null) ensures that
    // MOUNT_ATTR_RDONLY is enforced by the kernel (device nodes like
    // /dev/null bypass read-only mount restrictions).
    let deny_staging = new_root.join(".deny-staging");
    fs::create_dir_all(&deny_staging)?;
    mount_tmpfs(&deny_staging)?;
    let deny_empty_file = deny_staging.join("empty");
    fs::File::create(&deny_empty_file)?;
    // Make the staging tmpfs read-only so the overlay files can't be written.
    let deny_staging_c = path_to_cstring(&deny_staging)?;
    do_mount_setattr(
        &deny_staging_c,
        mount_attr::RDONLY | mount_attr::NOSUID | mount_attr::NOEXEC,
    )?;

    // For each bind-mounted directory, look for sibling symlinks in the
    // parent directory that point into the mounted path. These symlinks are
    // needed when other paths (like venv python) reference the mount target
    // via a symlink (e.g. cpython-3.12-* → cpython-3.12.12-*).
    let extra_symlinks = find_sibling_symlinks(bind_mounts);

    // Bind-mount /proc from the host into the new root. We use a bind
    // mount rather than mounting a fresh procfs because mounting procfs
    // requires CLONE_NEWPID (a new PID namespace), which is incompatible
    // with the pre_exec hook (the child would need to fork again to become
    // PID 1 in the new PID namespace). A read-only bind mount of the
    // host's /proc is sufficient for Python and most programs.
    //
    // The mount is made read-only via mount_setattr to prevent the
    // sandboxed process from writing to /proc/self/* paths (e.g.,
    // oom_score_adj, environment).
    let new_proc = new_root.join("proc");
    fs::create_dir_all(&new_proc)?;
    do_bind_mount(Path::new("/proc"), &new_proc)?;
    let proc_c = path_to_cstring(&new_proc)?;
    do_mount_setattr(&proc_c, mount_attr::RDONLY | mount_attr::NOSUID)?;

    let proc_prefix = Path::new("/proc");

    // Bind mount each allowed path into the new root.
    // We do this in two passes: first create all bind mounts, then apply
    // mount_setattr to each one. This ensures the mount point exists
    // before we restrict its attributes.
    let mut mount_targets: Vec<(PathBuf, u64)> = Vec::new();

    for m in bind_mounts {
        // Skip paths under /proc — they are already available via the
        // bind mount above, and mounting on top would conflict.
        if m.source.starts_with(proc_prefix) {
            continue;
        }

        // Destination under the new root.
        let relative = m.source.strip_prefix("/").unwrap_or(&m.source);
        let dst = new_root.join(relative);

        // Replicate the directory structure.
        copy_tree(&m.source, &new_root)?;

        // Bind mount with full permissions first.
        do_bind_mount(&m.source, &dst)?;

        mount_targets.push((dst, m.flags | mount_attr::NOSUID));
    }

    // Apply mount_setattr to each mount point individually.
    for (dst, flags) in &mount_targets {
        let dst_c = path_to_cstring(dst)?;
        do_mount_setattr(&dst_c, *flags)?;
    }

    // Recreate sibling symlinks that point into mounted directories.
    for (link_path, target) in &extra_symlinks {
        let relative = link_path.strip_prefix("/").unwrap_or(link_path);
        let dst = new_root.join(relative);
        if dst.symlink_metadata().is_ok() {
            // Already exists — skip.
            continue;
        }
        // Ensure parent directory exists.
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        std::os::unix::fs::symlink(target, &dst)?;
    }

    // Block symlinks in writable directories that point to denied paths.
    // Bind-mount /dev/null over each one so the sandboxed process cannot
    // follow them to escape the sandbox.
    for symlink_path in symlink_overrides {
        let relative = symlink_path.strip_prefix("/").unwrap_or(symlink_path);
        let dst = new_root.join(relative);
        if dst.symlink_metadata().is_ok() {
            do_bind_mount(&deny_empty_file, &dst)?;
            let dst_c = path_to_cstring(&dst)?;
            do_mount_setattr(&dst_c, mount_attr::RDONLY | mount_attr::NOSUID)?;
        }
    }

    // Recreate top-level symlinks (e.g. /bin → usr/bin, /lib → usr/lib,
    // /lib64 → usr/lib, /sbin → usr/bin) that many systems use. Without
    // these, the ELF dynamic linker (/lib64/ld-linux-x86-64.so.2) and
    // other fundamental paths won't resolve in the new root.
    recreate_root_symlinks(&new_root)?;

    // Collect the source paths of all bind mounts so we can check whether
    // a deny path falls within an actually-mounted subtree. Deny paths
    // outside any allowed mount are already inaccessible and should NOT be
    // created — doing so would leak their existence on the tmpfs (e.g.,
    // creating ~/.ssh as a file makes `os.listdir(home)` reveal entries).
    let mounted_sources: Vec<&Path> = bind_mounts.iter().map(|m| m.source.as_path()).collect();

    // Enforce deny paths by overlaying them with restricted mounts.
    //
    // - `HideContent` (deny-read): directories get an empty tmpfs, files get
    //   the staging empty file. This completely hides the original content.
    // - `ReadOnly` (deny-write/deny-execute without deny-read): the original
    //   content is bind-mounted back read-only, preserving visibility while
    //   blocking writes.
    //
    // We use a regular file instead of /dev/null for file overlays because
    // device nodes bypass MOUNT_ATTR_RDONLY restrictions.
    for entry in deny_entries {
        let deny_path = &entry.path;

        // Only overlay if the deny path is within an actually-mounted
        // subtree. Paths outside any allowed mount are already inaccessible
        // because only mounted paths exist in the new root filesystem.
        let within_mount = mounted_sources.iter().any(|src| deny_path.starts_with(src));
        if !within_mount {
            continue;
        }

        let relative = deny_path.strip_prefix("/").unwrap_or(deny_path);
        let dst = new_root.join(relative);

        // Check if parent exists in the new root.
        let parent_exists = dst.parent().is_some_and(|p| p.exists());
        if !parent_exists {
            continue;
        }

        match entry.kind {
            DenyKind::HideContent => {
                // Completely hide the content with an empty overlay.
                apply_hide_overlay(&dst, deny_path, &deny_empty_file)?;
            }
            DenyKind::ReadOnly => {
                // Make the path read-only but preserve content visibility.
                apply_readonly_overlay(&dst, deny_path)?;
            }
        }
    }

    // Pivot root.
    pivot_root(&new_root, &new_root).map_err(nix_to_io)?;

    // Unmount old root (which is now stacked on /).
    umount2("/", MntFlags::MNT_DETACH).map_err(nix_to_io)?;

    // Prevent child mount namespaces from seeing our mounts.
    deny_mount_propagation()?;

    // Restore the working directory. After pivot_root the old cwd handle is
    // invalid, so we use the path we saved earlier (which should still be
    // valid since the project directory is bind-mounted in the new root).
    if let Some(cwd) = saved_cwd {
        if std::env::set_current_dir(&cwd).is_err() {
            let _ = std::env::set_current_dir("/");
        }
    } else {
        let _ = std::env::set_current_dir("/");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Low-level helpers
// ---------------------------------------------------------------------------

/// Map the parent UID/GID to root (0) inside the user namespace.
fn map_ids(parent_uid: u32, parent_gid: u32) -> io::Result<()> {
    fs::write("/proc/self/uid_map", format!("0 {parent_uid} 1\n"))?;
    fs::write("/proc/self/setgroups", b"deny")?;
    fs::write("/proc/self/gid_map", format!("0 {parent_gid} 1\n"))?;
    Ok(())
}

fn mount_tmpfs(dst: &Path) -> io::Result<()> {
    mount(
        None::<&str>,
        dst,
        Some("tmpfs"),
        MsFlags::empty(),
        None::<&str>,
    )
    .map_err(nix_to_io)
}

fn do_bind_mount(src: &Path, dst: &Path) -> io::Result<()> {
    mount(
        Some(src),
        dst,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None::<&str>,
    )
    .map_err(nix_to_io)
}

/// Parameter for the `mount_setattr` syscall.
#[repr(C)]
#[derive(Default)]
struct MountAttrParam {
    attr_set: u64,
    attr_clr: u64,
    propagation: u64,
    userns_fd: u64,
}

/// Call `mount_setattr(2)` via raw syscall (not yet wrapped by nix or rustix).
///
/// Does NOT use `AT_RECURSIVE` so that child mount points retain their own
/// flags independently of parent mount restrictions.
fn do_mount_setattr(mount_path: &CString, flags: u64) -> io::Result<()> {
    let attrs = MountAttrParam {
        attr_set: flags,
        ..Default::default()
    };
    let result = unsafe {
        nix::libc::syscall(
            nix::libc::SYS_mount_setattr,
            nix::libc::AT_FDCWD,
            mount_path.as_ptr(),
            0u32, // no AT_RECURSIVE — each mount gets its own flags
            &attrs as *const _ as *const nix::libc::c_void,
            std::mem::size_of::<MountAttrParam>(),
        )
    };
    if result == 0 {
        return Ok(());
    }

    let err = io::Error::last_os_error();
    if err.raw_os_error() == Some(nix::libc::ENOSYS) {
        // Older kernels may not support `mount_setattr(2)`. Fall back to a
        // bind-remount with equivalent MS_* flags so sandbox setup still works.
        let mount_path = Path::new(std::ffi::OsStr::from_bytes(mount_path.as_bytes()));
        return mount(
            None::<&str>,
            mount_path,
            None::<&str>,
            attr_to_remount_flags(flags),
            None::<&str>,
        )
        .map_err(nix_to_io);
    }

    Err(err)
}

/// Convert `mount_setattr` attribute flags to bind-remount `MsFlags`.
fn attr_to_remount_flags(flags: u64) -> MsFlags {
    let mut remount_flags = MsFlags::MS_BIND | MsFlags::MS_REMOUNT;

    if flags & mount_attr::RDONLY != 0 {
        remount_flags |= MsFlags::MS_RDONLY;
    }
    if flags & mount_attr::NOSUID != 0 {
        remount_flags |= MsFlags::MS_NOSUID;
    }
    if flags & mount_attr::NOEXEC != 0 {
        remount_flags |= MsFlags::MS_NOEXEC;
    }

    remount_flags
}

fn deny_mount_propagation() -> io::Result<()> {
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_PRIVATE | MsFlags::MS_REC,
        None::<&str>,
    )
    .map_err(nix_to_io)
}

/// Apply a `HideContent` deny overlay: completely hide the path's content.
///
/// Directories get an empty tmpfs. Files get a bind-mount of the staging
/// empty file. Non-existent paths are created based on a host-filesystem
/// heuristic.
fn apply_hide_overlay(dst: &Path, deny_path: &Path, deny_empty_file: &Path) -> io::Result<()> {
    match dst.symlink_metadata() {
        Ok(meta) if meta.is_dir() => {
            mount_tmpfs(dst)?;
            let dst_c = path_to_cstring(dst)?;
            do_mount_setattr(
                &dst_c,
                mount_attr::RDONLY | mount_attr::NOSUID | mount_attr::NOEXEC,
            )?;
        }
        Ok(_) => {
            do_bind_mount(deny_empty_file, dst)?;
            let dst_c = path_to_cstring(dst)?;
            do_mount_setattr(&dst_c, mount_attr::RDONLY | mount_attr::NOSUID)?;
        }
        Err(_) => {
            let is_dir = if deny_path.is_dir() {
                true
            } else if deny_path.is_file() {
                false
            } else {
                deny_path.extension().is_none()
            };

            if is_dir {
                if fs::create_dir(dst).is_ok() {
                    mount_tmpfs(dst)?;
                    let dst_c = path_to_cstring(dst)?;
                    do_mount_setattr(
                        &dst_c,
                        mount_attr::RDONLY | mount_attr::NOSUID | mount_attr::NOEXEC,
                    )?;
                }
            } else if fs::File::create(dst).is_ok() {
                do_bind_mount(deny_empty_file, dst)?;
                let dst_c = path_to_cstring(dst)?;
                do_mount_setattr(&dst_c, mount_attr::RDONLY | mount_attr::NOSUID)?;
            }
        }
    }
    Ok(())
}

/// Apply a `ReadOnly` deny overlay: preserve content but block writes.
///
/// Bind-mounts the existing content back on top of itself and marks it
/// read-only via `mount_setattr`. If the path does not exist in the new
/// root, this is a no-op (the path is already inaccessible).
fn apply_readonly_overlay(dst: &Path, deny_path: &Path) -> io::Result<()> {
    match dst.symlink_metadata() {
        Ok(meta) if meta.is_dir() => {
            // Bind-mount the directory on top of itself, then make it read-only.
            do_bind_mount(dst, dst)?;
            let dst_c = path_to_cstring(dst)?;
            do_mount_setattr(
                &dst_c,
                mount_attr::RDONLY | mount_attr::NOSUID | mount_attr::NOEXEC,
            )?;
        }
        Ok(_) => {
            // Bind-mount the file on top of itself, then make it read-only.
            do_bind_mount(dst, dst)?;
            let dst_c = path_to_cstring(dst)?;
            do_mount_setattr(&dst_c, mount_attr::RDONLY | mount_attr::NOSUID)?;
        }
        Err(_) => {
            // Path doesn't exist in the new root — nothing to protect.
            // For non-existent deny-write paths, create a read-only placeholder
            // so the path can't be created as writable.
            let is_dir = if deny_path.is_dir() {
                true
            } else if deny_path.is_file() {
                false
            } else {
                deny_path.extension().is_none()
            };

            if is_dir {
                if fs::create_dir(dst).is_ok() {
                    mount_tmpfs(dst)?;
                    let dst_c = path_to_cstring(dst)?;
                    do_mount_setattr(
                        &dst_c,
                        mount_attr::RDONLY | mount_attr::NOSUID | mount_attr::NOEXEC,
                    )?;
                }
            } else if fs::File::create(dst).is_ok() {
                do_bind_mount(dst, dst)?;
                let dst_c = path_to_cstring(dst)?;
                do_mount_setattr(&dst_c, mount_attr::RDONLY | mount_attr::NOSUID)?;
            }
        }
    }
    Ok(())
}

/// Replicate a directory tree's structure under a different root.
///
/// Creates empty directories (preserving permissions) for each component of
/// `src` under `dst_root`. If `src` is a file, creates an empty file.
fn copy_tree(src: &Path, dst_root: &Path) -> io::Result<()> {
    let mut dst = dst_root.to_path_buf();
    let mut src_prefix = PathBuf::new();

    for component in src.components() {
        if component == Component::RootDir {
            src_prefix.push(component);
            continue;
        }

        src_prefix.push(component);
        dst.push(component);

        if dst.exists() {
            continue;
        }

        let metadata = src_prefix.metadata()?;
        if metadata.is_dir() {
            fs::create_dir(&dst)?;
            // Set intermediate directories to execute-only (0o111) so that
            // path traversal works but listing is denied. This prevents
            // `os.listdir(home)` from succeeding on directories that are
            // only created as intermediates for deeper mount points.
            // The final mount point will get its own permissions via
            // bind mount + mount_setattr.
            fs::set_permissions(&dst, std::os::unix::fs::PermissionsExt::from_mode(0o111))?;
        } else {
            // Create an empty file as a mount target.
            fs::File::create(&dst)?;
        }
    }

    Ok(())
}

/// Recreate top-level symlinks in the new root filesystem.
///
/// Many Linux distributions use a "merged /usr" layout where `/bin`, `/lib`,
/// `/lib64`, and `/sbin` are symlinks to their `/usr/*` counterparts. Without
/// these symlinks, the ELF dynamic linker (e.g. `/lib64/ld-linux-x86-64.so.2`)
/// cannot be found, causing all dynamically linked binaries to fail with ENOENT.
fn recreate_root_symlinks(new_root: &Path) -> io::Result<()> {
    let root = Path::new("/");
    let entries = fs::read_dir(root)?;

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = path.symlink_metadata() else {
            continue;
        };
        if !meta.file_type().is_symlink() {
            continue;
        }

        let Ok(target) = fs::read_link(&path) else {
            continue;
        };

        let Some(name) = path.file_name() else {
            continue;
        };

        let dst = new_root.join(name);
        if dst.symlink_metadata().is_ok() {
            // Already exists — skip.
            continue;
        }

        // Recreate the symlink with the same target.
        std::os::unix::fs::symlink(&target, &dst)?;
    }

    Ok(())
}

/// For each bind-mounted path, scan its parent directory for symlinks that
/// point to (or into) the bind-mounted path. Returns a list of (symlink_path,
/// symlink_target) pairs that should be recreated in the new root.
///
/// This handles the common case where uv creates a versioned directory like
/// `cpython-3.12.12-linux-x86_64-gnu/` and also a convenience symlink
/// `cpython-3.12-linux-x86_64-gnu → cpython-3.12.12-linux-x86_64-gnu`.
/// The venv's python symlink may reference the convenience symlink, so it
/// must exist in the sandbox's new root.
fn find_sibling_symlinks(bind_mounts: &[BindMount]) -> Vec<(PathBuf, PathBuf)> {
    let mut symlinks = Vec::new();

    for m in bind_mounts {
        let Some(parent) = m.source.parent() else {
            continue;
        };

        // Read the parent directory and look for symlinks.
        let Ok(entries) = fs::read_dir(parent) else {
            continue;
        };

        let dir_name = m.source.file_name();

        for entry in entries.flatten() {
            let path = entry.path();

            // Only interested in symlinks.
            let Ok(meta) = path.symlink_metadata() else {
                continue;
            };
            if !meta.file_type().is_symlink() {
                continue;
            }

            // Check if the symlink target resolves to our bind-mounted path.
            let Ok(link_target) = fs::read_link(&path) else {
                continue;
            };

            // The symlink target may be relative. Check if it matches the
            // bind mount's directory name (common case: cpython-3.12-* → cpython-3.12.12-*).
            let target_matches = if link_target.is_absolute() {
                link_target == m.source || link_target.starts_with(&m.source)
            } else {
                // Relative symlink — check if the file name component matches.
                Some(link_target.as_os_str()) == dir_name.map(|n| n)
                    || dunce::canonicalize(&path)
                        .map(|c| c == m.source || c.starts_with(&m.source))
                        .unwrap_or(false)
            };

            if target_matches {
                symlinks.push((path, link_target));
            }
        }
    }

    symlinks
}

fn path_to_cstring(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains null byte"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attr_to_remount_flags_maps_mount_attributes() {
        let flags =
            attr_to_remount_flags(mount_attr::RDONLY | mount_attr::NOSUID | mount_attr::NOEXEC);

        assert!(flags.contains(MsFlags::MS_BIND));
        assert!(flags.contains(MsFlags::MS_REMOUNT));
        assert!(flags.contains(MsFlags::MS_RDONLY));
        assert!(flags.contains(MsFlags::MS_NOSUID));
        assert!(flags.contains(MsFlags::MS_NOEXEC));
    }
}
