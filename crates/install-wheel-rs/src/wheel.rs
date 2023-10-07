#![allow(clippy::needless_borrow)]

use crate::install_location::{InstallLocation, LockedDir};
use crate::wheel_tags::WheelFilename;
use crate::{normalize_name, Error};
use configparser::ini::Ini;
use data_encoding::BASE64URL_NOPAD;
use fs_err as fs;
use fs_err::{DirEntry, File};
use mailparse::MailHeaderMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io::{BufRead, BufReader, BufWriter, Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::{env, io, iter};
use tempfile::{tempdir, TempDir};
use tracing::{debug, error, span, warn, Level};
use walkdir::WalkDir;
use zip::result::ZipError;
use zip::write::FileOptions;
use zip::{ZipArchive, ZipWriter};

/// `#!/usr/bin/env python`
pub const SHEBANG_PYTHON: &str = "#!/usr/bin/env python";

pub const LAUNCHER_T32: &[u8] = include_bytes!("../windows-launcher/t32.exe");
pub const LAUNCHER_T64: &[u8] = include_bytes!("../windows-launcher/t64.exe");
pub const LAUNCHER_T64_ARM: &[u8] = include_bytes!("../windows-launcher/t64-arm.exe");

/// Line in a RECORD file
/// <https://www.python.org/dev/peps/pep-0376/#record>
///
/// ```csv
/// tqdm/cli.py,sha256=x_c8nmc4Huc-lKEsAXj78ZiyqSJ9hJ71j7vltY67icw,10509
/// tqdm-4.62.3.dist-info/RECORD,,
/// ```
#[derive(Deserialize, Serialize, PartialOrd, PartialEq, Ord, Eq)]
pub struct RecordEntry {
    pub path: String,
    pub hash: Option<String>,
    #[allow(dead_code)]
    pub size: Option<usize>,
}

/// Minimal direct_url.json schema
///
/// <https://packaging.python.org/en/latest/specifications/direct-url/>
/// <https://www.python.org/dev/peps/pep-0610/>
#[derive(Serialize)]
struct DirectUrl {
    archive_info: HashMap<(), ()>,
    url: String,
}

/// A script defining the name of the runnable entrypoint and the module and function that should be
/// run.
#[cfg(feature = "python_bindings")]
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[pyo3::pyclass(dict)]
pub struct Script {
    #[pyo3(get)]
    pub script_name: String,
    #[pyo3(get)]
    pub module: String,
    #[pyo3(get)]
    pub function: String,
}

/// A script defining the name of the runnable entrypoint and the module and function that should be
/// run.
#[cfg(not(feature = "python_bindings"))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Script {
    pub script_name: String,
    pub module: String,
    pub function: String,
}

impl Script {
    /// Parses a script definition like `foo.bar:main` or `foomod:main_bar [bar,baz]`
    ///
    /// <https://packaging.python.org/en/latest/specifications/entry-points/>
    ///
    /// Extras are supposed to be ignored, which happens if you pass None for extras
    pub fn from_value(
        script_name: &str,
        value: &str,
        extras: Option<&[String]>,
    ) -> Result<Option<Script>, Error> {
        let script_regex = Regex::new(r"^(?P<module>[\w\d_\-.]+):(?P<function>[\w\d_\-.]+)(?:\s+\[(?P<extras>(?:[^,]+,?\s*)+)\])?$").unwrap();

        let captures = script_regex
            .captures(value)
            .ok_or_else(|| Error::InvalidWheel(format!("invalid console script: '{}'", value)))?;
        if let Some(script_extras) = captures.name("extras") {
            let script_extras = script_extras
                .as_str()
                .split(',')
                .map(|extra| extra.trim().to_string())
                .collect::<HashSet<String>>();
            if let Some(extras) = extras {
                if !script_extras.is_subset(&extras.iter().cloned().collect()) {
                    return Ok(None);
                }
            }
        }

        Ok(Some(Script {
            script_name: script_name.to_string(),
            module: captures.name("module").unwrap().as_str().to_string(),
            function: captures.name("function").unwrap().as_str().to_string(),
        }))
    }
}

