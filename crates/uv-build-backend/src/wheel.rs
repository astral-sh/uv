use fs_err::File;
use globset::{GlobSet, GlobSetBuilder};
use itertools::Itertools;
use sha2::{Digest, Sha256};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{io, mem};
use tracing::{debug, trace};
use walkdir::WalkDir;
use zip::{CompressionMethod, ZipWriter};

use uv_distribution_filename::WheelFilename;
use uv_fs::Simplified;
use uv_globfilter::{parse_portable_glob, GlobDirFilter};
use uv_platform_tags::{AbiTag, LanguageTag, PlatformTag};
use uv_pypi_types::Identifier;
use uv_warnings::warn_user_once;

use crate::metadata::{BuildBackendSettings, DEFAULT_EXCLUDES};
use crate::{DirectoryWriter, Error, FileList, ListWriter, PyProjectToml};

/// Build a wheel from the source tree and place it in the output directory.
pub fn build_wheel(
    source_tree: &Path,
    wheel_dir: &Path,
    metadata_directory: Option<&Path>,
    uv_version: &str,
) -> Result<WheelFilename, Error> {
    let contents = fs_err::read_to_string(source_tree.join("pyproject.toml"))?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    for warning in pyproject_toml.check_build_system(uv_version) {
        warn_user_once!("{warning}");
    }
    crate::check_metadata_directory(source_tree, metadata_directory, &pyproject_toml)?;

    let filename = WheelFilename::new(
        pyproject_toml.name().clone(),
        pyproject_toml.version().clone(),
        LanguageTag::Python {
            major: 3,
            minor: None,
        },
        AbiTag::None,
        PlatformTag::Any,
    );

    let wheel_path = wheel_dir.join(filename.to_string());
    debug!("Writing wheel at {}", wheel_path.user_display());
    let wheel_writer = ZipDirectoryWriter::new_wheel(File::create(&wheel_path)?);

    write_wheel(
        source_tree,
        &pyproject_toml,
        &filename,
        uv_version,
        wheel_writer,
    )?;

    Ok(filename)
}

/// List the files that would be included in a source distribution and their origin.
pub fn list_wheel(
    source_tree: &Path,
    uv_version: &str,
) -> Result<(WheelFilename, FileList), Error> {
    let contents = fs_err::read_to_string(source_tree.join("pyproject.toml"))?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    for warning in pyproject_toml.check_build_system(uv_version) {
        warn_user_once!("{warning}");
    }

    let filename = WheelFilename::new(
        pyproject_toml.name().clone(),
        pyproject_toml.version().clone(),
        LanguageTag::Python {
            major: 3,
            minor: None,
        },
        AbiTag::None,
        PlatformTag::Any,
    );

    let mut files = FileList::new();
    let writer = ListWriter::new(&mut files);
    write_wheel(source_tree, &pyproject_toml, &filename, uv_version, writer)?;
    // Ensure a deterministic order even when file walking changes
    files.sort_unstable();
    Ok((filename, files))
}

