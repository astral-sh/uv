use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uv_macros::OptionsMetadata;

/// Settings for the uv build backend (`uv_build`).
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
    /// For namespace packages with a single module, the path can be dotted, e.g., `foo.bar` or
    /// `foo-stubs.bar`.
    ///
    /// For namespace packages with multiple modules, the path can be a list, e.g.,
    /// `["foo", "bar"]`. We recommend using a single module per package, splitting multiple
    /// packages into a workspace.
    ///
    /// Note that using this option runs the risk of creating two packages with different names but
    /// the same module names. Installing such packages together leads to unspecified behavior,
    /// often with corrupted files or directory trees.
    #[option(
        default = r#"None"#,
        value_type = "str | list[str]",
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

    /// Build a namespace package.
    ///
    /// Build a PEP 420 implicit namespace package, allowing more than one root `__init__.py`.
    ///
    /// Use this option when the namespace package contains multiple root `__init__.py`, for
    /// namespace packages with a single root `__init__.py` use a dotted `module-name` instead.
    ///
    /// To compare dotted `module-name` and `namespace = true`, the first example below can be
    /// expressed with `module-name = "cloud.database"`: There is one root `__init__.py` `database`.
    /// In the second example, we have three roots (`cloud.database`, `cloud.database_pro`,
    /// `billing.modules.database_pro`), so `namespace = true` is required.
    ///
    /// ```text
    /// src
    /// └── cloud
    ///     └── database
    ///         ├── __init__.py
    ///         ├── query_builder
    ///         │   └── __init__.py
    ///         └── sql
    ///             ├── parser.py
    ///             └── __init__.py
    /// ```
    ///
    /// ```text
    /// src
    /// ├── cloud
    /// │   ├── database
    /// │   │   ├── __init__.py
    /// │   │   ├── query_builder
    /// │   │   │   └── __init__.py
    /// │   │   └── sql
    /// │   │       ├── __init__.py
    /// │   │       └── parser.py
    /// │   └── database_pro
    /// │       ├── __init__.py
    /// │       └── query_builder.py
    /// └── billing
    ///     └── modules
    ///         └── database_pro
    ///             ├── __init__.py
    ///             └── sql.py
    /// ```
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"namespace = true"#
    )]
    pub namespace: bool,

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
            namespace: false,
            data: WheelDataIncludes::default(),
        }
    }
}

/// Whether to include a single module or multiple modules.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum ModuleName {
    /// A single module name.
    Name(String),
    /// Multiple module names, which are all included.
    Names(Vec<String>),
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
