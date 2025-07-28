//! `uv-torch` is a library for determining the appropriate PyTorch index based on the operating
//! system and CUDA driver version.
//!
//! This library is derived from `light-the-torch` by Philipp Meier, which is available under the
//! following BSD-3 Clause license:
//!
//! ```text
//! BSD 3-Clause License
//!
//! Copyright (c) 2020, Philip Meier
//! All rights reserved.
//!
//! Redistribution and use in source and binary forms, with or without
//! modification, are permitted provided that the following conditions are met:
//!
//! 1. Redistributions of source code must retain the above copyright notice, this
//!    list of conditions and the following disclaimer.
//!
//! 2. Redistributions in binary form must reproduce the above copyright notice,
//!    this list of conditions and the following disclaimer in the documentation
//!    and/or other materials provided with the distribution.
//!
//! 3. Neither the name of the copyright holder nor the names of its
//!    contributors may be used to endorse or promote products derived from
//!    this software without specific prior written permission.
//!
//! THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
//! AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
//! IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//! DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
//! FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
//! DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//! SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
//! CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
//! OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
//! OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//! ```

use std::str::FromStr;
use std::sync::LazyLock;

use either::Either;
use url::Url;

use uv_distribution_types::IndexUrl;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::Os;

use crate::{Accelerator, AcceleratorError, AmdGpuArchitecture};

/// The strategy to use when determining the appropriate PyTorch index.
#[derive(Debug, Copy, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum TorchMode {
    /// Select the appropriate PyTorch index based on the operating system and CUDA driver version.
    Auto,
    /// Use the CPU-only PyTorch index.
    Cpu,
    /// Use the PyTorch index for CUDA 12.8.
    Cu128,
    /// Use the PyTorch index for CUDA 12.6.
    Cu126,
    /// Use the PyTorch index for CUDA 12.5.
    Cu125,
    /// Use the PyTorch index for CUDA 12.4.
    Cu124,
    /// Use the PyTorch index for CUDA 12.3.
    Cu123,
    /// Use the PyTorch index for CUDA 12.2.
    Cu122,
    /// Use the PyTorch index for CUDA 12.1.
    Cu121,
    /// Use the PyTorch index for CUDA 12.0.
    Cu120,
    /// Use the PyTorch index for CUDA 11.8.
    Cu118,
    /// Use the PyTorch index for CUDA 11.7.
    Cu117,
    /// Use the PyTorch index for CUDA 11.6.
    Cu116,
    /// Use the PyTorch index for CUDA 11.5.
    Cu115,
    /// Use the PyTorch index for CUDA 11.4.
    Cu114,
    /// Use the PyTorch index for CUDA 11.3.
    Cu113,
    /// Use the PyTorch index for CUDA 11.2.
    Cu112,
    /// Use the PyTorch index for CUDA 11.1.
    Cu111,
    /// Use the PyTorch index for CUDA 11.0.
    Cu110,
    /// Use the PyTorch index for CUDA 10.2.
    Cu102,
    /// Use the PyTorch index for CUDA 10.1.
    Cu101,
    /// Use the PyTorch index for CUDA 10.0.
    Cu100,
    /// Use the PyTorch index for CUDA 9.2.
    Cu92,
    /// Use the PyTorch index for CUDA 9.1.
    Cu91,
    /// Use the PyTorch index for CUDA 9.0.
    Cu90,
    /// Use the PyTorch index for CUDA 8.0.
    Cu80,
    /// Use the PyTorch index for ROCm 6.3.
    #[serde(rename = "rocm6.3")]
    #[cfg_attr(feature = "clap", clap(name = "rocm6.3"))]
    Rocm63,
    /// Use the PyTorch index for ROCm 6.2.4.
    #[serde(rename = "rocm6.2.4")]
    #[cfg_attr(feature = "clap", clap(name = "rocm6.2.4"))]
    Rocm624,
    /// Use the PyTorch index for ROCm 6.2.
    #[serde(rename = "rocm6.2")]
    #[cfg_attr(feature = "clap", clap(name = "rocm6.2"))]
    Rocm62,
    /// Use the PyTorch index for ROCm 6.1.
    #[serde(rename = "rocm6.1")]
    #[cfg_attr(feature = "clap", clap(name = "rocm6.1"))]
    Rocm61,
    /// Use the PyTorch index for ROCm 6.0.
    #[serde(rename = "rocm6.0")]
    #[cfg_attr(feature = "clap", clap(name = "rocm6.0"))]
    Rocm60,
    /// Use the PyTorch index for ROCm 5.7.
    #[serde(rename = "rocm5.7")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.7"))]
    Rocm57,
    /// Use the PyTorch index for ROCm 5.6.
    #[serde(rename = "rocm5.6")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.6"))]
    Rocm56,
    /// Use the PyTorch index for ROCm 5.5.
    #[serde(rename = "rocm5.5")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.5"))]
    Rocm55,
    /// Use the PyTorch index for ROCm 5.4.2.
    #[serde(rename = "rocm5.4.2")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.4.2"))]
    Rocm542,
    /// Use the PyTorch index for ROCm 5.4.
    #[serde(rename = "rocm5.4")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.4"))]
    Rocm54,
    /// Use the PyTorch index for ROCm 5.3.
    #[serde(rename = "rocm5.3")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.3"))]
    Rocm53,
    /// Use the PyTorch index for ROCm 5.2.
    #[serde(rename = "rocm5.2")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.2"))]
    Rocm52,
    /// Use the PyTorch index for ROCm 5.1.1.
    #[serde(rename = "rocm5.1.1")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.1.1"))]
    Rocm511,
    /// Use the PyTorch index for ROCm 4.2.
    #[serde(rename = "rocm4.2")]
    #[cfg_attr(feature = "clap", clap(name = "rocm4.2"))]
    Rocm42,
    /// Use the PyTorch index for ROCm 4.1.
    #[serde(rename = "rocm4.1")]
    #[cfg_attr(feature = "clap", clap(name = "rocm4.1"))]
    Rocm41,
    /// Use the PyTorch index for ROCm 4.0.1.
    #[serde(rename = "rocm4.0.1")]
    #[cfg_attr(feature = "clap", clap(name = "rocm4.0.1"))]
    Rocm401,
    /// Use the PyTorch index for Intel XPU.
    Xpu,
}

