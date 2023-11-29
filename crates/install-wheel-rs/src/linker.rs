//! Like `wheel.rs`, but for installing wheels that have already been unzipped, rather than
//! reading from a zip file.

use std::path::Path;

use configparser::ini::Ini;
use fs_err as fs;
use fs_err::File;
use mailparse::MailHeaderMap;
use tracing::{debug, span, Level};

use pypi_types::DirectUrl;

use crate::install_location::InstallLocation;
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
    location: &InstallLocation<impl AsRef<Path>>,
    wheel: impl AsRef<Path>,
    direct_url: Option<&DirectUrl>,
    link_mode: LinkMode,
) -> Result<(), Error> {
    let root = location.venv_root();

    // TODO(charlie): Pass this in.
    let site_packages_python = format!(
        "python{}.{}",
        location.python_version().0,
        location.python_version().1
    );
    let site_packages = if cfg!(target_os = "windows") {
        root.as_ref().join("Lib").join("site-packages")
    } else {
        root.as_ref()
            .join("lib")
            .join(site_packages_python)
            .join("site-packages")
    };

    let dist_info_prefix = find_dist_info(&wheel)?;
    let (name, _version) = read_metadata(&dist_info_prefix, &wheel)?;

    let _my_span = span!(Level::DEBUG, "install_wheel", name);

    // We're going step by step though
    // https://packaging.python.org/en/latest/specifications/binary-distribution-format/#installing-a-wheel-distribution-1-0-py32-none-any-whl
    // > 1.a Parse distribution-1.0.dist-info/WHEEL.
    // > 1.b Check that installer is compatible with Wheel-Version. Warn if minor version is greater, abort if major version is greater.
    let wheel_file_path = wheel
        .as_ref()
        .join(format!("{dist_info_prefix}.dist-info/WHEEL"));
    let wheel_text = fs::read_to_string(wheel_file_path)?;
    parse_wheel_version(&wheel_text)?;

    // > 1.c If Root-Is-Purelib == ‘true’, unpack archive into purelib (site-packages).
    // > 1.d Else unpack archive into platlib (site-packages).
    // We always install in the same virtualenv site packages
    debug!(name, "Extracting file");
    let num_unpacked = link_mode.link_wheel_files(&site_packages, &wheel)?;
    debug!(name, "Extracted {num_unpacked} files");

    // Read the RECORD file.
    let mut record_file = File::open(
        wheel
            .as_ref()
            .join(format!("{dist_info_prefix}.dist-info/RECORD")),
    )?;
    let mut record = read_record_file(&mut record_file)?;

    debug!(name, "Writing entrypoints");
    let (console_scripts, gui_scripts) = parse_scripts(&wheel, &dist_info_prefix, None)?;
    write_script_entrypoints(&site_packages, location, &console_scripts, &mut record)?;
    write_script_entrypoints(&site_packages, location, &gui_scripts, &mut record)?;

    let data_dir = site_packages.join(format!("{dist_info_prefix}.data"));
    // 2.a Unpacked archive includes distribution-1.0.dist-info/ and (if there is data) distribution-1.0.data/.
    // 2.b Move each subtree of distribution-1.0.data/ onto its destination path. Each subdirectory of distribution-1.0.data/ is a key into a dict of destination directories, such as distribution-1.0.data/(purelib|platlib|headers|scripts|data). The initially supported paths are taken from distutils.command.install.
    if data_dir.is_dir() {
        debug!(name, "Installing data");
        install_data(
            root.as_ref(),
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
        debug!(name, "No data");
    }

    debug!(name, "Writing extra metadata");
    extra_dist_info(
        &site_packages,
        &dist_info_prefix,
        true,
        direct_url,
        &mut record,
    )?;

    debug!(name, "Writing record");
    let mut record_writer = csv::WriterBuilder::new()
        .has_headers(false)
        .escape(b'"')
        .from_path(site_packages.join(format!("{dist_info_prefix}.dist-info/RECORD")))?;
    record.sort();
    for entry in record {
        record_writer.serialize(entry)?;
    }

    Ok(())
}

/// The metadata name may be uppercase, while the wheel and dist info names are lowercase, or
/// the metadata name and the dist info name are lowercase, while the wheel name is uppercase.
/// Either way, we just search the wheel for the name
///
/// <https://github.com/PyO3/python-pkginfo-rs>
fn find_dist_info(path: impl AsRef<Path>) -> Result<String, Error> {
    // Iterate over `path` to find the `.dist-info` directory. It should be at the top-level.
    let Some(dist_info) = fs::read_dir(path.as_ref())?.find_map(|entry| {
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
fn read_metadata(
    dist_info_prefix: &str,
    wheel: impl AsRef<Path>,
) -> Result<(String, String), Error> {
    let metadata_file = wheel
        .as_ref()
        .join(format!("{dist_info_prefix}.dist-info/METADATA"));

    let content = fs::read(&metadata_file)?;

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
    wheel: impl AsRef<Path>,
    dist_info_prefix: &str,
    extras: Option<&[String]>,
) -> Result<(Vec<Script>, Vec<Script>), Error> {
    let entry_points_path = wheel
        .as_ref()
        .join(format!("{dist_info_prefix}.dist-info/entry_points.txt"));

    // Read the entry points mapping. If the file doesn't exist, we just return an empty mapping.
    let Ok(ini) = fs::read_to_string(entry_points_path) else {
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

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum LinkMode {
    /// Clone (i.e., copy-on-write) packages from the wheel into the site packages.
    Clone,
    /// Copy packages from the wheel into the site packages.
    Copy,
    /// Hard link packages from the wheel into the site packages.
    Hardlink,
}

impl Default for LinkMode {
    fn default() -> Self {
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            Self::Clone
        } else {
            Self::Hardlink
        }
    }
}

impl LinkMode {
    /// Extract a wheel by linking all of its files into site packages.
    pub fn link_wheel_files(
        self,
        site_packages: impl AsRef<Path>,
        wheel: impl AsRef<Path>,
    ) -> Result<usize, Error> {
        match self {
            Self::Clone => clone_wheel_files(site_packages, wheel),
            Self::Copy => copy_wheel_files(site_packages, wheel),
            Self::Hardlink => hardlink_wheel_files(site_packages, wheel),
        }
    }
}

/// Extract a wheel by cloning all of its files into site packages. The files will be cloned
/// via copy-on-write, which is similar to a hard link, but allows the files to be modified
/// independently (that is, the file is copied upon modification).
///
/// This method uses `clonefile` on macOS, and `reflink` on Linux.
fn clone_wheel_files(
    site_packages: impl AsRef<Path>,
    wheel: impl AsRef<Path>,
) -> Result<usize, Error> {
    let mut count = 0usize;

    // On macOS, directly can be recursively copied with a single `clonefile` call.
    // So we only need to iterate over the top-level of the directory, and copy each file or
    // subdirectory.
    for entry in fs::read_dir(wheel.as_ref())? {
        let entry = entry?;
        let from = entry.path();
        let to = site_packages
            .as_ref()
            .join(from.strip_prefix(&wheel).unwrap());

        // Delete the destination if it already exists.
        fs::remove_dir_all(&to)
            .or_else(|_| fs::remove_file(&to))
            .ok();

        // Copy the file.
        reflink_copy::reflink(&from, &to).map_err(|err| Error::Reflink { from, to, err })?;

        count += 1;
    }

    Ok(count)
}

/// Extract a wheel by copying all of its files into site packages.
fn copy_wheel_files(
    site_packages: impl AsRef<Path>,
    wheel: impl AsRef<Path>,
) -> Result<usize, Error> {
    let mut count = 0usize;

    // Walk over the directory.
    for entry in walkdir::WalkDir::new(&wheel) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(&wheel).unwrap();
        let out_path = site_packages.as_ref().join(relative);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        // Copy the file.
        fs::copy(entry.path(), &out_path)?;

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

/// Extract a wheel by hard-linking all of its files into site packages.
fn hardlink_wheel_files(
    site_packages: impl AsRef<Path>,
    wheel: impl AsRef<Path>,
) -> Result<usize, Error> {
    // Hard linking might not be supported but we (afaik) can't detect this ahead of time, so we'll
    // try hard linking the first file, if this succeeds we'll know later hard linking errors are
    // not due to lack of os/fs support, if it fails we'll switch to copying for the rest of the
    // install
    #[derive(Debug, Default, Clone, Copy)]
    enum Attempt {
        #[default]
        Initial,
        Subsequent,
        UseCopyFallback,
    }

    let mut attempt = Attempt::default();
    let mut count = 0usize;

    // Walk over the directory.
    for entry in walkdir::WalkDir::new(&wheel) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(&wheel).unwrap();
        let out_path = site_packages.as_ref().join(relative);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        // The `RECORD` file is modified during installation, so we copy it instead of hard-linking.
        if entry.path().ends_with("RECORD") {
            fs::copy(entry.path(), &out_path)?;
            count += 1;
            continue;
        }

        // Fallback to copying if hardlinks aren't supported for this installation.
        match attempt {
            Attempt::Initial => {
                // Once https://github.com/rust-lang/rust/issues/86442 is stable, use that.
                attempt = Attempt::Subsequent;
                if let Err(err) = fs::hard_link(entry.path(), &out_path) {
                    // If the file already exists, remove it and try again.
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        fs::remove_file(&out_path)?;
                        if fs::hard_link(entry.path(), &out_path).is_err() {
                            fs::copy(entry.path(), &out_path)?;
                            attempt = Attempt::UseCopyFallback;
                        }
                    } else {
                        fs::copy(entry.path(), &out_path)?;
                        attempt = Attempt::UseCopyFallback;
                    }
                }
            }
            Attempt::Subsequent => {
                if let Err(err) = fs::hard_link(entry.path(), &out_path) {
                    // If the file already exists, remove it and try again.
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        fs::remove_file(&out_path)?;
                        fs::hard_link(entry.path(), &out_path)?;
                    } else {
                        return Err(err.into());
                    }
                }
            }
            Attempt::UseCopyFallback => {
                fs::copy(entry.path(), &out_path)?;
            }
        }

        count += 1;
    }

    Ok(count)
}
