use std::path::PathBuf;

use tracing::warn;

pub use built_wheel_index::BuiltWheelIndex;
pub use registry_wheel_index::RegistryWheelIndex;

mod built_wheel_index;
mod registry_wheel_index;

/// Iterate over the subdirectories of a directory.
fn iter_directories(read_dir: std::fs::ReadDir) -> impl Iterator<Item = PathBuf> {
    read_dir
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(err) => {
                warn!("Failed to read entry of cache: {}", err);
                None
            }
        })
        .filter(|entry| {
            entry
                .file_type()
                .map_or(false, |file_type| file_type.is_dir())
        })
        .map(|entry| entry.path())
}
