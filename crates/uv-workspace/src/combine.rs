use std::num::NonZeroUsize;
use std::path::PathBuf;

use distribution_types::IndexUrl;
use install_wheel_rs::linker::LinkMode;
use uv_configuration::{ConfigSettings, IndexStrategy, KeyringProviderType, TargetTriple};
use uv_interpreter::PythonVersion;
use uv_resolver::{AnnotationStyle, ExcludeNewer, PreReleaseMode, ResolutionMode};

use crate::{Options, PipOptions, Workspace};

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

impl Combine for Option<Workspace> {
    /// Combine the options used in two [`Workspace`]s. Retains the root of `self`.
    fn combine(self, other: Option<Workspace>) -> Option<Workspace> {
        match (self, other) {
            (Some(mut a), Some(b)) => {
                a.options = a.options.combine(b.options);
                Some(a)
            }
            (a, b) => a.or(b),
        }
    }
}

impl Combine for Options {
    fn combine(self, other: Options) -> Options {
        Options {
            native_tls: self.native_tls.combine(other.native_tls),
            no_cache: self.no_cache.combine(other.no_cache),
            preview: self.preview.combine(other.preview),
            cache_dir: self.cache_dir.combine(other.cache_dir),
            pip: self.pip.combine(other.pip),
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

impl Combine for PipOptions {
    fn combine(self, other: PipOptions) -> PipOptions {
        PipOptions {
            python: self.python.combine(other.python),
            system: self.system.combine(other.system),
            break_system_packages: self
                .break_system_packages
                .combine(other.break_system_packages),
            target: self.target.combine(other.target),
            offline: self.offline.combine(other.offline),
            index_url: self.index_url.combine(other.index_url),
            extra_index_url: self.extra_index_url.combine(other.extra_index_url),
            no_index: self.no_index.combine(other.no_index),
            find_links: self.find_links.combine(other.find_links),
            index_strategy: self.index_strategy.combine(other.index_strategy),
            keyring_provider: self.keyring_provider.combine(other.keyring_provider),
            no_build: self.no_build.combine(other.no_build),
            no_binary: self.no_binary.combine(other.no_binary),
            only_binary: self.only_binary.combine(other.only_binary),
            no_build_isolation: self.no_build_isolation.combine(other.no_build_isolation),
            strict: self.strict.combine(other.strict),
            extra: self.extra.combine(other.extra),
            all_extras: self.all_extras.combine(other.all_extras),
            no_deps: self.no_deps.combine(other.no_deps),
            resolution: self.resolution.combine(other.resolution),
            prerelease: self.prerelease.combine(other.prerelease),
            output_file: self.output_file.combine(other.output_file),
            no_strip_extras: self.no_strip_extras.combine(other.no_strip_extras),
            no_annotate: self.no_annotate.combine(other.no_annotate),
            no_header: self.no_header.combine(other.no_header),
            custom_compile_command: self
                .custom_compile_command
                .combine(other.custom_compile_command),
            generate_hashes: self.generate_hashes.combine(other.generate_hashes),
            legacy_setup_py: self.legacy_setup_py.combine(other.legacy_setup_py),
            config_settings: self.config_settings.combine(other.config_settings),
            python_version: self.python_version.combine(other.python_version),
            python_platform: self.python_platform.combine(other.python_platform),
            exclude_newer: self.exclude_newer.combine(other.exclude_newer),
            no_emit_package: self.no_emit_package.combine(other.no_emit_package),
            emit_index_url: self.emit_index_url.combine(other.emit_index_url),
            emit_find_links: self.emit_find_links.combine(other.emit_find_links),
            emit_marker_expression: self
                .emit_marker_expression
                .combine(other.emit_marker_expression),
            emit_index_annotation: self
                .emit_index_annotation
                .combine(other.emit_index_annotation),
            annotation_style: self.annotation_style.combine(other.annotation_style),
            link_mode: self.link_mode.combine(other.link_mode),
            compile_bytecode: self.compile_bytecode.combine(other.compile_bytecode),
            require_hashes: self.require_hashes.combine(other.require_hashes),
            concurrent_downloads: self
                .concurrent_downloads
                .combine(other.concurrent_downloads),
            concurrent_builds: self.concurrent_builds.combine(other.concurrent_builds),
            concurrent_installs: self.concurrent_installs.combine(other.concurrent_installs),
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
impl_combine_or!(PreReleaseMode);
impl_combine_or!(PythonVersion);
impl_combine_or!(ResolutionMode);
impl_combine_or!(String);
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