fn write_wheel(
    source_tree: &Path,
    pyproject_toml: &PyProjectToml,
    filename: &WheelFilename,
    uv_version: &str,
    mut wheel_writer: impl DirectoryWriter,
) -> Result<(), Error> {
    let settings = pyproject_toml
        .settings()
        .cloned()
        .unwrap_or_else(BuildBackendSettings::default);

    // Wheel excludes
    let mut excludes: Vec<String> = Vec::new();
    if settings.default_excludes {
        excludes.extend(DEFAULT_EXCLUDES.iter().map(ToString::to_string));
    }
    for exclude in settings.wheel_exclude {
        // Avoid duplicate entries.
        if !excludes.contains(&exclude) {
            excludes.push(exclude);
        }
    }
    // The wheel must not include any files excluded by the source distribution (at least until we
    // have files generated in the source dist -> wheel build step).
    for exclude in settings.source_exclude {
        // Avoid duplicate entries.
        if !excludes.contains(&exclude) {
            excludes.push(exclude);
        }
    }
    debug!("Wheel excludes: {:?}", excludes);
    let exclude_matcher = build_exclude_matcher(excludes)?;

    debug!("Adding content files to wheel");
    if settings.module_root.is_absolute() {
        return Err(Error::AbsoluteModuleRoot(settings.module_root.clone()));
    }
    let strip_root = source_tree.join(settings.module_root);

    let module_name = if let Some(module_name) = settings.module_name {
        module_name
    } else {
        // Should never error, the rules for package names (in dist-info formatting) are stricter
        // than those for identifiers
        Identifier::from_str(pyproject_toml.name().as_dist_info_name().as_ref())?
    };
    debug!("Module name: `{:?}`", module_name);

    let module_root = strip_root.join(module_name.as_ref());
    if !module_root.join("__init__.py").is_file() {
        return Err(Error::MissingModule(module_root));
    }
    let mut files_visited = 0;
    for entry in WalkDir::new(module_root)
        .into_iter()
        .filter_entry(|entry| !exclude_matcher.is_match(entry.path()))
    {
        let entry = entry.map_err(|err| Error::WalkDir {
            root: source_tree.to_path_buf(),
            err,
        })?;

        files_visited += 1;
        if files_visited > 10000 {
            warn_user_once!(
                "Visited more than 10,000 files for wheel build. \
                Consider using more constrained includes or more excludes."
            );
        }

        // We only want to take the module root, but since excludes start at the source tree root,
        // we strip higher than we iterate.
        let match_path = entry
            .path()
            .strip_prefix(source_tree)
            .expect("walkdir starts with root");
        let wheel_path = entry
            .path()
            .strip_prefix(&strip_root)
            .expect("walkdir starts with root");
        if exclude_matcher.is_match(match_path) {
            trace!("Excluding from module: `{}`", match_path.user_display());
            continue;
        }
        let wheel_path = wheel_path.portable_display().to_string();

        debug!("Adding to wheel: `{wheel_path}`");

        if entry.file_type().is_dir() {
            wheel_writer.write_directory(&wheel_path)?;
        } else if entry.file_type().is_file() {
            wheel_writer.write_file(&wheel_path, entry.path())?;
        } else {
            // TODO(konsti): We may want to support symlinks, there is support for installing them.
            return Err(Error::UnsupportedFileType(
                entry.path().to_path_buf(),
                entry.file_type(),
            ));
        }
    }
    debug!("Visited {files_visited} files for wheel build");

    // Add the license files
    if pyproject_toml.license_files_wheel().next().is_some() {
        debug!("Adding license files");
        let license_dir = format!(
            "{}-{}.dist-info/licenses/",
            pyproject_toml.name().as_dist_info_name(),
            pyproject_toml.version()
        );

        wheel_subdir_from_globs(
            source_tree,
            &license_dir,
            pyproject_toml.license_files_wheel(),
            &mut wheel_writer,
            "project.license-files",
        )?;
    }

    // Add the data files
    for (name, directory) in settings.data.iter() {
        debug!("Adding {name} data files from: `{directory}`");
        let data_dir = format!(
            "{}-{}.data/{}/",
            pyproject_toml.name().as_dist_info_name(),
            pyproject_toml.version(),
            name
        );

        wheel_subdir_from_globs(
            &source_tree.join(directory),
            &data_dir,
            &["**".to_string()],
            &mut wheel_writer,
            &format!("tool.uv.build-backend.data.{name}"),
        )?;
    }

    debug!("Adding metadata files to wheel");
    let dist_info_dir = write_dist_info(
        &mut wheel_writer,
        pyproject_toml,
        filename,
        source_tree,
        uv_version,
    )?;
    wheel_writer.close(&dist_info_dir)?;

    Ok(())
}

