//! Tests for direct URL hashes discovered from package metadata.

use std::collections::BTreeMap;

use anyhow::Result;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use fs_err as fs;
use fs_err::File;
use indoc::indoc;
use predicates::prelude::predicate;
use sha2::{Digest, Sha256};
use url::Url;
use wiremock::MockServer;

use uv_test::archive::write_tar_gz;
use uv_test::packse::{generate_wheel, mount_mismatched_distribution};
use uv_test::{TestContext as UvTestContext, uv_snapshot};

/// A test context for exercising hashes discovered from forged wheel metadata.
///
/// The hosted parent distribution serves forged wheel bytes to range requests and authentic wheel
/// bytes to full-file requests. This models metadata that introduces a direct URL dependency even
/// though the trusted parent wheel has no dependencies.
struct DirectUrlHashTestContext {
    /// The shared filesystem, cache, environment, and virtual environment for the `uv` invocation.
    inner: UvTestContext,
    /// A marker written if the direct URL dependency's build backend executes.
    backend_marker: ChildPath,
    /// The SHA-256 digest of the direct URL source distribution.
    source_hash: String,
    /// The file URL of the direct URL source distribution.
    source_url: Url,
    /// The URL of the authentic parent wheel.
    wheel_url: String,
    /// The SHA-256 digest of the authentic parent wheel.
    wheel_hash: String,
    /// The server hosting the mismatched parent distribution, retained for the test's lifetime.
    _server: MockServer,
}

