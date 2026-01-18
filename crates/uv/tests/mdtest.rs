//! Markdown-based integration tests for uv.
//!
//! This test runner uses the `uv-mdtest` crate to run tests defined in markdown files.
//! Tests are located in `crates/uv/tests/mdtest/`.
//!
//! Each section in a markdown file becomes a separate test, allowing full parallelism
//! with nextest.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use fs_err as fs;
use libtest_mimic::{Arguments, Failed, Trial};
use regex::Regex;
use walkdir::WalkDir;

use uv_mdtest::{MarkdownTestFile, SnapshotMode, SnapshotUpdater};
use uv_test::{TestContext, get_bin};

fn main() {
    let args = Arguments::from_args();
    let snapshot_mode = SnapshotMode::from_env();

    // Create a shared snapshot updater for batch updates
    let updater = Arc::new(SnapshotUpdater::new());

    // Find all markdown test files
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mdtest");
    let mut trials = Vec::new();

    for entry in WalkDir::new(&test_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        let path = entry.path().to_path_buf();
        let source = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to read {}: {}", path.display(), e);
                continue;
            }
        };

        let test_file = match MarkdownTestFile::parse(path.clone(), &source) {
            Ok(tf) => tf,
            Err(e) => {
                eprintln!("Failed to parse {}: {}", path.display(), e);
                continue;
            }
        };

        // Get relative path for display
        let relative_path = path
            .strip_prefix(&test_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        // Create one trial per test (section)
        for test in test_file.tests {
            let test_name = format!("{}::{}", relative_path, test.name);
            let path = path.clone();
            let test = Arc::new(test);
            let updater = Arc::clone(&updater);

            trials.push(Trial::test(test_name, move || {
                run_single_test(&path, &test, snapshot_mode, &updater)
            }));
        }
    }

    let conclusion = libtest_mimic::run(&args, trials);

    // Commit all snapshot updates after tests complete
    if snapshot_mode == SnapshotMode::Update {
        // Try to unwrap the Arc - if there are still references, wait isn't possible
        // In practice, all test closures should be done at this point
        match Arc::try_unwrap(updater) {
            Ok(updater) => match updater.commit() {
                Ok(updated_files) =>
                {
                    #[expect(clippy::print_stderr)]
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
    // Get the Python version from test config, defaulting to "3.12"
    let python_version = test
        .config
        .environment
        .python_version
        .as_deref()
        .unwrap_or("3.12");

    // Create a TestContext for this test - this handles all the proper setup
    let mut context = TestContext::new_with_bin(python_version, get_bin!());

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

    // Create files in the test context's temp directory
    for file in &test.files {
        let file_path = context.temp_dir.join(&file.path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                Failed::from(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
        fs::write(&file_path, &file.content).map_err(|e| {
            Failed::from(format!(
                "Failed to write file {}: {}",
                file_path.display(),
                e
            ))
        })?;
    }

    // Build a command builder that uses TestContext
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
