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
//!

use std::str::FromStr;
use std::sync::LazyLock;

use either::Either;
use url::Url;

use uv_distribution_types::IndexUrl;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::Os;

use crate::{Accelerator, AcceleratorError};

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
}

/// The strategy to use when determining the appropriate PyTorch index.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TorchStrategy {
    /// Select the appropriate PyTorch index based on the operating system and CUDA driver version.
    Auto { os: Os, driver_version: Version },
    /// Use the specified PyTorch index.
    Backend(TorchBackend),
}

impl TorchStrategy {
    /// Determine the [`TorchStrategy`] from the given [`TorchMode`], [`Os`], and [`Accelerator`].
    pub fn from_mode(mode: TorchMode, os: &Os) -> Result<Self, AcceleratorError> {
        match mode {
            TorchMode::Auto => {
                if let Some(Accelerator::Cuda { driver_version }) = Accelerator::detect()? {
                    Ok(Self::Auto {
                        os: os.clone(),
                        driver_version: driver_version.clone(),
                    })
                } else {
                    Ok(Self::Backend(TorchBackend::Cpu))
                }
            }
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
        )
    }

    /// Return the appropriate index URLs for the given [`TorchStrategy`].
    pub fn index_urls(&self) -> impl Iterator<Item = &IndexUrl> {
        match self {
            TorchStrategy::Auto { os, driver_version } => {
                // If this is a GPU-enabled package, and CUDA drivers are installed, use PyTorch's CUDA
                // indexes.
                //
                // See: https://github.com/pmeier/light-the-torch/blob/33397cbe45d07b51ad8ee76b004571a4c236e37f/light_the_torch/_patch.py#L36-L49
                match os {
                    Os::Manylinux { .. } | Os::Musllinux { .. } => Either::Left(Either::Left(
                        LINUX_DRIVERS
                            .iter()
                            .filter_map(move |(backend, version)| {
                                if driver_version >= version {
                                    Some(backend.index_url())
                                } else {
                                    None
                                }
                            })
                            .chain(std::iter::once(TorchBackend::Cpu.index_url())),
                    )),
                    Os::Windows => Either::Left(Either::Right(
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
                    )),
                    Os::Macos { .. }
                    | Os::FreeBsd { .. }
                    | Os::NetBsd { .. }
                    | Os::OpenBsd { .. }
                    | Os::Dragonfly { .. }
                    | Os::Illumos { .. }
                    | Os::Haiku { .. }
                    | Os::Android { .. } => {
                        Either::Right(std::iter::once(TorchBackend::Cpu.index_url()))
                    }
                }
            }
            TorchStrategy::Backend(backend) => Either::Right(std::iter::once(backend.index_url())),
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
            _ => Err(format!("Unknown PyTorch backend: {s}")),
        }
    }
}

/// Linux CUDA driver versions and the corresponding CUDA versions.
///
/// See: <https://github.com/pmeier/light-the-torch/blob/33397cbe45d07b51ad8ee76b004571a4c236e37f/light_the_torch/_cb.py#L150-L213>
static LINUX_DRIVERS: LazyLock<[(TorchBackend, Version); 24]> = LazyLock::new(|| {
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
