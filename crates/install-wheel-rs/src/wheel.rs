use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::{env, io};

use data_encoding::BASE64URL_NOPAD;
use fs_err as fs;
use fs_err::{DirEntry, File};
use rustc_hash::FxHashMap;
use sha2::{Digest, Sha256};
use tracing::{instrument, warn};
use walkdir::WalkDir;
use zip::write::FileOptions;
use zip::ZipWriter;

use pypi_types::DirectUrl;
use uv_fs::{relative_to, Simplified};
use uv_normalize::PackageName;

use crate::record::RecordEntry;
use crate::script::Script;
use crate::{Error, Layout};

const LAUNCHER_MAGIC_NUMBER: [u8; 4] = [b'U', b'V', b'U', b'V'];

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

/// Wrapper script template function
///
/// <https://github.com/pypa/pip/blob/7f8a6844037fb7255cfd0d34ff8e8cf44f2598d4/src/pip/_vendor/distlib/scripts.py#L41-L48>
fn get_script_launcher(entry_point: &Script, shebang: &str) -> String {
    let Script {
        module, function, ..
    } = entry_point;

    let import_name = entry_point.import_name();

    format!(
        r##"{shebang}
# -*- coding: utf-8 -*-
import re
import sys
from {module} import {import_name}
if __name__ == "__main__":
    sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
    sys.exit({function}())
"##
    )
}

/// Part of entrypoints parsing
pub(crate) fn read_scripts_from_section(
    scripts_section: &HashMap<String, Option<String>>,
    section_name: &str,
    extras: Option<&[String]>,
) -> Result<Vec<Script>, Error> {
    let mut scripts = Vec::new();
    for (script_name, python_location) in scripts_section {
        match python_location {
            Some(value) => {
                if let Some(script) = Script::from_value(script_name, value, extras)? {
                    scripts.push(script);
                }
            }
            None => {
                return Err(Error::InvalidWheel(format!(
                    "[{section_name}] key {script_name} must have a value"
                )));
            }
        }
    }
    Ok(scripts)
}

/// Shamelessly stolen (and updated for recent sha2)
/// <https://github.com/richo/hashing-copy/blob/d8dd2fdb63c6faf198de0c9e5713d6249cbb5323/src/lib.rs#L10-L52>
/// which in turn got it from std
/// <https://doc.rust-lang.org/1.58.0/src/std/io/copy.rs.html#128-156>
fn copy_and_hash(reader: &mut impl Read, writer: &mut impl Write) -> io::Result<(u64, String)> {
    // TODO: Do we need to support anything besides sha256?
    let mut hasher = Sha256::new();
    // Same buf size as std. Note that this number is important for performance
    let mut buf = vec![0; 8 * 1024];

    let mut written = 0;
    loop {
        let len = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(len) => len,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        hasher.update(&buf[..len]);
        writer.write_all(&buf[..len])?;
        written += len as u64;
    }
    Ok((
        written,
        format!("sha256={}", BASE64URL_NOPAD.encode(&hasher.finalize())),
    ))
}

