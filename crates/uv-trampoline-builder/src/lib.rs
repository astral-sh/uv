use std::io;
use std::path::{Path, PathBuf};
use std::str::Utf8Error;

use fs_err::File;
use thiserror::Error;

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

#[derive(Debug)]
pub struct Launcher {
    pub kind: LauncherKind,
    pub python_path: PathBuf,
    pub script_data: Option<Vec<u8>>,
}

impl Launcher {
    /// Attempt to read [`Launcher`] metadata from a trampoline executable file.
    ///
    /// On Unix, this always returns [`None`]. Trampolines are a Windows-specific feature and cannot
    /// be read on other platforms.
    #[cfg(not(windows))]
    pub fn try_from_path(_path: &Path) -> Result<Option<Self>, Error> {
        Ok(None)
    }

    /// Read [`Launcher`] metadata from a trampoline executable file.
    ///
    /// Returns `Ok(None)` if the file is not a trampoline executable.
    /// Returns `Err` if the file looks like a trampoline executable but is formatted incorrectly.
    #[cfg(windows)]
    pub fn try_from_path(path: &Path) -> Result<Option<Self>, Error> {
        let data = fs_err::read(path)?;
        let Ok(image) = editpe::Image::parse(data) else {
            return Ok(None);
        };

        let Some(kind_data) = read_resource_from_image(&image, RESOURCE_TRAMPOLINE_KIND) else {
            return Ok(None);
        };
        let Some(&kind_value) = kind_data.first() else {
            return Err(Error::UnprocessableMetadata);
        };
        let Some(kind) = LauncherKind::from_resource_value(kind_value) else {
            return Err(Error::UnprocessableMetadata);
        };

        let Some(path_data) = read_resource_from_image(&image, RESOURCE_PYTHON_PATH) else {
            return Ok(None);
        };
        let python_path = PathBuf::from(
            String::from_utf8(path_data).map_err(|err| Error::InvalidPath(err.utf8_error()))?,
        );

        let script_data = read_resource_from_image(&image, RESOURCE_SCRIPT_DATA);

        Ok(Some(Self {
            kind,
            python_path,
            script_data,
        }))
    }

    /// Write this trampoline launcher to a file.
    ///
    /// On Unix, this always returns [`Error::NotWindows`]. Trampolines are a Windows-specific
    /// feature and cannot be written on other platforms.
    #[cfg(not(windows))]
    pub fn write_to_file(self, _file: &mut File, _is_gui: bool) -> Result<(), Error> {
        Err(Error::NotWindows)
    }

    /// Write this trampoline launcher to a file.
    #[cfg(windows)]
    pub fn write_to_file(self, file: &mut File, is_gui: bool) -> Result<(), Error> {
        use std::io::Write;
        use uv_fs::Simplified;

        let python_path = self.python_path.simplified_display().to_string();

        let launcher_bin = get_launcher_bin(is_gui)?;

        let kind_value = [self.kind.to_resource_value()];
        let mut resources: Vec<(&str, &[u8])> = vec![
            (RESOURCE_TRAMPOLINE_KIND, &kind_value),
            (RESOURCE_PYTHON_PATH, python_path.as_bytes()),
        ];
        let script_data;
        if let Some(data) = self.script_data {
            script_data = data;
            resources.push((RESOURCE_SCRIPT_DATA, &script_data));
        }

        let output = write_resources(launcher_bin, &resources)?;
        file.write_all(&output)?;

        Ok(())
    }

