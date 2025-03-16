use std::io::{self, Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::str::Utf8Error;

use fs_err::File;
use thiserror::Error;
use uv_fs::Simplified;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

#[cfg(all(windows, target_arch = "x86"))]
const LAUNCHER_I686_GUI: &[u8] =
    include_bytes!("../../uv-trampoline/trampolines/uv-trampoline-i686-gui.exe");

#[cfg(all(windows, target_arch = "x86"))]
const LAUNCHER_I686_CONSOLE: &[u8] =
    include_bytes!("../../uv-trampoline/trampolines/uv-trampoline-i686-console.exe");

#[cfg(all(windows, target_arch = "x86_64"))]
const LAUNCHER_X86_64_GUI: &[u8] =
    include_bytes!("../../uv-trampoline/trampolines/uv-trampoline-x86_64-gui.exe");

#[cfg(all(windows, target_arch = "x86_64"))]
const LAUNCHER_X86_64_CONSOLE: &[u8] =
    include_bytes!("../../uv-trampoline/trampolines/uv-trampoline-x86_64-console.exe");

#[cfg(all(windows, target_arch = "aarch64"))]
const LAUNCHER_AARCH64_GUI: &[u8] =
    include_bytes!("../../uv-trampoline/trampolines/uv-trampoline-aarch64-gui.exe");

#[cfg(all(windows, target_arch = "aarch64"))]
const LAUNCHER_AARCH64_CONSOLE: &[u8] =
    include_bytes!("../../uv-trampoline/trampolines/uv-trampoline-aarch64-console.exe");

// See `uv-trampoline::bounce`. These numbers must match.
const PATH_LENGTH_SIZE: usize = size_of::<u32>();
const MAX_PATH_LENGTH: u32 = 32 * 1024;
const MAGIC_NUMBER_SIZE: usize = 4;

#[derive(Debug)]
pub struct Launcher {
    pub kind: LauncherKind,
    pub python_path: PathBuf,
}

impl Launcher {
    /// Read [`Launcher`] metadata from a trampoline executable file.
    ///
    /// Returns `Ok(None)` if the file is not a trampoline executable.
    /// Returns `Err` if the file looks like a trampoline executable but is formatted incorrectly.
    ///
    /// Expects the following metadata to be at the end of the file:
    ///
    /// ```text
    /// - file path (no greater than 32KB)
    /// - file path length (u32)
    /// - magic number(4 bytes)
    /// ```
    ///
    /// This should only be used on Windows, but should just return `Ok(None)` on other platforms.
    ///
    /// This is an implementation of [`uv-trampoline::bounce::read_trampoline_metadata`] that
    /// returns errors instead of panicking. Unlike the utility there, we don't assume that the
    /// file we are reading is a trampoline.
    #[allow(clippy::cast_possible_wrap)]
    pub fn try_from_path(path: &Path) -> Result<Option<Self>, Error> {
        let mut file = File::open(path)?;

        // Read the magic number
        let Some(kind) = LauncherKind::try_from_file(&mut file)? else {
            return Ok(None);
        };

        // Seek to the start of the path length.
        let path_length_offset = (MAGIC_NUMBER_SIZE + PATH_LENGTH_SIZE) as i64;
        file.seek(io::SeekFrom::End(-path_length_offset))
            .map_err(|err| {
                Error::InvalidLauncherSeek("path length".to_string(), path_length_offset, err)
            })?;

        // Read the path length
        let mut buffer = [0; PATH_LENGTH_SIZE];
        file.read_exact(&mut buffer)
            .map_err(|err| Error::InvalidLauncherRead("path length".to_string(), err))?;

        let path_length = {
            let raw_length = u32::from_le_bytes(buffer);

            if raw_length > MAX_PATH_LENGTH {
                return Err(Error::InvalidPathLength(raw_length));
            }

            // SAFETY: Above we guarantee the length is less than 32KB
            raw_length as usize
        };

        // Seek to the start of the path
        let path_offset = (MAGIC_NUMBER_SIZE + PATH_LENGTH_SIZE + path_length) as i64;
        file.seek(io::SeekFrom::End(-path_offset)).map_err(|err| {
            Error::InvalidLauncherSeek("executable path".to_string(), path_offset, err)
        })?;

        // Read the path
        let mut buffer = vec![0u8; path_length];
        file.read_exact(&mut buffer)
            .map_err(|err| Error::InvalidLauncherRead("executable path".to_string(), err))?;

        let path = PathBuf::from(
            String::from_utf8(buffer).map_err(|err| Error::InvalidPath(err.utf8_error()))?,
        );

        Ok(Some(Self {
            kind,
            python_path: path,
        }))
    }
}

/// The kind of trampoline launcher to create.
///
/// See [`uv-trampoline::bounce::TrampolineKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherKind {
    /// The trampoline should execute itself, it's a zipped Python script.
    Script,
    /// The trampoline should just execute Python, it's a proxy Python executable.
    Python,
}

