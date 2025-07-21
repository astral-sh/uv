use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use jiff::Timestamp;
use jiff::civil::{Date, DateTime, Time};
use jiff::tz::{Offset, TimeZone};
use serde::Deserialize;
use toml_edit::{Array, ArrayOfTables, Item, Table, value};
use url::Url;

use uv_cache_key::RepositoryUrl;
use uv_configuration::{
    BuildOptions, DependencyGroupsWithDefaults, EditableMode, ExtrasSpecificationWithDefaults,
    InstallOptions,
};
use uv_distribution_filename::{
    BuildTag, DistExtension, ExtensionError, SourceDistExtension, SourceDistFilename,
    SourceDistFilenameError, WheelFilename, WheelFilenameError,
};
use uv_distribution_types::{
    BuiltDist, DirectUrlBuiltDist, DirectUrlSourceDist, DirectorySourceDist, Dist, Edge,
    FileLocation, GitSourceDist, IndexUrl, Name, Node, PathBuiltDist, PathSourceDist,
    RegistryBuiltDist, RegistryBuiltWheel, RegistrySourceDist, RemoteSource, RequiresPython,
    Resolution, ResolvedDist, SourceDist, ToUrlError, UrlString,
};
use uv_fs::{PortablePathBuf, relative_to};
use uv_git::{RepositoryReference, ResolvedRepositoryReference};
use uv_git_types::{GitOid, GitReference, GitUrl, GitUrlParseError};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_pep508::{MarkerEnvironment, MarkerTree, MarkerVariantsUniversal, VerbatimUrl};
use uv_platform_tags::{TagCompatibility, TagPriority, Tags};
use uv_pypi_types::{HashDigests, Hashes, ParsedGitUrl, VcsKind};
use uv_redacted::DisplaySafeUrl;
use uv_small_str::SmallString;

use crate::lock::export::ExportableRequirements;
use crate::lock::{Source, WheelTagHint, each_element_on_its_line_array};
use crate::resolution::ResolutionGraphNode;
use crate::{Installable, LockError, ResolverOutput};

