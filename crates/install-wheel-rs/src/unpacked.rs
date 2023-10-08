//! Like `wheel.rs`, but for installing wheels that have already been unzipped, rather than
//! reading from a zip file.

use std::io::Read;
use std::path::Path;

use configparser::ini::Ini;
use fs_err as fs;
use fs_err::File;
use mailparse::MailHeaderMap;
use tracing::{debug, span, Level};

use wheel_filename::WheelFilename;

use crate::install_location::{InstallLocation, LockedDir};
use crate::wheel::{
    extra_dist_info, install_data, parse_wheel_version, read_scripts_from_section,
    write_script_entrypoints,
};
use crate::{read_record_file, Error, Script};

/// Install the given wheel to the given venv
///
/// The caller must ensure that the wheel is compatible to the environment.
///
/// <https://packaging.python.org/en/latest/specifications/binary-distribution-format/#installing-a-wheel-distribution-1-0-py32-none-any-whl>
///
/// Wheel 1.0: <https://www.python.org/dev/peps/pep-0427/>
pub fn install_wheel(
    location: &InstallLocation<LockedDir>,
    wheel: &Path,
    filename: &WheelFilename,
) -> Result<String, Error> {
    let name = &filename.distribution;
    let _my_span = span!(Level::DEBUG, "install_wheel", name = name.as_str());

    let base_location = location.venv_base();

    // TODO(charlie): Pass this in.
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

    debug!(name = name.as_str(), "Getting wheel metadata");
    let dist_info_prefix = find_dist_info(wheel)?;
    let (name, _version) = read_metadata(&dist_info_prefix, wheel)?;
    // TODO: Check that name and version match

    // We're going step by step though
    // https://packaging.python.org/en/latest/specifications/binary-distribution-format/#installing-a-wheel-distribution-1-0-py32-none-any-whl
    // > 1.a Parse distribution-1.0.dist-info/WHEEL.
    // > 1.b Check that installer is compatible with Wheel-Version. Warn if minor version is greater, abort if major version is greater.
    let wheel_file_path = wheel.join(format!("{dist_info_prefix}.dist-info/WHEEL"));
    let wheel_text = std::fs::read_to_string(&wheel_file_path)?;
    parse_wheel_version(&wheel_text)?;

    // > 1.c If Root-Is-Purelib == ‘true’, unpack archive into purelib (site-packages).
    // > 1.d Else unpack archive into platlib (site-packages).
    // We always install in the same virtualenv site packages
    debug!(name = name.as_str(), "Extracting file");
    let num_unpacked = unpack_wheel_files(&site_packages, wheel)?;
    debug!(name = name.as_str(), "Extracted {num_unpacked} files");

    // Read the RECORD file.
    let mut record_file = File::open(&wheel.join(format!("{dist_info_prefix}.dist-info/RECORD")))?;
    let mut record = read_record_file(&mut record_file)?;

    debug!(name = name.as_str(), "Writing entrypoints");
    let (console_scripts, gui_scripts) = parse_scripts(wheel, &dist_info_prefix, None)?;
    write_script_entrypoints(&site_packages, location, &console_scripts, &mut record)?;
    write_script_entrypoints(&site_packages, location, &gui_scripts, &mut record)?;

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

    debug!(name = name.as_str(), "Writing extra metadata");

    extra_dist_info(&site_packages, &dist_info_prefix, true, &mut record)?;

    debug!(name = name.as_str(), "Writing record");
    let mut record_writer = csv::WriterBuilder::new()
        .has_headers(false)
        .escape(b'"')
        .from_path(site_packages.join(format!("{dist_info_prefix}.dist-info/RECORD")))?;
    record.sort();
    for entry in record {
        record_writer.serialize(entry)?;
    }

    Ok(filename.get_tag())
}

/// The metadata name may be uppercase, while the wheel and dist info names are lowercase, or
/// the metadata name and the dist info name are lowercase, while the wheel name is uppercase.
/// Either way, we just search the wheel for the name
///
/// <https://github.com/PyO3/python-pkginfo-rs>
fn find_dist_info(path: &Path) -> Result<String, Error> {
    // Iterate over `path` to find the `.dist-info` directory. It should be at the top-level.
    let Some(dist_info) = std::fs::read_dir(path)?.find_map(|entry| {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.is_dir() {
            if path.extension().map_or(false, |ext| ext == "dist-info") {
                Some(path)
            } else {
                None
            }
        } else {
            None
        }
    }) else {
        return Err(Error::InvalidWheel(
            "Missing .dist-info directory".to_string(),
        ));
    };

    let Some(dist_info_prefix) = dist_info.file_stem() else {
        return Err(Error::InvalidWheel(
            "Missing .dist-info directory".to_string(),
        ));
    };

    Ok(dist_info_prefix.to_string_lossy().to_string())
}

