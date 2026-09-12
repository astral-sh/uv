use std::collections::BTreeMap;
use std::fmt::Write as _;

use anyhow::Result;
use serde_json::json;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::Requirement;
use uv_test::packse::generate_wheel_with_files;

pub(super) struct ToolPackage {
    pub name: &'static str,
    pub version: &'static str,
    pub requires: &'static [&'static str],
    pub scripts: &'static [&'static str],
}

/// Serve synthetic tool wheels with dependency metadata and executable stubs.
///
/// Each index is independent, so tests can switch indexes and reuse the index saved in a receipt.
pub(super) async fn tool_index(packages: &[ToolPackage]) -> Result<MockServer> {
    let server = MockServer::start().await;
    let mut projects = BTreeMap::<PackageName, Vec<_>>::new();
    for package in packages {
        let name: PackageName = package.name.parse()?;
        let version: Version = package.version.parse()?;
        let requires = package
            .requires
            .iter()
            .map(|requirement| requirement.parse::<Requirement>())
            .collect::<Result<Vec<_>, _>>()?;
        let module = name.as_dist_info_name();
        let entrypoints_path = format!("{module}-{version}.dist-info/entry_points.txt");
        let module_path = format!("{module}/cli.py");
        let mut entrypoints = String::from("[console_scripts]\n");
        for script in package.scripts {
            writeln!(entrypoints, "{script} = {module}.cli:main")?;
        }
        let (filename, bytes) = generate_wheel_with_files(
            &name,
            &version,
            &requires,
            &BTreeMap::new(),
            None,
            "py3-none-any",
            &[
                (&entrypoints_path, &entrypoints),
                (&module_path, "def main():\n    print('tool fixture')\n"),
            ],
        );
        projects.entry(name).or_default().push(json!({
            "filename": filename,
            "url": format!("{}/files/{filename}", server.uri()),
            "hashes": {},
            "upload-time": "2024-03-24T00:00:00Z",
        }));
        Mock::given(path(format!("/files/{filename}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
            .mount(&server)
            .await;
    }
    for (name, files) in projects {
        Mock::given(path(format!("/simple/{name}/")))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                json!({"meta": {"api-version": "1.1"}, "name": name, "files": files}).to_string(),
                "application/vnd.pypi.simple.v1+json",
            ))
            .mount(&server)
            .await;
    }
    Ok(server)
}