/// Format the shebang for a given Python executable.
///
/// Like pip, if a shebang is non-simple (too long or contains spaces), we use `/bin/sh` as the
/// executable.
///
/// See: <https://github.com/pypa/pip/blob/0ad4c94be74cc24874c6feb5bb3c2152c398a18e/src/pip/_vendor/distlib/scripts.py#L136-L165>
fn format_shebang(executable: impl AsRef<Path>, os_name: &str, relocatable: bool) -> String {
    // Convert the executable to a simplified path.
    let executable = executable.as_ref().simplified_display().to_string();

    // Validate the shebang.
    if os_name == "posix" {
        // The length of the full line: the shebang, plus the leading `#` and `!`, and a trailing
        // newline.
        let shebang_length = 2 + executable.len() + 1;

        // If the shebang is too long, or contains spaces, wrap it in `/bin/sh`.
        // Same applies for relocatable scripts (executable is relative to script dir, hence `dirname` trick)
        // (note: the Windows trampoline binaries natively support relative paths to executable)
        if shebang_length > 127 || executable.contains(' ') || relocatable {
            let prefix = if relocatable {
                r#""$(CDPATH= cd -- "$(dirname -- "$0")" && echo "$PWD")"/"#
            } else {
                ""
            };
            // Like Python's `shlex.quote`:
            // > Use single quotes, and put single quotes into double quotes
            // > The string $'b is then quoted as '$'"'"'b'
            let executable = format!("{}'{}'", prefix, executable.replace('\'', r#"'"'"'"#));
            return format!("#!/bin/sh\n'''exec' {executable} \"$0\" \"$@\"\n' '''");
        }
    }

    format!("#!{executable}")
}

/// A Windows script is a minimal .exe launcher binary with the python entrypoint script appended as
/// stored zip file.
///
/// <https://github.com/pypa/pip/blob/fd0ea6bc5e8cb95e518c23d901c26ca14db17f89/src/pip/_vendor/distlib/scripts.py#L248-L262>
#[allow(unused_variables)]
pub(crate) fn windows_script_launcher(
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
    let python_path = python.simplified_display().to_string();

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

/// Returns a [`PathBuf`] to `python[w].exe` for script execution.
///
/// <https://github.com/pypa/pip/blob/76e82a43f8fb04695e834810df64f2d9a2ff6020/src/pip/_vendor/distlib/scripts.py#L121-L126>
fn get_script_executable(python_executable: &Path, is_gui: bool) -> PathBuf {
    // Only check for pythonw.exe on Windows
    if cfg!(windows) && is_gui {
        python_executable
            .file_name()
            .map(|name| {
                let new_name = name.to_string_lossy().replace("python", "pythonw");
                python_executable.with_file_name(new_name)
            })
            .filter(|path| path.is_file())
            .unwrap_or_else(|| python_executable.to_path_buf())
    } else {
        python_executable.to_path_buf()
    }
}

/// Determine the absolute path to an entrypoint script.
fn entrypoint_path(entrypoint: &Script, layout: &Layout) -> PathBuf {
    if cfg!(windows) {
        // On windows we actually build an .exe wrapper
        let script_name = entrypoint
            .name
            // FIXME: What are the in-reality rules here for names?
            .strip_suffix(".py")
            .unwrap_or(&entrypoint.name)
            .to_string()
            + ".exe";

        layout.scheme.scripts.join(script_name)
    } else {
        layout.scheme.scripts.join(&entrypoint.name)
    }
}

/// Create the wrapper scripts in the bin folder of the venv for launching console scripts.
pub(crate) fn write_script_entrypoints(
    layout: &Layout,
    relocatable: bool,
    site_packages: &Path,
    entrypoints: &[Script],
    record: &mut Vec<RecordEntry>,
    is_gui: bool,
) -> Result<(), Error> {
    for entrypoint in entrypoints {
        let entrypoint_absolute = entrypoint_path(entrypoint, layout);

        let entrypoint_relative = pathdiff::diff_paths(&entrypoint_absolute, site_packages)
            .ok_or_else(|| {
                Error::Io(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "Could not find relative path for: {}",
                        entrypoint_absolute.simplified_display()
                    ),
                ))
            })?;

        // Generate the launcher script.
        let launcher_executable = get_script_executable(&layout.sys_executable, is_gui);
        let launcher_executable =
            get_relocatable_executable(launcher_executable, layout, relocatable)?;
        let launcher_python_script = get_script_launcher(
            entrypoint,
            &format_shebang(&launcher_executable, &layout.os_name, relocatable),
        );

        // If necessary, wrap the launcher script in a Windows launcher binary.
        if cfg!(windows) {
            write_file_recorded(
                site_packages,
                &entrypoint_relative,
                &windows_script_launcher(&launcher_python_script, is_gui, &launcher_executable)?,
                record,
            )?;
        } else {
            write_file_recorded(
                site_packages,
                &entrypoint_relative,
                &launcher_python_script,
                record,
            )?;

            // Make the launcher executable.
            #[cfg(unix)]
            {
                use std::fs::Permissions;
                use std::os::unix::fs::PermissionsExt;

                let path = site_packages.join(entrypoint_relative);
                let permissions = fs::metadata(&path)?.permissions();
                if permissions.mode() & 0o111 != 0o111 {
                    fs::set_permissions(path, Permissions::from_mode(permissions.mode() | 0o111))?;
                }
            }
        }
    }
    Ok(())
}

/// Whether the wheel should be installed into the `purelib` or `platlib` directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibKind {
    /// Install into the `purelib` directory.
    Pure,
    /// Install into the `platlib` directory.
    Plat,
}

