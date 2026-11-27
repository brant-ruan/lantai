//! Single-pass SQLite queries backing the analytics dashboard.
//!
//! This module only fetches filtered slice facts and tool aggregates; deduplication and charting logic live in `aj-core`.

use aj_ir::{AgentId, ToolKind, Usage};
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::params;

use crate::sql::{agent_at, SLICE_INCLUDED, SLICE_PROJECT};
use crate::Store;

#[derive(Debug, Clone, PartialEq)]
pub struct StatsSliceRow {
    pub slice_id: i64,
    pub session_id: i64,
    pub local_date: String,
    pub agent: AgentId,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub detected_cwd: Option<String>,
    pub git_root: Option<String>,
    pub turn_count: u64,
    pub active_intervals: Vec<(DateTime<Utc>, DateTime<Utc>)>,
    pub files_modified: Vec<String>,
    pub ignored: bool,
}

/// Token usage of one (local date, agent, model) group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenAggregateRow {
    pub local_date: String,
    pub agent: AgentId,
    pub model: Option<String>,
    pub calls: u64,
    pub usage: Usage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolAggregateRow {
    pub tool_kind: ToolKind,
    pub calls: u64,
    pub errors: u64,
    pub added: u64,
    pub removed: u64,
}

impl Store {
    /// Load slices in `[since_date, until_date]` for statistics aggregation.
    pub fn stats_slices(
        &self,
        since_date: Option<&str>,
        until_date: Option<&str>,
        include_ignored: bool,
    ) -> Result<Vec<StatsSliceRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT sds.id, sds.session_id, sds.local_date, s.agent,
                    {SLICE_PROJECT} AS eff_project_id,
                    lp.name, s.detected_cwd, s.git_root,
                    sds.start_turn_index, sds.end_turn_index,
                    sds.active_intervals_json, sds.files_modified_json,
                    CASE WHEN {SLICE_INCLUDED} THEN 0 ELSE 1 END AS is_ignored
             FROM session_daily_slices sds
             JOIN sessions s ON s.id = sds.session_id
             LEFT JOIN logical_projects lp ON lp.id = {SLICE_PROJECT}
             WHERE (?1 IS NULL OR sds.local_date >= ?1)
               AND (?2 IS NULL OR sds.local_date <= ?2)
               AND (?3 != 0 OR {SLICE_INCLUDED})
             ORDER BY sds.local_date ASC, sds.slice_start_at ASC",
        ))?;
        let rows = stmt
            .query_map(
                params![since_date, until_date, i32::from(include_ignored)],
                |r| {
                    let agent = agent_at(r, 3)?;
                    let start_turn: i64 = r.get(8)?;
                    let end_turn: i64 = r.get(9)?;
                    let intervals_json: String = r.get(10)?;
                    let files_json: String = r.get(11)?;
                    let ignored_int: i64 = r.get(12)?;
                    Ok(StatsSliceRow {
                        slice_id: r.get(0)?,
                        session_id: r.get(1)?,
                        local_date: r.get(2)?,
                        agent,
                        project_id: r.get(4)?,
                        project_name: r.get(5)?,
                        detected_cwd: r.get(6)?,
                        git_root: r.get(7)?,
                        turn_count: (end_turn - start_turn + 1).max(0) as u64,
                        active_intervals: serde_json::from_str(&intervals_json).unwrap_or_default(),
                        files_modified: serde_json::from_str(&files_json).unwrap_or_default(),
                        ignored: ignored_int != 0,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Distinct local dates with at least one slice, oldest first, regardless of any range.
    pub fn stats_active_dates(&self, include_ignored: bool) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT DISTINCT sds.local_date
             FROM session_daily_slices sds
             JOIN sessions s ON s.id = sds.session_id
             WHERE ?1 != 0 OR {SLICE_INCLUDED}
             ORDER BY sds.local_date ASC",
        ))?;
        let rows = stmt
            .query_map([i32::from(include_ignored)], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(rows)
    }

    /// Load token usage in `[since_date, until_date]` grouped by local date, agent and model.
    /// Usage reaches a day through its turn's slice, with the same ignore filter as the slices.
    pub fn stats_token_aggregates(
        &self,
        since_date: Option<&str>,
        until_date: Option<&str>,
        include_ignored: bool,
    ) -> Result<Vec<TokenAggregateRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT sds.local_date, s.agent, tu.model, COUNT(*),
                    SUM(tu.input_tokens), SUM(tu.output_tokens),
                    SUM(tu.cache_read_tokens), SUM(tu.cache_write_tokens)
             FROM token_usage tu
             JOIN turns t ON t.id = tu.turn_id
             JOIN session_daily_slices sds
               ON sds.session_id = t.session_id
              AND t.turn_index BETWEEN sds.start_turn_index AND sds.end_turn_index
             JOIN sessions s ON s.id = sds.session_id
             WHERE (?1 IS NULL OR sds.local_date >= ?1)
               AND (?2 IS NULL OR sds.local_date <= ?2)
               AND (?3 != 0 OR {SLICE_INCLUDED})
             GROUP BY sds.local_date, s.agent, tu.model
             ORDER BY sds.local_date ASC, s.agent ASC, tu.model ASC",
        ))?;
        let count = |r: &rusqlite::Row, i: usize| r.get::<_, i64>(i).map(|v| v.max(0) as u64);
        let rows = stmt
            .query_map(
                params![since_date, until_date, i32::from(include_ignored)],
                |r| {
                    let agent = agent_at(r, 1)?;
                    Ok(TokenAggregateRow {
                        local_date: r.get(0)?,
                        agent,
                        model: r.get(2)?,
                        calls: count(r, 3)?,
                        usage: Usage {
                            input_tokens: count(r, 4)?,
                            output_tokens: count(r, 5)?,
                            cache_read_tokens: count(r, 6)?,
                            cache_write_tokens: count(r, 7)?,
                        },
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Load tool call totals grouped by [`ToolKind`] across slices in `[since_date, until_date]`.
    pub fn stats_tool_aggregates(
        &self,
        since_date: Option<&str>,
        until_date: Option<&str>,
        include_ignored: bool,
    ) -> Result<Vec<ToolAggregateRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT ti.tool_kind,
                    COUNT(*),
                    COALESCE(SUM(CASE WHEN ti.is_error = 1 THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(COALESCE(ti.added, 0)), 0),
                    COALESCE(SUM(COALESCE(ti.removed, 0)), 0)
             FROM turn_items ti
             JOIN turns t ON t.id = ti.turn_id
             JOIN session_daily_slices sds
               ON sds.session_id = t.session_id
              AND t.turn_index BETWEEN sds.start_turn_index AND sds.end_turn_index
             JOIN sessions s ON s.id = sds.session_id
             WHERE ti.item_type = 'tool'
               AND (?1 IS NULL OR sds.local_date >= ?1)
               AND (?2 IS NULL OR sds.local_date <= ?2)
               AND (?3 != 0 OR {SLICE_INCLUDED})
             GROUP BY ti.tool_kind",
        ))?;
        let rows = stmt
            .query_map(
                params![since_date, until_date, i32::from(include_ignored)],
                |r| {
                    let kind_raw: Option<String> = r.get(0)?;
                    Ok(ToolAggregateRow {
                        tool_kind: ToolKind::parse(kind_raw.as_deref().unwrap_or_default()),
                        calls: r.get::<_, i64>(1)?.max(0) as u64,
                        errors: r.get::<_, i64>(2)?.max(0) as u64,
                        added: r.get::<_, i64>(3)?.max(0) as u64,
                        removed: r.get::<_, i64>(4)?.max(0) as u64,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NewSlice;
    use aj_ir::{DiffStat, IrEvent};

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn turn_events(
        prefix: &str,
        ts_str: &str,
        kind: ToolKind,
        err: bool,
        diff: Option<(u32, u32)>,
    ) -> Vec<IrEvent> {
        vec![
            IrEvent::TurnStart {
                source_event_id: format!("{prefix}-u"),
                timestamp: t(ts_str),
                user_prompt: prefix.into(),
            },
            IrEvent::ToolAction {
                source_event_id: format!("{prefix}-tool"),
                timestamp: t(ts_str),
                kind,
                tool_name: kind.as_str().into(),
                target: Some("/repo/src/main.rs".into()),
                summary: None,
                diff_stat: diff.map(|(added, removed)| DiffStat { added, removed }),
            },
            IrEvent::ToolOutcome {
                tool_source_id: format!("{prefix}-tool"),
                is_error: err,
                diff_stat: None,
                output_summary: None,
            },
        ]
    }

    #[test]
    fn stats_queries_respect_dates_ignores_and_project_overrides() {
        let mut store = Store::open_in_memory().unwrap();
        let p_main = store.create_project("Main", None).unwrap();
        let p_side = store.create_project("Side", None).unwrap();

        let mut events1 = turn_events(
            "d1",
            "2026-11-10T03:00:00Z",
            ToolKind::FileEdit,
            false,
            Some((10, 2)),
        );
        events1.extend(turn_events(
            "d2",
            "2026-11-15T03:00:00Z",
            ToolKind::BashCmd,
            true,
            None,
        ));
        let sid1 = store
            .ingest(&AgentId::CLAUDE, "s1", &events1)
            .unwrap()
            .session_id;
        store.assign_manual(sid1, Some(p_main)).unwrap();
        store
            .replace_slices(
                sid1,
                &[
                    NewSlice {
                        local_date: "2026-11-10".into(),
                        start_turn_index: 0,
                        end_turn_index: 0,
                        slice_start_at: t("2026-11-10T03:00:00Z"),
                        slice_end_at: t("2026-11-10T03:10:00Z"),
                        active_minutes: 10,
                        active_intervals_json:
                            r#"[["2026-11-10T03:00:00Z","2026-11-10T03:10:00Z"]]"#.into(),
                        files_modified_json: r#"["/repo/src/main.rs"]"#.into(),
                    },
                    NewSlice {
                        local_date: "2026-11-15".into(),
                        start_turn_index: 1,
                        end_turn_index: 1,
                        slice_start_at: t("2026-11-15T03:00:00Z"),
                        slice_end_at: t("2026-11-15T03:05:00Z"),
                        active_minutes: 5,
                        active_intervals_json:
                            r#"[["2026-11-15T03:00:00Z","2026-11-15T03:05:00Z"]]"#.into(),
                        files_modified_json: "[]".into(),
                    },
                ],
            )
            .unwrap();
        let slice_d1 = store.slices_for_date("2026-11-10").unwrap()[0].id;
        let slice_d2 = store.slices_for_date("2026-11-15").unwrap()[0].id;
        store
            .set_slice_project_override(slice_d2, Some(p_side))
            .unwrap();
        // Ignore the first slice explicitly while keeping session unignored.
        store.set_slice_ignore_state(slice_d1, "ignored").unwrap();

        // Second session is session-ignored, with one slice explicitly included.
        let mut events2 = turn_events(
            "ig",
            "2026-11-15T04:00:00Z",
            ToolKind::FileRead,
            false,
            None,
        );
        events2.extend(turn_events(
            "inc",
            "2026-11-16T04:00:00Z",
            ToolKind::FileWrite,
            false,
            Some((4, 0)),
        ));
        let sid2 = store
            .ingest(&AgentId::CODEX, "s2", &events2)
            .unwrap()
            .session_id;
        store.set_session_ignored(sid2, true).unwrap();
        store
            .replace_slices(
                sid2,
                &[
                    NewSlice {
                        local_date: "2026-11-15".into(),
                        start_turn_index: 0,
                        end_turn_index: 0,
                        slice_start_at: t("2026-11-15T04:00:00Z"),
                        slice_end_at: t("2026-11-15T04:01:00Z"),
                        active_minutes: 1,
                        active_intervals_json: "[]".into(),
                        files_modified_json: "[]".into(),
                    },
                    NewSlice {
                        local_date: "2026-11-16".into(),
                        start_turn_index: 1,
                        end_turn_index: 1,
                        slice_start_at: t("2026-11-16T04:00:00Z"),
                        slice_end_at: t("2026-11-16T04:01:00Z"),
                        active_minutes: 1,
                        active_intervals_json: "[]".into(),
                        files_modified_json: "[]".into(),
                    },
                ],
            )
            .unwrap();
        let slice_inc = store.slices_for_date("2026-11-16").unwrap()[0].id;
        store.set_slice_ignore_state(slice_inc, "included").unwrap();

        // Active slices across all dates should include s1@11-15 (Side) and s2@11-16 (Unassigned).
        let active = store.stats_slices(None, None, false).unwrap();
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].local_date, "2026-11-15");
        assert_eq!(active[0].project_name.as_deref(), Some("Side"));
        assert!(!active[0].ignored);
        assert_eq!(active[1].local_date, "2026-11-16");
        assert_eq!(active[1].agent, AgentId::CODEX);
        assert!(!active[1].ignored);

        // Date filter [2026-11-16, 2026-11-16] excludes earlier slices and counts only FileWrite.
        let ranged = store
            .stats_slices(Some("2026-11-16"), Some("2026-11-16"), true)
            .unwrap();
        assert_eq!(ranged.len(), 1);
        let tools_ranged = store
            .stats_tool_aggregates(Some("2026-11-16"), Some("2026-11-16"), false)
            .unwrap();
        assert_eq!(
            tools_ranged,
            vec![ToolAggregateRow {
                tool_kind: ToolKind::FileWrite,
                calls: 1,
                errors: 0,
                added: 4,
                removed: 0,
            }]
        );

        // All slices including ignored should return all 4 slices and all 4 tool items.
        assert_eq!(
            store.stats_active_dates(false).unwrap(),
            vec!["2026-11-15", "2026-11-16"]
        );
        assert_eq!(store.stats_active_dates(true).unwrap().len(), 3);

        let all = store.stats_slices(None, None, true).unwrap();
        assert_eq!(all.len(), 4);
        assert_eq!(all.iter().filter(|s| s.ignored).count(), 2);
        let tools_all = store.stats_tool_aggregates(None, None, true).unwrap();
        let total_calls: u64 = tools_all.iter().map(|r| r.calls).sum();
        assert_eq!(total_calls, 4);
    }

    #[test]
    fn token_usage_dedupes_and_groups_by_day_agent_model() {
        let mut store = Store::open_in_memory().unwrap();
        let usage = |input, output, cache_read| Usage {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache_read,
            cache_write_tokens: 0,
        };
        let tu = |id: &str, ts_str: &str, model: &str, u: Usage| IrEvent::TokenUsage {
            source_event_id: id.into(),
            timestamp: t(ts_str),
            model: Some(model.into()),
            usage: u,
        };
        let events = vec![
            // Before any turn: dropped.
            tu("u0", "2026-11-10T02:59:00Z", "m1", usage(9, 9, 9)),
            IrEvent::TurnStart {
                source_event_id: "t1".into(),
                timestamp: t("2026-11-10T03:00:00Z"),
                user_prompt: "a".into(),
            },
            tu("u1", "2026-11-10T03:00:01Z", "m1", usage(10, 5, 100)),
            tu("u1", "2026-11-10T03:00:02Z", "m1", usage(10, 5, 100)),
            tu("u2", "2026-11-10T03:00:03Z", "m2", usage(1, 2, 3)),
        ];
        let sid = store
            .ingest(&AgentId::CLAUDE, "s1", &events)
            .unwrap()
            .session_id;
        // A later batch repeating an earlier call stays idempotent.
        store
            .ingest(
                &AgentId::CLAUDE,
                "s1",
                &[tu("u1", "2026-11-10T03:00:01Z", "m1", usage(10, 5, 100))],
            )
            .unwrap();
        store
            .replace_slices(
                sid,
                &[NewSlice {
                    local_date: "2026-11-10".into(),
                    start_turn_index: 0,
                    end_turn_index: 0,
                    slice_start_at: t("2026-11-10T03:00:00Z"),
                    slice_end_at: t("2026-11-10T03:00:03Z"),
                    active_minutes: 1,
                    active_intervals_json: "[]".into(),
                    files_modified_json: "[]".into(),
                }],
            )
            .unwrap();

        let rows = store.stats_token_aggregates(None, None, false).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].model.as_deref(), Some("m1"));
        assert_eq!((rows[0].calls, rows[0].usage), (1, usage(10, 5, 100)));
        assert_eq!((rows[1].calls, rows[1].usage), (1, usage(1, 2, 3)));

        let slice = store.slices_for_date("2026-11-10").unwrap()[0].id;
        store.set_slice_ignore_state(slice, "ignored").unwrap();
        assert!(store
            .stats_token_aggregates(None, None, false)
            .unwrap()
            .is_empty());
    }
}