/// Build a wheel from the source tree and place it in the output directory.
pub fn build_editable(
    source_tree: &Path,
    wheel_dir: &Path,
    metadata_directory: Option<&Path>,
    uv_version: &str,
) -> Result<WheelFilename, Error> {
    let contents = fs_err::read_to_string(source_tree.join("pyproject.toml"))?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    for warning in pyproject_toml.check_build_system(uv_version) {
        warn_user_once!("{warning}");
    }
    let settings = pyproject_toml
        .settings()
        .cloned()
        .unwrap_or_else(BuildBackendSettings::default);

    crate::check_metadata_directory(source_tree, metadata_directory, &pyproject_toml)?;

    let filename = WheelFilename::new(
        pyproject_toml.name().clone(),
        pyproject_toml.version().clone(),
        LanguageTag::Python {
            major: 3,
            minor: None,
        },
        AbiTag::None,
        PlatformTag::Any,
    );

    let wheel_path = wheel_dir.join(filename.to_string());
    debug!("Writing wheel at {}", wheel_path.user_display());
    let mut wheel_writer = ZipDirectoryWriter::new_wheel(File::create(&wheel_path)?);

    debug!("Adding pth file to {}", wheel_path.user_display());
    if settings.module_root.is_absolute() {
        return Err(Error::AbsoluteModuleRoot(settings.module_root.clone()));
    }
    let src_root = source_tree.join(settings.module_root);

    let module_name = if let Some(module_name) = settings.module_name {
        module_name
    } else {
        // Should never error, the rules for package names (in dist-info formatting) are stricter
        // than those for identifiers
        Identifier::from_str(pyproject_toml.name().as_dist_info_name().as_ref())?
    };
    debug!("Module name: `{:?}`", module_name);

    let module_root = src_root.join(module_name.as_ref());
    if !module_root.join("__init__.py").is_file() {
        return Err(Error::MissingModule(module_root));
    }
    wheel_writer.write_bytes(
        &format!("{}.pth", pyproject_toml.name().as_dist_info_name()),
        src_root.as_os_str().as_encoded_bytes(),
    )?;

    debug!("Adding metadata files to: `{}`", wheel_path.user_display());
    let dist_info_dir = write_dist_info(
        &mut wheel_writer,
        &pyproject_toml,
        &filename,
        source_tree,
        uv_version,
    )?;
    wheel_writer.close(&dist_info_dir)?;

    Ok(filename)
}

/// Write the dist-info directory to the output directory without building the wheel.
pub fn metadata(
    source_tree: &Path,
    metadata_directory: &Path,
    uv_version: &str,
) -> Result<String, Error> {
    let contents = fs_err::read_to_string(source_tree.join("pyproject.toml"))?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    for warning in pyproject_toml.check_build_system(uv_version) {
        warn_user_once!("{warning}");
    }

    let filename = WheelFilename::new(
        pyproject_toml.name().clone(),
        pyproject_toml.version().clone(),
        LanguageTag::Python {
            major: 3,
            minor: None,
        },
        AbiTag::None,
        PlatformTag::Any,
    );

    debug!(
        "Writing metadata files to {}",
        metadata_directory.user_display()
    );
    let mut wheel_writer = FilesystemWriter::new(metadata_directory);
    let dist_info_dir = write_dist_info(
        &mut wheel_writer,
        &pyproject_toml,
        &filename,
        source_tree,
        uv_version,
    )?;
    wheel_writer.close(&dist_info_dir)?;

    Ok(dist_info_dir)
}

/// An entry in the `RECORD` file.
///
/// <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#the-record-file>
struct RecordEntry {
    /// The path to the file relative to the package root.
    ///
    /// While the spec would allow backslashes, we always use portable paths with forward slashes.
    path: String,
    /// The SHA256 of the files.
    hash: String,
    /// The size of the file in bytes.
    size: usize,
}

/// Read the input file and write it both to the hasher and the target file.
///
/// We're implementing this tee-ing manually since there is no sync `InspectReader` or std tee
/// function.
fn write_hashed(
    path: &str,
    reader: &mut dyn Read,
    writer: &mut dyn Write,
) -> Result<RecordEntry, io::Error> {
    let mut hasher = Sha256::new();
    let mut size = 0;
    // 8KB is the default defined in `std::sys_common::io`.
    let mut buffer = vec![0; 8 * 1024];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(read) => read,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        if read == 0 {
            // End of file
            break;
        }
        hasher.update(&buffer[..read]);
        writer.write_all(&buffer[..read])?;
        size += read;
    }
    Ok(RecordEntry {
        path: path.to_string(),
        hash: format!("{:x}", hasher.finalize()),
        size,
    })
}