/// Parse WHEEL file.
///
/// > {distribution}-{version}.dist-info/WHEEL is metadata about the archive itself in the same
/// > basic key: value format:
pub fn parse_wheel_file(wheel_text: &str) -> Result<LibKind, Error> {
    // {distribution}-{version}.dist-info/WHEEL is metadata about the archive itself in the same basic key: value format:
    let data = parse_key_value_file(&mut wheel_text.as_bytes(), "WHEEL")?;

    // Determine whether Root-Is-Purelib == ‘true’.
    // If it is, the wheel is pure, and should be installed into purelib.
    let root_is_purelib = data
        .get("Root-Is-Purelib")
        .and_then(|root_is_purelib| root_is_purelib.first())
        .is_some_and(|root_is_purelib| root_is_purelib == "true");
    let lib_kind = if root_is_purelib {
        LibKind::Pure
    } else {
        LibKind::Plat
    };

    // mkl_fft-1.3.6-58-cp310-cp310-manylinux2014_x86_64.whl has multiple Wheel-Version entries, we have to ignore that
    // like pip
    let wheel_version = data
        .get("Wheel-Version")
        .and_then(|wheel_versions| wheel_versions.first());
    let wheel_version = wheel_version
        .and_then(|wheel_version| wheel_version.split_once('.'))
        .ok_or_else(|| {
            Error::InvalidWheel(format!(
                "Invalid Wheel-Version in WHEEL file: {wheel_version:?}"
            ))
        })?;
    // pip has some test wheels that use that ancient version,
    // and technically we only need to check that the version is not higher
    if wheel_version == ("0", "1") {
        warn!("Ancient wheel version 0.1 (expected is 1.0)");
        return Ok(lib_kind);
    }
    // Check that installer is compatible with Wheel-Version. Warn if minor version is greater, abort if major version is greater.
    // Wheel-Version: 1.0
    if wheel_version.0 != "1" {
        return Err(Error::InvalidWheel(format!(
            "Unsupported wheel major version (expected {}, got {})",
            1, wheel_version.0
        )));
    }
    if wheel_version.1 > "0" {
        warn!(
            "Warning: Unsupported wheel minor version (expected {}, got {})",
            0, wheel_version.1
        );
    }
    Ok(lib_kind)
}

/// Moves the files and folders in src to dest, updating the RECORD in the process
pub(crate) fn move_folder_recorded(
    src_dir: &Path,
    dest_dir: &Path,
    site_packages: &Path,
    record: &mut [RecordEntry],
) -> Result<(), Error> {
    fs::create_dir_all(dest_dir)?;
    for entry in WalkDir::new(src_dir) {
        let entry = entry?;
        let src = entry.path();
        // This is the base path for moving to the actual target for the data
        // e.g. for data it's without <..>.data/data/
        let relative_to_data = src.strip_prefix(src_dir).expect("Prefix must no change");
        // This is the path stored in RECORD
        // e.g. for data it's with .data/data/
        let relative_to_site_packages = src
            .strip_prefix(site_packages)
            .expect("Prefix must no change");
        let target = dest_dir.join(relative_to_data);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            fs::rename(src, &target)?;
            let entry = record
                .iter_mut()
                .find(|entry| Path::new(&entry.path) == relative_to_site_packages)
                .ok_or_else(|| {
                    Error::RecordFile(format!(
                        "Could not find entry for {} ({})",
                        relative_to_site_packages.simplified_display(),
                        src.simplified_display()
                    ))
                })?;
            entry.path = relative_to(&target, site_packages)?.display().to_string();
        }
    }
    Ok(())
}