/// The strategy to use when determining the appropriate PyTorch index.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TorchStrategy {
    /// Select the appropriate PyTorch index based on the operating system and CUDA driver version (e.g., `550.144.03`).
    Cuda { os: Os, driver_version: Version },
    /// Select the appropriate PyTorch index based on the operating system and AMD GPU architecture (e.g., `gfx1100`).
    Amd {
        os: Os,
        gpu_architecture: AmdGpuArchitecture,
    },
    /// Select the appropriate PyTorch index based on the operating system and Intel GPU presence.
    Xpu { os: Os },
    /// Use the specified PyTorch index.
    Backend(TorchBackend),
}

impl TorchStrategy {
    /// Determine the [`TorchStrategy`] from the given [`TorchMode`], [`Os`], and [`Accelerator`].
    pub fn from_mode(mode: TorchMode, os: &Os) -> Result<Self, AcceleratorError> {
        match mode {
            TorchMode::Auto => match Accelerator::detect()? {
                Some(Accelerator::Cuda { driver_version }) => Ok(Self::Cuda {
                    os: os.clone(),
                    driver_version: driver_version.clone(),
                }),
                Some(Accelerator::Amd { gpu_architecture }) => Ok(Self::Amd {
                    os: os.clone(),
                    gpu_architecture,
                }),
                Some(Accelerator::Xpu) => Ok(Self::Xpu { os: os.clone() }),
                None => Ok(Self::Backend(TorchBackend::Cpu)),
            },
            TorchMode::Cpu => Ok(Self::Backend(TorchBackend::Cpu)),
            TorchMode::Cu128 => Ok(Self::Backend(TorchBackend::Cu128)),
            TorchMode::Cu126 => Ok(Self::Backend(TorchBackend::Cu126)),
            TorchMode::Cu125 => Ok(Self::Backend(TorchBackend::Cu125)),
            TorchMode::Cu124 => Ok(Self::Backend(TorchBackend::Cu124)),
            TorchMode::Cu123 => Ok(Self::Backend(TorchBackend::Cu123)),
            TorchMode::Cu122 => Ok(Self::Backend(TorchBackend::Cu122)),
            TorchMode::Cu121 => Ok(Self::Backend(TorchBackend::Cu121)),
            TorchMode::Cu120 => Ok(Self::Backend(TorchBackend::Cu120)),
            TorchMode::Cu118 => Ok(Self::Backend(TorchBackend::Cu118)),
            TorchMode::Cu117 => Ok(Self::Backend(TorchBackend::Cu117)),
            TorchMode::Cu116 => Ok(Self::Backend(TorchBackend::Cu116)),
            TorchMode::Cu115 => Ok(Self::Backend(TorchBackend::Cu115)),
            TorchMode::Cu114 => Ok(Self::Backend(TorchBackend::Cu114)),
            TorchMode::Cu113 => Ok(Self::Backend(TorchBackend::Cu113)),
            TorchMode::Cu112 => Ok(Self::Backend(TorchBackend::Cu112)),
            TorchMode::Cu111 => Ok(Self::Backend(TorchBackend::Cu111)),
            TorchMode::Cu110 => Ok(Self::Backend(TorchBackend::Cu110)),
            TorchMode::Cu102 => Ok(Self::Backend(TorchBackend::Cu102)),
            TorchMode::Cu101 => Ok(Self::Backend(TorchBackend::Cu101)),
            TorchMode::Cu100 => Ok(Self::Backend(TorchBackend::Cu100)),
            TorchMode::Cu92 => Ok(Self::Backend(TorchBackend::Cu92)),
            TorchMode::Cu91 => Ok(Self::Backend(TorchBackend::Cu91)),
            TorchMode::Cu90 => Ok(Self::Backend(TorchBackend::Cu90)),
            TorchMode::Cu80 => Ok(Self::Backend(TorchBackend::Cu80)),
            TorchMode::Rocm63 => Ok(Self::Backend(TorchBackend::Rocm63)),
            TorchMode::Rocm624 => Ok(Self::Backend(TorchBackend::Rocm624)),
            TorchMode::Rocm62 => Ok(Self::Backend(TorchBackend::Rocm62)),
            TorchMode::Rocm61 => Ok(Self::Backend(TorchBackend::Rocm61)),
            TorchMode::Rocm60 => Ok(Self::Backend(TorchBackend::Rocm60)),
            TorchMode::Rocm57 => Ok(Self::Backend(TorchBackend::Rocm57)),
            TorchMode::Rocm56 => Ok(Self::Backend(TorchBackend::Rocm56)),
            TorchMode::Rocm55 => Ok(Self::Backend(TorchBackend::Rocm55)),
            TorchMode::Rocm542 => Ok(Self::Backend(TorchBackend::Rocm542)),
            TorchMode::Rocm54 => Ok(Self::Backend(TorchBackend::Rocm54)),
            TorchMode::Rocm53 => Ok(Self::Backend(TorchBackend::Rocm53)),
            TorchMode::Rocm52 => Ok(Self::Backend(TorchBackend::Rocm52)),
            TorchMode::Rocm511 => Ok(Self::Backend(TorchBackend::Rocm511)),
            TorchMode::Rocm42 => Ok(Self::Backend(TorchBackend::Rocm42)),
            TorchMode::Rocm41 => Ok(Self::Backend(TorchBackend::Rocm41)),
            TorchMode::Rocm401 => Ok(Self::Backend(TorchBackend::Rocm401)),
            TorchMode::Xpu => Ok(Self::Backend(TorchBackend::Xpu)),
        }
    }

