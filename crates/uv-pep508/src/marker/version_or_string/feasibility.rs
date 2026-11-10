//! Check a spelling constraint and a version constraint on the same field.
use std::fmt::Write;
use std::ops::Bound;

use arcstr::ArcStr;
use regex_automata::Input;
use regex_automata::dfa::{Automaton, dense};
use regex_automata::util::primitives::StateID;
use rustc_hash::FxHashSet;
use uv_pep440::{LocalSegment, LocalVersionSlice, PrereleaseKind, Version};
use version_ranges::Ranges;

use super::{VersionOrStringValue, numeric_range, split};
use crate::marker::algebra::{Edges, INTERNER, Variable};
use crate::{CanonicalMarkerValueString, MarkerOperator, MarkerTree};

/// What the spelling and version comparisons say about a potentially conflicting fork.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformMarkerFeasibility {
    /// Every path to true requires an impossible spelling.
    Empty,
    /// At least one path has no unresolved spelling and version comparison.
    Possible,
    /// A path might accept a value of this field, but the checker could not decide.
    Unknown(CanonicalMarkerValueString),
}

/// Conditions on a single raw environment value along one path through the tree.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
struct Spelling<'a> {
    range: Option<&'a Ranges<ArcStr>>,
    containment: bool,
}

impl MarkerTree {
    /// Check the platform spelling constraints of a failed resolver fork.
    ///
    /// A path is empty if no single spelling can satisfy its raw and parsed comparisons.
    /// Other marker variables use the usual decision edges. A possible path is therefore
    /// evidence only about these platform constraints.
    pub fn platform_feasibility(self) -> PlatformMarkerFeasibility {
        if self.is_true() {
            return PlatformMarkerFeasibility::Possible;
        }
        if self.is_false() {
            return PlatformMarkerFeasibility::Empty;
        }
        let slot = |key| usize::from(key == CanonicalMarkerValueString::PlatformVersion);
        let mut pending = vec![(self.0, [Spelling::default(); 2], None)];
        let mut seen = FxHashSet::default();
        let mut unknown = None;
        while let Some((id, spellings, uncertain)) = pending.pop() {
            if id.is_false() {
                continue;
            }
            if id.is_true() {
                let Some(key) = uncertain else {
                    return PlatformMarkerFeasibility::Possible;
                };
                unknown = Some(key);
                continue;
            }
            if !seen.insert((id, spellings, uncertain)) {
                continue;
            }
            let node = INTERNER.shared.node(id);
            match (&node.var, &node.children) {
                (Variable::String(key), Edges::String { edges }) if key.is_version_or_string() => {
                    for (range, child) in edges {
                        let mut spellings = spellings;
                        spellings[slot(*key)].range = Some(range);
                        pending.push((child.negate(id), spellings, uncertain));
                    }
                }
                (Variable::VersionString(key), Edges::VersionOrString { edges }) => {
                    let spelling = spellings[slot(*key)];
                    for (range, child) in edges {
                        let child = child.negate(id);
                        if child.is_false() {
                            continue;
                        }
                        let mut uncertain = uncertain;
                        if let Some(raw) = spelling.range {
                            match check(raw, range) {
                                Feasibility::Empty => continue,
                                Feasibility::Unknown => uncertain = Some(*key),
                                Feasibility::Witness(_) => {}
                            }
                        }
                        // A witness for two ranges need not satisfy a separate substring test.
                        if spelling.containment {
                            uncertain = Some(*key);
                        }
                        pending.push((child, spellings, uncertain));
                    }
                }
                _ => {
                    let mut spellings = spellings;
                    if let Variable::In { key, .. } | Variable::Contains { key, .. } = node.var
                        && key.is_version_or_string()
                    {
                        spellings[slot(key)].containment = true;
                    }
                    pending.extend(
                        node.children
                            .nodes()
                            .map(|child| (child.negate(id), spellings, uncertain)),
                    );
                }
            }
        }
        unknown.map_or(
            PlatformMarkerFeasibility::Empty,
            PlatformMarkerFeasibility::Unknown,
        )
    }
}

#[derive(Debug)]
enum Feasibility {
    Empty,
    Witness(ArcStr),
    Unknown,
}