/// Installs a single script (not an entrypoint).
///
/// Has to deal with both binaries files (just move) and scripts (rewrite the shebang if applicable).
fn install_script(
    layout: &Layout,
    relocatable: bool,
    site_packages: &Path,
    record: &mut [RecordEntry],
    file: &DirEntry,
) -> Result<(), Error> {
    let file_type = file.file_type()?;

    if file_type.is_dir() {
        return Err(Error::InvalidWheel(format!(
            "Wheel contains an invalid entry (directory) in the `scripts` directory: {}",
            file.path().simplified_display()
        )));
    }

    if file_type.is_symlink() {
        let Ok(target) = file.path().canonicalize() else {
            return Err(Error::InvalidWheel(format!(
                "Wheel contains an invalid entry (broken symlink) in the `scripts` directory: {}",
                file.path().simplified_display(),
            )));
        };
        if target.is_dir() {
            return Err(Error::InvalidWheel(format!(
                "Wheel contains an invalid entry (directory symlink) in the `scripts` directory: {} ({})",
                file.path().simplified_display(),
                target.simplified_display()
            )));
        }
    }

    let script_absolute = layout.scheme.scripts.join(file.file_name());
    let script_relative =
        pathdiff::diff_paths(&script_absolute, site_packages).ok_or_else(|| {
            Error::Io(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Could not find relative path for: {}",
                    script_absolute.simplified_display()
                ),
            ))
        })?;

    let path = file.path();
    let mut script = File::open(&path)?;

    // https://sphinx-locales.github.io/peps/pep-0427/#recommended-installer-features
    // > In wheel, scripts are packaged in {distribution}-{version}.data/scripts/.
    // > If the first line of a file in scripts/ starts with exactly b'#!python',
    // > rewrite to point to the correct interpreter. Unix installers may need to
    // > add the +x bit to these files if the archive was created on Windows.
    //
    // > The b'#!pythonw' convention is allowed. b'#!pythonw' indicates a GUI script
    // > instead of a console script.
    let placeholder_python = b"#!python";
    // scripts might be binaries, so we read an exact number of bytes instead of the first line as string
    let mut start = vec![0; placeholder_python.len()];
    script.read_exact(&mut start)?;
    let size_and_encoded_hash = if start == placeholder_python {
        let is_gui = {
            let mut buf = vec![0; 1];
            script.read_exact(&mut buf)?;
            if buf == b"w" {
                true
            } else {
                script.seek_relative(-1)?;
                false
            }
        };
        let executable = get_script_executable(&layout.sys_executable, is_gui);
        let executable = get_relocatable_executable(executable, layout, relocatable)?;
        let start = format_shebang(&executable, &layout.os_name, relocatable)
            .as_bytes()
            .to_vec();

        let mut target = uv_fs::tempfile_in(&layout.scheme.scripts)?;
        let size_and_encoded_hash = copy_and_hash(&mut start.chain(script), &mut target)?;
        target.persist(&script_absolute).map_err(|err| {
            io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Failed to persist temporary file to {}: {}",
                    path.user_display(),
                    err.error
                ),
            )
        })?;
        fs::remove_file(&path)?;

        // Make the script executable. We just created the file, so we can set permissions directly.
        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;

            let permissions = fs::metadata(&script_absolute)?.permissions();
            if permissions.mode() & 0o111 != 0o111 {
                fs::set_permissions(
                    script_absolute,
                    Permissions::from_mode(permissions.mode() | 0o111),
                )?;
            }
        }

        Some(size_and_encoded_hash)
    } else {
        // Reading and writing is slow (especially for large binaries), so we move them instead, if
        // we can. This also retains the file permissions. We _can't_ move (and must copy) if the
        // file permissions need to be changed, since we might not own the file.
        drop(script);

        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;

            let permissions = fs::metadata(&path)?.permissions();

            if permissions.mode() & 0o111 == 0o111 {
                // If the permissions are already executable, we don't need to change them.
                fs::rename(&path, &script_absolute)?;
            } else {
                // If we have to modify the permissions, copy the file, since we might not own it.
                warn!(
                    "Copying script from {} to {} (permissions: {:o})",
                    path.simplified_display(),
                    script_absolute.simplified_display(),
                    permissions.mode()
                );

                uv_fs::copy_atomic_sync(&path, &script_absolute)?;

                fs::set_permissions(
                    script_absolute,
                    Permissions::from_mode(permissions.mode() | 0o111),
                )?;
            }
        }

        #[cfg(not(unix))]
        {
            fs::rename(&path, &script_absolute)?;
        }

        None
    };

    // Find the existing entry in the `RECORD`.
    let relative_to_site_packages = path
        .strip_prefix(site_packages)
        .expect("Prefix must no change");
    let entry = record
        .iter_mut()
        .find(|entry| Path::new(&entry.path) == relative_to_site_packages)
        .ok_or_else(|| {
            // This should be possible to occur at this point, but filesystems and such
            Error::RecordFile(format!(
                "Could not find entry for {} ({})",
                relative_to_site_packages.simplified_display(),
                path.simplified_display()
            ))
        })?;

    // Update the entry in the `RECORD`.
    entry.path = script_relative.simplified_display().to_string();
    if let Some((size, encoded_hash)) = size_and_encoded_hash {
        entry.size = Some(size);
        entry.hash = Some(encoded_hash);
    }
    Ok(())
}

