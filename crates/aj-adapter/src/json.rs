//! JSON helpers shared by adapters whose records are JSON.

use serde_json::Value;

/// The string at `key` of an object; `None` when it is missing or not a string.
pub fn str_of<'v>(v: &'v Value, key: &str) -> Option<&'v str> {
    v.get(key).and_then(Value::as_str)
}

/// Lines in a piece of file content, as counted for an edit's diff stat: the content a tool
/// call writes or replaces, not a real diff, so it is an estimate.
pub fn line_count(content: &str) -> u32 {
    content.lines().count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strings_and_line_counts() {
        let v = json!({"a": "x", "n": 1});
        assert_eq!(str_of(&v, "a"), Some("x"));
        assert_eq!(str_of(&v, "n"), None);
        assert_eq!(str_of(&v, "missing"), None);
        assert_eq!(line_count(""), 0);
        assert_eq!(line_count("one\ntwo\n"), 2);
        assert_eq!(line_count("one\ntwo"), 2);
    }
}
