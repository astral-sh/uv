use std::num::NonZeroUsize;
use std::path::PathBuf;

use distribution_types::IndexUrl;
use install_wheel_rs::linker::LinkMode;
use pypi_types::SupportedEnvironments;
use uv_configuration::{ConfigSettings, IndexStrategy, KeyringProviderType, TargetTriple};
use uv_python::{PythonDownloads, PythonPreference, PythonVersion};
use uv_resolver::{AnnotationStyle, ExcludeNewer, PrereleaseMode, ResolutionMode};

use crate::{FilesystemOptions, Options, PipOptions};

pub trait Combine {
    /// Combine two values, preferring the values in `self`.
    ///
    /// The logic should follow that of Cargo's `config.toml`:
    ///
    /// > If a key is specified in multiple config files, the values will get merged together.
    /// > Numbers, strings, and booleans will use the value in the deeper config directory taking
    /// > precedence over ancestor directories, where the home directory is the lowest priority.
    /// > Arrays will be joined together with higher precedence items being placed later in the
    /// > merged array.
    ///
    /// ...with one exception: we place items with higher precedence earlier in the merged array.
    #[must_use]
    fn combine(self, other: Self) -> Self;
}

impl Combine for Option<FilesystemOptions> {
    /// Combine the options used in two [`FilesystemOptions`]s. Retains the root of `self`.
    fn combine(self, other: Option<FilesystemOptions>) -> Option<FilesystemOptions> {
        match (self, other) {
            (Some(a), Some(b)) => Some(FilesystemOptions(
                a.into_options().combine(b.into_options()),
            )),
            (a, b) => a.or(b),
        }
    }
}

impl Combine for Option<Options> {
    /// Combine the options used in two [`Options`]s. Retains the root of `self`.
    fn combine(self, other: Option<Options>) -> Option<Options> {
        match (self, other) {
            (Some(a), Some(b)) => Some(a.combine(b)),
            (a, b) => a.or(b),
        }
    }
}

impl Combine for Option<PipOptions> {
    fn combine(self, other: Option<PipOptions>) -> Option<PipOptions> {
        match (self, other) {
            (Some(a), Some(b)) => Some(a.combine(b)),
            (a, b) => a.or(b),
        }
    }
}

macro_rules! impl_combine_or {
    ($name:ident) => {
        impl Combine for Option<$name> {
            fn combine(self, other: Option<$name>) -> Option<$name> {
                self.or(other)
            }
        }
    };
}

impl_combine_or!(AnnotationStyle);
impl_combine_or!(ExcludeNewer);
impl_combine_or!(IndexStrategy);
impl_combine_or!(IndexUrl);
impl_combine_or!(KeyringProviderType);
impl_combine_or!(LinkMode);
impl_combine_or!(NonZeroUsize);
impl_combine_or!(PathBuf);
impl_combine_or!(PrereleaseMode);
impl_combine_or!(PythonDownloads);
impl_combine_or!(PythonPreference);
impl_combine_or!(PythonVersion);
impl_combine_or!(ResolutionMode);
impl_combine_or!(String);
impl_combine_or!(SupportedEnvironments);
impl_combine_or!(TargetTriple);
impl_combine_or!(bool);

impl<T> Combine for Option<Vec<T>> {
    /// Combine two vectors by extending the vector in `self` with the vector in `other`, if they're
    /// both `Some`.
    fn combine(self, other: Option<Vec<T>>) -> Option<Vec<T>> {
        match (self, other) {
            (Some(mut a), Some(b)) => {
                a.extend(b);
                Some(a)
            }
            (a, b) => a.or(b),
        }
    }
}

impl Combine for Option<ConfigSettings> {
    /// Combine two maps by merging the map in `self` with the map in `other`, if they're both
    /// `Some`.
    fn combine(self, other: Option<ConfigSettings>) -> Option<ConfigSettings> {
        match (self, other) {
            (Some(a), Some(b)) => Some(a.merge(b)),
            (a, b) => a.or(b),
        }
    }
}

impl Combine for serde::de::IgnoredAny {
    fn combine(self, _other: Self) -> Self {
        self
    }
}
