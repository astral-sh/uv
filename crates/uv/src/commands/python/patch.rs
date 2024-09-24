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

struct PatchAndIndex {
    patch: u8,
    index: usize,
}

/// Update .python-version and .python-versions files to the latest patch version
pub(crate) async fn patch() -> Result<()> {
    let python_version_file = PythonVersionFile::discover(CWD.as_path(), false, false).await?;
    let python_versions_file = PythonVersionFile::discover(CWD.as_path(), false, true).await?;

    // Note that `python_versions_file` may resolve to a python-version file
    if let Some(python_versions_file) = &python_versions_file {
        let versions = python_versions_file.versions();
        let mut resulting_versions = python_versions_file.versions().cloned().collect_vec();

        // If there exist multiple of the same (major, minor) version, we only want to
        // update the one with the largest current patch version.
        //
        // We want to maintain the order of versions as read, so we keep an index associated
        // with each patch
        let mut versions_to_patch: BTreeMap<MajorAndMinorVersion, PatchAndIndex> = BTreeMap::new();

        for (index, version) in versions.enumerate() {
            if let PythonRequest::Version(VersionRequest::MajorMinorPatch(
                major,
                minor,
                current_patch,
            )) = version
            {
                versions_to_patch
                    .entry(MajorAndMinorVersion {
                        major: *major,
                        minor: *minor,
                    })
                    .and_modify(|patch_and_index| {
                        if patch_and_index.patch < *current_patch {
                            patch_and_index.patch = *current_patch;
                            patch_and_index.index = index;
                        }
                    })
                    .or_insert(PatchAndIndex {
                        patch: *current_patch,
                        index,
                    });
            }
        }

        let patched_versions: Vec<u8> = versions_to_patch
            .iter()
            .map(|(major_and_minor, patch_and_index)| {
                get_latest_patch_version(
                    major_and_minor.major,
                    major_and_minor.minor,
                    patch_and_index.patch,
                )
            })
            .collect_vec();

        versions_to_patch.into_iter().enumerate().for_each(
            |(i, (major_and_minor, patch_and_index))| {
                let new_patch = patched_versions[i];

                if new_patch != patch_and_index.patch {
                    println!(
                        "Bumped patch for version {}.{}.{} to {}",
                        major_and_minor.major,
                        major_and_minor.minor,
                        patch_and_index.patch,
                        new_patch
                    );

                    resulting_versions[patch_and_index.index] =
                        PythonRequest::Version(VersionRequest::MajorMinorPatch(
                            major_and_minor.major,
                            major_and_minor.minor,
                            new_patch,
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
        python_version_file == python_versions_file,
    ) {
        if let Some(PythonRequest::Version(VersionRequest::MajorMinorPatch(major, minor, patch))) =
            python_version_file.version()
        {
            let new_patch = get_latest_patch_version(*major, *minor, *patch);

            if new_patch != *patch {
                println!("Bumped patch for version {major}.{minor}.{patch} to {new_patch}",);
            }

            PythonVersionFile::new(python_version_file.path().to_path_buf())
                .with_versions(vec![PythonRequest::Version(
                    VersionRequest::MajorMinorPatch(*major, *minor, new_patch),
                )])
                .write()
                .await?;
        }
    }

    Ok(())
}

fn get_latest_patch_version(major: u8, minor: u8, patch: u8) -> u8 {
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

    let (base_version, cache) = (
        PythonRequest::Version(VersionRequest::MajorMinor(major, minor)),
        Cache::temp().unwrap(),
    );
    let system_installations = find_python_installations(
        &base_version,
        EnvironmentPreference::Any,
        PythonPreference::System,
        &cache,
    );

    for installation in system_installations.flatten().flatten() {
        max_patch = max_patch.max(installation.interpreter().python_patch());
    }

    max_patch
}
