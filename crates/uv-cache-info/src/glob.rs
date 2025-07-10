use std::{
    collections::BTreeMap,
    path::{Component, Components, Path, PathBuf},
};

/// Check if a component of the path looks like it may be a glob pattern.
///
/// Note: this function is being used when splitting a glob pattern into a long possible
/// base and the glob remainder (scanning through components until we hit the first component
/// for which this function returns true). It is acceptable for this function to return
/// false positives (e.g. patterns like 'foo[bar' or 'foo{bar') in which case correctness
/// will not be affected but efficiency might be (because we'll traverse more than we should),
/// however it should not return false negatives.
fn is_glob_like(part: Component) -> bool {
    matches!(part, Component::Normal(_))
        && part.as_os_str().to_str().is_some_and(|part| {
            ["*", "{", "}", "?", "[", "]"]
                .into_iter()
                .any(|c| part.contains(c))
        })
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct GlobParts {
    base: PathBuf,
    pattern: PathBuf,
}

/// Split a glob into longest possible base + shortest possible glob pattern.
fn split_glob(pattern: impl AsRef<str>) -> GlobParts {
    let pattern: &Path = pattern.as_ref().as_ref();

    let mut glob = GlobParts::default();
    let mut globbing = false;
    let mut last = None;

    for part in pattern.components() {
        if let Some(last) = last {
            if last != Component::CurDir {
                if globbing {
                    glob.pattern.push(last);
                } else {
                    glob.base.push(last);
                }
            }
        }
        if !globbing {
            globbing = is_glob_like(part);
        }
        // we don't know if this part is the last one, defer handling it by one iteration
        last = Some(part);
    }

    if let Some(last) = last {
        // defer handling the last component to prevent draining entire pattern into base
        if globbing || matches!(last, Component::Normal(_)) {
            glob.pattern.push(last);
        } else {
            glob.base.push(last);
        }
    }
    glob
}

/// Classic trie with edges being path components and values being glob patterns.
#[derive(Default)]
struct Trie<'a> {
    children: BTreeMap<Component<'a>, Trie<'a>>,
    patterns: Vec<&'a Path>,
}

