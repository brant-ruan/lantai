//! Source anchors: `[#S12]` (a slice) and `[#S12.T5]` (a turn inside a slice).
//!
//! Anchors written by the model are not trusted; they are verified against the set of IDs in this request's input.

use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

use regex::Regex;

/// Anything that looks like an anchor: `[#S<digits>` or `[S<digits>` followed by an arbitrary short tail.
fn anchor_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r" ?\[#?S(\d+)([^\]\n]{0,60})\]").expect("anchor regex"))
}

/// The tail of a well-formed anchor: empty, `.T5`, `.T5,T7`, or `.T5, S12.T7`.
fn turns_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\.T\d+(?:\s*,\s*(?:#?S\d+\.)?\.?T?\d+)*$").expect("turn list regex")
    })
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AnchorCheck {
    /// Number of slice or turn references found.
    pub total: usize,
    /// How many of them pointed at something that does not exist, or were malformed.
    pub invalid: usize,
}

/// Remove anchors that point to a slice or turn that does not exist, and rewrite merged
/// anchors into the canonical one-turn-per-anchor form. `valid` maps a slice ID to the
/// turn indexes it contains.
pub fn verify(markdown: &str, valid: &HashMap<i64, BTreeSet<i64>>) -> (String, AnchorCheck) {
    let mut check = AnchorCheck::default();
    let cleaned = anchor_re().replace_all(markdown, |caps: &regex::Captures| {
        let lead = if caps[0].starts_with(' ') { " " } else { "" };
        let tail = &caps[2];
        let Some(turns) = caps[1].parse::<i64>().ok().and_then(|id| valid.get(&id)) else {
            check.total += 1;
            check.invalid += 1;
            return String::new();
        };
        let slice = &caps[1];

        if tail.is_empty() {
            check.total += 1;
            return format!("{lead}[#S{slice}]");
        }
        if !turns_re().is_match(tail) {
            check.total += 1;
            check.invalid += 1;
            return String::new();
        }
        let mut kept = Vec::new();
        for part in tail[1..].split(',') {
            check.total += 1;
            let token = part.trim();
            let parsed_turn =
                if let Some(rest) = token.strip_prefix("#S").or_else(|| token.strip_prefix('S')) {
                    match rest.split_once('.') {
                        Some((part_slice, turn_part)) if part_slice == slice => {
                            turn_part.trim_start_matches('T').parse::<i64>().ok()
                        }
                        _ => None,
                    }
                } else {
                    token
                        .trim_start_matches('.')
                        .trim_start_matches('T')
                        .parse::<i64>()
                        .ok()
                };
            match parsed_turn {
                Some(t) if turns.contains(&t) => kept.push(format!("[#S{slice}.T{t}]")),
                _ => check.invalid += 1,
            }
        }
        if kept.is_empty() {
            String::new()
        } else {
            format!("{lead}{}", kept.join(" "))
        }
    });
    (cleaned.into_owned(), check)
}

/// A canonical anchor as stored after [`verify`]: `[#S12]` or `[#S12.T5]`, with its leading space.
fn canonical_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r" ?\[#S(\d+)(?:\.T(\d+))?\]").expect("canonical anchor regex"))
}

/// The slices and turns a stored report cites, in order of appearance.
pub fn references(markdown: &str) -> Vec<(i64, Option<i64>)> {
    canonical_re()
        .captures_iter(markdown)
        .filter_map(|c| {
            let slice = c[1].parse().ok()?;
            Some((slice, c.get(2).and_then(|t| t.as_str().parse().ok())))
        })
        .collect()
}

/// The text with every canonical anchor removed.
pub fn strip(markdown: &str) -> String {
    canonical_re().replace_all(markdown, "").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> HashMap<i64, BTreeSet<i64>> {
        HashMap::from([(12, BTreeSet::from([3, 5, 7]))])
    }

    #[test]
    fn references_and_strip_read_canonical_anchors() {
        let md = "- Did X [#S12.T5] [#S12.T7]\n- Did Y [#S3]";
        assert_eq!(
            references(md),
            vec![(12, Some(5)), (12, Some(7)), (3, None)]
        );
        assert_eq!(strip(md), "- Did X\n- Did Y");
    }

    #[test]
    fn strips_hallucinated_anchors_only() {
        let md = "- fix login [#S12.T5]\n- refactor cache [#S99]\n- add tests [#S12] [#S12.T8]";
        let (out, check) = verify(md, &valid());
        assert_eq!(
            out,
            "- fix login [#S12.T5]\n- refactor cache\n- add tests [#S12]"
        );
        assert_eq!(
            check,
            AnchorCheck {
                total: 4,
                invalid: 2
            }
        );
    }

    #[test]
    fn merged_anchors_are_checked_per_turn_and_rewritten() {
        let md = "- a [#S12.T3,T5]\n- b [#S12.T5, 9]\n- c [#S12.T8,T9]";
        let (out, check) = verify(md, &valid());
        assert_eq!(out, "- a [#S12.T3] [#S12.T5]\n- b [#S12.T5]\n- c");
        assert_eq!(
            check,
            AnchorCheck {
                total: 6,
                invalid: 3
            }
        );
    }

    #[test]
    fn malformed_anchors_are_removed_and_other_brackets_untouched() {
        let md = "- a [#S12-T5]\n- b [#S12.turn5]\n- see [#123] and [link](#S12)";
        let (out, check) = verify(md, &valid());
        assert_eq!(out, "- a\n- b\n- see [#123] and [link](#S12)");
        assert_eq!(
            check,
            AnchorCheck {
                total: 2,
                invalid: 2
            }
        );
    }

    #[test]
    fn anchors_without_hash_and_slice_repeated_merges_are_normalized() {
        let md = "- a [S12.T5] [S12] [S99.T1]\n- b [#S12.T3, S12.T5] [S12.T5, #S12.T7]";
        let (out, check) = verify(md, &valid());
        assert_eq!(
            out,
            "- a [#S12.T5] [#S12]\n- b [#S12.T3] [#S12.T5] [#S12.T5] [#S12.T7]"
        );
        assert_eq!(
            check,
            AnchorCheck {
                total: 7,
                invalid: 1
            }
        );
    }
}