/// Write the `RECORD` file.
///
/// <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#the-record-file>
fn write_record(
    writer: &mut dyn Write,
    dist_info_dir: &str,
    record: Vec<RecordEntry>,
) -> Result<(), Error> {
    let mut record_writer = csv::Writer::from_writer(writer);
    for entry in record {
        record_writer.write_record(&[
            entry.path,
            format!("sha256={}", entry.hash),
            entry.size.to_string(),
        ])?;
    }

    // We can't compute the hash or size for RECORD without modifying it at the same time.
    record_writer.write_record(&[
        format!("{dist_info_dir}/RECORD"),
        String::new(),
        String::new(),
    ])?;
    record_writer.flush()?;
    Ok(())
}

/// Build a globset matcher for excludes.
pub(crate) fn build_exclude_matcher(
    excludes: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<GlobSet, Error> {
    let mut exclude_builder = GlobSetBuilder::new();
    for exclude in excludes {
        let exclude = exclude.as_ref();
        // Excludes are unanchored
        let exclude = if let Some(exclude) = exclude.strip_prefix("/") {
            exclude.to_string()
        } else {
            format!("**/{exclude}").to_string()
        };
        let glob = parse_portable_glob(&exclude).map_err(|err| Error::PortableGlob {
            field: "tool.uv.build-backend.*-exclude".to_string(),
            source: err,
        })?;
        exclude_builder.add(glob);
    }
    let exclude_matcher = exclude_builder
        .build()
        .map_err(|err| Error::GlobSetTooLarge {
            field: "tool.uv.build-backend.*-exclude".to_string(),
            source: err,
        })?;
    Ok(exclude_matcher)
}

/// Add the files and directories matching from the source tree matching any of the globs in the
/// wheel subdirectory.
fn wheel_subdir_from_globs(
    src: &Path,
    target: &str,
    globs: impl IntoIterator<Item = impl AsRef<str>>,
    wheel_writer: &mut impl DirectoryWriter,
    // For error messages
    globs_field: &str,
) -> Result<(), Error> {
    let license_files_globs: Vec<_> = globs
        .into_iter()
        .map(|license_files| {
            let license_files = license_files.as_ref();
            trace!(
                "Including {} at `{}` with `{}`",
                globs_field,
                src.user_display(),
                license_files
            );
            parse_portable_glob(license_files)
        })
        .collect::<Result<_, _>>()
        .map_err(|err| Error::PortableGlob {
            field: globs_field.to_string(),
            source: err,
        })?;
    let matcher =
        GlobDirFilter::from_globs(&license_files_globs).map_err(|err| Error::GlobSetTooLarge {
            field: globs_field.to_string(),
            source: err,
        })?;

    wheel_writer.write_directory(target)?;

    for entry in WalkDir::new(src).into_iter().filter_entry(|entry| {
        // TODO(konsti): This should be prettier.
        let relative = entry
            .path()
            .strip_prefix(src)
            .expect("walkdir starts with root");

        // Fast path: Don't descend into a directory that can't be included.
        matcher.match_directory(relative)
    }) {
        let entry = entry.map_err(|err| Error::WalkDir {
            root: src.to_path_buf(),
            err,
        })?;

        // Skip the root path, which is already included as `target` prior to the loop.
        // (If `entry.path() == src`, then `relative` is empty, and `relative_licenses` is
        // `target`.)
        if entry.path() == src {
            continue;
        }

        // TODO(konsti): This should be prettier.
        let relative = entry
            .path()
            .strip_prefix(src)
            .expect("walkdir starts with root");

        if !matcher.match_path(relative) {
            trace!("Excluding {}: `{}`", globs_field, relative.user_display());
            continue;
        };

        let relative_licenses = Path::new(target)
            .join(relative)
            .portable_display()
            .to_string();

        if entry.file_type().is_dir() {
            wheel_writer.write_directory(&relative_licenses)?;
        } else if entry.file_type().is_file() {
            debug!("Adding {} file: `{}`", globs_field, relative.user_display());
            wheel_writer.write_file(&relative_licenses, entry.path())?;
        } else {
            // TODO(konsti): We may want to support symlinks, there is support for installing them.
            return Err(Error::UnsupportedFileType(
                entry.path().to_path_buf(),
                entry.file_type(),
            ));
        }
    }
    Ok(())
}

/// Add `METADATA` and `entry_points.txt` to the dist-info directory.
///
/// Returns the name of the dist-info directory.
fn write_dist_info(
    writer: &mut dyn DirectoryWriter,
    pyproject_toml: &PyProjectToml,
    filename: &WheelFilename,
    root: &Path,
    uv_version: &str,
) -> Result<String, Error> {
    let dist_info_dir = format!(
        "{}-{}.dist-info",
        pyproject_toml.name().as_dist_info_name(),
        pyproject_toml.version()
    );

    writer.write_directory(&dist_info_dir)?;

    // Add `WHEEL`.
    let wheel_info = wheel_info(filename, uv_version);
    writer.write_bytes(&format!("{dist_info_dir}/WHEEL"), wheel_info.as_bytes())?;

    // Add `entry_points.txt`.
    if let Some(entrypoint) = pyproject_toml.to_entry_points()? {
        writer.write_bytes(
            &format!("{dist_info_dir}/entry_points.txt"),
            entrypoint.as_bytes(),
        )?;
    }

    // Add `METADATA`.
    let metadata = pyproject_toml.to_metadata(root)?.core_metadata_format();
    writer.write_bytes(&format!("{dist_info_dir}/METADATA"), metadata.as_bytes())?;

    // `RECORD` is added on closing.

    Ok(dist_info_dir)
}

/// Returns the `WHEEL` file contents.
fn wheel_info(filename: &WheelFilename, uv_version: &str) -> String {
    // https://packaging.python.org/en/latest/specifications/binary-distribution-format/#file-contents
    let mut wheel_info = vec![
        ("Wheel-Version", "1.0".to_string()),
        ("Generator", format!("uv {uv_version}")),
        ("Root-Is-Purelib", "true".to_string()),
    ];
    for python_tag in filename.python_tags() {
        for abi_tag in filename.abi_tags() {
            for platform_tag in filename.platform_tags() {
                wheel_info.push(("Tag", format!("{python_tag}-{abi_tag}-{platform_tag}")));
            }
        }
    }
    wheel_info
        .into_iter()
        .map(|(key, value)| format!("{key}: {value}"))
        .join("\n")
}

/// Zip archive (wheel) writer.
struct ZipDirectoryWriter {
    writer: ZipWriter<File>,
    compression: CompressionMethod,
    /// The entries in the `RECORD` file.
    record: Vec<RecordEntry>,
}

impl ZipDirectoryWriter {
    /// A wheel writer with deflate compression.
    fn new_wheel(file: File) -> Self {
        Self {
            writer: ZipWriter::new(file),
            compression: CompressionMethod::Deflated,
            record: Vec::new(),
        }
    }

    /// A wheel writer with no (stored) compression.
    ///
    /// Since editables are temporary, we save time be skipping compression and decompression.
    #[expect(dead_code)]
    fn new_editable(file: File) -> Self {
        Self {
            writer: ZipWriter::new(file),
            compression: CompressionMethod::Stored,
            record: Vec::new(),
        }
    }

    /// Add a file with the given name and return a writer for it.
    fn new_writer<'slf>(
        &'slf mut self,
        path: &str,
        executable_bit: bool,
    ) -> Result<Box<dyn Write + 'slf>, Error> {
        // 644 is the default of the zip crate.
        let permissions = if executable_bit { 775 } else { 664 };
        let options = zip::write::SimpleFileOptions::default()
            .unix_permissions(permissions)
            .compression_method(self.compression);
        self.writer.start_file(path, options)?;
        Ok(Box::new(&mut self.writer))
    }
}

