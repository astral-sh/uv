pub use distribution::{Distribution, LocalDistribution, RemoteDistribution};
pub use downloader::{Downloader, Reporter as DownloadReporter};
pub use index::LocalIndex;
pub use installer::{Installer, Reporter as InstallReporter};
pub use uninstall::uninstall;

mod cache;
mod distribution;
mod downloader;
mod index;
mod installer;
mod uninstall;
mod vendor;
