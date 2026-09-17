use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use console::Term;

use uv_configuration::RequirementsInput;
use uv_fs::{CWD, Simplified};
use uv_requirements_txt::RequirementsTxtRequirement;

#[derive(Debug, Clone)]
pub enum RequirementsSource {
    /// A package was provided on the command line (e.g., `pip install flask`).
    Package(RequirementsTxtRequirement),
    /// An editable path was provided on the command line (e.g., `pip install -e ../flask`).
    Editable(RequirementsTxtRequirement),
    /// Dependencies were provided via a PEP 723 script.
    Pep723Script(RequirementsInput),
    /// Dependencies were provided via a `pylock.toml` file.
    PylockToml(RequirementsInput),
    /// Dependencies were provided via a `requirements.txt` file (e.g., `pip install -r requirements.txt`).
    RequirementsTxt(RequirementsInput),
    /// Dependencies were provided via a `pyproject.toml` file (e.g., `pip-compile pyproject.toml`).
    PyprojectToml(PathBuf),
    /// Dependencies were provided via a `setup.py` file (e.g., `pip-compile setup.py`).
    SetupPy(PathBuf),
    /// Dependencies were provided via a `setup.cfg` file (e.g., `pip-compile setup.cfg`).
    SetupCfg(PathBuf),
    /// Dependencies were provided via an unsupported Conda `environment.yml` file (e.g., `pip install -r environment.yml`).
    EnvironmentYml(RequirementsInput),
    /// An extensionless file that could be either a PEP 723 script or a requirements.txt file.
    /// We detect the format when reading the file.
    Extensionless(RequirementsInput),
}

