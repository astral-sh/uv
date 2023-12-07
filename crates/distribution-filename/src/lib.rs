use std::str::FromStr;

pub use source_dist::{SourceDistExtension, SourceDistFilename, SourceDistFilenameError};
pub use wheel::{WheelFilename, WheelFilenameError};

mod source_dist;
mod wheel;

#[derive(Debug, Clone)]
pub enum DistFilename {
    SourceDistFilename(SourceDistFilename),
    WheelFilename(WheelFilename),
}

impl DistFilename {
    pub fn try_from_filename(
        filename: &str,
        package_name: &puffin_normalize::PackageName,
    ) -> Option<Self> {
        if let Ok(filename) = WheelFilename::from_str(filename) {
            Some(Self::WheelFilename(filename))
        } else if let Ok(filename) = SourceDistFilename::parse(filename, package_name) {
            Some(Self::SourceDistFilename(filename))
        } else {
            None
        }
    }
}
