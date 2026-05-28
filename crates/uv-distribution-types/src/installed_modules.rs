//! Discovers importable modules provided by an installed wheel.
//!
//! Installed wheels record installed paths in `<name>-<version>.dist-info/RECORD`. Python source
//! files, legacy sourceless bytecode, and recognized native extension modules located under the
//! import root contribute a [`ModuleName`] and its parent package prefixes.
//!
//! This is intentionally file-based: it does not infer modules exposed through `.pth` files,
//! legacy namespace declarations in `__init__.py`, or `.pyi`-only stub distributions.

use std::collections::BTreeSet;
use std::path::{Component, Path};

use fs_err::File;
use uv_fs::normalize_path;
use uv_install_wheel::read_record;
use uv_pypi_types::ModuleName;

use crate::installed::{InstalledDist, InstalledDistError};

impl InstalledDist {
    /// Read the modules provided by this installed distribution.
    pub fn read_modules(
        &self,
        extension_suffixes: &[Box<str>],
    ) -> Result<BTreeSet<ModuleName>, InstalledDistError> {
        let dist_info = self.install_path();
        if !has_extension(dist_info, "dist-info") {
            return Ok(BTreeSet::new());
        }

        let record_path = dist_info.join("RECORD");
        let record = read_record(File::open(&record_path)?)?;

        let mut modules = BTreeSet::new();
        for entry in record {
            add_record_module(&entry.path, extension_suffixes, &mut modules);
        }

        Ok(modules)
    }
}

fn add_record_module(
    path: &str,
    extension_suffixes: &[Box<str>],
    modules: &mut BTreeSet<ModuleName>,
) {
    let Some(components) = record_path_components(path) else {
        return;
    };
    let Some((file_name, parents)) = components.split_last() else {
        return;
    };
    let file_name = file_name.as_ref();

    // Metadata and other entries under `.dist-info` directories are not modules.
    if components
        .iter()
        .any(|component| has_extension(component.as_ref(), "dist-info"))
    {
        return;
    }
    // Files in a `.data` directory that were not relocated into the import root are not modules.
    // Relocated files are recorded at their installed paths instead.
    if components
        .first()
        .is_some_and(|component| has_extension(component.as_ref(), "data"))
    {
        return;
    }

    let mut module_components = parents
        .iter()
        .map(std::convert::AsRef::as_ref)
        .collect::<Vec<_>>();
    // We intentionally skip `.pyi` files here because we're looking for runtime module ownership.
    // Type stubs will require separate ownership modeling.
    if file_name == "__init__.py" {
        // The parent path is the package.
    } else if let Some(stem) = file_name.strip_suffix(".py") {
        module_components.push(stem);
    } else if let Some(stem) = bytecode_module_stem(file_name, parents) {
        if stem != "__init__" {
            module_components.push(stem);
        }
    } else if let Some(stem) = {
        // Python reports the recognized suffixes in import lookup order through
        // `importlib.machinery.EXTENSION_SUFFIXES`; preserve that order so a generic suffix such
        // as `.so` does not consume a more-specific suffix such as `.abi3.so`.
        extension_suffixes.iter().find_map(|suffix| {
            let stem = file_name.strip_suffix(suffix.as_ref())?;
            (!stem.is_empty()).then_some(stem)
        })
    } {
        if stem != "__init__" {
            module_components.push(stem);
        }
    } else {
        return;
    }

    add_module_components(&module_components, modules);
}

fn record_path_components(path: &str) -> Option<Vec<Box<str>>> {
    let normalized = normalize_path(Path::new(path));
    let path = normalized.as_ref();

    // `RECORD` can include absolute paths and relative paths that leave the directory containing
    // `.dist-info`, for example installed scripts. Those entries cannot describe modules here.
    if path.is_absolute() {
        return None;
    }

    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(component) => {
                components.push(Box::from(component.to_str()?));
            }
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => return None,
        }
    }

    Some(components)
}

