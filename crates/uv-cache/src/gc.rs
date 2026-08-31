use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use rustc_hash::FxHashSet;
use tracing::debug;

use crate::usage::{UPDATE_INTERVAL, last_used, marker_path, touch};
use crate::{Cache, CacheBucket};

impl Cache {
    /// Remove unpacked wheels whose recorded use is older than `max_age`.
    ///
    /// The caller must hold the exclusive cache lock. Cached environments, source trees, compressed
    /// built wheels, and metadata are retained. The update interval is added to the retention period
    /// because usage markers can lag the last use by that amount.
    ///
    /// Returns the archive paths removed, or that would be removed during a dry run. A dry run does
    /// not update usage records. Otherwise, archives without records receive a fresh timestamp.
    pub fn prune_unused(&self, max_age: Duration, dry_run: bool) -> io::Result<Vec<PathBuf>> {
        if !self.supports_age_pruning()? {
            return Ok(Vec::new());
        }

        let archive_root = fs_err::canonicalize(self.root())?.join(CacheBucket::Archive.to_str());
        let protected = self.environment_archives(&archive_root)?;
        let references = self.find_archive_references()?;
        let max_age = max_age.saturating_add(UPDATE_INTERVAL);
        let now = SystemTime::now();
        let mut expired = Vec::new();

        // Discover deletion targets from known wheel references, never from marker filenames.
        // Archives without such references may belong to another cache user and are left alone.
        for (archive, references) in references {
            if archive.parent() != Some(archive_root.as_path()) || protected.contains(&archive) {
                continue;
            }
            let Some(name) = archive.file_name() else {
                continue;
            };
            let marker = marker_path(self.root(), name);
            let Some(last_used) = last_used(&marker)? else {
                if !dry_run {
                    touch(self.root(), name)?;
                }
                continue;
            };
            if !now
                .duration_since(last_used)
                .is_ok_and(|elapsed| elapsed > max_age)
            {
                continue;
            }

            if !dry_run {
                for reference in references {
                    // URL and local-wheel cache hits can trust these pointers without checking
                    // that the payload still exists. Remove pointers before unlinking the archive.
                    if reference.starts_with(self.bucket(CacheBucket::Wheels))
                        && let Some(filename) = reference.file_name()
                    {
                        for extension in [".http", ".rev"] {
                            let mut pointer = filename.to_os_string();
                            pointer.push(extension);
                            self.remove_path(reference.with_file_name(pointer))?;
                        }
                    }
                    self.remove_path(reference)?;
                }
                self.remove_path(&archive)?;
                fs_err::remove_file(marker)?;
            }
            expired.push(archive);
        }
        expired.sort_unstable();
        Ok(expired)
    }

    /// Avoid collecting archives whose references may use an unknown bucket format.
    fn supports_age_pruning(&self) -> io::Result<bool> {
        let buckets = [
            ("wheels-v", CacheBucket::Wheels),
            ("sdists-v", CacheBucket::SourceDistributions),
            ("environments-v", CacheBucket::Environments),
            ("archive-v", CacheBucket::Archive),
        ];
        for entry in fs_err::read_dir(self.root())? {
            let entry = entry?;
            let name = entry.file_name();
            for (prefix, bucket) in buckets {
                if name
                    .to_str()
                    .is_some_and(|name| name.starts_with(prefix) && name != bucket.to_str())
                {
                    debug!("Skipping age-based pruning with unknown cache bucket: {name:?}");
                    return Ok(false);
                }
                if name == bucket.to_str() && entry.file_type()?.is_symlink() {
                    debug!("Skipping age-based pruning with linked cache bucket: {name:?}");
                    return Ok(false);
                }
            }
        }
        for path in [
            self.bucket(CacheBucket::Usage),
            self.bucket(CacheBucket::Usage)
                .join(CacheBucket::Archive.to_str()),
        ] {
            match fs_err::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_dir() => {}
                Ok(_) => {
                    debug!(
                        "Skipping age-based pruning with invalid usage directory: {}",
                        path.display()
                    );
                    return Ok(false);
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }
        Ok(true)
    }

    /// Find archives owned by retained environments, including symlinked wheel dependencies.
    fn environment_archives(&self, archive_root: &Path) -> io::Result<FxHashSet<PathBuf>> {
        let environments = self.bucket(CacheBucket::Environments);
        let mut pending = vec![environments.clone()];
        let mut protected = FxHashSet::default();
        while let Some(root) = pending.pop() {
            for entry in walkdir::WalkDir::new(&root).follow_root_links(false) {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(err)
                        if err
                            .io_error()
                            .is_some_and(|err| err.kind() == io::ErrorKind::NotFound) =>
                    {
                        continue;
                    }
                    Err(err) => return Err(err.into()),
                };
                let target = if entry.file_type().is_symlink() {
                    fs_err::canonicalize(entry.path())
                } else if cfg!(windows)
                    && root == environments
                    && entry.depth() == 2
                    && entry.file_type().is_file()
                {
                    // Windows represents cached-environment links using structured files at
                    // environments/<interpreter>/<resolution>, rather than filesystem symlinks.
                    self.resolve_link(entry.path())
                } else {
                    continue;
                };
                let target = match target {
                    Ok(target) => target,
                    Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
                    // Files such as pyvenv.cfg can occupy the same depth as a Windows link.
                    Err(err) if err.kind() == io::ErrorKind::InvalidData => continue,
                    Err(err) => return Err(err),
                };
                if let Ok(relative) = target.strip_prefix(archive_root)
                    && let Some(Component::Normal(name)) = relative.components().next()
                {
                    let archive = archive_root.join(name);
                    if protected.insert(archive.clone()) {
                        pending.push(archive);
                    }
                }
            }
        }
        Ok(protected)
    }
}
