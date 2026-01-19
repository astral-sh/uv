//! Markdown-based integration tests for uv.
//!
//! This test runner uses the `uv-mdtest` crate to run tests defined in markdown files.
//! Tests are located in `test/uv/` at the workspace root.
//!
//! Each section in a markdown file becomes a separate test, allowing full parallelism
//! with nextest.

#![expect(clippy::print_stderr)]

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use fs_err as fs;
use libtest_mimic::{Arguments, Failed, Trial};
use regex::Regex;
use walkdir::WalkDir;

use uv_mdtest::{MarkdownTestFile, PythonVersions, SnapshotMode, SnapshotUpdater};
use uv_test::{TestContext, get_bin};

/// Convert a test name to a URL-friendly slug (like markdown anchors).
fn slugify(name: &str) -> String {
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn main() {
    let args = Arguments::from_args();
    let snapshot_mode = SnapshotMode::from_env();

    // Create a shared snapshot updater for batch updates
    let updater = Arc::new(SnapshotUpdater::new());

    // Find all markdown test files
    // mdtest files live in the workspace root's test/uv/ directory
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .and_then(Path::parent) // workspace root
        .expect("Failed to find workspace root")
        .join("test/uv");
    let mut trials = Vec::new();

    for entry in WalkDir::new(&test_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        let path = entry.path().to_path_buf();
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));

        let test_file = MarkdownTestFile::parse(path.clone(), &source)
            .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", path.display()));

        // Get relative path for display
        let relative_path = path
            .strip_prefix(&test_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        // Get enabled features from environment variable (comma-separated)
        // Example: UV_MDTEST_FEATURES=python-patch,other-feature
        let enabled_features: Vec<String> = std::env::var("UV_MDTEST_FEATURES")
            .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();

        // Create one trial per test (section)
        for test in test_file.tests {
            let test_name = format!("{}#{}", relative_path, slugify(&test.name));
            let path = path.clone();
            let should_run = test.should_run(&enabled_features);
            let test = Arc::new(test);
            let updater = Arc::clone(&updater);

            let mut trial = Trial::test(test_name, move || {
                run_single_test(&path, &test, snapshot_mode, &updater)
            });

            // Skip tests that don't match the current platform (target-os, target-family)
            if !should_run {
                trial = trial.with_ignored_flag(true);
            }

            trials.push(trial);
        }
    }

    let conclusion = libtest_mimic::run(&args, trials);

    // Commit all snapshot updates after tests complete
    if snapshot_mode == SnapshotMode::Update {
        // Try to unwrap the Arc - if there are still references, wait isn't possible
        // In practice, all test closures should be done at this point
        match Arc::try_unwrap(updater) {
            Ok(updater) => match updater.commit() {
                Ok(updated_files) => {
                    for file in updated_files {
                        eprintln!("Updated snapshots in: {}", file.display());
                    }
                }
                Err(e) => {
                    eprintln!("Failed to commit snapshot updates: {e}");
                }
            },
            Err(_) => {
                eprintln!("Warning: Could not commit snapshots - updater still has references");
            }
        }
    }

    conclusion.exit();
}