/// Move the files from the .data directory to the right location in the venv
#[instrument(skip_all)]
pub(crate) fn install_data(
    layout: &Layout,
    relocatable: bool,
    site_packages: &Path,
    data_dir: &Path,
    dist_name: &PackageName,
    console_scripts: &[Script],
    gui_scripts: &[Script],
    record: &mut [RecordEntry],
) -> Result<(), Error> {
    for entry in fs::read_dir(data_dir)? {
        let entry = entry?;
        let path = entry.path();

        match path.file_name().and_then(|name| name.to_str()) {
            Some("data") => {
                // Move the content of the folder to the root of the venv
                move_folder_recorded(&path, &layout.scheme.data, site_packages, record)?;
            }
            Some("scripts") => {
                let mut initialized = false;
                for file in fs::read_dir(path)? {
                    let file = file?;

                    // Couldn't find any docs for this, took it directly from
                    // https://github.com/pypa/pip/blob/b5457dfee47dd9e9f6ec45159d9d410ba44e5ea1/src/pip/_internal/operations/install/wheel.py#L565-L583
                    let name = file.file_name().to_string_lossy().to_string();
                    let match_name = name
                        .strip_suffix(".exe")
                        .or_else(|| name.strip_suffix("-script.py"))
                        .or_else(|| name.strip_suffix(".pya"))
                        .unwrap_or(&name);
                    if console_scripts
                        .iter()
                        .chain(gui_scripts)
                        .any(|script| script.name == match_name)
                    {
                        continue;
                    }

                    // Create the scripts directory, if it doesn't exist.
                    if !initialized {
                        fs::create_dir_all(&layout.scheme.scripts)?;
                        initialized = true;
                    }

                    install_script(layout, relocatable, site_packages, record, &file)?;
                }
            }
            Some("headers") => {
                let target_path = layout.scheme.include.join(dist_name.as_str());
                move_folder_recorded(&path, &target_path, site_packages, record)?;
            }
            Some("purelib") => {
                move_folder_recorded(&path, &layout.scheme.purelib, site_packages, record)?;
            }
            Some("platlib") => {
                move_folder_recorded(&path, &layout.scheme.platlib, site_packages, record)?;
            }
            _ => {
                return Err(Error::InvalidWheel(format!(
                    "Unknown wheel data type: {:?}",
                    entry.file_name()
                )));
            }
        }
    }
    Ok(())
}

/// Write the content to a file and add the hash to the RECORD list
///
/// We still the path in the absolute path to the site packages and the relative path in the
/// site packages because we must only record the relative path in RECORD
pub(crate) fn write_file_recorded(
    site_packages: &Path,
    relative_path: &Path,
    content: impl AsRef<[u8]>,
    record: &mut Vec<RecordEntry>,
) -> Result<(), Error> {
    debug_assert!(
        !relative_path.is_absolute(),
        "Path must be relative: {}",
        relative_path.display()
    );

    uv_fs::write_atomic_sync(site_packages.join(relative_path), content.as_ref())?;

    let hash = Sha256::new().chain_update(content.as_ref()).finalize();
    let encoded_hash = format!("sha256={}", BASE64URL_NOPAD.encode(&hash));
    record.push(RecordEntry {
        path: relative_path.display().to_string(),
        hash: Some(encoded_hash),
        size: Some(content.as_ref().len() as u64),
    });
    Ok(())
}

