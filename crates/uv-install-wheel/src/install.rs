//! Like `wheel.rs`, but for installing wheels that have already been unzipped, rather than
//! reading from a zip file.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use fs_err::File;
use itertools::{Itertools, iproduct};
use tracing::{instrument, trace};

use uv_distribution_filename::{ExpandedTags, WheelFilename};
use uv_pep440::Version;
use uv_pypi_types::{DirectUrl, Metadata10};

use crate::linker::{InstallState, LinkMode, link_wheel_files};
use crate::wheel::{
    LibKind, WheelFile, dist_info_metadata, find_dist_info, install_data, parse_scripts,
    read_record, write_installer_metadata, write_record, write_script_entrypoints,
};
use crate::{Error, Layout};

/// Return the path at which the wheel's `.dist-info` directory will be installed.
pub fn installed_dist_info_path(
    layout: &Layout,
    wheel: impl AsRef<Path>,
) -> Result<PathBuf, Error> {
    let (dist_info_prefix, site_packages, _) = wheel_destination(layout, wheel.as_ref())?;
    Ok(site_packages.join(format!("{dist_info_prefix}.dist-info")))
}

/// Return the wheel's `.dist-info` prefix, target `site-packages` directory, and parsed `WHEEL`.
fn wheel_destination<'layout>(
    layout: &'layout Layout,
    wheel: &Path,
) -> Result<(String, &'layout Path, WheelFile), Error> {
    let dist_info_prefix = find_dist_info(wheel)?;
    let wheel_file_path = wheel.join(format!("{dist_info_prefix}.dist-info/WHEEL"));
    let wheel_text = fs_err::read_to_string(wheel_file_path)?;
    let wheel_file = WheelFile::parse(&wheel_text)?;
    let site_packages = match wheel_file.lib_kind() {
        LibKind::Pure => &layout.scheme.purelib,
        LibKind::Plat => &layout.scheme.platlib,
    };
    Ok((dist_info_prefix, site_packages, wheel_file))
}

/// Install the given wheel to the given venv
///
/// The caller must ensure that the wheel is compatible to the environment.
///
/// <https://packaging.python.org/en/latest/specifications/binary-distribution-format/#installing-a-wheel-distribution-1-0-py32-none-any-whl>
///
/// Wheel 1.0: <https://www.python.org/dev/peps/pep-0427/>
#[instrument(skip_all, fields(wheel = %filename))]
pub fn install_wheel<Cache: serde::Serialize, Build: serde::Serialize>(
    layout: &Layout,
    relocatable: bool,
    wheel: impl AsRef<Path>,
    filename: &WheelFilename,
    direct_url: Option<&DirectUrl>,
    cache_info: Option<&Cache>,
    build_info: Option<&Build>,
    installer: Option<&str>,
    installer_metadata: bool,
    link_mode: LinkMode,
    state: &InstallState,
) -> Result<(), Error> {
    let wheel = wheel.as_ref();
    let (dist_info_prefix, site_packages, wheel_file) = wheel_destination(layout, wheel)?;
    let metadata = dist_info_metadata(&dist_info_prefix, wheel)?;
    let Metadata10 { name, version } = Metadata10::parse_pkg_info(&metadata)
        .map_err(|err| Error::InvalidWheel(err.to_string()))?;

    let version = Version::from_str(&version)?;

    // Validate the wheel name and version.
    if !uv_flags::contains(uv_flags::EnvironmentFlags::SKIP_WHEEL_FILENAME_CHECK) {
        if name != filename.name {
            return Err(Error::MismatchedName(name, filename.name.clone()));
        }

        if version != filename.version && version != filename.version.clone().without_local() {
            return Err(Error::MismatchedVersion(version, filename.version.clone()));
        }

        let mut wheel_tags = BTreeSet::new();
        for tag in wheel_file.tags().unwrap_or_default() {
            let expanded = ExpandedTags::parse([tag.as_str()])
                .map_err(|err| Error::InvalidWheel(err.to_string()))?;
            wheel_tags.extend(
                iproduct!(
                    expanded.python_tags(),
                    expanded.abi_tags(),
                    expanded.platform_tags()
                )
                .map(|(python, abi, platform)| format!("{python}-{abi}-{platform}")),
            );
        }
        let filename_tags = iproduct!(
            filename.python_tags(),
            filename.abi_tags(),
            filename.platform_tags()
        )
        .map(|(python, abi, platform)| format!("{python}-{abi}-{platform}"))
        .collect::<BTreeSet<_>>();
        if wheel_tags != filename_tags {
            return Err(Error::InvalidWheel(format!(
                "Wheel tags do not match filename ({} != {})",
                wheel_tags.iter().join(", "),
                filename_tags.iter().join(", ")
            )));
        }
    }

    // We're going step by step though
    // https://packaging.python.org/en/latest/specifications/binary-distribution-format/#installing-a-wheel-distribution-1-0-py32-none-any-whl
    // > 1.a Parse distribution-1.0.dist-info/WHEEL.
    // > 1.b Check that installer is compatible with Wheel-Version. Warn if minor version is greater, abort if major version is greater.
    // > 1.c If Root-Is-Purelib == ‘true’, unpack archive into purelib (site-packages).
    // > 1.d Else unpack archive into platlib (site-packages).
    trace!(?name, "Extracting wheel files");
    link_wheel_files(link_mode, site_packages, wheel, state, filename)?;
    trace!(?name, "Extracted wheel files");

    // Read the RECORD file.
    let mut record_file = File::open(wheel.join(format!("{dist_info_prefix}.dist-info/RECORD")))?;
    let mut record = read_record(&mut record_file)?;

    let (console_scripts, gui_scripts) =
        parse_scripts(wheel, &dist_info_prefix, None, layout.python_version.1)?;

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
        fs_err::remove_dir_all(data_dir)?;
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
            build_info,
            installer,
            &mut record,
        )?;
    }

    trace!(?name, "Writing record");
    write_record(site_packages, &dist_info_prefix, record)?;

    Ok(())
}
