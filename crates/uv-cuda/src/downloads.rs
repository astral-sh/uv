use std::io;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use fs_err as fs;
use futures_util::StreamExt;
use serde::Deserialize;
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};
use uv_client::BaseClientBuilder;
use uv_platform_tags::{Arch, Os, Platform};

use crate::managed::ManagedCudaInstallation;
use crate::version::CudaVersion;

pub trait Reporter: Send + Sync {
    fn on_download_start(&self, name: &str, size: Option<u64>) -> usize;
    fn on_download_progress(&self, id: usize, bytes: u64);
    fn on_download_complete(&self, id: usize);
    fn on_extract_start(&self, name: &str) -> usize;
    fn on_extract_complete(&self, id: usize);
}

#[derive(Debug, Deserialize)]
struct RedistribJson {
    nvidia_driver: NvidiaDriver,
}

#[derive(Debug, Deserialize)]
struct NvidiaDriver {
    version: String,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error("Failed to download CUDA: {0}")]
    Download(String),

    #[error("Failed to extract CUDA: {0}")]
    Extract(String),

    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),

    #[error("Invalid CUDA version: {0}")]
    InvalidVersion(String),

    #[error("Failed to serialize: {0}")]
    Serialize(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct CudaDownloadRequest {
    pub version: CudaVersion,
    pub platform: CudaPlatformRequest,
}

#[derive(Debug, Clone)]
pub struct CudaPlatformRequest {
    pub platform: Platform,
}

impl CudaPlatformRequest {
    pub fn from_env() -> Result<Self, Error> {
        let arch = if cfg!(target_arch = "x86_64") {
            Arch::X86_64
        } else if cfg!(target_arch = "aarch64") {
            Arch::Aarch64
        } else {
            return Err(Error::UnsupportedPlatform(
                "Unsupported architecture".to_string(),
            ));
        };

        let os = if cfg!(target_os = "linux") {
            Os::Manylinux {
                major: 2,
                minor: 28,
            }
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else {
            return Err(Error::UnsupportedPlatform(
                "Unsupported operating system".to_string(),
            ));
        };

        let platform = Platform::new(os, arch);

        // CUDA is only officially supported on Linux and Windows
        match platform.os() {
            Os::Manylinux { .. } => Ok(Self { platform }),
            Os::Windows => Ok(Self { platform }),
            other => Err(Error::UnsupportedPlatform(format!(
                "CUDA is not supported on {other}"
            ))),
        }
    }
}

impl CudaDownloadRequest {
    pub fn new(version: CudaVersion, platform: CudaPlatformRequest) -> Self {
        Self { version, platform }
    }

    /// generate the download URL for the CUDA .run installer.
    pub async fn url(&self, client: &uv_client::BaseClient) -> Result<String, Error> {
        let driver_version = self.driver_version(client).await?;

        let url = match self.platform.platform.os() {
            Os::Manylinux { .. } => {
                format!(
                    "https://developer.download.nvidia.com/compute/cuda/{}/local_installers/cuda_{}_{}_linux.run",
                    self.version, self.version, driver_version
                )
            }
            // TODO(alpin): test if this works for windows
            Os::Windows => {
                format!(
                    "https://developer.download.nvidia.com/compute/cuda/{}/local_installers/cuda_{}_{}_windows.exe",
                    self.version, self.version, driver_version
                )
            }
            other => {
                return Err(Error::UnsupportedPlatform(format!(
                    "CUDA is not supported on {other}"
                )));
            }
        };

        debug!("Constructed CUDA download URL: {}", url);
        Ok(url)
    }

    /// fetch the driver version from NVIDIA's redistrib JSON file.
    async fn fetch_driver_version(&self, client: &uv_client::BaseClient) -> Result<String, Error> {
        let redistrib_url = format!(
            "https://developer.download.nvidia.com/compute/cuda/redist/redistrib_{}.json",
            self.version
        );

        debug!("Fetching driver version from: {}", redistrib_url);

        let response = client
            .for_host(
                &uv_redacted::DisplaySafeUrl::parse(&redistrib_url)
                    .map_err(|e| Error::Download(format!("Invalid redistrib URL: {}", e)))?,
            )
            .get(&redistrib_url)
            .send()
            .await
            .map_err(|e| Error::Download(format!("Failed to fetch redistrib JSON: {}", e)))?;

        if !response.status().is_success() {
            return Err(Error::Download(format!(
                "Failed to fetch redistrib JSON: HTTP {}",
                response.status()
            )));
        }

        let json_text = response
            .text()
            .await
            .map_err(|e| Error::Download(format!("Failed to read redistrib JSON: {}", e)))?;

        let redistrib: RedistribJson = serde_json::from_str(&json_text)
            .map_err(|e| Error::Download(format!("Failed to parse redistrib JSON: {}", e)))?;

        Ok(redistrib.nvidia_driver.version)
    }

    /// get the appropriate driver version for this CUDA version.
    /// first tries to fetch from NVIDIA's redistrib JSON, falls back to hardcoded mapping.
    async fn driver_version(&self, client: &uv_client::BaseClient) -> Result<String, Error> {
        // try to fetch from redistrib JSON first
        match self.fetch_driver_version(client).await {
            Ok(version) => {
                debug!("Found driver version {} for CUDA {}", version, self.version);
                Ok(version)
            }
            Err(e) => {
                debug!("Failed to fetch driver version from redistrib JSON: {}", e);
                // fallback to hardcoded mapping -- this should never happen
                let fallback = match (self.version.major(), self.version.minor()) {
                    // CUDA 12.8 and 12.9 require >= 570.26
                    (12, 8) | (12, 9) => "570.26",
                    // CUDA 12.0-12.6 require >= 525.60.13
                    (12, 0)
                    | (12, 1)
                    | (12, 2)
                    | (12, 3)
                    | (12, 4)
                    | (12, 5)
                    | (12, 6) => "525.60.13",
                    // CUDA 11.x versions
                    // TODO: verify these versions
                    (11, 8) => "520.61.05",
                    (11, 7) => "515.43.04",
                    (11, 6) => "510.47.03",
                    (11, 5) => "495.29.05",
                    (11, 4) => "470.57.02",
                    (11, 3) => "465.19.01",
                    (11, 2) => "460.32.03",
                    (11, 1) => "455.32.00",
                    (11, 0) => "450.80.02",
                    // default fallback
                    _ => "525.60.13",
                };
                Ok(fallback.to_string())
            }
        }
    }

    pub async fn install(
        &self,
        client_builder: &BaseClientBuilder<'_>,
        installations_dir: &Path,
        scratch_dir: &Path,
        reporter: Option<&dyn Reporter>,
    ) -> Result<ManagedCudaInstallation, Error> {
        let client = client_builder.build();
        let url = self.url(&client).await?;
        info!("Downloading CUDA {} from {}", self.version, url);

        let display_url = uv_redacted::DisplaySafeUrl::parse(&url)
            .map_err(|e| Error::Download(format!("Invalid URL: {}", e)))?;
        let response = client
            .for_host(&display_url)
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::Download(e.to_string()))?;

        if !response.status().is_success() {
            return Err(Error::Download(format!(
                "Failed to download CUDA: HTTP {}",
                response.status()
            )));
        }

        let content_length = response.content_length();
        let download_id = if let Some(reporter) = reporter {
            reporter.on_download_start(&format!("CUDA {}", self.version), content_length)
        } else {
            0
        };

        let temp_file = scratch_dir.join(format!("cuda_{}.run", self.version));

        if let Some(reporter) = reporter {
            let mut stream = response.bytes_stream();
            let mut file = fs_err::tokio::File::create(&temp_file).await?;

            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(|e| Error::Download(e.to_string()))?;
                let chunk_len = chunk.len();

                file.write_all(&chunk)
                    .await
                    .map_err(|e| Error::Download(e.to_string()))?;

                reporter.on_download_progress(download_id, chunk_len as u64);
            }

            file.flush().await?;
            reporter.on_download_complete(download_id);
        } else {
            let bytes = response
                .bytes()
                .await
                .map_err(|e| Error::Download(e.to_string()))?;

            fs::write(&temp_file, bytes)?;
        }

        debug!("Downloaded CUDA to {}", temp_file.display());

        // make the file executable (Linux only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&temp_file)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&temp_file, perms)?;
        }

        let extract_dir = scratch_dir.join(format!("cuda_{}_extract", self.version));
        let extract_id = if let Some(reporter) = reporter {
            reporter.on_extract_start(&format!("CUDA {}", self.version))
        } else {
            0
        };

        self.extract_run_file(&temp_file, &extract_dir)?;

        if let Some(reporter) = reporter {
            reporter.on_extract_complete(extract_id);
        }

        let install_dir = installations_dir.join(format!("cuda-{}", self.version));
        let driver_version = self.driver_version(&client).await?;
        self.install_extracted(&extract_dir, &install_dir, &driver_version)?;

        // cleanups
        let _ = fs::remove_file(&temp_file);
        let _ = fs::remove_dir_all(&extract_dir);

        Ok(ManagedCudaInstallation::new(
            install_dir,
            self.version.clone(),
            Some(url.leak()),
            None,  // TODO(alpin): verify SHA256
        ))
    }

    /// reference: https://gitlab.archlinux.org/archlinux/packaging/packages/cuda/-/blob/main/PKGBUILD
    fn extract_run_file(&self, run_file: &Path, extract_dir: &Path) -> Result<(), Error> {
        fs::create_dir_all(extract_dir)?;

        info!("Extracting CUDA .run file to {}", extract_dir.display());

        let output = Command::new("sh")
            .arg(run_file)
            .arg("--target")
            .arg(extract_dir)
            .arg("--noexec")
            .output()
            .map_err(|e| Error::Extract(format!("Failed to run extraction command: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Extract(format!("Extraction failed: {}", stderr)));
        }

        debug!("Successfully extracted CUDA to {}", extract_dir.display());
        Ok(())
    }

    // reference: https://gitlab.archlinux.org/archlinux/packaging/packages/cuda/-/blob/main/PKGBUILD
    fn install_extracted(
        &self,
        extract_dir: &Path,
        install_dir: &Path,
        driver_version: &str,
    ) -> Result<(), Error> {
        info!("Installing CUDA to {}", install_dir.display());

        if install_dir.exists() {
            fs::remove_dir_all(install_dir)?;
        }

        fs::create_dir_all(install_dir)?;

        let builds_dir = extract_dir.join("builds");
        if !builds_dir.exists() {
            return Err(Error::Extract(
                "Could not find builds directory in extracted CUDA".to_string(),
            ));
        }

        self.copy_cuda_components(&builds_dir, install_dir)?;

        self.create_version_file(install_dir, driver_version)?;

        info!(
            "Successfully installed CUDA {} to {}",
            self.version,
            install_dir.display()
        );
        Ok(())
    }

    fn copy_cuda_components(&self, builds_dir: &Path, install_dir: &Path) -> Result<(), Error> {
        for entry in fs::read_dir(builds_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                self.copy_component_contents(&path, install_dir)?;
            }
        }

        // clean up any broken symlinks or problematic nested directories
        self.cleanup_installation(install_dir)?;

        Ok(())
    }

    fn copy_component_contents(
        &self,
        component_dir: &Path,
        install_dir: &Path,
    ) -> Result<(), Error> {
        for entry in fs::read_dir(component_dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name();

            if let Some(name) = file_name.to_str() {
                if name.starts_with('.') || name == "EULA.txt" || name == "version.json" {
                    continue;
                }
            }

            let dest = install_dir.join(&file_name);

            if path.is_dir() {
                // recursively copy directories, merging if they already exist
                if dest.exists() {
                    self.merge_directories(&path, &dest)?;
                } else {
                    self.copy_dir_all(&path, &dest)?;
                }
            } else {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                // try to create a hard link first, fall back to copy if it fails
                if let Err(_) = fs::hard_link(&path, &dest) {
                    fs::copy(&path, &dest).map_err(|e| {
                        Error::Io(io::Error::new(
                            io::ErrorKind::Other,
                            format!(
                                "Failed to copy file from {} to {}: {}",
                                path.display(),
                                dest.display(),
                                e
                            ),
                        ))
                    })?;
                }
            }
        }
        Ok(())
    }

    /// merge two directories.
    fn merge_directories(&self, src: &Path, dest: &Path) -> Result<(), Error> {
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if src_path.is_dir() {
                if dest_path.exists() {
                    self.merge_directories(&src_path, &dest_path)?;
                } else {
                    self.copy_dir_all(&src_path, &dest_path)?;
                }
            } else {
                // try to create a hard link first, fall back to copy if it fails
                if let Err(_) = fs::hard_link(&src_path, &dest_path) {
                    fs::copy(&src_path, &dest_path).map_err(|e| {
                        Error::Io(io::Error::new(
                            io::ErrorKind::Other,
                            format!(
                                "Failed to copy file from {} to {}: {}",
                                src_path.display(),
                                dest_path.display(),
                                e
                            ),
                        ))
                    })?;
                }
            }
        }
        Ok(())
    }

    /// copy a directory recursively.
    fn copy_dir_all(&self, src: &Path, dest: &Path) -> Result<(), Error> {
        fs::create_dir_all(dest)?;

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if path.is_dir() {
                self.copy_dir_all(&path, &dest_path)?;
            } else {
                // try to create a hard link first, fall back to copy if it fails
                if let Err(_) = fs::hard_link(&path, &dest_path) {
                    fs::copy(&path, &dest_path).map_err(|e| {
                        Error::Io(io::Error::new(
                            io::ErrorKind::Other,
                            format!(
                                "Failed to copy file from {} to {}: {}",
                                path.display(),
                                dest_path.display(),
                                e
                            ),
                        ))
                    })?;
                }
            }
        }
        Ok(())
    }

    /// clean up problematic directories and symlinks after installation.
    fn cleanup_installation(&self, install_dir: &Path) -> Result<(), Error> {
        // remove broken symlinks (like lib64/lib64)
        let lib64_dir = install_dir.join("lib64");
        if lib64_dir.exists() {
            let nested_lib64 = lib64_dir.join("lib64");
            if nested_lib64.exists() {
                // this is a problematic nested lib64 directory
                if nested_lib64.is_symlink() {
                    fs::remove_file(&nested_lib64)?;
                } else if nested_lib64.is_dir() {
                    // move contents up one level if it's a directory
                    for entry in fs::read_dir(&nested_lib64)? {
                        let entry = entry?;
                        let src_path = entry.path();
                        let dest_path = lib64_dir.join(entry.file_name());

                        if src_path.is_file() {
                            // try to create a hard link first, fall back to copy if it fails
                            if let Err(_) = fs::hard_link(&src_path, &dest_path) {
                                fs::copy(&src_path, &dest_path)?;
                            }
                        } else if src_path.is_dir() {
                            if dest_path.exists() {
                                self.merge_directories(&src_path, &dest_path)?;
                            } else {
                                self.copy_dir_all(&src_path, &dest_path)?;
                            }
                        }
                    }
                    fs::remove_dir_all(&nested_lib64)?;
                }
            }
        }

        // remove other problematic nested directories
        let include_dir = install_dir.join("include");
        if include_dir.exists() {
            let nested_include = include_dir.join("include");
            if nested_include.exists() {
                if nested_include.is_symlink() {
                    fs::remove_file(&nested_include)?;
                } else if nested_include.is_dir() {
                    // move contents up one level
                    for entry in fs::read_dir(&nested_include)? {
                        let entry = entry?;
                        let src_path = entry.path();
                        let dest_path = include_dir.join(entry.file_name());

                        if src_path.is_file() {
                            // try to create a hard link first, fall back to copy if it fails
                            if let Err(_) = fs::hard_link(&src_path, &dest_path) {
                                fs::copy(&src_path, &dest_path)?;
                            }
                        } else if src_path.is_dir() {
                            if dest_path.exists() {
                                self.merge_directories(&src_path, &dest_path)?;
                            } else {
                                self.copy_dir_all(&src_path, &dest_path)?;
                            }
                        }
                    }
                    fs::remove_dir_all(&nested_include)?;
                }
            }
        }

        Ok(())
    }

    fn create_version_file(&self, install_dir: &Path, driver_version: &str) -> Result<(), Error> {
        let version_info = serde_json::json!({
            "cuda_version": self.version.to_string(),
            "driver_version": driver_version,
            "platform": format!("{}", self.platform.platform.os()),
            "installed_at": chrono::Utc::now().to_rfc3339(),
        });

        let version_file = install_dir.join("version.json");
        fs::write(version_file, serde_json::to_string_pretty(&version_info)?)?;
        Ok(())
    }
}