    /// Returns `true` if the [`TorchStrategy`] applies to the given [`PackageName`].
    pub fn applies_to(&self, package_name: &PackageName) -> bool {
        matches!(
            package_name.as_str(),
            "torch"
                | "torch-model-archiver"
                | "torch-tb-profiler"
                | "torcharrow"
                | "torchaudio"
                | "torchcsprng"
                | "torchdata"
                | "torchdistx"
                | "torchserve"
                | "torchtext"
                | "torchvision"
                | "pytorch-triton"
                | "pytorch-triton-rocm"
                | "pytorch-triton-xpu"
        )
    }

    /// Return the appropriate index URLs for the given [`TorchStrategy`].
    pub fn index_urls(&self) -> impl Iterator<Item = &IndexUrl> {
        match self {
            TorchStrategy::Cuda { os, driver_version } => {
                // If this is a GPU-enabled package, and CUDA drivers are installed, use PyTorch's CUDA
                // indexes.
                //
                // See: https://github.com/pmeier/light-the-torch/blob/33397cbe45d07b51ad8ee76b004571a4c236e37f/light_the_torch/_patch.py#L36-L49
                match os {
                    Os::Manylinux { .. } | Os::Musllinux { .. } => {
                        Either::Left(Either::Left(Either::Left(
                            LINUX_CUDA_DRIVERS
                                .iter()
                                .filter_map(move |(backend, version)| {
                                    if driver_version >= version {
                                        Some(backend.index_url())
                                    } else {
                                        None
                                    }
                                })
                                .chain(std::iter::once(TorchBackend::Cpu.index_url())),
                        )))
                    }
                    Os::Windows => Either::Left(Either::Left(Either::Right(
                        WINDOWS_CUDA_VERSIONS
                            .iter()
                            .filter_map(move |(backend, version)| {
                                if driver_version >= version {
                                    Some(backend.index_url())
                                } else {
                                    None
                                }
                            })
                            .chain(std::iter::once(TorchBackend::Cpu.index_url())),
                    ))),
                    Os::Macos { .. }
                    | Os::FreeBsd { .. }
                    | Os::NetBsd { .. }
                    | Os::OpenBsd { .. }
                    | Os::Dragonfly { .. }
                    | Os::Illumos { .. }
                    | Os::Haiku { .. }
                    | Os::Android { .. }
                    | Os::Pyodide { .. } => {
                        Either::Right(Either::Left(std::iter::once(TorchBackend::Cpu.index_url())))
                    }
                }
            }
            TorchStrategy::Amd {
                os,
                gpu_architecture,
            } => match os {
                Os::Manylinux { .. } | Os::Musllinux { .. } => Either::Left(Either::Right(
                    LINUX_AMD_GPU_DRIVERS
                        .iter()
                        .filter_map(move |(backend, architecture)| {
                            if gpu_architecture == architecture {
                                Some(backend.index_url())
                            } else {
                                None
                            }
                        })
                        .chain(std::iter::once(TorchBackend::Cpu.index_url())),
                )),
                Os::Windows
                | Os::Macos { .. }
                | Os::FreeBsd { .. }
                | Os::NetBsd { .. }
                | Os::OpenBsd { .. }
                | Os::Dragonfly { .. }
                | Os::Illumos { .. }
                | Os::Haiku { .. }
                | Os::Android { .. }
                | Os::Pyodide { .. } => {
                    Either::Right(Either::Left(std::iter::once(TorchBackend::Cpu.index_url())))
                }
            },
            TorchStrategy::Xpu { os } => match os {
                Os::Manylinux { .. } => Either::Right(Either::Right(Either::Left(
                    std::iter::once(TorchBackend::Xpu.index_url()),
                ))),
                Os::Windows
                | Os::Musllinux { .. }
                | Os::Macos { .. }
                | Os::FreeBsd { .. }
                | Os::NetBsd { .. }
                | Os::OpenBsd { .. }
                | Os::Dragonfly { .. }
                | Os::Illumos { .. }
                | Os::Haiku { .. }
                | Os::Android { .. }
                | Os::Pyodide { .. } => {
                    Either::Right(Either::Left(std::iter::once(TorchBackend::Cpu.index_url())))
                }
            },
            TorchStrategy::Backend(backend) => Either::Right(Either::Right(Either::Right(
                std::iter::once(backend.index_url()),
            ))),
        }
    }
}