impl LauncherKind {
    /// Return the magic number for this [`LauncherKind`].
    const fn magic_number(self) -> &'static [u8; 4] {
        match self {
            Self::Script => b"UVSC",
            Self::Python => b"UVPY",
        }
    }

    /// Read a [`LauncherKind`] from 4 byte buffer.
    ///
    /// If the buffer does not contain a matching magic number, `None` is returned.
    fn try_from_bytes(bytes: [u8; MAGIC_NUMBER_SIZE]) -> Option<Self> {
        if &bytes == Self::Script.magic_number() {
            return Some(Self::Script);
        }
        if &bytes == Self::Python.magic_number() {
            return Some(Self::Python);
        }
        None
    }

    /// Read a [`LauncherKind`] from a file handle, based on the magic number.
    ///
    /// This will mutate the file handle, seeking to the end of the file.
    ///
    /// If the file cannot be read, an [`io::Error`] is returned. If the path is not a launcher,
    /// `None` is returned.
    #[allow(clippy::cast_possible_wrap)]
    pub fn try_from_file(file: &mut File) -> Result<Option<Self>, Error> {
        // If the file is less than four bytes, it's not a launcher.
        let Ok(_) = file.seek(io::SeekFrom::End(-(MAGIC_NUMBER_SIZE as i64))) else {
            return Ok(None);
        };

        let mut buffer = [0; MAGIC_NUMBER_SIZE];
        file.read_exact(&mut buffer)
            .map_err(|err| Error::InvalidLauncherRead("magic number".to_string(), err))?;

        Ok(Self::try_from_bytes(buffer))
    }
}

/// Note: The caller is responsible for adding the path of the wheel we're installing.
#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Only paths with a length up to 32KB are supported but found a length of {0} bytes")]
    InvalidPathLength(u32),
    #[error("Failed to parse executable path")]
    InvalidPath(#[source] Utf8Error),
    #[error("Failed to seek to {0} at offset {1}")]
    InvalidLauncherSeek(String, i64, #[source] io::Error),
    #[error("Failed to read launcher {0}")]
    InvalidLauncherRead(String, #[source] io::Error),
    #[error(
        "Unable to create Windows launcher for: {0} (only x86_64, x86, and arm64 are supported)"
    )]
    UnsupportedWindowsArch(&'static str),
    #[error("Unable to create Windows launcher on non-Windows platform")]
    NotWindows,
}

#[allow(clippy::unnecessary_wraps, unused_variables)]
fn get_launcher_bin(gui: bool) -> Result<&'static [u8], Error> {
    Ok(match std::env::consts::ARCH {
        #[cfg(all(windows, target_arch = "x86"))]
        "x86" => {
            if gui {
                LAUNCHER_I686_GUI
            } else {
                LAUNCHER_I686_CONSOLE
            }
        }
        #[cfg(all(windows, target_arch = "x86_64"))]
        "x86_64" => {
            if gui {
                LAUNCHER_X86_64_GUI
            } else {
                LAUNCHER_X86_64_CONSOLE
            }
        }
        #[cfg(all(windows, target_arch = "aarch64"))]
        "aarch64" => {
            if gui {
                LAUNCHER_AARCH64_GUI
            } else {
                LAUNCHER_AARCH64_CONSOLE
            }
        }
        #[cfg(windows)]
        arch => {
            return Err(Error::UnsupportedWindowsArch(arch));
        }
        #[cfg(not(windows))]
        _ => &[],
    })
}

/// A Windows script is a minimal .exe launcher binary with the python entrypoint script appended as
/// stored zip file.
///
/// <https://github.com/pypa/pip/blob/fd0ea6bc5e8cb95e518c23d901c26ca14db17f89/src/pip/_vendor/distlib/scripts.py#L248-L262>
#[allow(unused_variables)]
pub fn windows_script_launcher(
    launcher_python_script: &str,
    is_gui: bool,
    python_executable: impl AsRef<Path>,
) -> Result<Vec<u8>, Error> {
    // This method should only be called on Windows, but we avoid `#[cfg(windows)]` to retain
    // compilation on all platforms.
    if cfg!(not(windows)) {
        return Err(Error::NotWindows);
    }

    let launcher_bin: &[u8] = get_launcher_bin(is_gui)?;

    let mut payload: Vec<u8> = Vec::new();
    {
        // We're using the zip writer, but with stored compression
        // https://github.com/njsmith/posy/blob/04927e657ca97a5e35bb2252d168125de9a3a025/src/trampolines/mod.rs#L75-L82
        // https://github.com/pypa/distlib/blob/8ed03aab48add854f377ce392efffb79bb4d6091/PC/launcher.c#L259-L271
        let stored =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut archive = ZipWriter::new(Cursor::new(&mut payload));
        let error_msg = "Writing to Vec<u8> should never fail";
        archive.start_file("__main__.py", stored).expect(error_msg);
        archive
            .write_all(launcher_python_script.as_bytes())
            .expect(error_msg);
        archive.finish().expect(error_msg);
    }

    let python = python_executable.as_ref();
    let python_path = python.simplified_display().to_string();

    let mut launcher: Vec<u8> = Vec::with_capacity(launcher_bin.len() + payload.len());
    launcher.extend_from_slice(launcher_bin);
    launcher.extend_from_slice(&payload);
    launcher.extend_from_slice(python_path.as_bytes());
    launcher.extend_from_slice(
        &u32::try_from(python_path.len())
            .expect("file path should be smaller than 4GB")
            .to_le_bytes(),
    );
    launcher.extend_from_slice(LauncherKind::Script.magic_number());

    Ok(launcher)
}

