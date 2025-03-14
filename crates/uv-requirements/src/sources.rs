use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use console::Term;

use uv_fs::{Simplified, CWD};
use uv_requirements_txt::RequirementsTxtRequirement;
use uv_warnings::warn_user;

#[derive(Debug, Clone)]
pub enum RequirementsSource {
    /// A package was provided on the command line (e.g., `pip install flask`).
    Package(RequirementsTxtRequirement),
    /// An editable path was provided on the command line (e.g., `pip install -e ../flask`).
    Editable(RequirementsTxtRequirement),
    /// Dependencies were provided via a `requirements.txt` file (e.g., `pip install -r requirements.txt`).
    RequirementsTxt(PathBuf),
    /// Dependencies were provided via a `pyproject.toml` file (e.g., `pip-compile pyproject.toml`).
    PyprojectToml(PathBuf),
    /// Dependencies were provided via a `setup.py` file (e.g., `pip-compile setup.py`).
    SetupPy(PathBuf),
    /// Dependencies were provided via a `setup.cfg` file (e.g., `pip-compile setup.cfg`).
    SetupCfg(PathBuf),
    /// Dependencies were provided via a path to a source tree (e.g., `pip install .`).
    SourceTree(PathBuf),
}

impl RequirementsSource {
    /// Parse a [`RequirementsSource`] from a [`PathBuf`]. The file type is determined by the file
    /// extension.
    pub fn from_requirements_file(path: PathBuf) -> Self {
        if path.ends_with("pyproject.toml") {
            Self::PyprojectToml(path)
        } else if path.ends_with("setup.py") {
            Self::SetupPy(path)
        } else if path.ends_with("setup.cfg") {
            Self::SetupCfg(path)
        } else {
            Self::RequirementsTxt(path)
        }
    }

    /// Parse a [`RequirementsSource`] from a `requirements.txt` file.
    pub fn from_requirements_txt(path: PathBuf) -> Self {
        for filename in ["pyproject.toml", "setup.py", "setup.cfg"] {
            if path.ends_with(filename) {
                warn_user!(
                    "The file `{}` appears to be a `{}` file, but requirements must be specified in `requirements.txt` format.",
                    path.user_display(),
                    filename
                );
            }
        }
        Self::RequirementsTxt(path)
    }

    /// Parse a [`RequirementsSource`] from a `constraints.txt` file.
    pub fn from_constraints_txt(path: PathBuf) -> Self {
        for filename in ["pyproject.toml", "setup.py", "setup.cfg"] {
            if path.ends_with(filename) {
                warn_user!(
                    "The file `{}` appears to be a `{}` file, but constraints must be specified in `requirements.txt` format.",
                    path.user_display(),
                    filename
                );
            }
        }
        Self::RequirementsTxt(path)
    }

    /// Parse a [`RequirementsSource`] from an `overrides.txt` file.
    pub fn from_overrides_txt(path: PathBuf) -> Self {
        for filename in ["pyproject.toml", "setup.py", "setup.cfg"] {
            if path.ends_with(filename) {
                warn_user!(
                    "The file `{}` appears to be a `{}` file, but overrides must be specified in `requirements.txt` format.",
                    path.user_display(),
                    filename
                );
            }
        }
        Self::RequirementsTxt(path)
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
                let confirmation = uv_console::confirm(&prompt, &term, true)?;
                if confirmation {
                    return Ok(Self::from_requirements_file(name.into()));
                }
            }
        }

        // Similarly, if the user provided a `pyproject.toml` file without `-r` (as in
        // `uv pip install pyproject.toml`), prompt them to correct it.
        if (name == "pyproject.toml" || name == "setup.py" || name == "setup.cfg")
            && Path::new(&name).is_file()
        {
            let term = Term::stderr();
            if term.is_term() {
                let prompt = format!(
                    "`{name}` looks like a local metadata file but was passed as a package name. Did you mean `-r {name}`?"
                );
                let confirmation = uv_console::confirm(&prompt, &term, true)?;
                if confirmation {
                    return Ok(Self::from_requirements_file(name.into()));
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
                let confirmation = uv_console::confirm(&prompt, &term, true)?;
                if confirmation {
                    return Ok(Self::from_requirements_file(name.into()));
                }
            }
        }

        // Similarly, if the user provided a `pyproject.toml` file without `--with-requirements` (as in
        // `uvx --with pyproject.toml ruff`), prompt them to correct it.
        if (name == "pyproject.toml" || name == "setup.py" || name == "setup.cfg")
            && Path::new(&name).is_file()
        {
            let term = Term::stderr();
            if term.is_term() {
                let prompt = format!(
                    "`{name}` looks like a local metadata file but was passed as a package name. Did you mean `--with-requirements {name}`?"
                );
                let confirmation = uv_console::confirm(&prompt, &term, true)?;
                if confirmation {
                    return Ok(Self::from_requirements_file(name.into()));
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

    /// Parse a [`RequirementsSource`] from a user-provided string, assumed to be a path to a source
    /// tree.
    pub fn from_source_tree(path: PathBuf) -> Self {
        Self::SourceTree(path)
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
            Self::RequirementsTxt(path)
            | Self::PyprojectToml(path)
            | Self::SetupPy(path)
            | Self::SetupCfg(path)
            | Self::SourceTree(path) => {
                write!(f, "{}", path.simplified_display())
            }
        }
    }
}
