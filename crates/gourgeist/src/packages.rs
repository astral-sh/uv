use std::io;
use std::io::BufWriter;
use std::str::FromStr;

use camino::{Utf8Path, Utf8PathBuf};
use fs_err as fs;
use fs_err::File;
#[cfg(feature = "parallel")]
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tempfile::NamedTempFile;
use tracing::debug;

use distribution_filename::WheelFilename;
use install_wheel_rs::{install_wheel, InstallLocation};
use puffin_interpreter::InterpreterInfo;

use crate::bare::VenvPaths;
use crate::{crate_cache_dir, Error};

pub(crate) fn download_wheel_cached(filename: &str, url: &str) -> Result<Utf8PathBuf, Error> {
    let wheels_cache = crate_cache_dir()?.join("wheels");
    let cached_wheel = wheels_cache.join(filename);
    if cached_wheel.is_file() {
        debug!("Using cached wheel at {cached_wheel}");
        return Ok(cached_wheel);
    }

    debug!("Downloading wheel from {url} to {cached_wheel}");
    fs::create_dir_all(&wheels_cache)?;
    let mut tempfile = NamedTempFile::new_in(wheels_cache)?;
    let tempfile_path: Utf8PathBuf = tempfile
        .path()
        .to_path_buf()
        .try_into()
        .map_err(camino::FromPathBufError::into_io_error)?;
    let mut response = reqwest::blocking::get(url)?;
    io::copy(&mut response, &mut BufWriter::new(&mut tempfile)).map_err(|err| {
        Error::WheelDownload {
            url: url.to_string(),
            path: tempfile_path.to_path_buf(),
            err,
        }
    })?;
    tempfile.persist(&cached_wheel)?;
    Ok(cached_wheel)
}

/// Install pip, setuptools and wheel from cache pypi with atm fixed wheels
pub(crate) fn install_base_packages(
    location: &Utf8Path,
    info: &InterpreterInfo,
    paths: &VenvPaths,
) -> Result<(), Error> {
    let install_location = InstallLocation::new(location.canonicalize()?, info.simple_version());
    let install_location = install_location.acquire_lock()?;

    // TODO(konstin): Use the json api instead
    // TODO(konstin): Only check the json API so often (monthly? daily?)
    let packages = [
        ("pip-23.2.1-py3-none-any.whl", "https://files.pythonhosted.org/packages/50/c2/e06851e8cc28dcad7c155f4753da8833ac06a5c704c109313b8d5a62968a/pip-23.2.1-py3-none-any.whl"),
        ("setuptools-68.2.2-py3-none-any.whl", "https://files.pythonhosted.org/packages/bb/26/7945080113158354380a12ce26873dd6c1ebd88d47f5bc24e2c5bb38c16a/setuptools-68.2.2-py3-none-any.whl"),
        ("wheel-0.41.2-py3-none-any.whl", "https://files.pythonhosted.org/packages/b8/8b/31273bf66016be6ad22bb7345c37ff350276cfd46e389a0c2ac5da9d9073/wheel-0.41.2-py3-none-any.whl"),
    ];
    #[cfg(feature = "rayon")]
    let iterator = packages.into_par_iter();
    #[cfg(not(feature = "rayon"))]
    let iterator = packages.into_iter();
    iterator
        .map(|(filename, url)| {
            let wheel_file = download_wheel_cached(filename, url)?;
            let parsed_filename = WheelFilename::from_str(filename).unwrap();
            install_wheel(
                &install_location,
                File::open(wheel_file)?,
                &parsed_filename,
                None,
                false,
                false,
                &[],
                paths.interpreter.as_std_path(),
            )
            .map_err(|err| Error::InstallWheel {
                package: filename.to_string(),
                err,
            })?;
            Ok(())
        })
        .collect::<Result<Vec<()>, Error>>()?;
    Ok(())
}
