use std::path::{Path, PathBuf};

use fs_err as fs;

use puffin_distribution::{BuiltDist, Dist, Metadata, SourceDist};

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

    /// Return the path at which a given [`Dist`] would be stored.
    pub(crate) fn entry(&self, dist: &Dist) -> PathBuf {
        self.root
            .join(CacheShard::from(dist).segment())
            .join(dist.package_id())
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

impl From<&Dist> for CacheShard {
    fn from(dist: &Dist) -> Self {
        match dist {
            Dist::Built(BuiltDist::Registry(_)) => Self::Registry,
            Dist::Built(BuiltDist::DirectUrl(_)) => Self::Url,
            Dist::Source(SourceDist::Registry(_)) => Self::Registry,
            Dist::Source(SourceDist::DirectUrl(_)) => Self::Url,
            Dist::Source(SourceDist::Git(_)) => Self::Url,
        }
    }
}
