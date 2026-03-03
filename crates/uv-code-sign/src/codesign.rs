use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use owo_colors::OwoColorize;
use thiserror::Error;

/// Options for the `codesign` invocation.
#[derive(Clone)]
pub struct SignOptions {
    /// Signing identity. Defaults to `"-"` (ad-hoc).
    pub identity: String,
    /// Path to the `codesign` binary. Defaults to `"codesign"`.
    pub codesign_path: PathBuf,
}

impl Default for SignOptions {
    fn default() -> Self {
        Self {
            identity: "-".to_string(),
            codesign_path: PathBuf::from("codesign"),
        }
    }
}

#[derive(Debug, Error)]
pub enum CodeSignError {
    #[error("Failed to run `codesign`")]
    Command(#[source] std::io::Error),
    #[error(transparent)]
    Failed(#[from] CodeSignFailed),
}

/// `codesign` exited with a non-zero status.
#[derive(Debug, Error)]
pub struct CodeSignFailed {
    pub path: PathBuf,
    pub code: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl Display for CodeSignFailed {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Signing `{}` failed with exit status {}",
            self.path.display(),
            self.code
        )?;

        let mut non_empty = false;

        if !self.stdout.trim().is_empty() {
            write!(f, "\n\n{}\n{}", "[stdout]".red(), self.stdout)?;
            non_empty = true;
        }

        if !self.stderr.trim().is_empty() {
            write!(f, "\n\n{}\n{}", "[stderr]".red(), self.stderr)?;
            non_empty = true;
        }

        if non_empty {
            writeln!(f)?;
        }

        Ok(())
    }
}

/// Run `codesign --force --sign <identity> <path>` on a single file.
pub fn codesign_file(path: &Path, options: &SignOptions) -> Result<(), CodeSignError> {
    let output = Command::new(&options.codesign_path)
        .args(["--force", "--sign", &options.identity])
        .arg(path)
        .output()
        .map_err(CodeSignError::Command)?;

    if !output.status.success() {
        return Err(CodeSignFailed {
            path: path.to_path_buf(),
            code: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        }
        .into());
    }

    tracing::debug!("signed {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_options_default() {
        let opts = SignOptions::default();
        assert_eq!(opts.identity, "-");
        assert_eq!(opts.codesign_path, PathBuf::from("codesign"));
    }

    #[test]
    fn test_codesign_missing_binary() {
        let opts = SignOptions {
            codesign_path: PathBuf::from("/nonexistent/codesign"),
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("dummy");
        fs_err::write(&path, "not a binary").unwrap();

        let result = codesign_file(&path, &opts);
        assert!(
            matches!(result, Err(CodeSignError::Command(_))),
            "expected Command error, got: {result:?}"
        );
    }

    /// On macOS, codesign on a nonexistent path should fail.
    #[cfg(target_os = "macos")]
    #[test]
    fn test_codesign_nonexistent_path_fails() {
        let opts = SignOptions::default();
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("does_not_exist");

        let result = codesign_file(&path, &opts);
        assert!(
            matches!(result, Err(CodeSignError::Failed(_))),
            "expected Failed error, got: {result:?}"
        );
    }

    /// On macOS, signing a real Mach-O binary with ad-hoc identity should succeed.
    #[cfg(target_os = "macos")]
    #[test]
    fn test_codesign_real_binary() {
        let opts = SignOptions::default();

        // Copy /usr/bin/true to a temp location so we can sign it.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("true_copy");
        fs_err::copy("/usr/bin/true", &path).unwrap();

        let result = codesign_file(&path, &opts);
        assert!(result.is_ok(), "expected signing to succeed: {result:?}");
    }
}