/// Adds `INSTALLER`, `REQUESTED` and `direct_url.json` to the .dist-info dir
pub(crate) fn extra_dist_info(
    site_packages: &Path,
    dist_info_prefix: &str,
    requested: bool,
    direct_url: Option<&DirectUrl>,
    installer: Option<&str>,
    record: &mut Vec<RecordEntry>,
) -> Result<(), Error> {
    let dist_info_dir = PathBuf::from(format!("{dist_info_prefix}.dist-info"));
    if requested {
        write_file_recorded(site_packages, &dist_info_dir.join("REQUESTED"), "", record)?;
    }
    if let Some(direct_url) = direct_url {
        write_file_recorded(
            site_packages,
            &dist_info_dir.join("direct_url.json"),
            serde_json::to_string(direct_url)?.as_bytes(),
            record,
        )?;
    }
    if let Some(installer) = installer {
        write_file_recorded(
            site_packages,
            &dist_info_dir.join("INSTALLER"),
            installer,
            record,
        )?;
    }
    Ok(())
}

/// Get the path to the Python executable for the [`Layout`], based on whether the wheel should
/// be relocatable.
///
/// Returns `sys.executable` if the wheel is not relocatable; otherwise, returns a path relative
/// to the scripts directory.
pub(crate) fn get_relocatable_executable(
    executable: PathBuf,
    layout: &Layout,
    relocatable: bool,
) -> Result<PathBuf, Error> {
    Ok(if relocatable {
        pathdiff::diff_paths(&executable, &layout.scheme.scripts).ok_or_else(|| {
            Error::Io(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Could not find relative path for: {}",
                    executable.simplified_display()
                ),
            ))
        })?
    } else {
        executable
    })
}

/// Reads the record file
/// <https://www.python.org/dev/peps/pep-0376/#record>
pub fn read_record_file(record: &mut impl Read) -> Result<Vec<RecordEntry>, Error> {
    csv::ReaderBuilder::new()
        .has_headers(false)
        .escape(Some(b'"'))
        .from_reader(record)
        .deserialize()
        .map(|entry| {
            let entry: RecordEntry = entry?;
            Ok(RecordEntry {
                // selenium uses absolute paths for some reason
                path: entry.path.trim_start_matches('/').to_string(),
                ..entry
            })
        })
        .collect()
}

/// Parse a file with `Key: value` entries such as WHEEL and METADATA
fn parse_key_value_file(
    file: impl Read,
    debug_filename: &str,
) -> Result<FxHashMap<String, Vec<String>>, Error> {
    let mut data: FxHashMap<String, Vec<String>> = FxHashMap::default();

    let file = BufReader::new(file);
    for (line_no, line) in file.lines().enumerate() {
        let line = line?.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once(':').ok_or_else(|| {
            Error::InvalidWheel(format!(
                "Line {} of the {debug_filename} file is invalid",
                line_no + 1
            ))
        })?;
        data.entry(key.trim().to_string())
            .or_default()
            .push(value.trim().to_string());
    }
    Ok(data)
}

#[cfg(test)]
mod test {
    use std::io::Cursor;
    use std::path::Path;

    use anyhow::Result;
    use assert_fs::prelude::*;
    use indoc::{formatdoc, indoc};

    use crate::wheel::format_shebang;
    use crate::Error;

    use super::{
        get_script_executable, parse_key_value_file, parse_wheel_file, read_record_file, Script,
    };

