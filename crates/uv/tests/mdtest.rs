//! Markdown-based integration tests for uv.
//!
//! This test runner uses the `uv-mdtest` crate to run tests defined in markdown files.
//! Tests are located in `crates/uv/tests/mdtest/`.

use std::path::Path;
use std::process::Command;

use fs_err as fs;
use regex::Regex;

use uv_mdtest::{SnapshotMode, format_mismatch, parse, run_test_with_command_builder};

// Import the common module from the main integration tests
#[path = "it/common/mod.rs"]
mod common;

use common::TestContext;

/// Run a single markdown test file.
fn run_mdtest(path: &Path) -> datatest_stable::Result<()> {
    let source = fs::read_to_string(path)?;

    let test_file = parse(path.to_path_buf(), &source)
        .map_err(|e| format!("Failed to parse test file {}: {}", path.display(), e))?;

    let snapshot_mode = SnapshotMode::from_env();

    for test in &test_file.tests {
        // Get the Python version from test config, defaulting to "3.12"
        let python_version = test
            .config
            .environment
            .python_version
            .as_deref()
            .unwrap_or("3.12");

        // Create a TestContext for this test - this handles all the proper setup
        let mut context = TestContext::new(python_version);

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
                    format!("Failed to create directory {}: {}", parent.display(), e)
                })?;
            }
            fs::write(&file_path, &file.content)
                .map_err(|e| format!("Failed to write file {}: {}", file_path.display(), e))?;
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
        let result =
            run_test_with_command_builder(test, context.temp_dir.path(), &filters, command_builder)
                .map_err(|e| {
                    format!(
                        "Test '{}' execution failed at {}:{}: {}",
                        test.name,
                        path.display(),
                        test.line_number,
                        e
                    )
                })?;

        if !result.passed {
            if let Some(mismatch) = &result.mismatch {
                if snapshot_mode == SnapshotMode::Update {
                    // Update the snapshot in the source file
                    uv_mdtest::update_snapshot(path, mismatch)
                        .map_err(|e| format!("Failed to update snapshot: {e}"))?;
                    // Print a message to indicate the snapshot was updated
                    #[expect(clippy::print_stderr)]
                    {
                        eprintln!("Updated snapshot for test: {}", test.name);
                    }
                } else {
                    // Print the mismatch and fail
                    let message = format_mismatch(mismatch);
                    return Err(format!(
                        "Test '{}' failed at {}:{}\n\n{}",
                        test.name,
                        path.display(),
                        test.line_number,
                        message
                    )
                    .into());
                }
            }
        }
    }

    Ok(())
}

// Use datatest-stable to discover and run mdtest files
datatest_stable::harness! {
    { test = run_mdtest, root = "tests/mdtest", pattern = r".*\.md$" },
}
