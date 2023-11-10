pub use client::{RegistryClient, RegistryClientBuilder};
pub use error::Error;

mod cached_client;
mod client;
mod error;
mod remote_metadata;
