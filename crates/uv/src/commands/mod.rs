use std::borrow::Cow;
use std::io::stdout;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fmt::Write, process::ExitCode};

use anstream::AutoStream;
use anyhow::Context;
pub(crate) use auth::dir::dir as auth_dir;
pub(crate) use auth::helper::helper as auth_helper;
pub(crate) use auth::login::login as auth_login;
pub(crate) use auth::logout::logout as auth_logout;
pub(crate) use auth::token::token as auth_token;
pub(crate) use build_frontend::build_frontend;
pub(crate) use cache_clean::cache_clean;
pub(crate) use cache_dir::cache_dir;
pub(crate) use cache_prune::cache_prune;
pub(crate) use cache_size::cache_size;
pub(crate) use help::help;
use owo_colors::OwoColorize;
pub(crate) use pip::check::pip_check;
pub(crate) use pip::compile::pip_compile;
pub(crate) use pip::freeze::pip_freeze;
pub(crate) use pip::install::pip_install;
pub(crate) use pip::list::pip_list;
pub(crate) use pip::show::pip_show;
pub(crate) use pip::sync::pip_sync;
pub(crate) use pip::tree::pip_tree;
pub(crate) use pip::uninstall::pip_uninstall;
pub(crate) use project::add::add;
pub(crate) use project::audit::audit;
pub(crate) use project::export::export;
pub(crate) use project::format::format;
pub(crate) use project::init::{InitKind, InitProjectKind, init};
pub(crate) use project::lock::lock;
pub(crate) use project::remove::remove;
pub(crate) use project::run::{RunCommand, run};
pub(crate) use project::sync::sync;
pub(crate) use project::tree::tree;
pub(crate) use project::version::{project_version, self_version};
pub(crate) use publish::publish;
pub(crate) use python::dir::dir as python_dir;
pub(crate) use python::find::find as python_find;
pub(crate) use python::find::find_script as python_find_script;
pub(crate) use python::install::install as python_install;
pub(crate) use python::install::{PythonUpgrade, PythonUpgradeSource};
pub(crate) use python::list::list as python_list;
pub(crate) use python::pin::pin as python_pin;
pub(crate) use python::uninstall::uninstall as python_uninstall;
pub(crate) use python::update_shell::update_shell as python_update_shell;
#[cfg(feature = "self-update")]
pub(crate) use self_update::self_update;
pub(crate) use tool::dir::dir as tool_dir;
pub(crate) use tool::install::install as tool_install;
pub(crate) use tool::list::list as tool_list;
pub(crate) use tool::run::ToolRunCommand;
pub(crate) use tool::run::run as tool_run;
pub(crate) use tool::uninstall::uninstall as tool_uninstall;
pub(crate) use tool::update_shell::update_shell as tool_update_shell;
pub(crate) use tool::upgrade::upgrade as tool_upgrade;
use tracing::debug;
use uv_cache::{Cache, CacheBucket};
use uv_configuration::Concurrency;
pub(crate) use uv_console::human_readable_bytes;
use uv_distribution_types::CachedDist;
use uv_fs::{CWD, Simplified};
use uv_install_wheel::{find_dist_info, read_record_file};
use uv_installer::{compile_files, compile_tree};
use uv_python::PythonEnvironment;
use uv_scripts::Pep723Script;
pub(crate) use venv::venv;
pub(crate) use workspace::dir::dir;
pub(crate) use workspace::list::list;
pub(crate) use workspace::metadata::metadata;

use crate::commands::pip::operations::ChangedDist;
use crate::printer::Printer;

mod auth;
pub(crate) mod build_backend;
mod build_frontend;
mod cache_clean;
mod cache_dir;
mod cache_prune;
mod cache_size;
mod diagnostics;
mod help;
pub(crate) mod pip;
mod project;
mod publish;
mod python;
pub(crate) mod reporters;
#[cfg(feature = "self-update")]
mod self_update;
mod tool;
mod venv;
mod workspace;

#[derive(Copy, Clone)]
pub(crate) enum ExitStatus {
    /// The command succeeded.
    Success,

    /// The command failed due to an error in the user input.
    Failure,

    /// The command failed with an unexpected error.
    Error,

    /// The command's exit status is propagated from an external command.
    External(u8),
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        match status {
            ExitStatus::Success => Self::from(0),
            ExitStatus::Failure => Self::from(1),
            ExitStatus::Error => Self::from(2),
            ExitStatus::External(code) => Self::from(code),
        }
    }
}

