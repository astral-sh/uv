use std::borrow::Cow;
use std::fmt::Write;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use owo_colors::OwoColorize;
use thiserror::Error;
use uv_cache::Cache;
use uv_fs::{CWD, Simplified, ValidatedReader, is_virtualenv_base, normalize_path};
use uv_preview::{Preview, PreviewFeature};
use uv_scripts::{Pep723Error, Pep723Metadata};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// List workspace members or PEP 723 scripts.
pub(crate) async fn list(
    project_dir: &Path,
    paths: bool,
    scripts: bool,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    if scripts && !preview.is_enabled(PreviewFeature::WorkspaceListScripts) {
        warn_user!(
            "The `--scripts` option is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::WorkspaceListScripts
        );
    }

    let workspace = Workspace::discover(
        project_dir,
        &DiscoveryOptions::default(),
        cache,
        workspace_cache,
    )
    .await?;

    if scripts {
        let mut scripts = find_scripts(workspace.install_path(), cache)
            .filter_map(|script| match script {
                Ok(script) => Some(Ok(script)),
                Err(ScriptDiscoveryError::Parse { path, source }) => {
                    warn_user!(
                        "Skipping invalid PEP 723 script `{}`: {source}",
                        path.simplified_display()
                    );
                    None
                }
                Err(
                    error @ (ScriptDiscoveryError::Walk(_) | ScriptDiscoveryError::Read { .. }),
                ) => Some(Err(error)),
            })
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| {
                format!(
                    "Failed to discover PEP 723 scripts under workspace root `{}`",
                    workspace.install_path().simplified_display()
                )
            })?;
        scripts.sort_unstable();
        for script in scripts {
            let script = script
                .strip_prefix(workspace.install_path())
                .context("PEP 723 script was discovered outside the workspace root")?;
            writeln!(printer.stdout(), "{}", script.simplified_display().cyan())?;
        }
        return Ok(ExitStatus::Success);
    }

    for (name, member) in workspace.packages() {
        if paths {
            writeln!(
                printer.stdout(),
                "{}",
                member.root().simplified_display().cyan()
            )?;
        } else {
            writeln!(printer.stdout(), "{}", name.cyan())?;
        }
    }

    Ok(ExitStatus::Success)
}

/// A failure encountered while discovering PEP 723 scripts.
#[derive(Debug, Error)]
pub(crate) enum ScriptDiscoveryError {
    /// The workspace could not be traversed.
    #[error("Failed to walk workspace while discovering PEP 723 scripts")]
    Walk(#[source] ignore::Error),
    /// A candidate script could not be read.
    #[error("Failed to read candidate PEP 723 script: {}", path.simplified_display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// A candidate script contains invalid PEP 723 metadata.
    #[error("Failed to parse PEP 723 script: {}", path.simplified_display())]
    Parse {
        path: PathBuf,
        #[source]
        source: Pep723Error,
    },
}

/// Find PEP 723 scripts under a workspace root.
///
/// Respects ignore files and excludes repository internals, virtual environments, and the uv cache
/// from traversal. Script-specific errors are returned individually so callers can decide whether
/// invalid candidates should fail discovery.
pub(crate) fn find_scripts(
    workspace_root: &Path,
    cache: &Cache,
) -> impl Iterator<Item = Result<PathBuf, ScriptDiscoveryError>> {
    // Avoid descending into the cache when it is inside the workspace. If the workspace itself is
    // inside the cache, it is still the requested search root and must not be excluded.
    let cache_root = if cache.root().is_absolute() {
        Cow::Borrowed(cache.root())
    } else {
        Cow::Owned(CWD.join(cache.root()))
    };
    let cache_root = normalize_path(cache_root);
    // The filter closure requires owned data, but only capture the cache root when it is strictly
    // inside the workspace. This avoids allocation and per-entry comparisons for external caches.
    let cache_is_nested =
        cache_root.as_ref() != workspace_root && cache_root.starts_with(workspace_root);
    let cache_root = cache_is_nested.then(|| cache_root.into_owned());

    let mut builder = ignore::WalkBuilder::new(workspace_root);
    // Include scripts in hidden directories, such as `.github`.
    ignore::WalkBuilder::hidden(&mut builder, false);
    builder
        // Respect `.gitignore` files in source archives and other workspaces without `.git`.
        .require_git(false)
        .filter_entry(move |entry| {
            let path = entry.path();
            if cache_root
                .as_ref()
                .is_some_and(|cache_root| path.starts_with(cache_root))
            {
                return false;
            }

            if !entry
                .file_type()
                .is_some_and(|file_type| file_type.is_dir())
            {
                return true;
            }

            // Hidden directories are included above, but Git internals cannot contain workspace
            // scripts and can be very large.
            if entry.file_name() == ".git" {
                return false;
            }

            // Ignore rules have already been applied, but `.venv` is not guaranteed to be ignored.
            if entry.file_name() == ".venv" {
                return false;
            }

            // Detect virtual environments by their marker file so custom directory names are
            // handled too.
            !is_virtualenv_base(path)
        });
    builder.build().filter_map(|entry| {
        let entry = match entry {
            Ok(entry) => entry,
            Err(source) => return Some(Err(ScriptDiscoveryError::Walk(source))),
        };
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
            || !is_python_script_path(entry.path())
        {
            return None;
        }

        let contents = match read_script_candidate(entry.path()) {
            Ok(Some(contents)) => contents,
            Ok(None) => return None,
            Err(source) => {
                return Some(Err(ScriptDiscoveryError::Read {
                    path: entry.into_path(),
                    source,
                }));
            }
        };
        match Pep723Metadata::parse(&contents) {
            Ok(Some(_)) => Some(Ok(entry.into_path())),
            Ok(None) => None,
            Err(source) => Some(Err(ScriptDiscoveryError::Parse {
                path: entry.into_path(),
                source,
            })),
        }
    })
}

/// Read a candidate script.
///
/// Extensionless candidates are only read past their prefix when they begin with a shebang.
fn read_script_candidate(path: &Path) -> io::Result<Option<Vec<u8>>> {
    if path.extension().is_some() {
        return fs_err::read(path).map(Some);
    }

    ValidatedReader::new(fs_err::File::open(path)?)
        .require_prefix("#!")
        .require_utf8()
        .read()
}

/// Return whether a path could contain a Python script.
///
/// PEP 723 does not require a specific filename, and uv can run explicitly requested scripts with
/// arbitrary extensions or no extension. For discovery, restrict the search to Python extensions
/// and extensionless files to avoid treating metadata examples embedded in documentation as scripts.
/// Extensionless candidates are further restricted to shebang scripts and checked for binary
/// content as they are read.
fn is_python_script_path(path: &Path) -> bool {
    path.extension().is_none_or(|extension| {
        extension.eq_ignore_ascii_case("py") || extension.eq_ignore_ascii_case("pyw")
    })
}
