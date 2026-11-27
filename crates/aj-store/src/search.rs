//! Plain-text search queries: candidate turns and slices whose text contains a needle, and
//! the current version of every global report.
//!
//! Matching here is a substring scan (`LIKE`, ASCII case-insensitive) on one needle; combining
//! several terms, snippets and ranking live in `aj-core` and `aj-report`.

use aj_ir::AgentId;
use anyhow::Result;
use rusqlite::{named_params, params_from_iter};

use crate::sql::{agent_at, SLICE_INCLUDED, SLICE_PROJECT};
use crate::Store;

/// Which slices a search covers. Dates are inclusive local dates (`YYYY-MM-DD`).
#[derive(Debug, Clone, Default)]
pub struct SliceFilter {
    pub since: Option<String>,
    pub until: Option<String>,
    pub project_id: Option<i64>,
    pub agent: Option<AgentId>,
    pub include_ignored: bool,
}

/// Which stored text a match came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SearchField {
    Prompt,
    Reply,
    Tool,
    ToolOutput,
    Title,
    Note,
}

impl SearchField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Reply => "reply",
            Self::Tool => "tool",
            Self::ToolOutput => "tool_output",
            Self::Title => "title",
            Self::Note => "note",
        }
    }
}

/// Where a slice sits, shared by turn and slice hits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceRef {
    pub slice_id: i64,
    pub local_date: String,
    pub session_id: i64,
    pub agent: AgentId,
    pub title: Option<String>,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub ignored: bool,
}

/// One candidate turn with every searchable text it holds, in item order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnSearchRow {
    pub slice: SliceRef,
    pub turn_id: i64,
    pub turn_index: i64,
    pub started_at: String,
    pub texts: Vec<(SearchField, String)>,
}

/// One candidate slice whose session title or note matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceSearchRow {
    pub slice: SliceRef,
    pub texts: Vec<(SearchField, String)>,
}

/// The current version of a global (not per-project) report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportTextRow {
    pub report_type: String,
    pub period_key: String,
    pub status: crate::ReportStatus,
    pub markdown: String,
}

