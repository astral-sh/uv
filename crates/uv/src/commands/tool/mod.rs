use std::str::FromStr;

use tracing::debug;

use uv_normalize::{ExtraName, PackageName};
use uv_pep440::Version;
use uv_python::PythonRequest;

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
pub(crate) enum ToolRequest<'a> {
    // Running the interpreter directly e.g. `uvx python` or `uvx pypy@3.8`
    Python {
        /// The executable name (e.g., `bash`), if the interpreter was given via --from.
        executable: Option<&'a str>,
        // The interpreter to install or run (e.g., `python@3.8` or `pypy311`.
        request: PythonRequest,
    },
    // Running a Python package
    Package {
        /// The executable name (e.g., `ruff`), if the target was given via --from.
        executable: Option<&'a str>,
        /// The target to install or run (e.g., `ruff@latest` or `ruff==0.6.0`).
        target: Target<'a>,
    },
}

impl<'a> ToolRequest<'a> {
    /// Parse a tool request into an executable name and a target.
    pub(crate) fn parse(command: &'a str, from: Option<&'a str>) -> anyhow::Result<Self> {
        // If --from is used, the command could be an arbitrary binary in the PATH (e.g. `bash`),
        // and we don't try to parse it.
        let (component_to_parse, executable) = match from {
            Some(from) => (from, Some(command)),
            None => (command, None),
        };

        // First try parsing the command as a Python interpreter, like `python`, `python39`, or
        // `pypy@39`. `pythonw` is also allowed on Windows. This overlaps with how `--python` flag
        // values are parsed, but see `PythonRequest::parse` vs `PythonRequest::try_from_tool_name`
        // for the differences.
        if let Some(python_request) = PythonRequest::try_from_tool_name(component_to_parse)? {
            Ok(Self::Python {
                request: python_request,
                executable,
            })
        } else {
            // Otherwise the command is a Python package, like `ruff` or `ruff@0.6.0`.
            Ok(Self::Package {
                target: Target::parse(component_to_parse),
                executable,
            })
        }
    }

    /// Returns `true` if the target is `latest`.
    pub(crate) fn is_latest(&self) -> bool {
        matches!(
            self,
            Self::Package {
                target: Target::Latest(..),
                ..
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target<'a> {
    /// e.g., `ruff`
    Unspecified(&'a str),
    /// e.g., `ruff[extra]@0.6.0`
    Version(&'a str, PackageName, Box<[ExtraName]>, Version),
    /// e.g., `ruff[extra]@latest`
    Latest(&'a str, PackageName, Box<[ExtraName]>),
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
            .collect::<Result<Box<_>, _>>()
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
            Box::new([]),
            Version::new([3, 0, 0]),
        );
        assert_eq!(target, expected);

        let target = Target::parse("flask@3.0.0");
        let expected = Target::Version(
            "flask",
            PackageName::from_str("flask").unwrap(),
            Box::new([]),
            Version::new([3, 0, 0]),
        );
        assert_eq!(target, expected);

        let target = Target::parse("flask@latest");
        let expected = Target::Latest(
            "flask",
            PackageName::from_str("flask").unwrap(),
            Box::new([]),
        );
        assert_eq!(target, expected);

        let target = Target::parse("flask[dotenv]@3.0.0");
        let expected = Target::Version(
            "flask",
            PackageName::from_str("flask").unwrap(),
            Box::new([ExtraName::from_str("dotenv").unwrap()]),
            Version::new([3, 0, 0]),
        );
        assert_eq!(target, expected);

        let target = Target::parse("flask[dotenv]@latest");
        let expected = Target::Latest(
            "flask",
            PackageName::from_str("flask").unwrap(),
            Box::new([ExtraName::from_str("dotenv").unwrap()]),
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