/// Wrapper script template function
///
/// <https://github.com/pypa/pip/blob/7f8a6844037fb7255cfd0d34ff8e8cf44f2598d4/src/pip/_vendor/distlib/scripts.py#L41-L48>
pub fn get_script_launcher(module: &str, import_name: &str, shebang: &str) -> String {
    format!(
        r##"{shebang}
# -*- coding: utf-8 -*-
import re
import sys
from {module} import {import_name}
if __name__ == "__main__":
    sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
    sys.exit({import_name}())
"##,
        shebang = shebang,
        module = module,
        import_name = import_name
    )
}

/// Part of entrypoints parsing
fn read_scripts_from_section(
    scripts_section: &HashMap<String, Option<String>>,
    section_name: &str,
    extras: Option<&[String]>,
) -> Result<Vec<Script>, Error> {
    let mut scripts = Vec::new();
    for (script_name, python_location) in scripts_section.iter() {
        match python_location {
            Some(value) => {
                if let Some(script) = Script::from_value(script_name, value, extras)? {
                    scripts.push(script);
                }
            }
            None => {
                return Err(Error::InvalidWheel(format!(
                    "[{}] key {} must have a value",
                    section_name, script_name
                )));
            }
        }
    }
    Ok(scripts)
}

/// Parses the entry_points.txt entry in the wheel for console scripts
///
/// Returns (script_name, module, function)
///
/// Extras are supposed to be ignored, which happens if you pass None for extras
fn parse_scripts<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    dist_info_prefix: &str,
    extras: Option<&[String]>,
) -> Result<(Vec<Script>, Vec<Script>), Error> {
    let entry_points_path = format!("{dist_info_prefix}.dist-info/entry_points.txt");
    let entry_points_mapping = match archive.by_name(&entry_points_path) {
        Ok(mut file) => {
            let mut ini_text = String::new();
            file.read_to_string(&mut ini_text)?;
            Ini::new_cs().read(ini_text).map_err(|err| {
                Error::InvalidWheel(format!("entry_points.txt is invalid: {}", err))
            })?
        }
        Err(ZipError::FileNotFound) => return Ok((Vec::new(), Vec::new())),
        Err(err) => return Err(Error::from_zip_error(entry_points_path, err)),
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
pub fn copy_and_hash(reader: &mut impl Read, writer: &mut impl Write) -> io::Result<(u64, String)> {
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
    let mut created_dirs = HashSet::new();
    // https://github.com/zip-rs/zip/blob/7edf2489d5cff8b80f02ee6fc5febf3efd0a9442/examples/extract.rs
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|err| Error::from_zip_error(format!("(index {i})"), err))?;
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

        if let Some(p) = out_path.parent() {
            if !created_dirs.contains(p) {
                fs::create_dir_all(&p)?;
                created_dirs.insert(p.to_path_buf());
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
                        relative.display(),
                        encoded_hash
                    ))
                })?;
            if recorded_hash != &encoded_hash {
                if relative.as_os_str().to_string_lossy().starts_with("torch-") {
                    error!(
                        "Hash mismatch for {}. Recorded: {}, Actual: {}",
                        relative.display(),
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
                    relative.display(),
                    recorded_hash,
                    encoded_hash,
                )));
            }
        }
    }
    Ok(extracted_paths)
}

fn get_shebang(location: &InstallLocation<LockedDir>) -> String {
    if matches!(location, InstallLocation::Venv { .. }) {
        let path = location.get_python().display().to_string();
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
        format!("#!{}", path)
    } else {
        // This will use the monotrail binary moonlighting as python. `python` alone doesn't,
        // we need env to find the python link we put in PATH
        SHEBANG_PYTHON.to_string()
    }
}

