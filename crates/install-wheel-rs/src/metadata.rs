use std::io::{Read, Seek};
use std::path::Path;
use std::str::FromStr;

use tracing::warn;
use zip::ZipArchive;

use distribution_filename::WheelFilename;
use pep440_rs::Version;
use uv_normalize::PackageName;

use crate::Error;

/// Find the `.dist-info` directory in a zipped wheel.
///
/// Returns the dist info dir prefix without the `.dist-info` extension.
///
/// Reference implementation: <https://github.com/pypa/pip/blob/36823099a9cdd83261fdbc8c1d2a24fa2eea72ca/src/pip/_internal/utils/wheel.py#L38>
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
            let dist_info_prefix = dist_info_dir.strip_suffix(".dist-info")?;
            Some((payload, dist_info_prefix))
        })
        .collect();

    // Like `pip`, assert that there is exactly one `.dist-info` directory.
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

    // Like `pip`, validate that the `.dist-info` directory is prefixed with the canonical
    // package name, but only warn if the version is not the normalized version.
    let Some((name, version)) = dist_info_prefix.rsplit_once('-') else {
        return Err(Error::MissingDistInfoSegments(dist_info_prefix.to_string()));
    };
    if PackageName::from_str(name)? != filename.name {
        return Err(Error::MissingDistInfoPackageName(
            dist_info_prefix.to_string(),
            filename.name.to_string(),
        ));
    }
    if !Version::from_str(version).is_ok_and(|version| version == filename.version) {
        warn!(
            "{}",
            Error::MissingDistInfoVersion(
                dist_info_prefix.to_string(),
                filename.version.to_string(),
            )
        );
    }

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
    let Some(dist_info_prefix) = fs_err::read_dir(path.as_ref())?.find_map(|entry| {
        let entry = entry.ok()?;
        let file_type = entry.file_type().ok()?;
        if file_type.is_dir() {
            let path = entry.path();

            let extension = path.extension()?;
            if extension != "dist-info" {
                return None;
            }

            let dist_info_prefix = path.file_stem()?.to_str()?;
            Some(dist_info_prefix.to_string())
        } else {
            None
        }
    }) else {
        return Err(Error::InvalidWheel(
            "Missing .dist-info directory".to_string(),
        ));
    };

    // Like `pip`, validate that the `.dist-info` directory is prefixed with the canonical
    // package name, but only warn if the version is not the normalized version.
    let Some((name, version)) = dist_info_prefix.rsplit_once('-') else {
        return Err(Error::MissingDistInfoSegments(dist_info_prefix.to_string()));
    };
    if PackageName::from_str(name)? != filename.name {
        return Err(Error::MissingDistInfoPackageName(
            dist_info_prefix.to_string(),
            filename.name.to_string(),
        ));
    }
    if !Version::from_str(version).is_ok_and(|version| version == filename.version) {
        warn!(
            "{}",
            Error::MissingDistInfoVersion(
                dist_info_prefix.to_string(),
                filename.version.to_string(),
            )
        );
    }

    Ok(dist_info_prefix)
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