impl RequirementsSource {
    /// Parse a [`RequirementsSource`] from a [`RequirementsInput`]. The file type is determined by
    /// the file extension and, in some cases, the file contents.
    pub fn from_requirements_file(input: impl Into<RequirementsInput>) -> Result<Self> {
        let input = input.into();
        match input {
            RequirementsInput::Stdin => Ok(Self::Extensionless(RequirementsInput::Stdin)),
            RequirementsInput::Local(path) => Self::from_local_requirements_file(path),
            RequirementsInput::Remote(url) => {
                let filename = url
                    .path_segments()
                    .and_then(Iterator::last)
                    .unwrap_or("")
                    .to_string();
                if matches!(
                    filename.as_str(),
                    "pyproject.toml" | "setup.py" | "setup.cfg"
                ) {
                    return Err(anyhow::anyhow!(
                        "Remote `{filename}` inputs are not supported: `{url}`"
                    ));
                }

                let extension = filename
                    .rsplit_once('.')
                    .and_then(|(stem, extension)| (!stem.is_empty()).then_some(extension));
                let input = RequirementsInput::Remote(url);
                if filename == "environment.yml" {
                    Ok(Self::EnvironmentYml(input))
                } else if is_pylock_toml(&filename) {
                    Ok(Self::PylockToml(input))
                } else if extension.is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("py") || extension.eq_ignore_ascii_case("pyw")
                }) {
                    Ok(Self::Pep723Script(input))
                } else if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
                {
                    Err(anyhow::anyhow!(
                        "`{}` is not a valid PEP 751 filename: expected `pylock.toml` or `pylock.<name>.toml`, where `<name>` is non-empty and contains no dots",
                        input.user_display(),
                    ))
                } else if extension.is_some() {
                    Ok(Self::RequirementsTxt(input))
                } else {
                    // If we don't have an extension, mark it as extensionless so we can detect
                    // the format later (either a PEP 723 script or a requirements.txt file).
                    Ok(Self::Extensionless(input))
                }
            }
        }
    }

    fn from_local_requirements_file(path: PathBuf) -> Result<Self> {
        if path.ends_with("pyproject.toml") {
            Ok(Self::PyprojectToml(path))
        } else if path.ends_with("setup.py") {
            Ok(Self::SetupPy(path))
        } else if path.ends_with("setup.cfg") {
            Ok(Self::SetupCfg(path))
        } else if path.ends_with("environment.yml") {
            Ok(Self::EnvironmentYml(path.into()))
        } else if path
            .file_name()
            .is_some_and(|file_name| file_name.to_str().is_some_and(is_pylock_toml))
        {
            Ok(Self::PylockToml(path.into()))
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("py") || ext.eq_ignore_ascii_case("pyw"))
        {
            Ok(Self::Pep723Script(path.into()))
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        {
            Err(anyhow::anyhow!(
                "`{}` is not a valid PEP 751 filename: expected `pylock.toml` or `pylock.<name>.toml`, where `<name>` is non-empty and contains no dots",
                path.user_display()
            ))
        } else if path.extension().is_none() {
            // If we don't have an extension, mark it as extensionless so we can detect
            // the format later (either a PEP 723 script or a requirements.txt file).
            Ok(Self::Extensionless(path.into()))
        } else {
            Ok(Self::RequirementsTxt(path.into()))
        }
    }

    /// Parse a [`RequirementsSource`] from a `requirements.txt` file.
    pub fn from_requirements_txt(input: RequirementsInput) -> Result<Self> {
        Self::from_requirements_txt_kind(input, "requirements")
    }

    /// Parse a [`RequirementsSource`] from a `constraints.txt` file.
    pub fn from_constraints_txt(input: RequirementsInput) -> Result<Self> {
        Self::from_requirements_txt_kind(input, "constraints")
    }

    /// Parse a [`RequirementsSource`] from an `overrides.txt` file.
    pub fn from_overrides_txt(input: RequirementsInput) -> Result<Self> {
        Self::from_requirements_txt_kind(input, "overrides")
    }

    fn from_requirements_txt_kind(input: RequirementsInput, kind: &str) -> Result<Self> {
        let filename = match &input {
            RequirementsInput::Stdin => return Ok(Self::Extensionless(input)),
            RequirementsInput::Local(path) => path.file_name().and_then(OsStr::to_str),
            RequirementsInput::Remote(url) => url.path_segments().and_then(Iterator::last),
        };
        for file_name in ["pyproject.toml", "setup.py", "setup.cfg"] {
            if filename == Some(file_name) {
                return Err(anyhow::anyhow!(
                    "The file `{}` appears to be a `{file_name}` file, but {kind} must be specified in `requirements.txt` format",
                    input.user_display(),
                ));
            }
        }
        if filename.is_some_and(is_pylock_toml) {
            return Err(anyhow::anyhow!(
                "The file `{}` appears to be a `pylock.toml` file, but {kind} must be specified in `requirements.txt` format",
                input.user_display(),
            ));
        } else if filename
            .and_then(|filename| filename.rsplit_once('.'))
            .filter(|(stem, _)| !stem.is_empty())
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("toml"))
        {
            return Err(anyhow::anyhow!(
                "The file `{}` appears to be a TOML file, but {kind} must be specified in `requirements.txt` format",
                input.user_display(),
            ));
        }
        Ok(Self::RequirementsTxt(input))
    }

    /// Parse a [`RequirementsSource`] from a user-provided string, assumed to be a positional
    /// package (e.g., `uv pip install flask`).
    ///
    /// If the user provided a value that appears to be a `requirements.txt` file or a local
    /// directory, prompt them to correct it (if the terminal is interactive).
    pub fn from_package_argument(name: &str) -> Result<Self> {
        // If the user provided a `requirements.txt` file without `-r` (as in
        // `uv pip install requirements.txt`), prompt them to correct it.
        #[expect(clippy::case_sensitive_file_extension_comparisons)]
        if (name.ends_with(".txt") || name.ends_with(".in")) && Path::new(&name).is_file() {
            let term = Term::stderr();
            if term.is_term() {
                let prompt = format!(
                    "`{name}` looks like a local requirements file but was passed as a package name. Did you mean `-r {name}`?"
                );
                let confirmation =
                    uv_console::confirm(&prompt, &term, true).context("Confirm prompt failed")?;
                if confirmation {
                    return Self::from_requirements_file(PathBuf::from(name));
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
                    return Self::from_requirements_file(PathBuf::from(name));
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
        #[expect(clippy::case_sensitive_file_extension_comparisons)]
        if (name.ends_with(".txt") || name.ends_with(".in")) && Path::new(&name).is_file() {
            let term = Term::stderr();
            if term.is_term() {
                let prompt = format!(
                    "`{name}` looks like a local requirements file but was passed as a package name. Did you mean `--with-requirements {name}`?"
                );
                let confirmation =
                    uv_console::confirm(&prompt, &term, true).context("Confirm prompt failed")?;
                if confirmation {
                    return Self::from_requirements_file(PathBuf::from(name));
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
                    return Self::from_requirements_file(PathBuf::from(name));
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
            Self::PylockToml(_) | Self::PyprojectToml(_) | Self::SetupPy(_) | Self::SetupCfg(_)
        )
    }
}

impl std::fmt::Display for RequirementsSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Package(package) => write!(f, "{package:?}"),
            Self::Editable(path) => write!(f, "-e {path:?}"),
            Self::PylockToml(path)
            | Self::RequirementsTxt(path)
            | Self::Pep723Script(path)
            | Self::EnvironmentYml(path)
            | Self::Extensionless(path) => path.user_display().fmt(f),
            Self::PyprojectToml(path) | Self::SetupPy(path) | Self::SetupCfg(path) => {
                path.display().fmt(f)
            }
        }
    }
}

/// Returns `true` if a file name matches the `pylock.toml` pattern defined in PEP 751.
pub fn is_pylock_toml(file_name: &str) -> bool {
    if file_name == "pylock.toml" {
        return true;
    }

    file_name
        .strip_prefix("pylock.")
        .and_then(|name| name.strip_suffix(".toml"))
        .is_some_and(|name| !name.is_empty() && !name.contains('.'))
}
