use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::str::FromStr;
use std::{env, io, iter};

use configparser::ini::Ini;
use data_encoding::BASE64URL_NOPAD;
use fs_err as fs;
use fs_err::{DirEntry, File};
use mailparse::MailHeaderMap;
use rustc_hash::{FxHashMap, FxHashSet};
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use tracing::{debug, error, instrument, warn};
use walkdir::WalkDir;
use zip::result::ZipError;
use zip::write::FileOptions;
use zip::{ZipArchive, ZipWriter};

use distribution_filename::WheelFilename;
use pep440_rs::Version;
use pypi_types::DirectUrl;
use uv_fs::Normalized;
use uv_normalize::PackageName;

use crate::install_location::{InstallLocation, LockedDir};
use crate::record::RecordEntry;
use crate::script::Script;
use crate::{find_dist_info, Error};

/// `#!/usr/bin/env python`
pub const SHEBANG_PYTHON: &str = "#!/usr/bin/env python";

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
fn get_script_launcher(module: &str, import_name: &str, shebang: &str) -> String {
    format!(
        r##"{shebang}
# -*- coding: utf-8 -*-
import re
import sys
from {module} import {import_name}
if __name__ == "__main__":
    sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
    sys.exit({import_name}())
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

/// Parses the `entry_points.txt` entry in the wheel for console scripts
///
/// Returns (`script_name`, module, function)
///
/// Extras are supposed to be ignored, which happens if you pass None for extras
fn parse_scripts<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    dist_info_dir: &str,
    extras: Option<&[String]>,
) -> Result<(Vec<Script>, Vec<Script>), Error> {
    let entry_points_path = format!("{dist_info_dir}/entry_points.txt");
    let entry_points_mapping = match archive.by_name(&entry_points_path) {
        Ok(file) => {
            let ini_text = std::io::read_to_string(file)?;
            Ini::new_cs()
                .read(ini_text)
                .map_err(|err| Error::InvalidWheel(format!("entry_points.txt is invalid: {err}")))?
        }
        Err(ZipError::FileNotFound) => return Ok((Vec::new(), Vec::new())),
        Err(err) => return Err(Error::Zip(entry_points_path, err)),
    };

    // TODO: handle extras
    let console_scripts = match entry_points_mapping.get("console_scripts") {
        Some(console_scripts) => {
            read_scripts_from_section(console_scripts, "console_scripts", extras)?
        }
        None => Vec::new(),
    };
    let gui_scripts = match entry_points_mapping.get("gui_scripts") {
        Some(gui_scripts) => read_scripts_from_section(gui_scripts, "gui_scripts", extras)?,
        None => Vec::new(),
    };

    Ok((console_scripts, gui_scripts))
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

/// Extract all files from the wheel into the site packages
///
/// Matches with the RECORD entries
///
/// Returns paths relative to site packages
fn unpack_wheel_files<R: Read + Seek>(
    site_packages: &Path,
    record_path: &str,
    archive: &mut ZipArchive<R>,
    record: &[RecordEntry],
    check_hashes: bool,
) -> Result<Vec<PathBuf>, Error> {
    let mut extracted_paths = Vec::new();
    // Cache the created parent dirs to avoid io calls
    // When deactivating bytecode compilation and sha2 those were 5% of total runtime, with
    // cache it 2.3%
    let mut created_dirs = FxHashSet::default();
    // https://github.com/zip-rs/zip/blob/7edf2489d5cff8b80f02ee6fc5febf3efd0a9442/examples/extract.rs
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|err| {
            let file1 = format!("(index {i})");
            Error::Zip(file1, err)
        })?;
        // enclosed_name takes care of evil zip paths
        let relative = match file.enclosed_name() {
            Some(path) => path.to_owned(),
            None => continue,
        };
        let out_path = site_packages.join(&relative);

        if file.name().ends_with('/') {
            // pip seems to do ignore those folders, so do we
            // fs::create_dir_all(&out_path)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            if created_dirs.insert(parent.to_path_buf()) {
                fs::create_dir_all(parent)?;
            }
        }
        let mut outfile = BufWriter::new(File::create(&out_path)?);
        let encoded_hash = if check_hashes {
            let (_size, encoded_hash) = copy_and_hash(&mut file, &mut outfile)?;
            Some(encoded_hash)
        } else {
            io::copy(&mut file, &mut outfile)?;
            None
        };

        extracted_paths.push(relative.clone());

        // Get and Set permissions
        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;

            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&out_path, Permissions::from_mode(mode))?;
            }
        }

        // This is the RECORD file that contains the hashes so naturally it can't contain it's own
        // hash and size (but it does contain an entry with two empty fields)
        // > 6. RECORD.jws is used for digital signatures. It is not mentioned in RECORD.
        // > 7. RECORD.p7s is allowed as a courtesy to anyone who would prefer to use S/MIME
        // >    signatures to secure their wheel files. It is not mentioned in RECORD.
        let record_path = PathBuf::from(&record_path);
        if [
            record_path.clone(),
            record_path.with_extension("jws"),
            record_path.with_extension("p7s"),
        ]
        .contains(&relative)
        {
            continue;
        }

        if let Some(encoded_hash) = encoded_hash {
            // `relative == Path::new(entry.path)` was really slow
            let relative_str = relative.display().to_string();
            let recorded_hash = record
                .iter()
                .find(|entry| relative_str == entry.path)
                .and_then(|entry| entry.hash.as_ref())
                .ok_or_else(|| {
                    Error::RecordFile(format!(
                        "Missing hash for {} (expected {})",
                        relative.normalized_display(),
                        encoded_hash
                    ))
                })?;
            if recorded_hash != &encoded_hash {
                if relative.as_os_str().to_string_lossy().starts_with("torch-") {
                    error!(
                        "Hash mismatch for {}. Recorded: {}, Actual: {}",
                        relative.normalized_display(),
                        recorded_hash,
                        encoded_hash,
                    );
                    error!(
                        "Torch isn't capable of producing correct hashes 🙄 Ignoring. \
                    https://github.com/pytorch/pytorch/issues/47916"
                    );
                    continue;
                }
                return Err(Error::RecordFile(format!(
                    "Hash mismatch for {}. Recorded: {}, Actual: {}",
                    relative.normalized_display(),
                    recorded_hash,
                    encoded_hash,
                )));
            }
        }
    }
    Ok(extracted_paths)
}