/// A `LIKE` pattern that matches `needle` anywhere, with `\` as the escape character.
pub fn like_pattern(needle: &str) -> String {
    let mut out = String::with_capacity(needle.len() + 2);
    out.push('%');
    for c in needle.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

fn filter_sql() -> String {
    format!(
        "(:since IS NULL OR sds.local_date >= :since)
         AND (:until IS NULL OR sds.local_date <= :until)
         AND (:project IS NULL OR {SLICE_PROJECT} = :project)
         AND (:agent IS NULL OR s.agent = :agent)
         AND (:ignored != 0 OR {SLICE_INCLUDED})"
    )
}

const SLICE_REF_COLS: &str =
    "sds.id, sds.local_date, s.id, s.agent, COALESCE(s.title, s.fallback_title), \
     lp.id, lp.name";

fn slice_ref_from_row(r: &rusqlite::Row, ignored_col: usize) -> rusqlite::Result<SliceRef> {
    Ok(SliceRef {
        slice_id: r.get(0)?,
        local_date: r.get(1)?,
        session_id: r.get(2)?,
        agent: agent_at(r, 3)?,
        title: r.get(4)?,
        project_id: r.get(5)?,
        project_name: r.get(6)?,
        ignored: r.get::<_, i64>(ignored_col)? != 0,
    })
}

impl Store {
    /// IDs of turns inside the filtered slices whose prompt, reply or tool call contains the
    /// pattern (and tool output, when asked), newest day first, at most `limit`.
    pub fn search_turn_ids(
        &self,
        filter: &SliceFilter,
        pattern: &str,
        include_tool_output: bool,
        limit: usize,
    ) -> Result<Vec<i64>> {
        let sql = format!(
            r"WITH hit(turn_id) AS (
                SELECT id FROM turns WHERE user_prompt LIKE :pat ESCAPE '\'
                UNION
                SELECT turn_id FROM turn_items
                 WHERE text LIKE :pat ESCAPE '\'
                    OR target LIKE :pat ESCAPE '\'
                    OR summary LIKE :pat ESCAPE '\'
                    OR (:output != 0 AND output_summary LIKE :pat ESCAPE '\')
              )
              SELECT t.id
              FROM hit
              JOIN turns t ON t.id = hit.turn_id
              JOIN session_daily_slices sds ON sds.session_id = t.session_id
                   AND t.turn_index BETWEEN sds.start_turn_index AND sds.end_turn_index
              JOIN sessions s ON s.id = t.session_id
              WHERE {}
              ORDER BY sds.local_date DESC, sds.slice_start_at DESC, t.turn_index ASC
              LIMIT :limit",
            filter_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let ids = stmt
            .query_map(
                named_params! {
                    ":pat": pattern,
                    ":output": i32::from(include_tool_output),
                    ":since": filter.since,
                    ":until": filter.until,
                    ":project": filter.project_id,
                    ":agent": filter.agent.as_ref().map(AgentId::as_str),
                    ":ignored": i32::from(filter.include_ignored),
                    ":limit": limit as i64,
                },
                |r| r.get(0),
            )?
            .collect::<rusqlite::Result<Vec<i64>>>()?;
        Ok(ids)
    }

    /// Load the given turns with their slice context and searchable texts, in the order of `ids`.
    pub fn search_turn_rows(
        &self,
        ids: &[i64],
        include_tool_output: bool,
    ) -> Result<Vec<TurnSearchRow>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let marks = vec!["?"; ids.len()].join(",");
        let sql = format!(
            "SELECT {SLICE_REF_COLS}, CASE WHEN {SLICE_INCLUDED} THEN 0 ELSE 1 END,
                    t.id, t.turn_index, t.started_at, t.user_prompt
             FROM turns t
             JOIN session_daily_slices sds ON sds.session_id = t.session_id
                  AND t.turn_index BETWEEN sds.start_turn_index AND sds.end_turn_index
             JOIN sessions s ON s.id = t.session_id
             LEFT JOIN logical_projects lp ON lp.id = {SLICE_PROJECT}
             WHERE t.id IN ({marks})"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt
            .query_map(params_from_iter(ids), |r| {
                let prompt: String = r.get(11)?;
                Ok(TurnSearchRow {
                    slice: slice_ref_from_row(r, 7)?,
                    turn_id: r.get(8)?,
                    turn_index: r.get(9)?,
                    started_at: r.get(10)?,
                    texts: vec![(SearchField::Prompt, prompt)],
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let sql = format!(
            "SELECT turn_id, item_type, text, tool_name, target, summary, output_summary
             FROM turn_items WHERE turn_id IN ({marks}) ORDER BY turn_id, seq"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let items = stmt
            .query_map(params_from_iter(ids), |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let pos: std::collections::HashMap<i64, usize> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| (r.turn_id, i))
            .collect();
        for (turn_id, item_type, text, tool_name, target, summary, output) in items {
            let Some(&i) = pos.get(&turn_id) else {
                continue;
            };
            let texts = &mut rows[i].texts;
            if item_type == "message" {
                if let Some(t) = text.filter(|t| !t.is_empty()) {
                    texts.push((SearchField::Reply, t));
                }
                continue;
            }
            let call: Vec<String> = [tool_name, target, summary].into_iter().flatten().collect();
            if !call.is_empty() {
                texts.push((SearchField::Tool, call.join(" ")));
            }
            if include_tool_output {
                if let Some(o) = output.filter(|o| !o.is_empty()) {
                    texts.push((SearchField::ToolOutput, o));
                }
            }
        }

        let order: std::collections::HashMap<i64, usize> =
            ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        rows.sort_by_key(|r| order.get(&r.turn_id).copied().unwrap_or(usize::MAX));
        Ok(rows)
    }

    /// Filtered slices whose session title or note contains the pattern, newest day first.
    pub fn search_slice_titles(
        &self,
        filter: &SliceFilter,
        pattern: &str,
        limit: usize,
    ) -> Result<Vec<SliceSearchRow>> {
        let sql = format!(
            r"SELECT {SLICE_REF_COLS}, CASE WHEN {SLICE_INCLUDED} THEN 0 ELSE 1 END, s.user_note
              FROM session_daily_slices sds
              JOIN sessions s ON s.id = sds.session_id
              LEFT JOIN logical_projects lp ON lp.id = {SLICE_PROJECT}
              WHERE (COALESCE(s.title, s.fallback_title) LIKE :pat ESCAPE '\'
                     OR s.user_note LIKE :pat ESCAPE '\')
                AND {}
              ORDER BY sds.local_date DESC, sds.slice_start_at DESC
              LIMIT :limit",
            filter_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":pat": pattern,
                    ":since": filter.since,
                    ":until": filter.until,
                    ":project": filter.project_id,
                    ":agent": filter.agent.as_ref().map(AgentId::as_str),
                    ":ignored": i32::from(filter.include_ignored),
                    ":limit": limit as i64,
                },
                |r| {
                    let slice = slice_ref_from_row(r, 7)?;
                    let note: Option<String> = r.get(8)?;
                    let mut texts = Vec::new();
                    if let Some(t) = slice.title.clone() {
                        texts.push((SearchField::Title, t));
                    }
                    if let Some(n) = note.filter(|n| !n.is_empty()) {
                        texts.push((SearchField::Note, n));
                    }
                    Ok(SliceSearchRow { slice, texts })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The current version of every global report of any type.
    pub fn current_report_texts(&self) -> Result<Vec<ReportTextRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.report_type, r.period_key, r.status, v.markdown_content
             FROM reports r
             JOIN report_versions v ON v.id = r.current_version_id
             WHERE r.project_id IS NULL",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ReportTextRow {
                    report_type: r.get(0)?,
                    period_key: r.get(1)?,
                    status: r.get(2)?,
                    markdown: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_pattern_escapes_wildcards() {
        assert_eq!(like_pattern("a_b%c\\d"), r"%a\_b\%c\\d%");
        assert_eq!(like_pattern("日志"), "%日志%");
    }
}
