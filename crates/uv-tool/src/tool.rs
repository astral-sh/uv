use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use toml_edit::{Array, Item, Table, Value, value};

use uv_configuration::{ExcludeDependency, PackageOverride};
use uv_distribution_types::{NameRequirementSpecification, Requirement, RequirementSource};
use uv_fs::{PortablePath, Simplified};
use uv_pep508::VerbatimUrl;
use uv_pypi_types::{HashAlgorithm, HashDigest, VerbatimParsedUrl};
use uv_python::PythonRequest;
use uv_settings::{ToolOptions, ToolOptionsWire};

/// A tool entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "ToolWire")]
pub struct Tool {
    /// Whether dependencies must come from the package's bundled lock.
    locked: bool,
    /// The requirements requested by the user during installation.
    ///
    /// The first requirement is the tool target itself; any remaining requirements come from
    /// `--with`.
    requirements: Vec<Requirement>,
    /// The constraints requested by the user during installation.
    constraints: Vec<NameRequirementSpecification>,
    /// The overrides requested by the user during installation.
    overrides: Vec<Requirement>,
    /// Overrides with hashes, when any were supplied.
    override_specifications: Vec<NameRequirementSpecification>,
    /// Overrides scoped to a package and optional version.
    scoped_overrides: Vec<PackageOverride<Requirement>>,
    /// The excludes requested by the user during installation.
    excludes: Vec<ExcludeDependency>,
    /// The build constraints requested by the user during installation.
    build_constraints: Vec<NameRequirementSpecification>,
    /// The Python requested by the user during installation.
    python: Option<PythonRequest>,
    /// A mapping of entry point names to their metadata.
    entrypoints: Vec<ToolEntrypoint>,
    /// The [`ToolOptions`] used to install this tool.
    options: ToolOptions,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ToolWire {
    #[serde(default)]
    locked: bool,
    #[serde(default)]
    requirements: Vec<RequirementWire>,
    #[serde(default)]
    constraints: Vec<ReceiptSpecification>,
    #[serde(default)]
    overrides: Vec<ReceiptSpecification>,
    #[serde(default)]
    scoped_overrides: Vec<PackageOverride<Requirement>>,
    #[serde(default)]
    excludes: Vec<ExcludeDependency>,
    #[serde(default)]
    build_constraint_dependencies: Vec<ReceiptSpecification>,
    python: Option<PythonRequest>,
    entrypoints: Vec<ToolEntrypoint>,
    #[serde(default)]
    options: ToolOptionsWire,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
enum RequirementWire {
    /// A [`Requirement`] following our uv-specific schema.
    Requirement(ReceiptRequirement),
    /// A PEP 508-compatible requirement. We no longer write these, but there might be receipts out
    /// there that still use them.
    Deprecated(uv_pep508::Requirement<VerbatimParsedUrl>),
}

/// Additional URL hashes are stored only in tool receipts. The shared requirement format is also
/// used by project lockfiles, where changing the format would affect unrelated lock operations.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct ReceiptRequirement {
    #[serde(flatten)]
    requirement: Requirement,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "url-hashes")]
    url_hashes: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ReceiptSpecification {
    #[serde(flatten)]
    specification: NameRequirementSpecification,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "url-hashes")]
    url_hashes: Vec<String>,
}

fn archive_url_hashes(requirement: &Requirement) -> Vec<String> {
    let url = match &requirement.source {
        RequirementSource::Url { url, .. } | RequirementSource::Path { url, .. } => url,
        RequirementSource::Registry { .. }
        | RequirementSource::GitDirectory { .. }
        | RequirementSource::GitPath { .. }
        | RequirementSource::Directory { .. } => return Vec::new(),
    };
    url.fragment()
        .into_iter()
        .flat_map(|fragment| fragment.split('&'))
        .filter(|part| {
            part.split_once('=')
                .is_some_and(|(name, _)| HashAlgorithm::from_str(name).is_ok())
        })
        .map(str::to_string)
        .collect()
}

