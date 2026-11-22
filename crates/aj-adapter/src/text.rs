//! Text helpers shared by adapters: tag stripping and truncation that keep char boundaries.

/// Remove every `<tag>…</tag>` block.
pub fn strip_tag_blocks(text: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(&open) {
        out.push_str(&rest[..start]);
        match rest[start..].find(&close) {
            Some(end) => rest = &rest[start + end + close.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Remove the `<tag>` and `</tag>` markers but keep what is between them.
pub fn unwrap_tag(text: &str, tag: &str) -> String {
    text.replace(&format!("<{tag}>"), "")
        .replace(&format!("</{tag}>"), "")
}

pub fn tag_content(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].to_string())
}

/// When too long, keep the head and tail and omit the middle.
pub fn truncate_middle(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars * 2 / 3).collect();
    let tail: String = text.chars().skip(count - max_chars / 3).collect();
    format!("{head}\n…[{} chars omitted]…\n{tail}", count - max_chars)
}

/// When too long, keep only the tail (the conclusion of command output is usually at the end).
pub fn truncate_tail(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let tail: String = text.chars().skip(count - max_chars).collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_keeps_char_boundaries() {
        let s = "\u{4e2d}".repeat(100);
        let t = truncate_middle(&s, 30);
        assert!(t.contains("[70 chars omitted]"));
        assert!(truncate_tail(&s, 10).chars().count() == 11);
    }

    #[test]
    fn strips_blocks_and_reads_tags() {
        assert_eq!(strip_tag_blocks("a<x>1</x>b<x>2", "x"), "ab");
        assert_eq!(tag_content("<n>/r</n>", "n").as_deref(), Some("/r"));
        assert_eq!(tag_content("<n>/r", "n"), None);
        assert_eq!(unwrap_tag("<q>yes</q> ok", "q"), "yes ok");
    }
}
