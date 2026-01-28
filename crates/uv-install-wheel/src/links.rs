//! PEP 778 LINKS file handling for wheel installation.
//!
//! <https://peps.python.org/pep-0778/>

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use tracing::trace;

use crate::Error;
use crate::record::RecordEntry;

/// An entry from a LINKS file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinksEntry {
    pub source_path: String,
    pub target_path: String,
}

/// Parse a LINKS file.
pub(crate) fn read_links_file(reader: &mut impl Read) -> Result<Vec<LinksEntry>, Error> {
    let mut entries = Vec::new();

    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(reader);

    for result in csv_reader.records() {
        let record = result?;

        if record.len() < 2 {
            return Err(Error::LinksInvalidFormat(format!(
                "LINKS entry must have at least 2 fields, got {}",
                record.len()
            )));
        }

        let source_path = record.get(0).unwrap_or("").to_string();
        let target_path = record.get(1).unwrap_or("").to_string();

        if source_path.is_empty() || target_path.is_empty() {
            return Err(Error::LinksInvalidFormat(
                "LINKS entry has empty source or target path".to_string(),
            ));
        }

        entries.push(LinksEntry {
            source_path,
            target_path,
        });
    }

    Ok(entries)
}

/// Check if a path escapes the package namespace.
fn path_escapes_namespace(path: &str) -> bool {
    let path = Path::new(path);
    let mut depth: i32 = 0;

    for component in path.components() {
        match component {
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            }
            Component::Normal(_) => {
                depth += 1;
            }
            Component::RootDir => {
                // Absolute paths are not allowed
                return true;
            }
            Component::CurDir | Component::Prefix(_) => {}
        }
    }

    false
}

/// Normalize a path by resolving `.` and `..` components.
fn normalize_path(path: &str) -> PathBuf {
    let path = Path::new(path);
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            Component::ParentDir => {
                components.pop();
            }
            Component::CurDir => {}
            Component::Normal(part) => {
                components.push(part);
            }
            Component::RootDir | Component::Prefix(_) => {}
        }
    }

    components.iter().collect()
}

/// DFS visit state for cycle detection.
#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    InProgress,
    Completed,
}

/// DFS helper for cycle detection.
fn dfs_cycle_check(
    node: &PathBuf,
    graph: &HashMap<PathBuf, PathBuf>,
    state: &mut HashMap<PathBuf, VisitState>,
    path: &mut Vec<PathBuf>,
) -> Result<(), Error> {
    match state.get(node) {
        Some(VisitState::Completed) => return Ok(()),
        Some(VisitState::InProgress) => {
            path.push(node.clone());
            let cycle_desc: Vec<String> = path.iter().map(|p| p.display().to_string()).collect();
            return Err(Error::LinksCycle(cycle_desc.join(" -> ")));
        }
        None => {}
    }

    state.insert(node.clone(), VisitState::InProgress);
    path.push(node.clone());

    if let Some(target) = graph.get(node) {
        if graph.contains_key(target) {
            dfs_cycle_check(target, graph, state, path)?;
        }
    }

    path.pop();
    state.insert(node.clone(), VisitState::Completed);
    Ok(())
}

/// Detect cycles in the symlink graph.
fn detect_cycles(entries: &[LinksEntry]) -> Result<(), Error> {
    let mut graph: HashMap<PathBuf, PathBuf> = HashMap::new();
    for entry in entries {
        let source = normalize_path(&entry.source_path);
        let target = normalize_path(&entry.target_path);
        graph.insert(source, target);
    }

    let mut state: HashMap<PathBuf, VisitState> = HashMap::new();
    let mut sources: Vec<_> = graph.keys().collect();
    sources.sort();

    for source in sources {
        if !state.contains_key(source) {
            let mut path = Vec::new();
            dfs_cycle_check(source, &graph, &mut state, &mut path)?;
        }
    }

    Ok(())
}

/// Validate LINKS entries for path escapes, cycles, and dangling targets.
pub(crate) fn validate_links(
    entries: &[LinksEntry],
    existing_files: &HashSet<PathBuf>,
) -> Result<(), Error> {
    for entry in entries {
        if path_escapes_namespace(&entry.source_path) {
            return Err(Error::LinksPathEscape(entry.source_path.clone()));
        }
        if path_escapes_namespace(&entry.target_path) {
            return Err(Error::LinksPathEscape(entry.target_path.clone()));
        }
    }

    detect_cycles(entries)?;

    let symlink_sources: HashSet<PathBuf> = entries
        .iter()
        .map(|e| normalize_path(&e.source_path))
        .collect();

    for entry in entries {
        let target_normalized = normalize_path(&entry.target_path);
        let in_record = existing_files.contains(&target_normalized);
        let will_be_symlink = symlink_sources.contains(&target_normalized);

        if !in_record && !will_be_symlink {
            return Err(Error::LinksDangling {
                link_source: entry.source_path.clone(),
                link_target: entry.target_path.clone(),
            });
        }
    }

    Ok(())
}

/// Compute the relative symlink target from source's parent to target.
fn relative_symlink_target(source: &str, target: &str) -> PathBuf {
    let source_path = Path::new(source);
    let target_path = Path::new(target);
    let source_parent = source_path.parent().unwrap_or(Path::new(""));

    pathdiff::diff_paths(target_path, source_parent).unwrap_or_else(|| target_path.to_path_buf())
}

