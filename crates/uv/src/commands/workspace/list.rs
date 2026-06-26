use std::borrow::Cow;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use owo_colors::OwoColorize;
use uv_cache::Cache;
use uv_fs::{CWD, Simplified, is_virtualenv_base, normalize_path};
use uv_preview::{Preview, PreviewFeature};
use uv_scripts::Pep723Metadata;
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
        for script in find_scripts(workspace.install_path(), cache)? {
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

/// Find PEP 723 scripts under a workspace root.
///
/// Respects ignore files and excludes repository internals, virtual environments, and the uv cache
/// from traversal.
fn find_scripts(workspace_root: &Path, cache: &Cache) -> Result<Vec<PathBuf>> {
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
    let walker = builder.build();

    let mut scripts = Vec::new();
    for entry in walker {
        let entry = entry.context("Failed to walk workspace while discovering PEP 723 scripts")?;
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
            || !is_python_script_path(entry.path())
        {
            continue;
        }

        let contents = fs_err::read(entry.path()).with_context(|| {
            format!(
                "Failed to read candidate PEP 723 script: {}",
                entry.path().simplified_display()
            )
        })?;
        if Pep723Metadata::parse(&contents)
            .with_context(|| {
                format!(
                    "Failed to parse PEP 723 script: {}",
                    entry.path().simplified_display()
                )
            })?
            .is_some()
        {
            scripts.push(entry.into_path());
        }
    }

    scripts.sort_unstable();
    Ok(scripts)
}

/// Return whether a path uses a conventional Python script filename.
///
/// PEP 723 does not require a specific filename, and uv can run explicitly requested scripts with
/// arbitrary extensions or no extension. For discovery, restrict the search to Python extensions
/// to avoid treating metadata examples embedded in documentation as scripts. This could be expanded
/// if arbitrary script filenames can be distinguished without introducing false positives.
fn is_python_script_path(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        extension.to_str().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("py") || extension.eq_ignore_ascii_case("pyw")
        })
    })
}