/// Reparse the concrete spelling before accepting a witness, using the same
/// raw value for both predicates. An inconclusive search never prunes a fork.
fn check(raw: &Ranges<ArcStr>, typed: &Ranges<VersionOrStringValue>) -> Feasibility {
    let verify = |s: ArcStr| {
        let value = match s.parse() {
            Ok(v) => VersionOrStringValue::Version(v),
            Err(_) => VersionOrStringValue::Opaque(s.clone()),
        };
        if raw.contains(&s) && typed.contains(&value) {
            Feasibility::Witness(s)
        } else {
            Feasibility::Unknown
        }
    };
    let (versions, opaque) = split(typed);
    for (lo, hi) in raw.intersection(&opaque).iter() {
        // NUL is the least string extension. If the first string is a version,
        // its immediate successor is opaque.
        let mut s = match lo {
            Bound::Unbounded => String::new(),
            Bound::Included(s) => s.to_string(),
            Bound::Excluded(s) => format!("{s}\0"),
        };
        if s.parse::<Version>().is_ok() {
            s.push('\0');
        }
        let s = ArcStr::from(s);
        if upper_admits(hi, &s) {
            return verify(s);
        }
    }
    let mut unknown = false;
    for (lo, hi) in versions.iter() {
        let interval = Ranges::from_range_bounds((lo.cloned(), hi.cloned()));
        let Bound::Included(v) = lo else {
            unknown = true;
            continue;
        };
        let s = ArcStr::from(v.to_string());
        if s.parse::<Version>().as_ref().ok() != Some(v) {
            unknown = true;
            continue;
        }
        if raw.contains(&s) {
            return verify(s);
        }
        let public = numeric_range(MarkerOperator::Equal, &s).as_ref() == Some(&interval)
            && v.local().is_empty();
        if !public && interval.as_singleton().is_none() {
            unknown = true;
            continue;
        }
        let Ok(dfa) = dense::DFA::new(&spellings(v, public)) else {
            unknown = true;
            continue;
        };
        for (low, high) in raw.iter() {
            match search(&dfa, low, high) {
                Feasibility::Witness(s) => return verify(s),
                Feasibility::Empty => {}
                Feasibility::Unknown => unknown = true,
            }
        }
    }
    if unknown {
        Feasibility::Unknown
    } else {
        Feasibility::Empty
    }
}

fn upper_admits(upper: Bound<&ArcStr>, value: &ArcStr) -> bool {
    match upper {
        Bound::Unbounded => true,
        Bound::Included(s) => value <= s,
        Bound::Excluded(s) => value < s,
    }
}

fn number(n: u64) -> String {
    if n == 0 {
        "0+".to_owned()
    } else {
        format!("0*{n}")
    }
}

fn qualifier(tag: &str, n: u64) -> String {
    let digits = if n == 0 { "0*".to_owned() } else { number(n) };
    format!("[-._]?(?:{tag})[-._]?{digits}")
}

/// All spellings of an exact version, optionally including all local labels as
/// required by a PEP 440 equality against a public version.
fn spellings(v: &Version, any_local: bool) -> String {
    let ws = r"[\x09\x0a\x0c\x0d ]*";
    let mut re = format!(r"(?i-u:\A{ws}v?");
    if v.epoch() == 0 {
        re.push_str("(?:0+!)?");
    } else {
        write!(re, "{}!", number(v.epoch())).unwrap();
    }
    let trimmed = v.clone().without_trailing_zeros();
    for (i, &n) in trimmed.release().iter().enumerate() {
        if i > 0 {
            re.push_str(r"\.");
        }
        re.push_str(&number(n));
    }
    re.push_str(r"(?:\.0+)*");
    if let Some(pre) = v.pre() {
        let tag = match pre.kind {
            PrereleaseKind::Alpha => "alpha|a",
            PrereleaseKind::Beta => "beta|b",
            PrereleaseKind::Rc => "preview|pre|rc|c",
        };
        re.push_str(&qualifier(tag, pre.number));
    }
    if let Some(post) = v.post() {
        write!(
            re,
            "(?:-{}|{})",
            number(post),
            qualifier("post|rev|r", post)
        )
        .unwrap();
    }
    if let Some(dev) = v.dev() {
        re.push_str(&qualifier("dev", dev));
    }
    if any_local {
        re.push_str(r"(?:\+[a-z0-9]+(?:[-._][a-z0-9]+)*)?");
    } else if let LocalVersionSlice::Segments(segments) = v.local() {
        for (i, seg) in segments.iter().enumerate() {
            re.push_str(if i == 0 { r"\+" } else { "[-._]" });
            match seg {
                LocalSegment::Number(n) => re.push_str(&number(*n)),
                LocalSegment::String(s) => re.push_str(s),
            }
        }
    }
    write!(re, "{ws}\\z)").unwrap();
    re
}

