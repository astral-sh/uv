use std::ops::Deref;

use distribution_types::File;

/// A distribution can either be a wheel or a source distribution.
#[derive(Debug, Clone)]
pub(crate) struct WheelFile(pub(crate) File);

#[derive(Debug, Clone)]
pub(crate) struct SdistFile(pub(crate) File);

#[derive(Debug, Clone)]
pub(crate) enum DistFile {
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

impl From<WheelFile> for DistFile {
    fn from(wheel: WheelFile) -> Self {
        Self::Wheel(wheel)
    }
}

impl From<SdistFile> for DistFile {
    fn from(sdist: SdistFile) -> Self {
        Self::Sdist(sdist)
    }
}

impl DistFile {
    pub(crate) fn filename(&self) -> &str {
        match self {
            Self::Wheel(wheel) => wheel.filename.as_str(),
            Self::Sdist(sdist) => sdist.filename.as_str(),
        }
    }

    pub(crate) fn is_sdist(&self) -> bool {
        match self {
            Self::Wheel(_) => false,
            Self::Sdist(_) => true,
        }
    }
}

impl From<DistFile> for File {
    fn from(file: DistFile) -> Self {
        match file {
            DistFile::Wheel(wheel) => wheel.into(),
            DistFile::Sdist(sdist) => sdist.into(),
        }
    }
}

impl Deref for DistFile {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        match self {
            DistFile::Wheel(file) => &file.0,
            DistFile::Sdist(file) => &file.0,
        }
    }
}