/// Return the module stem for importable sourceless bytecode in a `RECORD` path.
///
/// CPython can import `package/module.pyc` directly when only bytecode is installed. In
/// contrast, `package/__pycache__/module.cpython-312.pyc` is not an import source without
/// `package/module.py`.
fn bytecode_module_stem<'a>(file_name: &'a str, parents: &[Box<str>]) -> Option<&'a str> {
    let stem = file_name.strip_suffix(".pyc")?;
    if parents
        .last()
        .is_some_and(|parent| parent.as_ref() == "__pycache__")
    {
        // A `.pyc` file in `__pycache__` does not make the module importable
        // without the corresponding source file. Sourceless imports use the
        // legacy `module.pyc` location instead.
        return None;
    }

    Some(stem)
}

fn has_extension(path: impl AsRef<Path>, extension: &str) -> bool {
    path.as_ref()
        .extension()
        .is_some_and(|candidate| candidate == extension)
}

fn add_module_components(components: &[&str], modules: &mut BTreeSet<ModuleName>) {
    let Ok(module) = ModuleName::from_components(components.iter().copied()) else {
        return;
    };

    modules.extend(module.prefixes());
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use uv_pypi_types::ModuleName;

    use super::add_record_module;

    fn extension_suffixes() -> Vec<Box<str>> {
        [
            ".cpython-312-darwin.so",
            ".cpython-314td-darwin.so",
            ".abi3.so",
            ".cp312-win_amd64.pyd",
            ".so",
        ]
        .into_iter()
        .map(Box::from)
        .collect()
    }

    fn module_names(modules: BTreeSet<ModuleName>) -> String {
        modules
            .into_iter()
            .map(|module| module.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn record_module_normalizes_record_paths() {
        let mut modules = BTreeSet::new();
        add_record_module("./package/../café.py", &[], &mut modules);

        assert_eq!(module_names(modules), "café");
    }

    #[test]
    fn record_module_from_legacy_bytecode() {
        let mut modules = BTreeSet::new();
        add_record_module("package/module.pyc", &[], &mut modules);
        add_record_module("legacy.pyc", &[], &mut modules);

        assert_eq!(module_names(modules), "legacy\npackage\npackage.module");
    }

    #[test]
    fn record_module_ignores_pycache_bytecode() {
        let mut modules = BTreeSet::new();
        add_record_module(
            "package/__pycache__/module.cpython-312.opt-1.pyc",
            &[],
            &mut modules,
        );
        add_record_module(
            "package/__pycache__/__init__.cpython-312.pyc",
            &[],
            &mut modules,
        );

        assert_eq!(module_names(modules), "");
    }

    #[test]
    fn record_module_from_extension_module() {
        let extension_suffixes = extension_suffixes();
        let mut modules = BTreeSet::new();
        add_record_module(
            "package/extension.cpython-312-darwin.so",
            &extension_suffixes,
            &mut modules,
        );
        add_record_module(
            "package/free_threaded.cpython-314td-darwin.so",
            &extension_suffixes,
            &mut modules,
        );
        add_record_module("package/limited.abi3.so", &extension_suffixes, &mut modules);
        add_record_module(
            "package/windows.cp312-win_amd64.pyd",
            &extension_suffixes,
            &mut modules,
        );
        add_record_module(
            "package/__init__.cpython-312-darwin.so",
            &extension_suffixes,
            &mut modules,
        );
        add_record_module("plain.so", &extension_suffixes, &mut modules);

        assert_eq!(
            module_names(modules),
            "package\npackage.extension\npackage.free_threaded\npackage.limited\npackage.windows\nplain"
        );
    }

    #[test]
    fn record_module_ignores_unrecognized_extension_suffixes() {
        let extension_suffixes = extension_suffixes();
        let mut modules = BTreeSet::new();
        add_record_module(
            "package/extension.not-an-extension-tag.so",
            &extension_suffixes,
            &mut modules,
        );
        add_record_module(
            "package/bogus.pypynonsense.so",
            &extension_suffixes,
            &mut modules,
        );

        assert_eq!(module_names(modules), "");
    }
}
