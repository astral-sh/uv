use puffin_client::File;
use std::ops::Deref;

/// A distribution can either be a wheel or a source distribution.
#[derive(Debug, Clone)]
pub(crate) struct WheelFile(File);

#[derive(Debug, Clone)]
pub(crate) struct SdistFile(File);

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

impl From<File> for WheelFile {
    fn from(file: File) -> Self {
        Self(file)
    }
}

impl From<File> for SdistFile {
    fn from(file: File) -> Self {
        Self(file)
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

impl From<File> for DistributionFile {
    fn from(file: File) -> Self {
        if std::path::Path::new(file.filename.as_str())
            .extension()
            .map_or(false, |ext| ext.eq_ignore_ascii_case("whl"))
        {
            Self::Wheel(WheelFile::from(file))
        } else {
            Self::Sdist(SdistFile::from(file))
        }
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
