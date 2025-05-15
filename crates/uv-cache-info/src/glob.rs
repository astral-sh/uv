use std::{
    collections::BTreeMap,
    path::{Component, Components, Path, PathBuf},
};

fn is_glob_like(part: Component) -> bool {
    matches!(part, Component::Normal(_))
        && part
            .as_os_str()
            .to_str()
            .is_some_and(|part| ["*", "{", "}", "?"].into_iter().any(|c| part.contains(c)))
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
    let mut current = PathBuf::new();
    let mut globbing = false;
    let mut last = None;

    for part in pattern.components() {
        if let Some(last) = last {
            if last != Component::CurDir {
                current.push(last);
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
    fn collect_patterns(&self, prefix: PathBuf, patterns: &mut Vec<PathBuf>) {
        // collect all patterns beneath and including this node
        for pattern in &self.patterns {
            patterns.push(prefix.join(pattern));
        }
        for (part, child) in &self.children {
            if let Component::Normal(_) = part {
                // important: we only include normal components here
                child.collect_patterns(prefix.join(part), patterns);
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
            // note: only normal components are included here
            self.collect_patterns(PathBuf::new(), &mut group);
            groups.push((prefix.clone(), group));
            // for non-normal components, we may have to descend separately
            for (part, child) in &self.children {
                if !matches!(part, Component::Normal(_)) {
                    child.collect_groups(prefix.join(part), groups);
                }
            }
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
    use std::path::PathBuf;

    use super::{cluster_globs, split_glob, GlobParts};

    fn dewindows(path: &str) -> String {
        if cfg!(windows) {
            path.replace('\\', "/")
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
        check("*", "", "*");
        check("*/*", "", "*/*");
        check("..", "..", "");
        check("/", "/", "");
    }

    #[test]
    fn test_cluster_globs() {
        #[track_caller]
        fn check(input: &[&str], expected: &[(&str, &[&str])]) {
            let result = cluster_globs(input);
            let expected_converted: Vec<(PathBuf, Vec<String>)> = expected
                .iter()
                .map(|&(base, patterns)| {
                    (
                        dewindows(base).into(),
                        patterns.iter().map(|&s| dewindows(s)).collect(),
                    )
                })
                .collect();
            assert_eq!(result, expected_converted, "{input:?} != {expected:?}");
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
                "../shared/a/*.png",
                "../shared/a/b/*",
                "../shared/b/c/?x/d",
                "docs/important/*.{doc,xls}",
                "docs/important/very/*",
            ],
            &[
                ("../shared/a", &["*.png", "b/*"]),
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
    }
}