impl DirectoryWriter for ZipDirectoryWriter {
    fn write_bytes(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error> {
        trace!("Adding {}", path);
        let options = zip::write::SimpleFileOptions::default().compression_method(self.compression);
        self.writer.start_file(path, options)?;
        self.writer.write_all(bytes)?;

        let hash = format!("{:x}", Sha256::new().chain_update(bytes).finalize());
        self.record.push(RecordEntry {
            path: path.to_string(),
            hash,
            size: bytes.len(),
        });

        Ok(())
    }

    fn write_file(&mut self, path: &str, file: &Path) -> Result<(), Error> {
        trace!("Adding {} from {}", path, file.user_display());
        let mut reader = BufReader::new(File::open(file)?);
        // Preserve the executable bit, especially for scripts
        #[cfg(unix)]
        let executable_bit = {
            use std::os::unix::fs::PermissionsExt;
            file.metadata()?.permissions().mode() & 0o111 != 0
        };
        // Windows has no executable bit
        #[cfg(not(unix))]
        let executable_bit = false;
        let mut writer = self.new_writer(path, executable_bit)?;
        let record = write_hashed(path, &mut reader, &mut writer)?;
        drop(writer);
        self.record.push(record);
        Ok(())
    }

    fn write_directory(&mut self, directory: &str) -> Result<(), Error> {
        trace!("Adding directory {}", directory);
        let options = zip::write::SimpleFileOptions::default().compression_method(self.compression);
        Ok(self.writer.add_directory(directory, options)?)
    }

    /// Write the `RECORD` file and the central directory.
    fn close(mut self, dist_info_dir: &str) -> Result<(), Error> {
        let record_path = format!("{dist_info_dir}/RECORD");
        trace!("Adding {record_path}");
        let record = mem::take(&mut self.record);
        write_record(
            &mut self.new_writer(&record_path, false)?,
            dist_info_dir,
            record,
        )?;

        trace!("Adding central directory");
        self.writer.finish()?;
        Ok(())
    }
}

struct FilesystemWriter {
    /// The virtualenv or metadata directory that add file paths are relative to.
    root: PathBuf,
    /// The entries in the `RECORD` file.
    record: Vec<RecordEntry>,
}

impl FilesystemWriter {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            record: Vec::new(),
        }
    }

    /// Add a file with the given name and return a writer for it.
    fn new_writer<'slf>(&'slf mut self, path: &str) -> Result<Box<dyn Write + 'slf>, Error> {
        trace!("Adding {}", path);
        Ok(Box::new(File::create(self.root.join(path))?))
    }
}

