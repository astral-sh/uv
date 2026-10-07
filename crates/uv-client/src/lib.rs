pub use error::{Error, ErrorKind};
pub use file_hash::FileHashError;
pub use flat_index::{FlatIndexClient, FlatIndexEntries, FlatIndexEntry, FlatIndexError};
pub use registry_client::{
    MetadataFormat, RegistryClient, RegistryClientBuilder, SimpleDetailMetadata,
    SimpleDetailMetadatum, SimpleIndexMetadata, VersionFiles,
};

mod error;
mod file_hash;
mod flat_index;
mod html;
mod registry_client;
mod remote_metadata;
