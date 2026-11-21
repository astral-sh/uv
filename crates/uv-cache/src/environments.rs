//! Records of the Python environments that reference entries in the cache.
//!
//! The cache is shared between environments: a cached archive entry is only reclaimable once no
//! environment references it. uv records the packages installed into each environment, so that
//! `uv cache prune --unused` can determine which entries are no longer referenced.

use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rustc_hash::FxHashSet;
use tracing::debug;
use uv_warnings::warn_user;

use uv_cache_key::hash_digest;
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::{Cache, CacheBucket, Removal};

/// The version of the tracked environment format.
///
/// Records written by a different version are rejected rather than interpreted, since the set of
/// packages that an environment references can't be recovered from another format.
const TRACKING_VERSION: u32 = 1;

/// The packages that a single Python environment references in the cache.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct TrackedEnvironment {
    /// The version of the tracked environment format.
    version: u32,
    /// The root of the environment.
    root: PathBuf,
    /// The `site-packages` directories of the environment.
    site_packages: Vec<PathBuf>,
}

impl Cache {
    /// Record the packages installed in the environment at `root`.
    ///
    /// The environment is recorded as a snapshot: the previous record, if any, is replaced.
    pub fn track_environment(&self, root: &Path, site_packages: &[PathBuf]) -> io::Result<()> {
        let environment = TrackedEnvironment {
            version: TRACKING_VERSION,
            root: std::path::absolute(root)?,
            site_packages: site_packages.to_vec(),
        };
        let contents = rmp_serde::to_vec(&environment).map_err(io::Error::other)?;

        let directory = self.bucket(CacheBucket::EnvironmentRefs);
        fs_err::create_dir_all(&directory)?;
        uv_fs::write_atomic_sync(self.environment_record(&environment.root), contents)
    }

    /// Remove the cache entries that are no longer referenced by any environment.
    ///
    /// The entries installed into a recorded environment are kept, along with entries whose
    /// package can't be determined. When `dry_run` is set, the returned [`Removal`] describes the
    /// entries that would be removed, without removing them.
    ///
    /// `environments` enumerates additional environments to consider, as `site-packages`
    /// directories or environment roots.
    pub fn prune_unused(&self, environments: &[PathBuf], dry_run: bool) -> io::Result<Removal> {
        let mut summary = self.removal();

        // Collect the packages installed into the recorded environments.
        let mut installed = FxHashSet::default();
        let mut count = 0_usize;
        for (record, environment) in self.tracked_environments()? {
            let site_packages = environment
                .site_packages
                .into_iter()
                .filter(|site_packages| site_packages.is_dir())
                .collect::<Vec<_>>();

            // The environment was removed; drop the record so that later runs don't reconsider it.
            if site_packages.is_empty() {
                debug!(
                    "Removing stale environment record for `{}`",
                    environment.root.display()
                );
                if !dry_run {
                    summary += self.remove_path(&record)?;
                }
                continue;
            }

            count += 1;
            for site_packages in site_packages {
                installed.extend(installed_packages(&site_packages)?);
            }
        }

        // Collect the packages installed into the environments given on the command-line.
        for environment in environments {
            for site_packages in discover_site_packages(environment)? {
                installed.extend(installed_packages(&site_packages)?);
            }
            count += 1;
        }

        // Without any environment, every cache entry would appear to be unused.
        if count == 0 {
            warn_user!(
                "Skipping removal of unused cache entries: no recorded environments; run `uv sync` in a project, or pass `--environment` to include an environment"
            );
            return Ok(summary);
        }

        // Mark: the cache entries that the environments reference.
        let references = self.find_archive_references()?;
        let mut unused = Vec::new();
        for (archive, paths) in &references {
            let mut packages = Vec::new();
            for path in paths {
                if let Some(package) = self.reference_package(path) {
                    packages.push(package);
                }
            }

            // Without the package, the entry can't be attributed to an environment.
            if packages.is_empty() {
                if let Some(path) = paths.first() {
                    debug!(
                        "Keeping `{}`: unable to determine the package it provides",
                        path.display()
                    );
                }
                continue;
            }

            if packages.iter().any(|package| installed.contains(package)) {
                continue;
            }

            unused.push((archive, packages, paths));
        }

        // A single environment is a weak signal: the packages it lacks may still be installed in an
        // environment that was never recorded, so emptying the cache warrants a warning.
        if count == 1 && !references.is_empty() && unused.len() == references.len() {
            warn_user!(
                "Removing all {} cache entries: the only recorded environment doesn't reference any of them; pass `--environment` to include another environment",
                unused.len()
            );
        }

        // Sweep: the cache entries that no environment references.
        for (archive, packages, paths) in unused {
            for (name, version) in &packages {
                if dry_run {
                    debug!("Would remove cached package `{name}` (`{version}`)");
                } else {
                    debug!("Removing cached package `{name}` (`{version}`)");
                }
            }

            let mut entries = Vec::new();
            for path in paths {
                entries.extend(self.unused_entries(path)?);
            }
            entries.push(archive.clone());

            for entry in entries {
                if dry_run {
                    summary += self.measure_path(&entry)?;
                } else {
                    summary += self.remove_path(&entry)?;
                }
            }
        }

        Ok(summary)
    }

    /// The directory containing the tracked environments.
    fn environment_records(&self) -> PathBuf {
        self.bucket(CacheBucket::EnvironmentRefs)
    }

    /// The record for the environment at `root`.
    fn environment_record(&self, root: &Path) -> PathBuf {
        self.environment_records()
            .join(format!("{}.msgpack", hash_digest(&root.to_path_buf())))
    }

