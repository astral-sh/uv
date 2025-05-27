use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use console::Term;

use uv_fs::{CWD, Simplified};
use uv_requirements_txt::RequirementsTxtRequirement;

#[derive(Debug, Clone)]
pub enum RequirementsSource {
    /// A package was provided on the command line (e.g., `pip install flask`).
    Package(RequirementsTxtRequirement),
    /// An editable path was provided on the command line (e.g., `pip install -e ../flask`).
    Editable(RequirementsTxtRequirement),
    /// Dependencies were provided via a `pylock.toml` file.
    PylockToml(PathBuf),
    /// Dependencies were provided via a `requirements.txt` file (e.g., `pip install -r requirements.txt`).
    RequirementsTxt(PathBuf),
    /// Dependencies were provided via a `pyproject.toml` file (e.g., `pip-compile pyproject.toml`).
    PyprojectToml(PathBuf),
    /// Dependencies were provided via a `setup.py` file (e.g., `pip-compile setup.py`).
    SetupPy(PathBuf),
    /// Dependencies were provided via a `setup.cfg` file (e.g., `pip-compile setup.cfg`).
    SetupCfg(PathBuf),
    /// Dependencies were provided via an unsupported Conda `environment.yml` file (e.g., `pip install -r environment.yml`).
    EnvironmentYml(PathBuf),
}

