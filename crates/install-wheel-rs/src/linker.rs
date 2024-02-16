//! Like `wheel.rs`, but for installing wheels that have already been unzipped, rather than
//! reading from a zip file.

use std::path::Path;
use std::str::FromStr;

use configparser::ini::Ini;
use fs_err as fs;
use fs_err::{DirEntry, File};
use tempfile::tempdir_in;
use tracing::{debug, instrument};

use distribution_filename::WheelFilename;
use pep440_rs::Version;
use pypi_types::DirectUrl;
use uv_normalize::PackageName;

use crate::install_location::InstallLocation;
use crate::wheel::{
    extra_dist_info, install_data, parse_metadata, parse_wheel_version, read_scripts_from_section,
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
#[instrument(skip_all, fields(wheel = % wheel.as_ref().display()))]
pub fn install_wheel(
    location: &InstallLocation<impl AsRef<Path>>,
    wheel: impl AsRef<Path>,
    filename: &WheelFilename,
    direct_url: Option<&DirectUrl>,
    installer: Option<&str>,
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
    let metadata = dist_info_metadata(&dist_info_prefix, &wheel)?;
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
        installer,
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

/// Find the `dist-info` directory in an unzipped wheel.
///
/// See: <https://github.com/PyO3/python-pkginfo-rs>
fn find_dist_info(path: impl AsRef<Path>) -> Result<String, Error> {
    // Iterate over `path` to find the `.dist-info` directory. It should be at the top-level.
    let Some(dist_info) = fs::read_dir(path.as_ref())?.find_map(|entry| {
        let entry = entry.ok()?;
        let file_type = entry.file_type().ok()?;
        if file_type.is_dir() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "dist-info") {
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

/// Read the `dist-info` metadata from a directory.
fn dist_info_metadata(dist_info_prefix: &str, wheel: impl AsRef<Path>) -> Result<Vec<u8>, Error> {
    let metadata_file = wheel
        .as_ref()
        .join(format!("{dist_info_prefix}.dist-info/METADATA"));
    Ok(fs::read(metadata_file)?)
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
    #[instrument(skip_all)]
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
    // subdirectory unless the subdirectory exists already in which case we'll need to recursively
    // merge its contents with the existing directory.
    for entry in fs::read_dir(wheel.as_ref())? {
        clone_recursive(site_packages.as_ref(), wheel.as_ref(), &entry?)?;
        count += 1;
    }

    Ok(count)
}

/// Recursively clone the contents of `from` into `to`.
fn clone_recursive(site_packages: &Path, wheel: &Path, entry: &DirEntry) -> Result<(), Error> {
    // Determine the existing and destination paths.
    let from = entry.path();
    let to = site_packages.join(from.strip_prefix(wheel).unwrap());

    debug!("Cloning {} to {}", from.display(), to.display());

    // Attempt to copy the file or directory
    let reflink = reflink_copy::reflink(&from, &to);

    if reflink
        .as_ref()
        .is_err_and(|err| matches!(err.kind(), std::io::ErrorKind::AlreadyExists))
    {
        // If copying fails and the directory exists already, it must be merged recursively.
        if entry.file_type()?.is_dir() {
            for entry in fs::read_dir(from)? {
                clone_recursive(site_packages, wheel, &entry?)?;
            }
        } else {
            // If file already exists, overwrite it.
            let tempdir = tempdir_in(site_packages)?;
            let tempfile = tempdir.path().join(from.file_name().unwrap());
            reflink_copy::reflink(from, &tempfile)?;
            fs::rename(&tempfile, to)?;
        }
    } else {
        // Other errors should be tracked
        reflink.map_err(|err| Error::Reflink {
            from: from.clone(),
            to: to.clone(),
            err,
        })?;
    }

    Ok(())
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
        let path = entry.path();

        let relative = path.strip_prefix(&wheel).unwrap();
        let out_path = site_packages.as_ref().join(relative);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        // Copy the file, which will also set its permissions.
        fs::copy(path, &out_path)?;

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
        let path = entry.path();

        let relative = path.strip_prefix(&wheel).unwrap();
        let out_path = site_packages.as_ref().join(relative);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        // The `RECORD` file is modified during installation, so we copy it instead of hard-linking.
        if path.ends_with("RECORD") {
            fs::copy(path, &out_path)?;
            count += 1;
            continue;
        }

        // Fallback to copying if hardlinks aren't supported for this installation.
        match attempt {
            Attempt::Initial => {
                // Once https://github.com/rust-lang/rust/issues/86442 is stable, use that.
                attempt = Attempt::Subsequent;
                if let Err(err) = fs::hard_link(path, &out_path) {
                    // If the file already exists, remove it and try again.
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        debug!(
                            "File already exists (initial attempt), overwriting: {}",
                            out_path.display()
                        );
                        // Removing and recreating would lead to race conditions.
                        let tempdir = tempdir_in(&site_packages)?;
                        let tempfile = tempdir.path().join(entry.file_name());
                        if fs::hard_link(path, &tempfile).is_ok() {
                            fs_err::rename(&tempfile, &out_path)?;
                        } else {
                            fs::copy(path, &out_path)?;
                            attempt = Attempt::UseCopyFallback;
                        }
                    } else {
                        fs::copy(path, &out_path)?;
                        attempt = Attempt::UseCopyFallback;
                    }
                }
            }
            Attempt::Subsequent => {
                if let Err(err) = fs::hard_link(path, &out_path) {
                    // If the file already exists, remove it and try again.
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        debug!(
                            "File already exists (subsequent attempt), overwriting: {}",
                            out_path.display()
                        );
                        // Removing and recreating would lead to race conditions.
                        let tempdir = tempdir_in(&site_packages)?;
                        let tempfile = tempdir.path().join(entry.file_name());
                        fs::hard_link(path, &tempfile)?;
                        fs_err::rename(&tempfile, &out_path)?;
                    } else {
                        return Err(err.into());
                    }
                }
            }
            Attempt::UseCopyFallback => {
                fs::copy(path, &out_path)?;
            }
        }

        count += 1;
    }

    Ok(count)
}
