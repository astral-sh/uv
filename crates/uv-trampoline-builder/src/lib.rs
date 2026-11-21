use std::io;
use std::path::{Path, PathBuf};
use std::str::Utf8Error;

#[cfg(windows)]
use editpe::{
    Image, ResourceData, ResourceDirectory, ResourceEntry, ResourceEntryName, ResourceTable,
};
#[cfg(windows)]
use fs_err::File;
use thiserror::Error;

use uv_fs::Simplified;

#[cfg(windows)]
mod resource_reader;
#[cfg(all(test, windows))]
mod resource_tests;

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

// https://learn.microsoft.com/en-us/windows/win32/menurc/resource-types
#[cfg(windows)]
const RT_RCDATA: u32 = 10;

// Resource names matching uv-trampoline
#[cfg(windows)]
const RESOURCE_TRAMPOLINE_KIND: &str = "UV_TRAMPOLINE_KIND";
#[cfg(windows)]
const RESOURCE_PYTHON_PATH: &str = "UV_PYTHON_PATH";
// Note: This does not need to be looked up as a resource, as we rely on `zipimport`
// to do the loading work. Still, keeping the content under a resource means that it
// sits nicely under the PE format.
#[cfg(windows)]
const RESOURCE_SCRIPT_DATA: &str = "UV_SCRIPT_DATA";

// An extended-length Windows path has at most 32,767 UTF-16 code units,
// each contributing at most three bytes to the UTF-8 resource.
#[cfg(windows)]
const MAX_PYTHON_PATH_BYTES: usize = 3 * 32_767;

/// Read the launcher kind and interpreter path without inspecting the script payload.
/// Return `Ok(None)` if the file has no launcher kind resource. Return an error
/// if the resource lookup is inconclusive or the interpreter cannot be resolved
/// to a single absolute path. Windows may select a target based on the user's
/// language or working directory.
#[cfg(windows)]
pub fn read_launcher_target(path: &Path) -> Result<Option<(LauncherKind, PathBuf)>, Error> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let Some((kind, mut target)) = resource_reader::read_stored_target(file)? else {
        return Ok(None);
    };
    if target.is_relative() {
        // The trampoline joins the stored path to the parent of current_exe().
        let executable = std::path::absolute(path)?;
        let parent = executable.parent().ok_or(Error::UnprocessableMetadata)?;
        target = parent.join(target);
        // A drive-relative target such as C:python.exe depends on the
        // working directory for that drive even after the join.
        if target.is_relative() {
            return Err(Error::UnprocessableMetadata);
        }
    }
    Ok(Some((kind, target)))
}

/// Returns `Ok(None)` on platforms without Windows trampolines.
#[cfg(not(windows))]
pub fn read_launcher_target(_path: &Path) -> Result<Option<(LauncherKind, PathBuf)>, Error> {
    Ok(None)
}

#[cfg(windows)]
#[derive(Debug)]
pub struct Launcher {
    kind: LauncherKind,
    window_mode: WindowMode,
    python_path: String,
    script_data: Option<Vec<u8>>,
}

#[cfg(windows)]
impl Launcher {
    /// Read a trampoline executable for rewriting.
    /// Use [`read_launcher_target`] to identify its interpreter independently of rewrite eligibility.
    ///
    /// Returns `Ok(None)` if the file cannot be identified as a rewritable trampoline.
    /// Returns `Err` if reading or decoding a located launcher payload fails.
    pub fn try_from_path(path: &Path) -> Result<Option<Self>, Error> {
        let Ok(file) = File::open(path) else {
            return Ok(None);
        };
        resource_reader::read_launcher(file)
    }

    /// Write this trampoline launcher to a file.
    pub fn write_to_file(&self, file: &mut File) -> Result<(), Error> {
        use std::io::Write;

        let launcher_bin = get_launcher_bin(self.window_mode == WindowMode::Windowed)?;

        let kind_value = [self.kind.to_resource_value()];
        let mut resources: Vec<(&str, &[u8])> = vec![
            (RESOURCE_TRAMPOLINE_KIND, &kind_value),
            (RESOURCE_PYTHON_PATH, self.python_path.as_bytes()),
        ];
        if let Some(payload) = &self.script_data {
            resources.push((RESOURCE_SCRIPT_DATA, payload));
        }

        let output = write_resources(launcher_bin, &resources)?;
        file.write_all(&output)?;
        Ok(())
    }

