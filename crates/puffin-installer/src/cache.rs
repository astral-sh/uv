use std::path::{Path, PathBuf};

use fs_err as fs;
use puffin_distribution::RemoteDistribution;

static WHEEL_CACHE: &str = "wheels-v0";

#[derive(Debug)]
pub(crate) struct WheelCache {
    root: PathBuf,
}

impl WheelCache {
    /// Create a handle to the wheel cache.
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            root: root.join(WHEEL_CACHE),
        }
    }

    /// Initialize the wheel cache.
    pub(crate) fn init(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.root)
    }

    /// Return the path at which a given [`RemoteDistribution`] would be stored.
    pub(crate) fn entry(&self, distribution: &RemoteDistribution) -> PathBuf {
        self.root
            .join(CacheShard::from(distribution).segment())
            .join(distribution.id())
    }

    /// Returns a handle to the wheel cache directory.
    pub(crate) fn read_dir(&self, shard: CacheShard) -> std::io::Result<fs::ReadDir> {
        fs::read_dir(self.root.join(shard.segment()))
    }

    /// Returns the cache root.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

/// A shard of the wheel cache.
#[derive(Debug, Copy, Clone)]
pub(crate) enum CacheShard {
    Registry,
    Url,
}

impl CacheShard {
    fn segment(&self) -> impl AsRef<Path> + '_ {
        match self {
            Self::Registry => "registry",
            Self::Url => "url",
        }
    }
}

impl From<&RemoteDistribution> for CacheShard {
    fn from(distribution: &RemoteDistribution) -> Self {
        match distribution {
            RemoteDistribution::Registry(_, _, _) => Self::Registry,
            RemoteDistribution::Url(_, _) => Self::Url,
        }
    }
}