    #[must_use]
    pub fn with_python_path(self, path: PathBuf) -> Self {
        Self {
            kind: self.kind,
            python_path: path,
            script_data: self.script_data,
        }
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
    #[error(
        "Unable to create Windows launcher for: {0} (only x86_64, x86, and arm64 are supported)"
    )]
    UnsupportedWindowsArch(&'static str),
    #[error("Unable to create Windows launcher on non-Windows platform")]
    NotWindows,
    #[error("Cannot process launcher metadata from resource")]
    UnprocessableMetadata,
    #[error("Resources over 2^32 bytes are not supported")]
    ResourceTooLarge,
    #[error("Failed to parse Windows PE image")]
    PeRead(#[from] editpe::ImageReadError),
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

/// Write PE resources into a launcher binary using the `editpe` crate.
///
/// This directly manipulates the PE resource section without using Windows API calls
/// like `BeginUpdateResource`/`UpdateResource`/`EndUpdateResource`, which are unavailable
/// on minimal Windows environments such as Nano Server.
#[cfg(all(windows, not(target_arch = "x86")))]
fn write_resources(launcher_data: &[u8], resources: &[(&str, &[u8])]) -> Result<Vec<u8>, Error> {
    use editpe::{
        Image, ResourceData, ResourceDirectory, ResourceEntry, ResourceEntryName, ResourceTable,
    };

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

/// Write PE resources into a launcher binary using the Win32 resource APIs.
///
/// The `editpe` crate currently produces invalid PE32 output for the 32-bit trampolines,
/// so keep the previous update path for i686 while using `editpe` for the Nano Server
/// supported targets.
#[cfg(all(windows, target_arch = "x86"))]
fn write_resources(launcher_data: &[u8], resources: &[(&str, &[u8])]) -> Result<Vec<u8>, Error> {
    let temp_dir = tempfile::TempDir::new()?;
    let temp_file = temp_dir.path().join("uv-trampoline.exe");

    fs_err::write(&temp_file, launcher_data)?;
    write_resources_with_winapi(&temp_file, resources)?;

    Ok(fs_err::read(&temp_file)?)
}

#[cfg(all(windows, target_arch = "x86"))]
fn write_resources_with_winapi(path: &Path, resources: &[(&str, &[u8])]) -> Result<(), Error> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::LibraryLoader::{
        BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW,
    };

    let path_wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();

    // SAFETY: `path_wide` and each `name_wide` are null-terminated UTF-16 strings, and the
    // resource buffers live for the duration of each Win32 call.
    #[allow(unsafe_code)]
    unsafe {
        let handle = BeginUpdateResourceW(windows::core::PCWSTR(path_wide.as_ptr()), false)
            .map_err(|err| Error::Io(io::Error::from_raw_os_error(err.code().0)))?;

        for (name, data) in resources {
            let name_wide = name
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>();

            UpdateResourceW(
                handle,
                windows::core::PCWSTR(RT_RCDATA as usize as *const u16),
                windows::core::PCWSTR(name_wide.as_ptr()),
                0,
                Some(data.as_ptr().cast()),
                u32::try_from(data.len()).map_err(|_| Error::ResourceTooLarge)?,
            )
            .map_err(|err| Error::Io(io::Error::from_raw_os_error(err.code().0)))?;
        }

        EndUpdateResourceW(handle, false)
            .map_err(|err| Error::Io(io::Error::from_raw_os_error(err.code().0)))?;
    }

    Ok(())
}

/// Read a named resource from a parsed PE [`Image`].
///
/// Navigates the PE resource directory tree: `RT_RCDATA` → `name` → first language entry.
#[cfg(windows)]
fn read_resource_from_image(image: &editpe::Image<'_>, name: &str) -> Option<Vec<u8>> {
    use editpe::ResourceEntryName;

    let resource_directory = image.resource_directory()?;
    let root = resource_directory.root();

    let rcdata_entry = root.get(ResourceEntryName::ID(RT_RCDATA))?;
    let rcdata_table = rcdata_entry.as_table()?;

    let name_entry = rcdata_table.get(ResourceEntryName::from_string(name))?;
    let language_table = name_entry.as_table()?;

    // Get the first language entry (typically ID(0) for neutral).
    let entries = language_table.entries();
    let first_language = entries.first()?;
    let language_entry = language_table.get(*first_language)?;
    let data = language_entry.as_data()?;

    Some(data.data().to_vec())
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
    use std::io::{Cursor, Write};

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use uv_fs::Simplified;

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
    _is_gui: bool,
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
    is_gui: bool,
) -> Result<Vec<u8>, Error> {
    use uv_fs::Simplified;

    let launcher_bin: &[u8] = get_launcher_bin(is_gui)?;

    let python = python_executable.as_ref();
    let python_path = python.simplified_display().to_string();

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

    use super::{Launcher, LauncherKind, windows_python_launcher, windows_script_launcher};

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
    #[cfg(not(target_arch = "x86"))]
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

        assert!(launcher.kind == LauncherKind::Script);
        assert!(launcher.python_path == python_executable_path);

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
        let console_launcher = windows_python_launcher(&python_executable_path, false)?;

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

        assert!(launcher.kind == LauncherKind::Python);
        assert!(launcher.python_path == python_executable_path);

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
