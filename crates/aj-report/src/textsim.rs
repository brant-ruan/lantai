//! Rough similarity of two short lines of text. Pure logic.

/// Dice coefficient over character bigrams of the line content (bullet marker and
/// indentation removed). Works for CJK text, where words are not space-separated.
pub(crate) fn similarity(a: &str, b: &str) -> f64 {
    let grams = |s: &str| -> Vec<(char, char)> {
        let chars: Vec<char> = content(s).chars().collect();
        chars.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let (mut ga, gb) = (grams(a), grams(b));
    if ga.is_empty() || gb.is_empty() {
        return if content(a) == content(b) { 1.0 } else { 0.0 };
    }
    let total = ga.len() + gb.len();
    let mut common = 0;
    for g in &gb {
        if let Some(pos) = ga.iter().position(|x| x == g) {
            ga.swap_remove(pos);
            common += 1;
        }
    }
    2.0 * common as f64 / total as f64
}

pub(crate) fn content(line: &str) -> &str {
    let t = line.trim_start();
    t.strip_prefix("- ")
        .or_else(|| t.strip_prefix("* "))
        .unwrap_or(t)
        .trim()
}