impl DirectUrlHashTestContext {
    /// Create a test context with forged ranged metadata and an authentic full-file download.
    async fn new() -> Result<Self> {
        let inner = uv_test::test_context!("3.12");

        let source = inner.temp_dir.child("ok-1.0.0.tar.gz");
        let backend_marker = inner.temp_dir.child("backend-marker");
        let child_wheel = inner
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl");
        let inner = inner
            .with_env("WHEEL_METADATA_MARKER", backend_marker.path())
            .with_env("WHEEL_METADATA_CHILD_WHEEL", child_wheel);
        write_tar_gz(
            File::create(source.path())?,
            &[
                (
                    "ok-1.0.0/pyproject.toml",
                    indoc! {r#"
                        [build-system]
                        requires = []
                        build-backend = "backend"
                        backend-path = ["."]
                    "#},
                ),
                (
                    "ok-1.0.0/backend.py",
                    indoc! {r#"
                        import os
                        import shutil
                        from pathlib import Path

                        Path(os.environ["WHEEL_METADATA_MARKER"]).write_text("executed")

                        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
                            wheel = Path(os.environ["WHEEL_METADATA_CHILD_WHEEL"])
                            shutil.copyfile(wheel, Path(wheel_directory) / wheel.name)
                            return wheel.name
                    "#},
                ),
            ],
        )?;
        let source_hash = hex::encode(Sha256::digest(fs::read(source.path())?));
        let inner = inner.with_filter((source_hash.clone(), "[SOURCE_HASH]"));
        let source_url =
            Url::from_file_path(source.path()).expect("source path is an absolute path");
        let dependency = format!("ok @ {source_url}#sha256={source_hash}").parse()?;

        let name = "metadata-parent".parse()?;
        let version = "1.0.0".parse()?;
        let (wheel_filename, authentic_wheel) =
            generate_wheel(&name, &version, &[], &BTreeMap::new(), None, "py3-none-any");
        let (_, forged_wheel) = generate_wheel(
            &name,
            &version,
            &[dependency],
            &BTreeMap::new(),
            None,
            "py3-none-any",
        );
        let wheel_hash = hex::encode(Sha256::digest(&authentic_wheel));
        let server = MockServer::start().await;
        let wheel_path = format!("/files/{wheel_filename}");
        let wheel_url = format!("{}{wheel_path}", server.uri());

        mount_mismatched_distribution(
            &server,
            &wheel_path,
            &wheel_filename,
            forged_wheel,
            authentic_wheel,
        )
        .await;

        Ok(Self {
            inner,
            backend_marker,
            source_hash,
            source_url,
            wheel_url,
            wheel_hash,
            _server: server,
        })
    }

    fn filters(&self) -> Vec<(&str, &str)> {
        self.inner.filters()
    }

    /// Write the trusted parent wheel and its hash to a requirements file.
    fn write_parent_requirement(&self) -> Result<ChildPath> {
        let requirements_txt = self.inner.temp_dir.child("requirements.txt");
        requirements_txt.write_str(&format!(
            "metadata-parent @ {} --hash=sha256:{}\n",
            self.wheel_url, self.wheel_hash
        ))?;
        Ok(requirements_txt)
    }

    /// Write the direct URL dependency and its hash to a requirements file.
    fn write_child_requirement(&self) -> Result<ChildPath> {
        let child_txt = self.inner.temp_dir.child("child.txt");
        child_txt.write_str(&format!(
            "ok @ {}#sha256={}\n",
            self.source_url, self.source_hash
        ))?;
        Ok(child_txt)
    }

    /// Assert that the direct URL dependency's backend ran and both packages were installed.
    fn assert_backend_ran(&self) {
        self.backend_marker.assert(predicate::path::is_file());
        self.inner.assert_installed("metadata_parent", "1.0.0");
        self.inner.assert_installed("ok", "1.0.0");
    }

    /// Assert that the direct URL dependency's backend did not run or install either package.
    fn assert_backend_did_not_run(&self) {
        self.backend_marker.assert(predicate::path::missing());
        self.inner.assert_not_installed("metadata_parent");
        self.inner.assert_not_installed("ok");
    }
}

/// A direct URL hash discovered only in wheel metadata cannot authorize the dependency.
#[tokio::test]
async fn require_hashes_rejects_direct_url_hash_discovered_in_wheel_metadata() -> Result<()> {
    let context = DirectUrlHashTestContext::new().await?;
    let requirements_txt = context.write_parent_requirement()?;

    uv_snapshot!(context.filters(), context.inner.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--no-index")
        .arg("--require-hashes"), @"
    exit_code: 1 (failure)
    ----- stderr -----
      × Failed to build `ok @ file://[TEMP_DIR]/ok-1.0.0.tar.gz#sha256=[SOURCE_HASH]`
      ╰─▶ Hash-checking is enabled, but no hashes were provided or computed for: `ok @ file://[TEMP_DIR]/ok-1.0.0.tar.gz#sha256=[SOURCE_HASH]`
    ");

    context.assert_backend_did_not_run();

    Ok(())
}

/// An explicit requirement can authorize a direct URL hash also present in wheel metadata.
#[tokio::test]
async fn require_hashes_accepts_direct_url_hash_from_explicit_requirement() -> Result<()> {
    let context = DirectUrlHashTestContext::new().await?;
    let requirements_txt = context.write_parent_requirement()?;
    let child_txt = context.write_child_requirement()?;

    uv_snapshot!(context.filters(), context.inner.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--no-index")
        .arg("--require-hashes")
        .arg("--requirement")
        .arg(child_txt.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + metadata-parent==1.0.0 (from http://[LOCALHOST]/files/metadata_parent-1.0.0-py3-none-any.whl)
     + ok==1.0.0 (from file://[TEMP_DIR]/ok-1.0.0.tar.gz#sha256=[SOURCE_HASH])
    ");

    context.assert_backend_ran();

    Ok(())
}

/// A constraint can authorize a direct URL hash without adding a root requirement.
#[tokio::test]
async fn require_hashes_accepts_direct_url_hash_from_constraint() -> Result<()> {
    let context = DirectUrlHashTestContext::new().await?;
    let requirements_txt = context.write_parent_requirement()?;
    let child_txt = context.write_child_requirement()?;

    uv_snapshot!(context.filters(), context.inner.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--no-index")
        .arg("--require-hashes")
        .arg("--constraint")
        .arg(child_txt.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + metadata-parent==1.0.0 (from http://[LOCALHOST]/files/metadata_parent-1.0.0-py3-none-any.whl)
     + ok==1.0.0 (from file://[TEMP_DIR]/ok-1.0.0.tar.gz#sha256=[SOURCE_HASH])
    ");

    context.assert_backend_ran();

    Ok(())
}

/// Optional verification permits a direct URL hash discovered in wheel metadata.
#[tokio::test]
async fn verify_hashes_accepts_direct_url_hash_discovered_in_wheel_metadata() -> Result<()> {
    let context = DirectUrlHashTestContext::new().await?;
    let requirements_txt = context.write_parent_requirement()?;

    uv_snapshot!(context.filters(), context.inner.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--no-index")
        .arg("--verify-hashes"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + metadata-parent==1.0.0 (from http://[LOCALHOST]/files/metadata_parent-1.0.0-py3-none-any.whl)
     + ok==1.0.0 (from file://[TEMP_DIR]/ok-1.0.0.tar.gz#sha256=[SOURCE_HASH])
    ");

    context.assert_backend_ran();

    Ok(())
}
