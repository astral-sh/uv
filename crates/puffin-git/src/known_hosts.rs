//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/known_hosts.rs>
//!
//! SSH host key validation support.
//!
//! The only public item in this module is [`certificate_check`],
//! which provides a callback to [`git2::RemoteCallbacks::certificate_check`].
//!
//! A primary goal with this implementation is to provide user-friendly error
//! messages, guiding them to understand the issue and how to resolve it.
//!
//! Note that there are a lot of limitations here. This reads OpenSSH
//! `known_hosts` files from well-known locations, but it does not read OpenSSH
//! config files. The config file can change the behavior of how OpenSSH
//! handles `known_hosts` files. For example, some things we don't handle:
//!
//! - `GlobalKnownHostsFile` — Changes the location of the global host file.
//! - `UserKnownHostsFile` — Changes the location of the user's host file.
//! - `KnownHostsCommand` — A command to fetch known hosts.
//! - `CheckHostIP` — DNS spoofing checks.
//! - `VisualHostKey` — Shows a visual ascii-art key.
//! - `VerifyHostKeyDNS` — Uses SSHFP DNS records to fetch a host key.
//!
//! There's also a number of things that aren't supported but could be easily
//! added (it just adds a little complexity). For example, hostname patterns,
//! and revoked markers. See "FIXME" comments littered in this file.

use base64::engine::general_purpose::STANDARD;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine as _;
use git2::cert::{Cert, SshHostKeyType};
use git2::CertificateCheckStatus;
use hmac::Mac;
use std::collections::HashSet;
use std::fmt::{Display, Write};
use std::path::{Path, PathBuf};

/// These are host keys that are hard-coded in cargo to provide convenience.
///
/// If GitHub ever publishes new keys, the user can add them to their own
/// configuration file to use those instead.
///
/// The GitHub keys are sourced from <https://api.github.com/meta> or
/// <https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/githubs-ssh-key-fingerprints>.
///
/// These will be ignored if the user adds their own entries for `github.com`,
/// which can be useful if GitHub ever revokes their old keys.
static BUNDLED_KEYS: &[(&str, &str, &str)] = &[
    ("github.com", "ssh-ed25519", "AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl"),
    ("github.com", "ecdsa-sha2-nistp256", "AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBEmKSENjQEezOmxkZMy7opKgwFB9nkt5YRrYMjNuG5N87uRgg6CLrbo5wAdT/y6v0mKV0U2w0WZ2YB/++Tpockg="),
    ("github.com", "ssh-rsa", "AAAAB3NzaC1yc2EAAAADAQABAAABgQCj7ndNxQowgcQnjshcLrqPEiiphnt+VTTvDP6mHBL9j1aNUkY4Ue1gvwnGLVlOhGeYrnZaMgRK6+PKCUXaDbC7qtbW8gIkhL7aGCsOr/C56SJMy/BCZfxd1nWzAOxSDPgVsmerOBYfNqltV9/hWCqBywINIR+5dIg6JTJ72pcEpEjcYgXkE2YEFXV1JHnsKgbLWNlhScqb2UmyRkQyytRLtL+38TGxkxCflmO+5Z8CSSNY7GidjMIZ7Q4zMjA2n1nGrlTDkzwDCsw+wqFPGQA179cnfGWOWRVruj16z6XyvxvjJwbz0wQZ75XK5tKSb7FNyeIEs4TT4jk+S4dhPeAUC5y+bDYirYgM4GC7uEnztnZyaVWQ7B381AK4Qdrwt51ZqExKbQpTUNn+EjqoTwvqNj4kqx5QUCI0ThS/YkOxJCXmPUWZbhjpCg56i+2aB6CmK2JGhn57K5mj0MNdBXA4/WnwH6XoPWJzK5Nyu2zB3nAZp+S5hpQs+p1vN1/wsjk="),
];

/// List of keys that public hosts have rotated away from.
///
/// We explicitly distrust these keys as users with the old key in their
/// local configuration will otherwise be vulnerable to MITM attacks if the
/// attacker has access to the old key. As there is no other way to distribute
/// revocations of ssh host keys, we need to bundle them with the client.
///
/// Unlike [`BUNDLED_KEYS`], these revocations will not be ignored if the user
/// has their own entries: we *know* that these keys are bad.
static BUNDLED_REVOCATIONS: &[(&str, &str, &str)] = &[
    // Used until March 24, 2023: https://github.blog/2023-03-23-we-updated-our-rsa-ssh-host-key/
    ("github.com", "ssh-rsa", "AAAAB3NzaC1yc2EAAAABIwAAAQEAq2A7hRGmdnm9tUDbO9IDSwBK6TbQa+PXYPCPy6rbTrTtw7PHkccKrpp0yVhp5HdEIcKr6pLlVDBfOLX9QUsyCOV0wzfjIJNlGEYsdlLJizHhbn2mUjvSAHQqZETYP81eFzLQNnPHt4EVVUh7VfDESU84KezmD5QlWpXLmvU31/yMf+Se8xhHTvKSCZIFImWwoG6mbUoWf9nzpIoaSjB+weqqUUmpaaasXVal72J+UX2B+2RPW3RcT0eOzQgqlJL3RKrTJvdsjE3JEAvGq3lGHSZXy28G3skua2SmVi/w4yCE6gbODqnTWlg7+wC604ydGXA8VJiS5ap43JXiUFFAaQ=="),
];