/// Ported from https://github.com/pypa/pip/blob/fd0ea6bc5e8cb95e518c23d901c26ca14db17f89/src/pip/_vendor/distlib/scripts.py#L248-L262
///
/// To get a launcher on windows we write a minimal .exe launcher binary and then attach the actual
/// python after it.
///
/// TODO pyw scripts
///
/// TODO: a nice, reproducible-without-distlib rust solution
fn windows_script_launcher(launcher_python_script: &str) -> Result<Vec<u8>, Error> {
    let launcher_bin = match env::consts::ARCH {
        "x84" => LAUNCHER_T32,
        "x86_64" => LAUNCHER_T64,
        "aarch64" => LAUNCHER_T64_ARM,
        arch => {
            let error = format!(
                "Don't know how to create windows launchers for script for {}, \
                        only x86, x86_64 and aarch64 (64-bit arm) are supported",
                arch
            );
            return Err(Error::OsVersionDetection(error));
        }
    };

    let mut stream: Vec<u8> = Vec::new();
    {
        // We're using the zip writer, but it turns out we're not actually deflating apparently
        // we're just using an offset
        // https://github.com/pypa/distlib/blob/8ed03aab48add854f377ce392efffb79bb4d6091/PC/launcher.c#L259-L271
        let stored = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut archive = ZipWriter::new(Cursor::new(&mut stream));
        let error_msg = "Writing to Vec<u8> should never fail";
        archive.start_file("__main__.py", stored).expect(error_msg);
        archive
            .write_all(launcher_python_script.as_bytes())
            .expect(error_msg);
        archive.finish().expect(error_msg);
    }

    let mut launcher: Vec<u8> = launcher_bin.to_vec();
    launcher.append(&mut stream);
    Ok(launcher)
}

/// Create the wrapper scripts in the bin folder of the venv for launching console scripts
///
/// We also pass venv_base so we can write the same path as pip does
///
/// TODO: Test for this launcher directly in install-wheel-rs
fn write_script_entrypoints(
    site_packages: &Path,
    location: &InstallLocation<LockedDir>,
    entrypoints: &[Script],
    record: &mut Vec<RecordEntry>,
) -> Result<(), Error> {
    // for monotrail
    fs::create_dir_all(site_packages.join(&bin_rel()))?;
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
        let launcher_python_script = get_script_launcher(
            &entrypoint.module,
            &entrypoint.function,
            &get_shebang(&location),
        );
        if cfg!(windows) {
            let launcher = windows_script_launcher(&launcher_python_script)?;
            write_file_recorded(site_packages, &entrypoint_relative, &launcher, record)?;
        } else {
            write_file_recorded(
                site_packages,
                &entrypoint_relative,
                &launcher_python_script,
                record,
            )?;
            // We need to make the launcher executable
            #[cfg(target_family = "unix")]
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
fn parse_wheel_version(wheel_text: &str) -> Result<(), Error> {
    // {distribution}-{version}.dist-info/WHEEL is metadata about the archive itself in the same basic key: value format:
    let data = parse_key_value_file(&mut wheel_text.as_bytes(), "WHEEL")?;

    let wheel_version = if let Some(wheel_version) =
        data.get("Wheel-Version").and_then(|wheel_versions| {
            if let [wheel_version] = wheel_versions.as_slice() {
                wheel_version.split_once('.')
            } else {
                None
            }
        }) {
        wheel_version
    } else {
        return Err(Error::InvalidWheel(
            "Invalid Wheel-Version in WHEEL file".to_string(),
        ));
    };
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
        eprint!(
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
            site_packages.join(path).is_file() && path.extension() == Some(&OsString::from("py"))
        })
        .collect();

    // bytecode compiling crashes non-deterministically with various errors, from syntax errors
    // to cpython segmentation faults, so we add a simple retry loop
    let mut retries = 3;
    let (status, lines) = loop {
        let (status, lines) =
            bytecode_compile_inner(site_packages, &py_source_paths, &sys_executable)?;
        retries -= 1;
        if status.success() || retries == 0 {
            break (status, lines);
        } else {
            warn!(
                "Failed to compile {} with python compileall, retrying",
                name,
            );
        }
    };
    if !status.success() {
        // lossy because we want the error reporting to survive c̴̞̏ü̸̜̹̈́ŕ̴͉̈ś̷̤ė̵̤͋d̷͙̄ filenames in the zip
        return Err(Error::PythonSubcommand(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to run python compileall, log above: {}", status),
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
                    site_packages.join(&pyc_path).display()
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
        })
    }

    Ok(())
}

/// The actual command part which we repeat if it fails
fn bytecode_compile_inner(
    site_packages: &Path,
    py_source_paths: &[PathBuf],
    sys_executable: &Path,
) -> Result<(ExitStatus, Vec<String>), Error> {
    let tempdir = tempdir()?;
    // Running python with an actual file will produce better error messages
    let pip_compileall_py = tempdir.path().join("pip_compileall.py");
    fs::write(&pip_compileall_py, include_str!("pip_compileall.py"))?;
    // We input the paths through stdin and get the successful paths returned through stdout
    let mut bytecode_compiler = Command::new(sys_executable)
        .arg(&pip_compileall_py)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .current_dir(&site_packages)
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
                format!("Invalid utf-8 returned by python compileall: {}", err),
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
/// bin/foo_launcher and lib/python/site-packages -> ../../../bin/foo_launcher
pub fn relative_to(path: &Path, base: &Path) -> Result<PathBuf, Error> {
    // Find the longest common prefix, and also return the path stripped from that prefix
    let (stripped, common_prefix) = base
        .ancestors()
        .filter_map(|ancestor| {
            path.strip_prefix(ancestor)
                .ok()
                .map(|stripped| (stripped, ancestor))
        })
        .next()
        .ok_or_else(|| {
            Error::IO(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "trivial strip case should have worked: {} vs {}",
                    path.display(),
                    base.display()
                ),
            ))
        })?;

    // go as many levels up as required
    let levels_up = base.components().count() - common_prefix.components().count();
    let up = iter::repeat("..").take(levels_up).collect::<PathBuf>();

    Ok(up.join(stripped))
}

