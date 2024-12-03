use globset::{Glob, GlobSet, GlobSetBuilder};
use regex_automata::dfa;
use regex_automata::dfa::Automaton;
use std::path::{Path, MAIN_SEPARATOR, MAIN_SEPARATOR_STR};
use tracing::warn;

/// Chosen at a whim -Konsti
const DFA_SIZE_LIMIT: usize = 1_000_000;

/// Filter a directory tree traversal (walkdir) by whether any paths of a directory can be included
/// at all.
///
/// Internally, the globs are converted to a regex and then to a DFA, which unlike the globs and the
/// regex allows to check for prefix matches.
pub struct GlobDirFilter {
    glob_set: GlobSet,
    dfa: Option<dfa::dense::DFA<Vec<u32>>>,
}

impl GlobDirFilter {
    /// The filter matches if any of the globs matches.
    ///
    /// See <https://github.com/BurntSushi/ripgrep/discussions/2927> for the error returned.
    pub fn from_globs(globs: &[Glob]) -> Result<Self, globset::Error> {
        let mut glob_set_builder = GlobSetBuilder::new();
        for glob in globs {
            glob_set_builder.add(glob.clone());
        }
        let glob_set = glob_set_builder.build()?;

        let regexes: Vec<_> = globs
            .iter()
            .map(|glob| {
                let main_separator = regex::escape(MAIN_SEPARATOR_STR);
                let regex = glob
                    .regex()
                    // We are using a custom DFA builder
                    .strip_prefix("(?-u)")
                    .expect("a glob is a non-unicode byte regex")
                    // Match windows paths if applicable
                    .replace('/', &main_separator);
                regex
            })
            .collect();

        let dfa_builder = dfa::dense::Builder::new()
            .syntax(
                // The glob regex is a byte matcher
                regex_automata::util::syntax::Config::new()
                    .unicode(false)
                    .utf8(false),
            )
            .configure(
                dfa::dense::Config::new()
                    .start_kind(dfa::StartKind::Anchored)
                    // DFA can grow exponentially, in which case we bail out
                    .dfa_size_limit(Some(DFA_SIZE_LIMIT))
                    .determinize_size_limit(Some(DFA_SIZE_LIMIT)),
            )
            .build_many(&regexes);
        let dfa = if let Ok(dfa) = dfa_builder {
            Some(dfa)
        } else {
            // TODO(konsti): `regex_automata::dfa::dense::BuildError` should allow asking whether
            // is a size error
            warn!(
                "Glob expressions regex is larger than {DFA_SIZE_LIMIT} bytes, \
                    falling back to full directory traversal!"
            );
            None
        };

        Ok(Self { glob_set, dfa })
    }

    /// Whether the path (file or directory) matches any of the globs.
    ///
    /// We include a directory if we are potentially including files it contains.
    pub fn match_path(&self, path: &Path) -> bool {
        self.match_directory(path) || self.glob_set.is_match(path)
    }

    /// Check whether a directory or any of its children can be matched by any of the globs.
    ///
    /// This option never returns false if any child matches, but it may return true even if we
    /// don't end up including any child.
    pub fn match_directory(&self, path: &Path) -> bool {
        let Some(dfa) = &self.dfa else {
            return false;
        };

        // Allow the root path
        if path == Path::new("") {
            return true;
        }

        let config_anchored =
            regex_automata::util::start::Config::new().anchored(regex_automata::Anchored::Yes);
        let mut state = dfa.start_state(&config_anchored).unwrap();

        // Paths aren't necessarily UTF-8, which we can gloss over since the globs match bytes only
        // anyway.
        let byte_path = path.as_os_str().as_encoded_bytes();
        for b in byte_path {
            state = dfa.next_state(state, *b);
        }
        // Say we're looking at a directory `foo/bar`. We want to continue if either `foo/bar` is
        // a match, e.g., from `foo/*`, or a path below it can match, e.g., from `foo/bar/*`.
        let eoi_state = dfa.next_eoi_state(state);
        // We must not call `next_eoi_state` on the slash state, we want to only check if more
        // characters (path components) are allowed, not if we're matching the `$` anchor at the
        // end.
        let slash_state = dfa.next_state(state, u8::try_from(MAIN_SEPARATOR).unwrap());

        debug_assert!(
            !dfa.is_quit_state(eoi_state) && !dfa.is_quit_state(slash_state),
            "matcher is in quit state"
        );

        dfa.is_match_state(eoi_state) || !dfa.is_dead_state(slash_state)
    }
}

#[cfg(test)]
mod tests {
    use crate::glob_dir_filter::GlobDirFilter;
    use crate::portable_glob::parse_portable_glob;
    use std::path::{Path, MAIN_SEPARATOR};
    use tempfile::tempdir;
    use walkdir::WalkDir;