/// A lexical bound can be forgotten once a prefix lies strictly inside it.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum Position {
    Inside,
    EqualPrefix(usize),
}

impl Position {
    fn start(bound: Bound<&ArcStr>) -> Self {
        if matches!(bound, Bound::Unbounded) {
            Self::Inside
        } else {
            Self::EqualPrefix(0)
        }
    }

    fn advance(self, bound: Bound<&ArcStr>, byte: u8, lower: bool) -> Option<Self> {
        let Self::EqualPrefix(i) = self else {
            return Some(self);
        };
        let (Bound::Included(s) | Bound::Excluded(s)) = bound else {
            unreachable!()
        };
        match s.as_bytes().get(i) {
            Some(&b) if b == byte => Some(Self::EqualPrefix(i + 1)),
            Some(&b) if (byte > b) == lower => Some(Self::Inside),
            None if lower => Some(Self::Inside),
            _ => None,
        }
    }

    fn accepts_end(self, bound: Bound<&ArcStr>, lower: bool) -> bool {
        let Self::EqualPrefix(i) = self else {
            return true;
        };
        match bound {
            Bound::Included(s) if i == s.len() => true,
            Bound::Included(s) | Bound::Excluded(s) => !lower && i < s.len(),
            Bound::Unbounded => unreachable!(),
        }
    }
}

/// Breadth-first search through the product of the spelling DFA and the two
/// lexical bounds. Exhausting it proves emptiness. Record parent links to
/// recover the shortest witness without enumerating all strings of a length.
fn search(dfa: &dense::DFA<Vec<u32>>, low: Bound<&ArcStr>, high: Bound<&ArcStr>) -> Feasibility {
    type State = (StateID, Position, Position);
    let initial: State = (
        dfa.start_state_forward(&Input::new(b"")).unwrap(),
        Position::start(low),
        Position::start(high),
    );
    let mut nodes = vec![(initial, None::<(usize, u8)>)];
    let mut seen = FxHashSet::default();
    seen.insert(initial);
    let mut i = 0;
    while i < nodes.len() {
        let ((state, l, h), _) = nodes[i];
        if dfa.is_match_state(dfa.next_eoi_state(state))
            && l.accepts_end(low, true)
            && h.accepts_end(high, false)
        {
            let mut bytes = Vec::new();
            let mut cursor = i;
            while let Some((previous, byte)) = nodes[cursor].1 {
                bytes.push(byte);
                cursor = previous;
            }
            bytes.reverse();
            return Feasibility::Witness(
                String::from_utf8(bytes)
                    .expect("version spellings are ASCII")
                    .into(),
            );
        }
        for byte in 0..=127 {
            let next = dfa.next_state(state, byte);
            if dfa.is_dead_state(next) {
                continue;
            }
            let Some(l) = l.advance(low, byte, true) else {
                continue;
            };
            let Some(h) = h.advance(high, byte, false) else {
                continue;
            };
            let key = (next, l, h);
            if seen.insert(key) {
                // Bound work in the prototype. Hitting the limit cannot prove emptiness.
                if nodes.len() >= 16384 {
                    return Feasibility::Unknown;
                }
                nodes.push((key, Some((i, byte))));
            }
        }
        i += 1;
    }
    Feasibility::Empty
}

#[cfg(test)]
mod tests {
    use super::{Feasibility, PlatformMarkerFeasibility, check};
    use crate::{MarkerEnvironment, MarkerEnvironmentBuilder, MarkerOperator, MarkerTree};
    use arcstr::ArcStr;
    use version_ranges::Ranges;

