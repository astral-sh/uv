use std::borrow::Cow;
use std::fmt::Write;
use std::io::{self, Read};
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

        let Some(contents) = read_script_candidate(entry.path()).with_context(|| {
            format!(
                "Failed to read candidate PEP 723 script: {}",
                entry.path().simplified_display()
            )
        })?
        else {
            continue;
        };
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

/// Read a candidate script.
///
/// Extensionless candidates are only read past their prefix when they begin with a shebang.
fn read_script_candidate(path: &Path) -> io::Result<Option<Vec<u8>>> {
    if path.extension().is_some() {
        return fs_err::read(path).map(Some);
    }

    let mut file = fs_err::File::open(path)?;
    read_extensionless_script(&mut file)
}

/// Read an extensionless script, if it starts with a shebang and is valid UTF-8 text.
fn read_extensionless_script(mut reader: impl Read) -> io::Result<Option<Vec<u8>>> {
    const READ_BUFFER_SIZE: usize = 8 * 1024;

    let mut prefix = [0; 2];
    match reader.read_exact(&mut prefix) {
        Ok(()) if &prefix == b"#!" => {}
        Ok(()) => return Ok(None),
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err),
    }

    let mut contents = prefix.to_vec();
    let mut valid_utf8_len = contents.len();
    let mut buffer = [0u8; READ_BUFFER_SIZE];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };

        let chunk = &buffer[..count];
        if chunk.contains(&0) {
            return Ok(None);
        }
        contents.extend_from_slice(chunk);
        match std::str::from_utf8(&contents[valid_utf8_len..]) {
            Ok(_) => valid_utf8_len = contents.len(),
            Err(err) if err.error_len().is_some() => return Ok(None),
            Err(err) => valid_utf8_len += err.valid_up_to(),
        }
    }

    Ok((valid_utf8_len == contents.len()).then_some(contents))
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

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor, Read};

    use super::read_extensionless_script;

    struct ErrorReader;

    impl Read for ErrorReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("read past binary marker"))
        }
    }

    #[test]
    fn extensionless_script_stops_at_non_shebang() -> io::Result<()> {
        let reader = Cursor::new(b"# ").chain(ErrorReader);
        assert!(read_extensionless_script(reader)?.is_none());
        Ok(())
    }

    #[test]
    fn extensionless_script_stops_at_binary_content() -> io::Result<()> {
        for marker in [0, 0xff] {
            let reader = Cursor::new([b'#', b'!', marker]).chain(ErrorReader);
            assert!(read_extensionless_script(reader)?.is_none());
        }
        Ok(())
    }

    #[test]
    fn extensionless_script_handles_split_utf8() -> io::Result<()> {
        let reader = Cursor::new(b"#!\xc3").chain(Cursor::new(b"\xa9"));
        assert_eq!(
            read_extensionless_script(reader)?,
            Some(b"#!\xc3\xa9".to_vec())
        );
        Ok(())
    }
}
