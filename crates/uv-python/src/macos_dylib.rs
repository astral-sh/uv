use std::{io::ErrorKind, path::PathBuf};

use uv_fs::Simplified as _;

pub fn patch_dylib_install_name(dylib: PathBuf) -> Result<(), Error> {
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
}
