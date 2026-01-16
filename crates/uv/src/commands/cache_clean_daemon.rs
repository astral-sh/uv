//! Background daemon for cache deletion.
//!
//! This module provides functionality to spawn a background process that deletes a directory.
//! The daemon is detached from the parent process, allowing the parent to exit immediately.

use std::path::Path;

use anyhow::{Context, Result};
use tracing::debug;

/// Spawn a background daemon process to delete the given directory.
///
/// The daemon is detached from the parent process, allowing the parent to exit immediately
/// while deletion continues in the background.
///
/// Re-invokes the current binary with the hidden `clean --background-daemon-target <dir>` flag.
pub(crate) fn spawn_background_clean(dir: &Path) -> Result<()> {
    let current_exe = std::env::current_exe().context("Failed to get current executable path")?;

    debug!("Spawning background daemon to delete: {}", dir.display());

    #[cfg(unix)]
    {
        spawn_unix_daemon(&current_exe, dir)?;
    }

    #[cfg(windows)]
    {
        spawn_windows_daemon(&current_exe, dir)?;
    }

    Ok(())
}

/// Spawn a daemon process on Unix systems.
///
/// Uses `setsid` to create a new session, detaching the process from the terminal.
#[cfg(unix)]
#[allow(unsafe_code)]
fn spawn_unix_daemon(exe: &Path, dir: &Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(exe);
    cmd.args(["clean", "--background-daemon-target"])
        .arg(dir)
        // Detach from parent's stdin/stdout/stderr
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Create a new session (setsid) to fully detach from the terminal.
    // SAFETY: `setsid` is async-signal-safe and can be called in a `pre_exec` hook.
    unsafe {
        cmd.pre_exec(|| {
            // Create a new session, making this process the session leader.
            // This detaches from the controlling terminal.
            nix::libc::setsid();
            Ok(())
        });
    }

    cmd.spawn().context("Failed to spawn background daemon")?;

    Ok(())
}

/// Spawn a daemon process on Windows systems.
///
/// Uses `DETACHED_PROCESS` to create a process that doesn't inherit the parent's console.
#[cfg(windows)]
fn spawn_windows_daemon(exe: &Path, dir: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    // DETACHED_PROCESS: The new process does not inherit the console of the parent.
    const DETACHED_PROCESS: u32 = 0x00000008;
    // CREATE_NO_WINDOW: The process does not have a console window.
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    Command::new(exe)
        .args(["clean", "--background-daemon-target"])
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
        .context("Failed to spawn background daemon")?;

    Ok(())
}

/// Entry point for the background daemon process.
///
/// This function is called when the daemon is invoked with `clean --background-daemon-target <dir>`.
/// It recursively deletes the specified directory and then exits.
pub(crate) fn run_background_clean(dir: &Path) -> Result<()> {
    debug!("Background daemon starting deletion of: {}", dir.display());

    if dir.exists() {
        fs_err::remove_dir_all(dir)
            .with_context(|| format!("Failed to remove directory: {}", dir.display()))?;
        debug!("Background daemon completed deletion of: {}", dir.display());
    } else {
        debug!(
            "Background daemon: directory does not exist: {}",
            dir.display()
        );
    }

    Ok(())
}
