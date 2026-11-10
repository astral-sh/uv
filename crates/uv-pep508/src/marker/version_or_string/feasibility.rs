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
use crate::MarkerOperator;

#[derive(Debug)]
pub(in crate::marker) enum Feasibility {
    Empty,
    Witness(ArcStr),
    Unknown,
}

/// Reparse the concrete spelling before accepting a witness, using the same
/// raw value for both predicates. An inconclusive search never prunes a fork.
pub(in crate::marker) fn check(
    raw: &Ranges<ArcStr>,
    typed: &Ranges<VersionOrStringValue>,
) -> Feasibility {
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
    use super::{Feasibility, check};
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
                                assert!(numeric.is_disjoint(lexical));
                                assert!(matches!(result, Feasibility::Empty));
                            }
                            if literal == "2rc1" {
                                assert!(!numeric.is_disjoint(lexical));
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
