use jiff::tz::TimeZone;
use jiff::Timestamp;
use toml_edit::{value, Array, ArrayOfTables, Item, Table};
use url::Url;

use uv_configuration::{DependencyGroupsWithDefaults, ExtrasSpecification, InstallOptions};
use uv_distribution_types::{IndexUrl, RegistryBuiltWheel, RemoteSource, SourceDist};
use uv_fs::{relative_to, PortablePathBuf};
use uv_git_types::GitOid;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_pep508::MarkerTree;
use uv_pypi_types::{Hashes, VcsKind};
use uv_small_str::SmallString;

use crate::lock::export::ExportableRequirements;
use crate::lock::{each_element_on_its_line_array, LockErrorKind, Source};
use crate::{Installable, LockError, RequiresPython};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PylockToml {
    lock_version: Version,
    created_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    requires_python: Option<RequiresPython>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    extras: Vec<ExtraName>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    dependency_groups: Vec<GroupName>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    default_groups: Vec<GroupName>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    packages: Vec<PylockTomlPackage>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    attestation_identities: Vec<PylockTomlAttestationIdentity>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PylockTomlPackage {
    name: PackageName,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<Version>,
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
    index: Option<Url>,
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
struct PylockTomlDependency;

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
    url: Option<Url>,
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
    url: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<PortablePathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "timestamp_to_toml_datetime"
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
    url: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<PortablePathBuf>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "timestamp_to_toml_datetime"
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
    name: Option<SmallString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<PortablePathBuf>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "timestamp_to_toml_datetime"
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
    /// Construct a [`PylockToml`] from a uv lockfile.
    pub fn from_lock(
        target: &impl Installable<'lock>,
        prune: &[PackageName],
        extras: &ExtrasSpecification,
        dev: &DependencyGroupsWithDefaults,
        annotate: bool,
        install_options: &'lock InstallOptions,
    ) -> Result<Self, LockError> {
        // Extract the packages from the lock file.
        let ExportableRequirements(mut nodes) = ExportableRequirements::from_lock(
            target,
            prune,
            extras,
            dev,
            annotate,
            install_options,
        );

        // Sort the nodes, such that unnamed URLs (editables) appear at the top.
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
                        .map(|wheel| wheel.to_registry_dist(source, target.install_path()))
                        .collect::<Result<Vec<RegistryBuiltWheel>, LockError>>()?;
                    Some(
                        wheels
                            .into_iter()
                            .map(|wheel| {
                                let url =
                                    wheel.file.url.to_url().map_err(LockErrorKind::InvalidUrl)?;
                                Ok(PylockTomlWheel {
                                    // Optional "when the last component of path/ url would be the same value".
                                    name: if url
                                        .filename()
                                        .is_ok_and(|filename| filename == *wheel.file.filename)
                                    {
                                        None
                                    } else {
                                        Some(wheel.file.filename.clone())
                                    },
                                    upload_time: wheel
                                        .file
                                        .upload_time_utc_ms
                                        .map(Timestamp::from_millisecond)
                                        .transpose()
                                        .map_err(LockErrorKind::InvalidTimestamp)?,
                                    url: Some(url),
                                    path: None,
                                    size: wheel.file.size,
                                    hashes: Hashes::from(wheel.file.hashes),
                                })
                            })
                            .collect::<Result<Vec<_>, LockError>>()?,
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
                    editable: Some(sdist.editable),
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
                    let url = sdist.file.url.to_url().map_err(LockErrorKind::InvalidUrl)?;
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
                            .transpose()
                            .map_err(LockErrorKind::InvalidTimestamp)?,
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

            let package = PylockTomlPackage {
                name: package.id.name.clone(),
                version: package.id.version.clone(),
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
}

impl PylockTomlPackage {
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
