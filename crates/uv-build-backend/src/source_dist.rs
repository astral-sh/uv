use crate::metadata::DEFAULT_EXCLUDES;
use crate::wheel::build_exclude_matcher;
use crate::{
    BuildBackendSettings, DirectoryWriter, Error, FileList, ListWriter, PyProjectToml,
    error_on_venv, find_roots,
};
use flate2::Compression;
use flate2::write::GzEncoder;
use fs_err::File;
use globset::{Glob, GlobSet};
use std::io;
use std::io::{BufReader, Cursor, Write};
use std::path::{Component, Path, PathBuf};
use tar::{EntryType, Header};
use tracing::{debug, trace};
use uv_distribution_filename::{SourceDistExtension, SourceDistFilename};
use uv_fs::Simplified;
use uv_globfilter::{GlobDirFilter, PortableGlobParser};
use uv_warnings::warn_user_once;
use walkdir::WalkDir;

/// Build a source distribution from the source tree and place it in the output directory.
pub fn build_source_dist(
    source_tree: &Path,
    source_dist_directory: &Path,
    uv_version: &str,
    show_warnings: bool,
) -> Result<SourceDistFilename, Error> {
    let pyproject_toml = PyProjectToml::parse(&source_tree.join("pyproject.toml"))?;
    let filename = SourceDistFilename {
        name: pyproject_toml.name().clone(),
        version: pyproject_toml.version().clone(),
        extension: SourceDistExtension::TarGz,
    };
    let source_dist_path = source_dist_directory.join(filename.to_string());

    if source_dist_path.exists() {
        fs_err::remove_file(&source_dist_path)?;
    }

    let temp_file = uv_fs::tempfile_in(source_dist_directory)?;
    let writer = TarGzWriter::new(temp_file.as_file(), &source_dist_path);
    write_source_dist(source_tree, writer, uv_version, show_warnings)?;
    temp_file
        .persist(&source_dist_path)
        .map_err(|err| Error::Persist(source_dist_path.clone(), err.error))?;

    Ok(filename)
}

/// List the files that would be included in a source distribution and their origin.
pub fn list_source_dist(
    source_tree: &Path,
    uv_version: &str,
    show_warnings: bool,
) -> Result<(SourceDistFilename, FileList), Error> {
    let pyproject_toml = PyProjectToml::parse(&source_tree.join("pyproject.toml"))?;
    let filename = SourceDistFilename {
        name: pyproject_toml.name().clone(),
        version: pyproject_toml.version().clone(),
        extension: SourceDistExtension::TarGz,
    };
    let mut files = FileList::new();
    let writer = ListWriter::new(&mut files);
    write_source_dist(source_tree, writer, uv_version, show_warnings)?;
    Ok((filename, files))
}

