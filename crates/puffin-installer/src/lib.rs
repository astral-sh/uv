pub use distribution::{Distribution, LocalDistribution, RemoteDistribution};
pub use index::LocalIndex;
pub use install::install;
pub use uninstall::uninstall;

mod cache;
mod distribution;
mod index;
mod install;
mod uninstall;
mod vendor;
