//! Merging IR events ([`aj_ir::IrEvent`]) into sessions, turns and the items inside a turn.
//!
//! Ingestion is idempotent: writing the same event twice never produces duplicates. Items that
//! arrive before any human input open an implicit turn with an empty prompt.

use aj_ir::{AgentId, IrEvent};
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Transaction};

use crate::sql::ts;
use crate::Store;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IngestStats {
    pub session_id: i64,
    pub new_turns: u64,
    pub new_items: u64,
    pub unknown: u64,
}

impl Store {
    /// Merge a batch of events into a session, inside a single transaction.
    pub fn ingest(
        &mut self,
        agent: &AgentId,
        source_session_id: &str,
        events: &[IrEvent],
    ) -> Result<IngestStats> {
        let tx = self.conn.transaction()?;
        let stats = ingest_tx(&tx, agent, source_session_id, events)?;
        tx.commit()?;
        Ok(stats)
    }
}

fn ingest_tx(
    tx: &Transaction,
    agent: &AgentId,
    source_session_id: &str,
    events: &[IrEvent],
) -> Result<IngestStats> {
    tx.execute(
        "INSERT OR IGNORE INTO sessions (agent, source_session_id) VALUES (?1, ?2)",
        params![agent.as_str(), source_session_id],
    )?;
    let session_id: i64 = tx.query_row(
        "SELECT id FROM sessions WHERE agent = ?1 AND source_session_id = ?2",
        params![agent.as_str(), source_session_id],
        |r| r.get(0),
    )?;

    let mut stats = IngestStats {
        session_id,
        ..Default::default()
    };
    // Current turn: the latest TurnStart in this batch; items at the start of a batch belong to the session's last existing turn.
    let mut current_turn: Option<i64> = tx
        .query_row(
            "SELECT id FROM turns WHERE session_id = ?1 ORDER BY turn_index DESC LIMIT 1",
            [session_id],
            |r| r.get(0),
        )
        .optional()?;

    for event in events {
        match event {
            IrEvent::SessionMeta {
                cwd,
                git_branch,
                agent_version,
                parent_session_id,
            } => {
                // The working directory keeps its first value (a mid-session cd does not change ownership); the rest take the latest value.
                tx.execute(
                    "UPDATE sessions SET
                       detected_cwd = COALESCE(detected_cwd, ?2),
                       git_branch = COALESCE(?3, git_branch),
                       agent_version = COALESCE(?4, agent_version),
                       parent_session_id = COALESCE(?5, parent_session_id)
                     WHERE id = ?1",
                    params![
                        session_id,
                        cwd,
                        git_branch,
                        agent_version,
                        parent_session_id
                    ],
                )?;
            }
            IrEvent::SessionTitle { title } => {
                tx.execute(
                    "UPDATE sessions SET title = ?2 WHERE id = ?1",
                    params![session_id, title],
                )?;
            }
            IrEvent::TurnStart {
                source_event_id,
                timestamp,
                user_prompt,
            } => {
                let turn_id = insert_turn(
                    tx,
                    session_id,
                    source_event_id,
                    *timestamp,
                    user_prompt,
                    &mut stats,
                )?;
                current_turn = Some(turn_id);
            }
            IrEvent::AgentMessage {
                source_event_id,
                timestamp,
                text,
                ..
            } => {
                let turn_id = match current_turn {
                    Some(id) => id,
                    None => implicit_turn(tx, session_id, source_event_id, *timestamp, &mut stats)?,
                };
                current_turn = Some(turn_id);
                let inserted = tx.execute(
                    "INSERT OR IGNORE INTO turn_items
                       (session_id, turn_id, seq, source_event_id, timestamp, item_type, text)
                     VALUES (?1, ?2, (SELECT COALESCE(MAX(seq), -1) + 1 FROM turn_items WHERE turn_id = ?2),
                             ?3, ?4, 'message', ?5)",
                    params![session_id, turn_id, source_event_id, ts(*timestamp), text],
                )?;
                if inserted > 0 {
                    stats.new_items += 1;
                    touch(tx, session_id, turn_id, *timestamp)?;
                }
            }
            IrEvent::ToolAction {
                source_event_id,
                timestamp,
                kind,
                tool_name,
                target,
                summary,
                diff_stat,
            } => {
                let turn_id = match current_turn {
                    Some(id) => id,
                    None => implicit_turn(tx, session_id, source_event_id, *timestamp, &mut stats)?,
                };
                current_turn = Some(turn_id);
                let inserted = tx.execute(
                    "INSERT OR IGNORE INTO turn_items
                       (session_id, turn_id, seq, source_event_id, timestamp, item_type,
                        tool_kind, tool_name, target, summary, added, removed)
                     VALUES (?1, ?2, (SELECT COALESCE(MAX(seq), -1) + 1 FROM turn_items WHERE turn_id = ?2),
                             ?3, ?4, 'tool', ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        session_id,
                        turn_id,
                        source_event_id,
                        ts(*timestamp),
                        kind.as_str(),
                        tool_name,
                        target,
                        summary,
                        diff_stat.map(|d| d.added),
                        diff_stat.map(|d| d.removed),
                    ],
                )?;
                if inserted > 0 {
                    stats.new_items += 1;
                    touch(tx, session_id, turn_id, *timestamp)?;
                }
            }
            IrEvent::ToolOutcome {
                tool_source_id,
                is_error,
                diff_stat,
                output_summary,
            } => {
                tx.execute(
                    "UPDATE turn_items SET
                       is_error = ?3,
                       added = COALESCE(?4, added),
                       removed = COALESCE(?5, removed),
                       output_summary = COALESCE(?6, output_summary)
                     WHERE session_id = ?1 AND source_event_id = ?2 AND item_type = 'tool'",
                    params![
                        session_id,
                        tool_source_id,
                        is_error,
                        diff_stat.map(|d| d.added),
                        diff_stat.map(|d| d.removed),
                        output_summary,
                    ],
                )?;
            }
            IrEvent::TokenUsage {
                source_event_id,
                timestamp,
                model,
                usage,
            } => {
                // Usage never opens a turn; before the session's first turn it has nowhere to go.
                let Some(turn_id) = current_turn else {
                    continue;
                };
                tx.execute(
                    "INSERT OR IGNORE INTO token_usage
                       (session_id, turn_id, source_event_id, timestamp, model,
                        input_tokens, output_tokens, cache_read_tokens, cache_write_tokens)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        session_id,
                        turn_id,
                        source_event_id,
                        ts(*timestamp),
                        model,
                        usage.input_tokens as i64,
                        usage.output_tokens as i64,
                        usage.cache_read_tokens as i64,
                        usage.cache_write_tokens as i64,
                    ],
                )?;
            }
            IrEvent::Unknown { .. } => stats.unknown += 1,
        }
    }
    Ok(stats)
}