/// The available backends for PyTorch.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TorchBackend {
    Cpu,
    Cu128,
    Cu126,
    Cu125,
    Cu124,
    Cu123,
    Cu122,
    Cu121,
    Cu120,
    Cu118,
    Cu117,
    Cu116,
    Cu115,
    Cu114,
    Cu113,
    Cu112,
    Cu111,
    Cu110,
    Cu102,
    Cu101,
    Cu100,
    Cu92,
    Cu91,
    Cu90,
    Cu80,
    Rocm63,
    Rocm624,
    Rocm62,
    Rocm61,
    Rocm60,
    Rocm57,
    Rocm56,
    Rocm55,
    Rocm542,
    Rocm54,
    Rocm53,
    Rocm52,
    Rocm511,
    Rocm42,
    Rocm41,
    Rocm401,
    Xpu,
}

impl TorchBackend {
    /// Return the appropriate index URL for the given [`TorchBackend`].
    fn index_url(self) -> &'static IndexUrl {
        match self {
            Self::Cpu => &CPU_INDEX_URL,
            Self::Cu128 => &CU128_INDEX_URL,
            Self::Cu126 => &CU126_INDEX_URL,
            Self::Cu125 => &CU125_INDEX_URL,
            Self::Cu124 => &CU124_INDEX_URL,
            Self::Cu123 => &CU123_INDEX_URL,
            Self::Cu122 => &CU122_INDEX_URL,
            Self::Cu121 => &CU121_INDEX_URL,
            Self::Cu120 => &CU120_INDEX_URL,
            Self::Cu118 => &CU118_INDEX_URL,
            Self::Cu117 => &CU117_INDEX_URL,
            Self::Cu116 => &CU116_INDEX_URL,
            Self::Cu115 => &CU115_INDEX_URL,
            Self::Cu114 => &CU114_INDEX_URL,
            Self::Cu113 => &CU113_INDEX_URL,
            Self::Cu112 => &CU112_INDEX_URL,
            Self::Cu111 => &CU111_INDEX_URL,
            Self::Cu110 => &CU110_INDEX_URL,
            Self::Cu102 => &CU102_INDEX_URL,
            Self::Cu101 => &CU101_INDEX_URL,
            Self::Cu100 => &CU100_INDEX_URL,
            Self::Cu92 => &CU92_INDEX_URL,
            Self::Cu91 => &CU91_INDEX_URL,
            Self::Cu90 => &CU90_INDEX_URL,
            Self::Cu80 => &CU80_INDEX_URL,
            Self::Rocm63 => &ROCM63_INDEX_URL,
            Self::Rocm624 => &ROCM624_INDEX_URL,
            Self::Rocm62 => &ROCM62_INDEX_URL,
            Self::Rocm61 => &ROCM61_INDEX_URL,
            Self::Rocm60 => &ROCM60_INDEX_URL,
            Self::Rocm57 => &ROCM57_INDEX_URL,
            Self::Rocm56 => &ROCM56_INDEX_URL,
            Self::Rocm55 => &ROCM55_INDEX_URL,
            Self::Rocm542 => &ROCM542_INDEX_URL,
            Self::Rocm54 => &ROCM54_INDEX_URL,
            Self::Rocm53 => &ROCM53_INDEX_URL,
            Self::Rocm52 => &ROCM52_INDEX_URL,
            Self::Rocm511 => &ROCM511_INDEX_URL,
            Self::Rocm42 => &ROCM42_INDEX_URL,
            Self::Rocm41 => &ROCM41_INDEX_URL,
            Self::Rocm401 => &ROCM401_INDEX_URL,
            Self::Xpu => &XPU_INDEX_URL,
        }
    }

    /// Extract a [`TorchBackend`] from an index URL.
    pub fn from_index(index: &Url) -> Option<Self> {
        let backend_identifier = if index.host_str() == Some("download.pytorch.org") {
            // E.g., `https://download.pytorch.org/whl/cu124`
            let mut path_segments = index.path_segments()?;
            if path_segments.next() != Some("whl") {
                return None;
            }
            path_segments.next()?
        } else {
            return None;
        };
        Self::from_str(backend_identifier).ok()
    }

    /// Returns the CUDA [`Version`] for the given [`TorchBackend`].
    pub fn cuda_version(&self) -> Option<Version> {
        match self {
            TorchBackend::Cpu => None,
            TorchBackend::Cu128 => Some(Version::new([12, 8])),
            TorchBackend::Cu126 => Some(Version::new([12, 6])),
            TorchBackend::Cu125 => Some(Version::new([12, 5])),
            TorchBackend::Cu124 => Some(Version::new([12, 4])),
            TorchBackend::Cu123 => Some(Version::new([12, 3])),
            TorchBackend::Cu122 => Some(Version::new([12, 2])),
            TorchBackend::Cu121 => Some(Version::new([12, 1])),
            TorchBackend::Cu120 => Some(Version::new([12, 0])),
            TorchBackend::Cu118 => Some(Version::new([11, 8])),
            TorchBackend::Cu117 => Some(Version::new([11, 7])),
            TorchBackend::Cu116 => Some(Version::new([11, 6])),
            TorchBackend::Cu115 => Some(Version::new([11, 5])),
            TorchBackend::Cu114 => Some(Version::new([11, 4])),
            TorchBackend::Cu113 => Some(Version::new([11, 3])),
            TorchBackend::Cu112 => Some(Version::new([11, 2])),
            TorchBackend::Cu111 => Some(Version::new([11, 1])),
            TorchBackend::Cu110 => Some(Version::new([11, 0])),
            TorchBackend::Cu102 => Some(Version::new([10, 2])),
            TorchBackend::Cu101 => Some(Version::new([10, 1])),
            TorchBackend::Cu100 => Some(Version::new([10, 0])),
            TorchBackend::Cu92 => Some(Version::new([9, 2])),
            TorchBackend::Cu91 => Some(Version::new([9, 1])),
            TorchBackend::Cu90 => Some(Version::new([9, 0])),
            TorchBackend::Cu80 => Some(Version::new([8, 0])),
            TorchBackend::Rocm63 => None,
            TorchBackend::Rocm624 => None,
            TorchBackend::Rocm62 => None,
            TorchBackend::Rocm61 => None,
            TorchBackend::Rocm60 => None,
            TorchBackend::Rocm57 => None,
            TorchBackend::Rocm56 => None,
            TorchBackend::Rocm55 => None,
            TorchBackend::Rocm542 => None,
            TorchBackend::Rocm54 => None,
            TorchBackend::Rocm53 => None,
            TorchBackend::Rocm52 => None,
            TorchBackend::Rocm511 => None,
            TorchBackend::Rocm42 => None,
            TorchBackend::Rocm41 => None,
            TorchBackend::Rocm401 => None,
            TorchBackend::Xpu => None,
        }
    }

    /// Returns the ROCM [`Version`] for the given [`TorchBackend`].
    pub fn rocm_version(&self) -> Option<Version> {
        match self {
            TorchBackend::Cpu => None,
            TorchBackend::Cu128 => None,
            TorchBackend::Cu126 => None,
            TorchBackend::Cu125 => None,
            TorchBackend::Cu124 => None,
            TorchBackend::Cu123 => None,
            TorchBackend::Cu122 => None,
            TorchBackend::Cu121 => None,
            TorchBackend::Cu120 => None,
            TorchBackend::Cu118 => None,
            TorchBackend::Cu117 => None,
            TorchBackend::Cu116 => None,
            TorchBackend::Cu115 => None,
            TorchBackend::Cu114 => None,
            TorchBackend::Cu113 => None,
            TorchBackend::Cu112 => None,
            TorchBackend::Cu111 => None,
            TorchBackend::Cu110 => None,
            TorchBackend::Cu102 => None,
            TorchBackend::Cu101 => None,
            TorchBackend::Cu100 => None,
            TorchBackend::Cu92 => None,
            TorchBackend::Cu91 => None,
            TorchBackend::Cu90 => None,
            TorchBackend::Cu80 => None,
            TorchBackend::Rocm63 => Some(Version::new([6, 3])),
            TorchBackend::Rocm624 => Some(Version::new([6, 2, 4])),
            TorchBackend::Rocm62 => Some(Version::new([6, 2])),
            TorchBackend::Rocm61 => Some(Version::new([6, 1])),
            TorchBackend::Rocm60 => Some(Version::new([6, 0])),
            TorchBackend::Rocm57 => Some(Version::new([5, 7])),
            TorchBackend::Rocm56 => Some(Version::new([5, 6])),
            TorchBackend::Rocm55 => Some(Version::new([5, 5])),
            TorchBackend::Rocm542 => Some(Version::new([5, 4, 2])),
            TorchBackend::Rocm54 => Some(Version::new([5, 4])),
            TorchBackend::Rocm53 => Some(Version::new([5, 3])),
            TorchBackend::Rocm52 => Some(Version::new([5, 2])),
            TorchBackend::Rocm511 => Some(Version::new([5, 1, 1])),
            TorchBackend::Rocm42 => Some(Version::new([4, 2])),
            TorchBackend::Rocm41 => Some(Version::new([4, 1])),
            TorchBackend::Rocm401 => Some(Version::new([4, 0, 1])),
            TorchBackend::Xpu => None,
        }
    }
}

