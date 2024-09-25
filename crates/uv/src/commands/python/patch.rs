use anstream::println;
use anyhow::Result;
use itertools::Itertools;
use std::{cmp::Eq, collections::BTreeMap};

use uv_cache::Cache;
use uv_fs::CWD;
use uv_python::{
    downloads::ManagedPythonDownload, find_python_installations, EnvironmentPreference,
    PythonPreference, PythonRequest, PythonVersionFile, VersionRequest,
};

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct MajorAndMinorVersion {
    major: u8,
    minor: u8,
}

struct MaxPatchAndIndex {
    max_patch: u8,
    index: usize,
    free_thread: bool,
}

/// Update .python-version and .python-versions files to the latest patch version
///
/// Handling .python-version files is trivial — we simply read the first line of the file,
/// get the latest patch (if one exists), and log the output.
///
/// Handling .python-versions files is trickier, as there might be two listed versions of
/// Python with the same major/minor version (in uv's .python-versions, there's both 3.8.12/3.8.18).
///
/// We only want to bump the one with the largest patch, so we keep an in memory map of
/// (major, minor) -> (max patch, index). The index is kept so the original ordering of
/// the versions is not lost after we bump the max patch.
pub(crate) async fn patch(cache: &Cache) -> Result<()> {
    let python_version_file = PythonVersionFile::discover(CWD.as_path(), false, false).await?;
    let python_versions_file = PythonVersionFile::discover(CWD.as_path(), false, true).await?;

    // Note that python_versions_file might be a .python-version file (the `discover` function
    // associated with the PythonVersionFile struct used above falls back to .python-version
    // if a .python-versions file cannot be found).
    if let Some(python_versions_file) = &python_versions_file {
        let versions = python_versions_file.versions();
        let mut resulting_versions = python_versions_file.versions().cloned().collect_vec();

        // Use a BTreeMap so the logged output for patches is ordered
        let mut versions_to_patch: BTreeMap<MajorAndMinorVersion, MaxPatchAndIndex> =
            BTreeMap::new();

        for (index, version) in versions.enumerate() {
            if let PythonRequest::Version(VersionRequest::MajorMinorPatch(
                major,
                minor,
                current_patch,
                free_thread,
            )) = version
            {
                versions_to_patch
                    .entry(MajorAndMinorVersion {
                        major: *major,
                        minor: *minor,
                    })
                    .and_modify(|patch_and_index| {
                        // Store the largest patch associated with each (major, minor) pair,
                        // along with its index in the original list of versions
                        if patch_and_index.max_patch < *current_patch {
                            patch_and_index.max_patch = *current_patch;
                            patch_and_index.index = index;
                        }
                    })
                    .or_insert(MaxPatchAndIndex {
                        max_patch: *current_patch,
                        index,
                        free_thread: *free_thread,
                    });
            }
        }

        // Get the latest patch for each given major/minor versions
        let patched_versions: Vec<u8> = versions_to_patch
            .iter()
            .map(|(major_and_minor, patch_and_index)| {
                get_latest_patch_version(
                    major_and_minor.major,
                    major_and_minor.minor,
                    patch_and_index.max_patch,
                    patch_and_index.free_thread,
                    cache,
                )
            })
            .collect_vec();

        // If a larger patch was found, update the result vector
        versions_to_patch.into_iter().enumerate().for_each(
            |(i, (major_and_minor, patch_and_index))| {
                let new_patch = patched_versions[i];

                if new_patch != patch_and_index.max_patch {
                    println!(
                        "Bumped patch for version {}.{}.{} to {}",
                        major_and_minor.major,
                        major_and_minor.minor,
                        patch_and_index.max_patch,
                        new_patch
                    );

                    resulting_versions[patch_and_index.index] =
                        PythonRequest::Version(VersionRequest::MajorMinorPatch(
                            major_and_minor.major,
                            major_and_minor.minor,
                            new_patch,
                            patch_and_index.free_thread,
                        ));
                }
            },
        );

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