/// Install symlinks from LINKS entries, returning the number created.
#[cfg(unix)]
pub(crate) fn install_links(
    site_packages: &Path,
    entries: &[LinksEntry],
    record: &mut Vec<RecordEntry>,
) -> Result<usize, Error> {
    let mut count = 0;

    for entry in entries {
        let source_path = site_packages.join(&entry.source_path);
        let relative_target = relative_symlink_target(&entry.source_path, &entry.target_path);

        if let Some(parent) = source_path.parent() {
            if !parent.exists() {
                fs_err::create_dir_all(parent)?;
            }
        }

        trace!(
            "Creating symlink: {} -> {}",
            source_path.display(),
            relative_target.display()
        );

        fs_err::os::unix::fs::symlink(&relative_target, &source_path).map_err(|err| {
            Error::LinksSymlinkFailed {
                source: entry.source_path.clone(),
                target: entry.target_path.clone(),
                err,
            }
        })?;

        // Add to RECORD (symlinks have no hash or size)
        record.push(RecordEntry {
            path: entry.source_path.clone(),
            hash: None,
            size: None,
        });

        count += 1;
    }

    Ok(count)
}

/// Stub for non-Unix platforms.
#[cfg(not(unix))]
pub(crate) fn install_links(
    _site_packages: &Path,
    entries: &[LinksEntry],
    _record: &mut Vec<RecordEntry>,
) -> Result<usize, Error> {
    if !entries.is_empty() {
        return Err(Error::LinksNotSupported);
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_links_file() {
        let content = "foo/bar.py,foo/baz.py\nspam/eggs.py,spam/ham.py\n";
        let mut reader = Cursor::new(content);
        let entries = read_links_file(&mut reader).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].source_path, "foo/bar.py");
        assert_eq!(entries[0].target_path, "foo/baz.py");
        assert_eq!(entries[1].source_path, "spam/eggs.py");
        assert_eq!(entries[1].target_path, "spam/ham.py");
    }

    #[test]
    fn test_read_links_file_empty() {
        let content = "";
        let mut reader = Cursor::new(content);
        let entries = read_links_file(&mut reader).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_read_links_file_invalid_format() {
        let content = "only_one_field\n";
        let mut reader = Cursor::new(content);
        let result = read_links_file(&mut reader);
        assert!(result.is_err());
    }

    #[test]
    fn test_path_escapes_namespace() {
        // Valid paths
        assert!(!path_escapes_namespace("foo/bar.py"));
        assert!(!path_escapes_namespace("foo/../foo/bar.py"));
        assert!(!path_escapes_namespace("./foo/bar.py"));

        // Invalid paths (escape namespace)
        assert!(path_escapes_namespace("../foo/bar.py"));
        assert!(path_escapes_namespace("foo/../../bar.py"));
        assert!(path_escapes_namespace("/absolute/path"));
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("foo/bar.py"), PathBuf::from("foo/bar.py"));
        assert_eq!(
            normalize_path("foo/../baz/bar.py"),
            PathBuf::from("baz/bar.py")
        );
        assert_eq!(normalize_path("./foo/bar.py"), PathBuf::from("foo/bar.py"));
        assert_eq!(
            normalize_path("foo/./bar/../baz.py"),
            PathBuf::from("foo/baz.py")
        );
    }

    #[test]
    fn test_detect_cycles_no_cycle() {
        let entries = vec![
            LinksEntry {
                source_path: "a".to_string(),
                target_path: "b".to_string(),
            },
            LinksEntry {
                source_path: "b".to_string(),
                target_path: "c".to_string(),
            },
        ];
        assert!(detect_cycles(&entries).is_ok());
    }

    #[test]
    fn test_detect_cycles_with_cycle() {
        let entries = vec![
            LinksEntry {
                source_path: "a".to_string(),
                target_path: "b".to_string(),
            },
            LinksEntry {
                source_path: "b".to_string(),
                target_path: "a".to_string(),
            },
        ];
        let result = detect_cycles(&entries);
        assert!(result.is_err());
        if let Err(Error::LinksCycle(desc)) = result {
            assert!(desc.contains("a") && desc.contains("b"));
        }
    }

    #[test]
    fn test_detect_cycles_self_reference() {
        let entries = vec![LinksEntry {
            source_path: "a".to_string(),
            target_path: "a".to_string(),
        }];
        let result = detect_cycles(&entries);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_links_path_escape() {
        let entries = vec![LinksEntry {
            source_path: "../escape/path.py".to_string(),
            target_path: "valid/target.py".to_string(),
        }];
        let result = validate_links(&entries, &HashSet::new());
        assert!(matches!(result, Err(Error::LinksPathEscape(_))));
    }

    #[test]
    fn test_relative_symlink_target() {
        // Same directory - just the filename
        assert_eq!(
            relative_symlink_target("pkg/lib/foo.so", "pkg/lib/foo.so.1.2.3"),
            PathBuf::from("foo.so.1.2.3")
        );

        // Target in parent directory
        assert_eq!(
            relative_symlink_target("pkg/sub/link.py", "pkg/target.py"),
            PathBuf::from("../target.py")
        );

        // Target in sibling directory
        assert_eq!(
            relative_symlink_target("pkg/a/link.py", "pkg/b/target.py"),
            PathBuf::from("../b/target.py")
        );

        // Deep .data path (real wheel scenario)
        assert_eq!(
            relative_symlink_target(
                "pkg-1.0.data/data/lib/native/lib/libbz2.dylib",
                "pkg-1.0.data/data/lib/native/lib/libbz2.1.0.8.dylib"
            ),
            PathBuf::from("libbz2.1.0.8.dylib")
        );

        // Source at root level
        assert_eq!(
            relative_symlink_target("link.py", "pkg/target.py"),
            PathBuf::from("pkg/target.py")
        );
    }
}