fn get_shebang(location: &InstallLocation<impl AsRef<Path>>) -> String {
    let path = location.python().to_string_lossy().to_string();
    let path = if cfg!(windows) {
        // https://stackoverflow.com/a/50323079
        const VERBATIM_PREFIX: &str = r"\\?\";
        if let Some(stripped) = path.strip_prefix(VERBATIM_PREFIX) {
            stripped.to_string()
        } else {
            path
        }
    } else {
        path
    };
    format!("#!{path}")
}

/// A Windows script is a minimal .exe launcher binary with the python entrypoint script appended as
/// stored zip file. The launcher will look for `python[w].exe` adjacent to it in the same directory
/// to start the embedded script.
///
/// <https://github.com/pypa/pip/blob/fd0ea6bc5e8cb95e518c23d901c26ca14db17f89/src/pip/_vendor/distlib/scripts.py#L248-L262>
#[allow(unused_variables)]
pub(crate) fn windows_script_launcher(
    launcher_python_script: &str,
    is_gui: bool,
) -> Result<Vec<u8>, Error> {
    // This method should only be called on Windows, but we avoid `#[cfg(windows)]` to retain
    // compilation on all platforms.
    if cfg!(not(windows)) {
        return Err(Error::NotWindows);
    }

    let launcher_bin: &[u8] = match env::consts::ARCH {
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

    let mut launcher: Vec<u8> = Vec::with_capacity(launcher_bin.len() + payload.len());
    launcher.extend_from_slice(launcher_bin);
    launcher.extend_from_slice(&payload);
    Ok(launcher)
}

/// Create the wrapper scripts in the bin folder of the venv for launching console scripts
///
/// We also pass `venv_base` so we can write the same path as pip does
///
/// TODO: Test for this launcher directly in install-wheel-rs
pub(crate) fn write_script_entrypoints(
    site_packages: &Path,
    location: &InstallLocation<impl AsRef<Path>>,
    entrypoints: &[Script],
    record: &mut Vec<RecordEntry>,
    is_gui: bool,
) -> Result<(), Error> {
    for entrypoint in entrypoints {
        let entrypoint_relative = if cfg!(windows) {
            // On windows we actually build an .exe wrapper
            let script_name = entrypoint
                .script_name
                // FIXME: What are the in-reality rules here for names?
                .strip_suffix(".py")
                .unwrap_or(&entrypoint.script_name)
                .to_string()
                + ".exe";
            bin_rel().join(script_name)
        } else {
            bin_rel().join(&entrypoint.script_name)
        };

        // Generate the launcher script.
        let launcher_python_script = get_script_launcher(
            &entrypoint.module,
            &entrypoint.function,
            &get_shebang(location),
        );

        // If necessary, wrap the launcher script in a Windows launcher binary.
        if cfg!(windows) {
            write_file_recorded(
                site_packages,
                &entrypoint_relative,
                &windows_script_launcher(&launcher_python_script, is_gui)?,
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
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    site_packages.join(entrypoint_relative),
                    std::fs::Permissions::from_mode(0o755),
                )?;
            }
        }
    }
    Ok(())
}

