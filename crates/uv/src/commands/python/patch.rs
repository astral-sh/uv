use futures::future::join_all;
use std::collections::HashSet;
use uv_fs::CWD;
use uv_python::{PythonRequest, PythonVersionFile, VersionRequest};

/// Update .python-version and .python-versions files to the latest patch version
pub(crate) async fn patch() -> anyhow::Result<()> {
    let python_version_file = PythonVersionFile::discover(CWD.as_path(), false, false).await?;
    let python_versions_file = PythonVersionFile::discover(CWD.as_path(), false, true).await?;

    if let Some(python_versions_file) = &python_versions_file {
        let versions = python_versions_file.versions();
        let mut seen_major_minor_versions = HashSet::new();

        let mut versions_to_ignore: Vec<PythonRequest> = vec![];
        let mut versions_to_patch: Vec<(u8, u8, u8)> = vec![];

        for version in versions {
            if let PythonRequest::Version(VersionRequest::MajorMinorPatch(major, minor, patch)) =
                version
            {
                if seen_major_minor_versions.contains(&(major, minor)) {
                    versions_to_ignore.push(version.clone());
                    continue;
                }

                seen_major_minor_versions.insert((major, minor));
                versions_to_patch.push((*major, *minor, *patch));
            } else {
                versions_to_ignore.push(version.clone());
            }
        }

        let mut patched_versions: Vec<PythonRequest> = join_all(
            versions_to_patch
                .into_iter()
                .map(|(major, minor, patch)| get_latest_patch_version(major, minor, patch)),
        )
        .await;

        PythonVersionFile::new(python_versions_file.path().to_path_buf())
            .with_versions({
                patched_versions.append(&mut versions_to_ignore);
                patched_versions
            })
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
            let new_version = get_latest_patch_version(*major, *minor, *patch).await;
            PythonVersionFile::new(python_version_file.path().to_path_buf())
                .with_versions(vec![new_version])
                .write()
                .await?;
        }
    }

    Ok(())
}

async fn get_latest_patch_version(major: u8, minor: u8, _patch: u8) -> PythonRequest {
    PythonRequest::Version(VersionRequest::MajorMinorPatch(major, minor, _patch + 1))
}
