use std::collections::BTreeSet;
use std::str::FromStr;
use std::sync::LazyLock;

use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::{Stmt, StmtImport, StmtImportFrom};
use rustc_hash::{FxHashMap, FxHashSet};

use uv_normalize::PackageName;

/// Extract the inferred requirements from Python source code.
///
/// Assumes that all non-relative imports in the source code are either standard library modules or
/// PyPI packages.
pub fn extract_requirements(
    source: &str,
) -> Result<BTreeSet<PackageName>, ruff_python_parser::ParseError> {
    // Parse the source code into a Python module.
    let module = ruff_python_parser::parse_module(source)?;

    // Extract the import names from the module.
    let imports = {
        let mut imports_visitor = ImportsVisitor::new();
        imports_visitor.visit_body(module.suite());

        imports_visitor.into_imports()
    };

    // Map the imports to requirements.
    let requirements = imports
        .into_iter()
        .filter_map(|module| {
            // Skip standard library modules.
            if STDLIB.contains(module.as_ref()) {
                return None;
            }

            // Use static mapping for known modules.
            if let Some(name) = MODULE_MAPPING.lookup(module.as_ref()).cloned() {
                return Some(name);
            }

            // Otherwise, treat it as a package name.
            PackageName::from_str(module).ok()
        })
        .collect::<BTreeSet<_>>();

    Ok(requirements)
}

/// A collection of known Python standard library modules.
#[derive(Debug)]
struct Stdlib<'a>(FxHashSet<&'a str>);

impl<'a> Stdlib<'a> {
    /// Generate a [`Stdlib`] from a string representation, with each line containing a module name.
    fn from_str(source: &'a str) -> Self {
        let mut stdlib = FxHashSet::default();
        for line in source.lines() {
            let module = line.trim();
            if !module.is_empty() {
                stdlib.insert(module);
            }
        }
        Self(stdlib)
    }

    /// Returns `true` if the standard library contains the given module.
    fn contains(&self, module: &str) -> bool {
        self.0.contains(module)
    }
}

/// A mapping from module name to PyPI package name.
struct ModuleMap<'a>(FxHashMap<&'a str, PackageName>);

impl<'a> ModuleMap<'a> {
    /// Generate a [`ModuleMap`] from a string representation, encoded in `${module}:{package}` format.
    fn from_str(source: &'a str) -> Self {
        let mut mapping = FxHashMap::default();
        for line in source.lines() {
            if let Some((module, package)) = line.split_once(':') {
                let module = module.trim();
                let package = PackageName::from_str(package.trim()).unwrap();
                mapping.insert(module, package);
            }
        }
        Self(mapping)
    }

    /// Look up a PyPI package name for a given module name.
    fn lookup(&self, module: &str) -> Option<&PackageName> {
        self.0.get(module)
    }
}

/// A mapping from module name to PyPI package name.
static MODULE_MAPPING: LazyLock<ModuleMap> =
    LazyLock::new(|| ModuleMap::from_str(include_str!("pipreqs/mapping")));

/// A mapping of known standard library modules to their names.
static STDLIB: LazyLock<Stdlib> =
    LazyLock::new(|| Stdlib::from_str(include_str!("pipreqs/stdlib")));

/// Extracts the set of global names from a given scope.
#[derive(Debug)]
struct ImportsVisitor<'a>(FxHashSet<&'a str>);

impl<'a> ImportsVisitor<'a> {
    fn new() -> Self {
        Self(FxHashSet::default())
    }

    /// Extracts the set of import names from a given scope.
    fn into_imports(self) -> FxHashSet<&'a str> {
        self.0
    }
}

impl<'a> StatementVisitor<'a> for ImportsVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Import(StmtImport { names, .. }) => {
                for name in names {
                    let name = name.name.as_str();
                    let module = name.split('.').next().unwrap_or(name);
                    self.0.insert(module);
                }
            }
            Stmt::ImportFrom(StmtImportFrom {
                names,
                module: Some(..),
                level: 0,
                ..
            }) => {
                for name in names {
                    let name = name.name.as_str();
                    let module = name.split('.').next().unwrap_or(name);
                    self.0.insert(module);
                }
            }
            _ => walk_stmt(self, stmt),
        }
    }
}
