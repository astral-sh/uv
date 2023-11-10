use std::ops::Deref;

use pypi_types::File;

/// A distribution can either be a wheel or a source distribution.
#[derive(Debug, Clone)]
pub(crate) struct WheelFile(pub(crate) File);

#[derive(Debug, Clone)]
pub(crate) struct SdistFile(pub(crate) File);

#[derive(Debug, Clone)]
pub(crate) enum DistributionFile {
    Wheel(WheelFile),
    Sdist(SdistFile),
}

impl Deref for WheelFile {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for SdistFile {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<WheelFile> for File {
    fn from(wheel: WheelFile) -> Self {
        wheel.0
    }
}

impl From<SdistFile> for File {
    fn from(sdist: SdistFile) -> Self {
        sdist.0
    }
}

impl From<WheelFile> for DistributionFile {
    fn from(wheel: WheelFile) -> Self {
        Self::Wheel(wheel)
    }
}

impl From<SdistFile> for DistributionFile {
    fn from(sdist: SdistFile) -> Self {
        Self::Sdist(sdist)
    }
}

impl DistributionFile {
    pub(crate) fn filename(&self) -> &str {
        match self {
            Self::Wheel(wheel) => wheel.filename.as_str(),
            Self::Sdist(sdist) => sdist.filename.as_str(),
        }
    }
}

impl From<DistributionFile> for File {
    fn from(file: DistributionFile) -> Self {
        match file {
            DistributionFile::Wheel(wheel) => wheel.into(),
            DistributionFile::Sdist(sdist) => sdist.into(),
        }
    }
}