fn bin_rel() -> PathBuf {
    if cfg!(windows) {
        // windows doesn't have the python part, only Lib/site-packages
        Path::new("..").join("..").join("Scripts")
    } else {
        // linux/mac has lib/python/site-packages
        Path::new("..").join("..").join("..").join("bin")
    }
}

/// Parse WHEEL file
///
/// > {distribution}-{version}.dist-info/WHEEL is metadata about the archive itself in the same
/// > basic key: value format:
pub(crate) fn parse_wheel_version(wheel_text: &str) -> Result<(), Error> {
    // {distribution}-{version}.dist-info/WHEEL is metadata about the archive itself in the same basic key: value format:
    let data = parse_key_value_file(&mut wheel_text.as_bytes(), "WHEEL")?;

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
        return Ok(());
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
    Ok(())
}

/// Call `python -m compileall` to generate pyc file for the installed code
///
/// 2.f Compile any installed .py to .pyc. (Uninstallers should be smart enough to remove .pyc
/// even if it is not mentioned in RECORD.)
#[instrument(skip_all)]
fn bytecode_compile(
    site_packages: &Path,
    unpacked_paths: Vec<PathBuf>,
    python_version: (u8, u8),
    sys_executable: &Path,
    // Only for logging
    name: &str,
    record: &mut Vec<RecordEntry>,
) -> Result<(), Error> {
    // https://github.com/pypa/pip/blob/b5457dfee47dd9e9f6ec45159d9d410ba44e5ea1/src/pip/_internal/operations/install/wheel.py#L592-L603
    let py_source_paths: Vec<_> = unpacked_paths
        .into_iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("py"))
                && site_packages.join(path).is_file()
        })
        .collect();

    // bytecode compiling crashes non-deterministically with various errors, from syntax errors
    // to cpython segmentation faults, so we add a simple retry loop
    let mut retries = 3;
    let (status, lines) = loop {
        let (status, lines) =
            bytecode_compile_inner(site_packages, &py_source_paths, sys_executable)?;
        retries -= 1;
        if status.success() || retries == 0 {
            break (status, lines);
        }

        warn!("Failed to compile {name} with python compileall, retrying",);
    };
    if !status.success() {
        // lossy because we want the error reporting to survive c̴̞̏ü̸̜̹̈́ŕ̴͉̈ś̷̤ė̵̤͋d̷͙̄ filenames in the zip
        return Err(Error::PythonSubcommand(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to run python compileall, log above: {status}"),
        )));
    }

    // like pip, we just ignored all that failed to compile
    // Add each that succeeded to the RECORD
    for py_path in lines {
        let py_path = py_path.trim();
        if py_path.is_empty() {
            continue;
        }
        let py_path = Path::new(py_path);
        let pyc_path = py_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join("__pycache__")
            // Unwrap is save because we checked for an extension before
            .join(py_path.file_name().unwrap())
            .with_extension(format!(
                "cpython-{}{}.pyc",
                python_version.0, python_version.1
            ));
        if !site_packages.join(&pyc_path).is_file() {
            return Err(Error::PythonSubcommand(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "Didn't find pyc generated by compileall: {}",
                    site_packages.join(&pyc_path).normalized_display()
                ),
            )));
        }
        // 2.d Update distribution-1.0.dist-info/RECORD with the installed paths.

        // https://www.python.org/dev/peps/pep-0376/#record
        // > [..] a hash of the file's contents. Notice that pyc and pyo generated files don't have
        // > any hash because they are automatically produced from py files. So checking the hash of
        // > the corresponding py file is enough to decide if the file and its associated pyc or pyo
        // > files have changed.
        record.push(RecordEntry {
            path: pyc_path.display().to_string(),
            hash: None,
            size: None,
        });
    }

    Ok(())
}