/// Run a single markdown test section.
fn run_single_test(
    path: &Path,
    test: &uv_mdtest::MarkdownTest,
    snapshot_mode: SnapshotMode,
    updater: &SnapshotUpdater,
) -> Result<(), Failed> {
    // Get the Python versions from test config
    let python_versions: Vec<String> = match &test.config.environment.python_versions {
        PythonVersions::Default => vec!["3.12".to_string()],
        PythonVersions::None => vec![],
        PythonVersions::Only(versions) => versions.clone(),
    };
    let version_strs: Vec<&str> = python_versions
        .iter()
        .map(std::string::String::as_str)
        .collect();

    // Create a TestContext for this test - this handles all the proper setup
    // Use new_with_versions_and_bin to avoid automatic venv creation, then conditionally create it
    let mut context = TestContext::new_with_versions_and_bin(&version_strs, get_bin!());
    if test.config.environment.create_venv.unwrap_or(true) {
        context.reset_venv();
    }

    // Apply environment options from test config
    if let Some(exclude_newer) = &test.config.environment.exclude_newer {
        context = context.with_exclude_newer(exclude_newer);
    }
    if let Some(http_timeout) = &test.config.environment.http_timeout {
        context = context.with_http_timeout(http_timeout);
    }
    if let Some(concurrent_installs) = &test.config.environment.concurrent_installs {
        context = context.with_concurrent_installs(concurrent_installs);
    }

    // Apply custom environment variables from test config
    for (key, value) in &test.config.environment.env {
        context = context.with_env(key, value);
    }

    // Remove environment variables from test config
    for key in &test.config.environment.env_remove {
        context = context.with_env_remove(key);
    }

    // Apply filter options from test config
    let filters_config = &test.config.filters;
    if filters_config.counts {
        context = context.with_filtered_counts();
    }
    if filters_config.exe_suffix {
        context = context.with_filtered_exe_suffix();
    }
    if filters_config.python_names {
        context = context.with_filtered_python_names();
    }
    if filters_config.virtualenv_bin {
        context = context.with_filtered_virtualenv_bin();
    }
    if filters_config.python_install_bin {
        context = context.with_filtered_python_install_bin();
    }
    if filters_config.python_sources {
        context = context.with_filtered_python_sources();
    }
    if filters_config.pyvenv_cfg {
        context = context.with_pyvenv_cfg_filters();
    }
    if filters_config.link_mode_warning {
        context = context.with_filtered_link_mode_warning();
    }
    if filters_config.not_executable {
        context = context.with_filtered_not_executable();
    }
    if filters_config.python_keys {
        context = context.with_filtered_python_keys();
    }
    if filters_config.latest_python_versions {
        context = context.with_filtered_latest_python_versions();
    }
    if filters_config.compiled_file_count {
        context = context.with_filtered_compiled_file_count();
    }
    if filters_config.cyclonedx {
        context = context.with_cyclonedx_filters();
    }
    if filters_config.collapse_whitespace {
        context = context.with_collapsed_whitespace();
    }
    if filters_config.cache_size {
        context = context.with_filtered_cache_size();
    }
    if filters_config.missing_file_error {
        context = context.with_filtered_missing_file_error();
    }

    // Build filters from TestContext
    let context_filters = context.filters();
    let mut filters: Vec<(Regex, String)> = Vec::new();
    for (pattern, replacement) in context_filters {
        if let Ok(regex) = Regex::new(pattern) {
            filters.push((regex, replacement.to_string()));
        }
    }

    // Build a command builder that uses TestContext
    // Note: File creation is now handled by the runner in document order
    let command_builder = |cmd_str: &str| -> Command {
        // Parse the command - first word is the command name
        let parts: Vec<&str> = cmd_str.split_whitespace().collect();
        if parts.is_empty() {
            return Command::new("false");
        }

        let cmd_name = parts[0];
        let args = &parts[1..];

        // Use TestContext's command method for uv commands
        if cmd_name == "uv" {
            let mut command = context.command();
            command.args(args);
            command
        } else {
            // For non-uv commands, still set up the environment from TestContext
            let mut command = Command::new(cmd_name);
            command.args(args);
            context.add_shared_options(&mut command, true);
            command
        }
    };

    // Run the test using the command builder
    let result = test
        .run_with_command_builder(context.temp_dir.path(), &filters, command_builder)
        .map_err(|e| {
            Failed::from(format!(
                "Test execution failed at {}:{}: {}",
                path.display(),
                test.line_number,
                e
            ))
        })?;

    if !result.passed {
        if let Some(mismatch) = &result.mismatch {
            if snapshot_mode == SnapshotMode::Update {
                // Queue the snapshot update (will be committed after all tests)
                updater.add(path, mismatch);
            } else {
                // Return the mismatch as a failure
                return Err(Failed::from(format!(
                    "Test failed at {}:{}\n\n{}",
                    path.display(),
                    test.line_number,
                    mismatch.format()
                )));
            }
        }
    }

    Ok(())
}
