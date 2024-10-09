use std::io::{Cursor, Write};
use std::path::Path;
use std::process::Command;
use std::{env, io};

use anyhow::Result;
use assert_cmd::prelude::OutputAssertExt;
use assert_fs::prelude::PathChild;
use fs_err::File;
use thiserror::Error;
use which::which;
use zip::write::FileOptions;
use zip::ZipWriter;

const LAUNCHER_MAGIC_NUMBER: [u8; 4] = [b'U', b'V', b'U', b'V'];

#[cfg(all(windows, target_arch = "x86"))]
const LAUNCHER_I686_GUI: &[u8] = include_bytes!("../trampolines/uv-trampoline-i686-gui.exe");

#[cfg(all(windows, target_arch = "x86"))]
const LAUNCHER_I686_CONSOLE: &[u8] =
    include_bytes!("../trampolines/uv-trampoline-i686-console.exe");

#[cfg(all(windows, target_arch = "x86_64"))]
const LAUNCHER_X86_64_GUI: &[u8] = include_bytes!("../trampolines/uv-trampoline-x86_64-gui.exe");

#[cfg(all(windows, target_arch = "x86_64"))]
const LAUNCHER_X86_64_CONSOLE: &[u8] =
    include_bytes!("../trampolines/uv-trampoline-x86_64-console.exe");

#[cfg(all(windows, target_arch = "aarch64"))]
const LAUNCHER_AARCH64_GUI: &[u8] = include_bytes!("../trampolines/uv-trampoline-aarch64-gui.exe");

#[cfg(all(windows, target_arch = "aarch64"))]
const LAUNCHER_AARCH64_CONSOLE: &[u8] =
    include_bytes!("../trampolines/uv-trampoline-aarch64-console.exe");

/// Note: The caller is responsible for adding the path of the wheel we're installing.
#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(
        "Unable to create Windows launcher for: {0} (only x86_64, x86, and arm64 are supported)"
    )]
    UnsupportedWindowsArch(&'static str),
    #[error("Unable to create Windows launcher on non-Windows platform")]
    NotWindows,
}

/// Wrapper script template function
///
/// <https://github.com/pypa/pip/blob/7f8a6844037fb7255cfd0d34ff8e8cf44f2598d4/src/pip/_vendor/distlib/scripts.py#L41-L48>
fn get_script_launcher(shebang: &str, is_gui: bool) -> String {
    if is_gui {
        format!(
            r##"{shebang}
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
"##
        )
    } else {
        format!(
            r##"{shebang}
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
"##
        )
    }
}

/// Format the shebang for a given Python executable.
///
/// Like pip, if a shebang is non-simple (too long or contains spaces), we use `/bin/sh` as the
/// executable.
///
/// See: <https://github.com/pypa/pip/blob/0ad4c94be74cc24874c6feb5bb3c2152c398a18e/src/pip/_vendor/distlib/scripts.py#L136-L165>
fn format_shebang(executable: impl AsRef<Path>) -> String {
    // Convert the executable to a simplified path.
    let executable = executable.as_ref().display().to_string();
    format!("#!{executable}")
}

/// A Windows script is a minimal .exe launcher binary with the python entrypoint script appended as
/// stored zip file.
///
/// <https://github.com/pypa/pip/blob/fd0ea6bc5e8cb95e518c23d901c26ca14db17f89/src/pip/_vendor/distlib/scripts.py#L248-L262>
#[allow(unused_variables)]
fn windows_script_launcher(
    launcher_python_script: &str,
    is_gui: bool,
    python_executable: impl AsRef<Path>,
) -> Result<Vec<u8>, Error> {
    // This method should only be called on Windows, but we avoid `#[cfg(windows)]` to retain
    // compilation on all platforms.
    if cfg!(not(windows)) {
        return Err(Error::NotWindows);
    }

    let launcher_bin: &[u8] = match env::consts::ARCH {
        #[cfg(all(windows, target_arch = "x86"))]
        "x86" => {
            if is_gui {
                LAUNCHER_I686_GUI
            } else {
                LAUNCHER_I686_CONSOLE
            }
        }
        #[cfg(all(windows, target_arch = "x86_64"))]
        "x86_64" => {
            if is_gui {
                LAUNCHER_X86_64_GUI
            } else {
                LAUNCHER_X86_64_CONSOLE
            }
        }
        #[cfg(all(windows, target_arch = "aarch64"))]
        "aarch64" => {
            if is_gui {
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
        arch => &[],
    };

    let mut payload: Vec<u8> = Vec::new();
    {
        // We're using the zip writer, but with stored compression
        // https://github.com/njsmith/posy/blob/04927e657ca97a5e35bb2252d168125de9a3a025/src/trampolines/mod.rs#L75-L82
        // https://github.com/pypa/distlib/blob/8ed03aab48add854f377ce392efffb79bb4d6091/PC/launcher.c#L259-L271
        let stored = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut archive = ZipWriter::new(Cursor::new(&mut payload));
        let error_msg = "Writing to Vec<u8> should never fail";
        archive.start_file("__main__.py", stored).expect(error_msg);
        archive
            .write_all(launcher_python_script.as_bytes())
            .expect(error_msg);
        archive.finish().expect(error_msg);
    }

    let python = python_executable.as_ref();
    let python_path = python.display().to_string();

    let mut launcher: Vec<u8> = Vec::with_capacity(launcher_bin.len() + payload.len());
    launcher.extend_from_slice(launcher_bin);
    launcher.extend_from_slice(&payload);
    launcher.extend_from_slice(python_path.as_bytes());
    launcher.extend_from_slice(
        &u32::try_from(python_path.as_bytes().len())
            .expect("File Path to be smaller than 4GB")
            .to_le_bytes(),
    );
    launcher.extend_from_slice(&LAUNCHER_MAGIC_NUMBER);

    Ok(launcher)
}

#[test]
fn generate_console_launcher() -> Result<()> {
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
    #[cfg(windows)]
    Command::new(console_bin_path.path())
        .args(args_to_test)
        .assert()
        .success()
        .stdout(stdout_predicate)
        .stderr(stderr_predicate);

    Ok(())
}

#[test]
#[ignore]
fn generate_gui_launcher() -> Result<()> {
    // Create Temp Dirs
    let temp_dir = assert_fs::TempDir::new()?;
    let gui_bin_path = temp_dir.child("launcher.gui.exe");

    // Locate an arbitrary pythonw installation from PATH
    let pythonw_executable_path = which("pythonw")?;

    // Generate Launcher Script
    let launcher_gui_script = get_script_launcher(&format_shebang(&pythonw_executable_path), true);

    // Generate Launcher Payload
    let gui_launcher =
        windows_script_launcher(&launcher_gui_script, true, &pythonw_executable_path)?;

    // Create Launcher
    File::create(gui_bin_path.path())?.write_all(gui_launcher.as_ref())?;

    println!("Wrote GUI Launcher in {}", gui_bin_path.path().display());

    // Test GUI Launcher
    // NOTICE: This will spawn a GUI and will wait until you close the window.
    #[cfg(windows)]
    Command::new(gui_bin_path.path()).assert().success();

    Ok(())
}
