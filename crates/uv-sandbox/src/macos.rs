//! macOS sandboxing using Seatbelt (`sandbox_init`).
//!
//! Generates an SBPL (Seatbelt Profile Language) profile from a [`SandboxSpec`]
//! and provides a closure suitable for [`std::os::unix::process::CommandExt::pre_exec`].

use std::ffi::CString;
use std::io::Write;
use std::path::Path;
use std::ptr;

use crate::spec::SandboxSpec;

/// Deny-all baseline with minimal required permissions.
///
/// `process-exec` is intentionally absent — it is controlled by the
/// `allow-execute` / `deny-execute` paths in the [`SandboxSpec`].
///
/// # IPC hardening notes
///
/// `mach*` and `ipc*` are broader than ideal. The highest-value tightening
/// would be replacing `(allow mach*)` with a specific `mach-lookup` allowlist,
/// blocking access to Keychain (`com.apple.SecurityServer`), pasteboard
/// (`com.apple.pbs`), and other system services that could be used for data
/// exfiltration via `pyobjc` or `ctypes`.
///
/// The difficulty is that the required Mach services are undocumented Apple
/// internals that vary by macOS version and fail with SIGABRT rather than a
/// clean error when missing. Tightening requires:
///
/// 1. A version-aware test matrix covering macOS 13–15+
/// 2. Sandbox violation monitoring to discover required services:
///    ```text
///    log stream --predicate 'subsystem == "com.apple.sandbox" AND process == "python3"'
///    ```
/// 3. Gradual rollout to catch edge cases in real Python packages
///
/// IPC could similarly be narrowed to `ipc-posix-shm` and `ipc-posix-sem`
/// (what Python's `multiprocessing` needs), and `system*` to `system-info` +
/// `distributed-notification-post`.
///
/// `sysctl-read` is allowed without an enumerated name allowlist. Python and
/// its ecosystem query a wide variety of sysctls and an incomplete list causes
/// silent failures. Read-only without an allowlist is a reasonable middle ground.
///
/// # Signal hardening notes
///
/// `(allow signal (target others))` permits the sandboxed process to send
/// signals to any process owned by the same user, not just its children.
/// Seatbelt only supports `(target self)` and `(target others)` — there is
/// no `(target children)` filter. Python's `subprocess.Popen.terminate()`,
/// `subprocess.Popen.kill()`, and `multiprocessing` worker management all
/// require signaling child processes, so `(target others)` is necessary.
/// On Linux this is mitigated by `setsid()` (new session = signals can't
/// reach processes outside the sandbox), but macOS has no equivalent.
///
/// # Metadata notes
///
/// `(allow file-read-metadata)` is global, meaning the sandboxed process
/// can `stat()`/`lstat()` any path including denied ones (e.g. `~/.ssh`).
/// File *contents* are still protected by `deny file-read*` rules, but
/// existence, size, permissions, and timestamps are visible. This is
/// required for Python's `os.path.exists()`, `importlib` path scanning,
/// and general filesystem navigation to work.
///
/// TODO(zanie): File a tracking issue for IPC/Mach/signal hardening.
const BASELINE_PROFILE: &[u8] = b"\
(version 1)
(import \"system.sb\")

(deny default)
(allow mach*)
(allow ipc*)
(allow signal (target others))
(allow process-fork)
(allow sysctl-read)
(allow system*)
(allow file-read-metadata)
";

/// Build a Seatbelt profile string from a [`SandboxSpec`].
///
/// This can be called on the parent side (before fork), and the resulting
/// string passed into a `pre_exec` closure.
pub fn build_profile(spec: &SandboxSpec) -> Result<CString, SandboxError> {
    let profile = generate_profile(spec)?;
    CString::new(profile).map_err(|_| SandboxError::Activation("profile contains null byte".into()))
}

/// Apply a pre-built Seatbelt profile to the current process.
///
/// # Safety
///
/// Must be called in a single-threaded context (e.g., inside `pre_exec`
/// after fork, before exec).
pub unsafe fn apply_profile(profile: &CString) -> Result<(), SandboxError> {
    let mut error: *mut i8 = ptr::null_mut();

    // SAFETY: `sandbox_init` is a stable macOS C API. We pass a valid C string
    // and a pointer to receive error messages. The `0` flags argument means
    // the profile is a string (not a file path).
    let result = unsafe { sandbox_init(profile.as_ptr(), 0, &mut error) };

    if result == 0 {
        Ok(())
    } else {
        let error_msg = if error.is_null() {
            "sandbox_init failed with unknown error".into()
        } else {
            // SAFETY: `error` is a valid C string allocated by sandbox_init.
            let msg = unsafe { std::ffi::CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned();
            // SAFETY: `error` was allocated by sandbox_init and must be freed.
            unsafe { sandbox_free_error(error) };
            msg
        };
        Err(SandboxError::Activation(error_msg))
    }
}

/// Generate a Seatbelt profile from a sandbox spec.
fn generate_profile(spec: &SandboxSpec) -> Result<Vec<u8>, SandboxError> {
    let mut profile = BASELINE_PROFILE.to_vec();

    // Always allow read/write/ioctl on standard device files and stdio.
    write_device_rules(&mut profile)?;

    // Filesystem: allow-read
    for path in &spec.allow_read {
        write_path_rule(&mut profile, "allow", "file-read*", path)?;
    }

    // Filesystem: allow-write (implies read)
    for path in &spec.allow_write {
        write_path_rule(&mut profile, "allow", "file-read*", path)?;
        write_path_rule(&mut profile, "allow", "file-write*", path)?;
    }

    // Filesystem: allow-execute (implies read)
    for path in &spec.allow_execute {
        write_path_rule(&mut profile, "allow", "file-read*", path)?;
        write_path_rule(&mut profile, "allow", "process-exec", path)?;
    }

    // Filesystem: deny-read (overrides allow).
    // Also deny unlink on the path and all parent directories to prevent
    // deletion/rename attacks that could circumvent the deny rule.
    for path in &spec.deny_read {
        write_path_rule(&mut profile, "deny", "file-read*", path)?;
        write_unlink_protection(&mut profile, path)?;
    }

    // Filesystem: deny-write (overrides allow).
    // Also deny unlink on the path and all parent directories.
    for path in &spec.deny_write {
        write_path_rule(&mut profile, "deny", "file-write*", path)?;
        write_unlink_protection(&mut profile, path)?;
    }

    // Filesystem: deny-execute (overrides allow)
    for path in &spec.deny_execute {
        write_path_rule(&mut profile, "deny", "process-exec", path)?;
    }

    // Network
    if spec.allow_net {
        // Enable full networking including DNS resolution via mDNSResponder.
        profile.write_all(b"(system-network)\n")?;
        profile.write_all(b"(allow network*)\n")?;
    } else {
        // When network is denied, allow only the minimal system-socket
        // operations that Python needs (e.g., AF_ROUTE for gethostname(2))
        // without enabling DNS resolution or actual network I/O.
        //
        // Notably, we do NOT call `(system-network)` here — that macro
        // grants access to `com.apple.dnssd.service` (mDNSResponder),
        // which would allow DNS-based data exfiltration even with all
        // other network access denied.
        profile.write_all(
            b"\
(allow system-socket
  (socket-domain AF_ROUTE))
",
        )?;
    }

    Ok(profile)
}

/// Write path rule(s) to the profile buffer.
///
/// Uses `subpath` for directories and `literal` for files. For `deny` rules,
/// emits both `subpath` and `literal` to be robust against TOCTOU races
/// (a path could change type between profile generation and sandbox_init).
/// For `allow` rules, we accept the minor TOCTOU risk because the consequence
/// is too-narrow access (which produces a clear permission error) rather than
/// a sandbox escape.
fn write_path_rule(
    buffer: &mut Vec<u8>,
    mode: &str,
    access_type: &str,
    path: &Path,
) -> Result<(), SandboxError> {
    let escaped = escape_path(path)?;

    if mode == "deny" {
        // For deny rules, emit both filters to be safe: if the path is a
        // directory we need `subpath` to deny the subtree, and if it's a
        // file we need `literal`. Emitting both is strictly more restrictive,
        // which is the safe direction for deny rules.
        writeln!(buffer, "({mode} {access_type} (subpath {escaped}))")?;
        writeln!(buffer, "({mode} {access_type} (literal {escaped}))")?;
    } else {
        // For allow rules, check the path type. Fall back to `subpath` for
        // non-existent paths (common for sandbox presets like temp dirs).
        let is_dir = dunce::canonicalize(path)
            .map(|p| p.is_dir())
            .unwrap_or(true);

        let filter = if is_dir { "subpath" } else { "literal" };
        writeln!(buffer, "({mode} {access_type} ({filter} {escaped}))")?;
    }
    Ok(())
}

/// Write rules allowing read, write, and ioctl on standard device files and stdio.
///
/// Without these, sandboxed processes cannot write to stdout/stderr, read from
/// `/dev/urandom`, or interact with the terminal.
///
/// Only devices that genuinely need writes (null, zero, stdio, tty) get
/// `file-write*`. Read-only devices (urandom, random) and devices the sandbox
/// has no reason to write to (dtracehelper, autofs_nowait) are read-only.
fn write_device_rules(buffer: &mut Vec<u8>) -> Result<(), SandboxError> {
    buffer.write_all(
        b"\
; Standard device files -- always accessible.
; Devices that need write access (stdio, null, zero, tty):
(allow file-read* file-write*
  (literal \"/dev/null\")
  (literal \"/dev/zero\")
  (literal \"/dev/stdout\")
  (literal \"/dev/stderr\")
  (literal \"/dev/stdin\")
  (literal \"/dev/tty\"))

; Read-only devices (RNG, DTrace helper, autofs):
(allow file-read*
  (literal \"/dev/random\")
  (literal \"/dev/urandom\")
  (literal \"/dev/dtracehelper\")
  (literal \"/dev/autofs_nowait\"))

(allow file-ioctl
  (literal \"/dev/null\")
  (literal \"/dev/zero\")
  (literal \"/dev/random\")
  (literal \"/dev/urandom\")
  (literal \"/dev/tty\"))
",
    )?;
    Ok(())
}

/// Deny `file-write-unlink` on a path and every parent directory up to `/`.
///
/// This prevents an attacker from bypassing a deny rule by renaming or
/// unlinking a parent directory (e.g., `mv /home/user /home/user.bak`) and
/// recreating the tree without the protected path. This technique is used by
/// Claude Code's Seatbelt sandbox for the same reason.
fn write_unlink_protection(buffer: &mut Vec<u8>, path: &Path) -> Result<(), SandboxError> {
    // Canonicalize once, then walk parents without re-canonicalizing.
    let canonical = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut current = canonical.as_path();
    loop {
        let escaped = format_escaped_path(current)?;
        writeln!(buffer, "(deny file-write-unlink (literal {escaped}))")?;
        if current == Path::new("/") {
            break;
        }
        current = current.parent().unwrap_or(Path::new("/"));
    }
    Ok(())
}

/// Canonicalize a path and escape it for use in SBPL expressions.
///
/// The path is canonicalized to resolve symlinks (Seatbelt operates on
/// real paths), then escaped for SBPL string syntax.
fn escape_path(path: &Path) -> Result<String, SandboxError> {
    // Try to canonicalize; fall back to the original if the path doesn't exist yet.
    let canonical = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    format_escaped_path(&canonical)
}

/// Escape and quote an already-resolved path for use in SBPL expressions.
///
/// Unlike [`escape_path`], this does not canonicalize -- the caller is
/// responsible for providing an absolute, symlink-resolved path.
fn format_escaped_path(path: &Path) -> Result<String, SandboxError> {
    let path_str = path
        .as_os_str()
        .to_str()
        .ok_or_else(|| SandboxError::InvalidPath(path.to_path_buf()))?;

    // Strip trailing slashes (SBPL subpath requires no trailing slash).
    let trimmed = path_str.trim_end_matches('/');
    let trimmed = if trimmed.is_empty() { "/" } else { trimmed };

    // Reject paths containing characters that could break or inject into
    // the line-oriented SBPL profile syntax:
    // - newlines/carriage returns: would inject arbitrary sandbox rules
    // - NUL bytes: would truncate the C string passed to sandbox_init
    // - parentheses: SBPL expression delimiters
    // - semicolons: SBPL comment delimiter
    if trimmed
        .bytes()
        .any(|b| b == b'\n' || b == b'\r' || b == b'\0' || b == b'(' || b == b')' || b == b';')
    {
        return Err(SandboxError::InvalidPath(path.to_path_buf()));
    }

    // Escape special characters for SBPL string literals.
    let escaped = trimmed.replace('\\', r"\\").replace('"', r#"\""#);

    Ok(format!("\"{escaped}\""))
}

/// Errors from sandbox operations.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("failed to activate sandbox: {0}")]
    Activation(String),

    #[error("invalid path for sandbox: {}", _0.display())]
    InvalidPath(std::path::PathBuf),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

// FFI bindings to macOS sandbox API.
unsafe extern "C" {
    fn sandbox_init(profile: *const i8, flags: u64, errorbuf: *mut *mut i8) -> i32;
    fn sandbox_free_error(errorbuf: *mut i8);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_escape_path_simple() {
        // /tmp always exists on macOS.
        let result = escape_path(Path::new("/tmp")).unwrap();
        // /tmp -> /private/tmp after canonicalization on macOS
        assert!(
            result == "\"/private/tmp\"" || result == "\"/tmp\"",
            "got: {result}"
        );
    }

    #[test]
    fn test_escape_path_trailing_slash() {
        let result = escape_path(Path::new("/tmp/")).unwrap();
        // Should not end with /
        assert!(!result.trim_matches('"').ends_with('/') || result == "\"/\"");
    }

    #[test]
    fn test_generate_profile_basic() {
        let spec = SandboxSpec {
            allow_read: vec![PathBuf::from("/tmp")],
            deny_read: vec![],
            allow_write: vec![],
            deny_write: vec![],
            allow_execute: vec![],
            deny_execute: vec![],
            allow_net: false,
            env: None,
        };

        let profile = generate_profile(&spec).unwrap();
        let profile_str = String::from_utf8(profile).unwrap();

        assert!(profile_str.contains("(deny default)"));
        assert!(profile_str.contains("(allow file-read*"));
        assert!(!profile_str.contains("(allow network*)"));
    }

    #[test]
    fn test_generate_profile_no_network_excludes_system_network() {
        let spec = SandboxSpec {
            allow_read: vec![],
            deny_read: vec![],
            allow_write: vec![],
            deny_write: vec![],
            allow_execute: vec![],
            deny_execute: vec![],
            allow_net: false,
            env: None,
        };

        let profile = generate_profile(&spec).unwrap();
        let profile_str = String::from_utf8(profile).unwrap();

        // `(system-network)` must NOT be present when network is denied —
        // it grants access to mDNSResponder, enabling DNS exfiltration.
        assert!(
            !profile_str.contains("(system-network)"),
            "system-network should not be in profile when allow_net = false"
        );
        assert!(!profile_str.contains("(allow network*)"));
        // AF_ROUTE should still be allowed for gethostname(2).
        assert!(
            profile_str.contains("AF_ROUTE"),
            "AF_ROUTE should be allowed for gethostname"
        );
    }

    #[test]
    fn test_generate_profile_with_network() {
        let spec = SandboxSpec {
            allow_read: vec![],
            deny_read: vec![],
            allow_write: vec![],
            deny_write: vec![],
            allow_execute: vec![],
            deny_execute: vec![],
            allow_net: true,
            env: None,
        };

        let profile = generate_profile(&spec).unwrap();
        let profile_str = String::from_utf8(profile).unwrap();

        assert!(profile_str.contains("(allow network*)"));
        // `(system-network)` should be present for DNS resolution.
        assert!(
            profile_str.contains("(system-network)"),
            "system-network should be in profile when allow_net = true"
        );
    }

    #[test]
    fn test_escape_path_rejects_newline() {
        let result = escape_path(Path::new("/tmp/evil\n(allow network*)"));
        assert!(result.is_err(), "paths with newlines should be rejected");
    }

    #[test]
    fn test_escape_path_rejects_carriage_return() {
        let result = escape_path(Path::new("/tmp/evil\r(allow network*)"));
        assert!(
            result.is_err(),
            "paths with carriage returns should be rejected"
        );
    }

    #[test]
    fn test_escape_path_rejects_nul_byte() {
        let result = escape_path(Path::new("/tmp/evil\0inject"));
        assert!(result.is_err(), "paths with NUL bytes should be rejected");
    }

    #[test]
    fn test_escape_path_rejects_parentheses() {
        let result = escape_path(Path::new("/tmp/evil(allow network*)"));
        assert!(result.is_err(), "paths with parentheses should be rejected");
    }

    #[test]
    fn test_escape_path_rejects_semicolon() {
        let result = escape_path(Path::new("/tmp/evil;comment"));
        assert!(
            result.is_err(),
            "paths with semicolons should be rejected (SBPL comment delimiter)"
        );
    }

    #[test]
    fn test_generate_profile_deny_overrides() {
        let spec = SandboxSpec {
            allow_read: vec![PathBuf::from("/home/user")],
            deny_read: vec![PathBuf::from("/home/user/.ssh")],
            allow_write: vec![PathBuf::from("/home/user")],
            deny_write: vec![PathBuf::from("/home/user/.bashrc")],
            allow_execute: vec![],
            deny_execute: vec![],
            allow_net: false,
            env: None,
        };

        let profile = generate_profile(&spec).unwrap();
        let profile_str = String::from_utf8(profile).unwrap();

        // Allow rules should come before deny rules for the same parent.
        let allow_read_pos = profile_str.find("(allow file-read*").unwrap();
        let deny_read_pos = profile_str.find("(deny file-read*").unwrap();
        assert!(allow_read_pos < deny_read_pos);

        let allow_write_pos = profile_str.find("(allow file-write*").unwrap();
        let deny_write_pos = profile_str.find("(deny file-write*").unwrap();
        assert!(allow_write_pos < deny_write_pos);
    }
}
