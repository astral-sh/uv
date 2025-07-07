use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::{
    env,
    path::{Component, PathBuf},
    process::Stdio,
};

use anyhow::Error;
use walkdir::WalkDir;
use tracing::info;

pub mod init;

/// This function is intended to function same as println! without emmiting the Clippy warning for
/// using println! in library code. This is needed since the shell function expects stdout for
/// command execution.
#[macro_export]
macro_rules! shell_out {
    ($($arg:tt)*) => {{
        use std::io::{self, Write};
        let mut stdout = io::stdout().lock();
        writeln!(stdout, $($arg)*).expect("Failed to write to stdout");
    }};
}


pub fn envy(jump: bool) -> Result<(), anyhow::Error> {
    // This function is the main entry point for the envy command.
    let user_dir = env::current_dir()?;
    let envs = get_environments(&user_dir);
    if envs.is_empty() {
        return Err(Error::msg("No environments found."));
    } else if envs.len() == 1 || jump {
        info!("Teleporting...");
        shell_out!(
            "<ENVY> source {}",
            envs.first()
                .unwrap()
                .0
                .join(Path::new("activate"))
                .to_str()
                .unwrap()
        );
    } else {
        let mut input: String = String::new();
        for env in envs {
            let new_item = format!("{}\t{}", env.0.to_str().unwrap(), env.1);
            input = format!("{input}\n{new_item}");
        }

        // Step 1: Spawn fzf
        let mut child = std::process::Command::new("fzf")
            .arg("--prompt=Choose env > ")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Step 2: Write choices to fzf's stdin
        {
            let mut stdin = child.stdin.take().expect("Failed to open stdin");
            write!(stdin, "{input}")?;
        }

        // Step 3: Read the selected result from fzf's stdout
        let output = child.wait_with_output()?;
        if output.status.success() {
            let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
            shell_out!(
                "<ENVY> source {}",
                Path::new(selected.split('\t').collect::<Vec<&str>>()[0])
                    .join("activate")
                    .to_str()
                    .unwrap()
            );
        } else {
            return Err(Error::msg("No selection made or fzf was cancelled."));
        }
    }

    Ok(())
}

fn get_environments(current_path: &PathBuf) -> Vec<(PathBuf, usize)> {
    // This function searches for Python virtual environments in the current directory
    let mut all_envs: HashMap<PathBuf, usize> = HashMap::new();

    for entry in WalkDir::new(current_path) {
        let entry = entry.unwrap();
        if entry.file_name() == "python" {
            let components: Vec<Component> = entry.path().components().collect();
            if components[components.len() - 2]
                .as_os_str()
                .to_str()
                .unwrap()
                == "bin"
            {
                all_envs.insert(
                    entry.path().parent().unwrap().to_path_buf(),
                    components.len() - current_path.components().count(),
                );
            }
        }
    }
    let mut envs: Vec<(PathBuf, usize)> = all_envs.into_iter().collect();

    envs.sort_by(|&(_, v1), &(_, v2)| v1.cmp(&v2));
    envs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_get_environments() -> Result<(), Box<dyn std::error::Error>> {
        // Create a temporary directory
        let temp_dir = tempdir()?;

        // Build the directory structure:
        // temp_dir/
        // ├── env1/
        // │   └── bin/
        // │       └── python
        // ├── env2/
        // │   └── bin/
        // │       └── python
        // └── deeper/
        //     └── env3/
        //         └── bin/
        //             └── python

        let env1_bin = temp_dir.path().join("env1").join("bin");
        let env2_bin = temp_dir.path().join("env2").join("bin");
        let deeper_env3_bin = temp_dir.path().join("deeper").join("env3").join("bin");

        fs::create_dir_all(&env1_bin)?;
        fs::create_dir_all(&env2_bin)?;
        fs::create_dir_all(&deeper_env3_bin)?;

        // Create dummy 'python' files
        fs::write(env1_bin.join("python"), "")?;
        fs::write(env2_bin.join("python"), "")?;
        fs::write(deeper_env3_bin.join("python"), "")?;

        // Call your function on the temp_dir
        let envs = get_environments(&temp_dir.path().to_path_buf());

        // Check that it detected exactly 2 environments
        assert_eq!(envs.len(), 3);

        // Extract only the paths for easier assertion
        let detected_paths: Vec<_> = envs.iter().map(|(p, _)| p.clone()).collect();

        // Assert that both env1 and env2 paths are detected
        assert!(detected_paths.contains(&temp_dir.path().join("env1").join("bin")));
        assert!(detected_paths.contains(&temp_dir.path().join("env2").join("bin")));
        assert!(detected_paths.contains(&temp_dir.path().join("deeper").join("env3").join("bin")));

        Ok(())
    }
}
