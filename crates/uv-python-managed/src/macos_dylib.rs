use std::ffi::CString;
use std::fs::Permissions;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;

use uv_fs::Simplified as _;
use uv_macho::{Error as MachoError, InstallName, SignedDylib, SigningIdentifier};
use uv_preview::PreviewFeature;
use uv_warnings::warn_user;

use crate::managed::ManagedPythonInstallation;

pub(crate) fn patch_dylib_install_name(dylib: PathBuf) -> Result<(), Error> {
    if uv_preview::is_enabled(PreviewFeature::NativeMachoEdit) {
        return NativeDylib::read(dylib)?.patch()?.persist();
    }

    let output = match std::process::Command::new("install_name_tool")
        .arg("-id")
        .arg(&dylib)
        .arg(&dylib)
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            let e = if e.kind() == ErrorKind::NotFound {
                Error::MissingInstallNameTool
            } else {
                e.into()
            };
            return Err(e);
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(Error::RenameError { dylib, stderr });
    }

    Ok(())
}

/// A resolved dylib file whose image transitions from original bytes to signed output.
struct NativeDylib<Image> {
    dylib: PathBuf,
    resolved: PathBuf,
    permissions: Permissions,
    image: Image,
}

impl NativeDylib<Vec<u8>> {
    fn read(dylib: PathBuf) -> Result<Self, Error> {
        let resolved = fs_err::canonicalize(&dylib)?;
        let permissions = fs_err::metadata(&resolved)?.permissions();
        let image = fs_err::read(&resolved)?;

        Ok(Self {
            dylib,
            resolved,
            permissions,
            image,
        })
    }

    fn patch(self) -> Result<NativeDylib<SignedDylib>, Error> {
        let filename = self.dylib.file_name().ok_or_else(|| {
            std::io::Error::new(ErrorKind::InvalidInput, "Missing dylib filename")
        })?;
        let image = InstallName::new(self.dylib.as_os_str().as_encoded_bytes())
            .and_then(|name| {
                let identifier = CString::new(filename.as_encoded_bytes())
                    .map_err(|_| MachoError::InvalidIdentifier)?;
                let identifier = SigningIdentifier::new(&identifier)?;
                uv_macho::set_install_name(&self.image, &name, identifier)
            })
            .map_err(|source| Error::NativeRenameError {
                dylib: self.dylib.clone(),
                source,
            })?;

        Ok(NativeDylib {
            dylib: self.dylib,
            resolved: self.resolved,
            permissions: self.permissions,
            image,
        })
    }
}

impl NativeDylib<SignedDylib> {
    fn persist(self) -> Result<(), Error> {
        // Replacing the resolved file preserves symlinks and avoids modifying an inode
        // whose signed pages may already be cached by the macOS kernel.
        let parent = self.resolved.parent().ok_or_else(|| {
            std::io::Error::new(ErrorKind::InvalidInput, "Missing dylib parent directory")
        })?;

        let mut temporary = uv_fs::tempfile_in(parent)?;
        temporary.write_all(self.image.as_bytes())?;
        temporary.as_file().set_permissions(self.permissions)?;
        uv_fs::persist_with_retry_sync(temporary, &self.resolved)?;

        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("`install_name_tool` is not available on this system.
This utility is part of macOS Developer Tools. Please ensure that the Xcode Command Line Tools are installed by running:

    xcode-select --install

For more information, see: https://developer.apple.com/xcode/")]
    MissingInstallNameTool,
    #[error("Failed to update the install name of the Python dynamic library located at `{}`", dylib.user_display())]
    RenameError { dylib: PathBuf, stderr: String },
    #[error("Failed to update the install name of the Python dynamic library located at `{}`: {source}", dylib.user_display())]
    NativeRenameError { dylib: PathBuf, source: MachoError },
}

impl Error {
    /// Emit a user-friendly warning about the patching failure.
    pub fn warn_user(&self, installation: &ManagedPythonInstallation) {
        let error = if tracing::enabled!(tracing::Level::DEBUG) {
            format!("\nUnderlying error: {self}")
        } else {
            String::new()
        };
        warn_user!(
            "Failed to patch the install name of the dynamic library for `{}`. This may cause issues when building Python native extensions.{}",
            installation.executable(false).simplified_display(),
            error
        );
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    use anyhow::Result;

    use super::NativeDylib;

    #[test]
    fn patch_dylib_preserves_symlink_and_permissions() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let dylib = directory.path().join("libfixture.dylib");
        let link = directory.path().join("link.dylib");
        fs_err::write(
            &dylib,
            include_bytes!("../../uv-macho/tests/fixtures/arm64.dylib"),
        )?;
        fs_err::set_permissions(&dylib, std::fs::Permissions::from_mode(0o755))?;
        fs_err::os::unix::fs::symlink(&dylib, &link)?;
        let before = fs_err::metadata(&dylib)?;

        NativeDylib::read(link.clone())?.patch()?.persist()?;

        let after = fs_err::metadata(&dylib)?;
        assert_eq!(fs_err::read_link(&link)?, dylib);
        assert_eq!(after.permissions().mode(), before.permissions().mode());
        assert_ne!(after.ino(), before.ino());

        let contents = fs_err::read(&dylib)?;
        NativeDylib::read(link)?.patch()?.persist()?;
        assert_eq!(fs_err::read(&dylib)?, contents);

        Ok(())
    }

    #[test]
    fn patch_dylib_failure_leaves_file_unchanged() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let dylib = directory.path().join("libfixture.dylib");
        let contents = b"not a Mach-O image";
        fs_err::write(&dylib, contents)?;
        let before = fs_err::metadata(&dylib)?;

        assert!(NativeDylib::read(dylib.clone())?.patch().is_err());

        assert_eq!(fs_err::read(&dylib)?, contents);
        assert_eq!(fs_err::metadata(&dylib)?.ino(), before.ino());
        assert_eq!(fs_err::read_dir(directory.path())?.count(), 1);

        Ok(())
    }
}