/// Moves the files and folders in src to dest, updating the RECORD in the process
fn move_folder_recorded(
    src_dir: &Path,
    dest_dir: &Path,
    site_packages: &Path,
    record: &mut [RecordEntry],
) -> Result<(), Error> {
    if !dest_dir.is_dir() {
        fs::create_dir_all(&dest_dir)?;
    }
    for entry in WalkDir::new(&src_dir) {
        let entry = entry?;
        let src = entry.path();
        // This is the base path for moving to the actual target for the data
        // e.g. for data it's without <..>.data/data/
        let relative_to_data = src.strip_prefix(&src_dir).expect("Prefix must no change");
        // This is the path stored in RECORD
        // e.g. for data it's with .data/data/
        let relative_to_site_packages = src
            .strip_prefix(site_packages)
            .expect("Prefix must no change");
        let target = dest_dir.join(relative_to_data);
        if src.is_dir() {
            if !target.is_dir() {
                fs::create_dir(target)?;
            }
        } else {
            fs::rename(src, &target)?;
            let entry = record
                .iter_mut()
                .find(|entry| Path::new(&entry.path) == relative_to_site_packages)
                .ok_or_else(|| {
                    Error::RecordFile(format!(
                        "Could not find entry for {} ({})",
                        relative_to_site_packages.display(),
                        src.display()
                    ))
                })?;
            entry.path = relative_to(&target, &site_packages)?.display().to_string();
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
    file: DirEntry,
    location: &InstallLocation<LockedDir>,
) -> Result<(), Error> {
    let path = file.path();
    if !path.is_file() {
        return Err(Error::InvalidWheel(format!(
            "Wheel contains entry in scripts directory that is not a file: {}",
            path.display()
        )));
    }

    let target_path = bin_rel().join(file.file_name());
    let mut script = File::open(&path)?;
    // https://sphinx-locales.github.io/peps/pep-0427/#recommended-installer-features
    // > In wheel, scripts are packaged in {distribution}-{version}.data/scripts/.
    // > If the first line of a file in scripts/ starts with exactly b'#!python',
    // > rewrite to point to the correct interpreter. Unix installers may need to
    // > add the +x bit to these files if the archive was created on Windows.
    //
    // > The b'#!pythonw' convention is allowed. b'#!pythonw' indicates a GUI script
    // > instead of a console script.
    //
    // We do this in venvs as required, but in monotrail mode we use a fake shebang
    // (#!/usr/bin/env python) for injection monotrail as python into PATH later
    let placeholder_python = b"#!python";
    // scripts might be binaries, so we read an exact number of bytes instead of the first line as string
    let mut start = Vec::new();
    start.resize(placeholder_python.len(), 0);
    script.read_exact(&mut start)?;
    let size_and_encoded_hash = if start == placeholder_python {
        let start = get_shebang(&location).as_bytes().to_vec();
        let mut target = File::create(site_packages.join(&target_path))?;
        let size_and_encoded_hash = copy_and_hash(&mut start.chain(script), &mut target)?;
        fs::remove_file(&path)?;
        Some(size_and_encoded_hash)
    } else {
        // reading and writing is slow especially for large binaries, so we move them instead
        drop(script);
        fs::rename(&path, &site_packages.join(&target_path))?;
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
                relative_to_site_packages.display(),
                path.display()
            ))
        })?;
    entry.path = target_path.display().to_string();
    if let Some((size, encoded_hash)) = size_and_encoded_hash {
        entry.size = Some(size as usize);
        entry.hash = Some(encoded_hash);
    }
    Ok(())
}

