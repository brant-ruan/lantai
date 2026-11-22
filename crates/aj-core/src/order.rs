//! Fractional ordering keys: strings that sort between any two existing keys, so an item can be
//! placed between neighbors by writing only its own key. Pure logic.
//!
//! Keys use the digits `0-9A-Za-z` in ASCII order (which is also SQLite's default text order) and
//! never end in `0`, so there is always room between two distinct keys.

const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

fn digit(c: u8) -> usize {
    DIGITS
        .iter()
        .position(|&d| d == c)
        .expect("ordering keys only contain base-62 digits")
}

/// A key strictly between `before` and `after` (either end open). Panics when `before >= after`
/// or a key is not a valid ordering key, which only a caller's bug produces.
pub fn key_between(before: Option<&str>, after: Option<&str>) -> String {
    let a = before.unwrap_or("");
    if let Some(b) = after {
        assert!(a < b, "ordering keys out of order: {a:?} >= {b:?}");
    }
    assert!(!a.ends_with('0') && !after.is_some_and(|b| b.ends_with('0')));
    match (a.is_empty(), after) {
        // Appending and prepending are the common cases: step one digit at a time so keys
        // grow by one character only every few dozen items.
        (false, None) => key_after(a.as_bytes()),
        (true, Some(b)) => key_before(b.as_bytes()),
        _ => midpoint(a.as_bytes(), after.map(str::as_bytes)),
    }
}

fn key_after(a: &[u8]) -> String {
    match a.split_first() {
        None => midpoint(&[], None),
        Some((&c, _)) if digit(c) + 1 < DIGITS.len() => (DIGITS[digit(c) + 1] as char).to_string(),
        Some((&c, rest)) => format!("{}{}", c as char, key_after(rest)),
    }
}

fn key_before(b: &[u8]) -> String {
    let (&c, rest) = b.split_first().expect("a non-empty key");
    match digit(c) {
        d if d >= 2 => (DIGITS[d - 1] as char).to_string(),
        // "1..." -> "0" followed by a fresh key; "0..." -> keep the zero and go deeper.
        1 => format!("0{}", midpoint(&[], None)),
        _ => format!("0{}", key_before(rest)),
    }
}

/// `n` keys in order, for laying out a list from scratch.
pub fn keys_after(before: Option<&str>, n: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(n);
    for _ in 0..n {
        let prev = out.last().map(String::as_str).or(before);
        out.push(key_between(prev, None));
    }
    out
}

fn midpoint(a: &[u8], b: Option<&[u8]>) -> String {
    if let Some(b) = b {
        // Shared prefix (with `a` padded by zeros): keep it and recurse on the rest.
        let mut n = 0;
        while n < b.len() && a.get(n).copied().unwrap_or(DIGITS[0]) == b[n] {
            n += 1;
        }
        if n > 0 {
            let prefix = String::from_utf8(b[..n].to_vec()).expect("ascii");
            let rest_a = if n < a.len() { &a[n..] } else { &[][..] };
            return prefix + &midpoint(rest_a, Some(&b[n..]));
        }
    }
    let da = a.first().map_or(0, |&c| digit(c));
    let db = b.map_or(DIGITS.len(), |b| digit(b[0]));
    if db - da > 1 {
        let mid = (da + db).div_ceil(2);
        return (DIGITS[mid] as char).to_string();
    }
    // Adjacent digits: take `b`'s first digit if `b` goes on, or extend `a`.
    if let Some(b) = b.filter(|b| b.len() > 1) {
        return (b[0] as char).to_string();
    }
    let rest_a = if a.is_empty() { &[][..] } else { &a[1..] };
    format!("{}{}", DIGITS[da] as char, midpoint(rest_a, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_sort_between_their_neighbors() {
        let first = key_between(None, None);
        let after = key_between(Some(&first), None);
        let before = key_between(None, Some(&first));
        let mid = key_between(Some(&first), Some(&after));
        assert!(before < first && first < mid && mid < after);
    }

    #[test]
    fn repeated_insertion_at_one_spot_keeps_order() {
        let mut keys = keys_after(None, 3);
        // Keep inserting right after the first key, and right before the last.
        for _ in 0..200 {
            let k = key_between(Some(&keys[0]), Some(&keys[1]));
            keys.insert(1, k);
            let n = keys.len();
            let k = key_between(Some(&keys[n - 2]), Some(&keys[n - 1]));
            keys.insert(n - 1, k);
        }
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len());
        assert!(keys.iter().all(|k| !k.ends_with('0')));
    }

    #[test]
    fn prepending_and_appending_many_times_stays_ordered() {
        let mut keys = vec![key_between(None, None)];
        for _ in 0..300 {
            keys.insert(0, key_between(None, Some(&keys[0])));
            keys.push(key_between(keys.last().map(String::as_str), None));
        }
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        assert!(keys.iter().map(String::len).max().unwrap() <= 12);
    }
}
