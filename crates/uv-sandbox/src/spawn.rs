//! Spawn a sandboxed child process using OS-level isolation.
//!
//! Uses [`CommandExt::pre_exec`] to apply sandboxing in the forked child
//! process before it exec's the target command. This means the parent
//! uv process is never sandboxed.

use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

use crate::spec::SandboxSpec;

/// Apply sandbox restrictions to an existing [`Command`].
///
/// This installs a `pre_exec` hook that applies OS-level sandboxing
/// (Seatbelt on macOS, namespaces on Linux) in the child process after
/// fork but before exec.
///
/// Environment variable filtering is also applied via the Command API
/// (no unsafe env manipulation needed).
pub fn apply_sandbox(cmd: &mut Command, spec: &SandboxSpec) -> io::Result<()> {
    // Environment variable filtering: done on the parent side via Command API.
    if let Some(ref allowed_env) = spec.env {
        cmd.env_clear();
        for (key, value) in allowed_env {
            cmd.env(key, value);
        }
    }

    // Build platform-specific sandbox setup and install as pre_exec hook.
    install_pre_exec(cmd, spec)?;

    Ok(())
}

/// Install the platform-specific `pre_exec` hook on the command.
#[cfg(target_os = "macos")]
fn install_pre_exec(cmd: &mut Command, spec: &SandboxSpec) -> io::Result<()> {
    // Build the Seatbelt profile on the parent side (before fork).
    let profile = crate::macos::build_profile(spec)
        .map_err(|err| io::Error::other(format!("failed to build sandbox profile: {err}")))?;

    // SAFETY: The `pre_exec` closure runs in the forked child process,
    // which is single-threaded (between fork and exec). Calling
    // `sandbox_init` here is safe and applies the sandbox only to the
    // child that will become the target command.
    unsafe {
        cmd.pre_exec(move || {
            crate::macos::apply_profile(&profile)
                .map_err(|err| io::Error::other(format!("sandbox_init failed: {err}")))
        });
    }

    Ok(())
}

/// Install the platform-specific `pre_exec` hook on the command.
#[cfg(target_os = "linux")]
fn install_pre_exec(cmd: &mut Command, spec: &SandboxSpec) -> io::Result<()> {
    let spec = spec.clone();

    // SAFETY: The `pre_exec` closure runs in the forked child process,
    // which is single-threaded (between fork and exec). Calling
    // `unshare`, `mount`, and `pivot_root` here is safe and applies the
    // sandbox only to the child that will become the target command.
    unsafe {
        cmd.pre_exec(move || crate::linux::apply_sandbox(&spec));
    }

    Ok(())
}

/// Stub for unsupported platforms.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn install_pre_exec(cmd: &mut Command, spec: &SandboxSpec) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Sandboxing is not supported on this platform",
    ))
}