/// Compare requirements as stored in a tool receipt, ignoring unrelated archive URL fragments.
pub fn receipt_requirements_equal(left: &Requirement, right: &Requirement) -> bool {
    fn normalized(requirement: &Requirement) -> Requirement {
        let mut hashes = archive_url_hashes(requirement);
        hashes.sort();
        let mut requirement = requirement.clone();
        match &mut requirement.source {
            RequirementSource::Url {
                subdirectory, url, ..
            } => {
                let mut normalized = url.clone().into_url();
                let mut fragments = subdirectory
                    .as_ref()
                    .map(|path| format!("subdirectory={}", path.display()))
                    .into_iter()
                    .collect::<Vec<_>>();
                fragments.extend(hashes);
                normalized.set_fragment(
                    (!fragments.is_empty())
                        .then(|| fragments.join("&"))
                        .as_deref(),
                );
                *url = VerbatimUrl::from_url(normalized);
            }
            RequirementSource::Path { url, .. } => {
                let mut normalized = url.clone().into_url();
                normalized.set_fragment((!hashes.is_empty()).then(|| hashes.join("&")).as_deref());
                *url = VerbatimUrl::from_url(normalized);
            }
            RequirementSource::Registry { .. }
            | RequirementSource::GitDirectory { .. }
            | RequirementSource::GitPath { .. }
            | RequirementSource::Directory { .. } => {}
        }
        requirement
    }
    left == right || normalized(left) == normalized(right)
}

fn restore_archive_url_hashes(
    requirement: &mut Requirement,
    hashes: &[String],
) -> Result<(), String> {
    if hashes.is_empty() {
        return Ok(());
    }
    for hash in hashes {
        let (algorithm, digest) = hash
            .split_once('=')
            .ok_or_else(|| format!("Invalid URL hash `{hash}`"))?;
        let algorithm = HashAlgorithm::from_str(algorithm).map_err(|err| err.to_string())?;
        HashDigest::new(algorithm, digest).map_err(|err| err.to_string())?;
    }
    let url = match &mut requirement.source {
        RequirementSource::Url { url, .. } | RequirementSource::Path { url, .. } => url,
        RequirementSource::Registry { .. }
        | RequirementSource::GitDirectory { .. }
        | RequirementSource::GitPath { .. }
        | RequirementSource::Directory { .. } => {
            return Err("URL hashes require an archive URL".to_string());
        }
    };
    let mut restored = url.clone().into_url();
    let mut fragments = restored
        .fragment()
        .map(str::to_string)
        .into_iter()
        .collect::<Vec<_>>();
    fragments.extend(hashes.iter().cloned());
    restored.set_fragment(Some(&fragments.join("&")));
    *url = VerbatimUrl::from_url(restored);
    Ok(())
}

impl ReceiptRequirement {
    fn new(requirement: Requirement) -> Self {
        let url_hashes = archive_url_hashes(&requirement);
        Self {
            requirement,
            url_hashes,
        }
    }

    fn into_requirement(mut self) -> Result<Requirement, String> {
        restore_archive_url_hashes(&mut self.requirement, &self.url_hashes)?;
        Ok(self.requirement)
    }
}

impl ReceiptSpecification {
    fn new(specification: NameRequirementSpecification) -> Self {
        let url_hashes = archive_url_hashes(&specification.requirement);
        Self {
            specification,
            url_hashes,
        }
    }

    fn into_specification(mut self) -> Result<NameRequirementSpecification, String> {
        restore_archive_url_hashes(&mut self.specification.requirement, &self.url_hashes)?;
        Ok(self.specification)
    }
}

impl TryFrom<ToolWire> for Tool {
    type Error = serde::de::value::Error;

