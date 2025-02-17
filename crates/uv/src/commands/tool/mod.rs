use std::str::FromStr;

use tracing::debug;

use uv_normalize::{ExtraName, PackageName};
use uv_pep440::Version;

mod common;
pub(crate) mod dir;
pub(crate) mod install;
pub(crate) mod list;
pub(crate) mod run;
pub(crate) mod uninstall;
pub(crate) mod update_shell;
pub(crate) mod upgrade;

/// A request to run or install a tool (e.g., `uvx ruff@latest`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolRequest<'a> {
    /// The executable name (e.g., `ruff`), if specified explicitly.
    pub(crate) executable: Option<&'a str>,
    /// The target to install or run (e.g., `ruff@latest` or `ruff==0.6.0`).
    pub(crate) target: Target<'a>,
}

impl<'a> ToolRequest<'a> {
    /// Parse a tool request into an executable name and a target.
    pub(crate) fn parse(command: &'a str, from: Option<&'a str>) -> Self {
        if let Some(from) = from {
            let target = Target::parse(from);
            Self {
                executable: Some(command),
                target,
            }
        } else {
            let target = Target::parse(command);
            Self {
                executable: None,
                target,
            }
        }
    }

    /// Returns whether the target package is Python.
    pub(crate) fn is_python(&self) -> bool {
        let name = match self.target {
            Target::Unspecified(name) => name,
            Target::Version(name, ..) => name,
            Target::Latest(name, ..) => name,
        };
        name.eq_ignore_ascii_case("python") || cfg!(windows) && name.eq_ignore_ascii_case("pythonw")
    }

    /// Returns `true` if the target is `latest`.
    pub(crate) fn is_latest(&self) -> bool {
        matches!(self.target, Target::Latest(..))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target<'a> {
    /// e.g., `ruff`
    Unspecified(&'a str),
    /// e.g., `ruff[extra]@0.6.0`
    Version(&'a str, PackageName, Vec<ExtraName>, Version),
    /// e.g., `ruff[extra]@latest`
    Latest(&'a str, PackageName, Vec<ExtraName>),
}

impl<'a> Target<'a> {
    /// Parse a target into a command name and a requirement.
    pub(crate) fn parse(target: &'a str) -> Self {
        // e.g. `ruff`, no special handling
        let Some((name, version)) = target.split_once('@') else {
            return Self::Unspecified(target);
        };

        // e.g. `ruff@`, warn and treat the whole thing as the command
        if version.is_empty() {
            debug!("Ignoring empty version request in command");
            return Self::Unspecified(target);
        }

        // Split into name and extras (e.g., `flask[dotenv]`).
        let (executable, extras) = match name.split_once('[') {
            Some((executable, extras)) => {
                let Some(extras) = extras.strip_suffix(']') else {
                    // e.g., ignore `flask[dotenv`.
                    return Self::Unspecified(target);
                };
                (executable, extras)
            }
            None => (name, ""),
        };

        // e.g., ignore `git+https://github.com/astral-sh/ruff.git@main`
        let Ok(name) = PackageName::from_str(executable) else {
            debug!("Ignoring non-package name `{name}` in command");
            return Self::Unspecified(target);
        };

        // e.g., ignore `ruff[1.0.0]` or any other invalid extra.
        let Ok(extras) = extras
            .split(',')
            .map(str::trim)
            .filter(|extra| !extra.is_empty())
            .map(ExtraName::from_str)
            .collect::<Result<Vec<_>, _>>()
        else {
            debug!("Ignoring invalid extras `{extras}` in command");
            return Self::Unspecified(target);
        };

        match version {
            // e.g., `ruff@latest`
            "latest" => Self::Latest(executable, name, extras),
            // e.g., `ruff@0.6.0`
            version => {
                if let Ok(version) = Version::from_str(version) {
                    Self::Version(executable, name, extras, version)
                } else {
                    // e.g. `ruff@invalid`, warn and treat the whole thing as the command
                    debug!("Ignoring invalid version request `{version}` in command");
                    Self::Unspecified(target)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target() {
        let target = Target::parse("flask");
        let expected = Target::Unspecified("flask");
        assert_eq!(target, expected);

        let target = Target::parse("flask@3.0.0");
        let expected = Target::Version(
            "flask",
            PackageName::from_str("flask").unwrap(),
            vec![],
            Version::new([3, 0, 0]),
        );
        assert_eq!(target, expected);

        let target = Target::parse("flask@3.0.0");
        let expected = Target::Version(
            "flask",
            PackageName::from_str("flask").unwrap(),
            vec![],
            Version::new([3, 0, 0]),
        );
        assert_eq!(target, expected);

        let target = Target::parse("flask@latest");
        let expected = Target::Latest("flask", PackageName::from_str("flask").unwrap(), vec![]);
        assert_eq!(target, expected);

        let target = Target::parse("flask[dotenv]@3.0.0");
        let expected = Target::Version(
            "flask",
            PackageName::from_str("flask").unwrap(),
            vec![ExtraName::from_str("dotenv").unwrap()],
            Version::new([3, 0, 0]),
        );
        assert_eq!(target, expected);

        let target = Target::parse("flask[dotenv]@latest");
        let expected = Target::Latest(
            "flask",
            PackageName::from_str("flask").unwrap(),
            vec![ExtraName::from_str("dotenv").unwrap()],
        );
        assert_eq!(target, expected);

        // Missing a closing `]`.
        let target = Target::parse("flask[dotenv");
        let expected = Target::Unspecified("flask[dotenv");
        assert_eq!(target, expected);

        // Too many `]`.
        let target = Target::parse("flask[dotenv]]");
        let expected = Target::Unspecified("flask[dotenv]]");
        assert_eq!(target, expected);
    }
}