    const FILES: [&str; 5] = [
        "path1/dir1/subdir/a.txt",
        "path2/dir2/subdir/a.txt",
        "path3/dir3/subdir/a.txt",
        "path4/dir4/subdir/a.txt",
        "path5/dir5/subdir/a.txt",
    ];

    const PATTERNS: [&str; 5] = [
        // Only sufficient for descending one level
        "path1/*",
        // Only sufficient for descending one level
        "path2/dir2",
        // Sufficient for descending
        "path3/dir3/subdir/a.txt",
        // Sufficient for descending
        "path4/**/*",
        // Not sufficient for descending
        "path5",
    ];

    #[test]
    fn match_directory() {
        let patterns = PATTERNS.map(|pattern| parse_portable_glob(pattern).unwrap());
        let matcher = GlobDirFilter::from_globs(&patterns).unwrap();
        assert!(matcher.match_directory(&Path::new("path1").join("dir1")));
        assert!(matcher.match_directory(&Path::new("path2").join("dir2")));
        assert!(matcher.match_directory(&Path::new("path3").join("dir3")));
        assert!(matcher.match_directory(&Path::new("path4").join("dir4")));
        assert!(!matcher.match_directory(&Path::new("path5").join("dir5")));
    }

    /// Check that we skip directories that can never match.
    #[test]
    fn prefilter() {
        let dir = tempdir().unwrap();
        for file in FILES {
            let file = dir.path().join(file);
            fs_err::create_dir_all(file.parent().unwrap()).unwrap();
            fs_err::File::create(file).unwrap();
        }
        let patterns = PATTERNS.map(|pattern| parse_portable_glob(pattern).unwrap());
        let matcher = GlobDirFilter::from_globs(&patterns).unwrap();

        // Test the prefix filtering
        let mut visited: Vec<_> = WalkDir::new(dir.path())
            .into_iter()
            .filter_entry(|entry| {
                let relative = entry
                    .path()
                    .strip_prefix(dir.path())
                    .expect("walkdir starts with root");
                matcher.match_directory(relative)
            })
            .map(|entry| {
                let entry = entry.unwrap();
                let relative = entry
                    .path()
                    .strip_prefix(dir.path())
                    .expect("walkdir starts with root")
                    .to_str()
                    .unwrap()
                    .to_string();
                // Translate windows paths back to the unix fixture
                relative.replace(MAIN_SEPARATOR, "/")
            })
            .collect();
        visited.sort();
        assert_eq!(
            visited,
            [
                "",
                "path1",
                "path1/dir1",
                "path2",
                "path2/dir2",
                "path3",
                "path3/dir3",
                "path3/dir3/subdir",
                "path3/dir3/subdir/a.txt",
                "path4",
                "path4/dir4",
                "path4/dir4/subdir",
                "path4/dir4/subdir/a.txt",
                "path5"
            ]
        );
    }

    /// Check that the walkdir yield the correct set of files.
    #[test]
    fn walk_dir() {
        let dir = tempdir().unwrap();

        for file in FILES {
            let file = dir.path().join(file);
            fs_err::create_dir_all(file.parent().unwrap()).unwrap();
            fs_err::File::create(file).unwrap();
        }
        let patterns = PATTERNS.map(|pattern| parse_portable_glob(pattern).unwrap());

        let include_matcher = GlobDirFilter::from_globs(&patterns).unwrap();

        let walkdir_root = dir.path();
        let mut matches: Vec<_> = WalkDir::new(walkdir_root)
            .into_iter()
            .filter_entry(|entry| {
                // TODO(konsti): This should be prettier.
                let relative = entry
                    .path()
                    .strip_prefix(walkdir_root)
                    .expect("walkdir starts with root");

                include_matcher.match_directory(relative)
            })
            .filter_map(|entry| {
                let entry = entry.as_ref().unwrap();
                // TODO(konsti): This should be prettier.
                let relative = entry
                    .path()
                    .strip_prefix(walkdir_root)
                    .expect("walkdir starts with root");
                if include_matcher.match_path(relative) {
                    // Translate windows paths back to the unix fixture
                    Some(relative.to_str().unwrap().replace(MAIN_SEPARATOR, "/"))
                } else {
                    None
                }
            })
            .collect();
        matches.sort();
        assert_eq!(
            matches,
            [
                "",
                "path1",
                "path1/dir1",
                "path2",
                "path2/dir2",
                "path3",
                "path3/dir3",
                "path3/dir3/subdir",
                "path3/dir3/subdir/a.txt",
                "path4",
                "path4/dir4",
                "path4/dir4/subdir",
                "path4/dir4/subdir/a.txt",
                "path5"
            ]
        );
    }
}
