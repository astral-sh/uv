//! Lexicographic cuts in the language of strings accepted by the version parser.
use std::sync::LazyLock;

use arcstr::ArcStr;
use regex_automata::Input;
use regex_automata::dfa::{Automaton, dense};

/// All opaque literals in a gap between version spellings give the same cut.
/// The opaque part of a marker still compares with the user's original literal.
#[derive(Debug, Eq, PartialEq)]
pub(in crate::marker) enum Cut {
    BeforeAll,
    Before(ArcStr),
    AfterAll,
}

/// PEP 440 spelling syntax, including the aliases accepted by uv-pep440.
static SPELLINGS: LazyLock<dense::DFA<Vec<u32>>> = LazyLock::new(|| {
    dense::DFA::new(concat!(
        r"(?i-u:\A[\x09\x0a\x0c\x0d ]*v?(?:[0-9]+!)?[0-9]+(?:\.[0-9]+)*",
        r"(?:[-._]?(?:alpha|beta|preview|pre|rc|a|b|c)[-._]?[0-9]*)?",
        r"(?:-[0-9]+|[-._]?(?:post|rev|r)[-._]?[0-9]*)?",
        r"(?:[-._]?dev[-._]?[0-9]*)?(?:\+[a-z0-9]+(?:[-._][a-z0-9]+)*)?",
        r"[\x09\x0a\x0c\x0d ]*\z)"
    ))
    .expect("valid version spelling grammar")
});

#[derive(Clone, Copy)]
struct Prefix {
    state: regex_automata::util::primitives::StateID,
    /// Outside a local label, uv reserves `u64::MAX` in numeric components.
    number: Option<u64>,
}

impl Prefix {
    fn extend(self, byte: u8) -> Option<Self> {
        let state = SPELLINGS.next_state(self.state, byte);
        if SPELLINGS.is_dead_state(state) {
            return None;
        }
        let number = match self.number {
            None => None,
            Some(_) if byte == b'+' => None,
            Some(n) if byte.is_ascii_digit() => {
                let next = n.checked_mul(10)?.checked_add(u64::from(byte - b'0'))?;
                if next == u64::MAX {
                    return None;
                }
                Some(next)
            }
            Some(_) => Some(0),
        };
        Some(Self { state, number })
    }
}

/// Find the deepest prefix that has accepted spellings on both sides. Choose a
/// representative immediately after its last lower child. All continuations of
/// that child sort before `~`, which is outside the version alphabet.
pub(in crate::marker) fn cut(literal: &str) -> Cut {
    let dfa = &*SPELLINGS;
    let mut prefix = Prefix {
        state: dfa.start_state_forward(&Input::new(b"")).unwrap(),
        number: Some(0),
    };
    let mut path = Vec::new();
    for byte in literal.bytes() {
        path.push((prefix, byte));
        let Some(next) = prefix.extend(byte) else {
            break;
        };
        prefix = next;
    }
    let mut subtree_below = false;
    for (i, (prefix, byte)) in path.into_iter().enumerate().rev() {
        let split = usize::from(byte) + usize::from(subtree_below);
        let live = |b: &u8| prefix.extend(*b).is_some();
        let last = (0..=127u8).take(split).rev().find(live);
        let below = last.is_some() || dfa.is_match_state(dfa.next_eoi_state(prefix.state));
        let above = (0..=127u8).skip(split).any(|b| live(&b));
        if below && above {
            let mut boundary = literal[..i].to_owned();
            if let Some(last) = last {
                boundary.push(char::from(last));
                boundary.push('~');
            } else {
                // Only the complete prefix is below the cut.
                boundary.push('\0');
            }
            return Cut::Before(boundary.into());
        }
        subtree_below = below;
    }
    if subtree_below {
        Cut::AfterAll
    } else {
        Cut::BeforeAll
    }
}

#[cfg(test)]
mod tests {
    use uv_pep440::Version;

    use super::{Cut, Prefix, SPELLINGS, cut};
    use regex_automata::{Input, dfa::Automaton};

    fn recognizes(s: &str) -> bool {
        let dfa = &*SPELLINGS;
        let start = Prefix {
            state: dfa.start_state_forward(&Input::new(b"")).unwrap(),
            number: Some(0),
        };
        s.bytes()
            .try_fold(start, Prefix::extend)
            .is_some_and(|end| dfa.is_match_state(dfa.next_eoi_state(end.state)))
    }

    #[test]
    fn agrees_with_version_parser() {
        for release in [
            "",
            "V001",
            "1!02.0",
            "1.",
            "18446744073709551614",
            "18446744073709551615",
            "00000000000000000000001",
        ] {
            for pre in [
                "", "Alpha0", "a", "_BETA-", "pre", "preview", "rc_01", "c", "0unknown",
            ] {
                for post in [
                    "",
                    "-012",
                    "r",
                    ".rev",
                    "POST-1",
                    "post18446744073709551615",
                ] {
                    for tail in [
                        "",
                        "_Dev",
                        ".dev01",
                        "+az-01",
                        "+18446744073709551615",
                        "+",
                        ".*",
                        " \t",
                        "\u{b}",
                    ] {
                        let s = format!("\t{release}{pre}{post}{tail}");
                        assert_eq!(recognizes(&s), s.parse::<Version>().is_ok(), "{s:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn cuts_preserve_order_of_actual_spellings() {
        for (low, high) in [
            ("kernel", "kernelz"),
            ("kernel", "linux"),
            ("1.g", "1.o"),
            ("1+@", "1+@suffix"),
            ("1.dev18446744073709551615", "1.dev18446744073709551615a"),
        ] {
            assert_eq!(cut(low), cut(high), "{low:?}..{high:?}");
        }
        assert_ne!(cut("1.q"), cut("1.s")); // 1.rc0
        assert_ne!(cut("u"), cut("w")); // v1
        for bound in [
            "",
            "0!",
            "v",
            "1.Po",
            "1+@",
            "1\x01",
            "1.dev18446744073709551615",
            "1.é",
            "kernel",
            "~",
        ] {
            let cut = cut(bound);
            if let Cut::Before(s) = &cut {
                assert_eq!(super::cut(s), cut);
                assert!(s.parse::<Version>().is_err());
            }
            for version in [
                "\t1",
                "0!1",
                "01",
                "1",
                "1\t",
                "1.post1",
                "1.preview",
                "1+0",
                "1+abc",
                "1.dev18446744073709551614",
                "V2",
                "v1",
            ] {
                assert_eq!(
                    version < bound,
                    match &cut {
                        Cut::BeforeAll => false,
                        Cut::Before(s) => version < s.as_str(),
                        Cut::AfterAll => true,
                    },
                    "{version:?} against {bound:?}"
                );
            }
        }
    }
}
