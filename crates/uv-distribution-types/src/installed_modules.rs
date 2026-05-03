use std::collections::BTreeSet;
use std::path::{Component, Path};

use fs_err::File;
use uv_fs::normalize_path;
use uv_install_wheel::read_record;
use uv_pypi_types::ModuleName;

use crate::installed::{InstalledDist, InstalledDistError};

impl InstalledDist {
    /// Read the modules provided by this installed distribution.
    pub fn read_modules(&self) -> Result<BTreeSet<ModuleName>, InstalledDistError> {
        read_modules(self.install_path())
    }
}

fn read_modules(dist_info: &Path) -> Result<BTreeSet<ModuleName>, InstalledDistError> {
    if !has_extension(dist_info, "dist-info") {
        return Ok(BTreeSet::new());
    }

    let record_path = dist_info.join("RECORD");
    let record = read_record(File::open(&record_path)?)?;

    let mut modules = BTreeSet::new();
    for entry in record {
        add_record_module(&entry.path, &mut modules);
    }

    Ok(modules)
}

fn add_record_module(path: &str, modules: &mut BTreeSet<ModuleName>) {
    let Some(components) = record_path_components(path) else {
        return;
    };
    let Some((file_name, parents)) = components.split_last() else {
        return;
    };

    if components
        .iter()
        .any(|component| has_extension(component, "dist-info"))
    {
        return;
    }
    if components
        .first()
        .is_some_and(|component| has_extension(component, "data"))
    {
        return;
    }

    let mut module_components = parents.iter().map(String::as_str).collect::<Vec<_>>();
    if file_name == "__init__.py" {
        // The parent path is the package.
    } else if let Some(stem) = file_name.strip_suffix(".py") {
        module_components.push(stem);
    } else if let Some((stem, bytecode_parents)) = bytecode_module_stem(file_name, parents) {
        module_components = bytecode_parents.iter().map(String::as_str).collect();
        if stem != "__init__" {
            module_components.push(stem);
        }
    } else if let Some(stem) = extension_module_stem(file_name) {
        if stem != "__init__" {
            module_components.push(stem);
        }
    } else {
        return;
    }

    add_module_components(&module_components, modules);
}

fn record_path_components(path: &str) -> Option<Vec<String>> {
    let normalized = normalize_path(Path::new(path));
    let path = normalized.as_ref();

    if path.is_absolute() {
        return None;
    }

    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(component) => {
                components.push(component.to_str()?.to_string());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => return None,
        }
    }

    Some(components)
}

fn bytecode_module_stem<'a>(
    file_name: &'a str,
    parents: &'a [String],
) -> Option<(&'a str, &'a [String])> {
    let stem = file_name.strip_suffix(".pyc")?;
    if parents.last().is_some_and(|parent| parent == "__pycache__") {
        // A `.pyc` file in `__pycache__` does not make the module importable
        // without the corresponding source file. Sourceless imports use the
        // legacy `module.pyc` location instead.
        return None;
    }

    Some((stem, parents))
}

fn extension_module_stem(file_name: &str) -> Option<&str> {
    let stem = file_name
        .strip_suffix(".so")
        .or_else(|| file_name.strip_suffix(".pyd"))?;
    if stem.is_empty() {
        return None;
    }

    if let Some(module) = stem.strip_suffix(".abi3") {
        return non_empty(module);
    }

    let Some((module, tag)) = stem.rsplit_once('.') else {
        return Some(stem);
    };
    if is_extension_module_tag(tag) {
        non_empty(module)
    } else {
        None
    }
}

fn is_extension_module_tag(tag: &str) -> bool {
    // Hardcoded forms from common `importlib.machinery.EXTENSION_SUFFIXES` values.
    // These resemble wheel ABI tags, but they are import suffixes instead. For
    // example, Windows debug builds use `_d.cp314t-win_amd64.pyd`, with the
    // debug marker attached to the module stem rather than encoded in the tag as
    // `cp314td-win_amd64`.
    if tag.starts_with("cpython-") || tag.starts_with("pypy") || tag.starts_with("graalpy") {
        return true;
    }

    let Some(rest) = tag.strip_prefix("cp") else {
        return false;
    };
    let digit_count = rest
        .chars()
        .take_while(|char| char.is_ascii_digit())
        .count();
    if digit_count == 0 {
        return false;
    }

    let rest = &rest[digit_count..];
    let rest = rest.strip_prefix('t').unwrap_or(rest);
    rest.is_empty() || rest.starts_with('-') || rest.starts_with('_')
}

fn non_empty(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

fn has_extension(path: impl AsRef<Path>, extension: &str) -> bool {
    path.as_ref()
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(extension))
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
        add_record_module("./package/../café.py", &mut modules);

        assert_eq!(module_names(modules), "café");
    }

    #[test]
    fn record_module_from_legacy_bytecode() {
        let mut modules = BTreeSet::new();
        add_record_module("package/module.pyc", &mut modules);
        add_record_module("legacy.pyc", &mut modules);

        assert_eq!(module_names(modules), "legacy\npackage\npackage.module");
    }

    #[test]
    fn record_module_ignores_pycache_bytecode() {
        let mut modules = BTreeSet::new();
        add_record_module(
            "package/__pycache__/module.cpython-312.opt-1.pyc",
            &mut modules,
        );
        add_record_module("package/__pycache__/__init__.cpython-312.pyc", &mut modules);

        assert_eq!(module_names(modules), "");
    }

    #[test]
    fn record_module_from_extension_module() {
        let mut modules = BTreeSet::new();
        add_record_module("package/extension.cpython-312-darwin.so", &mut modules);
        add_record_module(
            "package/free_threaded.cpython-314td-darwin.so",
            &mut modules,
        );
        add_record_module("package/limited.abi3.so", &mut modules);
        add_record_module("package/windows.cp312-win_amd64.pyd", &mut modules);
        add_record_module("package/__init__.cpython-312-darwin.so", &mut modules);
        add_record_module("plain.so", &mut modules);

        assert_eq!(
            module_names(modules),
            "package\npackage.extension\npackage.free_threaded\npackage.limited\npackage.windows\nplain"
        );
    }

    #[test]
    fn record_module_ignores_unknown_extension_tags() {
        let mut modules = BTreeSet::new();
        add_record_module("package/extension.not-an-extension-tag.so", &mut modules);

        assert_eq!(module_names(modules), "");
    }
}