/// Build includes and excludes for source tree walking for source dists.
fn source_dist_matcher(
    source_tree: &Path,
    pyproject_toml: &PyProjectToml,
    settings: BuildBackendSettings,
    show_warnings: bool,
) -> Result<(GlobDirFilter, GlobSet), Error> {
    // File and directories to include in the source directory
    let mut include_globs = Vec::new();
    let mut includes: Vec<String> = settings.source_include;
    // pyproject.toml is always included.
    includes.push(globset::escape("pyproject.toml"));

    // Check that the source tree contains a module.
    let (src_root, modules_relative) = find_roots(
        source_tree,
        pyproject_toml,
        &settings.module_root,
        settings.module_name.as_ref(),
        settings.namespace,
        show_warnings,
    )?;
    for module_relative in modules_relative {
        // The wheel must not include any files included by the source distribution (at least until we
        // have files generated in the source dist -> wheel build step).
        let import_path = uv_fs::normalize_path(
            &uv_fs::relative_to(src_root.join(module_relative), source_tree)
                .expect("module root is inside source tree"),
        )
        .portable_display()
        .to_string();
        includes.push(format!("{}/**", globset::escape(&import_path)));
    }
    for include in includes {
        let glob = PortableGlobParser::Uv
            .parse(&include)
            .map_err(|err| Error::PortableGlob {
                field: "tool.uv.build-backend.source-include".to_string(),
                source: err,
            })?;
        include_globs.push(glob);
    }

    // Include the Readme
    if let Some(readme) = pyproject_toml
        .readme()
        .as_ref()
        .and_then(|readme| readme.path())
    {
        let readme = uv_fs::normalize_path(readme);
        trace!("Including readme at: {}", readme.user_display());
        let readme = readme.portable_display().to_string();
        let glob = Glob::new(&globset::escape(&readme)).expect("escaped globset is parseable");
        include_globs.push(glob);
    }

    // Include the license files
    for license_files in pyproject_toml.license_files_source_dist() {
        trace!("Including license files at: {license_files}`");
        let glob = PortableGlobParser::Pep639
            .parse(license_files)
            .map_err(|err| Error::PortableGlob {
                field: "project.license-files".to_string(),
                source: err,
            })?;
        include_globs.push(glob);
    }

    // Include the data files
    for (name, directory) in settings.data.iter() {
        let directory = uv_fs::normalize_path(directory);
        trace!("Including data ({}) at: {}", name, directory.user_display());
        if directory
            .components()
            .next()
            .is_some_and(|component| !matches!(component, Component::CurDir | Component::Normal(_)))
        {
            return Err(Error::InvalidDataRoot {
                name: name.to_string(),
                path: directory.to_path_buf(),
            });
        }
        let directory = directory.portable_display().to_string();
        let glob = PortableGlobParser::Uv
            .parse(&format!("{}/**", globset::escape(&directory)))
            .map_err(|err| Error::PortableGlob {
                field: format!("tool.uv.build-backend.data.{name}"),
                source: err,
            })?;
        include_globs.push(glob);
    }

    debug!(
        "Source distribution includes: {:?}",
        include_globs
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    let include_matcher =
        GlobDirFilter::from_globs(&include_globs).map_err(|err| Error::GlobSetTooLarge {
            field: "tool.uv.build-backend.source-include".to_string(),
            source: err,
        })?;

    let mut excludes: Vec<String> = Vec::new();
    if settings.default_excludes {
        excludes.extend(DEFAULT_EXCLUDES.iter().map(ToString::to_string));
    }
    for exclude in settings.source_exclude {
        // Avoid duplicate entries.
        if !excludes.contains(&exclude) {
            excludes.push(exclude);
        }
    }
    debug!("Source dist excludes: {:?}", excludes);
    let exclude_matcher = build_exclude_matcher(excludes)?;
    if exclude_matcher.is_match("pyproject.toml") {
        return Err(Error::PyprojectTomlExcluded);
    }
    Ok((include_matcher, exclude_matcher))
}

/// Shared implementation for building and listing a source distribution.
fn write_source_dist(
    source_tree: &Path,
    mut writer: impl DirectoryWriter,
    uv_version: &str,
    show_warnings: bool,
) -> Result<SourceDistFilename, Error> {
    let pyproject_toml = PyProjectToml::parse(&source_tree.join("pyproject.toml"))?;
    for warning in pyproject_toml.check_build_system(uv_version) {
        warn_user_once!("{warning}");
    }
    let settings = pyproject_toml
        .settings()
        .cloned()
        .unwrap_or_else(BuildBackendSettings::default);

    let filename = SourceDistFilename {
        name: pyproject_toml.name().clone(),
        version: pyproject_toml.version().clone(),
        extension: SourceDistExtension::TarGz,
    };

    let top_level = format!(
        "{}-{}",
        pyproject_toml.name().as_dist_info_name(),
        pyproject_toml.version()
    );

    let metadata = pyproject_toml.to_metadata(source_tree)?;
    let metadata_email = metadata.core_metadata_format();

    debug!("Adding content files to source distribution");
    writer.write_bytes(
        &Path::new(&top_level)
            .join("PKG-INFO")
            .portable_display()
            .to_string(),
        metadata_email.as_bytes(),
    )?;

    let (include_matcher, exclude_matcher) =
        source_dist_matcher(source_tree, &pyproject_toml, settings, show_warnings)?;

    let mut files_visited = 0;
    for entry in WalkDir::new(source_tree)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            // TODO(konsti): This should be prettier.
            let relative = entry
                .path()
                .strip_prefix(source_tree)
                .expect("walkdir starts with root");

            // Fast path: Don't descend into a directory that can't be included. This is the most
            // important performance optimization, it avoids descending into directories such as
            // `.venv`. While walkdir is generally cheap, we still avoid traversing large data
            // directories that often exist on the top level of a project. This is especially noticeable
            // on network file systems with high latencies per operation (while contiguous reading may
            // still be fast).
            include_matcher.match_directory(relative) && !exclude_matcher.is_match(relative)
        })
    {
        let entry = entry.map_err(|err| Error::WalkDir {
            root: source_tree.to_path_buf(),
            err,
        })?;

        files_visited += 1;
        if files_visited > 10000 {
            warn_user_once!(
                "Visited more than 10,000 files for source distribution build. \
                Consider using more constrained includes or more excludes."
            );
        }
        // TODO(konsti): This should be prettier.
        let relative = entry
            .path()
            .strip_prefix(source_tree)
            .expect("walkdir starts with root");

        if !include_matcher.match_path(relative) || exclude_matcher.is_match(relative) {
            trace!("Excluding from sdist: {}", relative.user_display());
            continue;
        }

        error_on_venv(entry.file_name(), entry.path())?;

        let entry_path = Path::new(&top_level)
            .join(relative)
            .portable_display()
            .to_string();
        debug!("Adding to sdist: {}", relative.user_display());
        writer.write_dir_entry(&entry, &entry_path)?;
    }
    debug!("Visited {files_visited} files for source dist build");

    writer.close(&top_level)?;

    Ok(filename)
}