impl FromStr for TorchBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cpu" => Ok(TorchBackend::Cpu),
            "cu128" => Ok(TorchBackend::Cu128),
            "cu126" => Ok(TorchBackend::Cu126),
            "cu125" => Ok(TorchBackend::Cu125),
            "cu124" => Ok(TorchBackend::Cu124),
            "cu123" => Ok(TorchBackend::Cu123),
            "cu122" => Ok(TorchBackend::Cu122),
            "cu121" => Ok(TorchBackend::Cu121),
            "cu120" => Ok(TorchBackend::Cu120),
            "cu118" => Ok(TorchBackend::Cu118),
            "cu117" => Ok(TorchBackend::Cu117),
            "cu116" => Ok(TorchBackend::Cu116),
            "cu115" => Ok(TorchBackend::Cu115),
            "cu114" => Ok(TorchBackend::Cu114),
            "cu113" => Ok(TorchBackend::Cu113),
            "cu112" => Ok(TorchBackend::Cu112),
            "cu111" => Ok(TorchBackend::Cu111),
            "cu110" => Ok(TorchBackend::Cu110),
            "cu102" => Ok(TorchBackend::Cu102),
            "cu101" => Ok(TorchBackend::Cu101),
            "cu100" => Ok(TorchBackend::Cu100),
            "cu92" => Ok(TorchBackend::Cu92),
            "cu91" => Ok(TorchBackend::Cu91),
            "cu90" => Ok(TorchBackend::Cu90),
            "cu80" => Ok(TorchBackend::Cu80),
            "rocm6.3" => Ok(TorchBackend::Rocm63),
            "rocm6.2.4" => Ok(TorchBackend::Rocm624),
            "rocm6.2" => Ok(TorchBackend::Rocm62),
            "rocm6.1" => Ok(TorchBackend::Rocm61),
            "rocm6.0" => Ok(TorchBackend::Rocm60),
            "rocm5.7" => Ok(TorchBackend::Rocm57),
            "rocm5.6" => Ok(TorchBackend::Rocm56),
            "rocm5.5" => Ok(TorchBackend::Rocm55),
            "rocm5.4.2" => Ok(TorchBackend::Rocm542),
            "rocm5.4" => Ok(TorchBackend::Rocm54),
            "rocm5.3" => Ok(TorchBackend::Rocm53),
            "rocm5.2" => Ok(TorchBackend::Rocm52),
            "rocm5.1.1" => Ok(TorchBackend::Rocm511),
            "rocm4.2" => Ok(TorchBackend::Rocm42),
            "rocm4.1" => Ok(TorchBackend::Rocm41),
            "rocm4.0.1" => Ok(TorchBackend::Rocm401),
            "xpu" => Ok(TorchBackend::Xpu),
            _ => Err(format!("Unknown PyTorch backend: {s}")),
        }
    }
}