/// Move the files from the .data directory to the right location in the venv
#[allow(clippy::too_many_arguments)]
fn install_data(
    venv_base: &Path,
    site_packages: &Path,
    data_dir: &Path,
    dist_name: &str,
    location: &InstallLocation<LockedDir>,
    console_scripts: &[Script],
    gui_scripts: &[Script],
    record: &mut [RecordEntry],
) -> Result<(), Error> {
    for data_entry in fs::read_dir(data_dir)? {
        let data_entry = data_entry?;
        match data_entry.file_name().as_os_str().to_str() {
            Some("data") => {
                // Move the content of the folder to the root of the venv
                move_folder_recorded(&data_entry.path(), venv_base, site_packages, record)?;
            }
            Some("scripts") => {
                for file in fs::read_dir(data_entry.path())? {
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

                    install_script(site_packages, record, file, &location)?;
                }
            }
            Some("headers") => {
                let target_path = venv_base
                    .join("include")
                    .join("site")
                    // TODO: Also use just python here in monotrail
                    .join(format!(
                        "python{}.{}",
                        location.get_python_version().0,
                        location.get_python_version().1
                    ))
                    .join(dist_name);
                move_folder_recorded(&data_entry.path(), &target_path, site_packages, record)?;
            }
            Some("purelib" | "platlib") => {
                // purelib and platlib locations are not relevant when using venvs
                // https://stackoverflow.com/a/27882460/3549270
                move_folder_recorded(&data_entry.path(), site_packages, site_packages, record)?;
            }
            _ => {
                return Err(Error::InvalidWheel(format!(
                    "Unknown wheel data type: {:?}",
                    data_entry.file_name()
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
fn write_file_recorded(
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
        size: Some(content.as_ref().len()),
    });
    Ok(())
}

/// Adds INSTALLER, REQUESTED and direct_url.json to the .dist-info dir
fn extra_dist_info(
    site_packages: &Path,
    dist_info_prefix: &str,
    requested: bool,
    record: &mut Vec<RecordEntry>,
) -> Result<(), Error> {
    write_file_recorded(
        site_packages,
        &PathBuf::from(format!("{dist_info_prefix}.dist-info")).join("INSTALLER"),
        env!("CARGO_PKG_NAME"),
        record,
    )?;
    if requested {
        write_file_recorded(
            site_packages,
            &PathBuf::from(format!("{dist_info_prefix}.dist-info")).join("REQUESTED"),
            "",
            record,
        )?;
    }

    // https://github.com/python-poetry/poetry/issues/6356
    /*
    let wheel_path_url = format!("file://{}", wheel_path.canonicalize()?.display());
    let direct_url = DirectUrl {
        archive_info: HashMap::new(),
        url: wheel_path_url,
    };

    // Map explicitly because we special cased that error
    let direct_url_json =
        serde_json::to_string(&direct_url).map_err(WheelInstallerError::DirectUrlSerdeJsonError)?;
    write_file_recorded(
        site_packages,
        &dist_info.join("direct_url.json"),
        &direct_url_json,
        record,
    )?;
    */
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
) -> Result<HashMap<String, Vec<String>>, Error> {
    let mut data = HashMap::new();

    let file = BufReader::new(file);
    for (line_no, line) in file.lines().enumerate() {
        let line = line?.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once(": ").ok_or_else(|| {
            Error::InvalidWheel(format!(
                "Line {} of the {} file is invalid",
                line_no, debug_filename
            ))
        })?;
        data.entry(key.to_string())
            .or_insert_with(Vec::new)
            .push(value.to_string())
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
pub fn install_wheel(
    location: &InstallLocation<LockedDir>,
    reader: impl Read + Seek,
    filename: WheelFilename,
    compile: bool,
    check_hashes: bool,
    // initially used to the console scripts, currently unused. Keeping it because we likely need
    // it for validation later
    _extras: &[String],
    unique_version: &str,
    sys_executable: impl AsRef<Path>,
) -> Result<String, Error> {
    let name = &filename.distribution;
    let _my_span = span!(Level::DEBUG, "install_wheel", name = name.as_str());

    let (temp_dir_final_location, base_location) = match location {
        InstallLocation::Venv { venv_base, .. } => (None, venv_base.to_path_buf()),
        InstallLocation::Monotrail { monotrail_root, .. } => {
            let name_version_dir = monotrail_root
                .join(normalize_name(name))
                .join(unique_version);
            fs::create_dir_all(&name_version_dir)?;
            let final_location = name_version_dir.join(filename.get_tag());
            // temp dir and rename for atomicity
            // well, except for windows, because there renaming fails for undeterminable reasons
            // with an os error 5 permission denied.
            if cfg!(not(windows)) {
                let temp_dir = TempDir::new_in(&name_version_dir)?;
                let base_location = temp_dir.path().to_path_buf();
                (Some((temp_dir, final_location)), base_location)
            } else {
                fs::create_dir(&final_location)?;
                (None, final_location)
            }
        }
    };

    let site_packages_python = match location {
        InstallLocation::Venv { .. } => {
            format!(
                "python{}.{}",
                location.get_python_version().0,
                location.get_python_version().1
            )
        }
        // Monotrail installation is for multiple python versions (depending on the wheel tag)
        // Potentially needs to be changed to creating pythonx.y symlinks for each python version
        // we use it with (on install in that python version)
        InstallLocation::Monotrail { .. } => "python".to_string(),
    };
    let site_packages = if cfg!(target_os = "windows") {
        base_location.join("Lib").join("site-packages")
    } else {
        base_location
            .join("lib")
            .join(site_packages_python)
            .join("site-packages")
    };

    debug!(name = name.as_str(), "Opening zip");
    // No BufReader: https://github.com/zip-rs/zip/issues/381
    let mut archive =
        ZipArchive::new(reader).map_err(|err| Error::from_zip_error("(index)".to_string(), err))?;

    debug!(name = name.as_str(), "Getting wheel metadata");
    let dist_info_prefix = find_dist_info(&filename, &mut archive)?;
    let (name, _version) = read_metadata(&dist_info_prefix, &mut archive)?;
    // TODO: Check that name and version match

    let record_path = format!("{dist_info_prefix}.dist-info/RECORD");
    let mut record = read_record_file(
        &mut archive
            .by_name(&record_path)
            .map_err(|err| Error::from_zip_error(record_path.clone(), err))?,
    )?;

    // We're going step by step though
    // https://packaging.python.org/en/latest/specifications/binary-distribution-format/#installing-a-wheel-distribution-1-0-py32-none-any-whl
    // > 1.a Parse distribution-1.0.dist-info/WHEEL.
    // > 1.b Check that installer is compatible with Wheel-Version. Warn if minor version is greater, abort if major version is greater.
    let wheel_file_path = format!("{dist_info_prefix}.dist-info/WHEEL");
    let mut wheel_text = String::new();
    archive
        .by_name(&wheel_file_path)
        .map_err(|err| Error::from_zip_error(wheel_file_path, err))?
        .read_to_string(&mut wheel_text)?;
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
    write_script_entrypoints(&site_packages, &location, &console_scripts, &mut record)?;
    write_script_entrypoints(&site_packages, &location, &gui_scripts, &mut record)?;

    let data_dir = site_packages.join(format!("{dist_info_prefix}.data"));
    // 2.a Unpacked archive includes distribution-1.0.dist-info/ and (if there is data) distribution-1.0.data/.
    // 2.b Move each subtree of distribution-1.0.data/ onto its destination path. Each subdirectory of distribution-1.0.data/ is a key into a dict of destination directories, such as distribution-1.0.data/(purelib|platlib|headers|scripts|data). The initially supported paths are taken from distutils.command.install.
    if data_dir.is_dir() {
        debug!(name = name.as_str(), "Installing data");
        install_data(
            &base_location,
            &site_packages,
            &data_dir,
            &name,
            &location,
            &console_scripts,
            &gui_scripts,
            &mut record,
            // For the monotrail install, we want to keep the fake shebang for our own
            // later replacement logic
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
            location.get_python_version(),
            sys_executable.as_ref(),
            name.as_str(),
            &mut record,
        )?;
    }

    debug!(name = name.as_str(), "Writing extra metadata");

    extra_dist_info(&site_packages, &dist_info_prefix, true, &mut record)?;

    debug!(name = name.as_str(), "Writing record");
    let mut record_writer = csv::WriterBuilder::new()
        .has_headers(false)
        .escape(b'"')
        .from_path(site_packages.join(record_path))?;
    record.sort();
    for entry in record {
        record_writer.serialize(entry)?;
    }

    // rename for atomicity
    // well, except for windows, see comment above
    if let Some((_temp_dir, final_location)) = temp_dir_final_location {
        fs::rename(base_location, final_location)?;
    }

    Ok(filename.get_tag())
}

/// From https://github.com/PyO3/python-pkginfo-rs
///
/// The metadata name may be uppercase, while the wheel and dist info names are lowercase, or
/// the metadata name and the dist info name are lowercase, while the wheel name is uppercase.
/// Either way, we just search the wheel for the name
fn find_dist_info(
    filename: &WheelFilename,
    archive: &mut ZipArchive<impl Read + Seek + Sized>,
) -> Result<String, Error> {
    let dist_info_matcher =
        format!("{}-{}", filename.distribution, filename.version).to_lowercase();
    let dist_infos: Vec<_> = archive
        .file_names()
        .filter_map(|name| name.split_once('/'))
        .filter_map(|(dir, file)| Some((dir.strip_suffix(".dist-info")?, file)))
        .filter(|(dir, file)| dir.to_lowercase() == dist_info_matcher && *file == "METADATA")
        .map(|(dir, _file)| dir)
        .collect();
    let dist_info = match dist_infos.as_slice() {
        [] => {
            return Err(Error::InvalidWheel(
                "Missing .dist-info directory".to_string(),
            ))
        }
        [dist_info] => dist_info.to_string(),
        _ => {
            return Err(Error::InvalidWheel(format!(
                "Multiple .dist-info directories: {}",
                dist_infos.join(", ")
            )));
        }
    };
    Ok(dist_info)
}

/// Adapted from https://github.com/PyO3/python-pkginfo-rs
fn read_metadata(
    dist_info_prefix: &str,
    archive: &mut ZipArchive<impl Read + Seek + Sized>,
) -> Result<(String, String), Error> {
    let mut content = Vec::new();
    let metadata_file = format!("{dist_info_prefix}.dist-info/METADATA");
    archive
        .by_name(&metadata_file)
        .map_err(|err| Error::from_zip_error(metadata_file.to_string(), err))?
        .read_to_end(&mut content)?;
    // HACK: trick mailparse to parse as UTF-8 instead of ASCII
    let mut mail = b"Content-Type: text/plain; charset=utf-8\n".to_vec();
    mail.extend_from_slice(&content);
    let msg = mailparse::parse_mail(&mail)
        .map_err(|err| Error::InvalidWheel(format!("Invalid {}: {}", metadata_file, err)))?;
    let headers = msg.get_headers();
    let metadata_version =
        headers
            .get_first_value("Metadata-Version")
            .ok_or(Error::InvalidWheel(format!(
                "No Metadata-Version field in {}",
                metadata_file
            )))?;
    // Crude but it should do https://packaging.python.org/en/latest/specifications/core-metadata/#metadata-version
    // At time of writing:
    // > Version of the file format; legal values are “1.0”, “1.1”, “1.2”, “2.1”, “2.2”, and “2.3”.
    if !(metadata_version.starts_with("1.") || metadata_version.starts_with("2.")) {
        return Err(Error::InvalidWheel(format!(
            "Metadata-Version field has unsupported value {}",
            metadata_version
        )));
    }
    let name = headers
        .get_first_value("Name")
        .ok_or(Error::InvalidWheel(format!(
            "No Name field in {}",
            metadata_file
        )))?;
    let version = headers
        .get_first_value("Version")
        .ok_or(Error::InvalidWheel(format!(
            "No Version field in {}",
            metadata_file
        )))?;
    Ok((name, version))
}

#[cfg(test)]
mod test {
    use super::parse_wheel_version;
    use crate::wheel::{read_record_file, relative_to};
    use crate::{install_wheel, parse_key_value_file, InstallLocation, Script, WheelFilename};
    use fs_err as fs;
    use indoc::{formatdoc, indoc};
    use std::fs::File;
    use std::path::{Path, PathBuf};
    use std::str::FromStr;
    use tempfile::TempDir;

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

    /// Previously `__pycache__` paths were erroneously absolute
    #[test]
    fn installed_paths_relative() {
        let filename = "colander-0.9.9-py2.py3-none-any.whl";
        let wheel = Path::new("../../test-data/wheels").join(filename);
        let temp_dir = TempDir::new().unwrap();
        // TODO: Would be nicer to pick the default python here, but i don't want to launch a
        //  subprocess
        let python = if cfg!(target_os = "windows") {
            PathBuf::from("python.exe")
        } else {
            PathBuf::from("python3.8")
        };
        let install_location = InstallLocation::<PathBuf>::Monotrail {
            monotrail_root: temp_dir.path().to_path_buf(),
            python: python.clone(),
            python_version: (3, 8),
        }
        .acquire_lock()
        .unwrap();
        install_wheel(
            &install_location,
            File::open(wheel).unwrap(),
            WheelFilename::from_str(&filename).unwrap(),
            true,
            true,
            &[],
            "0.9.9",
            &python,
        )
        .unwrap();

        let base = temp_dir
            .path()
            .join("colander")
            .join("0.9.9")
            .join("py2.py3-none-any");
        let mid = if cfg!(windows) {
            base.join("Lib")
        } else {
            base.join("lib").join("python")
        };
        let record = mid
            .join("site-packages")
            .join("colander-0.9.9.dist-info")
            .join("RECORD");
        let record = fs::read_to_string(&record).unwrap();
        for line in record.lines() {
            assert!(!line.starts_with('/'), "{}", line);
        }
    }

    #[test]
    fn test_relative_to() {
        assert_eq!(
            relative_to(
                Path::new("/home/ferris/carcinization/lib/python/site-packages/foo/__init__.py"),
                Path::new("/home/ferris/carcinization/lib/python/site-packages")
            )
            .unwrap(),
            Path::new("foo/__init__.py")
        );
        assert_eq!(
            relative_to(
                Path::new("/home/ferris/carcinization/lib/marker.txt"),
                Path::new("/home/ferris/carcinization/lib/python/site-packages")
            )
            .unwrap(),
            Path::new("../../marker.txt")
        );
        assert_eq!(
            relative_to(
                Path::new("/home/ferris/carcinization/bin/foo_launcher"),
                Path::new("/home/ferris/carcinization/lib/python/site-packages")
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
                Some(&["bar".to_string(), "baz".to_string()])
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
                Some(&["bar".to_string(), "baz".to_string()])
            )
            .unwrap(),
            Some(Script {
                script_name: "launcher".to_string(),
                module: "foomod".to_string(),
                function: "main_bar".to_string(),
            })
        );
    }
}
