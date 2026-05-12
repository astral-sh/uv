#![cfg(not(feature = "self-update"))]

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// Known sources for uv installations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstallSource {
    Homebrew,
}

impl InstallSource {
    /// Attempt to infer the install source for the given executable path.
    fn from_path(path: &Path) -> Option<Self> {
        let canonical = path.canonicalize().unwrap_or_else(|_| PathBuf::from(path));

        let components = canonical
            .components()
            .map(|component| component.as_os_str().to_owned())
            .collect::<Vec<_>>();

        let cellar = OsStr::new("Cellar");
        let formula = OsStr::new("uv");

        if components
            .windows(2)
            .any(|window| window[0] == cellar && window[1] == formula)
        {
            return Some(Self::Homebrew);
        }

        None
    }

    /// Detect how uv was installed by inspecting the current executable path.
    pub(crate) fn detect() -> Option<Self> {
        Self::from_path(&std::env::current_exe().ok()?)
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::Homebrew => "Homebrew",
        }
    }

    pub(crate) fn update_instructions(self) -> &'static str {
        match self {
            Self::Homebrew => "brew update && brew upgrade uv",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_homebrew_cellar() {
        assert_eq!(
            InstallSource::from_path(Path::new("/opt/homebrew/Cellar/uv/0.9.11/bin/uv")),
            Some(InstallSource::Homebrew)
        );
    }

    #[test]
    fn ignores_non_cellar_paths() {
        assert_eq!(
            InstallSource::from_path(Path::new("/usr/local/bin/uv")),
            None
        );
    }
}
