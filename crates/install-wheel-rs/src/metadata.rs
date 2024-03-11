use std::io::{Read, Seek};
use std::path::Path;
use std::str::FromStr;

use zip::ZipArchive;

use distribution_filename::WheelFilename;
use pep440_rs::Version;
use uv_normalize::PackageName;

use crate::Error;

/// Returns `true` if the file is a `METADATA` file in a `.dist-info` directory that matches the
/// wheel filename.
pub fn is_metadata_entry(path: &str, filename: &WheelFilename) -> bool {
    let Some((dist_info_dir, file)) = path.split_once('/') else {
        return false;
    };
    if file != "METADATA" {
        return false;
    }
    let Some(dir_stem) = dist_info_dir.strip_suffix(".dist-info") else {
        return false;
    };
    let Some((name, version)) = dir_stem.rsplit_once('-') else {
        return false;
    };
    let Ok(name) = PackageName::from_str(name) else {
        return false;
    };
    if name != filename.name {
        return false;
    }
    let Ok(version) = Version::from_str(version) else {
        return false;
    };
    if version != filename.version {
        return false;
    }
    true
}

/// Find the `.dist-info` directory in a zipped wheel.
///
/// The metadata name may be uppercase, while the wheel and dist info names are lowercase, or
/// the metadata name and the dist info name are lowercase, while the wheel name is uppercase.
/// Either way, we just search the wheel for the name.
///
/// Returns the dist info dir prefix without the `.dist-info` extension.
///
/// Reference implementation: <https://github.com/pypa/packaging/blob/2f83540272e79e3fe1f5d42abae8df0c14ddf4c2/src/packaging/utils.py#L146-L172>
pub fn find_archive_dist_info<'a, T: Copy>(
    filename: &WheelFilename,
    files: impl Iterator<Item = (T, &'a str)>,
) -> Result<(T, &'a str), Error> {
    let metadatas: Vec<_> = files
        .filter_map(|(payload, path)| {
            let (dist_info_dir, file) = path.split_once('/')?;
            if file != "METADATA" {
                return None;
            }

            let dir_stem = dist_info_dir.strip_suffix(".dist-info")?;
            let (name, version) = dir_stem.rsplit_once('-')?;
            if PackageName::from_str(name).ok()? != filename.name {
                return None;
            }

            if Version::from_str(version).ok()? != filename.version {
                return None;
            }

            Some((payload, dir_stem))
        })
        .collect();
    let (payload, dist_info_prefix) = match metadatas[..] {
        [] => {
            return Err(Error::MissingDistInfo);
        }
        [(payload, path)] => (payload, path),
        _ => {
            return Err(Error::MultipleDistInfo(
                metadatas
                    .into_iter()
                    .map(|(_, dist_info_dir)| dist_info_dir.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
        }
    };
    Ok((payload, dist_info_prefix))
}

/// Given an archive, read the `METADATA` from the `.dist-info` directory.
pub fn read_archive_metadata(
    filename: &WheelFilename,
    archive: &mut ZipArchive<impl Read + Seek + Sized>,
) -> Result<Vec<u8>, Error> {
    let dist_info_prefix =
        find_archive_dist_info(filename, archive.file_names().map(|name| (name, name)))?.1;

    let mut file = archive
        .by_name(&format!("{dist_info_prefix}.dist-info/METADATA"))
        .map_err(|err| Error::Zip(filename.to_string(), err))?;

    #[allow(clippy::cast_possible_truncation)]
    let mut buffer = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut buffer)?;

    Ok(buffer)
}

/// Find the `.dist-info` directory in an unzipped wheel.
///
/// See: <https://github.com/PyO3/python-pkginfo-rs>
pub fn find_flat_dist_info(
    filename: &WheelFilename,
    path: impl AsRef<Path>,
) -> Result<String, Error> {
    // Iterate over `path` to find the `.dist-info` directory. It should be at the top-level.
    let Some(dist_info) = fs_err::read_dir(path.as_ref())?.find_map(|entry| {
        let entry = entry.ok()?;
        let file_type = entry.file_type().ok()?;
        if file_type.is_dir() {
            let path = entry.path();

            let extension = path.extension()?;
            if extension != "dist-info" {
                return None;
            }

            let stem = path.file_stem()?;
            let (name, version) = stem.to_str()?.rsplit_once('-')?;
            if PackageName::from_str(name).ok()? != filename.name {
                return None;
            }
            if Version::from_str(version).ok()? != filename.version {
                return None;
            }

            Some(path)
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

/// Read the wheel `METADATA` metadata from a `.dist-info` directory.
pub fn read_dist_info_metadata(
    dist_info_prefix: &str,
    wheel: impl AsRef<Path>,
) -> Result<Vec<u8>, Error> {
    let metadata_file = wheel
        .as_ref()
        .join(format!("{dist_info_prefix}.dist-info/METADATA"));
    Ok(fs_err::read(metadata_file)?)
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use distribution_filename::WheelFilename;

    use crate::metadata::find_archive_dist_info;

    #[test]
    fn test_dot_in_name() {
        let files = [
            "mastodon/Mastodon.py",
            "mastodon/__init__.py",
            "mastodon/streaming.py",
            "Mastodon.py-1.5.1.dist-info/DESCRIPTION.rst",
            "Mastodon.py-1.5.1.dist-info/metadata.json",
            "Mastodon.py-1.5.1.dist-info/top_level.txt",
            "Mastodon.py-1.5.1.dist-info/WHEEL",
            "Mastodon.py-1.5.1.dist-info/METADATA",
            "Mastodon.py-1.5.1.dist-info/RECORD",
        ];
        let filename = WheelFilename::from_str("Mastodon.py-1.5.1-py2.py3-none-any.whl").unwrap();
        let (_, dist_info_prefix) =
            find_archive_dist_info(&filename, files.into_iter().map(|file| (file, file))).unwrap();
        assert_eq!(dist_info_prefix, "Mastodon.py-1.5.1");
    }
}