fn insert_turn(
    tx: &Transaction,
    session_id: i64,
    source_event_id: &str,
    timestamp: DateTime<Utc>,
    user_prompt: &str,
    stats: &mut IngestStats,
) -> Result<i64> {
    let t = ts(timestamp);
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO turns (session_id, turn_index, source_event_id, started_at, ended_at, user_prompt)
         VALUES (?1, (SELECT COALESCE(MAX(turn_index), -1) + 1 FROM turns WHERE session_id = ?1), ?2, ?3, ?3, ?4)",
        params![session_id, source_event_id, t, user_prompt],
    )?;
    let turn_id: i64 = tx.query_row(
        "SELECT id FROM turns WHERE session_id = ?1 AND source_event_id = ?2",
        params![session_id, source_event_id],
        |r| r.get(0),
    )?;
    if inserted > 0 {
        stats.new_turns += 1;
        // Without an explicit title, use the beginning of the first input.
        let first_line = user_prompt
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();
        let fallback: String = first_line.chars().take(60).collect();
        tx.execute(
            "UPDATE sessions SET fallback_title = COALESCE(fallback_title, NULLIF(?2, '')) WHERE id = ?1",
            params![session_id, fallback],
        )?;
        touch(tx, session_id, turn_id, timestamp)?;
    }
    Ok(turn_id)
}

/// Agent activity appeared before any human input (e.g. the session starts from a compaction summary): add a turn with an empty prompt.
fn implicit_turn(
    tx: &Transaction,
    session_id: i64,
    source_event_id: &str,
    timestamp: DateTime<Utc>,
    stats: &mut IngestStats,
) -> Result<i64> {
    insert_turn(
        tx,
        session_id,
        &format!("implicit:{source_event_id}"),
        timestamp,
        "",
        stats,
    )
}

fn touch(tx: &Transaction, session_id: i64, turn_id: i64, timestamp: DateTime<Utc>) -> Result<()> {
    let t = ts(timestamp);
    tx.execute(
        "UPDATE turns SET ended_at = MAX(ended_at, ?2) WHERE id = ?1",
        params![turn_id, t],
    )?;
    tx.execute(
        "UPDATE sessions SET
           first_active_at = MIN(COALESCE(first_active_at, ?2), ?2),
           last_active_at = MAX(COALESCE(last_active_at, ?2), ?2)
         WHERE id = ?1",
        params![session_id, t],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_events::{sample_events, t};
    use crate::{Store, TurnItem};
    use aj_ir::AgentId;

    #[test]
    fn ingest_is_idempotent() {
        let mut store = Store::open_in_memory().unwrap();
        let events = sample_events();
        let first = store.ingest(&AgentId::CLAUDE, "s1", &events).unwrap();
        assert_eq!((first.new_turns, first.new_items), (1, 2));
        let second = store.ingest(&AgentId::CLAUDE, "s1", &events).unwrap();
        assert_eq!((second.new_turns, second.new_items), (0, 0));

        let session = store.session(first.session_id).unwrap().unwrap();
        assert_eq!(session.title.as_deref(), Some("fix the login"));
        assert_eq!(session.detected_cwd.as_deref(), Some("/work/app"));

        let turns = store.all_turns(first.session_id).unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].ended_at, t("2026-11-16T01:01:00Z"));
        let items = store.turn_items(turns[0].id).unwrap();
        assert_eq!(items.len(), 2);
        // The patch line counts from the tool result override the estimate from the arguments.
        match &items[0] {
            TurnItem::Tool {
                added,
                removed,
                is_error,
                ..
            } => {
                assert_eq!(
                    (*added, *removed, *is_error),
                    (Some(5), Some(2), Some(false))
                );
            }
            other => panic!("expected a tool item, got {other:?}"),
        }
    }

    #[test]
    fn items_in_later_batch_attach_to_last_turn() {
        let mut store = Store::open_in_memory().unwrap();
        let events = sample_events();
        store.ingest(&AgentId::CLAUDE, "s1", &events[..2]).unwrap();
        let stats = store.ingest(&AgentId::CLAUDE, "s1", &events[2..]).unwrap();
        assert_eq!((stats.new_turns, stats.new_items), (0, 2));
        assert_eq!(store.all_turns(stats.session_id).unwrap().len(), 1);
    }

    #[test]
    fn activity_before_any_prompt_gets_implicit_turn() {
        let mut store = Store::open_in_memory().unwrap();
        let events = sample_events();
        let stats = store.ingest(&AgentId::CLAUDE, "s1", &events[4..]).unwrap();
        let turns = store.all_turns(stats.session_id).unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].user_prompt, "");
    }
}
