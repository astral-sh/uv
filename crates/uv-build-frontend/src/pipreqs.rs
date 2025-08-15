use std::str::FromStr;
use std::sync::LazyLock;

use rustc_hash::FxHashMap;
use uv_normalize::PackageName;

/// A mapping from module name to PyPI package name.
pub(crate) struct ModuleMap<'a>(FxHashMap<&'a str, PackageName>);

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
    pub(crate) fn lookup(&self, module: &str) -> Option<&PackageName> {
        self.0.get(module)
    }
}

/// A mapping from module name to PyPI package name.
pub(crate) static MODULE_MAPPING: LazyLock<ModuleMap> =
    LazyLock::new(|| ModuleMap::from_str(include_str!("pipreqs/mapping")));