/// Linux CUDA driver versions and the corresponding CUDA versions.
///
/// See: <https://github.com/pmeier/light-the-torch/blob/33397cbe45d07b51ad8ee76b004571a4c236e37f/light_the_torch/_cb.py#L150-L213>
static LINUX_CUDA_DRIVERS: LazyLock<[(TorchBackend, Version); 24]> = LazyLock::new(|| {
    [
        // Table 2 from
        // https://docs.nvidia.com/cuda/cuda-toolkit-release-notes/index.html
        (TorchBackend::Cu128, Version::new([525, 60, 13])),
        (TorchBackend::Cu126, Version::new([525, 60, 13])),
        (TorchBackend::Cu125, Version::new([525, 60, 13])),
        (TorchBackend::Cu124, Version::new([525, 60, 13])),
        (TorchBackend::Cu123, Version::new([525, 60, 13])),
        (TorchBackend::Cu122, Version::new([525, 60, 13])),
        (TorchBackend::Cu121, Version::new([525, 60, 13])),
        (TorchBackend::Cu120, Version::new([525, 60, 13])),
        // Table 2 from
        // https://docs.nvidia.com/cuda/archive/11.8.0/cuda-toolkit-release-notes/index.html
        (TorchBackend::Cu118, Version::new([450, 80, 2])),
        (TorchBackend::Cu117, Version::new([450, 80, 2])),
        (TorchBackend::Cu116, Version::new([450, 80, 2])),
        (TorchBackend::Cu115, Version::new([450, 80, 2])),
        (TorchBackend::Cu114, Version::new([450, 80, 2])),
        (TorchBackend::Cu113, Version::new([450, 80, 2])),
        (TorchBackend::Cu112, Version::new([450, 80, 2])),
        (TorchBackend::Cu111, Version::new([450, 80, 2])),
        (TorchBackend::Cu110, Version::new([450, 36, 6])),
        // Table 1 from
        // https://docs.nvidia.com/cuda/archive/10.2/cuda-toolkit-release-notes/index.html
        (TorchBackend::Cu102, Version::new([440, 33])),
        (TorchBackend::Cu101, Version::new([418, 39])),
        (TorchBackend::Cu100, Version::new([410, 48])),
        (TorchBackend::Cu92, Version::new([396, 26])),
        (TorchBackend::Cu91, Version::new([390, 46])),
        (TorchBackend::Cu90, Version::new([384, 81])),
        (TorchBackend::Cu80, Version::new([375, 26])),
    ]
});