#[derive(Debug, thiserror::Error)]
pub enum PylockTomlErrorKind {
    #[error(
        "Package `{0}` includes both a registry (`packages.wheels`) and a directory source (`packages.directory`)"
    )]
    WheelWithDirectory(PackageName),
    #[error(
        "Package `{0}` includes both a registry (`packages.wheels`) and a VCS source (`packages.vcs`)"
    )]
    WheelWithVcs(PackageName),
    #[error(
        "Package `{0}` includes both a registry (`packages.wheels`) and an archive source (`packages.archive`)"
    )]
    WheelWithArchive(PackageName),
    #[error(
        "Package `{0}` includes both a registry (`packages.sdist`) and a directory source (`packages.directory`)"
    )]
    SdistWithDirectory(PackageName),
    #[error(
        "Package `{0}` includes both a registry (`packages.sdist`) and a VCS source (`packages.vcs`)"
    )]
    SdistWithVcs(PackageName),
    #[error(
        "Package `{0}` includes both a registry (`packages.sdist`) and an archive source (`packages.archive`)"
    )]
    SdistWithArchive(PackageName),
    #[error(
        "Package `{0}` includes both a directory (`packages.directory`) and a VCS source (`packages.vcs`)"
    )]
    DirectoryWithVcs(PackageName),
    #[error(
        "Package `{0}` includes both a directory (`packages.directory`) and an archive source (`packages.archive`)"
    )]
    DirectoryWithArchive(PackageName),
    #[error(
        "Package `{0}` includes both a VCS (`packages.vcs`) and an archive source (`packages.archive`)"
    )]
    VcsWithArchive(PackageName),
    #[error(
        "Package `{0}` must include one of: `wheels`, `directory`, `archive`, `sdist`, or `vcs`"
    )]
    MissingSource(PackageName),
    #[error("Package `{0}` does not include a compatible wheel for the current platform")]
    MissingWheel(PackageName),
    #[error("`packages.wheel` entry for `{0}` must have a `path` or `url`")]
    WheelMissingPathUrl(PackageName),
    #[error("`packages.sdist` entry for `{0}` must have a `path` or `url`")]
    SdistMissingPathUrl(PackageName),
    #[error("`packages.archive` entry for `{0}` must have a `path` or `url`")]
    ArchiveMissingPathUrl(PackageName),
    #[error("`packages.vcs` entry for `{0}` must have a `url` or `path`")]
    VcsMissingPathUrl(PackageName),
    #[error("URL must end in a valid wheel filename: `{0}`")]
    UrlMissingFilename(DisplaySafeUrl),
    #[error("Path must end in a valid wheel filename: `{0}`")]
    PathMissingFilename(Box<Path>),
    #[error("Failed to convert path to URL")]
    PathToUrl,
    #[error("Failed to convert URL to path")]
    UrlToPath,
    #[error(
        "Package `{0}` can't be installed because it doesn't have a source distribution or wheel for the current platform"
    )]
    NeitherSourceDistNorWheel(PackageName),
    #[error(
        "Package `{0}` can't be installed because it is marked as both `--no-binary` and `--no-build`"
    )]
    NoBinaryNoBuild(PackageName),
    #[error(
        "Package `{0}` can't be installed because it is marked as `--no-binary` but has no source distribution"
    )]
    NoBinary(PackageName),
    #[error(
        "Package `{0}` can't be installed because it is marked as `--no-build` but has no binary distribution"
    )]
    NoBuild(PackageName),
    #[error(
        "Package `{0}` can't be installed because the binary distribution is incompatible with the current platform"
    )]
    IncompatibleWheelOnly(PackageName),
    #[error(
        "Package `{0}` can't be installed because it is marked as `--no-binary` but is itself a binary distribution"
    )]
    NoBinaryWheelOnly(PackageName),
    #[error(transparent)]
    WheelFilename(#[from] WheelFilenameError),
    #[error(transparent)]
    SourceDistFilename(#[from] SourceDistFilenameError),
    #[error(transparent)]
    ToUrl(#[from] ToUrlError),
    #[error(transparent)]
    GitUrlParse(#[from] GitUrlParseError),
    #[error(transparent)]
    LockError(#[from] LockError),
    #[error(transparent)]
    Extension(#[from] ExtensionError),
    #[error(transparent)]
    Jiff(#[from] jiff::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Deserialize(#[from] toml::de::Error),
}

#[derive(Debug)]
pub struct PylockTomlError {
    kind: Box<PylockTomlErrorKind>,
    hint: Option<WheelTagHint>,
}

impl std::error::Error for PylockTomlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.kind.source()
    }
}

impl std::fmt::Display for PylockTomlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(hint) = &self.hint {
            write!(f, "\n\n{hint}")?;
        }
        Ok(())
    }
}

impl<E> From<E> for PylockTomlError
where
    PylockTomlErrorKind: From<E>,
{
    fn from(err: E) -> Self {
        Self {
            kind: Box::new(PylockTomlErrorKind::from(err)),
            hint: None,
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PylockToml {
    lock_version: Version,
    created_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_python: Option<RequiresPython>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub extras: Vec<ExtraName>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub dependency_groups: Vec<GroupName>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub default_groups: Vec<GroupName>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub packages: Vec<PylockTomlPackage>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    attestation_identities: Vec<PylockTomlAttestationIdentity>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PylockTomlPackage {
    pub name: PackageName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<Version>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<DisplaySafeUrl>,
    #[serde(
        skip_serializing_if = "uv_pep508::marker::ser::is_empty",
        serialize_with = "uv_pep508::marker::ser::serialize",
        default
    )]
    marker: MarkerTree,
    #[serde(skip_serializing_if = "Option::is_none")]
    requires_python: Option<RequiresPython>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    dependencies: Vec<PylockTomlDependency>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vcs: Option<PylockTomlVcs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    directory: Option<PylockTomlDirectory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive: Option<PylockTomlArchive>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sdist: Option<PylockTomlSdist>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wheels: Option<Vec<PylockTomlWheel>>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(clippy::empty_structs_with_brackets)]
struct PylockTomlDependency {}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PylockTomlDirectory {
    path: PortablePathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    editable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subdirectory: Option<PortablePathBuf>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PylockTomlVcs {
    r#type: VcsKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<DisplaySafeUrl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<PortablePathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    requested_revision: Option<String>,
    commit_id: GitOid,
    #[serde(skip_serializing_if = "Option::is_none")]
    subdirectory: Option<PortablePathBuf>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PylockTomlArchive {
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<DisplaySafeUrl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<PortablePathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "timestamp_to_toml_datetime",
        deserialize_with = "timestamp_from_toml_datetime",
        default
    )]
    upload_time: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subdirectory: Option<PortablePathBuf>,
    hashes: Hashes,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PylockTomlSdist {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<SmallString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<DisplaySafeUrl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<PortablePathBuf>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "timestamp_to_toml_datetime",
        deserialize_with = "timestamp_from_toml_datetime",
        default
    )]
    upload_time: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    hashes: Hashes,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PylockTomlWheel {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<WheelFilename>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<DisplaySafeUrl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<PortablePathBuf>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "timestamp_to_toml_datetime",
        deserialize_with = "timestamp_from_toml_datetime",
        default
    )]
    upload_time: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    hashes: Hashes,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PylockTomlAttestationIdentity {
    kind: String,
}

impl<'lock> PylockToml {
    /// Construct a [`PylockToml`] from a [`ResolverOutput`].
    pub fn from_resolution(
        resolution: &ResolverOutput,
        omit: &[PackageName],
        install_path: &Path,
    ) -> Result<Self, PylockTomlErrorKind> {
        // The lock version is always `1.0` at time of writing.
        let lock_version = Version::new([1, 0]);

        // The created by field is always `uv` at time of writing.
        let created_by = "uv".to_string();

        // Use the `requires-python` from the target lockfile.
        let requires_python = resolution.requires_python.clone();

        // We don't support locking for multiple extras at time of writing.
        let extras = vec![];

        // We don't support locking for multiple dependency groups at time of writing.
        let dependency_groups = vec![];

        // We don't support locking for multiple dependency groups at time of writing.
        let default_groups = vec![];

        // We don't support attestation identities at time of writing.
        let attestation_identities = vec![];

        // Convert each node to a `pylock.toml`-style package.
        let mut packages = Vec::with_capacity(resolution.graph.node_count());
        for node_index in resolution.graph.node_indices() {
            let ResolutionGraphNode::Dist(node) = &resolution.graph[node_index] else {
                continue;
            };
            if !node.is_base() {
                continue;
            }
            let ResolvedDist::Installable { dist, version, .. } = &node.dist else {
                continue;
            };
            if omit.contains(dist.name()) {
                continue;
            }

            // "The version MUST NOT be included when it cannot be guaranteed to be consistent with the code used (i.e. when a source tree is used)."
            let version = version
                .as_ref()
                .filter(|_| !matches!(&**dist, Dist::Source(SourceDist::Directory(..))));

            // Create a `pylock.toml`-style package.
            let mut package = PylockTomlPackage {
                name: dist.name().clone(),
                version: version.cloned(),
                marker: node.marker.pep508(),
                requires_python: None,
                dependencies: vec![],
                index: None,
                vcs: None,
                directory: None,
                archive: None,
                sdist: None,
                wheels: None,
            };

            match &**dist {
                Dist::Built(BuiltDist::DirectUrl(dist)) => {
                    package.archive = Some(PylockTomlArchive {
                        url: Some((*dist.location).clone()),
                        path: None,
                        size: dist.size(),
                        upload_time: None,
                        subdirectory: None,
                        hashes: Hashes::from(node.hashes.clone()),
                    });
                }
                Dist::Built(BuiltDist::Path(dist)) => {
                    let path = relative_to(&dist.install_path, install_path)
                        .map(Box::<Path>::from)
                        .unwrap_or_else(|_| dist.install_path.clone());
                    package.archive = Some(PylockTomlArchive {
                        url: None,
                        path: Some(PortablePathBuf::from(path)),
                        size: dist.size(),
                        upload_time: None,
                        subdirectory: None,
                        hashes: Hashes::from(node.hashes.clone()),
                    });
                }
                Dist::Built(BuiltDist::Registry(dist)) => {
                    package.wheels = Some(
                        dist.wheels
                            .iter()
                            .map(|wheel| {
                                let url = wheel
                                    .file
                                    .url
                                    .to_url()
                                    .map_err(PylockTomlErrorKind::ToUrl)?;
                                Ok(PylockTomlWheel {
                                    // Optional "when the last component of path/ url would be the same value".
                                    name: if url
                                        .filename()
                                        .is_ok_and(|filename| filename == *wheel.file.filename)
                                    {
                                        None
                                    } else {
                                        Some(wheel.filename.clone())
                                    },
                                    upload_time: wheel
                                        .file
                                        .upload_time_utc_ms
                                        .map(Timestamp::from_millisecond)
                                        .transpose()?,
                                    url: Some(
                                        wheel
                                            .file
                                            .url
                                            .to_url()
                                            .map_err(PylockTomlErrorKind::ToUrl)?,
                                    ),
                                    path: None,
                                    size: wheel.file.size,
                                    hashes: Hashes::from(wheel.file.hashes.clone()),
                                })
                            })
                            .collect::<Result<Vec<_>, PylockTomlErrorKind>>()?,
                    );

                    if let Some(sdist) = dist.sdist.as_ref() {
                        let url = sdist
                            .file
                            .url
                            .to_url()
                            .map_err(PylockTomlErrorKind::ToUrl)?;
                        package.sdist = Some(PylockTomlSdist {
                            // Optional "when the last component of path/ url would be the same value".
                            name: if url
                                .filename()
                                .is_ok_and(|filename| filename == *sdist.file.filename)
                            {
                                None
                            } else {
                                Some(sdist.file.filename.clone())
                            },
                            upload_time: sdist
                                .file
                                .upload_time_utc_ms
                                .map(Timestamp::from_millisecond)
                                .transpose()?,
                            url: Some(url),
                            path: None,
                            size: sdist.file.size,
                            hashes: Hashes::from(sdist.file.hashes.clone()),
                        });
                    }
                }
                Dist::Source(SourceDist::DirectUrl(dist)) => {
                    package.archive = Some(PylockTomlArchive {
                        url: Some((*dist.location).clone()),
                        path: None,
                        size: dist.size(),
                        upload_time: None,
                        subdirectory: dist.subdirectory.clone().map(PortablePathBuf::from),
                        hashes: Hashes::from(node.hashes.clone()),
                    });
                }
                Dist::Source(SourceDist::Directory(dist)) => {
                    let path = relative_to(&dist.install_path, install_path)
                        .map(Box::<Path>::from)
                        .unwrap_or_else(|_| dist.install_path.clone());
                    package.directory = Some(PylockTomlDirectory {
                        path: PortablePathBuf::from(path),
                        editable: dist.editable,
                        subdirectory: None,
                    });
                }
                Dist::Source(SourceDist::Git(dist)) => {
                    package.vcs = Some(PylockTomlVcs {
                        r#type: VcsKind::Git,
                        url: Some(dist.git.repository().clone()),
                        path: None,
                        requested_revision: dist.git.reference().as_str().map(ToString::to_string),
                        commit_id: dist.git.precise().unwrap_or_else(|| {
                            panic!("Git distribution is missing a precise hash: {dist}")
                        }),
                        subdirectory: dist.subdirectory.clone().map(PortablePathBuf::from),
                    });
                }
                Dist::Source(SourceDist::Path(dist)) => {
                    let path = relative_to(&dist.install_path, install_path)
                        .map(Box::<Path>::from)
                        .unwrap_or_else(|_| dist.install_path.clone());
                    package.archive = Some(PylockTomlArchive {
                        url: None,
                        path: Some(PortablePathBuf::from(path)),
                        size: dist.size(),
                        upload_time: None,
                        subdirectory: None,
                        hashes: Hashes::from(node.hashes.clone()),
                    });
                }
                Dist::Source(SourceDist::Registry(dist)) => {
                    package.wheels = Some(
                        dist.wheels
                            .iter()
                            .map(|wheel| {
                                let url = wheel
                                    .file
                                    .url
                                    .to_url()
                                    .map_err(PylockTomlErrorKind::ToUrl)?;
                                Ok(PylockTomlWheel {
                                    // Optional "when the last component of path/ url would be the same value".
                                    name: if url
                                        .filename()
                                        .is_ok_and(|filename| filename == *wheel.file.filename)
                                    {
                                        None
                                    } else {
                                        Some(wheel.filename.clone())
                                    },
                                    upload_time: wheel
                                        .file
                                        .upload_time_utc_ms
                                        .map(Timestamp::from_millisecond)
                                        .transpose()?,
                                    url: Some(
                                        wheel
                                            .file
                                            .url
                                            .to_url()
                                            .map_err(PylockTomlErrorKind::ToUrl)?,
                                    ),
                                    path: None,
                                    size: wheel.file.size,
                                    hashes: Hashes::from(wheel.file.hashes.clone()),
                                })
                            })
                            .collect::<Result<Vec<_>, PylockTomlErrorKind>>()?,
                    );

                    let url = dist.file.url.to_url().map_err(PylockTomlErrorKind::ToUrl)?;
                    package.sdist = Some(PylockTomlSdist {
                        // Optional "when the last component of path/ url would be the same value".
                        name: if url
                            .filename()
                            .is_ok_and(|filename| filename == *dist.file.filename)
                        {
                            None
                        } else {
                            Some(dist.file.filename.clone())
                        },
                        upload_time: dist
                            .file
                            .upload_time_utc_ms
                            .map(Timestamp::from_millisecond)
                            .transpose()?,
                        url: Some(url),
                        path: None,
                        size: dist.file.size,
                        hashes: Hashes::from(dist.file.hashes.clone()),
                    });
                }
            }

            // Add the package to the list of packages.
            packages.push(package);
        }

        // Sort the packages by name, then version.
        packages.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));

        // Return the constructed `pylock.toml`.
        Ok(Self {
            lock_version,
            created_by,
            requires_python: Some(requires_python),
            extras,
            dependency_groups,
            default_groups,
            packages,
            attestation_identities,
        })
    }

    /// Construct a [`PylockToml`] from a uv lockfile.
    pub fn from_lock(
        target: &impl Installable<'lock>,
        prune: &[PackageName],
        extras: &ExtrasSpecificationWithDefaults,
        dev: &DependencyGroupsWithDefaults,
        annotate: bool,
        editable: Option<EditableMode>,
        install_options: &'lock InstallOptions,
    ) -> Result<Self, PylockTomlErrorKind> {
        // Extract the packages from the lock file.
        let ExportableRequirements(mut nodes) = ExportableRequirements::from_lock(
            target,
            prune,
            extras,
            dev,
            annotate,
            install_options,
        );

        // Sort the nodes.
        nodes.sort_unstable_by_key(|node| &node.package.id);

        // The lock version is always `1.0` at time of writing.
        let lock_version = Version::new([1, 0]);

        // The created by field is always `uv` at time of writing.
        let created_by = "uv".to_string();

        // Use the `requires-python` from the target lockfile.
        let requires_python = target.lock().requires_python.clone();

        // We don't support locking for multiple extras at time of writing.
        let extras = vec![];

        // We don't support locking for multiple dependency groups at time of writing.
        let dependency_groups = vec![];

        // We don't support locking for multiple dependency groups at time of writing.
        let default_groups = vec![];

        // We don't support attestation identities at time of writing.
        let attestation_identities = vec![];

        // Convert each node to a `pylock.toml`-style package.
        let mut packages = Vec::with_capacity(nodes.len());
        for node in nodes {
            let package = node.package;

            // Extract the `packages.wheels` field.
            //
            // This field only includes wheels from a registry. Wheels included via direct URL or
            // direct path instead map to the `packages.archive` field.
            let wheels = match &package.id.source {
                Source::Registry(source) => {
                    let wheels = package
                        .wheels
                        .iter()
                        .map(|wheel| wheel.to_registry_wheel(source, target.install_path()))
                        .collect::<Result<Vec<RegistryBuiltWheel>, LockError>>()?;
                    Some(
                        wheels
                            .into_iter()
                            .map(|wheel| {
                                let url = wheel
                                    .file
                                    .url
                                    .to_url()
                                    .map_err(PylockTomlErrorKind::ToUrl)?;
                                Ok(PylockTomlWheel {
                                    // Optional "when the last component of path/ url would be the same value".
                                    name: if url
                                        .filename()
                                        .is_ok_and(|filename| filename == *wheel.file.filename)
                                    {
                                        None
                                    } else {
                                        Some(wheel.filename.clone())
                                    },
                                    upload_time: wheel
                                        .file
                                        .upload_time_utc_ms
                                        .map(Timestamp::from_millisecond)
                                        .transpose()?,
                                    url: Some(url),
                                    path: None,
                                    size: wheel.file.size,
                                    hashes: Hashes::from(wheel.file.hashes),
                                })
                            })
                            .collect::<Result<Vec<_>, PylockTomlErrorKind>>()?,
                    )
                }
                Source::Path(..) => None,
                Source::Git(..) => None,
                Source::Direct(..) => None,
                Source::Directory(..) => None,
                Source::Editable(..) => None,
                Source::Virtual(..) => {
                    // Omit virtual packages entirely; they shouldn't be installed.
                    continue;
                }
            };

            // Extract the source distribution from the lockfile entry.
            let sdist = package.to_source_dist(target.install_path())?;

            // Extract some common fields from the source distribution.
            let size = package
                .sdist
                .as_ref()
                .and_then(super::super::SourceDist::size);
            let hash = package.sdist.as_ref().and_then(|sdist| sdist.hash());

            // Extract the `packages.directory` field.
            let directory = match &sdist {
                Some(SourceDist::Directory(sdist)) => Some(PylockTomlDirectory {
                    path: PortablePathBuf::from(
                        relative_to(&sdist.install_path, target.install_path())
                            .unwrap_or_else(|_| sdist.install_path.to_path_buf())
                            .into_boxed_path(),
                    ),
                    editable: match editable {
                        None => sdist.editable,
                        Some(EditableMode::NonEditable) => None,
                        Some(EditableMode::Editable) => Some(true),
                    },
                    subdirectory: None,
                }),
                _ => None,
            };

            // Extract the `packages.vcs` field.
            let vcs = match &sdist {
                Some(SourceDist::Git(sdist)) => Some(PylockTomlVcs {
                    r#type: VcsKind::Git,
                    url: Some(sdist.git.repository().clone()),
                    path: None,
                    requested_revision: sdist.git.reference().as_str().map(ToString::to_string),
                    commit_id: sdist.git.precise().unwrap_or_else(|| {
                        panic!("Git distribution is missing a precise hash: {sdist}")
                    }),
                    subdirectory: sdist.subdirectory.clone().map(PortablePathBuf::from),
                }),
                _ => None,
            };

            // Extract the `packages.archive` field, which can either be a direct URL or a local
            // path, pointing to either a source distribution or a wheel.
            let archive = match &sdist {
                Some(SourceDist::DirectUrl(sdist)) => Some(PylockTomlArchive {
                    url: Some(sdist.url.to_url()),
                    path: None,
                    size,
                    upload_time: None,
                    subdirectory: sdist.subdirectory.clone().map(PortablePathBuf::from),
                    hashes: hash.cloned().map(Hashes::from).unwrap_or_default(),
                }),
                Some(SourceDist::Path(sdist)) => Some(PylockTomlArchive {
                    url: None,
                    path: Some(PortablePathBuf::from(
                        relative_to(&sdist.install_path, target.install_path())
                            .unwrap_or_else(|_| sdist.install_path.to_path_buf())
                            .into_boxed_path(),
                    )),
                    size,
                    upload_time: None,
                    subdirectory: None,
                    hashes: hash.cloned().map(Hashes::from).unwrap_or_default(),
                }),
                _ => match &package.id.source {
                    Source::Registry(..) => None,
                    Source::Path(source) => package.wheels.first().map(|wheel| PylockTomlArchive {
                        url: None,
                        path: Some(PortablePathBuf::from(
                            relative_to(source, target.install_path())
                                .unwrap_or_else(|_| source.to_path_buf())
                                .into_boxed_path(),
                        )),
                        size: wheel.size,
                        upload_time: None,
                        subdirectory: None,
                        hashes: wheel.hash.clone().map(Hashes::from).unwrap_or_default(),
                    }),
                    Source::Git(..) => None,
                    Source::Direct(source, ..) => {
                        if let Some(wheel) = package.wheels.first() {
                            Some(PylockTomlArchive {
                                url: Some(source.to_url()?),
                                path: None,
                                size: wheel.size,
                                upload_time: None,
                                subdirectory: None,
                                hashes: wheel.hash.clone().map(Hashes::from).unwrap_or_default(),
                            })
                        } else {
                            None
                        }
                    }
                    Source::Directory(..) => None,
                    Source::Editable(..) => None,
                    Source::Virtual(..) => None,
                },
            };

            // Extract the `packages.sdist` field.
            let sdist = match &sdist {
                Some(SourceDist::Registry(sdist)) => {
                    let url = sdist
                        .file
                        .url
                        .to_url()
                        .map_err(PylockTomlErrorKind::ToUrl)?;
                    Some(PylockTomlSdist {
                        // Optional "when the last component of path/ url would be the same value".
                        name: if url
                            .filename()
                            .is_ok_and(|filename| filename == *sdist.file.filename)
                        {
                            None
                        } else {
                            Some(sdist.file.filename.clone())
                        },
                        upload_time: sdist
                            .file
                            .upload_time_utc_ms
                            .map(Timestamp::from_millisecond)
                            .transpose()?,
                        url: Some(url),
                        path: None,
                        size,
                        hashes: hash.cloned().map(Hashes::from).unwrap_or_default(),
                    })
                }
                _ => None,
            };

            // Extract the `packages.index` field.
            let index = package
                .index(target.install_path())?
                .map(IndexUrl::into_url);

            // Extract the `packages.name` field.
            let name = package.id.name.clone();

            // Extract the `packages.version` field.
            // "The version MUST NOT be included when it cannot be guaranteed to be consistent with the code used (i.e. when a source tree is used)."
            let version = package
                .id
                .version
                .as_ref()
                .filter(|_| directory.is_none())
                .cloned();

            let package = PylockTomlPackage {
                name,
                version,
                marker: node.marker,
                requires_python: None,
                dependencies: vec![],
                index,
                vcs,
                directory,
                archive,
                sdist,
                wheels,
            };

            packages.push(package);
        }

        Ok(Self {
            lock_version,
            created_by,
            requires_python: Some(requires_python),
            extras,
            dependency_groups,
            default_groups,
            packages,
            attestation_identities,
        })
    }

    /// Returns the TOML representation of this lockfile.
    pub fn to_toml(&self) -> Result<String, toml_edit::ser::Error> {
        // We construct a TOML document manually instead of going through Serde to enable
        // the use of inline tables.
        let mut doc = toml_edit::DocumentMut::new();

        doc.insert("lock-version", value(self.lock_version.to_string()));
        doc.insert("created-by", value(self.created_by.to_string()));
        if let Some(ref requires_python) = self.requires_python {
            doc.insert("requires-python", value(requires_python.to_string()));
        }
        if !self.extras.is_empty() {
            doc.insert(
                "extras",
                value(each_element_on_its_line_array(
                    self.extras.iter().map(ToString::to_string),
                )),
            );
        }
        if !self.dependency_groups.is_empty() {
            doc.insert(
                "dependency-groups",
                value(each_element_on_its_line_array(
                    self.dependency_groups.iter().map(ToString::to_string),
                )),
            );
        }
        if !self.default_groups.is_empty() {
            doc.insert(
                "default-groups",
                value(each_element_on_its_line_array(
                    self.default_groups.iter().map(ToString::to_string),
                )),
            );
        }
        if !self.attestation_identities.is_empty() {
            let attestation_identities = self
                .attestation_identities
                .iter()
                .map(|attestation_identity| {
                    serde::Serialize::serialize(
                        &attestation_identity,
                        toml_edit::ser::ValueSerializer::new(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let attestation_identities = match attestation_identities.as_slice() {
                [] => Array::new(),
                [attestation_identity] => Array::from_iter([attestation_identity]),
                attestation_identities => {
                    each_element_on_its_line_array(attestation_identities.iter())
                }
            };
            doc.insert("attestation-identities", value(attestation_identities));
        }
        if !self.packages.is_empty() {
            let mut packages = ArrayOfTables::new();
            for dist in &self.packages {
                packages.push(dist.to_toml()?);
            }
            doc.insert("packages", Item::ArrayOfTables(packages));
        }

        Ok(doc.to_string())
    }

    /// Convert the [`PylockToml`] to a [`Resolution`].
    pub fn to_resolution(
        self,
        install_path: &Path,
        markers: &MarkerEnvironment,
        extras: &[ExtraName],
        groups: &[GroupName],
        tags: &Tags,
        build_options: &BuildOptions,
    ) -> Result<Resolution, PylockTomlError> {
        // Convert the extras and dependency groups specifications to a concrete environment.
        let mut graph =
            petgraph::graph::DiGraph::with_capacity(self.packages.len(), self.packages.len());

        // Add the root node.
        let root = graph.add_node(Node::Root);

        for package in self.packages {
            // Omit packages that aren't relevant to the current environment.
            if !package
                .marker
                .evaluate_pep751(markers, MarkerVariantsUniversal, extras, groups)
            {
                continue;
            }

            match (
                package.wheels.is_some(),
                package.sdist.is_some(),
                package.directory.is_some(),
                package.vcs.is_some(),
                package.archive.is_some(),
            ) {
                // `packages.wheels` is mutually exclusive with `packages.directory`, `packages.vcs`, and `packages.archive`.
                (true, _, true, _, _) => {
                    return Err(
                        PylockTomlErrorKind::WheelWithDirectory(package.name.clone()).into(),
                    );
                }
                (true, _, _, true, _) => {
                    return Err(PylockTomlErrorKind::WheelWithVcs(package.name.clone()).into());
                }
                (true, _, _, _, true) => {
                    return Err(PylockTomlErrorKind::WheelWithArchive(package.name.clone()).into());
                }
                // `packages.sdist` is mutually exclusive with `packages.directory`, `packages.vcs`, and `packages.archive`.
                (_, true, true, _, _) => {
                    return Err(
                        PylockTomlErrorKind::SdistWithDirectory(package.name.clone()).into(),
                    );
                }
                (_, true, _, true, _) => {
                    return Err(PylockTomlErrorKind::SdistWithVcs(package.name.clone()).into());
                }
                (_, true, _, _, true) => {
                    return Err(PylockTomlErrorKind::SdistWithArchive(package.name.clone()).into());
                }
                // `packages.directory` is mutually exclusive with `packages.vcs`, and `packages.archive`.
                (_, _, true, true, _) => {
                    return Err(PylockTomlErrorKind::DirectoryWithVcs(package.name.clone()).into());
                }
                (_, _, true, _, true) => {
                    return Err(
                        PylockTomlErrorKind::DirectoryWithArchive(package.name.clone()).into(),
                    );
                }
                // `packages.vcs` is mutually exclusive with `packages.archive`.
                (_, _, _, true, true) => {
                    return Err(PylockTomlErrorKind::VcsWithArchive(package.name.clone()).into());
                }
                (false, false, false, false, false) => {
                    return Err(PylockTomlErrorKind::MissingSource(package.name.clone()).into());
                }
                _ => {}
            }

            let no_binary = build_options.no_binary_package(&package.name);
            let no_build = build_options.no_build_package(&package.name);
            let is_wheel = package
                .archive
                .as_ref()
                .map(|archive| archive.is_wheel(&package.name))
                .transpose()?
                .unwrap_or_default();

            // Search for a matching wheel.
            let dist = if let Some(best_wheel) =
                package.find_best_wheel(tags).filter(|_| !no_binary)
            {
                let hashes = HashDigests::from(best_wheel.hashes.clone());
                let built_dist = Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                    wheels: vec![best_wheel.to_registry_wheel(
                        install_path,
                        &package.name,
                        package.index.as_ref(),
                    )?],
                    best_wheel_index: 0,
                    sdist: None,
                }));
                let dist = ResolvedDist::Installable {
                    dist: Arc::new(built_dist),
                    variants_json: None,
                    version: package.version,
                };
                Node::Dist {
                    dist,
                    hashes,
                    install: true,
                }
            } else if let Some(sdist) = package.sdist.as_ref().filter(|_| !no_build) {
                let hashes = HashDigests::from(sdist.hashes.clone());
                let sdist = Dist::Source(SourceDist::Registry(sdist.to_sdist(
                    install_path,
                    &package.name,
                    package.version.as_ref(),
                    package.index.as_ref(),
                )?));
                let dist = ResolvedDist::Installable {
                    dist: Arc::new(sdist),
                    variants_json: None,
                    version: package.version,
                };
                Node::Dist {
                    dist,
                    hashes,
                    install: true,
                }
            } else if let Some(sdist) = package.directory.as_ref().filter(|_| !no_build) {
                let hashes = HashDigests::empty();
                let sdist = Dist::Source(SourceDist::Directory(
                    sdist.to_sdist(install_path, &package.name)?,
                ));
                let dist = ResolvedDist::Installable {
                    dist: Arc::new(sdist),
                    variants_json: None,
                    version: package.version,
                };
                Node::Dist {
                    dist,
                    hashes,
                    install: true,
                }
            } else if let Some(sdist) = package.vcs.as_ref().filter(|_| !no_build) {
                let hashes = HashDigests::empty();
                let sdist = Dist::Source(SourceDist::Git(
                    sdist.to_sdist(install_path, &package.name)?,
                ));
                let dist = ResolvedDist::Installable {
                    dist: Arc::new(sdist),
                    variants_json: None,
                    version: package.version,
                };
                Node::Dist {
                    dist,
                    hashes,
                    install: true,
                }
            } else if let Some(dist) = package
                .archive
                .as_ref()
                .filter(|_| if is_wheel { !no_binary } else { !no_build })
            {
                let hashes = HashDigests::from(dist.hashes.clone());
                let dist = dist.to_dist(install_path, &package.name, package.version.as_ref())?;
                let dist = ResolvedDist::Installable {
                    dist: Arc::new(dist),
                    variants_json: None,
                    version: package.version,
                };
                Node::Dist {
                    dist,
                    hashes,
                    install: true,
                }
            } else {
                return match (no_binary, no_build) {
                    (true, true) => {
                        Err(PylockTomlErrorKind::NoBinaryNoBuild(package.name.clone()).into())
                    }
                    (true, false) if is_wheel => {
                        Err(PylockTomlErrorKind::NoBinaryWheelOnly(package.name.clone()).into())
                    }
                    (true, false) => {
                        Err(PylockTomlErrorKind::NoBinary(package.name.clone()).into())
                    }
                    (false, true) => Err(PylockTomlErrorKind::NoBuild(package.name.clone()).into()),
                    (false, false) if is_wheel => Err(PylockTomlError {
                        kind: Box::new(PylockTomlErrorKind::IncompatibleWheelOnly(
                            package.name.clone(),
                        )),
                        hint: package.tag_hint(tags),
                    }),
                    (false, false) => Err(PylockTomlError {
                        kind: Box::new(PylockTomlErrorKind::NeitherSourceDistNorWheel(
                            package.name.clone(),
                        )),
                        hint: package.tag_hint(tags),
                    }),
                };
            };

            let index = graph.add_node(dist);
            graph.add_edge(root, index, Edge::Prod);
        }

        Ok(Resolution::new(graph))
    }
}

impl PylockTomlPackage {
    /// Convert the [`PylockTomlPackage`] to a TOML [`Table`].
    fn to_toml(&self) -> Result<Table, toml_edit::ser::Error> {
        let mut table = Table::new();
        table.insert("name", value(self.name.to_string()));
        if let Some(ref version) = self.version {
            table.insert("version", value(version.to_string()));
        }
        if let Some(marker) = self.marker.try_to_string() {
            table.insert("marker", value(marker));
        }
        if let Some(ref requires_python) = self.requires_python {
            table.insert("requires-python", value(requires_python.to_string()));
        }
        if !self.dependencies.is_empty() {
            let dependencies = self
                .dependencies
                .iter()
                .map(|dependency| {
                    serde::Serialize::serialize(&dependency, toml_edit::ser::ValueSerializer::new())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let dependencies = match dependencies.as_slice() {
                [] => Array::new(),
                [dependency] => Array::from_iter([dependency]),
                dependencies => each_element_on_its_line_array(dependencies.iter()),
            };
            table.insert("dependencies", value(dependencies));
        }
        if let Some(ref index) = self.index {
            table.insert("index", value(index.to_string()));
        }
        if let Some(ref vcs) = self.vcs {
            table.insert(
                "vcs",
                value(serde::Serialize::serialize(
                    &vcs,
                    toml_edit::ser::ValueSerializer::new(),
                )?),
            );
        }
        if let Some(ref directory) = self.directory {
            table.insert(
                "directory",
                value(serde::Serialize::serialize(
                    &directory,
                    toml_edit::ser::ValueSerializer::new(),
                )?),
            );
        }
        if let Some(ref archive) = self.archive {
            table.insert(
                "archive",
                value(serde::Serialize::serialize(
                    &archive,
                    toml_edit::ser::ValueSerializer::new(),
                )?),
            );
        }
        if let Some(ref sdist) = self.sdist {
            table.insert(
                "sdist",
                value(serde::Serialize::serialize(
                    &sdist,
                    toml_edit::ser::ValueSerializer::new(),
                )?),
            );
        }
        if let Some(wheels) = self.wheels.as_ref().filter(|wheels| !wheels.is_empty()) {
            let wheels = wheels
                .iter()
                .map(|wheel| {
                    serde::Serialize::serialize(wheel, toml_edit::ser::ValueSerializer::new())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let wheels = match wheels.as_slice() {
                [] => Array::new(),
                [wheel] => Array::from_iter([wheel]),
                wheels => each_element_on_its_line_array(wheels.iter()),
            };
            table.insert("wheels", value(wheels));
        }

        Ok(table)
    }

    /// Return the index of the best wheel for the given tags.
    fn find_best_wheel(&self, tags: &Tags) -> Option<&PylockTomlWheel> {
        type WheelPriority = (TagPriority, Option<BuildTag>);

        let mut best: Option<(WheelPriority, &PylockTomlWheel)> = None;
        for wheel in self.wheels.iter().flatten() {
            let Ok(filename) = wheel.filename(&self.name) else {
                continue;
            };
            let TagCompatibility::Compatible(tag_priority) = filename.compatibility(tags) else {
                continue;
            };
            let build_tag = filename.build_tag().cloned();
            let wheel_priority = (tag_priority, build_tag);
            match &best {
                None => {
                    best = Some((wheel_priority, wheel));
                }
                Some((best_priority, _)) => {
                    if wheel_priority > *best_priority {
                        best = Some((wheel_priority, wheel));
                    }
                }
            }
        }

        best.map(|(_, i)| i)
    }

    /// Generate a [`WheelTagHint`] based on wheel-tag incompatibilities.
    fn tag_hint(&self, tags: &Tags) -> Option<WheelTagHint> {
        let filenames = self
            .wheels
            .iter()
            .flatten()
            .filter_map(|wheel| wheel.filename(&self.name).ok())
            .collect::<Vec<_>>();
        let filenames = filenames.iter().map(Cow::as_ref).collect::<Vec<_>>();
        WheelTagHint::from_wheels(&self.name, self.version.as_ref(), &filenames, tags)
    }

    /// Returns the [`ResolvedRepositoryReference`] for the package, if it is a Git source.
    pub fn as_git_ref(&self) -> Option<ResolvedRepositoryReference> {
        let vcs = self.vcs.as_ref()?;
        let url = vcs.url.as_ref()?;
        let requested_revision = vcs.requested_revision.as_ref()?;
        Some(ResolvedRepositoryReference {
            reference: RepositoryReference {
                url: RepositoryUrl::new(url),
                reference: GitReference::from_rev(requested_revision.clone()),
            },
            sha: vcs.commit_id,
        })
    }
}

impl PylockTomlWheel {
    /// Return the [`WheelFilename`] for this wheel.
    fn filename(&self, name: &PackageName) -> Result<Cow<'_, WheelFilename>, PylockTomlErrorKind> {
        if let Some(name) = self.name.as_ref() {
            Ok(Cow::Borrowed(name))
        } else if let Some(path) = self.path.as_ref() {
            let Some(filename) = path.as_ref().file_name().and_then(OsStr::to_str) else {
                return Err(PylockTomlErrorKind::PathMissingFilename(Box::<Path>::from(
                    path.clone(),
                )));
            };
            let filename = WheelFilename::from_str(filename).map(Cow::Owned)?;
            Ok(filename)
        } else if let Some(url) = self.url.as_ref() {
            let Some(filename) = url.filename().ok() else {
                return Err(PylockTomlErrorKind::UrlMissingFilename(url.clone()));
            };
            let filename = WheelFilename::from_str(&filename).map(Cow::Owned)?;
            Ok(filename)
        } else {
            Err(PylockTomlErrorKind::WheelMissingPathUrl(name.clone()))
        }
    }

    /// Convert the wheel to a [`RegistryBuiltWheel`].
    fn to_registry_wheel(
        &self,
        install_path: &Path,
        name: &PackageName,
        index: Option<&DisplaySafeUrl>,
    ) -> Result<RegistryBuiltWheel, PylockTomlErrorKind> {
        let filename = self.filename(name)?.into_owned();

        let file_url = if let Some(url) = self.url.as_ref() {
            UrlString::from(url)
        } else if let Some(path) = self.path.as_ref() {
            let path = install_path.join(path);
            let url = DisplaySafeUrl::from_file_path(path)
                .map_err(|()| PylockTomlErrorKind::PathToUrl)?;
            UrlString::from(url)
        } else {
            return Err(PylockTomlErrorKind::WheelMissingPathUrl(name.clone()));
        };

        let index = if let Some(index) = index {
            IndexUrl::from(VerbatimUrl::from_url(index.clone()))
        } else {
            // Including the index is only a SHOULD in PEP 751. If it's omitted, we treat the
            // URL (less the filename) as the index. This isn't correct, but it's the best we can
            // do. In practice, the only effect here should be that we cache the wheel under a hash
            // of this URL (since we cache under the hash of the index).
            let mut index = file_url.to_url().map_err(PylockTomlErrorKind::ToUrl)?;
            index.path_segments_mut().unwrap().pop();
            IndexUrl::from(VerbatimUrl::from_url(index))
        };

        let file = Box::new(uv_distribution_types::File {
            dist_info_metadata: false,
            filename: SmallString::from(filename.to_string()),
            hashes: HashDigests::from(self.hashes.clone()),
            requires_python: None,
            size: self.size,
            upload_time_utc_ms: self.upload_time.map(Timestamp::as_millisecond),
            url: FileLocation::AbsoluteUrl(file_url),
            yanked: None,
            zstd: None,
        });

        Ok(RegistryBuiltWheel {
            filename,
            file,
            index,
        })
    }
}

impl PylockTomlDirectory {
    /// Convert the sdist to a [`DirectorySourceDist`].
    fn to_sdist(
        &self,
        install_path: &Path,
        name: &PackageName,
    ) -> Result<DirectorySourceDist, PylockTomlErrorKind> {
        let path = if let Some(subdirectory) = self.subdirectory.as_ref() {
            install_path.join(&self.path).join(subdirectory)
        } else {
            install_path.join(&self.path)
        };
        let path = uv_fs::normalize_path_buf(path);
        let url =
            VerbatimUrl::from_normalized_path(&path).map_err(|_| PylockTomlErrorKind::PathToUrl)?;
        Ok(DirectorySourceDist {
            name: name.clone(),
            install_path: path.into_boxed_path(),
            editable: self.editable,
            r#virtual: Some(false),
            url,
        })
    }
}

impl PylockTomlVcs {
    /// Convert the sdist to a [`GitSourceDist`].
    fn to_sdist(
        &self,
        install_path: &Path,
        name: &PackageName,
    ) -> Result<GitSourceDist, PylockTomlErrorKind> {
        let subdirectory = self.subdirectory.clone().map(Box::<Path>::from);

        // Reconstruct the `GitUrl` from the individual fields.
        let git_url = {
            let mut url = if let Some(url) = self.url.as_ref() {
                url.clone()
            } else if let Some(path) = self.path.as_ref() {
                DisplaySafeUrl::from(
                    Url::from_directory_path(install_path.join(path))
                        .map_err(|()| PylockTomlErrorKind::PathToUrl)?,
                )
            } else {
                return Err(PylockTomlErrorKind::VcsMissingPathUrl(name.clone()));
            };
            url.set_fragment(None);
            url.set_query(None);

            let reference = self
                .requested_revision
                .clone()
                .map(GitReference::from_rev)
                .unwrap_or_else(|| GitReference::BranchOrTagOrCommit(self.commit_id.to_string()));
            let precise = self.commit_id;

            GitUrl::from_commit(url, reference, precise)?
        };

        // Reconstruct the PEP 508-compatible URL from the `GitSource`.
        let url = DisplaySafeUrl::from(ParsedGitUrl {
            url: git_url.clone(),
            subdirectory: subdirectory.clone(),
        });

        Ok(GitSourceDist {
            name: name.clone(),
            git: Box::new(git_url),
            subdirectory: self.subdirectory.clone().map(Box::<Path>::from),
            url: VerbatimUrl::from_url(url),
        })
    }
}

impl PylockTomlSdist {
    /// Return the filename for this sdist.
    fn filename(&self, name: &PackageName) -> Result<Cow<'_, SmallString>, PylockTomlErrorKind> {
        if let Some(name) = self.name.as_ref() {
            Ok(Cow::Borrowed(name))
        } else if let Some(path) = self.path.as_ref() {
            let Some(filename) = path.as_ref().file_name().and_then(OsStr::to_str) else {
                return Err(PylockTomlErrorKind::PathMissingFilename(Box::<Path>::from(
                    path.clone(),
                )));
            };
            Ok(Cow::Owned(SmallString::from(filename)))
        } else if let Some(url) = self.url.as_ref() {
            let Some(filename) = url.filename().ok() else {
                return Err(PylockTomlErrorKind::UrlMissingFilename(url.clone()));
            };
            Ok(Cow::Owned(SmallString::from(filename)))
        } else {
            Err(PylockTomlErrorKind::SdistMissingPathUrl(name.clone()))
        }
    }

    /// Convert the sdist to a [`RegistrySourceDist`].
    fn to_sdist(
        &self,
        install_path: &Path,
        name: &PackageName,
        version: Option<&Version>,
        index: Option<&DisplaySafeUrl>,
    ) -> Result<RegistrySourceDist, PylockTomlErrorKind> {
        let filename = self.filename(name)?.into_owned();
        let ext = SourceDistExtension::from_path(filename.as_ref())?;

        let version = if let Some(version) = version {
            Cow::Borrowed(version)
        } else {
            let filename = SourceDistFilename::parse(&filename, ext, name)?;
            Cow::Owned(filename.version)
        };

        let file_url = if let Some(url) = self.url.as_ref() {
            UrlString::from(url)
        } else if let Some(path) = self.path.as_ref() {
            let path = install_path.join(path);
            let url = DisplaySafeUrl::from_file_path(path)
                .map_err(|()| PylockTomlErrorKind::PathToUrl)?;
            UrlString::from(url)
        } else {
            return Err(PylockTomlErrorKind::SdistMissingPathUrl(name.clone()));
        };

        let index = if let Some(index) = index {
            IndexUrl::from(VerbatimUrl::from_url(index.clone()))
        } else {
            // Including the index is only a SHOULD in PEP 751. If it's omitted, we treat the
            // URL (less the filename) as the index. This isn't correct, but it's the best we can
            // do. In practice, the only effect here should be that we cache the sdist under a hash
            // of this URL (since we cache under the hash of the index).
            let mut index = file_url.to_url().map_err(PylockTomlErrorKind::ToUrl)?;
            index.path_segments_mut().unwrap().pop();
            IndexUrl::from(VerbatimUrl::from_url(index))
        };

        let file = Box::new(uv_distribution_types::File {
            dist_info_metadata: false,
            filename,
            hashes: HashDigests::from(self.hashes.clone()),
            requires_python: None,
            size: self.size,
            upload_time_utc_ms: self.upload_time.map(Timestamp::as_millisecond),
            url: FileLocation::AbsoluteUrl(file_url),
            yanked: None,
            zstd: None,
        });

        Ok(RegistrySourceDist {
            name: name.clone(),
            version: version.into_owned(),
            file,
            ext,
            index,
            wheels: vec![],
        })
    }
}

impl PylockTomlArchive {
    fn to_dist(
        &self,
        install_path: &Path,
        name: &PackageName,
        version: Option<&Version>,
    ) -> Result<Dist, PylockTomlErrorKind> {
        if let Some(url) = self.url.as_ref() {
            let filename = url
                .filename()
                .map_err(|_| PylockTomlErrorKind::UrlMissingFilename(url.clone()))?;

            let ext = DistExtension::from_path(filename.as_ref())?;
            match ext {
                DistExtension::Wheel => {
                    let filename = WheelFilename::from_str(&filename)?;
                    Ok(Dist::Built(BuiltDist::DirectUrl(DirectUrlBuiltDist {
                        filename,
                        location: Box::new(url.clone()),
                        url: VerbatimUrl::from_url(url.clone()),
                    })))
                }
                DistExtension::Source(ext) => {
                    Ok(Dist::Source(SourceDist::DirectUrl(DirectUrlSourceDist {
                        name: name.clone(),
                        location: Box::new(url.clone()),
                        subdirectory: self.subdirectory.clone().map(Box::<Path>::from),
                        ext,
                        url: VerbatimUrl::from_url(url.clone()),
                    })))
                }
            }
        } else if let Some(path) = self.path.as_ref() {
            let filename = path
                .as_ref()
                .file_name()
                .and_then(OsStr::to_str)
                .ok_or_else(|| {
                    PylockTomlErrorKind::PathMissingFilename(Box::<Path>::from(path.clone()))
                })?;

            let ext = DistExtension::from_path(filename)?;
            match ext {
                DistExtension::Wheel => {
                    let filename = WheelFilename::from_str(filename)?;
                    let install_path = install_path.join(path);
                    let url = VerbatimUrl::from_absolute_path(&install_path)
                        .map_err(|_| PylockTomlErrorKind::PathToUrl)?;
                    Ok(Dist::Built(BuiltDist::Path(PathBuiltDist {
                        filename,
                        install_path: install_path.into_boxed_path(),
                        url,
                    })))
                }
                DistExtension::Source(ext) => {
                    let install_path = install_path.join(path);
                    let url = VerbatimUrl::from_absolute_path(&install_path)
                        .map_err(|_| PylockTomlErrorKind::PathToUrl)?;
                    Ok(Dist::Source(SourceDist::Path(PathSourceDist {
                        name: name.clone(),
                        version: version.cloned(),
                        install_path: install_path.into_boxed_path(),
                        ext,
                        url,
                    })))
                }
            }
        } else {
            Err(PylockTomlErrorKind::ArchiveMissingPathUrl(name.clone()))
        }
    }

    /// Returns `true` if the [`PylockTomlArchive`] is a wheel.
    fn is_wheel(&self, name: &PackageName) -> Result<bool, PylockTomlErrorKind> {
        if let Some(url) = self.url.as_ref() {
            let filename = url
                .filename()
                .map_err(|_| PylockTomlErrorKind::UrlMissingFilename(url.clone()))?;

            let ext = DistExtension::from_path(filename.as_ref())?;
            Ok(matches!(ext, DistExtension::Wheel))
        } else if let Some(path) = self.path.as_ref() {
            let filename = path
                .as_ref()
                .file_name()
                .and_then(OsStr::to_str)
                .ok_or_else(|| {
                    PylockTomlErrorKind::PathMissingFilename(Box::<Path>::from(path.clone()))
                })?;

            let ext = DistExtension::from_path(filename)?;
            Ok(matches!(ext, DistExtension::Wheel))
        } else {
            Err(PylockTomlErrorKind::ArchiveMissingPathUrl(name.clone()))
        }
    }
}

/// Convert a Jiff timestamp to a TOML datetime.
#[allow(clippy::ref_option)]
fn timestamp_to_toml_datetime<S>(
    timestamp: &Option<Timestamp>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let Some(timestamp) = timestamp else {
        return serializer.serialize_none();
    };
    let timestamp = timestamp.to_zoned(TimeZone::UTC);
    let timestamp = toml_edit::Datetime {
        date: Some(toml_edit::Date {
            year: u16::try_from(timestamp.year()).map_err(serde::ser::Error::custom)?,
            month: u8::try_from(timestamp.month()).map_err(serde::ser::Error::custom)?,
            day: u8::try_from(timestamp.day()).map_err(serde::ser::Error::custom)?,
        }),
        time: Some(toml_edit::Time {
            hour: u8::try_from(timestamp.hour()).map_err(serde::ser::Error::custom)?,
            minute: u8::try_from(timestamp.minute()).map_err(serde::ser::Error::custom)?,
            second: u8::try_from(timestamp.second()).map_err(serde::ser::Error::custom)?,
            nanosecond: u32::try_from(timestamp.nanosecond()).map_err(serde::ser::Error::custom)?,
        }),
        offset: Some(toml_edit::Offset::Z),
    };
    serializer.serialize_some(&timestamp)
}

/// Convert a TOML datetime to a Jiff timestamp.
fn timestamp_from_toml_datetime<'de, D>(deserializer: D) -> Result<Option<Timestamp>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let Some(datetime) = Option::<toml_edit::Datetime>::deserialize(deserializer)? else {
        return Ok(None);
    };

    // If the date is omitted, we can't parse the datetime.
    let Some(date) = datetime.date else {
        return Err(serde::de::Error::custom("missing date"));
    };

    let year = i16::try_from(date.year).map_err(serde::de::Error::custom)?;
    let month = i8::try_from(date.month).map_err(serde::de::Error::custom)?;
    let day = i8::try_from(date.day).map_err(serde::de::Error::custom)?;
    let date = Date::new(year, month, day).map_err(serde::de::Error::custom)?;

    // If the timezone is omitted, assume UTC.
    let tz = if let Some(offset) = datetime.offset {
        match offset {
            toml_edit::Offset::Z => TimeZone::UTC,
            toml_edit::Offset::Custom { minutes } => {
                let hours = i8::try_from(minutes / 60).map_err(serde::de::Error::custom)?;
                TimeZone::fixed(Offset::constant(hours))
            }
        }
    } else {
        TimeZone::UTC
    };

    // If the time is omitted, assume midnight.
    let time = if let Some(time) = datetime.time {
        let hour = i8::try_from(time.hour).map_err(serde::de::Error::custom)?;
        let minute = i8::try_from(time.minute).map_err(serde::de::Error::custom)?;
        let second = i8::try_from(time.second).map_err(serde::de::Error::custom)?;
        let nanosecond = i32::try_from(time.nanosecond).map_err(serde::de::Error::custom)?;
        Time::new(hour, minute, second, nanosecond).map_err(serde::de::Error::custom)?
    } else {
        Time::midnight()
    };

    let timestamp = tz
        .to_timestamp(DateTime::from_parts(date, time))
        .map_err(serde::de::Error::custom)?;
    Ok(Some(timestamp))
}
