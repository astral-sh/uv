use std::path::{Path, PathBuf};

use console::Term;

use uv_fs::Simplified;
use uv_normalize::ExtraName;
use uv_warnings::warn_user;

use crate::confirm;

#[derive(Debug)]
pub enum RequirementsSource {
    /// A package was provided on the command line (e.g., `pip install flask`).
    Package(String),
    /// An editable path was provided on the command line (e.g., `pip install -e ../flask`).
    Editable(String),
    /// Dependencies were provided via a `requirements.txt` file (e.g., `pip install -r requirements.txt`).
    RequirementsTxt(PathBuf),
    /// Dependencies were provided via a `pyproject.toml` file (e.g., `pip-compile pyproject.toml`).
    PyprojectToml(PathBuf),
    /// Dependencies were provided via a `setup.py` file (e.g., `pip-compile setup.py`).
    SetupPy(PathBuf),
    /// Dependencies were provided via a `setup.cfg` file (e.g., `pip-compile setup.cfg`).
    SetupCfg(PathBuf),
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

    /// Parse a [`RequirementsSource`] from a user-provided string, assumed to be a package.
    ///
    /// If the user provided a value that appears to be a `requirements.txt` file or a local
    /// directory, prompt them to correct it (if the terminal is interactive).
    pub fn from_package(name: String) -> Self {
        // If the user provided a `requirements.txt` file without `-r` (as in
        // `uv pip install requirements.txt`), prompt them to correct it.
        #[allow(clippy::case_sensitive_file_extension_comparisons)]
        if (name.ends_with(".txt") || name.ends_with(".in")) && Path::new(&name).is_file() {
            let term = Term::stderr();
            if term.is_term() {
                let prompt = format!(
                    "`{name}` looks like a requirements file but was passed as a package name. Did you mean `-r {name}`?"
                );
                let confirmation = confirm::confirm(&prompt, &term, true).unwrap();
                if confirmation {
                    return Self::RequirementsTxt(name.into());
                }
            }
        }

        Self::Package(name)
    }

    /// Returns `true` if the source allows extras to be specified.
    pub fn allows_extras(&self) -> bool {
        matches!(
            self,
            Self::PyprojectToml(_) | Self::SetupPy(_) | Self::SetupCfg(_)
        )
    }
}

impl std::fmt::Display for RequirementsSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Package(package) => write!(f, "{package}"),
            Self::Editable(path) => write!(f, "-e {path}"),
            Self::RequirementsTxt(path)
            | Self::PyprojectToml(path)
            | Self::SetupPy(path)
            | Self::SetupCfg(path) => {
                write!(f, "{}", path.simplified_display())
            }
        }
    }
}

#[derive(Debug, Default, Clone)]
pub enum ExtrasSpecification<'a> {
    #[default]
    None,
    All,
    Some(&'a [ExtraName]),
}

impl ExtrasSpecification<'_> {
    /// Returns true if a name is included in the extra specification.
    pub fn contains(&self, name: &ExtraName) -> bool {
        match self {
            ExtrasSpecification::All => true,
            ExtrasSpecification::None => false,
            ExtrasSpecification::Some(extras) => extras.contains(name),
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, ExtrasSpecification::None)
    }
}
