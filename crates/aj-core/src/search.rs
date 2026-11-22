//! Session search: split a query into terms, keep the turns and slices that contain every term,
//! and cut highlighted snippets around the matches.
//!
//! Term handling and snippets are pure functions; [`search_sessions`] adds the store queries.
//! Matching is ASCII case-insensitive, the same as SQLite `LIKE`, so byte offsets stay valid.

use std::collections::HashMap;

use aj_store::{like_pattern, SearchField, SliceFilter, SliceRef, Store};
use anyhow::Result;

/// Characters of context kept before the first match of a snippet.
const SNIPPET_LEAD: usize = 40;
/// Maximum characters in one snippet.
const SNIPPET_LEN: usize = 160;
/// Snippets kept per turn or per slice title.
const SNIPPETS_PER_HIT: usize = 3;

/// Split a query into lowercase-ASCII terms. Whitespace separates terms; `"double quotes"`
/// keep a phrase together. Duplicates are dropped.
pub fn parse_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    let mut push = |t: &str| {
        let t = t.trim().to_ascii_lowercase();
        if !t.is_empty() && !terms.contains(&t) {
            terms.push(t);
        }
    };
    for (i, part) in query.split('"').enumerate() {
        if i % 2 == 1 {
            push(part);
        } else {
            part.split_whitespace().for_each(&mut push);
        }
    }
    terms
}

/// The term the store scans for: the longest one, since it is usually the most selective.
pub fn scan_term(terms: &[String]) -> Option<&str> {
    terms
        .iter()
        .max_by_key(|t| t.chars().count())
        .map(String::as_str)
}

/// Byte ranges of every non-overlapping match of `term` (already lowercase) in `text`.
pub fn find_all(text: &str, term: &str) -> Vec<(usize, usize)> {
    if term.is_empty() {
        return Vec::new();
    }
    let hay = text.as_bytes();
    let needle = term.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()].eq_ignore_ascii_case(needle) && text.is_char_boundary(i) {
            out.push((i, i + needle.len()));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

pub fn contains(text: &str, term: &str) -> bool {
    !find_all(text, term).is_empty()
}

/// True when every term occurs in at least one of the texts.
pub fn covers_all<'a>(texts: impl Iterator<Item = &'a str> + Clone, terms: &[String]) -> bool {
    terms
        .iter()
        .all(|t| texts.clone().any(|text| contains(text, t)))
}

/// Total matches of all terms in `text`.
pub fn count_matches(text: &str, terms: &[String]) -> usize {
    terms.iter().map(|t| find_all(text, t).len()).sum()
}

/// A run of snippet text, highlighted when it is a match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetPart {
    pub text: String,
    pub hit: bool,
}

/// Merged, sorted match ranges of all terms.
fn match_ranges(text: &str, terms: &[String]) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = terms.iter().flat_map(|t| find_all(text, t)).collect();
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in ranges {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    merged
}

/// Byte offset `n` characters before `at` (or 0).
fn back_chars(text: &str, at: usize, n: usize) -> usize {
    text[..at]
        .char_indices()
        .rev()
        .nth(n.saturating_sub(1))
        .map_or(0, |(i, _)| i)
}

/// Byte offset `n` characters after `at` (or the end).
fn forward_chars(text: &str, at: usize, n: usize) -> usize {
    text[at..]
        .char_indices()
        .nth(n)
        .map_or(text.len(), |(i, _)| at + i)
}

/// Collapse whitespace runs (newlines included) into single spaces.
fn squash(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            space = false;
            out.push(c);
        }
    }
    out
}

/// A window of `text` around its first match, split into plain and highlighted parts, with
/// `…` where it was cut. `None` when nothing matches.
pub fn snippet(text: &str, terms: &[String]) -> Option<Vec<SnippetPart>> {
    let ranges = match_ranges(text, terms);
    let first = ranges.first()?;
    let start = back_chars(text, first.0, SNIPPET_LEAD);
    let end = forward_chars(text, start, SNIPPET_LEN);
    let mut parts: Vec<SnippetPart> = Vec::new();
    let mut push = |s: &str, hit: bool| {
        let s = squash(s);
        if s.is_empty() {
            return;
        }
        match parts.last_mut() {
            Some(last) if last.hit == hit => last.text.push_str(&s),
            _ => parts.push(SnippetPart { text: s, hit }),
        }
    };
    if start > 0 {
        push("…", false);
    }
    let mut at = start;
    for &(s, e) in ranges.iter().filter(|r| r.0 < end) {
        let e = e.min(end);
        if s > at {
            push(&text[at..s], false);
        }
        push(&text[s.max(at)..e], true);
        at = e;
    }
    if at < end {
        push(&text[at..end], false);
    }
    if end < text.len() {
        push("…", false);
    }
    if let Some(p) = parts.first_mut() {
        p.text = p.text.trim_start().to_string();
    }
    Some(parts)
}