/// A minimal .exe launcher binary for Python.
///
/// Sort of equivalent to a `python` symlink on Unix.
#[allow(unused_variables)]
pub fn windows_python_launcher(
    python_executable: impl AsRef<Path>,
    is_gui: bool,
) -> Result<Vec<u8>, Error> {
    // This method should only be called on Windows, but we avoid `#[cfg(windows)]` to retain
    // compilation on all platforms.
    if cfg!(not(windows)) {
        return Err(Error::NotWindows);
    }

    let launcher_bin: &[u8] = get_launcher_bin(is_gui)?;

    let python = python_executable.as_ref();
    let python_path = python.simplified_display().to_string();

    let mut launcher: Vec<u8> = Vec::with_capacity(launcher_bin.len());
    launcher.extend_from_slice(launcher_bin);
    launcher.extend_from_slice(python_path.as_bytes());
    launcher.extend_from_slice(
        &u32::try_from(python_path.len())
            .expect("file path should be smaller than 4GB")
            .to_le_bytes(),
    );
    launcher.extend_from_slice(LauncherKind::Python.magic_number());

    Ok(launcher)
}

#[cfg(all(test, windows))]
#[allow(clippy::print_stdout)]
mod test {
    use std::io::Write;
    use std::path::Path;
    use std::process::Command;

    use anyhow::Result;
    use assert_cmd::prelude::OutputAssertExt;
    use assert_fs::prelude::PathChild;
    use fs_err::File;

    use which::which;

    use super::{windows_python_launcher, windows_script_launcher, Launcher, LauncherKind};

    #[test]
    #[cfg(all(windows, target_arch = "x86", feature = "production"))]
    fn test_launchers_are_small() {
        // At time of writing, they are ~45kb.
        assert!(
            super::LAUNCHER_I686_GUI.len() < 45 * 1024,
            "GUI launcher: {}",
            super::LAUNCHER_I686_GUI.len()
        );
        assert!(
            super::LAUNCHER_I686_CONSOLE.len() < 45 * 1024,
            "CLI launcher: {}",
            super::LAUNCHER_I686_CONSOLE.len()
        );
    }

    #[test]
    #[cfg(all(windows, target_arch = "x86_64", feature = "production"))]
    fn test_launchers_are_small() {
        // At time of writing, they are ~45kb.
        assert!(
            super::LAUNCHER_X86_64_GUI.len() < 45 * 1024,
            "GUI launcher: {}",
            super::LAUNCHER_X86_64_GUI.len()
        );
        assert!(
            super::LAUNCHER_X86_64_CONSOLE.len() < 45 * 1024,
            "CLI launcher: {}",
            super::LAUNCHER_X86_64_CONSOLE.len()
        );
    }

    #[test]
    #[cfg(all(windows, target_arch = "aarch64", feature = "production"))]
    fn test_launchers_are_small() {
        // At time of writing, they are ~45kb.
        assert!(
            super::LAUNCHER_AARCH64_GUI.len() < 45 * 1024,
            "GUI launcher: {}",
            super::LAUNCHER_AARCH64_GUI.len()
        );
        assert!(
            super::LAUNCHER_AARCH64_CONSOLE.len() < 45 * 1024,
            "CLI launcher: {}",
            super::LAUNCHER_AARCH64_CONSOLE.len()
        );
    }

    /// Utility script for the test.
    fn get_script_launcher(shebang: &str, is_gui: bool) -> String {
        if is_gui {
            format!(
                r#"{shebang}
# -*- coding: utf-8 -*-
import re
import sys

def make_gui() -> None:
    from tkinter import Tk, ttk
    root = Tk()
    root.title("uv Test App")
    frm = ttk.Frame(root, padding=10)
    frm.grid()
    ttk.Label(frm, text="Hello from uv-trampoline-gui.exe").grid(column=0, row=0)
    root.mainloop()

if __name__ == "__main__":
    sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
    sys.exit(make_gui())
"#
            )
        } else {
            format!(
                r#"{shebang}
# -*- coding: utf-8 -*-
import re
import sys

def main_console() -> None:
    print("Hello from uv-trampoline-console.exe", file=sys.stdout)
    print("Hello from uv-trampoline-console.exe", file=sys.stderr)
    for arg in sys.argv[1:]:
        print(arg, file=sys.stderr)

if __name__ == "__main__":
    sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
    sys.exit(main_console())
"#
            )
        }
    }

