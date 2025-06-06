use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;
use uv_macros::OptionsMetadata;
use uv_pypi_types::Identifier;

/// Settings for the uv build backend (`uv_build`).
///
/// !!! note
///
///     The uv build backend is currently in preview and may change in any future release.
///
/// Note that those settings only apply when using the `uv_build` backend, other build backends
/// (such as hatchling) have their own configuration.
///
/// All options that accept globs use the portable glob patterns from
/// [PEP 639](https://packaging.python.org/en/latest/specifications/glob-patterns/).
#[derive(Deserialize, Serialize, OptionsMetadata, Debug, Clone, PartialEq, Eq)]
#[serde(default, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct BuildBackendSettings {
    /// The directory that contains the module directory.
    ///
    /// Common values are `src` (src layout, the default) or an empty path (flat layout).
    #[option(
        default = r#""src""#,
        value_type = "str",
        example = r#"module-root = """#
    )]
    pub module_root: PathBuf,

    /// The name of the module directory inside `module-root`.
    ///
    /// The default module name is the package name with dots and dashes replaced by underscores.
    ///
    /// Package names need to be valid Python identifiers, and the directory needs to contain a
    /// `__init__.py`. An exception are stubs packages, whose name ends with `-stubs`, with the stem
    /// being the module name, and which contain a `__init__.pyi` file.
    ///
    /// Note that using this option runs the risk of creating two packages with different names but
    /// the same module names. Installing such packages together leads to unspecified behavior,
    /// often with corrupted files or directory trees.
    #[option(
        default = r#"None"#,
        value_type = "str",
        example = r#"module-name = "sklearn""#
    )]
    pub module_name: Option<ModuleName>,

    /// Glob expressions which files and directories to additionally include in the source
    /// distribution.
    ///
    /// `pyproject.toml` and the contents of the module directory are always included.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"source-include = ["tests/**"]"#
    )]
    pub source_include: Vec<String>,

    /// If set to `false`, the default excludes aren't applied.
    ///
    /// Default excludes: `__pycache__`, `*.pyc`, and `*.pyo`.
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"default-excludes = false"#
    )]
    pub default_excludes: bool,

    /// Glob expressions which files and directories to exclude from the source distribution.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"source-exclude = ["*.bin"]"#
    )]
    pub source_exclude: Vec<String>,

    /// Glob expressions which files and directories to exclude from the wheel.
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"wheel-exclude = ["*.bin"]"#
    )]
    pub wheel_exclude: Vec<String>,

    /// Data includes for wheels.
    ///
    /// Each entry is a directory, whose contents are copied to the matching directory in the wheel
    /// in `<name>-<version>.data/(purelib|platlib|headers|scripts|data)`. Upon installation, this
    /// data is moved to its target location, as defined by
    /// <https://docs.python.org/3.12/library/sysconfig.html#installation-paths>. Usually, small
    /// data files are included by placing them in the Python module instead of using data includes.
    ///
    /// - `scripts`: Installed to the directory for executables, `<venv>/bin` on Unix or
    ///   `<venv>\Scripts` on Windows. This directory is added to `PATH` when the virtual
    ///   environment  is activated or when using `uv run`, so this data type can be used to install
    ///   additional binaries. Consider using `project.scripts` instead for Python entrypoints.
    /// - `data`: Installed over the virtualenv environment root.
    ///
    ///     Warning: This may override existing files!
    ///
    /// - `headers`: Installed to the include directory. Compilers building Python packages
    ///   with this package as build requirement use the include directory to find additional header
    ///   files.
    /// - `purelib` and `platlib`: Installed to the `site-packages` directory. It is not recommended
    ///   to uses these two options.
    // TODO(konsti): We should show a flat example instead.
    // ```toml
    // [tool.uv.build-backend.data]
    // headers = "include/headers",
    // scripts = "bin"
    // ```
    #[option(
        default = r#"{}"#,
        value_type = "dict[str, str]",
        example = r#"data = { "headers": "include/headers", "scripts": "bin" }"#
    )]
    pub data: WheelDataIncludes,
}

impl Default for BuildBackendSettings {
    fn default() -> Self {
        Self {
            module_root: PathBuf::from("src"),
            module_name: None,
            source_include: Vec::new(),
            default_excludes: true,
            source_exclude: Vec::new(),
            wheel_exclude: Vec::new(),
            data: WheelDataIncludes::default(),
        }
    }
}

/// Packages come in two kinds: Regular packages, where the name must be a valid Python identifier,
/// and stubs packages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleName {
    /// A Python module name, which needs to be a valid Python identifier to be used with `import`.
    Identifier(Identifier),
    /// A type stubs package, whose name ends with `-stubs` with the stem being the module name.
    Stubs(String),
}

impl Display for ModuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModuleName::Identifier(module_name) => Display::fmt(module_name, f),
            ModuleName::Stubs(module_name) => Display::fmt(module_name, f),
        }
    }
}

impl<'de> Deserialize<'de> for ModuleName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let module_name = String::deserialize(deserializer)?;
        if let Some(stem) = module_name.strip_suffix("-stubs") {
            // Check that the stubs belong to a valid module.
            Identifier::from_str(stem)
                .map(ModuleName::Identifier)
                .map_err(serde::de::Error::custom)?;
            Ok(ModuleName::Stubs(module_name))
        } else {
            Identifier::from_str(&module_name)
                .map(ModuleName::Identifier)
                .map_err(serde::de::Error::custom)
        }
    }
}

impl Serialize for ModuleName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ModuleName::Identifier(module_name) => module_name.serialize(serializer),
            ModuleName::Stubs(module_name) => module_name.serialize(serializer),
        }
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for ModuleName {
    fn schema_name() -> String {
        "ModuleName".to_string()
    }

    fn json_schema(_gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            string: Some(Box::new(schemars::schema::StringValidation {
                // Best-effort Unicode support (https://stackoverflow.com/a/68844380/3549270)
                pattern: Some(r"^[_\p{Alphabetic}][_0-9\p{Alphabetic}]*(-stubs)?$".to_string()),
                ..schemars::schema::StringValidation::default()
            })),
            metadata: Some(Box::new(schemars::schema::Metadata {
                description: Some(
                    "The name of the module, or the name of a stubs package".to_string(),
                ),
                ..schemars::schema::Metadata::default()
            })),
            ..schemars::schema::SchemaObject::default()
        }
        .into()
    }
}

/// Data includes for wheels.
///
/// See `BuildBackendSettings::data`.
#[derive(Default, Deserialize, Serialize, OptionsMetadata, Debug, Clone, PartialEq, Eq)]
// `deny_unknown_fields` to catch typos such as `header` vs `headers`.
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct WheelDataIncludes {
    purelib: Option<String>,
    platlib: Option<String>,
    headers: Option<String>,
    scripts: Option<String>,
    data: Option<String>,
}

impl WheelDataIncludes {
    /// Yield all data directories name and corresponding paths.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [
            ("purelib", self.purelib.as_deref()),
            ("platlib", self.platlib.as_deref()),
            ("headers", self.headers.as_deref()),
            ("scripts", self.scripts.as_deref()),
            ("data", self.data.as_deref()),
        ]
        .into_iter()
        .filter_map(|(name, value)| Some((name, value?)))
    }
}