/// <https://github.com/PyO3/python-pkginfo-rs>
fn read_metadata(dist_info_prefix: &str, wheel: &Path) -> Result<(String, String), Error> {
    let metadata_file = wheel.join(format!("{dist_info_prefix}.dist-info/METADATA"));

    // Read into a buffer.
    let mut content = Vec::new();
    File::open(&metadata_file)?.read_to_end(&mut content)?;

    // HACK: trick mailparse to parse as UTF-8 instead of ASCII
    let mut mail = b"Content-Type: text/plain; charset=utf-8\n".to_vec();
    mail.extend_from_slice(&content);
    let msg = mailparse::parse_mail(&mail).map_err(|err| {
        Error::InvalidWheel(format!("Invalid {}: {}", metadata_file.display(), err))
    })?;
    let headers = msg.get_headers();
    let metadata_version =
        headers
            .get_first_value("Metadata-Version")
            .ok_or(Error::InvalidWheel(format!(
                "No Metadata-Version field in {}",
                metadata_file.display()
            )))?;
    // Crude but it should do https://packaging.python.org/en/latest/specifications/core-metadata/#metadata-version
    // At time of writing:
    // > Version of the file format; legal values are “1.0”, “1.1”, “1.2”, “2.1”, “2.2”, and “2.3”.
    if !(metadata_version.starts_with("1.") || metadata_version.starts_with("2.")) {
        return Err(Error::InvalidWheel(format!(
            "Metadata-Version field has unsupported value {metadata_version}"
        )));
    }
    let name = headers
        .get_first_value("Name")
        .ok_or(Error::InvalidWheel(format!(
            "No Name field in {}",
            metadata_file.display()
        )))?;
    let version = headers
        .get_first_value("Version")
        .ok_or(Error::InvalidWheel(format!(
            "No Version field in {}",
            metadata_file.display()
        )))?;
    Ok((name, version))
}

/// Parses the `entry_points.txt` entry in the wheel for console scripts
///
/// Returns (`script_name`, module, function)
///
/// Extras are supposed to be ignored, which happens if you pass None for extras
fn parse_scripts(
    wheel: &Path,
    dist_info_prefix: &str,
    extras: Option<&[String]>,
) -> Result<(Vec<Script>, Vec<Script>), Error> {
    let entry_points_path = wheel.join(format!("{dist_info_prefix}.dist-info/entry_points.txt"));

    // Read the entry points mapping. If the file doesn't exist, we just return an empty mapping.
    let Ok(ini) = std::fs::read_to_string(entry_points_path) else {
        return Ok((Vec::new(), Vec::new()));
    };

    let entry_points_mapping = Ini::new_cs()
        .read(ini)
        .map_err(|err| Error::InvalidWheel(format!("entry_points.txt is invalid: {err}")))?;

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

/// Extract all files from the wheel into the site packages.
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn unpack_wheel_files(site_packages: &Path, wheel: &Path) -> Result<usize, Error> {
    use crate::reflink::reflink;

    let mut count = 0usize;

    // On macOS, directly can be recursively copied with a single `clonefile` call.
    // So we only need to iterate over the top-level of the directory, and copy each file or
    // subdirectory.
    for entry in std::fs::read_dir(wheel)? {
        let entry = entry?;
        let from = entry.path();
        let to = site_packages.join(from.strip_prefix(wheel).unwrap());

        // Delete the destination if it already exists.
        if let Ok(metadata) = to.metadata() {
            if metadata.is_dir() {
                fs::remove_dir_all(&to)?;
            } else if metadata.is_file() {
                fs::remove_file(&to)?;
            }
        }

        // Copy the file.
        reflink(&from, &to)?;

        count += 1;
    }

    Ok(count)
}

/// Extract all files from the wheel into the site packages
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn unpack_wheel_files(site_packages: &Path, wheel: &Path) -> Result<usize, Error> {
    let mut count = 0usize;

    // Walk over the directory.
    for entry in walkdir::WalkDir::new(wheel) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(wheel).unwrap();
        let out_path = site_packages.join(relative);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        reflink_copy::reflink_or_copy(entry.path(), &out_path)?;

        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;

            if let Ok(metadata) = entry.metadata() {
                fs::set_permissions(
                    &out_path,
                    Permissions::from_mode(metadata.permissions().mode()),
                )?;
            }
        }

        count += 1;
    }

    Ok(count)
}
