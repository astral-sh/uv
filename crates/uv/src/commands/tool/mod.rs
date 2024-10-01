use std::str::FromStr;

use tracing::debug;

use pep440_rs::Version;
use uv_normalize::PackageName;

mod common;
pub(crate) mod dir;
pub(crate) mod install;
pub(crate) mod list;
pub(crate) mod run;
pub(crate) mod uninstall;
pub(crate) mod update_shell;
pub(crate) mod upgrade;

#[derive(Debug, Clone)]
pub(crate) enum Target<'a> {
    /// e.g., `ruff`
    Unspecified(&'a str),
    /// e.g., `ruff@0.6.0`
    Version(&'a str, Version),
    /// e.g., `ruff@latest`
    Latest(&'a str),
    /// e.g., `ruff --from ruff>=0.6.0`
    From(&'a str, &'a str),
    /// e.g., `ruff --from ruff@0.6.0`
    FromVersion(&'a str, &'a str, Version),
    /// e.g., `ruff --from ruff@latest`
    FromLatest(&'a str, &'a str),
    /// e.g., `python`
    Python,
    /// e.g., `python@3.13.0`
    FromPythonVersion(Version),
}

impl<'a> Target<'a> {
    /// Parse a target into a command name and a requirement.
    pub(crate) fn parse(target: &'a str, from: Option<&'a str>) -> Self {
        if let Some(from) = from {
            // e.g. `--from ruff`, no special handling
            let Some((name, version)) = from.split_once('@') else {
                return Self::From(target, from);
            };

            // e.g. `--from ruff@`, warn and treat the whole thing as the command
            if version.is_empty() {
                debug!("Ignoring empty version request in `--from`");
                return Self::From(target, from);
            }

            // e.g., ignore `git+https://github.com/astral-sh/ruff.git@main`
            if PackageName::from_str(name).is_err() {
                debug!("Ignoring non-package name `{name}` in `--from`");
                return Self::From(target, from);
            }

            match version {
                // e.g., `ruff@latest`
                "latest" => return Self::FromLatest(target, name),
                // e.g., `ruff@0.6.0`
                version => {
                    if let Ok(version) = Version::from_str(version) {
                        return Self::FromVersion(target, name, version);
                    }
                }
            };

            // e.g. `--from ruff@invalid`, warn and treat the whole thing as the command
            debug!("Ignoring invalid version request `{version}` in `--from`");
            return Self::From(target, from);
        }

        if let Some(target) = Target::handle_interpreter_request(target) {
            return target;
        }

        // e.g. `ruff` or `python`, no special handling
        let Some((name, version)) = target.split_once('@') else {
            return Self::Unspecified(target);
        };

        // e.g. `ruff@`, warn and treat the whole thing as the command
        if version.is_empty() {
            debug!("Ignoring empty version request in command");
            return Self::Unspecified(target);
        }

        // e.g., ignore `git+https://github.com/astral-sh/ruff.git@main`
        if PackageName::from_str(name).is_err() {
            debug!("Ignoring non-package name `{name}` in command");
            return Self::Unspecified(target);
        }

        match version {
            // e.g., `ruff@latest`
            "latest" => return Self::Latest(name),
            // e.g., `ruff@0.6.0` or `python@3.13.0`
            version => {
                if let Ok(version) = Version::from_str(version) {
                    return Self::Version(name, version);
                }
            }
        };

        // e.g. `ruff@invalid`, warn and treat the whole thing as the command
        debug!("Ignoring invalid version request `{version}` in command");
        Self::Unspecified(target)
    }

    /// Returns the name of the executable.
    pub(crate) fn executable(&self) -> &str {
        match self {
            Self::Unspecified(name) => name,
            Self::Version(name, _) => name,
            Self::Latest(name) => name,
            Self::FromVersion(name, _, _) => name,
            Self::FromLatest(name, _) => name,
            Self::From(name, _) => name,
            Self::Python | Self::FromPythonVersion(_) => "python",
        }
    }

    /// Returns `true` if the target is `latest`.
    fn is_latest(&self) -> bool {
        matches!(self, Self::Latest(_) | Self::FromLatest(_, _))
    }

    /// `uvx python` or `uvx python@3.13.0`
    fn handle_interpreter_request(target: &'a str) -> Option<Self> {
        let Some((name, version)) = target.split_once('@') else {
            match target {
                "python" => return Some(Self::Python),
                _ => return None,
            }
        };
        match name {
            "python" => {
                if let Ok(version) = Version::from_str(version) {
                    Some(Self::FromPythonVersion(version))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