impl<'a> Trie<'a> {
    fn insert(&mut self, mut components: Components<'a>, pattern: &'a Path) {
        if let Some(part) = components.next() {
            self.children
                .entry(part)
                .or_default()
                .insert(components, pattern);
        } else {
            self.patterns.push(pattern);
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    fn collect_patterns(
        &self,
        pattern_prefix: PathBuf,
        group_prefix: PathBuf,
        patterns: &mut Vec<PathBuf>,
        groups: &mut Vec<(PathBuf, Vec<PathBuf>)>,
    ) {
        // collect all patterns beneath and including this node
        for pattern in &self.patterns {
            patterns.push(pattern_prefix.join(pattern));
        }
        for (part, child) in &self.children {
            if let Component::Normal(_) = part {
                // for normal components, collect all descendant patterns ('normal' edges only)
                child.collect_patterns(
                    pattern_prefix.join(part),
                    group_prefix.join(part),
                    patterns,
                    groups,
                );
            } else {
                // for non-normal component edges, kick off separate group collection at this node
                child.collect_groups(group_prefix.join(part), groups);
            }
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    fn collect_groups(&self, prefix: PathBuf, groups: &mut Vec<(PathBuf, Vec<PathBuf>)>) {
        // LCP-style grouping of patterns
        if self.patterns.is_empty() {
            // no patterns in this node; child nodes can form independent groups
            for (part, child) in &self.children {
                child.collect_groups(prefix.join(part), groups);
            }
        } else {
            // pivot point, we've hit a pattern node; we have to stop here and form a group
            let mut group = Vec::new();
            self.collect_patterns(PathBuf::new(), prefix.clone(), &mut group, groups);
            groups.push((prefix, group));
        }
    }
}

/// Given a collection of globs, cluster them into (base, globs) groups so that:
/// - base doesn't contain any glob symbols
/// - each directory would only be walked at most once
/// - base of each group is the longest common prefix of globs in the group
pub(crate) fn cluster_globs(patterns: &[impl AsRef<str>]) -> Vec<(PathBuf, Vec<String>)> {
    // split all globs into base/pattern
    let globs: Vec<_> = patterns.iter().map(split_glob).collect();

    // construct a path trie out of all split globs
    let mut trie = Trie::default();
    for glob in &globs {
        trie.insert(glob.base.components(), &glob.pattern);
    }

    // run LCP-style aggregation of patterns in the trie into groups
    let mut groups = Vec::new();
    trie.collect_groups(PathBuf::new(), &mut groups);

    // finally, convert resulting patterns to strings
    groups
        .into_iter()
        .map(|(base, patterns)| {
            (
                base,
                patterns
                    .iter()
                    // NOTE: this unwrap is ok because input patterns are valid utf-8
                    .map(|p| p.to_str().unwrap().to_owned())
                    .collect(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{GlobParts, cluster_globs, split_glob};

    fn windowsify(path: &str) -> String {
        if cfg!(windows) {
            path.replace('/', "\\")
        } else {
            path.to_owned()
        }
    }

    #[test]
    fn test_split_glob() {
        #[track_caller]
        fn check(input: &str, base: &str, pattern: &str) {
            let result = split_glob(input);
            let expected = GlobParts {
                base: base.into(),
                pattern: pattern.into(),
            };
            assert_eq!(result, expected, "{input:?} != {base:?} + {pattern:?}");
        }

        check("", "", "");
        check("a", "", "a");
        check("a/b", "a", "b");
        check("a/b/", "a", "b");
        check("a/.//b/", "a", "b");
        check("./a/b/c", "a/b", "c");
        check("c/d/*", "c/d", "*");
        check("c/d/*/../*", "c/d", "*/../*");
        check("a/?b/c", "a", "?b/c");
        check("/a/b/*", "/a/b", "*");
        check("../x/*", "../x", "*");
        check("a/{b,c}/d", "a", "{b,c}/d");
        check("a/[bc]/d", "a", "[bc]/d");
        check("*", "", "*");
        check("*/*", "", "*/*");
        check("..", "..", "");
        check("/", "/", "");
    }

    #[test]
    fn test_cluster_globs() {
        #[track_caller]
        fn check(input: &[&str], expected: &[(&str, &[&str])]) {
            let input = input.iter().map(|s| windowsify(s)).collect::<Vec<_>>();

            let mut result_sorted = cluster_globs(&input);
            for (_, patterns) in &mut result_sorted {
                patterns.sort_unstable();
            }
            result_sorted.sort_unstable();

            let mut expected_sorted = Vec::new();
            for (base, patterns) in expected {
                let mut patterns_sorted = Vec::new();
                for pattern in *patterns {
                    patterns_sorted.push(windowsify(pattern));
                }
                patterns_sorted.sort_unstable();
                expected_sorted.push((windowsify(base).into(), patterns_sorted));
            }
            expected_sorted.sort_unstable();

            assert_eq!(
                result_sorted, expected_sorted,
                "{input:?} != {expected_sorted:?} (got: {result_sorted:?})"
            );
        }

        check(&["a/b/*", "a/c/*"], &[("a/b", &["*"]), ("a/c", &["*"])]);
        check(&["./a/b/*", "a/c/*"], &[("a/b", &["*"]), ("a/c", &["*"])]);
        check(&["/a/b/*", "/a/c/*"], &[("/a/b", &["*"]), ("/a/c", &["*"])]);
        check(
            &["../a/b/*", "../a/c/*"],
            &[("../a/b", &["*"]), ("../a/c", &["*"])],
        );
        check(&["x/*", "y/*"], &[("x", &["*"]), ("y", &["*"])]);
        check(&[], &[]);
        check(
            &["./*", "a/*", "../foo/*.png"],
            &[("", &["*", "a/*"]), ("../foo", &["*.png"])],
        );
        check(
            &[
                "?",
                "/foo/?",
                "/foo/bar/*",
                "../bar/*.png",
                "../bar/../baz/*.jpg",
            ],
            &[
                ("", &["?"]),
                ("/foo", &["?", "bar/*"]),
                ("../bar", &["*.png"]),
                ("../bar/../baz", &["*.jpg"]),
            ],
        );
        check(&["/abs/path/*"], &[("/abs/path", &["*"])]);
        check(&["/abs/*", "rel/*"], &[("/abs", &["*"]), ("rel", &["*"])]);
        check(&["a/{b,c}/*", "a/d?/*"], &[("a", &["{b,c}/*", "d?/*"])]);
        check(
            &[
                "../shared/a/[abc].png",
                "../shared/a/b/*",
                "../shared/b/c/?x/d",
                "docs/important/*.{doc,xls}",
                "docs/important/very/*",
            ],
            &[
                ("../shared/a", &["[abc].png", "b/*"]),
                ("../shared/b/c", &["?x/d"]),
                ("docs/important", &["*.{doc,xls}", "very/*"]),
            ],
        );
        check(&["file.txt"], &[("", &["file.txt"])]);
        check(&["/"], &[("/", &[""])]);
        check(&[".."], &[("..", &[""])]);
        check(
            &["file1.txt", "file2.txt"],
            &[("", &["file1.txt", "file2.txt"])],
        );
        check(
            &["a/file1.txt", "a/file2.txt"],
            &[("a", &["file1.txt", "file2.txt"])],
        );
        check(
            &["*", "a/b/*", "a/../c/*.jpg", "a/../c/*.png", "/a/*", "/b/*"],
            &[
                ("", &["*", "a/b/*"]),
                ("a/../c", &["*.jpg", "*.png"]),
                ("/a", &["*"]),
                ("/b", &["*"]),
            ],
        );

        if cfg!(windows) {
            check(
                &[
                    r"\\foo\bar\shared/a/[abc].png",
                    r"\\foo\bar\shared/a/b/*",
                    r"\\foo\bar/shared/b/c/?x/d",
                    r"D:\docs\important/*.{doc,xls}",
                    r"D:\docs/important/very/*",
                ],
                &[
                    (r"\\foo\bar\shared\a", &["[abc].png", r"b\*"]),
                    (r"\\foo\bar\shared\b\c", &[r"?x\d"]),
                    (r"D:\docs\important", &["*.{doc,xls}", r"very\*"]),
                ],
            );
        }
    }
}