/// Windows CUDA driver versions and the corresponding CUDA versions.
///
/// See: <https://github.com/pmeier/light-the-torch/blob/33397cbe45d07b51ad8ee76b004571a4c236e37f/light_the_torch/_cb.py#L150-L213>
static WINDOWS_CUDA_VERSIONS: LazyLock<[(TorchBackend, Version); 24]> = LazyLock::new(|| {
    [
        // Table 2 from
        // https://docs.nvidia.com/cuda/cuda-toolkit-release-notes/index.html
        (TorchBackend::Cu128, Version::new([528, 33])),
        (TorchBackend::Cu126, Version::new([528, 33])),
        (TorchBackend::Cu125, Version::new([528, 33])),
        (TorchBackend::Cu124, Version::new([528, 33])),
        (TorchBackend::Cu123, Version::new([528, 33])),
        (TorchBackend::Cu122, Version::new([528, 33])),
        (TorchBackend::Cu121, Version::new([528, 33])),
        (TorchBackend::Cu120, Version::new([528, 33])),
        // Table 2 from
        // https://docs.nvidia.com/cuda/archive/11.8.0/cuda-toolkit-release-notes/index.html
        (TorchBackend::Cu118, Version::new([452, 39])),
        (TorchBackend::Cu117, Version::new([452, 39])),
        (TorchBackend::Cu116, Version::new([452, 39])),
        (TorchBackend::Cu115, Version::new([452, 39])),
        (TorchBackend::Cu114, Version::new([452, 39])),
        (TorchBackend::Cu113, Version::new([452, 39])),
        (TorchBackend::Cu112, Version::new([452, 39])),
        (TorchBackend::Cu111, Version::new([452, 39])),
        (TorchBackend::Cu110, Version::new([451, 22])),
        // Table 1 from
        // https://docs.nvidia.com/cuda/archive/10.2/cuda-toolkit-release-notes/index.html
        (TorchBackend::Cu102, Version::new([441, 22])),
        (TorchBackend::Cu101, Version::new([418, 96])),
        (TorchBackend::Cu100, Version::new([411, 31])),
        (TorchBackend::Cu92, Version::new([398, 26])),
        (TorchBackend::Cu91, Version::new([391, 29])),
        (TorchBackend::Cu90, Version::new([385, 54])),
        (TorchBackend::Cu80, Version::new([376, 51])),
    ]
});