/// The actual command part which we repeat if it fails
fn bytecode_compile_inner(
    site_packages: &Path,
    py_source_paths: &[PathBuf],
    sys_executable: &Path,
) -> Result<(ExitStatus, Vec<String>), Error> {
    let temp_dir = tempdir()?;
    // Running python with an actual file will produce better error messages
    let pip_compileall_py = temp_dir.path().join("pip_compileall.py");
    fs::write(&pip_compileall_py, include_str!("pip_compileall.py"))?;
    // We input the paths through stdin and get the successful paths returned through stdout
    let mut bytecode_compiler = Command::new(sys_executable)
        .arg(&pip_compileall_py)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .current_dir(site_packages.normalized())
        .spawn()
        .map_err(Error::PythonSubcommand)?;

    // https://stackoverflow.com/questions/49218599/write-to-child-process-stdin-in-rust/49597789#comment120223107_49597789
    let mut child_stdin = bytecode_compiler
        .stdin
        .take()
        .expect("Child must have stdin");

    // Pass paths newline terminated to compileall
    for path in py_source_paths {
        debug!("bytecode compiling {}", path.display());
        // There is no OsStr -> Bytes conversion on windows :o
        // https://stackoverflow.com/questions/43083544/how-can-i-convert-osstr-to-u8-vecu8-on-windows
        writeln!(&mut child_stdin, "{}", path.display()).map_err(Error::PythonSubcommand)?;
    }
    // Close stdin to finish and avoid indefinite blocking
    drop(child_stdin);

    // Already read stdout here to avoid it running full (pipes are limited)
    let stdout = bytecode_compiler.stdout.take().unwrap();
    let mut lines: Vec<String> = Vec::new();
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|err| {
            Error::PythonSubcommand(io::Error::new(
                io::ErrorKind::Other,
                format!("Invalid utf-8 returned by python compileall: {err}"),
            ))
        })?;
        lines.push(line);
    }

    let output = bytecode_compiler
        .wait_with_output()
        .map_err(Error::PythonSubcommand)?;
    Ok((output.status, lines))
}