pub struct SimpleProgressReporter {
    progress: std::sync::Mutex<Option<indicatif::ProgressBar>>,
}

impl SimpleProgressReporter {
    pub fn new() -> Self {
        Self {
            progress: std::sync::Mutex::new(None),
        }
    }
}

// TODO(alpin): make the progress bar better
impl Reporter for SimpleProgressReporter {
    fn on_download_start(&self, name: &str, size: Option<u64>) -> usize {
        let progress = indicatif::ProgressBar::new(size.unwrap_or(0));

        let template = if let Some(_size) = size {
            format!(
                "{{msg:{}.dim}} {{bar:30.green/dim}} {{binary_bytes:>7}}/{{binary_total_bytes:7}}",
                name.len().max(20)
            )
        } else {
            "{wide_msg:.dim} ....".to_string()
        };

        progress.set_style(
            indicatif::ProgressStyle::with_template(&template)
                .unwrap()
                .progress_chars("--"),
        );

        progress.set_message(name.to_string());

        *self.progress.lock().unwrap() = Some(progress);

        0
    }

    fn on_download_progress(&self, _id: usize, bytes: u64) {
        if let Some(progress) = &mut *self.progress.lock().unwrap() {
            progress.inc(bytes);
        }
    }

    fn on_download_complete(&self, _id: usize) {
        if let Some(progress) = self.progress.lock().unwrap().take() {
            progress.finish_and_clear();
        }
    }

    fn on_extract_start(&self, name: &str) -> usize {
        eprintln!("Extracting {}", name);
        0
    }

    fn on_extract_complete(&self, _id: usize) {
        eprintln!("Extraction complete");
    }
}