enum KnownHostError {
    /// Some general error happened while validating the known hosts.
    CheckError(anyhow::Error),
    /// The host key was not found.
    HostKeyNotFound {
        hostname: String,
        key_type: SshHostKeyType,
        remote_host_key: String,
        remote_fingerprint: String,
        other_hosts: Vec<KnownHost>,
    },
    /// The host key was found, but does not match the remote's key.
    HostKeyHasChanged {
        hostname: String,
        key_type: SshHostKeyType,
        old_known_host: KnownHost,
        remote_host_key: String,
        remote_fingerprint: String,
    },
    /// The host key was found with a @revoked marker, it must not be accepted.
    HostKeyRevoked {
        hostname: String,
        key_type: SshHostKeyType,
        remote_host_key: String,
        location: KnownHostLocation,
    },
    /// The host key was not found, but there was a matching known host with a
    /// @cert-authority marker (which Cargo doesn't yet support).
    HostHasOnlyCertAuthority {
        hostname: String,
        location: KnownHostLocation,
    },
}

impl From<anyhow::Error> for KnownHostError {
    fn from(err: anyhow::Error) -> KnownHostError {
        KnownHostError::CheckError(err)
    }
}

/// The location where a host key was located.
#[derive(Clone)]
enum KnownHostLocation {
    /// Loaded from a file from disk.
    File { path: PathBuf, lineno: usize },
    /// Part of the hard-coded bundled keys in Cargo.
    Bundled,
}

impl Display for KnownHostLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let loc = match self {
            KnownHostLocation::File { path, lineno } => {
                format!("{} line {lineno}", path.display())
            }
            KnownHostLocation::Bundled => "bundled with cargo".to_string(),
        };
        f.write_str(&loc)
    }
}