impl RequirementsSource {
    /// Parse a [`RequirementsSource`] from a [`PathBuf`]. The file type is determined by the file
    /// extension.
    pub fn from_requirements_file(path: PathBuf) -> Result<Self> {
        if path.ends_with("pyproject.toml") {
            Ok(Self::PyprojectToml(path))
        } else if path.ends_with("setup.py") {
            Ok(Self::SetupPy(path))
        } else if path.ends_with("setup.cfg") {
            Ok(Self::SetupCfg(path))
        } else if path.ends_with("environment.yml") {
            Ok(Self::EnvironmentYml(path))
        } else if path
            .file_name()
            .is_some_and(|file_name| file_name.to_str().is_some_and(is_pylock_toml))
        {
            Ok(Self::PylockToml(path))
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        {
            Err(anyhow::anyhow!(
                "`{}` is not a valid PEP 751 filename: expected TOML file to start with `pylock.` and end with `.toml` (e.g., `pylock.toml`, `pylock.dev.toml`)",
                path.user_display(),
            ))
        } else {
            Ok(Self::RequirementsTxt(path))
        }
    }

    /// Parse a [`RequirementsSource`] from a `requirements.txt` file.
    pub fn from_requirements_txt(path: PathBuf) -> Result<Self> {
        for file_name in ["pyproject.toml", "setup.py", "setup.cfg"] {
            if path.ends_with(file_name) {
                return Err(anyhow::anyhow!(
                    "The file `{}` appears to be a `{}` file, but requirements must be specified in `requirements.txt` format",
                    path.user_display(),
                    file_name
                ));
            }
        }
        if path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(is_pylock_toml)
        {
            return Err(anyhow::anyhow!(
                "The file `{}` appears to be a `pylock.toml` file, but requirements must be specified in `requirements.txt` format",
                path.user_display(),
            ));
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        {
            return Err(anyhow::anyhow!(
                "The file `{}` appears to be a TOML file, but requirements must be specified in `requirements.txt` format",
                path.user_display(),
            ));
        }
        Ok(Self::RequirementsTxt(path))
    }

    /// Parse a [`RequirementsSource`] from a `constraints.txt` file.
    pub fn from_constraints_txt(path: PathBuf) -> Result<Self> {
        for file_name in ["pyproject.toml", "setup.py", "setup.cfg"] {
            if path.ends_with(file_name) {
                return Err(anyhow::anyhow!(
                    "The file `{}` appears to be a `{}` file, but constraints must be specified in `requirements.txt` format",
                    path.user_display(),
                    file_name
                ));
            }
        }
        if path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(is_pylock_toml)
        {
            return Err(anyhow::anyhow!(
                "The file `{}` appears to be a `pylock.toml` file, but constraints must be specified in `requirements.txt` format",
                path.user_display(),
            ));
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        {
            return Err(anyhow::anyhow!(
                "The file `{}` appears to be a TOML file, but constraints must be specified in `requirements.txt` format",
                path.user_display(),
            ));
        }
        Ok(Self::RequirementsTxt(path))
    }

    /// Parse a [`RequirementsSource`] from an `overrides.txt` file.
    pub fn from_overrides_txt(path: PathBuf) -> Result<Self> {
        for file_name in ["pyproject.toml", "setup.py", "setup.cfg"] {
            if path.ends_with(file_name) {
                return Err(anyhow::anyhow!(
                    "The file `{}` appears to be a `{}` file, but overrides must be specified in `requirements.txt` format",
                    path.user_display(),
                    file_name
                ));
            }
        }
        if path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(is_pylock_toml)
        {
            return Err(anyhow::anyhow!(
                "The file `{}` appears to be a `pylock.toml` file, but overrides must be specified in `requirements.txt` format",
                path.user_display(),
            ));
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        {
            return Err(anyhow::anyhow!(
                "The file `{}` appears to be a TOML file, but overrides must be specified in `requirements.txt` format",
                path.user_display(),
            ));
        }
        Ok(Self::RequirementsTxt(path))
    }

    /// Parse a [`RequirementsSource`] from a user-provided string, assumed to be a positional
    /// package (e.g., `uv pip install flask`).
    ///
    /// If the user provided a value that appears to be a `requirements.txt` file or a local
    /// directory, prompt them to correct it (if the terminal is interactive).
    pub fn from_package_argument(name: &str) -> Result<Self> {
        // If the user provided a `requirements.txt` file without `-r` (as in
        // `uv pip install requirements.txt`), prompt them to correct it.
        #[allow(clippy::case_sensitive_file_extension_comparisons)]
        if (name.ends_with(".txt") || name.ends_with(".in")) && Path::new(&name).is_file() {
            let term = Term::stderr();
            if term.is_term() {
                let prompt = format!(
                    "`{name}` looks like a local requirements file but was passed as a package name. Did you mean `-r {name}`?"
                );
                let confirmation =
                    uv_console::confirm(&prompt, &term, true).context("Confirm prompt failed")?;
                if confirmation {
                    return Self::from_requirements_file(name.into());
                }
            }
        }

        // Similarly, if the user provided a `pyproject.toml` file without `-r` (as in
        // `uv pip install pyproject.toml`), prompt them to correct it.
        if (name == "pyproject.toml"
            || name == "setup.py"
            || name == "setup.cfg"
            || is_pylock_toml(name))
            && Path::new(&name).is_file()
        {
            let term = Term::stderr();
            if term.is_term() {
                let prompt = format!(
                    "`{name}` looks like a local metadata file but was passed as a package name. Did you mean `-r {name}`?"
                );
                let confirmation =
                    uv_console::confirm(&prompt, &term, true).context("Confirm prompt failed")?;
                if confirmation {
                    return Self::from_requirements_file(name.into());
                }
            }
        }

        let requirement = RequirementsTxtRequirement::parse(name, &*CWD, false)
            .with_context(|| format!("Failed to parse: `{name}`"))?;

        Ok(Self::Package(requirement))
    }

    /// Parse a [`RequirementsSource`] from a user-provided string, assumed to be a `--with`
    /// package (e.g., `uvx --with flask ruff`).
    ///
    /// If the user provided a value that appears to be a `requirements.txt` file or a local
    /// directory, prompt them to correct it (if the terminal is interactive).
    pub fn from_with_package_argument(name: &str) -> Result<Self> {
        // If the user provided a `requirements.txt` file without `--with-requirements` (as in
        // `uvx --with requirements.txt ruff`), prompt them to correct it.
        #[allow(clippy::case_sensitive_file_extension_comparisons)]
        if (name.ends_with(".txt") || name.ends_with(".in")) && Path::new(&name).is_file() {
            let term = Term::stderr();
            if term.is_term() {
                let prompt = format!(
                    "`{name}` looks like a local requirements file but was passed as a package name. Did you mean `--with-requirements {name}`?"
                );
                let confirmation =
                    uv_console::confirm(&prompt, &term, true).context("Confirm prompt failed")?;
                if confirmation {
                    return Self::from_requirements_file(name.into());
                }
            }
        }

        // Similarly, if the user provided a `pyproject.toml` file without `--with-requirements` (as in
        // `uvx --with pyproject.toml ruff`), prompt them to correct it.
        if (name == "pyproject.toml"
            || name == "setup.py"
            || name == "setup.cfg"
            || is_pylock_toml(name))
            && Path::new(&name).is_file()
        {
            let term = Term::stderr();
            if term.is_term() {
                let prompt = format!(
                    "`{name}` looks like a local metadata file but was passed as a package name. Did you mean `--with-requirements {name}`?"
                );
                let confirmation =
                    uv_console::confirm(&prompt, &term, true).context("Confirm prompt failed")?;
                if confirmation {
                    return Self::from_requirements_file(name.into());
                }
            }
        }

        let requirement = RequirementsTxtRequirement::parse(name, &*CWD, false)
            .with_context(|| format!("Failed to parse: `{name}`"))?;

        Ok(Self::Package(requirement))
    }

    /// Parse an editable [`RequirementsSource`] (e.g., `uv pip install -e .`).
    pub fn from_editable(name: &str) -> Result<Self> {
        let requirement = RequirementsTxtRequirement::parse(name, &*CWD, true)
            .with_context(|| format!("Failed to parse: `{name}`"))?;

        Ok(Self::Editable(requirement))
    }

    /// Parse a package [`RequirementsSource`] (e.g., `uv pip install ruff`).
    pub fn from_package(name: &str) -> Result<Self> {
        let requirement = RequirementsTxtRequirement::parse(name, &*CWD, false)
            .with_context(|| format!("Failed to parse: `{name}`"))?;

        Ok(Self::Package(requirement))
    }

    /// Returns `true` if the source allows extras to be specified.
    pub fn allows_extras(&self) -> bool {
        matches!(
            self,
            Self::PyprojectToml(_) | Self::SetupPy(_) | Self::SetupCfg(_)
        )
    }

    /// Returns `true` if the source allows groups to be specified.
    pub fn allows_groups(&self) -> bool {
        matches!(self, Self::PyprojectToml(_))
    }
}

impl std::fmt::Display for RequirementsSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Package(package) => write!(f, "{package:?}"),
            Self::Editable(path) => write!(f, "-e {path:?}"),
            Self::PylockToml(path)
            | Self::RequirementsTxt(path)
            | Self::PyprojectToml(path)
            | Self::SetupPy(path)
            | Self::SetupCfg(path)
            | Self::EnvironmentYml(path) => {
                write!(f, "{}", path.simplified_display())
            }
        }
    }
}

/// Returns `true` if a file name matches the `pylock.toml` pattern defined in PEP 751.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
pub fn is_pylock_toml(file_name: &str) -> bool {
    file_name.starts_with("pylock.") && file_name.ends_with(".toml")
}