    #[test]
    fn test_parse_key_value_file() {
        let text = indoc! {"
            Wheel-Version: 1.0
            Generator: bdist_wheel (0.37.1)
            Root-Is-Purelib: false
            Tag: cp38-cp38-manylinux_2_17_x86_64
            Tag: cp38-cp38-manylinux2014_x86_64
        "};

        parse_key_value_file(&mut text.as_bytes(), "WHEEL").unwrap();
    }

    #[test]
    fn test_parse_wheel_version() {
        fn wheel_with_version(version: &str) -> String {
            formatdoc! {"
                Wheel-Version: {}
                Generator: bdist_wheel (0.37.0)
                Root-Is-Purelib: true
                Tag: py2-none-any
                Tag: py3-none-any
                ",
                version
            }
        }
        parse_wheel_file(&wheel_with_version("1.0")).unwrap();
        parse_wheel_file(&wheel_with_version("2.0")).unwrap_err();
    }

    #[test]
    fn record_with_absolute_paths() {
        let record: &str = indoc! {"
            /selenium/__init__.py,sha256=l8nEsTP4D2dZVula_p4ZuCe8AGnxOq7MxMeAWNvR0Qc,811
            /selenium/common/exceptions.py,sha256=oZx2PS-g1gYLqJA_oqzE4Rq4ngplqlwwRBZDofiqni0,9309
            selenium-4.1.0.dist-info/METADATA,sha256=jqvBEwtJJ2zh6CljTfTXmpF1aiFs-gvOVikxGbVyX40,6468
            selenium-4.1.0.dist-info/RECORD,,
        "};

        let entries = read_record_file(&mut record.as_bytes()).unwrap();
        let expected = [
            "selenium/__init__.py",
            "selenium/common/exceptions.py",
            "selenium-4.1.0.dist-info/METADATA",
            "selenium-4.1.0.dist-info/RECORD",
        ]
        .map(ToString::to_string)
        .to_vec();
        let actual = entries
            .into_iter()
            .map(|entry| entry.path)
            .collect::<Vec<String>>();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_script_from_value() {
        assert_eq!(
            Script::from_value("launcher", "foo.bar:main", None).unwrap(),
            Some(Script {
                name: "launcher".to_string(),
                module: "foo.bar".to_string(),
                function: "main".to_string(),
            })
        );
        assert_eq!(
            Script::from_value(
                "launcher",
                "foo.bar:main",
                Some(&["bar".to_string(), "baz".to_string()]),
            )
            .unwrap(),
            Some(Script {
                name: "launcher".to_string(),
                module: "foo.bar".to_string(),
                function: "main".to_string(),
            })
        );
        assert_eq!(
            Script::from_value("launcher", "foomod:main_bar [bar,baz]", Some(&[])).unwrap(),
            None
        );
        assert_eq!(
            Script::from_value(
                "launcher",
                "foomod:main_bar [bar,baz]",
                Some(&["bar".to_string(), "baz".to_string()]),
            )
            .unwrap(),
            Some(Script {
                name: "launcher".to_string(),
                module: "foomod".to_string(),
                function: "main_bar".to_string(),
            })
        );
    }

    #[test]
    fn test_shebang() {
        // By default, use a simple shebang.
        let executable = Path::new("/usr/bin/python3");
        let os_name = "posix";
        assert_eq!(
            format_shebang(executable, os_name, false),
            "#!/usr/bin/python3"
        );

        // If the path contains spaces, we should use the `exec` trick.
        let executable = Path::new("/usr/bin/path to python3");
        let os_name = "posix";
        assert_eq!(
            format_shebang(executable, os_name, false),
            "#!/bin/sh\n'''exec' '/usr/bin/path to python3' \"$0\" \"$@\"\n' '''"
        );

        // And if we want a relocatable script, we should use the `exec` trick with `dirname`.
        let executable = Path::new("python3");
        let os_name = "posix";
        assert_eq!(
            format_shebang(executable, os_name, true),
            "#!/bin/sh\n'''exec' \"$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && echo \"$PWD\")\"/'python3' \"$0\" \"$@\"\n' '''"
        );

        // Except on Windows...
        let executable = Path::new("/usr/bin/path to python3");
        let os_name = "nt";
        assert_eq!(
            format_shebang(executable, os_name, false),
            "#!/usr/bin/path to python3"
        );

        // Quotes, however, are ok.
        let executable = Path::new("/usr/bin/'python3'");
        let os_name = "posix";
        assert_eq!(
            format_shebang(executable, os_name, false),
            "#!/usr/bin/'python3'"
        );

        // If the path is too long, we should not use the `exec` trick.
        let executable = Path::new("/usr/bin/path/to/a/very/long/executable/executable/executable/executable/executable/executable/executable/executable/name/python3");
        let os_name = "posix";
        assert_eq!(format_shebang(executable, os_name, false), "#!/bin/sh\n'''exec' '/usr/bin/path/to/a/very/long/executable/executable/executable/executable/executable/executable/executable/executable/name/python3' \"$0\" \"$@\"\n' '''");
    }

    #[test]
    fn test_empty_value() -> Result<(), Error> {
        let wheel = indoc! {r"
        Wheel-Version: 1.0
        Generator: custom
        Root-Is-Purelib: false
        Tag:
        Tag: -manylinux_2_17_x86_64
        Tag: -manylinux2014_x86_64
        "
        };
        let reader = Cursor::new(wheel.to_string().into_bytes());
        let wheel_file = parse_key_value_file(reader, "WHEEL")?;
        assert_eq!(
            wheel_file.get("Wheel-Version"),
            Some(&["1.0".to_string()].to_vec())
        );
        assert_eq!(
            wheel_file.get("Tag"),
            Some(
                &[
                    String::new(),
                    "-manylinux_2_17_x86_64".to_string(),
                    "-manylinux2014_x86_64".to_string()
                ]
                .to_vec()
            )
        );
        Ok(())
    }

    #[test]
    #[cfg(all(windows, target_arch = "x86"))]
    fn test_launchers_are_small() {
        // At time of writing, they are 17408 bytes.
        assert!(
            super::LAUNCHER_I686_GUI.len() < 25 * 1024,
            "GUI launcher: {}",
            super::LAUNCHER_I686_GUI.len()
        );
        assert!(
            super::LAUNCHER_I686_CONSOLE.len() < 25 * 1024,
            "CLI launcher: {}",
            super::LAUNCHER_I686_CONSOLE.len()
        );
    }

    #[test]
    #[cfg(all(windows, target_arch = "x86_64"))]
    fn test_launchers_are_small() {
        // At time of writing, they are 21504 and 20480 bytes.
        assert!(
            super::LAUNCHER_X86_64_GUI.len() < 25 * 1024,
            "GUI launcher: {}",
            super::LAUNCHER_X86_64_GUI.len()
        );
        assert!(
            super::LAUNCHER_X86_64_CONSOLE.len() < 25 * 1024,
            "CLI launcher: {}",
            super::LAUNCHER_X86_64_CONSOLE.len()
        );
    }

    #[test]
    #[cfg(all(windows, target_arch = "aarch64"))]
    fn test_launchers_are_small() {
        // At time of writing, they are 20480 and 19456 bytes.
        assert!(
            super::LAUNCHER_AARCH64_GUI.len() < 25 * 1024,
            "GUI launcher: {}",
            super::LAUNCHER_AARCH64_GUI.len()
        );
        assert!(
            super::LAUNCHER_AARCH64_CONSOLE.len() < 25 * 1024,
            "CLI launcher: {}",
            super::LAUNCHER_AARCH64_CONSOLE.len()
        );
    }

    #[test]
    fn test_script_executable() -> Result<()> {
        // Test with adjacent pythonw.exe
        let temp_dir = assert_fs::TempDir::new()?;
        let python_exe = temp_dir.child("python.exe");
        let pythonw_exe = temp_dir.child("pythonw.exe");
        python_exe.write_str("")?;
        pythonw_exe.write_str("")?;

        let script_path = get_script_executable(&python_exe, true);
        #[cfg(windows)]
        assert_eq!(script_path, pythonw_exe.to_path_buf());
        #[cfg(not(windows))]
        assert_eq!(script_path, python_exe.to_path_buf());

        let script_path = get_script_executable(&python_exe, false);
        assert_eq!(script_path, python_exe.to_path_buf());

        // Test without adjacent pythonw.exe
        let temp_dir = assert_fs::TempDir::new()?;
        let python_exe = temp_dir.child("python.exe");
        python_exe.write_str("")?;

        let script_path = get_script_executable(&python_exe, true);
        assert_eq!(script_path, python_exe.to_path_buf());

        let script_path = get_script_executable(&python_exe, false);
        assert_eq!(script_path, python_exe.to_path_buf());

        // Test with overridden python.exe and pythonw.exe
        let temp_dir = assert_fs::TempDir::new()?;
        let python_exe = temp_dir.child("python.exe");
        let pythonw_exe = temp_dir.child("pythonw.exe");
        let dot_python_exe = temp_dir.child(".python.exe");
        let dot_pythonw_exe = temp_dir.child(".pythonw.exe");
        python_exe.write_str("")?;
        pythonw_exe.write_str("")?;
        dot_python_exe.write_str("")?;
        dot_pythonw_exe.write_str("")?;

        let script_path = get_script_executable(&dot_python_exe, true);
        #[cfg(windows)]
        assert_eq!(script_path, dot_pythonw_exe.to_path_buf());
        #[cfg(not(windows))]
        assert_eq!(script_path, dot_python_exe.to_path_buf());

        let script_path = get_script_executable(&dot_python_exe, false);
        assert_eq!(script_path, dot_python_exe.to_path_buf());

        Ok(())
    }
}