    /// Read the recorded environments, along with the path to their records.
    fn tracked_environments(&self) -> io::Result<Vec<(PathBuf, TrackedEnvironment)>> {
        let mut environments = Vec::new();
        let entries = match fs_err::read_dir(self.environment_records()) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(environments),
            Err(err) => return Err(err),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("msgpack"))
            {
                continue;
            }

            let contents = fs_err::read(&path)?;
            let environment =
                rmp_serde::from_slice::<TrackedEnvironment>(&contents).map_err(|err| {
                    io::Error::other(format!(
                        "Failed to read the environment recorded in `{}`: {err}",
                        path.display()
                    ))
                })?;

            if environment.version != TRACKING_VERSION {
                return Err(io::Error::other(format!(
                    "Unsupported version of the environment recorded in `{}`: {}",
                    path.display(),
                    environment.version
                )));
            }

            environments.push((path, environment));
        }
        Ok(environments)
    }

    /// Return the package that the environment reference at `path` provides.
    ///
    /// Returns `None` if the reference doesn't follow a known cache layout.
    fn reference_package(&self, path: &Path) -> Option<(PackageName, Version)> {
        // Wheels are referenced by `<bucket>/<kind>/<package>/<version>-<tags>`.
        if let Some(suffix) = path
            .strip_prefix(self.bucket(CacheBucket::Wheels))
            .ok()
            .and_then(|suffix| suffix.parent()?.file_name().zip(suffix.file_name()))
        {
            let (name, cache_key) = suffix;
            let (version, _) = cache_key.to_str()?.split_once('-')?;
            return Some((
                PackageName::from_str(name.to_str()?).ok()?,
                Version::from_str(version).ok()?,
            ));
        }

        // Source distributions are referenced by
        // `<bucket>/<kind>/<package>/<version>/<revision>/<version>-<tags>`.
        if let Some(suffix) = path
            .strip_prefix(self.bucket(CacheBucket::SourceDistributions))
            .ok()
            .and_then(|suffix| {
                let version = suffix.parent()?.parent()?.file_name()?;
                let name = suffix.parent()?.parent()?.parent()?.file_name()?;
                Some((name, version))
            })
        {
            let (name, version) = suffix;
            return Some((
                PackageName::from_str(name.to_str()?).ok()?,
                Version::from_str(version.to_str()?).ok()?,
            ));
        }

        None
    }

    /// Return the cache entries to remove along with the environment reference at `path`.
    fn unused_entries(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut entries = Vec::new();

        // The reference is nested in the revision directory, which holds the cache entry for the
        // source distribution.
        if path.starts_with(self.bucket(CacheBucket::SourceDistributions)) {
            if let Some(revision) = path.parent() {
                entries.push(revision.to_path_buf());
                entries.push(revision.with_extension("http"));
            }
            return Ok(entries);
        }

        // The metadata, lock, and wheel files share the prefix of the reference.
        entries.push(path.to_path_buf());
        let Some(directory) = path.parent() else {
            return Ok(entries);
        };
        let Some(prefix) = path.file_name().and_then(|name| name.to_str()) else {
            return Ok(entries);
        };
        for entry in fs_err::read_dir(directory)? {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name
                .strip_prefix(prefix)
                .is_some_and(|suffix| suffix.starts_with('.'))
            {
                entries.push(entry.path());
            }
        }

        Ok(entries)
    }
}

/// Return the packages installed into the `site-packages` directory at `site_packages`.
fn installed_packages(site_packages: &Path) -> io::Result<FxHashSet<(PackageName, Version)>> {
    let mut packages = FxHashSet::default();
    for entry in fs_err::read_dir(site_packages)? {
        let entry = entry?;
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dist-info"))
        {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            return Err(invalid_installed_package(&path));
        };
        let Some((name, version)) = stem.split_once('-') else {
            return Err(invalid_installed_package(&path));
        };
        let (Ok(name), Ok(version)) = (PackageName::from_str(name), Version::from_str(version))
        else {
            return Err(invalid_installed_package(&path));
        };

        packages.insert((name, version));
    }
    Ok(packages)
}

/// Return the error to use for an installed package that can't be resolved.
fn invalid_installed_package(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "Failed to determine the package installed as `{}`",
            path.display()
        ),
    )
}

/// Return the `site-packages` directories of the environment at `environment`.
fn discover_site_packages(environment: &Path) -> io::Result<Vec<PathBuf>> {
    let name = environment.file_name().and_then(|name| name.to_str());
    if matches!(name, Some("site-packages" | "dist-packages")) {
        return Ok(vec![environment.to_path_buf()]);
    }

    let mut site_packages = Vec::new();

    #[cfg(windows)]
    {
        let path = environment.join("Lib").join("site-packages");
        if path.is_dir() {
            site_packages.push(path);
        }
    }

    #[cfg(unix)]
    for library in uv_fs::directories(environment)? {
        if !library
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("lib"))
        {
            continue;
        }
        for interpreter in uv_fs::directories(&library)? {
            if !interpreter
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("python") || name.starts_with("pypy"))
            {
                continue;
            }
            for candidate in ["site-packages", "dist-packages"] {
                let path = interpreter.join(candidate);
                if path.is_dir() {
                    site_packages.push(path);
                }
            }
        }
    }

    if site_packages.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "Failed to find a `site-packages` directory in `{}`",
                environment.display()
            ),
        ));
    }

    Ok(site_packages)
}
