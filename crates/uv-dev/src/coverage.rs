use std::env;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anstream::println;
use anyhow::{Context, Result, bail};
use clap::Parser;
use serde_json::Value;
use uv_fastid::Id;

use crate::ROOT_DIR;

const CARGO_MESSAGE_REASONS: [&str; 4] = [
    "compiler-artifact",
    "compiler-message",
    "build-script-executed",
    "build-finished",
];

fn parse_coverage_id(value: &str) -> Result<String, String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(format!(
            "invalid ID: {value:?} (must be non-empty and use only [A-Za-z0-9-_])"
        ));
    }
    Ok(value.to_string())
}

#[derive(Parser)]
pub(crate) struct CoverageArgs {
    /// The ID to use for the coverage run.
    #[arg(long, value_parser = parse_coverage_id)]
    id: Option<String>,

    /// Additional arguments to pass to `cargo nextest run`.
    #[arg(last = true)]
    nextest_args: Vec<OsString>,
}

pub(crate) fn coverage(args: CoverageArgs) -> Result<()> {
    let root = fs_err::canonicalize(ROOT_DIR).context("failed to locate the workspace root")?;
    let llvm_tools = find_llvm_tools(&root)?;

    let tracking_id = args.id.unwrap_or_else(|| Id::insecure().to_string());
    let raw_profiles = root
        .join("target")
        .join("coverage")
        .join("profraw")
        .join(&tracking_id);
    let merged_profiles = root.join("target").join("coverage").join("profdata");
    let merged_profile = merged_profiles.join(format!("{tracking_id}.profdata"));
    let lcov_profiles = root.join("target").join("coverage").join("lcov");
    let lcov_profile = lcov_profiles.join(format!("{tracking_id}.lcov"));

    fs_err::create_dir_all(&raw_profiles)
        .with_context(|| format!("failed to create `{}`", raw_profiles.display()))?;

    println!("Coverage tracking ID: {tracking_id}");
    println!("Raw profiles: {}", raw_profiles.display());

    // Bound disk usage while retaining enough merge slots for concurrent test processes.
    let profile_pattern = raw_profiles.join("%16m.profraw");
    let mut child = Command::new("cargo")
        .args([
            "nextest",
            "run",
            "--config",
            ".cargo/coverage.toml",
            "--cargo-message-format",
            "json-render-diagnostics",
        ])
        .args(args.nextest_args)
        .current_dir(&root)
        .env("LLVM_PROFILE_FILE", profile_pattern)
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to run `cargo nextest run`")?;

    let Some(stdout) = child.stdout.take() else {
        bail!("failed to read output from `cargo nextest run`");
    };
    let binaries = collect_coverage_binaries(BufReader::new(stdout))?;
    let status = child
        .wait()
        .context("failed to wait for `cargo nextest run`")?;
    if !status.success() {
        bail!("`cargo nextest run` failed with {status}");
    }

    let profraw_files = collect_profraw_files(&raw_profiles)?;

    if profraw_files.is_empty() {
        bail!(
            "`cargo nextest run` produced no raw coverage profiles in `{}`",
            raw_profiles.display()
        );
    }

    fs_err::create_dir_all(&merged_profiles)
        .with_context(|| format!("failed to create `{}`", merged_profiles.display()))?;

    // Use an input file to avoid exceeding the Windows command-line length limit when there are
    // many raw profiles.
    let mut profraw_file_list = tempfile::NamedTempFile::new_in(&merged_profiles)
        .context("failed to create temporary raw profile list")?;
    for profraw_file in &profraw_files {
        writeln!(profraw_file_list, "{}", profraw_file.display())?;
    }
    profraw_file_list
        .flush()
        .context("failed to flush temporary raw profile list")?;

    let input_files_argument = format!("--input-files={}", profraw_file_list.path().display());
    let status = Command::new(&llvm_tools.profdata)
        .args(["merge", "-sparse"])
        .arg(input_files_argument)
        .arg("-o")
        .arg(&merged_profile)
        .status()
        .with_context(|| format!("failed to run `{}`", llvm_tools.profdata.display()))?;

    if !status.success() {
        bail!("`llvm-profdata merge` failed with {status}");
    }

    println!("Merged profile: {}", merged_profile.display());

    if binaries.is_empty() {
        bail!("`cargo nextest run` produced no executable artifacts for coverage export");
    }

    fs_err::create_dir_all(&lcov_profiles)
        .with_context(|| format!("failed to create `{}`", lcov_profiles.display()))?;
    export_lcov(
        &llvm_tools.cov,
        &merged_profile,
        &binaries,
        &root,
        &lcov_profile,
    )?;

    println!("LCOV profile: {}", lcov_profile.display());
    println!(
        "Generate an HTML report with: uv run --script scripts/coverage-html-report.py {tracking_id}"
    );

    Ok(())
}

