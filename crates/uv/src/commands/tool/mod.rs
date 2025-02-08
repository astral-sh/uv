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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target<'a> {
    /// e.g., `ruff`
    Unspecified(&'a str),
    /// e.g., `ruff[extra]@0.6.0`
    Version(&'a str, Vec<ExtraName>, Version),
    /// e.g., `ruff[extra]@latest`
    Latest(&'a str, Vec<ExtraName>),
    /// e.g., `ruff --from ruff[extra]>=0.6.0`
    From(&'a str, &'a str),
    /// e.g., `ruff --from ruff[extra]@0.6.0`
    FromVersion(&'a str, &'a str, Vec<ExtraName>, Version),
    /// e.g., `ruff --from ruff[extra]@latest`
    FromLatest(&'a str, &'a str, Vec<ExtraName>),
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

            // Split into name and extras (e.g., `flask[dotenv]`).
            let (name, extras) = match name.split_once('[') {
                Some((name, extras)) => {
                    let Some(extras) = extras.strip_suffix(']') else {
                        // e.g., ignore `flask[dotenv`.
                        debug!("Ignoring invalid extras in `--from`");
                        return Self::From(target, from);
                    };
                    (name, extras)
                }
                None => (name, ""),
            };

            // e.g., ignore `git+https://github.com/astral-sh/ruff.git@main`
            if PackageName::from_str(name).is_err() {
                debug!("Ignoring non-package name `{name}` in `--from`");
                return Self::From(target, from);
            }

            // e.g., ignore `ruff[1.0.0]` or any other invalid extra.
            let Ok(extras) = extras
                .split(',')
                .map(str::trim)
                .filter(|extra| !extra.is_empty())
                .map(ExtraName::from_str)
                .collect::<Result<Vec<_>, _>>()
            else {
                debug!("Ignoring invalid extras `{extras}` in `--from`");
                return Self::From(target, from);
            };

            match version {
                // e.g., `ruff@latest`
                "latest" => return Self::FromLatest(target, name, extras),
                // e.g., `ruff@0.6.0`
                version => {
                    if let Ok(version) = Version::from_str(version) {
                        return Self::FromVersion(target, name, extras, version);
                    }
                }
            };

            // e.g. `--from ruff@invalid`, warn and treat the whole thing as the command
            debug!("Ignoring invalid version request `{version}` in `--from`");
            return Self::From(target, from);
        }

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
        let (name, extras) = match name.split_once('[') {
            Some((name, extras)) => {
                let Some(extras) = extras.strip_suffix(']') else {
                    // e.g., ignore `flask[dotenv`.
                    return Self::Unspecified(name);
                };
                (name, extras)
            }
            None => (name, ""),
        };

        // e.g., ignore `git+https://github.com/astral-sh/ruff.git@main`
        if PackageName::from_str(name).is_err() {
            debug!("Ignoring non-package name `{name}` in command");
            return Self::Unspecified(target);
        }

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
            "latest" => return Self::Latest(name, extras),
            // e.g., `ruff@0.6.0`
            version => {
                if let Ok(version) = Version::from_str(version) {
                    return Self::Version(name, extras, version);
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
            Self::Unspecified(name) => {
                // Identify the package name from the PEP 508 specifier.
                //
                // For example, given `ruff>=0.6.0`, extract `ruff`, to use as the executable name.
                let index = name
                    .find(|c| !matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.'))
                    .unwrap_or(name.len());
                &name[..index]
            }
            Self::Version(name, _, _) => name,
            Self::Latest(name, _) => name,
            Self::FromVersion(name, _, _, _) => name,
            Self::FromLatest(name, _, _) => name,
            Self::From(name, _) => name,
        }
    }

    /// Returns whether the target package is Python.
    pub(crate) fn is_python(&self) -> bool {
        let name = match self {
            Self::Unspecified(name) => {
                // Identify the package name from the PEP 508 specifier.
                //
                // For example, given `ruff>=0.6.0`, extract `ruff`, to use as the executable name.
                let index = name
                    .find(|c| !matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.'))
                    .unwrap_or(name.len());
                &name[..index]
            }
            Self::Version(name, _, _) => name,
            Self::Latest(name, _) => name,
            Self::FromVersion(_, name, _, _) => name,
            Self::FromLatest(_, name, _) => name,
            Self::From(_, name) => name,
        };
        name.eq_ignore_ascii_case("python") || cfg!(windows) && name.eq_ignore_ascii_case("pythonw")
    }

    /// Returns `true` if the target is `latest`.
    fn is_latest(&self) -> bool {
        matches!(self, Self::Latest(..) | Self::FromLatest(..))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target() {
        let target = Target::parse("flask", None);
        let expected = Target::Unspecified("flask");
        assert_eq!(target, expected);

        let target = Target::parse("flask@3.0.0", None);
        let expected = Target::Version("flask", vec![], Version::new([3, 0, 0]));
        assert_eq!(target, expected);

        let target = Target::parse("flask@3.0.0", None);
        let expected = Target::Version("flask", vec![], Version::new([3, 0, 0]));
        assert_eq!(target, expected);

        let target = Target::parse("flask@latest", None);
        let expected = Target::Latest("flask", vec![]);
        assert_eq!(target, expected);

        let target = Target::parse("flask[dotenv]@3.0.0", None);
        let expected = Target::Version(
            "flask",
            vec![ExtraName::from_str("dotenv").unwrap()],
            Version::new([3, 0, 0]),
        );
        assert_eq!(target, expected);

        let target = Target::parse("flask[dotenv]@latest", None);
        let expected = Target::Latest("flask", vec![ExtraName::from_str("dotenv").unwrap()]);
        assert_eq!(target, expected);

        // Missing a closing `]`.
        let target = Target::parse("flask[dotenv", None);
        let expected = Target::Unspecified("flask[dotenv");
        assert_eq!(target, expected);

        // Too many `]`.
        let target = Target::parse("flask[dotenv]]", None);
        let expected = Target::Unspecified("flask[dotenv]]");
        assert_eq!(target, expected);
    }
}
