use std::path::PathBuf;

use tracing::debug;
use uv_fs::Simplified;

/// A collection of `.env` file paths.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct EnvFile {
    /// The paths from the `env-file` setting, which are skipped if they don't exist.
    settings: Vec<PathBuf>,
    /// The paths from the command-line or `UV_ENV_FILE`, which must exist.
    cli: Vec<PathBuf>,
}

impl EnvFile {
    /// Parse the env file paths from command-line arguments and the `env-file` setting.
    pub fn from_args(
        env_file: Vec<String>,
        no_env_file: bool,
        settings_env_file: Option<Vec<PathBuf>>,
    ) -> Self {
        if no_env_file {
            return Self::default();
        }

        Self {
            settings: settings_env_file.unwrap_or_default(),
            cli: Self::parse_args(env_file),
        }
    }

    /// Split the command-line arguments into paths.
    fn parse_args(env_file: Vec<String>) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Split on spaces, but respect backslashes.
        for env_file in env_file {
            let mut current = String::new();
            let mut escape = false;
            for c in env_file.chars() {
                if escape {
                    current.push(c);
                    escape = false;
                } else if c == '\\' {
                    escape = true;
                } else if c.is_whitespace() {
                    if !current.is_empty() {
                        paths.push(PathBuf::from(current));
                        current = String::new();
                    }
                } else {
                    current.push(c);
                }
            }
            if !current.is_empty() {
                paths.push(PathBuf::from(current));
            }
        }

        paths
    }

    /// Return the paths to the environment files to load, in order of increasing precedence.
    ///
    /// Files from the `env-file` setting come first and are omitted if they don't exist, followed
    /// by the files from the command-line.
    pub fn paths(&self) -> Vec<PathBuf> {
        self.settings
            .iter()
            .filter(|path| {
                if path.is_file() {
                    true
                } else {
                    debug!(
                        "Skipping missing environment file from settings: {}",
                        path.simplified_display()
                    );
                    false
                }
            })
            .chain(&self.cli)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_args_default() {
        let env_file = EnvFile::from_args(vec![], false, None);
        assert_eq!(env_file, EnvFile::default());
    }

    #[test]
    fn test_from_args_no_env_file() {
        let env_file = EnvFile::from_args(vec!["path1 path2".to_string()], true, None);
        assert_eq!(env_file, EnvFile::default());
    }

    #[test]
    fn test_from_args_empty_string() {
        let env_file = EnvFile::from_args(vec![String::new()], false, None);
        assert_eq!(env_file, EnvFile::default());
    }

    #[test]
    fn test_from_args_whitespace_only() {
        let env_file = EnvFile::from_args(vec!["   ".to_string()], false, None);
        assert_eq!(env_file, EnvFile::default());
    }

    #[test]
    fn test_from_args_single_path() {
        let env_file = EnvFile::from_args(vec!["path1".to_string()], false, None);
        assert_eq!(env_file.cli, vec![PathBuf::from("path1")]);
    }

    #[test]
    fn test_from_args_multiple_paths() {
        let env_file = EnvFile::from_args(vec!["path1 path2 path3".to_string()], false, None);
        assert_eq!(
            env_file.cli,
            vec![
                PathBuf::from("path1"),
                PathBuf::from("path2"),
                PathBuf::from("path3")
            ]
        );
    }

    #[test]
    fn test_from_args_escaped_spaces() {
        let env_file = EnvFile::from_args(vec![r"path\ with\ spaces".to_string()], false, None);
        assert_eq!(env_file.cli, vec![PathBuf::from("path with spaces")]);
    }

    #[test]
    fn test_from_args_mixed_escaped_and_normal() {
        let env_file = EnvFile::from_args(
            vec![r"path1 path\ with\ spaces path2".to_string()],
            false,
            None,
        );
        assert_eq!(
            env_file.cli,
            vec![
                PathBuf::from("path1"),
                PathBuf::from("path with spaces"),
                PathBuf::from("path2")
            ]
        );
    }

    #[test]
    fn test_from_args_escaped_backslash() {
        let env_file =
            EnvFile::from_args(vec![r"path\\with\\backslashes".to_string()], false, None);
        assert_eq!(env_file.cli, vec![PathBuf::from(r"path\with\backslashes")]);
    }

    #[test]
    fn test_from_args_no_env_file_with_settings() {
        let env_file = EnvFile::from_args(vec![], true, Some(vec![PathBuf::from("path1")]));
        assert_eq!(env_file, EnvFile::default());
    }

    #[test]
    fn test_paths() {
        let env_file = EnvFile {
            settings: vec![],
            cli: vec![PathBuf::from("path1"), PathBuf::from("path2")],
        };
        let paths = env_file.paths();
        assert_eq!(paths, [PathBuf::from("path1"), PathBuf::from("path2")]);
    }
}
