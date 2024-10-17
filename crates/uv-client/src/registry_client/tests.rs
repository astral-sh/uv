use std::str::FromStr;

use url::Url;

use uv_normalize::PackageName;
use uv_pypi_types::{JoinRelativeError, SimpleJson};

use crate::{html::SimpleHtml, SimpleMetadata, SimpleMetadatum};

#[test]
fn ignore_failing_files() {
    // 1.7.7 has an invalid requires-python field (double comma), 1.7.8 is valid
    let response = r#"
    {
        "files": [
        {
            "core-metadata": false,
            "data-dist-info-metadata": false,
            "filename": "pyflyby-1.7.7.tar.gz",
            "hashes": {
            "sha256": "0c4d953f405a7be1300b440dbdbc6917011a07d8401345a97e72cd410d5fb291"
            },
            "requires-python": ">=2.5, !=3.0.*, !=3.1.*, !=3.2.*, !=3.2.*, !=3.3.*, !=3.4.*,, !=3.5.*, !=3.6.*, <4",
            "size": 427200,
            "upload-time": "2022-05-19T09:14:36.591835Z",
            "url": "https://files.pythonhosted.org/packages/61/93/9fec62902d0b4fc2521333eba047bff4adbba41f1723a6382367f84ee522/pyflyby-1.7.7.tar.gz",
            "yanked": false
        },
        {
            "core-metadata": false,
            "data-dist-info-metadata": false,
            "filename": "pyflyby-1.7.8.tar.gz",
            "hashes": {
            "sha256": "1ee37474f6da8f98653dbcc208793f50b7ace1d9066f49e2707750a5ba5d53c6"
            },
            "requires-python": ">=2.5, !=3.0.*, !=3.1.*, !=3.2.*, !=3.2.*, !=3.3.*, !=3.4.*, !=3.5.*, !=3.6.*, <4",
            "size": 424460,
            "upload-time": "2022-08-04T10:42:02.190074Z",
            "url": "https://files.pythonhosted.org/packages/ad/39/17180d9806a1c50197bc63b25d0f1266f745fc3b23f11439fccb3d6baa50/pyflyby-1.7.8.tar.gz",
            "yanked": false
        }
        ]
    }
    "#;
    let data: SimpleJson = serde_json::from_str(response).unwrap();
    let base = Url::parse("https://pypi.org/simple/pyflyby/").unwrap();
    let simple_metadata = SimpleMetadata::from_files(
        data.files,
        &PackageName::from_str("pyflyby").unwrap(),
        &base,
    );
    let versions: Vec<String> = simple_metadata
        .iter()
        .map(|SimpleMetadatum { version, .. }| version.to_string())
        .collect();
    assert_eq!(versions, ["1.7.8".to_string()]);
}

/// Test for AWS Code Artifact registry
///
/// See: <https://github.com/astral-sh/uv/issues/1388>
#[test]
fn relative_urls_code_artifact() -> Result<(), JoinRelativeError> {
    let text = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Links for flask</title>
        </head>
        <body>
            <h1>Links for flask</h1>
            <a href="0.1/Flask-0.1.tar.gz#sha256=9da884457e910bf0847d396cb4b778ad9f3c3d17db1c5997cb861937bd284237"  data-gpg-sig="false" >Flask-0.1.tar.gz</a>
            <br/>
            <a href="0.10.1/Flask-0.10.1.tar.gz#sha256=4c83829ff83d408b5e1d4995472265411d2c414112298f2eb4b359d9e4563373"  data-gpg-sig="false" >Flask-0.10.1.tar.gz</a>
            <br/>
            <a href="3.0.1/flask-3.0.1.tar.gz#sha256=6489f51bb3666def6f314e15f19d50a1869a19ae0e8c9a3641ffe66c77d42403" data-requires-python="&gt;=3.8" data-gpg-sig="false" >flask-3.0.1.tar.gz</a>
            <br/>
        </body>
        </html>
    "#;

    // Note the lack of a trailing `/` here is important for coverage of url-join behavior
    let base = Url::parse("https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/flask")
        .unwrap();
    let SimpleHtml { base, files } = SimpleHtml::parse(text, &base).unwrap();

    // Test parsing of the file urls
    let urls = files
        .iter()
        .map(|file| uv_pypi_types::base_url_join_relative(base.as_url().as_str(), &file.url))
        .collect::<Result<Vec<_>, JoinRelativeError>>()?;
    let urls = urls.iter().map(reqwest::Url::as_str).collect::<Vec<_>>();
    insta::assert_debug_snapshot!(urls, @r###"
    [
        "https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/0.1/Flask-0.1.tar.gz#sha256=9da884457e910bf0847d396cb4b778ad9f3c3d17db1c5997cb861937bd284237",
        "https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/0.10.1/Flask-0.10.1.tar.gz#sha256=4c83829ff83d408b5e1d4995472265411d2c414112298f2eb4b359d9e4563373",
        "https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/3.0.1/flask-3.0.1.tar.gz#sha256=6489f51bb3666def6f314e15f19d50a1869a19ae0e8c9a3641ffe66c77d42403",
    ]
    "###);

    Ok(())
}
