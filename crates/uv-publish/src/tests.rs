use crate::{build_request, form_metadata, Reporter};
use insta::{assert_debug_snapshot, assert_snapshot};
use itertools::Itertools;
use std::path::PathBuf;
use std::sync::Arc;
use url::Url;
use uv_client::BaseClientBuilder;
use uv_distribution_filename::DistFilename;

struct DummyReporter;

impl Reporter for DummyReporter {
    fn on_progress(&self, _name: &str, _id: usize) {}
    fn on_download_start(&self, _name: &str, _size: Option<u64>) -> usize {
        0
    }
    fn on_download_progress(&self, _id: usize, _inc: u64) {}
    fn on_download_complete(&self, _id: usize) {}
}

/// Snapshot the data we send for an upload request for a source distribution.
#[tokio::test]
async fn upload_request_source_dist() {
    let raw_filename = "tqdm-999.0.0.tar.gz";
    let file = PathBuf::from("../../scripts/links/").join(raw_filename);
    let filename = DistFilename::try_from_normalized_filename(raw_filename).unwrap();

    let form_metadata = form_metadata(&file, &filename).await.unwrap();

    let formatted_metadata = form_metadata
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .join("\n");
    assert_snapshot!(&formatted_metadata, @r###"
    :action: file_upload
    sha256_digest: 89fa05cffa7f457658373b85de302d24d0c205ceda2819a8739e324b75e9430b
    protocol_version: 1
    metadata_version: 2.3
    name: tqdm
    version: 999.0.0
    filetype: sdist
    pyversion: source
    description: # tqdm

    [![PyPI - Version](https://img.shields.io/pypi/v/tqdm.svg)](https://pypi.org/project/tqdm)
    [![PyPI - Python Version](https://img.shields.io/pypi/pyversions/tqdm.svg)](https://pypi.org/project/tqdm)

    -----

    **Table of Contents**

    - [Installation](#installation)
    - [License](#license)

    ## Installation

    ```console
    pip install tqdm
    ```

    ## License

    `tqdm` is distributed under the terms of the [MIT](https://spdx.org/licenses/MIT.html) license.

    description_content_type: text/markdown
    author_email: Charlie Marsh <charlie.r.marsh@gmail.com>
    requires_python: >=3.8
    classifiers: Development Status :: 4 - Beta
    classifiers: Programming Language :: Python
    classifiers: Programming Language :: Python :: 3.8
    classifiers: Programming Language :: Python :: 3.9
    classifiers: Programming Language :: Python :: 3.10
    classifiers: Programming Language :: Python :: 3.11
    classifiers: Programming Language :: Python :: 3.12
    classifiers: Programming Language :: Python :: Implementation :: CPython
    classifiers: Programming Language :: Python :: Implementation :: PyPy
    project_urls: Documentation, https://github.com/unknown/tqdm#readme
    project_urls: Issues, https://github.com/unknown/tqdm/issues
    project_urls: Source, https://github.com/unknown/tqdm
    "###);

    let (request, _) = build_request(
        &file,
        raw_filename,
        &filename,
        &Url::parse("https://example.org/upload").unwrap(),
        &BaseClientBuilder::new().build().client(),
        Some("ferris"),
        Some("F3RR!S"),
        &form_metadata,
        Arc::new(DummyReporter),
    )
    .await
    .unwrap();

    insta::with_settings!({
        filters => [("boundary=[0-9a-f-]+", "boundary=[...]")],
    }, {
        assert_debug_snapshot!(&request, @r###"
        RequestBuilder {
            inner: RequestBuilder {
                method: POST,
                url: Url {
                    scheme: "https",
                    cannot_be_a_base: false,
                    username: "",
                    password: None,
                    host: Some(
                        Domain(
                            "example.org",
                        ),
                    ),
                    port: None,
                    path: "/upload",
                    query: None,
                    fragment: None,
                },
                headers: {
                    "content-type": "multipart/form-data; boundary=[...]",
                    "accept": "application/json;q=0.9, text/plain;q=0.8, text/html;q=0.7",
                    "authorization": "Basic ZmVycmlzOkYzUlIhUw==",
                },
            },
            ..
        }
        "###);
    });
}

/// Snapshot the data we send for an upload request for a wheel.
#[tokio::test]
async fn upload_request_wheel() {
    let raw_filename =
        "tqdm-4.66.1-py3-none-manylinux_2_12_x86_64.manylinux2010_x86_64.musllinux_1_1_x86_64.whl";
    let file = PathBuf::from("../../scripts/links/").join(raw_filename);
    let filename = DistFilename::try_from_normalized_filename(raw_filename).unwrap();

    let form_metadata = form_metadata(&file, &filename).await.unwrap();

    let formatted_metadata = form_metadata
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .join("\n");
    assert_snapshot!(&formatted_metadata, @r###"
    :action: file_upload
    sha256_digest: 0d88ca657bc6b64995ca416e0c59c71af85cc10015d940fa446c42a8b485ee1c
    protocol_version: 1
    metadata_version: 2.1
    name: tqdm
    version: 4.66.1
    filetype: bdist_wheel
    pyversion: py3
    summary: Fast, Extensible Progress Meter
    description_content_type: text/x-rst
    maintainer_email: tqdm developers <devs@tqdm.ml>
    license: MPL-2.0 AND MIT
    keywords: progressbar,progressmeter,progress,bar,meter,rate,eta,console,terminal,time
    requires_python: >=3.7
    classifiers: Development Status :: 5 - Production/Stable
    classifiers: Environment :: Console
    classifiers: Environment :: MacOS X
    classifiers: Environment :: Other Environment
    classifiers: Environment :: Win32 (MS Windows)
    classifiers: Environment :: X11 Applications
    classifiers: Framework :: IPython
    classifiers: Framework :: Jupyter
    classifiers: Intended Audience :: Developers
    classifiers: Intended Audience :: Education
    classifiers: Intended Audience :: End Users/Desktop
    classifiers: Intended Audience :: Other Audience
    classifiers: Intended Audience :: System Administrators
    classifiers: License :: OSI Approved :: MIT License
    classifiers: License :: OSI Approved :: Mozilla Public License 2.0 (MPL 2.0)
    classifiers: Operating System :: MacOS
    classifiers: Operating System :: MacOS :: MacOS X
    classifiers: Operating System :: Microsoft
    classifiers: Operating System :: Microsoft :: MS-DOS
    classifiers: Operating System :: Microsoft :: Windows
    classifiers: Operating System :: POSIX
    classifiers: Operating System :: POSIX :: BSD
    classifiers: Operating System :: POSIX :: BSD :: FreeBSD
    classifiers: Operating System :: POSIX :: Linux
    classifiers: Operating System :: POSIX :: SunOS/Solaris
    classifiers: Operating System :: Unix
    classifiers: Programming Language :: Python
    classifiers: Programming Language :: Python :: 3
    classifiers: Programming Language :: Python :: 3.7
    classifiers: Programming Language :: Python :: 3.8
    classifiers: Programming Language :: Python :: 3.9
    classifiers: Programming Language :: Python :: 3.10
    classifiers: Programming Language :: Python :: 3.11
    classifiers: Programming Language :: Python :: 3 :: Only
    classifiers: Programming Language :: Python :: Implementation
    classifiers: Programming Language :: Python :: Implementation :: IronPython
    classifiers: Programming Language :: Python :: Implementation :: PyPy
    classifiers: Programming Language :: Unix Shell
    classifiers: Topic :: Desktop Environment
    classifiers: Topic :: Education :: Computer Aided Instruction (CAI)
    classifiers: Topic :: Education :: Testing
    classifiers: Topic :: Office/Business
    classifiers: Topic :: Other/Nonlisted Topic
    classifiers: Topic :: Software Development :: Build Tools
    classifiers: Topic :: Software Development :: Libraries
    classifiers: Topic :: Software Development :: Libraries :: Python Modules
    classifiers: Topic :: Software Development :: Pre-processors
    classifiers: Topic :: Software Development :: User Interfaces
    classifiers: Topic :: System :: Installation/Setup
    classifiers: Topic :: System :: Logging
    classifiers: Topic :: System :: Monitoring
    classifiers: Topic :: System :: Shells
    classifiers: Topic :: Terminals
    classifiers: Topic :: Utilities
    requires_dist: colorama ; platform_system == "Windows"
    requires_dist: pytest >=6 ; extra == 'dev'
    requires_dist: pytest-cov ; extra == 'dev'
    requires_dist: pytest-timeout ; extra == 'dev'
    requires_dist: pytest-xdist ; extra == 'dev'
    requires_dist: ipywidgets >=6 ; extra == 'notebook'
    requires_dist: slack-sdk ; extra == 'slack'
    requires_dist: requests ; extra == 'telegram'
    project_urls: homepage, https://tqdm.github.io
    project_urls: repository, https://github.com/tqdm/tqdm
    project_urls: changelog, https://tqdm.github.io/releases
    project_urls: wiki, https://github.com/tqdm/tqdm/wiki
    "###);

    let (request, _) = build_request(
        &file,
        raw_filename,
        &filename,
        &Url::parse("https://example.org/upload").unwrap(),
        &BaseClientBuilder::new().build().client(),
        Some("ferris"),
        Some("F3RR!S"),
        &form_metadata,
        Arc::new(DummyReporter),
    )
    .await
    .unwrap();

    insta::with_settings!({
        filters => [("boundary=[0-9a-f-]+", "boundary=[...]")],
    }, {
        assert_debug_snapshot!(&request, @r###"
        RequestBuilder {
            inner: RequestBuilder {
                method: POST,
                url: Url {
                    scheme: "https",
                    cannot_be_a_base: false,
                    username: "",
                    password: None,
                    host: Some(
                        Domain(
                            "example.org",
                        ),
                    ),
                    port: None,
                    path: "/upload",
                    query: None,
                    fragment: None,
                },
                headers: {
                    "content-type": "multipart/form-data; boundary=[...]",
                    "accept": "application/json;q=0.9, text/plain;q=0.8, text/html;q=0.7",
                    "authorization": "Basic ZmVycmlzOkYzUlIhUw==",
                },
            },
            ..
        }
        "###);
    });
}