fn collect_coverage_binaries(reader: impl BufRead) -> Result<Vec<PathBuf>> {
    let mut binaries = Vec::new();
    let mut stdout = std::io::stdout().lock();

    for line in reader.lines() {
        let line = line.context("failed to read output from `cargo nextest run`")?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            writeln!(stdout, "{line}")?;
            stdout.flush()?;
            continue;
        };
        let Some(reason) = message.get("reason").and_then(Value::as_str) else {
            writeln!(stdout, "{line}")?;
            stdout.flush()?;
            continue;
        };

        if reason == "compiler-artifact"
            && let Some(executable) = message.get("executable").and_then(Value::as_str)
        {
            let is_test = message
                .pointer("/profile/test")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let is_binary = message
                .pointer("/target/kind")
                .and_then(Value::as_array)
                .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some("bin")));
            if is_test || is_binary {
                binaries.push(PathBuf::from(executable));
            }
        }

        if !CARGO_MESSAGE_REASONS.contains(&reason) {
            writeln!(stdout, "{line}")?;
            stdout.flush()?;
        }
    }

    binaries.sort_unstable();
    binaries.dedup();
    Ok(binaries)
}

fn collect_profraw_files(raw_profiles: &Path) -> Result<Vec<PathBuf>> {
    let mut profraw_files = Vec::new();
    for entry in fs_err::read_dir(raw_profiles)
        .with_context(|| format!("failed to read `{}`", raw_profiles.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "profraw")
        {
            profraw_files.push(path);
        }
    }
    profraw_files.sort_unstable();
    Ok(profraw_files)
}

fn export_lcov(
    llvm_cov: &Path,
    merged_profile: &Path,
    binaries: &[PathBuf],
    root: &Path,
    lcov_profile: &Path,
) -> Result<()> {
    let Some((binary, additional_binaries)) = binaries.split_first() else {
        bail!("no executable artifacts were provided for coverage export");
    };

    let mut command = Command::new(llvm_cov);
    command
        .arg("export")
        .arg(binary)
        .arg(format!("--instr-profile={}", merged_profile.display()))
        .args([
            r"--ignore-filename-regex=[/\\]\.cargo[/\\]",
            r"--ignore-filename-regex=[/\\]rustc[/\\]",
            r"--ignore-filename-regex=[/\\]\.rustup[/\\]toolchains[/\\]",
            r"--ignore-filename-regex=[/\\]target[/\\]",
            "--format=lcov",
        ])
        .stdout(Stdio::piped());
    for binary in additional_binaries {
        command.arg("-object").arg(binary);
    }

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to run `{}`", llvm_cov.display()))?;
    let Some(stdout) = child.stdout.take() else {
        bail!("failed to read output from `llvm-cov export`");
    };

    let mut temp_file = tempfile::NamedTempFile::new_in(
        lcov_profile
            .parent()
            .context("LCOV output path must have a parent")?,
    )
    .context("failed to create temporary LCOV output")?;
    for line in BufReader::new(stdout).lines() {
        let line = line.context("failed to read output from `llvm-cov export`")?;
        if let Some(source) = line.strip_prefix("SF:") {
            let source = Path::new(source);
            if let Ok(relative) = source.strip_prefix(root) {
                writeln!(
                    temp_file,
                    "SF:{}",
                    relative.to_string_lossy().replace('\\', "/")
                )?;
            } else {
                writeln!(
                    temp_file,
                    "SF:{}",
                    source.to_string_lossy().replace('\\', "/")
                )?;
            }
        } else {
            writeln!(temp_file, "{line}")?;
        }
    }

    let status = child
        .wait()
        .context("failed to wait for `llvm-cov export`")?;
    if !status.success() {
        bail!("`llvm-cov export` failed with {status}");
    }

    temp_file.flush().context("failed to flush LCOV output")?;
    temp_file
        .persist(lcov_profile)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to write `{}`", lcov_profile.display()))?;

    Ok(())
}

struct LlvmTools {
    profdata: PathBuf,
    cov: PathBuf,
}

fn find_llvm_tools(root: &Path) -> Result<LlvmTools> {
    let output = Command::new("rustc")
        .args(["--print", "target-libdir"])
        .current_dir(root)
        .output()
        .context("failed to run `rustc --print=target-libdir`")?;

    if !output.status.success() {
        bail!(
            "`rustc --print=target-libdir` failed with {}",
            output.status
        );
    }

    let target_libdir = String::from_utf8(output.stdout)
        .context("`rustc --print=target-libdir` returned a non-UTF-8 path")?;
    let target_libdir = Path::new(target_libdir.trim());
    let Some(target_dir) = target_libdir.parent() else {
        bail!(
            "`rustc --print=target-libdir` returned an invalid path: `{}`",
            target_libdir.display()
        );
    };

    let tool_dir = target_dir.join("bin");
    let profdata = tool_dir.join(format!("llvm-profdata{}", env::consts::EXE_SUFFIX));
    let cov = tool_dir.join(format!("llvm-cov{}", env::consts::EXE_SUFFIX));
    for tool in [&profdata, &cov] {
        if !tool.is_file() {
            bail!(
                "`{}` was not found at `{}`; install it with `rustup component add llvm-tools`",
                tool.file_name().unwrap_or_default().to_string_lossy(),
                tool.display()
            );
        }
    }

    Ok(LlvmTools { profdata, cov })
}