/// File system writer.
impl DirectoryWriter for FilesystemWriter {
    fn write_bytes(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error> {
        trace!("Adding {}", path);
        let hash = format!("{:x}", Sha256::new().chain_update(bytes).finalize());
        self.record.push(RecordEntry {
            path: path.to_string(),
            hash,
            size: bytes.len(),
        });

        Ok(fs_err::write(self.root.join(path), bytes)?)
    }
    fn write_file(&mut self, path: &str, file: &Path) -> Result<(), Error> {
        trace!("Adding {} from {}", path, file.user_display());
        let mut reader = BufReader::new(File::open(file)?);
        let mut writer = self.new_writer(path)?;
        let record = write_hashed(path, &mut reader, &mut writer)?;
        drop(writer);
        self.record.push(record);
        Ok(())
    }

    fn write_directory(&mut self, directory: &str) -> Result<(), Error> {
        trace!("Adding directory {}", directory);
        Ok(fs_err::create_dir(self.root.join(directory))?)
    }

    /// Write the `RECORD` file.
    fn close(mut self, dist_info_dir: &str) -> Result<(), Error> {
        let record = mem::take(&mut self.record);
        write_record(
            &mut self.new_writer(&format!("{dist_info_dir}/RECORD"))?,
            dist_info_dir,
            record,
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use insta::assert_snapshot;
    use std::path::Path;
    use std::str::FromStr;
    use tempfile::TempDir;
    use uv_distribution_filename::WheelFilename;
    use uv_fs::Simplified;
    use uv_normalize::PackageName;
    use uv_pep440::Version;
    use uv_platform_tags::{AbiTag, PlatformTag};
    use walkdir::WalkDir;

    #[test]
    fn test_wheel() {
        let filename = WheelFilename::new(
            PackageName::from_str("foo").unwrap(),
            Version::from_str("1.2.3").unwrap(),
            LanguageTag::Python {
                major: 3,
                minor: None,
            },
            AbiTag::None,
            PlatformTag::Any,
        );

        assert_snapshot!(wheel_info(&filename, "1.0.0+test"), @r"
        Wheel-Version: 1.0
        Generator: uv 1.0.0+test
        Root-Is-Purelib: true
        Tag: py3-none-any
    ");
    }

    #[test]
    fn test_record() {
        let record = vec![RecordEntry {
            path: "built_by_uv/__init__.py".to_string(),
            hash: "89f869e53a3a0061a52c0233e6442d4d72de80a8a2d3406d9ea0bfd397ed7865".to_string(),
            size: 37,
        }];

        let mut writer = Vec::new();
        write_record(&mut writer, "built_by_uv-0.1.0", record).unwrap();
        assert_snapshot!(String::from_utf8(writer).unwrap(), @r"
            built_by_uv/__init__.py,sha256=89f869e53a3a0061a52c0233e6442d4d72de80a8a2d3406d9ea0bfd397ed7865,37
            built_by_uv-0.1.0/RECORD,,
        ");
    }

    /// Snapshot all files from the prepare metadata hook.
    #[test]
    fn test_prepare_metadata() {
        let metadata_dir = TempDir::new().unwrap();
        let built_by_uv = Path::new("../../scripts/packages/built-by-uv");
        metadata(built_by_uv, metadata_dir.path(), "1.0.0+test").unwrap();

        let mut files: Vec<_> = WalkDir::new(metadata_dir.path())
            .into_iter()
            .map(|entry| {
                entry
                    .unwrap()
                    .path()
                    .strip_prefix(metadata_dir.path())
                    .expect("walkdir starts with root")
                    .portable_display()
                    .to_string()
            })
            .filter(|path| !path.is_empty())
            .collect();
        files.sort();
        assert_snapshot!(files.join("\n"), @r###"
        built_by_uv-0.1.0.dist-info
        built_by_uv-0.1.0.dist-info/METADATA
        built_by_uv-0.1.0.dist-info/RECORD
        built_by_uv-0.1.0.dist-info/WHEEL
        built_by_uv-0.1.0.dist-info/entry_points.txt
        "###);

        let metadata_file = metadata_dir
            .path()
            .join("built_by_uv-0.1.0.dist-info/METADATA");
        assert_snapshot!(fs_err::read_to_string(metadata_file).unwrap(), @r###"
        Metadata-Version: 2.4
        Name: built-by-uv
        Version: 0.1.0
        Summary: A package to be built with the uv build backend that uses all features exposed by the build backend
        License-File: LICENSE-APACHE
        License-File: LICENSE-MIT
        License-File: third-party-licenses/PEP-401.txt
        Requires-Dist: anyio>=4,<5
        Requires-Python: >=3.12
        Description-Content-Type: text/markdown

        # built_by_uv

        A package to be built with the uv build backend that uses all features exposed by the build backend.
        "###);

        let record_file = metadata_dir
            .path()
            .join("built_by_uv-0.1.0.dist-info/RECORD");
        assert_snapshot!(fs_err::read_to_string(record_file).unwrap(), @r###"
        built_by_uv-0.1.0.dist-info/WHEEL,sha256=3da1bfa0e8fd1b6cc246aa0b2b44a35815596c600cb485c39a6f8c106c3d5a8d,83
        built_by_uv-0.1.0.dist-info/entry_points.txt,sha256=f883bac9aabac7a1d297ecd61fdeab666818bdfc87947d342f9590a02a73f982,50
        built_by_uv-0.1.0.dist-info/METADATA,sha256=9ba12456f2ab1a6ab1e376ff551e392c70f7ec86713d80b4348e90c7dfd45cb1,474
        built_by_uv-0.1.0.dist-info/RECORD,,
        "###);

        let wheel_file = metadata_dir
            .path()
            .join("built_by_uv-0.1.0.dist-info/WHEEL");
        assert_snapshot!(fs_err::read_to_string(wheel_file).unwrap(), @r###"
        Wheel-Version: 1.0
        Generator: uv 1.0.0+test
        Root-Is-Purelib: true
        Tag: py3-none-any
    "###);
    }
}