    /// The interpreter path stored in the launcher.
    pub fn python_path(&self) -> &Path {
        Path::new(&self.python_path)
    }

    /// Encode a replacement path in the launcher's UTF-8 resource.
    pub fn with_python_path(mut self, path: impl AsRef<Path>) -> Result<Self, Error> {
        encode_python_path(&path)?.clone_into(&mut self.python_path);
        Ok(self)
    }
}

/// The window mode of a Windows trampoline launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    /// The launcher runs attached to a console.
    Console,
    /// The launcher runs without opening a console window.
    Windowed,
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

#[cfg(windows)]
impl LauncherKind {
    fn to_resource_value(self) -> u8 {
        match self {
            Self::Script => 1,
            Self::Python => 2,
        }
    }

    fn from_resource_value(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Script),
            2 => Some(Self::Python),
            _ => None,
        }
    }
}

/// Note: The caller is responsible for adding the path of the wheel we're installing.
#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Failed to parse executable path")]
    InvalidPath(#[source] Utf8Error),
    #[cfg(windows)]
    #[error("A Windows launcher requires a nonempty Python path")]
    EmptyPath,
    #[cfg(windows)]
    #[error("The Python path in a Windows launcher cannot contain a NUL character")]
    NulInPath,
    #[cfg(windows)]
    #[error("Cannot encode the Python path `{}` as UTF-8 in a Windows launcher", path.user_display())]
    NonUtf8Path { path: PathBuf },
    #[cfg(windows)]
    #[error("The Python path is too long for a Windows launcher")]
    PathTooLong,
    #[error(
        "Unable to create Windows launcher for: {0} (only x86_64, x86, and arm64 are supported)"
    )]
    UnsupportedWindowsArch(&'static str),
    #[error("Unable to create Windows launcher on non-Windows platform")]
    NotWindows,
    #[error("Cannot process launcher metadata from resource")]
    UnprocessableMetadata,
    #[cfg(windows)]
    #[error("Failed to write Windows launcher ZIP payload")]
    AsyncZip(#[from] async_zip::error::ZipError),
    #[error("Resources over 2^32 bytes are not supported")]
    ResourceTooLarge,
    #[error("Failed to update Windows PE resources: {}", path.user_display())]
    WriteResources {
        path: PathBuf,
        #[source]
        err: io::Error,
    },
    #[cfg(windows)]
    #[error("Failed to parse Windows PE image")]
    PeRead(#[from] editpe::ImageReadError),
    #[cfg(windows)]
    #[error("Failed to update Windows PE resources")]
    PeWrite(#[from] editpe::ImageWriteError),
}

#[allow(clippy::unnecessary_wraps, unused_variables)]
#[cfg(windows)]
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
    })
}

/// Write PE resources into a launcher binary.
#[cfg(windows)]
fn write_resources(launcher_data: &[u8], resources: &[(&str, &[u8])]) -> Result<Vec<u8>, Error> {
    let mut image = Image::parse(launcher_data.to_vec())?;

    let mut resource_directory = image
        .resource_directory()
        .cloned()
        .unwrap_or_else(ResourceDirectory::default);
    let root = resource_directory.root_mut();

    // Get or create the RT_RCDATA type entry.
    let rcdata_name = ResourceEntryName::ID(RT_RCDATA);
    if root.get(rcdata_name.clone()).is_none() {
        root.insert(
            rcdata_name.clone(),
            ResourceEntry::Table(ResourceTable::default()),
        );
    }
    let rcdata_table = root
        .get_mut(rcdata_name)
        .and_then(ResourceEntry::as_table_mut)
        .expect("RT_RCDATA entry was just inserted");

    for (name, data) in resources {
        let entry_name = ResourceEntryName::from_string(name);

        // Create language table with neutral language (0).
        let mut language_table = ResourceTable::default();
        let mut resource_data = ResourceData::default();
        resource_data.set_data(data.to_vec());
        language_table.insert(ResourceEntryName::ID(0), ResourceEntry::Data(resource_data));

        rcdata_table.insert(entry_name, ResourceEntry::Table(language_table));
    }

    image.set_resource_directory(resource_directory)?;

    Ok(image.data().to_vec())
}