/// Give the path relative to the base directory
///
/// lib/python/site-packages/foo/__init__.py and lib/python/site-packages -> foo/__init__.py
/// lib/marker.txt and lib/python/site-packages -> ../../marker.txt
/// `bin/foo_launcher` and lib/python/site-packages -> ../../../`bin/foo_launcher`
pub fn relative_to(path: &Path, base: &Path) -> Result<PathBuf, Error> {
    // Find the longest common prefix, and also return the path stripped from that prefix
    let (stripped, common_prefix) = base
        .ancestors()
        .find_map(|ancestor| {
            path.strip_prefix(ancestor)
                .ok()
                .map(|stripped| (stripped, ancestor))
        })
        .ok_or_else(|| {
            Error::Io(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Trivial strip failed: {} vs. {}",
                    path.normalized_display(),
                    base.normalized_display()
                ),
            ))
        })?;

    // go as many levels up as required
    let levels_up = base.components().count() - common_prefix.components().count();
    let up = iter::repeat("..").take(levels_up).collect::<PathBuf>();

    Ok(up.join(stripped))
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
                        relative_to_site_packages.normalized_display(),
                        src.normalized_display()
                    ))
                })?;
            entry.path = relative_to(&target, site_packages)?.display().to_string();
        }
    }
    Ok(())
}

/// Installs a single script (not an entrypoint)
///
/// Has to deal with both binaries files (just move) and scripts (rewrite the shebang if applicable)
fn install_script(
    site_packages: &Path,
    record: &mut [RecordEntry],
    file: &DirEntry,
    location: &InstallLocation<impl AsRef<Path>>,
) -> Result<(), Error> {
    if !file.file_type()?.is_file() {
        return Err(Error::InvalidWheel(format!(
            "Wheel contains entry in scripts directory that is not a file: {}",
            file.path().display()
        )));
    }

    let target_path = bin_rel().join(file.file_name());

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
        let start = get_shebang(location).as_bytes().to_vec();
        let mut target = File::create(site_packages.join(&target_path))?;
        let size_and_encoded_hash = copy_and_hash(&mut start.chain(script), &mut target)?;
        fs::remove_file(&path)?;
        Some(size_and_encoded_hash)
    } else {
        // reading and writing is slow especially for large binaries, so we move them instead
        drop(script);
        fs::rename(&path, site_packages.join(&target_path))?;
        None
    };
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(
            site_packages.join(&target_path),
            Permissions::from_mode(0o755),
        )?;
    }

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
                relative_to_site_packages.normalized_display(),
                path.normalized_display()
            ))
        })?;
    entry.path = target_path.display().to_string();
    if let Some((size, encoded_hash)) = size_and_encoded_hash {
        entry.size = Some(size);
        entry.hash = Some(encoded_hash);
    }
    Ok(())
}