/// Format a duration as a human-readable string, Cargo-style.
pub(super) fn elapsed(duration: Duration) -> String {
    let secs = duration.as_secs();
    let ms = duration.subsec_millis();

    if secs >= 60 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else if secs > 0 {
        format!("{}.{:02}s", secs, duration.subsec_nanos() / 10_000_000)
    } else if ms > 0 {
        format!("{ms}ms")
    } else {
        format!("0.{:02}ms", duration.subsec_nanos() / 10_000)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum ChangeEventKind {
    /// The package was removed from the environment.
    Removed,
    /// The package was added to the environment.
    Added,
    /// The package was reinstalled without changing versions.
    Reinstalled,
}

#[derive(Debug)]
pub(super) struct ChangeEvent<'a> {
    dist: &'a ChangedDist,
    kind: ChangeEventKind,
}

/// Compile all Python source files in site-packages to bytecode.
///
/// This is used as a fallback when no install information is available, e.g., when
/// `--compile` is passed but nothing new was installed.
pub(super) async fn compile_bytecode(
    venv: &PythonEnvironment,
    concurrency: &Concurrency,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let mut files = 0;
    for site_packages in venv.site_packages() {
        let site_packages = CWD.join(site_packages);
        if !site_packages.exists() {
            debug!(
                "Skipping non-existent site-packages directory: {}",
                site_packages.display()
            );
            continue;
        }
        files += compile_tree(
            &site_packages,
            venv.python_executable(),
            concurrency,
            cache.root(),
        )
        .await
        .with_context(|| {
            format!(
                "Failed to bytecode-compile Python file in: {}",
                site_packages.user_display()
            )
        })?;
    }
    let s = if files == 1 { "" } else { "s" };
    writeln!(
        printer.stderr(),
        "{}",
        format!(
            "Bytecode compiled {} {}",
            format!("{files} file{s}").bold(),
            format!("in {}", elapsed(start.elapsed())).dimmed()
        )
        .dimmed()
    )?;
    Ok(())
}

/// Return the Python implementation tag for bytecode cache keys (e.g., `cpython312`).
fn python_tag(venv: &PythonEnvironment) -> String {
    let interpreter = venv.interpreter();
    let impl_name = interpreter.implementation_name().to_lowercase();
    let major = interpreter.python_major();
    let minor = interpreter.python_minor();
    format!("{impl_name}{major}{minor}")
}

/// Return the bytecode cache directory for a specific package and Python version.
///
/// Layout: `bytecode-v0/{python_tag}/{invalidation_mode}/{package_name}-{version}-{wheel_id}/`
///
/// The cache key includes:
/// - `python_tag`: Python implementation and version (e.g., `cpython312`)
/// - `invalidation_mode`: The `PYC_INVALIDATION_MODE` used during compilation, so
///   switching modes doesn't serve stale `.pyc` files with the wrong format
/// - `wheel_id`: Derived from the wheel's cache path to distinguish different
///   wheels that share a name and version (path installs, git installs, etc.)
fn bytecode_cache_dir(
    cache: &Cache,
    tag: &str,
    invalidation_mode: &str,
    dist: &CachedDist,
) -> PathBuf {
    let wheel_id = dist
        .path()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    cache
        .bucket(CacheBucket::Bytecode)
        .join(tag)
        .join(invalidation_mode)
        .join(format!(
            "{}-{}-{}",
            dist.filename().name,
            dist.filename().version,
            wheel_id
        ))
}

/// Marker file written after a cache entry is fully populated.
const CACHE_COMPLETE_MARKER: &str = ".complete";

/// Try to restore cached `.pyc` files for a distribution into `site_packages`.
///
/// Returns the number of files restored, or `0` if no cache entry exists.
/// Only restores from cache entries that have a `.complete` marker, to avoid
/// serving partially written caches from interrupted saves.
fn restore_cached_bytecode(cache_dir: &Path, site_packages: &Path) -> anyhow::Result<usize> {
    if !cache_dir.join(CACHE_COMPLETE_MARKER).is_file() {
        // No complete cache entry — either doesn't exist or was partially written.
        // Clean up any partial cache to allow a fresh save.
        if cache_dir.is_dir() {
            let _ = fs_err::remove_dir_all(cache_dir);
        }
        return Ok(0);
    }

    let mut count = 0usize;
    for entry in walkdir::WalkDir::new(cache_dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(cache_dir) else {
            continue;
        };
        // Skip the marker file itself.
        if relative.as_os_str() == CACHE_COMPLETE_MARKER {
            continue;
        }
        let target = site_packages.join(relative);
        if let Some(parent) = target.parent() {
            fs_err::create_dir_all(parent)?;
        }
        // Copy the .pyc file to site-packages. Use copy (not hardlink) since
        // the cache is a long-lived directory and we don't want venv cleanup to
        // affect it.
        fs_err::copy(entry.path(), &target)?;
        count += 1;
    }
    Ok(count)
}

/// Save compiled `.pyc` files from `site_packages` into the bytecode cache.
///
/// Only saves `.pyc` files that correspond to `.py` files listed in `py_files`.
fn save_bytecode_to_cache(
    cache_dir: &Path,
    site_packages: &Path,
    py_files: &[PathBuf],
) -> anyhow::Result<()> {
    fs_err::create_dir_all(cache_dir)?;

    for py_file in py_files {
        let Ok(relative) = py_file.strip_prefix(site_packages) else {
            continue;
        };

        // Find __pycache__/*.pyc files for this source file.
        let Some(parent) = py_file.parent() else {
            continue;
        };
        let pycache_dir = parent.join("__pycache__");
        if !pycache_dir.is_dir() {
            continue;
        }

        let Some(stem) = relative.file_stem() else {
            continue;
        };
        let stem = stem.to_string_lossy();

        // Match files like `__init__.cpython-312.pyc` for source `__init__.py`.
        if let Ok(entries) = fs_err::read_dir(&pycache_dir) {
            for entry in entries {
                let entry = entry?;
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with(&*stem) && name_str.ends_with(".pyc") {
                    if let Ok(relative_pycache) = pycache_dir.strip_prefix(site_packages) {
                        let cache_target = cache_dir.join(relative_pycache).join(&name);
                        if let Some(parent) = cache_target.parent() {
                            fs_err::create_dir_all(parent)?;
                        }
                        fs_err::copy(entry.path(), &cache_target)?;
                    }
                }
            }
        }
    }

    // Write completion marker so restore_cached_bytecode knows this is a
    // complete cache entry, not a partially written one.
    fs_err::write(cache_dir.join(CACHE_COMPLETE_MARKER), "")?;

    Ok(())
}

/// Compile bytecode only for the given installed distributions.
///
/// Reads each distribution's `RECORD` file to find installed `.py` files, then compiles only
/// those files. This avoids recompiling the entire `site-packages` directory when only a few
/// packages were installed or updated.
///
/// When bytecode caching is available, previously compiled `.pyc` files are restored from the
/// cache instead of re-running the Python compiler. Newly compiled bytecode is saved to the
/// cache for future use.
pub(super) async fn compile_bytecode_for_installs(
    venv: &PythonEnvironment,
    installs: &[CachedDist],
    concurrency: &Concurrency,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<()> {
    if installs.is_empty() {
        return Ok(());
    }

    let start = std::time::Instant::now();
    let tag = python_tag(venv);
    let invalidation_mode =
        std::env::var("PYC_INVALIDATION_MODE").unwrap_or_else(|_| "CHECKED_HASH".to_string());
    let mut total_files = 0usize;
    let mut cached_files = 0usize;

    // Collect all site-packages roots (purelib + platlib). On most systems these
    // are the same, but on some (e.g., Debian) they differ. RECORD entries for
    // files in the other root use relative `../` paths, so we need to check
    // resolved paths against all roots.
    let all_site_packages: Vec<PathBuf> = venv
        .site_packages()
        .map(|sp| CWD.join(sp))
        .filter(|sp| sp.exists())
        .collect();

    for site_packages in &all_site_packages {
        // Separate distributions into cache hits and misses.
        let mut files_to_compile: Vec<PathBuf> = Vec::new();
        let mut dists_to_cache: Vec<(&CachedDist, Vec<PathBuf>)> = Vec::new();

        for dist in installs {
            let cache_dir = bytecode_cache_dir(cache, &tag, &invalidation_mode, dist);

            // Try to restore cached bytecode.
            let restored = restore_cached_bytecode(&cache_dir, site_packages)?;
            if restored > 0 {
                cached_files += restored;
                total_files += restored;
                debug!(
                    "Restored {} cached bytecode files for {}-{}",
                    restored,
                    dist.filename().name,
                    dist.filename().version
                );
                continue;
            }

            // Cache miss — collect .py files for compilation.
            let dist_info_prefix = match find_dist_info(dist.path()) {
                Ok(prefix) => prefix,
                Err(_) => {
                    format!(
                        "{}-{}",
                        dist.filename().name.as_dist_info_name(),
                        dist.filename().version,
                    )
                }
            };
            let record_path = site_packages
                .join(format!("{dist_info_prefix}.dist-info"))
                .join("RECORD");

            let record = match fs_err::File::open(&record_path) {
                Ok(mut file) => read_record_file(&mut file)?,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => return Err(err.into()),
            };

            let mut dist_py_files = Vec::new();
            for entry in &record {
                if Path::new(&entry.path)
                    .extension()
                    .is_some_and(|ext| ext == "py")
                {
                    // Normalize the path to resolve `..` segments (e.g., RECORD
                    // entries like `../../platlib/pkg/mod.py` for cross-root files).
                    let full_path = uv_fs::normalize_path_buf(site_packages.join(&entry.path));
                    // Only compile files within a site-packages root. This prevents
                    // compiling files outside the package tree (e.g., scripts in bin/)
                    // which would leave orphaned .pyc files after uninstall.
                    if all_site_packages.iter().any(|sp| full_path.starts_with(sp))
                        && full_path.exists()
                    {
                        dist_py_files.push(full_path);
                    }
                }
            }

            if !dist_py_files.is_empty() {
                files_to_compile.extend(dist_py_files.iter().cloned());
                dists_to_cache.push((dist, dist_py_files));
            }
        }

        // Compile cache-miss files.
        if !files_to_compile.is_empty() {
            let compiled = compile_files(
                files_to_compile,
                venv.python_executable(),
                concurrency,
                cache.root(),
                true, // Use hash-based validation for cached bytecode
            )
            .await
            .context("Failed to bytecode-compile installed packages")?;
            total_files += compiled;

            // Save newly compiled .pyc files to cache.
            for (dist, py_files) in &dists_to_cache {
                let cache_dir = bytecode_cache_dir(cache, &tag, &invalidation_mode, dist);
                if let Err(err) = save_bytecode_to_cache(&cache_dir, site_packages, py_files) {
                    debug!(
                        "Failed to cache bytecode for {}-{}: {err}",
                        dist.filename().name,
                        dist.filename().version
                    );
                }
            }
        }
    }

    if total_files > 0 {
        let s = if total_files == 1 { "" } else { "s" };
        let cache_note = if cached_files > 0 {
            format!(" ({cached_files} cached)")
        } else {
            String::new()
        };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Bytecode compiled {} {}{cache_note}",
                format!("{total_files} file{s}").bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
            )
            .dimmed()
        )?;
    }
    Ok(())
}

