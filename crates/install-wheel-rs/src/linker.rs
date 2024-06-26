//! Like `wheel.rs`, but for installing wheels that have already been unzipped, rather than
//! reading from a zip file.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use fs_err as fs;
use fs_err::{DirEntry, File};
use reflink_copy as reflink;
use serde::{Deserialize, Serialize};
use tempfile::tempdir_in;
use tracing::{debug, instrument};

use distribution_filename::WheelFilename;
use pep440_rs::Version;
use pypi_types::DirectUrl;
use uv_normalize::PackageName;

use crate::script::{scripts_from_ini, Script};
use crate::wheel::{
    extra_dist_info, install_data, parse_metadata, parse_wheel_file, read_record_file,
    write_script_entrypoints, LibKind,
};
use crate::{Error, Layout};

/// Install the given wheel to the given venv
///
/// The caller must ensure that the wheel is compatible to the environment.
///
/// <https://packaging.python.org/en/latest/specifications/binary-distribution-format/#installing-a-wheel-distribution-1-0-py32-none-any-whl>
///
/// Wheel 1.0: <https://www.python.org/dev/peps/pep-0427/>
#[instrument(skip_all, fields(wheel = %filename))]
pub fn install_wheel(
    layout: &Layout,
    wheel: impl AsRef<Path>,
    filename: &WheelFilename,
    direct_url: Option<&DirectUrl>,
    installer: Option<&str>,
    link_mode: LinkMode,
) -> Result<(), Error> {
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
        if version != filename.version && version != filename.version.clone().without_local() {
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
    let lib_kind = parse_wheel_file(&wheel_text)?;

    // > 1.c If Root-Is-Purelib == ‘true’, unpack archive into purelib (site-packages).
    // > 1.d Else unpack archive into platlib (site-packages).
    debug!(name, "Extracting file");
    let site_packages = match lib_kind {
        LibKind::Pure => &layout.scheme.purelib,
        LibKind::Plat => &layout.scheme.platlib,
    };
    let num_unpacked = link_mode.link_wheel_files(site_packages, &wheel)?;
    debug!(name, "Extracted {num_unpacked} files");

    // Read the RECORD file.
    let mut record_file = File::open(
        wheel
            .as_ref()
            .join(format!("{dist_info_prefix}.dist-info/RECORD")),
    )?;
    let mut record = read_record_file(&mut record_file)?;

    let (console_scripts, gui_scripts) =
        parse_scripts(&wheel, &dist_info_prefix, None, layout.python_version.1)?;

    if console_scripts.is_empty() && gui_scripts.is_empty() {
        debug!(name, "No entrypoints");
    } else {
        debug!(name, "Writing entrypoints");

        fs_err::create_dir_all(&layout.scheme.scripts)?;
        write_script_entrypoints(layout, site_packages, &console_scripts, &mut record, false)?;
        write_script_entrypoints(layout, site_packages, &gui_scripts, &mut record, true)?;
    }

    // 2.a Unpacked archive includes distribution-1.0.dist-info/ and (if there is data) distribution-1.0.data/.
    // 2.b Move each subtree of distribution-1.0.data/ onto its destination path. Each subdirectory of distribution-1.0.data/ is a key into a dict of destination directories, such as distribution-1.0.data/(purelib|platlib|headers|scripts|data). The initially supported paths are taken from distutils.command.install.
    let data_dir = site_packages.join(format!("{dist_info_prefix}.data"));
    if data_dir.is_dir() {
        debug!(name, "Installing data");
        install_data(
            layout,
            site_packages,
            &data_dir,
            &name,
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
        site_packages,
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

/// Determine the absolute path to an entrypoint script.
pub fn entrypoint_path(entrypoint: &Script, layout: &Layout) -> PathBuf {
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

/// Find the `dist-info` directory in an unzipped wheel.
///
/// See: <https://github.com/PyO3/python-pkginfo-rs>
///
/// See: <https://github.com/pypa/pip/blob/36823099a9cdd83261fdbc8c1d2a24fa2eea72ca/src/pip/_internal/utils/wheel.py#L38>
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
/// Extras are supposed to be ignored, which happens if you pass None for extras.
fn parse_scripts(
    wheel: impl AsRef<Path>,
    dist_info_prefix: &str,
    extras: Option<&[String]>,
    python_minor: u8,
) -> Result<(Vec<Script>, Vec<Script>), Error> {
    let entry_points_path = wheel
        .as_ref()
        .join(format!("{dist_info_prefix}.dist-info/entry_points.txt"));

    // Read the entry points mapping. If the file doesn't exist, we just return an empty mapping.
    let Ok(ini) = fs::read_to_string(entry_points_path) else {
        return Ok((Vec::new(), Vec::new()));
    };

    scripts_from_ini(extras, python_minor, ini)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
    let mut attempt = Attempt::default();

    // On macOS, directly can be recursively copied with a single `clonefile` call.
    // So we only need to iterate over the top-level of the directory, and copy each file or
    // subdirectory unless the subdirectory exists already in which case we'll need to recursively
    // merge its contents with the existing directory.
    for entry in fs::read_dir(wheel.as_ref())? {
        clone_recursive(
            site_packages.as_ref(),
            wheel.as_ref(),
            &entry?,
            &mut attempt,
        )?;
        count += 1;
    }

    // The directory mtime is not updated when cloning and the mtime is used by CPython's
    // import mechanisms to determine if it should look for new packages in a directory.
    // Here, we force the mtime to be updated to ensure that packages are importable without
    // manual cache invalidation.
    //
    // <https://github.com/python/cpython/blob/8336cb2b6f428246803b02a4e97fce49d0bb1e09/Lib/importlib/_bootstrap_external.py#L1601>
    let now = SystemTime::now();

    // `File.set_modified` is not available in `fs_err` yet
    #[allow(clippy::disallowed_types)]
    match std::fs::File::open(site_packages.as_ref()) {
        Ok(dir) => {
            if let Err(err) = dir.set_modified(now) {
                debug!(
                    "Failed to update mtime for {}: {err}",
                    site_packages.as_ref().display()
                );
            }
        }
        Err(err) => debug!(
            "Failed to open {} to update mtime: {err}",
            site_packages.as_ref().display()
        ),
    }

    Ok(count)
}

// Hard linking / reflinking might not be supported but we (afaik) can't detect this ahead of time,
// so we'll try hard linking / reflinking the first file - if this succeeds we'll know later
// errors are not due to lack of os/fs support. If it fails, we'll switch to copying for the rest of the
// install.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Attempt {
    #[default]
    Initial,
    Subsequent,
    UseCopyFallback,
}

/// Recursively clone the contents of `from` into `to`.
fn clone_recursive(
    site_packages: &Path,
    wheel: &Path,
    entry: &DirEntry,
    attempt: &mut Attempt,
) -> Result<(), Error> {
    // Determine the existing and destination paths.
    let from = entry.path();
    let to = site_packages.join(from.strip_prefix(wheel).unwrap());

    debug!("Cloning {} to {}", from.display(), to.display());

    if cfg!(windows) && from.is_dir() {
        // On Windows, reflinking directories is not supported, so we copy each file instead.
        fs::create_dir_all(&to)?;
        for entry in fs::read_dir(from)? {
            clone_recursive(site_packages, wheel, &entry?, attempt)?;
        }
        return Ok(());
    }

    match attempt {
        Attempt::Initial => {
            if let Err(err) = reflink::reflink(&from, &to) {
                if matches!(err.kind(), std::io::ErrorKind::AlreadyExists) {
                    // If cloning/copying fails and the directory exists already, it must be merged recursively.
                    if entry.file_type()?.is_dir() {
                        for entry in fs::read_dir(from)? {
                            clone_recursive(site_packages, wheel, &entry?, attempt)?;
                        }
                    } else {
                        // If file already exists, overwrite it.
                        let tempdir = tempdir_in(site_packages)?;
                        let tempfile = tempdir.path().join(from.file_name().unwrap());
                        if reflink::reflink(&from, &tempfile).is_ok() {
                            fs::rename(&tempfile, to)?;
                        } else {
                            debug!(
                                "Failed to clone `{}` to temporary location `{}`, attempting to copy files as a fallback",
                                from.display(),
                                tempfile.display());
                            *attempt = Attempt::UseCopyFallback;
                            fs::copy(&from, &to)?;
                        }
                    }
                } else {
                    debug!(
                        "Failed to clone `{}` to `{}`, attempting to copy files as a fallback",
                        from.display(),
                        to.display()
                    );
                    // switch to copy fallback
                    *attempt = Attempt::UseCopyFallback;
                    clone_recursive(site_packages, wheel, entry, attempt)?;
                }
            }
        }
        Attempt::Subsequent => {
            if let Err(err) = reflink::reflink(&from, &to) {
                if matches!(err.kind(), std::io::ErrorKind::AlreadyExists) {
                    // If cloning/copying fails and the directory exists already, it must be merged recursively.
                    if entry.file_type()?.is_dir() {
                        for entry in fs::read_dir(from)? {
                            clone_recursive(site_packages, wheel, &entry?, attempt)?;
                        }
                    } else {
                        // If file already exists, overwrite it.
                        let tempdir = tempdir_in(site_packages)?;
                        let tempfile = tempdir.path().join(from.file_name().unwrap());
                        reflink::reflink(&from, &tempfile)?;
                        fs::rename(&tempfile, to)?;
                    }
                } else {
                    return Err(Error::Reflink { from, to, err });
                }
            }
        }
        Attempt::UseCopyFallback => {
            if entry.file_type()?.is_dir() {
                fs::create_dir_all(&to)?;
                for entry in fs::read_dir(from)? {
                    clone_recursive(site_packages, wheel, &entry?, attempt)?;
                }
            } else {
                fs::copy(&from, &to)?;
            }
        }
    }

    if *attempt == Attempt::Initial {
        *attempt = Attempt::Subsequent;
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
                            debug!(
                                "Failed to hardlink `{}` to `{}`, attempting to copy files as a fallback",
                                out_path.display(),
                                path.display()
                            );
                            fs::copy(path, &out_path)?;
                            attempt = Attempt::UseCopyFallback;
                        }
                    } else {
                        debug!(
                            "Failed to hardlink `{}` to `{}`, attempting to copy files as a fallback",
                            out_path.display(),
                            path.display()
                        );
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