struct TarGzWriter<W: Write> {
    path: PathBuf,
    tar: tar::Builder<GzEncoder<W>>,
}

impl<W: Write> TarGzWriter<W> {
    fn new(writer: W, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let enc = GzEncoder::new(writer, Compression::default());
        let tar = tar::Builder::new(enc);
        Self { path, tar }
    }
}

impl<W: Write> DirectoryWriter for TarGzWriter<W> {
    fn write_bytes(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error> {
        let mut header = Header::new_gnu();
        // Work around bug in Python's std tar module
        // https://github.com/python/cpython/issues/141707
        // https://github.com/astral-sh/uv/pull/17043#issuecomment-3636841022
        header.set_entry_type(EntryType::Regular);
        header.set_size(bytes.len() as u64);
        // Reasonable default to avoid 0o000 permissions, the user's umask will be applied on
        // unpacking.
        header.set_mode(0o644);
        self.tar
            .append_data(&mut header, path, Cursor::new(bytes))
            .map_err(|err| Error::TarWrite(self.path.clone(), err))?;
        Ok(())
    }

    fn write_file(&mut self, path: &str, file: &Path) -> Result<(), Error> {
        let metadata = fs_err::metadata(file)?;
        let mut header = Header::new_gnu();
        // Work around bug in Python's std tar module
        // https://github.com/python/cpython/issues/141707
        // https://github.com/astral-sh/uv/pull/17043#issuecomment-3636841022
        header.set_entry_type(EntryType::Regular);
        // Preserve the executable bit, especially for scripts
        #[cfg(unix)]
        let executable_bit = {
            use std::os::unix::fs::PermissionsExt;
            file.metadata()?.permissions().mode() & 0o111 != 0
        };
        // Windows has no executable bit
        #[cfg(not(unix))]
        let executable_bit = false;

        // Set reasonable defaults to avoid 0o000 permissions, while avoiding adding the exact
        // filesystem permissions to the archive for reproducibility. Where applicable, the
        // operating system filters the stored permission by the user's umask when unpacking.
        if executable_bit {
            header.set_mode(0o755);
        } else {
            header.set_mode(0o644);
        }
        header.set_size(metadata.len());
        let reader = BufReader::new(File::open(file)?);
        self.tar
            .append_data(&mut header, path, reader)
            .map_err(|err| Error::TarWrite(self.path.clone(), err))?;
        Ok(())
    }

    fn write_directory(&mut self, directory: &str) -> Result<(), Error> {
        let mut header = Header::new_gnu();
        // Directories are always executable, which means they can be listed.
        header.set_mode(0o755);
        header.set_entry_type(EntryType::Directory);
        header.set_size(0);
        self.tar
            .append_data(&mut header, directory, io::empty())
            .map_err(|err| Error::TarWrite(self.path.clone(), err))?;
        Ok(())
    }

    fn close(mut self, _dist_info_dir: &str) -> Result<(), Error> {
        self.tar
            .finish()
            .map_err(|err| Error::TarWrite(self.path.clone(), err))?;
        Ok(())
    }
}