/// A snippet labelled with the text it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSnippet {
    pub field: SearchField,
    pub parts: Vec<SnippetPart>,
}

/// Snippets for the texts that match, in text order, at most [`SNIPPETS_PER_HIT`].
fn field_snippets(texts: &[(SearchField, String)], terms: &[String]) -> Vec<FieldSnippet> {
    texts
        .iter()
        .filter_map(|(field, text)| {
            snippet(text, terms).map(|parts| FieldSnippet {
                field: *field,
                parts,
            })
        })
        .take(SNIPPETS_PER_HIT)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnHit {
    pub turn_index: i64,
    pub started_at: String,
    pub snippets: Vec<FieldSnippet>,
    pub match_count: usize,
}

/// The matches inside one slice: its title or note, and its turns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceHits {
    pub slice: SliceRef,
    /// Snippets from the session title or note, when they contain every term.
    pub title_snippets: Vec<FieldSnippet>,
    pub turns: Vec<TurnHit>,
    pub match_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionSearch {
    pub slices: Vec<SliceHits>,
    /// The scan stopped at its limit; older matches may be missing.
    pub truncated: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SessionQuery {
    pub terms: Vec<String>,
    pub filter: SliceFilter,
    pub include_tool_output: bool,
    /// Most candidate turns scanned before giving up on older ones.
    pub limit: usize,
}

/// Find turns (every term within one turn) and session titles or notes (every term within
/// them), grouped by slice: newest day first, then by number of matches.
pub fn search_sessions(store: &Store, q: &SessionQuery) -> Result<SessionSearch> {
    let Some(scan) = scan_term(&q.terms) else {
        return Ok(SessionSearch::default());
    };
    let pattern = like_pattern(scan);
    let ids = store.search_turn_ids(&q.filter, &pattern, q.include_tool_output, q.limit + 1)?;
    let truncated = ids.len() > q.limit;
    let ids = &ids[..ids.len().min(q.limit)];

    let mut groups: Vec<SliceHits> = Vec::new();
    let mut by_slice: HashMap<i64, usize> = HashMap::new();
    let mut group_of = |slice: &SliceRef, groups: &mut Vec<SliceHits>| -> usize {
        *by_slice.entry(slice.slice_id).or_insert_with(|| {
            groups.push(SliceHits {
                slice: slice.clone(),
                title_snippets: Vec::new(),
                turns: Vec::new(),
                match_count: 0,
            });
            groups.len() - 1
        })
    };

    for row in store.search_turn_rows(ids, q.include_tool_output)? {
        if !covers_all(row.texts.iter().map(|(_, t)| t.as_str()), &q.terms) {
            continue;
        }
        let match_count = row
            .texts
            .iter()
            .map(|(_, t)| count_matches(t, &q.terms))
            .sum();
        let g = group_of(&row.slice, &mut groups);
        groups[g].match_count += match_count;
        groups[g].turns.push(TurnHit {
            turn_index: row.turn_index,
            started_at: row.started_at,
            snippets: field_snippets(&row.texts, &q.terms),
            match_count,
        });
    }

    for row in store.search_slice_titles(&q.filter, &pattern, q.limit)? {
        if !covers_all(row.texts.iter().map(|(_, t)| t.as_str()), &q.terms) {
            continue;
        }
        let g = group_of(&row.slice, &mut groups);
        groups[g].match_count += row
            .texts
            .iter()
            .map(|(_, t)| count_matches(t, &q.terms))
            .sum::<usize>();
        groups[g].title_snippets = field_snippets(&row.texts, &q.terms);
    }

    for g in &mut groups {
        g.turns.sort_by_key(|t| t.turn_index);
    }
    groups.sort_by(|a, b| {
        b.slice
            .local_date
            .cmp(&a.slice.local_date)
            .then(b.match_count.cmp(&a.match_count))
            .then(a.slice.slice_id.cmp(&b.slice.slice_id))
    });
    Ok(SessionSearch {
        slices: groups,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(q: &str) -> Vec<String> {
        parse_terms(q)
    }

    fn render(parts: &[SnippetPart]) -> String {
        parts
            .iter()
            .map(|p| {
                if p.hit {
                    format!("[{}]", p.text)
                } else {
                    p.text.clone()
                }
            })
            .collect()
    }

    #[test]
    fn terms_split_on_whitespace_and_keep_quoted_phrases() {
        assert_eq!(
            terms(r#"  Keychain "busy timeout" 日志 keychain "#),
            vec!["keychain", "busy timeout", "日志"]
        );
        assert!(terms("   ").is_empty());
        assert_eq!(scan_term(&terms("ab 数据库 x")), Some("数据库"));
    }

    #[test]
    fn matching_is_ascii_case_insensitive_and_handles_cjk() {
        assert_eq!(find_all("Rust rust RUST", "rust").len(), 3);
        assert_eq!(
            find_all("今天写日志，日志很长", "日志"),
            vec![(9, 15), (18, 24)]
        );
        assert!(covers_all(
            ["foo Bar", "baz"].into_iter(),
            &terms("bar baz")
        ));
        assert!(!covers_all(["foo"].into_iter(), &terms("foo qux")));
    }

    #[test]
    fn snippet_highlights_and_trims_long_text() {
        let text = format!(
            "{}needle in\nthe   haystack{}",
            "a".repeat(100),
            "b".repeat(300)
        );
        let parts = snippet(&text, &terms("NEEDLE haystack")).unwrap();
        let shown = render(&parts);
        assert!(shown.starts_with('…'));
        assert!(shown.ends_with('…'));
        assert!(shown.contains("[needle] in the [haystack]"));
        assert!(shown.chars().count() <= SNIPPET_LEN + 4);
    }

    fn store_with_two_days() -> Store {
        use crate::{slice::reslice_session, Config};
        use aj_ir::{AgentKind, IrEvent, ToolKind};
        use chrono::{FixedOffset, Utc};

        let t = |s: &str| s.parse::<chrono::DateTime<Utc>>().unwrap();
        let tz = FixedOffset::east_opt(0).unwrap();
        let mut store = Store::open_in_memory().unwrap();
        let events = vec![
            IrEvent::SessionMeta {
                cwd: Some("/work/app".into()),
                git_branch: None,
                agent_version: None,
                parent_session_id: None,
            },
            IrEvent::TurnStart {
                source_event_id: "u1".into(),
                timestamp: t("2026-11-16T10:00:00Z"),
                user_prompt: "store the API key in the Keychain".into(),
            },
            IrEvent::ToolAction {
                source_event_id: "b1".into(),
                timestamp: t("2026-11-16T10:00:10Z"),
                kind: ToolKind::BashCmd,
                tool_name: "Bash".into(),
                target: Some("security add-generic-password".into()),
                summary: None,
                diff_stat: None,
            },
            IrEvent::ToolOutcome {
                tool_source_id: "b1".into(),
                is_error: true,
                diff_stat: None,
                output_summary: Some("keychain locked".into()),
            },
            IrEvent::AgentMessage {
                source_event_id: "m1".into(),
                timestamp: t("2026-11-16T10:01:00Z"),
                text: "saved to the keychain via security".into(),
            },
            IrEvent::TurnStart {
                source_event_id: "u2".into(),
                timestamp: t("2026-11-17T10:00:00Z"),
                user_prompt: "now add search".into(),
            },
        ];
        let sid = store
            .ingest(AgentKind::Claude, "s", &events)
            .unwrap()
            .session_id;
        reslice_session(&mut store, sid, &Config::default(), &tz).unwrap();
        store
    }

    fn query(q: &str) -> SessionQuery {
        SessionQuery {
            terms: parse_terms(q),
            limit: 50,
            ..Default::default()
        }
    }

    #[test]
    fn sessions_require_every_term_within_one_turn() {
        let store = store_with_two_days();
        let res = search_sessions(&store, &query("keychain security")).unwrap();
        assert_eq!(res.slices.len(), 1);
        let hit = &res.slices[0];
        assert_eq!(hit.slice.local_date, "2026-11-16");
        assert_eq!(hit.turns.len(), 1);
        let fields: Vec<_> = hit.turns[0].snippets.iter().map(|s| s.field).collect();
        assert_eq!(
            fields,
            vec![SearchField::Prompt, SearchField::Tool, SearchField::Reply]
        );

        // Terms split across turns on different days do not match.
        assert!(search_sessions(&store, &query("keychain search"))
            .unwrap()
            .slices
            .is_empty());
    }

    #[test]
    fn tool_output_and_date_filter_are_honoured() {
        let store = store_with_two_days();
        assert!(search_sessions(&store, &query("locked"))
            .unwrap()
            .slices
            .is_empty());
        let mut q = query("locked");
        q.include_tool_output = true;
        assert_eq!(search_sessions(&store, &q).unwrap().slices.len(), 1);

        // On 11-17 only the session title (taken from the first prompt) still matches.
        let mut q = query("keychain");
        q.filter.since = Some("2026-11-17".into());
        let res = search_sessions(&store, &q).unwrap();
        assert_eq!(res.slices.len(), 1);
        assert!(res.slices[0].turns.is_empty());
        assert_eq!(res.slices[0].title_snippets[0].field, SearchField::Title);
    }

    #[test]
    fn snippet_merges_overlapping_terms_and_skips_misses() {
        let parts = snippet("abcdef", &terms("abc cde")).unwrap();
        assert_eq!(render(&parts), "[abcde]f");
        assert!(snippet("abcdef", &terms("xyz")).is_none());
    }
}
