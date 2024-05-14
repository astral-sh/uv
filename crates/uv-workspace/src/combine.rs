use uv_configuration::ConfigSettings;

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
            native_tls: self.native_tls.or(other.native_tls),
            no_cache: self.no_cache.or(other.no_cache),
            preview: self.preview.or(other.preview),
            cache_dir: self.cache_dir.or(other.cache_dir),
            pip: match (self.pip, other.pip) {
                (Some(a), Some(b)) => Some(a.combine(b)),
                (a, b) => a.or(b),
            },
        }
    }
}

impl Combine for PipOptions {
    fn combine(self, other: PipOptions) -> PipOptions {
        PipOptions {
            // Collection types, which must be merged element-wise.
            extra_index_url: self.extra_index_url.combine(other.extra_index_url),
            find_links: self.find_links.combine(other.find_links),
            no_binary: self.no_binary.combine(other.no_binary),
            only_binary: self.only_binary.combine(other.only_binary),
            extra: self.extra.combine(other.extra),
            config_settings: self.config_settings.combine(other.config_settings),
            no_emit_package: self.no_emit_package.combine(other.no_emit_package),

            // Non-collections, where the last value wins.
            python: self.python.or(other.python),
            system: self.system.or(other.system),
            break_system_packages: self.break_system_packages.or(other.break_system_packages),
            target: self.target.or(other.target),
            offline: self.offline.or(other.offline),
            index_url: self.index_url.or(other.index_url),
            no_index: self.no_index.or(other.no_index),
            index_strategy: self.index_strategy.or(other.index_strategy),
            keyring_provider: self.keyring_provider.or(other.keyring_provider),
            no_build: self.no_build.or(other.no_build),
            no_build_isolation: self.no_build_isolation.or(other.no_build_isolation),
            strict: self.strict.or(other.strict),
            all_extras: self.all_extras.or(other.all_extras),
            no_deps: self.no_deps.or(other.no_deps),
            resolution: self.resolution.or(other.resolution),
            prerelease: self.prerelease.or(other.prerelease),
            output_file: self.output_file.or(other.output_file),
            no_strip_extras: self.no_strip_extras.or(other.no_strip_extras),
            no_annotate: self.no_annotate.or(other.no_annotate),
            no_header: self.no_header.or(other.no_header),
            custom_compile_command: self.custom_compile_command.or(other.custom_compile_command),
            generate_hashes: self.generate_hashes.or(other.generate_hashes),
            legacy_setup_py: self.legacy_setup_py.or(other.legacy_setup_py),
            python_version: self.python_version.or(other.python_version),
            python_platform: self.python_platform.or(other.python_platform),
            exclude_newer: self.exclude_newer.or(other.exclude_newer),
            emit_index_url: self.emit_index_url.or(other.emit_index_url),
            emit_find_links: self.emit_find_links.or(other.emit_find_links),
            emit_marker_expression: self.emit_marker_expression.or(other.emit_marker_expression),
            emit_index_annotation: self.emit_index_annotation.or(other.emit_index_annotation),
            annotation_style: self.annotation_style.or(other.annotation_style),
            link_mode: self.link_mode.or(other.link_mode),
            compile_bytecode: self.compile_bytecode.or(other.compile_bytecode),
            require_hashes: self.require_hashes.or(other.require_hashes),
            concurrent_downloads: self.concurrent_downloads.or(other.concurrent_downloads),
            concurrent_builds: self.concurrent_builds.or(other.concurrent_builds),
        }
    }
}

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
