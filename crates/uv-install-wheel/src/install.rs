//! Like `wheel.rs`, but for installing wheels that have already been unzipped, rather than
//! reading from a zip file.

use std::path::Path;
use std::str::FromStr;

use fs_err as fs;
use fs_err::File;
use tracing::{instrument, trace};

use uv_cache_info::CacheInfo;
use uv_distribution_filename::WheelFilename;
use uv_pep440::Version;
use uv_pypi_types::{DirectUrl, Metadata10};

use crate::linker::{LinkMode, Locks};
use crate::wheel::{
    dist_info_metadata, find_dist_info, install_data, parse_scripts, parse_wheel_file,
    read_record_file, write_installer_metadata, write_script_entrypoints, LibKind,
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
    relocatable: bool,
    wheel: impl AsRef<Path>,
    filename: &WheelFilename,
    direct_url: Option<&DirectUrl>,
    cache_info: Option<&CacheInfo>,
    installer: Option<&str>,
    installer_metadata: bool,
    link_mode: LinkMode,
    locks: &Locks,
) -> Result<(), Error> {
    let dist_info_prefix = find_dist_info(&wheel)?;
    let metadata = dist_info_metadata(&dist_info_prefix, &wheel)?;
    let Metadata10 { name, version } = Metadata10::parse_pkg_info(&metadata)
        .map_err(|err| Error::InvalidWheel(err.to_string()))?;

    let version = Version::from_str(&version)?;

    // Validate the wheel name and version.
    {
        if name != filename.name {
            return Err(Error::MismatchedName(name, filename.name.clone()));
        }

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
    trace!(?name, "Extracting file");
    let site_packages = match lib_kind {
        LibKind::Pure => &layout.scheme.purelib,
        LibKind::Plat => &layout.scheme.platlib,
    };
    let num_unpacked = link_mode.link_wheel_files(site_packages, &wheel, locks)?;
    trace!(?name, "Extracted {num_unpacked} files");

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
        trace!(?name, "No entrypoints");
    } else {
        trace!(?name, "Writing entrypoints");

        fs_err::create_dir_all(&layout.scheme.scripts)?;
        write_script_entrypoints(
            layout,
            relocatable,
            site_packages,
            &console_scripts,
            &mut record,
            false,
        )?;
        write_script_entrypoints(
            layout,
            relocatable,
            site_packages,
            &gui_scripts,
            &mut record,
            true,
        )?;
    }

    // 2.a Unpacked archive includes distribution-1.0.dist-info/ and (if there is data) distribution-1.0.data/.
    // 2.b Move each subtree of distribution-1.0.data/ onto its destination path. Each subdirectory of distribution-1.0.data/ is a key into a dict of destination directories, such as distribution-1.0.data/(purelib|platlib|headers|scripts|data). The initially supported paths are taken from distutils.command.install.
    let data_dir = site_packages.join(format!("{dist_info_prefix}.data"));
    if data_dir.is_dir() {
        install_data(
            layout,
            relocatable,
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
        trace!(?name, "No data");
    }

    if installer_metadata {
        trace!(?name, "Writing installer metadata");
        write_installer_metadata(
            site_packages,
            &dist_info_prefix,
            true,
            direct_url,
            cache_info,
            installer,
            &mut record,
        )?;
    }

    trace!(?name, "Writing record");
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
