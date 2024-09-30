#![allow(clippy::print_stderr)]

use crate::common::{run_and_format, TestContext};
use anyhow::Context;
use rustc_hash::FxHashMap;
use serde::Deserialize;
use std::env;
use std::path::Path;
use std::process::Command;
use toml_edit::DocumentMut;
use uv_fs::Simplified;

#[derive(Debug, Deserialize)]
pub struct DeclarativeTest {
    /// Default: 3.12
    python_version: Option<String>,
    step: Vec<DeclarativeStep>,
}

impl DeclarativeTest {
    /// Run a declarative test scenario.
    pub fn run(scenario_name: &str) {
        let update = env::var("INSTA_UPDATE").as_deref() == Ok("always");
        let scenario_path = Path::new("scenarios").join(scenario_name);
        run_scenario(scenario_name, update, &scenario_path);
    }
}

/// A toml serialized test scenario.
///
/// A toml test has steps, which can have the following component:
/// * Inputs, a filename to content mapping.
/// * Command, a command to which the default options are attached, and an output field that
///   snapshots that command's output.
/// * Outputs, a filename to content mapping of snapshots.
///
/// By defaults, the test fails if the snapshots mismatch. Set the `INSTA_UPDATE` environment
/// variable to `always` (e.g., `INSTA_UPDATE=always cargo nextest run`) to update the snapshots.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum DeclarativeCommand {
    PipCompile,
    PipSync,
    PipShow,
    PipFreeze,
    PipCheck,
    PipList,
    Venv,
    PipInstall,
    PipUninstall,
    PipTree,
    Help,
    Init,
    Sync,
    Lock,
    Export,
    Build,
    Publish,
    PythonFind,
    PythonPin,
    PythonDir,
    Run,
    ToolRun,
    ToolUpgrade,
    ToolInstall,
    ToolList,
    ToolDir,
    ToolUninstall,
    Add,
    Remove,
    Tree,
    Clean,
    Prune,
}

impl DeclarativeCommand {
    fn to_command(&self, context: &TestContext) -> Command {
        match self {
            Self::PipCompile => context.pip_compile(),
            Self::PipSync => context.pip_sync(),
            Self::PipShow => context.pip_show(),
            Self::PipFreeze => context.pip_freeze(),
            Self::PipCheck => context.pip_check(),
            Self::PipList => context.pip_list(),
            Self::Venv => context.venv(),
            Self::PipInstall => context.pip_install(),
            Self::PipUninstall => context.pip_uninstall(),
            Self::PipTree => context.pip_tree(),
            Self::Help => context.help(),
            Self::Init => context.init(),
            Self::Sync => context.sync(),
            Self::Lock => context.lock(),
            Self::Export => context.export(),
            Self::Build => context.build(),
            Self::Publish => context.publish(),
            Self::PythonFind => context.python_find(),
            Self::PythonPin => context.python_pin(),
            Self::PythonDir => context.python_dir(),
            Self::Run => context.run(),
            Self::ToolRun => context.tool_run(),
            Self::ToolUpgrade => context.tool_upgrade(),
            Self::ToolInstall => context.tool_install(),
            Self::ToolList => context.tool_list(),
            Self::ToolDir => context.tool_dir(),
            Self::ToolUninstall => context.tool_uninstall(),
            Self::Add => context.add(),
            Self::Remove => context.remove(),
            Self::Tree => context.tree(),
            Self::Clean => context.clean(),
            Self::Prune => context.prune(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclarativeStep {
    #[serde(default)]
    input: FxHashMap<String, String>,
    command: DeclarativeCommand,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env_remove: Vec<String>,
    output: Option<String>,
    #[serde(default)]
    snapshot: FxHashMap<String, String>,
}

fn run_scenario(scenario_name: &str, update: bool, scenario_path: &Path) {
    let snapshot_serialized = fs_err::read_to_string(scenario_path).unwrap();
    // Structured, for the main routine
    let scenario: DeclarativeTest = toml::from_str(&snapshot_serialized).unwrap();
    // Formatted, for snapshot updates
    let mut doc = snapshot_serialized.parse::<DocumentMut>().unwrap();

    let context = TestContext::new(scenario.python_version.as_deref().unwrap_or("3.12"));

    for (step_id, step) in scenario.step.iter().enumerate() {
        // Write the input files
        for (path, content) in &step.input {
            fs_err::write(context.temp_dir.join(path), content).unwrap();
        }

        // Run the command
        let mut command = step.command.to_command(&context);
        for arg in &step.args {
            command.arg(arg);
        }
        for env_remove in &step.env_remove {
            command.env_remove(env_remove);
        }
        let (actual, _) = run_and_format(
            &mut command,
            context.filters(),
            &format!("Step {}: {:?}", step_id + 1, step.command),
            None,
        );
        if let Some(expected) = &step.output {
            if update {
                if actual.trim() != expected.trim() {
                    eprintln!(
                        "Updating snapshot for {scenario_name} step {} output",
                        step_id + 1
                    );
                    doc["step"][step_id]["output"] = toml_edit::value(actual);
                    fs_err::write(scenario_path, doc.to_string()).unwrap();
                }
            } else {
                assert_eq!(
                    actual.trim(),
                    expected.trim(),
                    "Output mismatch for step {}: {:?}",
                    step_id + 1,
                    step.command
                );
            }
        }

        // Check the output file
        for (path, expected) in &step.snapshot {
            let actual = fs_err::read_to_string(context.temp_dir.join(path))
                .with_context(|| format!("Missing expected output file: `{}`", path.user_display()))
                .unwrap();
            if update {
                if actual.trim() != expected.trim() {
                    eprintln!(
                        "Updating snapshot for {scenario_name} step {} snapshot {path}",
                        step_id + 1
                    );
                    doc["step"][step_id]["snapshot"][path] = toml_edit::value(actual);
                    fs_err::write(scenario_path, doc.to_string()).unwrap();
                }
            } else {
                assert_eq!(
                    actual.trim(),
                    expected.trim(),
                    "Snapshot mismatch for step {}: {:?}",
                    step_id + 1,
                    step.command
                );
            }
        }
    }
}