/// Move the files from the .data directory to the right location in the venv
#[allow(clippy::too_many_arguments)]
#[instrument(skip_all)]
pub(crate) fn install_data(
    venv_root: &Path,
    site_packages: &Path,
    data_dir: &Path,
    dist_name: &str,
    location: &InstallLocation<impl AsRef<Path>>,
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
                move_folder_recorded(&path, venv_root, site_packages, record)?;
            }
            Some("scripts") => {
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
                        .any(|script| script.script_name == match_name)
                    {
                        continue;
                    }

                    install_script(site_packages, record, &file, location)?;
                }
            }
            Some("headers") => {
                let target_path = venv_root
                    .join("include")
                    .join("site")
                    .join(format!(
                        "python{}.{}",
                        location.python_version().0,
                        location.python_version().1
                    ))
                    .join(dist_name);
                move_folder_recorded(&path, &target_path, site_packages, record)?;
            }
            Some("purelib" | "platlib") => {
                // purelib and platlib locations are not relevant when using venvs
                // https://stackoverflow.com/a/27882460/3549270
                move_folder_recorded(&path, site_packages, site_packages, record)?;
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
    File::create(site_packages.join(relative_path))?.write_all(content.as_ref())?;
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
    write_file_recorded(
        site_packages,
        &dist_info_dir.join("INSTALLER"),
        env!("CARGO_PKG_NAME"),
        record,
    )?;
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
pub fn parse_key_value_file(
    file: &mut impl Read,
    debug_filename: &str,
) -> Result<FxHashMap<String, Vec<String>>, Error> {
    let mut data: FxHashMap<String, Vec<String>> = FxHashMap::default();

    let file = BufReader::new(file);
    for (line_no, line) in file.lines().enumerate() {
        let line = line?.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once(": ").ok_or_else(|| {
            Error::InvalidWheel(format!(
                "Line {line_no} of the {debug_filename} file is invalid"
            ))
        })?;
        data.entry(key.to_string())
            .or_default()
            .push(value.to_string());
    }
    Ok(data)
}

/// Install the given wheel to the given venv
///
/// The caller must ensure that the wheel is compatible to the environment.
///
/// <https://packaging.python.org/en/latest/specifications/binary-distribution-format/#installing-a-wheel-distribution-1-0-py32-none-any-whl>
///
/// Wheel 1.0: <https://www.python.org/dev/peps/pep-0427/>
#[allow(clippy::too_many_arguments)]
#[instrument(skip_all, fields(name = %filename.name))]
pub fn install_wheel(
    location: &InstallLocation<LockedDir>,
    reader: impl Read + Seek,
    filename: &WheelFilename,
    direct_url: Option<&DirectUrl>,
    installer: Option<&str>,
    compile: bool,
    check_hashes: bool,
    // initially used to the console scripts, currently unused. Keeping it because we likely need
    // it for validation later.
    _extras: &[String],
    sys_executable: impl AsRef<Path>,
) -> Result<String, Error> {
    let name = &filename.name;

    let base_location = location.venv_root();

    let site_packages_python = format!(
        "python{}.{}",
        location.python_version().0,
        location.python_version().1
    );
    let site_packages = if cfg!(target_os = "windows") {
        base_location.as_ref().join("Lib").join("site-packages")
    } else {
        base_location
            .as_ref()
            .join("lib")
            .join(site_packages_python)
            .join("site-packages")
    };

    debug!(name = name.as_ref(), "Opening zip");
    // No BufReader: https://github.com/zip-rs/zip/issues/381
    let mut archive = ZipArchive::new(reader).map_err(|err| {
        let file = "(index)".to_string();
        Error::Zip(file, err)
    })?;

    debug!(name = name.as_ref(), "Getting wheel metadata");
    let dist_info_prefix = find_dist_info(filename, archive.file_names().map(|name| (name, name)))?
        .1
        .to_string();
    let metadata = dist_info_metadata(&dist_info_prefix, &mut archive)?;
    let (name, version) = parse_metadata(&dist_info_prefix, &metadata)?;

    // Validate the wheel name and version.
    {
        let name = PackageName::from_str(&name)?;
        if name != filename.name {
            return Err(Error::MismatchedName(name, filename.name.clone()));
        }

        let version = Version::from_str(&version)?;
        if version != filename.version {
            return Err(Error::MismatchedVersion(version, filename.version.clone()));
        }
    }

    let record_path = format!("{dist_info_prefix}.dist-info/RECORD");
    let mut record = read_record_file(&mut archive.by_name(&record_path).map_err(|err| {
        let file = record_path.clone();
        Error::Zip(file, err)
    })?)?;

    // We're going step by step though
    // https://packaging.python.org/en/latest/specifications/binary-distribution-format/#installing-a-wheel-distribution-1-0-py32-none-any-whl
    // > 1.a Parse distribution-1.0.dist-info/WHEEL.
    // > 1.b Check that installer is compatible with Wheel-Version. Warn if minor version is greater, abort if major version is greater.
    let wheel_file_path = format!("{dist_info_prefix}.dist-info/WHEEL");
    let wheel_file = archive
        .by_name(&wheel_file_path)
        .map_err(|err| Error::Zip(wheel_file_path, err))?;
    let wheel_text = io::read_to_string(wheel_file)?;
    parse_wheel_version(&wheel_text)?;
    // > 1.c If Root-Is-Purelib == ‘true’, unpack archive into purelib (site-packages).
    // > 1.d Else unpack archive into platlib (site-packages).
    // We always install in the same virtualenv site packages
    debug!(name = name.as_str(), "Extracting file");
    let unpacked_paths = unpack_wheel_files(
        &site_packages,
        &record_path,
        &mut archive,
        &record,
        check_hashes,
    )?;
    debug!(
        name = name.as_str(),
        "Extracted {} files",
        unpacked_paths.len()
    );

    debug!(name = name.as_str(), "Writing entrypoints");
    let (console_scripts, gui_scripts) = parse_scripts(&mut archive, &dist_info_prefix, None)?;
    write_script_entrypoints(
        &site_packages,
        location,
        &console_scripts,
        &mut record,
        false,
    )?;
    write_script_entrypoints(&site_packages, location, &gui_scripts, &mut record, true)?;

    let data_dir = site_packages.join(format!("{dist_info_prefix}.data"));
    // 2.a Unpacked archive includes distribution-1.0.dist-info/ and (if there is data) distribution-1.0.data/.
    // 2.b Move each subtree of distribution-1.0.data/ onto its destination path. Each subdirectory of distribution-1.0.data/ is a key into a dict of destination directories, such as distribution-1.0.data/(purelib|platlib|headers|scripts|data). The initially supported paths are taken from distutils.command.install.
    if data_dir.is_dir() {
        debug!(name = name.as_str(), "Installing data");
        install_data(
            base_location.as_ref(),
            &site_packages,
            &data_dir,
            &name,
            location,
            &console_scripts,
            &gui_scripts,
            &mut record,
        )?;
        // 2.c If applicable, update scripts starting with #!python to point to the correct interpreter.
        // Script are unsupported through data
        // 2.e Remove empty distribution-1.0.data directory.
        fs::remove_dir_all(data_dir)?;
    } else {
        debug!(name = name.as_str(), "No data");
    }

    // 2.f Compile any installed .py to .pyc. (Uninstallers should be smart enough to remove .pyc even if it is not mentioned in RECORD.)
    if compile {
        debug!(name = name.as_str(), "Bytecode compiling");
        bytecode_compile(
            &site_packages,
            unpacked_paths,
            location.python_version(),
            sys_executable.as_ref(),
            name.as_str(),
            &mut record,
        )?;
    }

    debug!(name = name.as_str(), "Writing extra metadata");

    extra_dist_info(
        &site_packages,
        &dist_info_prefix,
        true,
        direct_url,
        installer,
        &mut record,
    )?;

    debug!(name = name.as_str(), "Writing record");
    let mut record_writer = csv::WriterBuilder::new()
        .has_headers(false)
        .escape(b'"')
        .from_path(site_packages.join(record_path))?;
    record.sort();
    for entry in record {
        record_writer.serialize(entry)?;
    }

    Ok(filename.get_tag())
}

/// Read the `dist-info` metadata from a wheel archive.
fn dist_info_metadata(
    dist_info_prefix: &str,
    archive: &mut ZipArchive<impl Read + Seek + Sized>,
) -> Result<Vec<u8>, Error> {
    let mut content = Vec::new();
    let dist_info_file = format!("{dist_info_prefix}.dist-info/METADATA");
    archive
        .by_name(&dist_info_file)
        .map_err(|err| Error::Zip(dist_info_file.clone(), err))?
        .read_to_end(&mut content)?;
    Ok(content)
}

/// Parse the distribution name and version from a wheel's `dist-info` metadata.
///
/// See: <https://github.com/PyO3/python-pkginfo-rs>
pub(crate) fn parse_metadata(
    dist_info_prefix: &str,
    content: &[u8],
) -> Result<(String, String), Error> {
    // HACK: trick mailparse to parse as UTF-8 instead of ASCII
    let mut mail = b"Content-Type: text/plain; charset=utf-8\n".to_vec();
    mail.extend_from_slice(content);
    let msg = mailparse::parse_mail(&mail).map_err(|err| {
        Error::InvalidWheel(format!(
            "Invalid metadata in {dist_info_prefix}.dist-info/METADATA: {err}"
        ))
    })?;
    let headers = msg.get_headers();
    let metadata_version =
        headers
            .get_first_value("Metadata-Version")
            .ok_or(Error::InvalidWheel(format!(
                "No `Metadata-Version` field in: {dist_info_prefix}.dist-info/METADATA"
            )))?;
    // Crude but it should do https://packaging.python.org/en/latest/specifications/core-metadata/#metadata-version
    // At time of writing:
    // > Version of the file format; legal values are “1.0”, “1.1”, “1.2”, “2.1”, “2.2”, and “2.3”.
    if !(metadata_version.starts_with("1.") || metadata_version.starts_with("2.")) {
        return Err(Error::InvalidWheel(format!(
            "`Metadata-Version` field has unsupported value {metadata_version} in: {dist_info_prefix}.dist-info/METADATA"
        )));
    }
    let name = headers
        .get_first_value("Name")
        .ok_or(Error::InvalidWheel(format!(
            "No `Name` field in: {dist_info_prefix}.dist-info/METADATA"
        )))?;
    let version = headers
        .get_first_value("Version")
        .ok_or(Error::InvalidWheel(format!(
            "No `Version` field in: {dist_info_prefix}.dist-info/METADATA"
        )))?;
    Ok((name, version))
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use indoc::{formatdoc, indoc};

    use super::{parse_key_value_file, parse_wheel_version, read_record_file, relative_to, Script};

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
        parse_wheel_version(&wheel_with_version("1.0")).unwrap();
        parse_wheel_version(&wheel_with_version("2.0")).unwrap_err();
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
    fn test_relative_to() {
        assert_eq!(
            relative_to(
                Path::new("/home/ferris/carcinization/lib/python/site-packages/foo/__init__.py"),
                Path::new("/home/ferris/carcinization/lib/python/site-packages"),
            )
            .unwrap(),
            Path::new("foo/__init__.py")
        );
        assert_eq!(
            relative_to(
                Path::new("/home/ferris/carcinization/lib/marker.txt"),
                Path::new("/home/ferris/carcinization/lib/python/site-packages"),
            )
            .unwrap(),
            Path::new("../../marker.txt")
        );
        assert_eq!(
            relative_to(
                Path::new("/home/ferris/carcinization/bin/foo_launcher"),
                Path::new("/home/ferris/carcinization/lib/python/site-packages"),
            )
            .unwrap(),
            Path::new("../../../bin/foo_launcher")
        );
    }

    #[test]
    fn test_script_from_value() {
        assert_eq!(
            Script::from_value("launcher", "foo.bar:main", None).unwrap(),
            Some(Script {
                script_name: "launcher".to_string(),
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
                script_name: "launcher".to_string(),
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
                script_name: "launcher".to_string(),
                module: "foomod".to_string(),
                function: "main_bar".to_string(),
            })
        );
    }

    #[test]
    #[cfg(all(windows, target_arch = "x86_64"))]
    fn test_launchers_are_small() {
        // At time of writing, they are 15872 bytes.
        assert!(
            super::LAUNCHER_X86_64_GUI.len() < 20 * 1024,
            "GUI launcher: {}",
            super::LAUNCHER_X86_64_GUI.len()
        );
        assert!(
            super::LAUNCHER_X86_64_CONSOLE.len() < 20 * 1024,
            "CLI launcher: {}",
            super::LAUNCHER_X86_64_CONSOLE.len()
        );
    }

    #[test]
    #[cfg(all(windows, target_arch = "aarch64"))]
    fn test_launchers_are_small() {
        // At time of writing, they are 14848 and 14336 bytes.
        assert!(
            super::LAUNCHER_AArch64_GUI.len() < 20 * 1024,
            "GUI launcher: {}",
            super::LAUNCHER_AArch64_GUI.len()
        );
        assert!(
            super::LAUNCHER_AArch64_CONSOLE.len() < 20 * 1024,
            "CLI launcher: {}",
            super::LAUNCHER_AArch64_CONSOLE.len()
        );
    }
}