    fn try_from(tool: ToolWire) -> Result<Self, Self::Error> {
        use serde::de::Error as _;

        let requirements = tool
            .requirements
            .into_iter()
            .map(|entry| match entry {
                RequirementWire::Requirement(requirement) => requirement.into_requirement(),
                RequirementWire::Deprecated(requirement) => Ok(Requirement::from(requirement)),
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::Error::custom)?;
        let constraints = tool
            .constraints
            .into_iter()
            .map(ReceiptSpecification::into_specification)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::Error::custom)?;
        let override_specifications = tool
            .overrides
            .into_iter()
            .map(ReceiptSpecification::into_specification)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::Error::custom)?;
        let build_constraints = tool
            .build_constraint_dependencies
            .into_iter()
            .map(ReceiptSpecification::into_specification)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::Error::custom)?;
        Ok(Self {
            locked: tool.locked,
            requirements,
            constraints,
            overrides: override_specifications
                .iter()
                .map(|entry| entry.requirement.clone())
                .collect(),
            override_specifications: if override_specifications
                .iter()
                .any(|entry| !entry.hashes.is_empty())
            {
                override_specifications
            } else {
                Vec::new()
            },
            scoped_overrides: tool.scoped_overrides,
            excludes: tool.excludes,
            build_constraints,
            python: tool.python,
            entrypoints: tool.entrypoints,
            options: tool.options.into(),
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ToolEntrypoint {
    pub name: String,
    pub install_path: PathBuf,
    pub from: Option<String>,
}

impl Display for ToolEntrypoint {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        cfg_select! {
            windows => {
                write!(
                    f,
                    "{} ({})",
                    self.name,
                    self.install_path
                        .simplified_display()
                        .to_string()
                        .replace('/', "\\")
                )
            },
            unix => {
                write!(
                    f,
                    "{} ({})",
                    self.name,
                    self.install_path.simplified_display()
                )
            },
        }
    }
}

/// Format an array so that each element is on its own line and has a trailing comma.
///
/// Example:
///
/// ```toml
/// requirements = [
///     "foo",
///     "bar",
/// ]
/// ```
fn each_element_on_its_line_array(elements: impl Iterator<Item = impl Into<Value>>) -> Array {
    let mut array = elements
        .map(Into::into)
        .map(|mut value| {
            // Each dependency is on its own line and indented.
            value.decor_mut().set_prefix("\n    ");
            value
        })
        .collect::<Array>();
    // With a trailing comma, inserting another entry doesn't change the preceding line,
    // reducing the diff noise.
    array.set_trailing_comma(true);
    // The line break between the last element's comma and the closing square bracket.
    array.set_trailing("\n");
    array
}

impl Tool {
    /// Whether dependencies must come from the package's bundled lock.
    pub fn locked(&self) -> bool {
        self.locked
    }

    /// Require the package's bundled lock for subsequent upgrades.
    #[must_use]
    pub fn with_locked(mut self, locked: bool) -> Self {
        self.locked = locked;
        self
    }

    /// Create a new `Tool`.
    pub fn new(
        requirements: Vec<Requirement>,
        constraints: Vec<NameRequirementSpecification>,
        overrides: Vec<Requirement>,
        excludes: Vec<ExcludeDependency>,
        build_constraints: Vec<NameRequirementSpecification>,
        python: Option<PythonRequest>,
        entrypoints: impl IntoIterator<Item = ToolEntrypoint>,
        options: ToolOptions,
    ) -> Self {
        let mut entrypoints: Vec<_> = entrypoints.into_iter().collect();
        entrypoints.sort();
        Self {
            locked: false,
            requirements,
            constraints,
            overrides,
            override_specifications: Vec::new(),
            scoped_overrides: Vec::new(),
            excludes,
            build_constraints,
            python,
            entrypoints,
            options,
        }
    }

    /// Create a new [`Tool`] with the given [`ToolOptions`].
    #[must_use]
    pub fn with_options(self, options: ToolOptions) -> Self {
        Self { options, ..self }
    }

    /// Returns the TOML table for this tool.
    pub(crate) fn to_toml(&self) -> Result<Table, toml_edit::ser::Error> {
        let mut table = Table::new();
        if self.locked {
            table.insert("locked", value(true));
        }

        if !self.requirements.is_empty() {
            table.insert("requirements", {
                let requirements = self
                    .requirements
                    .iter()
                    .map(|requirement| {
                        serde::Serialize::serialize(
                            &ReceiptRequirement::new(requirement.clone()),
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let requirements = match requirements.as_slice() {
                    [] => Array::new(),
                    [requirement] => Array::from_iter([requirement]),
                    requirements => each_element_on_its_line_array(requirements.iter()),
                };
                value(requirements)
            });
        }

        if !self.constraints.is_empty() {
            table.insert("constraints", {
                let constraints = self
                    .constraints
                    .iter()
                    .map(|constraint| {
                        serde::Serialize::serialize(
                            &ReceiptSpecification::new(constraint.clone()),
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let constraints = match constraints.as_slice() {
                    [] => Array::new(),
                    [constraint] => Array::from_iter([constraint]),
                    constraints => each_element_on_its_line_array(constraints.iter()),
                };
                value(constraints)
            });
        }

        if !self.overrides.is_empty() {
            table.insert("overrides", {
                let specifications = if self.override_specifications.is_empty() {
                    self.overrides
                        .iter()
                        .cloned()
                        .map(NameRequirementSpecification::from)
                        .collect::<Vec<_>>()
                } else {
                    self.override_specifications.clone()
                };
                let overrides = specifications
                    .into_iter()
                    .map(|r#override| {
                        serde::Serialize::serialize(
                            &ReceiptSpecification::new(r#override),
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let overrides = match overrides.as_slice() {
                    [] => Array::new(),
                    [r#override] => Array::from_iter([r#override]),
                    overrides => each_element_on_its_line_array(overrides.iter()),
                };
                value(overrides)
            });
        }

        if !self.scoped_overrides.is_empty() {
            let overrides = self
                .scoped_overrides
                .iter()
                .map(|entry| {
                    serde::Serialize::serialize(entry, toml_edit::ser::ValueSerializer::new())
                })
                .collect::<Result<Vec<_>, _>>()?;
            table.insert(
                "scoped-overrides",
                value(each_element_on_its_line_array(overrides.iter())),
            );
        }

        if !self.excludes.is_empty() {
            table.insert("excludes", {
                let excludes = self
                    .excludes
                    .iter()
                    .map(|r#exclude| {
                        serde::Serialize::serialize(
                            &r#exclude,
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let excludes = match excludes.as_slice() {
                    [] => Array::new(),
                    [r#exclude] => Array::from_iter([r#exclude]),
                    excludes => each_element_on_its_line_array(excludes.iter()),
                };
                value(excludes)
            });
        }

        if !self.build_constraints.is_empty() {
            table.insert("build-constraint-dependencies", {
                let build_constraints = self
                    .build_constraints
                    .iter()
                    .map(|r#build_constraint| {
                        serde::Serialize::serialize(
                            &ReceiptSpecification::new(r#build_constraint.clone()),
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let build_constraints = match build_constraints.as_slice() {
                    [] => Array::new(),
                    [r#build_constraint] => Array::from_iter([r#build_constraint]),
                    build_constraints => each_element_on_its_line_array(build_constraints.iter()),
                };
                value(build_constraints)
            });
        }

        if let Some(ref python) = self.python {
            table.insert(
                "python",
                value(serde::Serialize::serialize(
                    &python,
                    toml_edit::ser::ValueSerializer::new(),
                )?),
            );
        }

        table.insert("entrypoints", {
            let entrypoints = each_element_on_its_line_array(
                self.entrypoints
                    .iter()
                    .map(ToolEntrypoint::to_toml)
                    .map(Table::into_inline_table),
            );
            value(entrypoints)
        });

        if self.options != ToolOptions::default() {
            let serialized = serde::Serialize::serialize(
                &ToolOptionsWire::from(self.options.clone()),
                toml_edit::ser::ValueSerializer::new(),
            )?;
            let Value::InlineTable(serialized) = serialized else {
                return Err(toml_edit::ser::Error::Custom(
                    "Expected an inline table".to_string(),
                ));
            };
            table.insert("options", Item::Table(serialized.into_table()));
        }

        Ok(table)
    }

    pub fn entrypoints(&self) -> &[ToolEntrypoint] {
        &self.entrypoints
    }

    pub fn requirements(&self) -> &[Requirement] {
        &self.requirements
    }

    pub fn constraints(&self) -> &[NameRequirementSpecification] {
        &self.constraints
    }

    pub fn overrides(&self) -> &[Requirement] {
        &self.overrides
    }

    /// Preserve hashes supplied alongside overrides for later upgrades.
    #[must_use]
    pub fn with_override_specifications(
        mut self,
        overrides: Vec<NameRequirementSpecification>,
    ) -> Self {
        if overrides.iter().any(|entry| !entry.hashes.is_empty()) {
            self.override_specifications = overrides;
        }
        self
    }

    pub fn override_specifications(&self) -> &[NameRequirementSpecification] {
        &self.override_specifications
    }

    /// Preserve package-scoped overrides for later upgrades.
    #[must_use]
    pub fn with_scoped_overrides(mut self, overrides: Vec<PackageOverride<Requirement>>) -> Self {
        self.scoped_overrides = overrides;
        self
    }

    pub fn scoped_overrides(&self) -> &[PackageOverride<Requirement>] {
        &self.scoped_overrides
    }

    pub fn excludes(&self) -> &[ExcludeDependency] {
        &self.excludes
    }

    pub fn build_constraints(&self) -> &[NameRequirementSpecification] {
        &self.build_constraints
    }

    pub fn python(&self) -> &Option<PythonRequest> {
        &self.python
    }

    pub fn options(&self) -> &ToolOptions {
        &self.options
    }
}

impl ToolEntrypoint {
    /// Create a new [`ToolEntrypoint`].
    pub fn new(name: &str, install_path: PathBuf, from: String) -> Self {
        let name = name
            .trim_end_matches(std::env::consts::EXE_SUFFIX)
            .to_string();
        Self {
            name,
            install_path,
            from: Some(from),
        }
    }

    /// Returns the TOML table for this entrypoint.
    fn to_toml(&self) -> Table {
        let mut table = Table::new();
        table.insert("name", value(&self.name));
        table.insert(
            "install-path",
            // Use cross-platform slashes so the toml string type does not change
            value(PortablePath::from(&self.install_path).to_string()),
        );
        if let Some(from) = &self.from {
            table.insert("from", value(from));
        }
        table
    }
}
