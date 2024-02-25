use configparser::ini::Ini;
use once_cell::sync::Lazy;
use regex::Regex;
use rustc_hash::FxHashSet;
use serde::Serialize;

use crate::{wheel, Error};

/// A script defining the name of the runnable entrypoint and the module and function that should be
/// run.
#[cfg(feature = "python_bindings")]
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[pyo3::pyclass(dict)]
pub struct Script {
    #[pyo3(get)]
    pub script_name: String,
    #[pyo3(get)]
    pub module: String,
    #[pyo3(get)]
    pub function: String,
}

/// A script defining the name of the runnable entrypoint and the module and function that should be
/// run.
#[cfg(not(feature = "python_bindings"))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Script {
    pub script_name: String,
    pub module: String,
    pub function: String,
}

impl Script {
    /// Parses a script definition like `foo.bar:main` or `foomod:main_bar [bar,baz]`
    ///
    /// <https://packaging.python.org/en/latest/specifications/entry-points/>
    ///
    /// Extras are supposed to be ignored, which happens if you pass None for extras
    pub fn from_value(
        script_name: &str,
        value: &str,
        extras: Option<&[String]>,
    ) -> Result<Option<Self>, Error> {
        // "Within a value, readers must accept and ignore spaces (including multiple consecutive spaces) before or after the colon,
        //  between the object reference and the left square bracket, between the extra names and the square brackets and colons delimiting them,
        //  and after the right square bracket."
        // – https://packaging.python.org/en/latest/specifications/entry-points/#file-format
        static SCRIPT_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^(?P<module>[\w\d_\-.]+)\s*:\s*(?P<function>[\w\d_\-.]+)(?:\s*\[\s*(?P<extras>(?:[^,]+,?\s*)+)\])?\s*$").unwrap()
        });

        let captures = SCRIPT_REGEX
            .captures(value)
            .ok_or_else(|| Error::InvalidWheel(format!("invalid console script: '{value}'")))?;
        if let Some(script_extras) = captures.name("extras") {
            if let Some(extras) = extras {
                let script_extras = script_extras
                    .as_str()
                    .split(',')
                    .map(|extra| extra.trim().to_string())
                    .collect::<FxHashSet<String>>();
                if !script_extras.is_subset(&extras.iter().cloned().collect()) {
                    return Ok(None);
                }
            }
        }

        Ok(Some(Self {
            script_name: script_name.to_string(),
            module: captures.name("module").unwrap().as_str().to_string(),
            function: captures.name("function").unwrap().as_str().to_string(),
        }))
    }

    pub fn import_name(&self) -> &str {
        self.function
            .split_once('.')
            .map_or(&self.function, |(import_name, _)| import_name)
    }
}

pub(crate) fn scripts_from_ini(
    extras: Option<&[String]>,
    python_minor: u8,
    ini: String,
) -> Result<(Vec<Script>, Vec<Script>), Error> {
    let entry_points_mapping = Ini::new_cs()
        .read(ini)
        .map_err(|err| Error::InvalidWheel(format!("entry_points.txt is invalid: {err}")))?;

    // TODO: handle extras
    let mut console_scripts = match entry_points_mapping.get("console_scripts") {
        Some(console_scripts) => {
            wheel::read_scripts_from_section(console_scripts, "console_scripts", extras)?
        }
        None => Vec::new(),
    };
    let gui_scripts = match entry_points_mapping.get("gui_scripts") {
        Some(gui_scripts) => wheel::read_scripts_from_section(gui_scripts, "gui_scripts", extras)?,
        None => Vec::new(),
    };

    // Special case to generate versioned pip launchers.
    // https://github.com/pypa/pip/blob/3898741e29b7279e7bffe044ecfbe20f6a438b1e/src/pip/_internal/operations/install/wheel.py#L283
    // https://github.com/astral-sh/uv/issues/1593
    for script in &mut console_scripts {
        let Some((left, right)) = script.script_name.split_once('.') else {
            continue;
        };
        if left != "pip3" || right.parse::<u8>().is_err() {
            continue;
        }
        script.script_name = format!("pip3.{python_minor}");
    }

    Ok((console_scripts, gui_scripts))
}

#[cfg(test)]
mod test {
    use crate::Script;

    #[test]
    fn test_valid_script_names() {
        for case in [
            "foomod:main",
            "foomod:main_bar [bar,baz]",
            "pylutron_caseta.cli:lap_pair[cli]",
        ] {
            assert!(Script::from_value("script", case, None).is_ok());
        }
    }

    #[test]
    fn test_invalid_script_names() {
        for case in [
            "",                     // Empty
            ":weh",                 // invalid module
            "foomod:main_bar [bar", // extras malformed
            "pylutron_caseta",      // missing function part
            "weh:",                 // invalid function
        ] {
            assert!(
                Script::from_value("script", case, None).is_err(),
                "case: {case}"
            );
        }
    }

    #[test]
    fn test_split_of_import_name_from_function() {
        let entrypoint = "foomod:mod_bar.sub_foo.func_baz";

        let script = Script::from_value("script", entrypoint, None)
            .unwrap()
            .unwrap();
        assert_eq!(script.function, "mod_bar.sub_foo.func_baz");
        assert_eq!(script.import_name(), "mod_bar");
    }
}