#[cfg(windows)]
fn encode_python_path(path: &impl AsRef<Path>) -> Result<&str, Error> {
    let path = path
        .simplified()
        .to_str()
        .ok_or_else(|| Error::NonUtf8Path {
            path: path.as_ref().to_path_buf(),
        })?;
    validate_python_path(path)?;
    Ok(path)
}

#[cfg(windows)]
fn validate_python_path(path: &str) -> Result<(), Error> {
    if path.is_empty() {
        return Err(Error::EmptyPath);
    }
    if path.len() > MAX_PYTHON_PATH_BYTES {
        return Err(Error::PathTooLong);
    }
    // The trampoline passes the path in a NUL-terminated command line.
    if path.contains('\0') {
        return Err(Error::NulInPath);
    }
    Ok(())
}

/// Construct a Windows script launcher.
///
/// On Unix, this always returns [`Error::NotWindows`]. Trampolines are a Windows-specific feature
/// and cannot be created on other platforms.
#[cfg(not(windows))]
pub fn windows_script_launcher(
    _launcher_python_script: &str,
    _is_gui: bool,
    _python_executable: impl AsRef<Path>,
) -> Result<Vec<u8>, Error> {
    Err(Error::NotWindows)
}

/// Construct a Windows script launcher.
///
/// A Windows script is a minimal .exe launcher binary with the python entrypoint script appended as
/// stored zip file.
///
/// <https://github.com/pypa/pip/blob/fd0ea6bc5e8cb95e518c23d901c26ca14db17f89/src/pip/_vendor/distlib/scripts.py#L248-L262>
#[cfg(windows)]
pub fn windows_script_launcher(
    launcher_python_script: &str,
    is_gui: bool,
    python_executable: impl AsRef<Path>,
) -> Result<Vec<u8>, Error> {
    use async_zip::base::write::ZipFileWriter;
    use async_zip::{Compression, ZipEntryBuilder};
    use futures_lite::future::block_on;
    use futures_lite::io::Cursor;

    let launcher_bin: &[u8] = get_launcher_bin(is_gui)?;
    let python_path = encode_python_path(&python_executable)?;

    // Store the launcher script without compression.
    // https://github.com/njsmith/posy/blob/04927e657ca97a5e35bb2252d168125de9a3a025/src/trampolines/mod.rs#L75-L82
    // https://github.com/pypa/distlib/blob/8ed03aab48add854f377ce392efffb79bb4d6091/PC/launcher.c#L259-L271
    let mut archive = ZipFileWriter::new(Cursor::new(Vec::new()));
    let entry = ZipEntryBuilder::new("__main__.py".to_string().into(), Compression::Stored);
    block_on(archive.write_entry_whole(entry, launcher_python_script.as_bytes()))?;
    let payload = block_on(archive.close())?.into_inner();

    let resources: &[(&str, &[u8])] = &[
        (
            RESOURCE_TRAMPOLINE_KIND,
            &[LauncherKind::Script.to_resource_value()],
        ),
        (RESOURCE_PYTHON_PATH, python_path.as_bytes()),
        (RESOURCE_SCRIPT_DATA, &payload),
    ];

    write_resources(launcher_bin, resources)
}

/// Construct a Windows Python launcher.
///
/// On Unix, this always returns [`Error::NotWindows`]. Trampolines are a Windows-specific feature
/// and cannot be created on other platforms.
#[cfg(not(windows))]
pub fn windows_python_launcher(
    _python_executable: impl AsRef<Path>,
    _window_mode: WindowMode,
) -> Result<Vec<u8>, Error> {
    Err(Error::NotWindows)
}

