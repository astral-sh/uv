//! Python versions, requests, installation keys, and configuration values.

mod architecture;
mod download_request;
mod implementation;
mod installation_key;
mod mirrors;
mod prefix;
mod python_version;
mod request;
mod target;

pub use architecture::PythonArchitecture;
pub use download_request::{
    ArchRequest, PlatformRequest, PythonDownloadRequest, PythonDownloadRequestError,
};
pub use implementation::{
    Error as ImplementationError, ImplementationName, LenientImplementationName,
};
pub use installation_key::{
    PythonInstallationKey, PythonInstallationKeyError, PythonInstallationMinorVersionKey,
};
pub use mirrors::PythonDownloadMirrors;
pub use prefix::Prefix;
pub(crate) use python_version::python_build_version_from_env;
pub use python_version::{BuildVersionError, PythonVersion, python_build_versions_from_env};
pub use request::{
    EnvironmentPreference, ExecutableName, PythonDownloads, PythonPreference, PythonRequest,
    PythonRequestError, PythonSource, PythonVariant, VersionRequest,
};
pub use target::Target;
