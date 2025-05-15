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
            child.collect_patterns(prefix.join(part), patterns);
        }
    }

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
            self.collect_patterns(PathBuf::new(), &mut group);
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