/// The git2 callback used to validate a certificate (only ssh known hosts are validated).
pub(crate) fn certificate_check(
    cert: &Cert<'_>,
    host: &str,
    port: Option<u16>,
) -> Result<CertificateCheckStatus, git2::Error> {
    let Some(host_key) = cert.as_hostkey() else {
        // Return passthrough for TLS X509 certificates to use whatever validation
        // was done in git2.
        return Ok(CertificateCheckStatus::CertificatePassthrough);
    };
    // If a nonstandard port is in use, check for that first.
    // The fallback to check without a port is handled in the HostKeyNotFound handler.
    let host_maybe_port = match port {
        Some(port) if port != 22 => format!("[{host}]:{port}"),
        _ => host.to_string(),
    };
    // The error message must be constructed as a string to pass through the libgit2 C API.
    let err_msg = match check_ssh_known_hosts(host_key, &host_maybe_port) {
        Ok(()) => {
            return Ok(CertificateCheckStatus::CertificateOk);
        }
        Err(KnownHostError::CheckError(e)) => {
            format!("error: failed to validate host key:\n{e:#}")
        }
        Err(KnownHostError::HostKeyNotFound {
            hostname,
            key_type,
            remote_host_key,
            remote_fingerprint,
            other_hosts,
        }) => {
            // Try checking without the port.
            if port.is_some()
                && !matches!(port, Some(22))
                && check_ssh_known_hosts(host_key, host).is_ok()
            {
                return Ok(CertificateCheckStatus::CertificateOk);
            }
            let key_type_short_name = key_type.short_name();
            let key_type_name = key_type.name();
            let other_hosts_message = if other_hosts.is_empty() {
                String::new()
            } else {
                let mut msg = String::from(
                    "Note: This host key was found, \
                    but is associated with a different host:\n",
                );
                for known_host in other_hosts {
                    writeln!(
                        msg,
                        "    {loc}: {patterns}",
                        loc = known_host.location,
                        patterns = known_host.patterns
                    )
                    .unwrap();
                }
                msg
            };
            format!("error: unknown SSH host key\n\
                The SSH host key for `{hostname}` is not known and cannot be validated.\n\
                \n\
                To resolve this issue, add the host key to the list of known hosts.\n\
                \n\
                The key to add is:\n\
                \n\
                {hostname} {key_type_name} {remote_host_key}\n\
                \n\
                The {key_type_short_name} key fingerprint is: SHA256:{remote_fingerprint}\n\
                This fingerprint should be validated with the server administrator that it is correct.\n\
                {other_hosts_message}\n\
                See https://doc.rust-lang.org/stable/cargo/appendix/git-authentication.html#ssh-known-hosts \
                for more information.\n\
                ")
        }
        Err(KnownHostError::HostKeyHasChanged {
            hostname,
            key_type,
            old_known_host,
            remote_host_key,
            remote_fingerprint,
        }) => {
            let key_type_short_name = key_type.short_name();
            let key_type_name = key_type.name();
            let old_key_resolution = match old_known_host.location {
                KnownHostLocation::File { path, lineno } => {
                    let old_key_location = path.display();
                    format!(
                        "removing the old {key_type_name} key for `{hostname}` \
                        located at {old_key_location} line {lineno}, \
                        and adding the new key to the list of known hosts.",
                    )
                }
                KnownHostLocation::Bundled => "adding the new key to the list of known hosts.\n\
                        The current host key is bundled as part of Cargo."
                    .to_string(),
            };
            format!("error: SSH host key has changed for `{hostname}`\n\
                *********************************\n\
                * WARNING: HOST KEY HAS CHANGED *\n\
                *********************************\n\
                This may be caused by a man-in-the-middle attack, or the \
                server may have changed its host key.\n\
                \n\
                The {key_type_short_name} fingerprint for the key from the remote host is:\n\
                    SHA256:{remote_fingerprint}\n\
                \n\
                You are strongly encouraged to contact the server \
                administrator for `{hostname}` to verify that this new key is \
                correct.\n\
                \n\
                If you can verify that the server has a new key, you can \
                resolve this error by {old_key_resolution}\n\
                \n\
                The key provided by the remote host is:\n\
                \n\
                {hostname} {key_type_name} {remote_host_key}\n\
                \n\
                See https://doc.rust-lang.org/stable/cargo/appendix/git-authentication.html#ssh-known-hosts \
                for more information.\n\
                ")
        }
        Err(KnownHostError::HostKeyRevoked {
            hostname,
            key_type,
            remote_host_key,
            location,
        }) => {
            let key_type_short_name = key_type.short_name();
            format!(
                "error: Key has been revoked for `{hostname}`\n\
                **************************************\n\
                * WARNING: REVOKED HOST KEY DETECTED *\n\
                **************************************\n\
                This may indicate that the key provided by this host has been\n\
                compromised and should not be accepted.
                \n\
                The host key {key_type_short_name} {remote_host_key} is revoked\n\
                in {location} and has been rejected.\n\
                "
            )
        }
        Err(KnownHostError::HostHasOnlyCertAuthority { hostname, location }) => {
            format!("error: Found a `@cert-authority` marker for `{hostname}`\n\
                \n\
                Cargo doesn't support certificate authorities for host key verification. It is\n\
                recommended that the command line Git client is used instead. This can be achieved\n\
                by setting `net.git-fetch-with-cli` to `true` in the Cargo config.\n\
                \n
                The `@cert-authority` line was found in {location}.\n\
                \n\
                See https://doc.rust-lang.org/stable/cargo/appendix/git-authentication.html#ssh-known-hosts \
                for more information.\n\
                ")
        }
    };
    Err(git2::Error::new(
        git2::ErrorCode::GenericError,
        git2::ErrorClass::Callback,
        err_msg,
    ))
}

/// Checks if the given host/host key pair is known.
#[allow(clippy::result_large_err)]
fn check_ssh_known_hosts(
    cert_host_key: &git2::cert::CertHostkey<'_>,
    host: &str,
) -> Result<(), KnownHostError> {
    let Some(remote_host_key) = cert_host_key.hostkey() else {
        return Err(anyhow::format_err!("remote host key is not available").into());
    };
    let remote_key_type = cert_host_key.hostkey_type().unwrap();

    // Collect all the known host entries from disk.
    let mut known_hosts = Vec::new();
    for path in known_host_files() {
        if !path.exists() {
            continue;
        }
        let hosts = load_hostfile(&path)?;
        known_hosts.extend(hosts);
    }
    // Load the bundled keys. Don't add keys for hosts that the user has
    // configured, which gives them the option to override them. This could be
    // useful if the keys are ever revoked.
    let configured_hosts: HashSet<_> = known_hosts
        .iter()
        .flat_map(|known_host| known_host.patterns.split(',').map(str::to_lowercase))
        .collect();
    for (patterns, key_type, key) in BUNDLED_KEYS {
        if !configured_hosts.contains(*patterns) {
            let key = STANDARD.decode(key).unwrap();
            known_hosts.push(KnownHost {
                location: KnownHostLocation::Bundled,
                patterns: (*patterns).to_string(),
                key_type: (*key_type).to_string(),
                key,
                line_type: KnownHostLineType::Key,
            });
        }
    }
    for (patterns, key_type, key) in BUNDLED_REVOCATIONS {
        let key = STANDARD.decode(key).unwrap();
        known_hosts.push(KnownHost {
            location: KnownHostLocation::Bundled,
            patterns: (*patterns).to_string(),
            key_type: (*key_type).to_string(),
            key,
            line_type: KnownHostLineType::Revoked,
        });
    }
    check_ssh_known_hosts_loaded(&known_hosts, host, remote_key_type, remote_host_key)
}

/// Checks a host key against a loaded set of known hosts.
#[allow(clippy::result_large_err)]
fn check_ssh_known_hosts_loaded(
    known_hosts: &[KnownHost],
    host: &str,
    remote_key_type: SshHostKeyType,
    remote_host_key: &[u8],
) -> Result<(), KnownHostError> {
    // `latent_error` keeps track of a potential error that will be returned
    // in case a matching host key isn't found.
    let mut latent_errors: Vec<KnownHostError> = Vec::new();

    // `other_hosts` keeps track of any entries that have an identical key,
    // but a different hostname.
    let mut other_hosts = Vec::new();

    // `accepted_known_host_found` keeps track of whether we've found a matching
    // line in the `known_hosts` file that we would accept. We can't return that
    // immediately, because there may be a subsequent @revoked key.
    let mut accepted_known_host_found = false;

    // Older versions of OpenSSH (before 6.8, March 2015) showed MD5
    // fingerprints (see FingerprintHash ssh config option). Here we only
    // support SHA256.
    let mut remote_fingerprint = cargo_util::Sha256::new();
    remote_fingerprint.update(remote_host_key);
    let remote_fingerprint = STANDARD_NO_PAD.encode(remote_fingerprint.finish());
    let remote_host_key_encoded = STANDARD.encode(remote_host_key);

    for known_host in known_hosts {
        // The key type from libgit2 needs to match the key type from the host file.
        if known_host.key_type != remote_key_type.name() {
            continue;
        }
        let key_matches = known_host.key == remote_host_key;
        if !known_host.host_matches(host) {
            if key_matches {
                other_hosts.push(known_host.clone());
            }
            continue;
        }
        match known_host.line_type {
            KnownHostLineType::Key => {
                if key_matches {
                    accepted_known_host_found = true;
                } else {
                    // The host and key type matched, but the key itself did not.
                    // This indicates the key has changed.
                    // This is only reported as an error if no subsequent lines have a
                    // correct key.
                    latent_errors.push(KnownHostError::HostKeyHasChanged {
                        hostname: host.to_string(),
                        key_type: remote_key_type,
                        old_known_host: known_host.clone(),
                        remote_host_key: remote_host_key_encoded.clone(),
                        remote_fingerprint: remote_fingerprint.clone(),
                    });
                }
            }
            KnownHostLineType::Revoked => {
                if key_matches {
                    return Err(KnownHostError::HostKeyRevoked {
                        hostname: host.to_string(),
                        key_type: remote_key_type,
                        remote_host_key: remote_host_key_encoded,
                        location: known_host.location.clone(),
                    });
                }
            }
            KnownHostLineType::CertAuthority => {
                // The host matches a @cert-authority line, which is unsupported.
                latent_errors.push(KnownHostError::HostHasOnlyCertAuthority {
                    hostname: host.to_string(),
                    location: known_host.location.clone(),
                });
            }
        }
    }

    // We have an accepted host key and it hasn't been revoked.
    if accepted_known_host_found {
        return Ok(());
    }

    if latent_errors.is_empty() {
        // FIXME: Ideally the error message should include the IP address of the
        // remote host (to help the user validate that they are connecting to the
        // host they were expecting to). However, I don't see a way to obtain that
        // information from libgit2.
        Err(KnownHostError::HostKeyNotFound {
            hostname: host.to_string(),
            key_type: remote_key_type,
            remote_host_key: remote_host_key_encoded,
            remote_fingerprint,
            other_hosts,
        })
    } else {
        // We're going to take the first HostKeyHasChanged error if
        // we find one, otherwise we'll take the first error (which
        // we expect to be a CertAuthority error).
        if let Some(index) = latent_errors
            .iter()
            .position(|e| matches!(e, KnownHostError::HostKeyHasChanged { .. }))
        {
            Err(latent_errors.remove(index))
        } else {
            // Otherwise, we take the first error (which we expect to be
            // a CertAuthority error).
            Err(latent_errors.pop().unwrap())
        }
    }
}

/// Returns a list of files to try loading OpenSSH-formatted known hosts.
fn known_host_files() -> Vec<PathBuf> {
    let mut result = Vec::new();
    if cfg!(unix) {
        result.push(PathBuf::from("/etc/ssh/ssh_known_hosts"));
    } else if cfg!(windows) {
        // The msys/cygwin version of OpenSSH uses `/etc` from the posix root
        // filesystem there (such as `C:\msys64\etc\ssh\ssh_known_hosts`).
        // However, I do not know of a way to obtain that location from
        // Windows-land. The ProgramData version here is what the PowerShell
        // port of OpenSSH does.
        if let Some(progdata) = std::env::var_os("ProgramData") {
            let mut progdata = PathBuf::from(progdata);
            progdata.push("ssh");
            progdata.push("ssh_known_hosts");
            result.push(progdata);
        }
    }
    result.extend(user_known_host_location());
    result
}

/// The location of the user's `known_hosts` file.
fn user_known_host_location() -> Option<PathBuf> {
    // NOTE: This is a potentially inaccurate prediction of what the user
    // actually wants. The actual location depends on several factors:
    //
    // - Windows OpenSSH Powershell version: I believe this looks up the home
    //   directory via ProfileImagePath in the registry, falling back to
    //   `GetWindowsDirectoryW` if that fails.
    // - OpenSSH Portable (under msys): This is very complicated. I got lost
    //   after following it through some ldap/active directory stuff.
    // - OpenSSH (most unix platforms): Uses `pw->pw_dir` from `getpwuid()`.
    //
    // This doesn't do anything close to that. home_dir's behavior is:
    // - Windows: $USERPROFILE, or SHGetKnownFolderPath()
    // - Unix: $HOME, or getpwuid_r()
    //
    // Since there is a mismatch here, the location returned here might be
    // different than what the user's `ssh` CLI command uses. We may want to
    // consider trying to align it better.
    home::home_dir().map(|mut home| {
        home.push(".ssh");
        home.push("known_hosts");
        home
    })
}

const HASH_HOSTNAME_PREFIX: &str = "|1|";

#[derive(Clone)]
enum KnownHostLineType {
    Key,
    CertAuthority,
    Revoked,
}

/// A single known host entry.
#[derive(Clone)]
struct KnownHost {
    location: KnownHostLocation,
    /// The hostname. May be comma separated to match multiple hosts.
    patterns: String,
    key_type: String,
    key: Vec<u8>,
    line_type: KnownHostLineType,
}

impl KnownHost {
    /// Returns whether or not the given host matches this known host entry.
    fn host_matches(&self, host: &str) -> bool {
        let mut match_found = false;
        let host = host.to_lowercase();
        if let Some(hashed) = self.patterns.strip_prefix(HASH_HOSTNAME_PREFIX) {
            return hashed_hostname_matches(&host, hashed);
        }
        for pattern in self.patterns.split(',') {
            let pattern = pattern.to_lowercase();
            // FIXME: support * and ? wildcards
            if let Some(pattern) = pattern.strip_prefix('!') {
                if pattern == host {
                    return false;
                }
            } else {
                match_found |= pattern == host;
            }
        }
        match_found
    }
}

fn hashed_hostname_matches(host: &str, hashed: &str) -> bool {
    let Some((b64_salt, b64_host)) = hashed.split_once('|') else {
        return false;
    };
    let Ok(salt) = STANDARD.decode(b64_salt) else {
        return false;
    };
    let Ok(hashed_host) = STANDARD.decode(b64_host) else {
        return false;
    };
    let Ok(mut mac) = hmac::Hmac::<sha1::Sha1>::new_from_slice(&salt) else {
        return false;
    };
    mac.update(host.as_bytes());
    let result = mac.finalize().into_bytes();
    hashed_host == result[..]
}

/// Loads an OpenSSH `known_hosts` file.
fn load_hostfile(path: &Path) -> Result<Vec<KnownHost>, anyhow::Error> {
    let contents = cargo_util::paths::read(path)?;
    Ok(load_hostfile_contents(path, &contents))
}

fn load_hostfile_contents(path: &Path, contents: &str) -> Vec<KnownHost> {
    let entries = contents
        .lines()
        .enumerate()
        .filter_map(|(lineno, line)| {
            let location = KnownHostLocation::File {
                path: path.to_path_buf(),
                lineno: lineno + 1,
            };
            parse_known_hosts_line(line, location)
        })
        .collect();
    entries
}

fn parse_known_hosts_line(line: &str, location: KnownHostLocation) -> Option<KnownHost> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut parts = line.split([' ', '\t']).filter(|s| !s.is_empty());

    let line_type = if line.starts_with('@') {
        let line_type = parts.next()?;

        if line_type == "@cert-authority" {
            KnownHostLineType::CertAuthority
        } else if line_type == "@revoked" {
            KnownHostLineType::Revoked
        } else {
            // No other markers are defined
            return None;
        }
    } else {
        KnownHostLineType::Key
    };

    let patterns = parts.next()?;
    let key_type = parts.next()?;
    let key = parts.next().map(|p| STANDARD.decode(p))?.ok()?;
    Some(KnownHost {
        line_type,
        location,
        patterns: patterns.to_string(),
        key_type: key_type.to_string(),
        key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    static COMMON_CONTENTS: &str = r"
        # Comments allowed at start of line

        example.com,rust-lang.org ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC5MzWIpZwpkpDjyCNiTIEVFhSA9OUUQvjFo7CgZBGCAj/cqeUIgiLsgtfmtBsfWIkAECQpM7ePP7NLZFGJcHvoyg5jXJiIX5s0eKo9IlcuTLLrMkW5MkHXE7bNklVbW1WdCfF2+y7Ao25B4L8FFRokMh0yp/H6+8xZ7PdVwL3FRPEg8ftZ5R0kuups6xiMHPRX+f/07vfJzA47YDPmXfhkn+JK8kL0JYw8iy8BtNBfRQL99d9iXJzWXnNce5NHMuKD5rOonD3aQHLDlwK+KhrFRrdaxQEM8ZWxNti0ux8yT4Dl5jJY0CrIu3Xl6+qroVgTqJGNkTbhs5DGWdFh6BLPTTH15rN4buisg7uMyLyHqx06ckborqD33gWu+Jig7O+PV6KJmL5mp1O1HXvZqkpBdTiT6GiDKG3oECCIXkUk0BSU9VG9VQcrMxxvgiHlyoXUAfYQoXv/lnxkTnm+Sr36kutsVOs7n5B43ZKAeuaxyQ11huJZpxamc0RA1HM641s= eric@host
        Example.net ssh-dss AAAAB3NzaC1kc3MAAACBAK2Ek3jVxisXmz5UcZ7W65BAj/nDJCCVvSe0Aytndn4PH6k7sVesut5OoY6PdksZ9tEfuFjjS9HR5SJb8j1GW0GxtaSHHbf+rNc36PeU75bffzyIWwpA8uZFONt5swUAXJXcsHOoapNbUFuhHsRhB2hXxz9QGNiiwIwRJeSHixKRAAAAFQChKfxO1z9H2/757697xP5nJ/Z5dwAAAIEAoc+HIWas+4WowtB/KtAp6XE0B9oHI+55wKtdcGwwb7zHKK9scWNXwxIcMhSvyB3Oe2I7dQQlvyIWxsdZlzOkX0wdsTHjIAnBAP68MyvMv4kq3+I5GAVcFsqoLZfZvh0dlcgUq1/YNYZwKlt89tnzk8Fp4KLWmuw8Bd8IShYVa78AAACAL3qd8kNTY7CthgsQ8iWdjbkGSF/1KCeFyt8UjurInp9wvPDjqagwakbyLOzN7y3/ItTPCaGuX+RjFP0zZTf8i9bsAVyjFJiJ7vzRXcWytuFWANrpzLTn1qzPfh63iK92Aw8AVBYvEA/4bxo+XReAvhNBB/m78G6OedTeu6ZoTsI= eric@host
        [example.net]:2222 ssh-dss AAAAB3NzaC1kc3MAAACBAJJN5kLZEpOJpXWyMT4KwYvLAj+b9ErNtglxOi86C6Kw7oZeYdDMCfD3lc3PJyX64udQcWGfO4abSESMiYdY43yFAZH279QGH5Q/B5CklVvTqYpfAUR+1r9TQxy3OVQHk7FB2wOi4xNQ3myO0vaYlBOB9il+P223aERbXx4JTWdvAAAAFQCTHWTcXxLK5Z6ZVPmfdSDyHzkF2wAAAIEAhp41/mTnM0Y0EWSyCXuETMW1QSpKGF8sqoZKp6wdzyhLXu0i32gLdXj4p24em/jObYh93hr+MwgxqWq+FHgD+D80Qg5f6vj4yEl4Uu5hqtTpCBFWUQoyEckbUkPf8uZ4/XzAne+tUSjZm09xATCmK9U2IGqZE+D+90eBkf1Svc8AAACAeKhi4EtfwenFYqKz60ZoEEhIsE1yI2jH73akHnfHpcW84w+fk3YlwjcfDfyYso+D0jZBdJeK5qIdkbUWhAX8wDjJVO0WL6r/YPr4yu/CgEyW1H59tAbujGJ4NR0JDqioulzYqNHnxpiw1RJukZnPBfSFKzRElvPOCq/NkQM/Mwk= eric@host
        nistp256.example.org ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBJ4iYGCcJrUIfrHfzlsv8e8kaF36qpcUpe3VNAKVCZX/BDptIdlEe8u8vKNRTPgUO9jqS0+tjTcPiQd8/8I9qng= eric@host
        nistp384.example.org ecdsa-sha2-nistp384 AAAAE2VjZHNhLXNoYTItbmlzdHAzODQAAAAIbmlzdHAzODQAAABhBNuGT3TqMz2rcwOt2ZqkiNqq7dvWPE66W2qPCoZsh0pQhVU3BnhKIc6nEr6+Wts0Z3jdF3QWwxbbTjbVTVhdr8fMCFhDCWiQFm9xLerYPKnu9qHvx9K87/fjc5+0pu4hLA== eric@host
        nistp521.example.org ecdsa-sha2-nistp521 AAAAE2VjZHNhLXNoYTItbmlzdHA1MjEAAAAIbmlzdHA1MjEAAACFBAD35HH6OsK4DN75BrKipVj/GvZaUzjPNa1F8wMjUdPB1JlVcUfgzJjWSxrhmaNN3u0soiZw8WNRFINsGPCw5E7DywF1689WcIj2Ye2rcy99je15FknScTzBBD04JgIyOI50mCUaPCBoF14vFlN6BmO00cFo+yzy5N8GuQ2sx9kr21xmFQ== eric@host
        # Revoked is supported, but without Cert-Authority support, it will only negate some other fixed key.
        @revoked revoked.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKtQsi+KPYispwm2rkMidQf30fG1Niy8XNkvASfePoca eric@host
        # Cert-Authority is not supported (below key should not be valid anyway)
        @cert-authority ca.example.com ssh-rsa AABBB5Wm
        example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAWkjI6XT2SZh3xNk5NhisA3o3sGzWR+VAKMSqHtI0aY eric@host
        192.168.42.12 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKVYJpa0yUGaNk0NXQTPWa0tHjqRpx+7hl2diReH6DtR eric@host
        |1|QxzZoTXIWLhUsuHAXjuDMIV3FjQ=|M6NCOIkjiWdCWqkh5+Q+/uFLGjs= ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIHgN3O21U4LWtP5OzjTzPnUnSDmCNDvyvlaj6Hi65JC eric@host
        # Negation isn't terribly useful without globs.
        neg.example.com,!neg.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOXfUnaAHTlo1Qi//rNk26OcmHikmkns1Z6WW/UuuS3K eric@host
    ";

    #[test]
    fn known_hosts_parse() {
        let kh_path = Path::new("/home/abc/.known_hosts");
        let khs = load_hostfile_contents(kh_path, COMMON_CONTENTS);
        assert_eq!(khs.len(), 12);
        match &khs[0].location {
            KnownHostLocation::File { path, lineno } => {
                assert_eq!(path, kh_path);
                assert_eq!(*lineno, 4);
            }
            KnownHostLocation::Bundled => panic!("unexpected"),
        }
        assert_eq!(khs[0].patterns, "example.com,rust-lang.org");
        assert_eq!(khs[0].key_type, "ssh-rsa");
        assert_eq!(khs[0].key.len(), 407);
        assert_eq!(&khs[0].key[..30], b"\x00\x00\x00\x07ssh-rsa\x00\x00\x00\x03\x01\x00\x01\x00\x00\x01\x81\x00\xb935\x88\xa5\x9c)");
        match &khs[1].location {
            KnownHostLocation::File { path, lineno } => {
                assert_eq!(path, kh_path);
                assert_eq!(*lineno, 5);
            }
            KnownHostLocation::Bundled => panic!("unexpected"),
        }
        assert_eq!(khs[2].patterns, "[example.net]:2222");
        assert_eq!(khs[3].patterns, "nistp256.example.org");
        assert_eq!(khs[9].patterns, "192.168.42.12");
    }

    #[test]
    fn host_matches() {
        let kh_path = Path::new("/home/abc/.known_hosts");
        let khs = load_hostfile_contents(kh_path, COMMON_CONTENTS);
        assert!(khs[0].host_matches("example.com"));
        assert!(khs[0].host_matches("rust-lang.org"));
        assert!(khs[0].host_matches("EXAMPLE.COM"));
        assert!(khs[1].host_matches("example.net"));
        assert!(!khs[0].host_matches("example.net"));
        assert!(khs[2].host_matches("[example.net]:2222"));
        assert!(!khs[2].host_matches("example.net"));
        assert!(khs[10].host_matches("hashed.example.com"));
        assert!(!khs[10].host_matches("example.com"));
        assert!(!khs[11].host_matches("neg.example.com"));
    }

    #[test]
    fn check_match() {
        let kh_path = Path::new("/home/abc/.known_hosts");
        let khs = load_hostfile_contents(kh_path, COMMON_CONTENTS);

        assert!(check_ssh_known_hosts_loaded(
            &khs,
            "example.com",
            SshHostKeyType::Rsa,
            &khs[0].key
        )
        .is_ok());

        match check_ssh_known_hosts_loaded(&khs, "example.com", SshHostKeyType::Dss, &khs[0].key) {
            Err(KnownHostError::HostKeyNotFound {
                hostname,
                remote_fingerprint,
                other_hosts,
                ..
            }) => {
                assert_eq!(
                    remote_fingerprint,
                    "yn+pONDn0EcgdOCVptgB4RZd/wqmsVKrPnQMLtrvhw8"
                );
                assert_eq!(hostname, "example.com");
                assert_eq!(other_hosts.len(), 0);
            }
            _ => panic!("unexpected"),
        }

        match check_ssh_known_hosts_loaded(
            &khs,
            "foo.example.com",
            SshHostKeyType::Rsa,
            &khs[0].key,
        ) {
            Err(KnownHostError::HostKeyNotFound { other_hosts, .. }) => {
                assert_eq!(other_hosts.len(), 1);
                assert_eq!(other_hosts[0].patterns, "example.com,rust-lang.org");
            }
            _ => panic!("unexpected"),
        }

        let mut modified_key = khs[0].key.clone();
        modified_key[0] = 1;
        match check_ssh_known_hosts_loaded(&khs, "example.com", SshHostKeyType::Rsa, &modified_key)
        {
            Err(KnownHostError::HostKeyHasChanged { old_known_host, .. }) => {
                assert!(matches!(
                    old_known_host.location,
                    KnownHostLocation::File { lineno: 4, .. }
                ));
            }
            _ => panic!("unexpected"),
        }
    }

    #[test]
    fn revoked() {
        let kh_path = Path::new("/home/abc/.known_hosts");
        let khs = load_hostfile_contents(kh_path, COMMON_CONTENTS);

        match check_ssh_known_hosts_loaded(
            &khs,
            "revoked.example.com",
            SshHostKeyType::Ed255219,
            &khs[6].key,
        ) {
            Err(KnownHostError::HostKeyRevoked {
                hostname, location, ..
            }) => {
                assert_eq!("revoked.example.com", hostname);
                assert!(matches!(
                    location,
                    KnownHostLocation::File { lineno: 11, .. }
                ));
            }
            _ => panic!("Expected key to be revoked for revoked.example.com."),
        }
    }

    #[test]
    fn cert_authority() {
        let kh_path = Path::new("/home/abc/.known_hosts");
        let khs = load_hostfile_contents(kh_path, COMMON_CONTENTS);

        match check_ssh_known_hosts_loaded(
            &khs,
            "ca.example.com",
            SshHostKeyType::Rsa,
            &khs[0].key, // The key should not matter
        ) {
            Err(KnownHostError::HostHasOnlyCertAuthority {
                hostname, location, ..
            }) => {
                assert_eq!("ca.example.com", hostname);
                assert!(matches!(
                    location,
                    KnownHostLocation::File { lineno: 13, .. }
                ));
            }
            Err(KnownHostError::HostKeyNotFound { hostname, .. }) => {
                panic!("host key not found... {hostname}");
            }
            _ => panic!("Expected host to only have @cert-authority line (which is unsupported)."),
        }
    }

    #[test]
    fn multiple_errors() {
        let contents = r"
        not-used.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAWkjI6XT2SZh3xNk5NhisA3o3sGzWR+VAKMSqHtI0aY eric@host
        # Cert-authority and changed key for the same host - changed key error should prevail
        @cert-authority example.com ssh-ed25519 AABBB5Wm
        example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKVYJpa0yUGaNk0NXQTPWa0tHjqRpx+7hl2diReH6DtR eric@host
        ";

        let kh_path = Path::new("/home/abc/.known_hosts");
        let khs = load_hostfile_contents(kh_path, contents);

        match check_ssh_known_hosts_loaded(
            &khs,
            "example.com",
            SshHostKeyType::Ed255219,
            &khs[0].key,
        ) {
            Err(KnownHostError::HostKeyHasChanged {
                hostname,
                old_known_host,
                remote_host_key,
                ..
            }) => {
                assert_eq!("example.com", hostname);
                assert_eq!(
                    "AAAAC3NzaC1lZDI1NTE5AAAAIAWkjI6XT2SZh3xNk5NhisA3o3sGzWR+VAKMSqHtI0aY",
                    remote_host_key
                );
                assert!(matches!(
                    old_known_host.location,
                    KnownHostLocation::File { lineno: 5, .. }
                ));
            }
            _ => panic!("Expected error to be of type HostKeyHasChanged."),
        }
    }

    #[test]
    fn known_host_and_revoked() {
        let contents = r"
        example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKVYJpa0yUGaNk0NXQTPWa0tHjqRpx+7hl2diReH6DtR eric@host
        # Later in the file the same host key is revoked
        @revoked example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKVYJpa0yUGaNk0NXQTPWa0tHjqRpx+7hl2diReH6DtR eric@host
        ";

        let kh_path = Path::new("/home/abc/.known_hosts");
        let khs = load_hostfile_contents(kh_path, contents);

        match check_ssh_known_hosts_loaded(
            &khs,
            "example.com",
            SshHostKeyType::Ed255219,
            &khs[0].key,
        ) {
            Err(KnownHostError::HostKeyRevoked {
                hostname,
                remote_host_key,
                location,
                ..
            }) => {
                assert_eq!("example.com", hostname);
                assert_eq!(
                    "AAAAC3NzaC1lZDI1NTE5AAAAIKVYJpa0yUGaNk0NXQTPWa0tHjqRpx+7hl2diReH6DtR",
                    remote_host_key
                );
                assert!(matches!(
                    location,
                    KnownHostLocation::File { lineno: 4, .. }
                ));
            }
            _ => panic!("Expected host key to be reject with error HostKeyRevoked."),
        }
    }
}
