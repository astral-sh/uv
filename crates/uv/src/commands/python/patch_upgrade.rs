use anstream::println;
use anyhow::Result;
use std::{cmp::Eq, collections::HashMap, path::Path};
use tracing::debug;

use uv_cache::Cache;
use uv_python::{
    downloads::ManagedPythonDownload, find_python_installations, EnvironmentPreference,
    PythonPreference, PythonRequest, PythonVersionFile, VersionRequest,
};

#[derive(Debug, PartialEq, Eq, Hash)]
struct MajorAndMinorVersion {
    major: u8,
    minor: u8,
}

#[derive(Debug)]
struct MaxPatch(u8);

/// Update .python-version and .python-versions files to the latest patch version
///
/// Handling .python-version files is trivial — we simply read the first line of the file,
/// get the latest patch (if one exists), and log the output.
///
/// Handling .python-versions files is trickier, as there might be two listed versions of
/// Python with the same major/minor version (in uv's .python-versions, there's both 3.8.12/3.8.18).
/// Since we only want to bump the one with the largest patch, we keep an in memory map of
/// (major, minor) -> (max patch).
pub(crate) async fn patch(project_dir: &Path, cache: &Cache) -> Result<()> {
    let python_version_file = PythonVersionFile::discover(project_dir, false, false).await?;
    let python_versions_file = PythonVersionFile::discover(project_dir, false, true).await?;

    // Note that python_versions_file might be a .python-version file (the `discover` function
    // associated with the PythonVersionFile struct used above falls back to .python-version
    // if a .python-versions file cannot be found).
    if let Some(python_versions_file) = &python_versions_file {
        let versions: Vec<PythonRequest> = python_versions_file.versions().cloned().collect();

        // Store the largest patch associated with each (major, minor) pair
        let mut versions_to_patch: HashMap<MajorAndMinorVersion, MaxPatch> = HashMap::new();
        for version in &versions {
            if let PythonRequest::Version(VersionRequest::MajorMinorPatch(
                major,
                minor,
                current_patch,
                _,
            )) = version
            {
                versions_to_patch
                    .entry(MajorAndMinorVersion {
                        major: *major,
                        minor: *minor,
                    })
                    .and_modify(|max_patch| {
                        if max_patch.0 < *current_patch {
                            max_patch.0 = *current_patch;
                        }
                    })
                    .or_insert(MaxPatch(*current_patch));
            }
        }

        // For each version, check if this version should be patched. If so,
        // find the latest patch and update the version.
        let mut resulting_versions = vec![];
        for version in versions {
            let version = match version {
                PythonRequest::Version(VersionRequest::MajorMinorPatch(
                    major,
                    minor,
                    current_patch,
                    free_thread,
                )) => {
                    let major_and_minor = MajorAndMinorVersion { major, minor };

                    match versions_to_patch.get(&major_and_minor) {
                        // If this version has the largest patch of all others which share
                        // the same (major, minor) pair, get the latest patch.
                        Some(max_patch) if max_patch.0 == current_patch => {
                            let latest_patch = get_latest_patch_version(
                                major,
                                minor,
                                max_patch.0,
                                free_thread,
                                cache,
                            );

                            if latest_patch == max_patch.0 {
                                // The patch declared in the original list of versions was the
                                // largest available.
                                version
                            } else {
                                println!(
                                    "Bumped patch for version {}.{}.{} to {}",
                                    major, minor, max_patch.0, latest_patch
                                );

                                PythonRequest::Version(VersionRequest::MajorMinorPatch(
                                    major,
                                    minor,
                                    latest_patch,
                                    free_thread,
                                ))
                            }
                        }
                        _ => {
                            debug!(
                                "Version {major}.{minor}.x should've been present in `versions_to_patch`, but wasn't."
                            );
                            version
                        }
                    }
                }
                // This version wasn't declared as a combination of major/minor/patch
                _ => version,
            };

            resulting_versions.push(version);
        }

        PythonVersionFile::new(python_versions_file.path().to_path_buf())
            .with_versions(resulting_versions)
            .write()
            .await?;
    }

    if let (Some(python_version_file), false) = (
        &python_version_file,
        // This ensures we're not doing duplicate work in the case
        // `python_versions_file` resolved to a .python-version file
        python_version_file == python_versions_file,
    ) {
        if let Some(PythonRequest::Version(VersionRequest::MajorMinorPatch(
            major,
            minor,
            patch,
            free_thread,
        ))) = python_version_file.version()
        {
            let new_patch = get_latest_patch_version(*major, *minor, *patch, *free_thread, cache);

            if new_patch != *patch {
                println!("Bumped patch for version {major}.{minor}.{patch} to {new_patch}",);
            }

            PythonVersionFile::new(python_version_file.path().to_path_buf())
                .with_versions(vec![PythonRequest::Version(
                    VersionRequest::MajorMinorPatch(*major, *minor, new_patch, *free_thread),
                )])
                .write()
                .await?;
        }
    }

    Ok(())
}

/// To get the latest patch version, we first check all versions of Python
/// downloaded by uv. If none match the given major and minor, we check against
/// the system installations.
fn get_latest_patch_version(
    major: u8,
    minor: u8,
    patch: u8,
    free_thread: bool,
    cache: &Cache,
) -> u8 {
    let mut max_patch = patch;
    for download in ManagedPythonDownload::iter_all() {
        if !(download.python_version().major() == major
            && download.python_version().minor() == minor)
        {
            continue;
        }

        if let Some(downloaded_patch) = download.python_version().patch() {
            max_patch = max_patch.max(downloaded_patch);
        }
    }

    if max_patch != patch {
        return max_patch;
    }

    let base_version =
        PythonRequest::Version(VersionRequest::MajorMinor(major, minor, free_thread));
    let system_installations = find_python_installations(
        &base_version,
        EnvironmentPreference::OnlySystem,
        PythonPreference::System,
        cache,
    );

    for installation in system_installations.flatten().flatten() {
        max_patch = max_patch.max(installation.interpreter().python_patch());
    }

    max_patch
}