    /// See [`uv-install-wheel::wheel::format_shebang`].
    fn format_shebang(executable: impl AsRef<Path>) -> String {
        // Convert the executable to a simplified path.
        let executable = executable.as_ref().display().to_string();
        format!("#!{executable}")
    }

    #[test]
    fn console_script_launcher() -> Result<()> {
        // Create Temp Dirs
        let temp_dir = assert_fs::TempDir::new()?;
        let console_bin_path = temp_dir.child("launcher.console.exe");

        // Locate an arbitrary python installation from PATH
        let python_executable_path = which("python")?;

        // Generate Launcher Script
        let launcher_console_script =
            get_script_launcher(&format_shebang(&python_executable_path), false);

        // Generate Launcher Payload
        let console_launcher =
            windows_script_launcher(&launcher_console_script, false, &python_executable_path)?;

        // Create Launcher
        File::create(console_bin_path.path())?.write_all(console_launcher.as_ref())?;

        println!(
            "Wrote Console Launcher in {}",
            console_bin_path.path().display()
        );

        let stdout_predicate = "Hello from uv-trampoline-console.exe\r\n";
        let stderr_predicate = "Hello from uv-trampoline-console.exe\r\n";

        // Test Console Launcher
        #[cfg(windows)]
        Command::new(console_bin_path.path())
            .assert()
            .success()
            .stdout(stdout_predicate)
            .stderr(stderr_predicate);

        let args_to_test = vec!["foo", "bar", "foo bar", "foo \"bar\"", "foo 'bar'"];
        let stderr_predicate = format!("{}{}\r\n", stderr_predicate, args_to_test.join("\r\n"));

        // Test Console Launcher (with args)
        Command::new(console_bin_path.path())
            .args(args_to_test)
            .assert()
            .success()
            .stdout(stdout_predicate)
            .stderr(stderr_predicate);

        let launcher = Launcher::try_from_path(console_bin_path.path())
            .expect("We should succeed at reading the launcher")
            .expect("The launcher should be valid");

        assert!(launcher.kind == LauncherKind::Script);
        assert!(launcher.python_path == python_executable_path);

        Ok(())
    }

    #[test]
    fn console_python_launcher() -> Result<()> {
        // Create Temp Dirs
        let temp_dir = assert_fs::TempDir::new()?;
        let console_bin_path = temp_dir.child("launcher.console.exe");

        // Locate an arbitrary python installation from PATH
        let python_executable_path = which("python")?;

        // Generate Launcher Payload
        let console_launcher = windows_python_launcher(&python_executable_path, false)?;

        // Create Launcher
        File::create(console_bin_path.path())?.write_all(console_launcher.as_ref())?;

        println!(
            "Wrote Python Launcher in {}",
            console_bin_path.path().display()
        );

        // Test Console Launcher
        Command::new(console_bin_path.path())
            .arg("-c")
            .arg("print('Hello from Python Launcher')")
            .assert()
            .success()
            .stdout("Hello from Python Launcher\r\n");

        let launcher = Launcher::try_from_path(console_bin_path.path())
            .expect("We should succeed at reading the launcher")
            .expect("The launcher should be valid");

        assert!(launcher.kind == LauncherKind::Python);
        assert!(launcher.python_path == python_executable_path);

        Ok(())
    }

    #[test]
    #[ignore]
    fn gui_launcher() -> Result<()> {
        // Create Temp Dirs
        let temp_dir = assert_fs::TempDir::new()?;
        let gui_bin_path = temp_dir.child("launcher.gui.exe");

        // Locate an arbitrary pythonw installation from PATH
        let pythonw_executable_path = which("pythonw")?;

        // Generate Launcher Script
        let launcher_gui_script =
            get_script_launcher(&format_shebang(&pythonw_executable_path), true);

        // Generate Launcher Payload
        let gui_launcher =
            windows_script_launcher(&launcher_gui_script, true, &pythonw_executable_path)?;

        // Create Launcher
        File::create(gui_bin_path.path())?.write_all(gui_launcher.as_ref())?;

        println!("Wrote GUI Launcher in {}", gui_bin_path.path().display());

        // Test GUI Launcher
        // NOTICE: This will spawn a GUI and will wait until you close the window.
        Command::new(gui_bin_path.path()).assert().success();

        Ok(())
    }
}