    fn environment() -> MarkerEnvironment {
        MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
            implementation_name: "cpython",
            implementation_version: "3.12",
            os_name: "posix",
            platform_machine: "arm64",
            platform_python_implementation: "CPython",
            platform_release: "",
            platform_system: "Darwin",
            platform_version: "",
            python_full_version: "3.12",
            python_version: "3.12",
            sys_platform: "darwin",
        })
        .unwrap()
    }

    #[test]
    fn platform_feasibility_follows_the_entire_fork() -> Result<(), Box<dyn std::error::Error>> {
        let raw: MarkerTree = "platform_version > '2.q' and platform_version < '2.s'".parse()?;
        let empty = raw.and("platform_version == '1'".parse()?);
        let unknown = raw.and("platform_version > '7'".parse()?);
        let witness = raw.and("platform_version == '2rc1'".parse()?);
        let release: MarkerTree =
            "platform_release == '1' and platform_release > '2.q' and platform_release < '2.s'"
                .parse()?;
        let python: MarkerTree = "python_version >= '3.12'".parse()?;
        assert_eq!(
            empty.or(release).platform_feasibility(),
            PlatformMarkerFeasibility::Empty
        );
        assert_eq!(
            unknown.and(release).platform_feasibility(),
            PlatformMarkerFeasibility::Empty
        );
        assert_eq!(
            unknown.or(witness.and(python)).platform_feasibility(),
            PlatformMarkerFeasibility::Possible
        );
        assert_eq!(
            empty.or(unknown).platform_feasibility(),
            PlatformMarkerFeasibility::Unknown(crate::CanonicalMarkerValueString::PlatformVersion)
        );
        assert_eq!(
            witness
                .and("'never' in platform_version".parse()?)
                .platform_feasibility(),
            PlatformMarkerFeasibility::Unknown(crate::CanonicalMarkerValueString::PlatformVersion)
        );
        Ok(())
    }

    #[test]
    fn joint_witness_satisfies_both_input_markers() {
        let env = environment();
        let probes = [
            "1",
            "01",
            "V1",
            "2.rc1",
            "2.preview",
            "2.r",
            "2.post1",
            "1.0RC_2.post3.DEV",
            "kernel",
            "2.q",
            "2.s",
            "1.0+foo-001",
            "1!01.0a",
            "2.rc1+local",
        ];
        let mut outcomes = [0; 3];
        for key in ["platform_version", "platform_release"] {
            for literal in [
                "1",
                "2rc1",
                "2.post1",
                "1rc2.post3.dev0",
                "kernel",
                "1.0+foo.1",
                "1!1a0",
                "7",
            ] {
                for (op, symbol) in [
                    (MarkerOperator::Equal, "=="),
                    (MarkerOperator::NotEqual, "!="),
                    (MarkerOperator::GreaterThan, ">"),
                ] {
                    let Some(typed) = super::super::range(op, &literal.into()) else {
                        continue;
                    };
                    let numeric: MarkerTree =
                        format!("{key} {symbol} '{literal}'").parse().unwrap();
                    for (lo, hi) in [
                        ("2.q", "2.s"),
                        ("2.q", "2.suffix"),
                        ("1.0Q", "1.0S"),
                        ("kernel", "kernelz"),
                        ("U", "w"),
                        ("1!", "1!x"),
                        ("1.0+", "1.0~"),
                    ] {
                        let raw = Ranges::strictly_higher_than(ArcStr::from(lo))
                            .intersection(&Ranges::strictly_lower_than(ArcStr::from(hi)));
                        let lexical: MarkerTree = format!("{key} > '{lo}' and {key} < '{hi}'")
                            .parse()
                            .unwrap();
                        let matches = |s: &str| {
                            let env = env
                                .clone()
                                .with_platform_release(s)
                                .with_platform_version(s);
                            numeric.evaluate(&env, &[]) && lexical.evaluate(&env, &[])
                        };
                        let result = check(&raw, &typed);
                        match &result {
                            Feasibility::Witness(s) => {
                                outcomes[0] += 1;
                                assert!(matches(s), "{key} {symbol} {literal}, {lo}..{hi}: {s:?}");
                            }
                            Feasibility::Empty => {
                                outcomes[1] += 1;
                                assert!(
                                    !probes.iter().any(|s| matches(s)),
                                    "{literal}, {lo}..{hi}"
                                );
                            }
                            Feasibility::Unknown => outcomes[2] += 1,
                        }
                        if op == MarkerOperator::GreaterThan
                            && literal == "7"
                            && lo == "2.q"
                            && hi == "2.s"
                        {
                            assert!(matches!(result, Feasibility::Unknown));
                        }
                        if op == MarkerOperator::Equal && lo == "2.q" && hi == "2.s" {
                            if literal == "1" {
                                assert_eq!(
                                    numeric.and(lexical).platform_feasibility(),
                                    PlatformMarkerFeasibility::Empty
                                );
                                assert!(matches!(result, Feasibility::Empty));
                            }
                            if literal == "2rc1" {
                                assert_eq!(
                                    numeric.and(lexical).platform_feasibility(),
                                    PlatformMarkerFeasibility::Possible
                                );
                                assert!(matches!(result, Feasibility::Witness(_)));
                            }
                        }
                    }
                }
            }
        }
        assert!(outcomes.into_iter().all(|n| n > 0));
    }
}