/// Construct a Windows Python launcher.
///
/// A minimal .exe launcher binary for Python.
///
/// Sort of equivalent to a `python` symlink on Unix.
#[cfg(windows)]
pub fn windows_python_launcher(
    python_executable: impl AsRef<Path>,
    window_mode: WindowMode,
) -> Result<Vec<u8>, Error> {
    let launcher_bin: &[u8] = get_launcher_bin(matches!(window_mode, WindowMode::Windowed))?;

    let python_path = encode_python_path(&python_executable)?;

    let resources: &[(&str, &[u8])] = &[
        (
            RESOURCE_TRAMPOLINE_KIND,
            &[LauncherKind::Python.to_resource_value()],
        ),
        (RESOURCE_PYTHON_PATH, python_path.as_bytes()),
    ];

    write_resources(launcher_bin, resources)
}

#[cfg(all(test, windows))]
#[expect(clippy::print_stdout)]
mod test {
    use std::io::Write;
    use std::path::Path;
    use std::path::PathBuf;
    use std::process::Command;

    use anyhow::Result;
    use assert_cmd::prelude::OutputAssertExt;
    use assert_fs::prelude::PathChild;
    use fs_err::File;

    use which::which;

    use super::{
        Launcher, LauncherKind, WindowMode, windows_python_launcher, windows_script_launcher,
    };

    #[test]
    #[cfg(all(windows, target_arch = "x86", feature = "production"))]
    fn test_launchers_are_small() {
        // At time of writing, they are ~40kb.
        assert!(
            super::LAUNCHER_I686_GUI.len() < 50 * 1024,
            "GUI launcher: {}",
            super::LAUNCHER_I686_GUI.len()
        );
        assert!(
            super::LAUNCHER_I686_CONSOLE.len() < 50 * 1024,
            "CLI launcher: {}",
            super::LAUNCHER_I686_CONSOLE.len()
        );
    }

    #[test]
    #[cfg(all(windows, target_arch = "x86_64", feature = "production"))]
    fn test_launchers_are_small() {
        // At time of writing, they are ~45kb.
        assert!(
            super::LAUNCHER_X86_64_GUI.len() < 50 * 1024,
            "GUI launcher: {}",
            super::LAUNCHER_X86_64_GUI.len()
        );
        assert!(
            super::LAUNCHER_X86_64_CONSOLE.len() < 50 * 1024,
            "CLI launcher: {}",
            super::LAUNCHER_X86_64_CONSOLE.len()
        );
    }

    #[test]
    #[cfg(all(windows, target_arch = "aarch64", feature = "production"))]
    fn test_launchers_are_small() {
        // At time of writing, they are ~45kb.
        assert!(
            super::LAUNCHER_AARCH64_GUI.len() < 50 * 1024,
            "GUI launcher: {}",
            super::LAUNCHER_AARCH64_GUI.len()
        );
        assert!(
            super::LAUNCHER_AARCH64_CONSOLE.len() < 50 * 1024,
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

    /// Creates a self-signed certificate and returns its path.
    fn create_temp_certificate(temp_dir: &tempfile::TempDir) -> Result<(PathBuf, PathBuf)> {
        use rcgen::{
            CertificateParams, DnType, ExtendedKeyUsagePurpose, KeyPair, KeyUsagePurpose, SanType,
        };

        let mut params = CertificateParams::default();
        params.key_usages.push(KeyUsagePurpose::DigitalSignature);
        params
            .extended_key_usages
            .push(ExtendedKeyUsagePurpose::CodeSigning);
        params
            .distinguished_name
            .push(DnType::OrganizationName, "Astral Software Inc.");
        params
            .distinguished_name
            .push(DnType::CommonName, "uv-test-signer");
        params
            .subject_alt_names
            .push(SanType::DnsName("uv-test-signer".try_into()?));

        let private_key = KeyPair::generate()?;
        let public_cert = params.self_signed(&private_key)?;

        let public_cert_path = temp_dir.path().join("uv-trampoline-test.crt");
        let private_key_path = temp_dir.path().join("uv-trampoline-test.key");
        fs_err::write(public_cert_path.as_path(), public_cert.pem())?;
        fs_err::write(private_key_path.as_path(), private_key.serialize_pem())?;

        Ok((public_cert_path, private_key_path))
    }

    /// Signs the given binary using `PowerShell`'s `Set-AuthenticodeSignature` with a temporary certificate.
    fn sign_authenticode(bin_path: impl AsRef<Path>) {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temporary directory");
        let (public_cert, private_key) =
            create_temp_certificate(&temp_dir).expect("Failed to create self-signed certificate");

        // Instead of powershell, we rely on pwsh which supports CreateFromPemFile.
        Command::new("pwsh")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!(
                    r"
                    $ErrorActionPreference = 'Stop'
                    Import-Module Microsoft.PowerShell.Security
                    $cert = [System.Security.Cryptography.X509Certificates.X509Certificate2]::CreateFromPemFile('{}', '{}')
                    Set-AuthenticodeSignature -FilePath '{}' -Certificate $cert;
                    ",
                    public_cert.display().to_string().replace('\'', "''"),
                    private_key.display().to_string().replace('\'', "''"),
                    bin_path.as_ref().display().to_string().replace('\'', "''"),
                ),
            ])
            .env_remove("PSModulePath")
            .assert()
            .success();

        println!("Signed binary: {}", bin_path.as_ref().display());
    }

