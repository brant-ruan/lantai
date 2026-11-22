//! Splitting a long slice into parts at turn boundaries. Pure logic.

use std::ops::Range;

/// Greedily pack consecutive turns into parts of at most `budget` characters.
///
/// `sizes` are the lengths of the turns in order. Packing starts from the first turn,
/// so when turns are appended to a session only the last part changes and the earlier
/// parts keep their boundaries (and their cached digests). A single turn larger than
/// the budget gets a part of its own.
pub fn plan_chunks(sizes: &[usize], budget: usize) -> Vec<Range<usize>> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut used = 0;
    for (i, &size) in sizes.iter().enumerate() {
        if i > start && used + size > budget {
            parts.push(start..i);
            start = i;
            used = 0;
        }
        used += size;
    }
    if start < sizes.len() {
        parts.push(start..sizes.len());
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_turns_up_to_the_budget() {
        assert_eq!(
            plan_chunks(&[40, 40, 40, 40, 40], 100),
            vec![0..2, 2..4, 4..5]
        );
        assert_eq!(plan_chunks(&[10, 20, 30], 100), vec![0..3]);
        assert!(plan_chunks(&[], 100).is_empty());
    }

    #[test]
    fn oversized_turn_gets_its_own_part() {
        assert_eq!(plan_chunks(&[30, 500, 30, 30], 100), vec![0..1, 1..2, 2..4]);
        assert_eq!(plan_chunks(&[500], 100), vec![0..1]);
    }

    #[test]
    fn appending_turns_keeps_earlier_boundaries() {
        let before = plan_chunks(&[40, 40, 40, 40, 40], 100);
        let after = plan_chunks(&[40, 40, 40, 40, 40, 40, 40], 100);
        assert_eq!(before[..2], after[..2]);
        assert_eq!(after, vec![0..2, 2..4, 4..6, 6..7]);
    }
}
