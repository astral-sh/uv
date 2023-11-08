use std::path::{Path, PathBuf};

use fs_err as fs;

use puffin_distribution::{BuiltDistribution, Distribution, DistributionIdentifier, SourceDistribution};

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

    /// Return the path at which a given [`Distribution`] would be stored.
    pub(crate) fn entry(&self, distribution: &Distribution) -> PathBuf {
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

impl From<&Distribution> for CacheShard {
    fn from(distribution: &Distribution) -> Self {
        match distribution {
            Distribution::Built(BuiltDistribution::Registry(_)) => Self::Registry,
            Distribution::Built(BuiltDistribution::DirectUrl(_)) => Self::Url,
            Distribution::Source(SourceDistribution::Registry(_)) => Self::Registry,
            Distribution::Source(SourceDistribution::DirectUrl(_)) => Self::Url,
            Distribution::Source(SourceDistribution::Git(_)) => Self::Url,
        }
    }
}