    #[test]
    fn malformed_trampoline_is_not_recognized() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let launcher_path = temp_dir.child("malformed.exe");
        fs_err::write(launcher_path.path(), b"MZ")?;

        assert!(Launcher::try_from_path(launcher_path.path())?.is_none());

        Ok(())
    }

    #[test]
    fn empty_kind_resource_is_rejected() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let launcher_path = temp_dir.child("launcher.console.exe");

        let launcher = super::write_resources(
            super::get_launcher_bin(false)?,
            &[
                (super::RESOURCE_TRAMPOLINE_KIND, &[]),
                (super::RESOURCE_PYTHON_PATH, b"C:/Python312/python.exe"),
            ],
        )?;

        File::create(launcher_path.path())?.write_all(&launcher)?;

        let error = Launcher::try_from_path(launcher_path.path())
            .expect_err("Empty launcher kind resources should be rejected");
        assert!(matches!(error, super::Error::UnprocessableMetadata));

        Ok(())
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

        assert_eq!(launcher.kind, LauncherKind::Script);
        assert_eq!(launcher.python_path(), python_executable_path);

        // Now code-sign the launcher and verify that it still works.
        sign_authenticode(console_bin_path.path());

        let stdout_predicate = "Hello from uv-trampoline-console.exe\r\n";
        let stderr_predicate = "Hello from uv-trampoline-console.exe\r\n";
        Command::new(console_bin_path.path())
            .assert()
            .success()
            .stdout(stdout_predicate)
            .stderr(stderr_predicate);

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
        let console_launcher =
            windows_python_launcher(&python_executable_path, WindowMode::Console)?;

        // Create Launcher
        {
            File::create(console_bin_path.path())?.write_all(console_launcher.as_ref())?;
        }

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

        assert_eq!(launcher.kind, LauncherKind::Python);
        assert_eq!(launcher.python_path(), python_executable_path);

        // Now code-sign the launcher and verify that it still works.
        sign_authenticode(console_bin_path.path());
        Command::new(console_bin_path.path())
            .arg("-c")
            .arg("print('Hello from Python Launcher')")
            .assert()
            .success()
            .stdout("Hello from Python Launcher\r\n");

        Ok(())
    }

    #[test]
    #[ignore = "This test will spawn a GUI and wait until you close the window."]
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
        {
            File::create(gui_bin_path.path())?.write_all(gui_launcher.as_ref())?;
        }

        println!("Wrote GUI Launcher in {}", gui_bin_path.path().display());

        // Test GUI Launcher
        // NOTICE: This will spawn a GUI and will wait until you close the window.
        Command::new(gui_bin_path.path()).assert().success();

        Ok(())
    }
}