/// A multicasting writer that writes to both the standard output and an output file, if present.
struct OutputWriter<'a> {
    stdout: Option<AutoStream<std::io::Stdout>>,
    output_file: Option<&'a Path>,
    buffer: Vec<u8>,
}

impl<'a> OutputWriter<'a> {
    /// Create a new output writer.
    fn new(include_stdout: bool, output_file: Option<&'a Path>) -> Self {
        let stdout = include_stdout.then(|| AutoStream::<std::io::Stdout>::auto(stdout()));
        Self {
            stdout,
            output_file,
            buffer: Vec::new(),
        }
    }

    /// Commit the buffer to the output file.
    async fn commit(self) -> std::io::Result<()> {
        if let Some(output_file) = self.output_file {
            if let Some(parent_dir) = output_file.parent() {
                fs_err::create_dir_all(parent_dir)?;
            }

            // If the output file is an existing symlink, write to the destination instead.
            let output_file = fs_err::read_link(output_file)
                .map(Cow::Owned)
                .unwrap_or(Cow::Borrowed(output_file));
            let stream = anstream::adapter::strip_bytes(&self.buffer).into_vec();
            uv_fs::write_atomic(output_file, &stream).await?;
        }
        Ok(())
    }
}

impl std::io::Write for OutputWriter<'_> {
    /// Write to both standard output and the output buffer, if present.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // Write to the buffer.
        if self.output_file.is_some() {
            self.buffer.write_all(buf)?;
        }

        // Write to standard output.
        if let Some(stdout) = &mut self.stdout {
            stdout.write_all(buf)?;
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(stdout) = &mut self.stdout {
            stdout.flush()?;
        }
        Ok(())
    }
}

/// Given a list of names, return a conjunction of the names (e.g., "Alice, Bob, and Charlie").
pub(super) fn conjunction(names: Vec<String>) -> String {
    let mut names = names.into_iter();
    let first = names.next();
    let last = names.next_back();
    match (first, last) {
        (Some(first), Some(last)) => {
            let mut result = first;
            let mut comma = false;
            for name in names {
                result.push_str(", ");
                result.push_str(&name);
                comma = true;
            }
            if comma {
                result.push_str(", and ");
            } else {
                result.push_str(" and ");
            }
            result.push_str(&last);
            result
        }
        (Some(first), None) => first,
        _ => String::new(),
    }
}

/// Capitalize the first letter of a string.
pub(super) fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// A Python file that may or may not include an existing PEP 723 script tag.
#[derive(Debug)]
#[expect(clippy::large_enum_variant)]
pub(crate) enum ScriptPath {
    /// The Python file already includes a PEP 723 script tag.
    Script(Pep723Script),
    /// The Python file does not include a PEP 723 script tag.
    Path(PathBuf),
}