/// Linux AMD GPU architectures and the corresponding PyTorch backends.
///
/// These were inferred by running the following snippet for each ROCm version:
///
/// ```python
/// import torch
///
/// print(torch.cuda.get_arch_list())
/// ```
///
/// AMD also provides a compatibility matrix: <https://rocm.docs.amd.com/en/latest/compatibility/compatibility-matrix.html>;
/// however, this list includes a broader array of GPUs than those in the matrix.
static LINUX_AMD_GPU_DRIVERS: LazyLock<[(TorchBackend, AmdGpuArchitecture); 44]> =
    LazyLock::new(|| {
        [
            // ROCm 6.3
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx942),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx1101),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx1102),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx1200),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx1201),
            // ROCm 6.2.4
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx942),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx1101),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx1102),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx1200),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx1201),
            // ROCm 6.2
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx942),
            // ROCm 6.1
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx942),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx1101),
            // ROCm 6.0
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx942),
        ]
    });

static CPU_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cpu").unwrap());
static CU128_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu128").unwrap());
static CU126_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu126").unwrap());
static CU125_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu125").unwrap());
static CU124_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu124").unwrap());
static CU123_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu123").unwrap());
static CU122_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu122").unwrap());
static CU121_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu121").unwrap());
static CU120_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu120").unwrap());
static CU118_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu118").unwrap());
static CU117_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu117").unwrap());
static CU116_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu116").unwrap());
static CU115_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu115").unwrap());
static CU114_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu114").unwrap());
static CU113_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu113").unwrap());
static CU112_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu112").unwrap());
static CU111_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu111").unwrap());
static CU110_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu110").unwrap());
static CU102_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu102").unwrap());
static CU101_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu101").unwrap());
static CU100_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu100").unwrap());
static CU92_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu92").unwrap());
static CU91_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu91").unwrap());
static CU90_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu90").unwrap());
static CU80_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu80").unwrap());
static ROCM63_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm6.3").unwrap());
static ROCM624_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm6.2.4").unwrap());
static ROCM62_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm6.2").unwrap());
static ROCM61_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm6.1").unwrap());
static ROCM60_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm6.0").unwrap());
static ROCM57_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.7").unwrap());
static ROCM56_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.6").unwrap());
static ROCM55_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.5").unwrap());
static ROCM542_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.4.2").unwrap());
static ROCM54_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.4").unwrap());
static ROCM53_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.3").unwrap());
static ROCM52_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.2").unwrap());
static ROCM511_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.1.1").unwrap());
static ROCM42_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm4.2").unwrap());
static ROCM41_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm4.1").unwrap());
static ROCM401_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm4.0.1").unwrap());
static XPU_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/xpu").unwrap());
