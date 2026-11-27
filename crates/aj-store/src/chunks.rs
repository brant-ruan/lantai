//! Cached intermediate summaries: part digests for long slices, and project briefs for oversized days.
//!
//! A slice that is too long for one request is summarized part by part. Keeping the
//! part digests means that when a session grows, only its last part is redone.
//! A project condensed into task clusters before the daily report is cached by
//! `(local_date, project)` and reused while that project's digests are unchanged.

use anyhow::Result;
use rusqlite::params;

use crate::Store;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkDigestRow {
    pub start_turn_index: i64,
    pub end_turn_index: i64,
    /// Hash of everything that went into the request for this part.
    pub input_hash: String,
    pub digest_json: String,
    /// Report language the part digest is written in.
    pub language: String,
    /// Engine that wrote the part digest (`None` before schema 12).
    pub engine: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectBriefRow {
    pub project: String,
    pub input_hash: String,
    /// The task clusters, as JSON.
    pub clusters_json: String,
    /// Report language the clusters are written in.
    pub language: String,
    /// Engine that wrote the clusters (`None` before schema 12).
    pub engine: Option<String>,
}

impl Store {
    pub fn chunk_digests(&self, slice_id: i64) -> Result<Vec<ChunkDigestRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT start_turn_index, end_turn_index, input_hash, digest_json, language, engine
             FROM digest_chunks WHERE slice_id = ?1 ORDER BY start_turn_index",
        )?;
        let rows = stmt
            .query_map([slice_id], |r| {
                Ok(ChunkDigestRow {
                    start_turn_index: r.get(0)?,
                    end_turn_index: r.get(1)?,
                    input_hash: r.get(2)?,
                    digest_json: r.get(3)?,
                    language: r.get(4)?,
                    engine: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Replace the cached parts of a slice with the given set (empty clears them).
    pub fn replace_chunk_digests(
        &mut self,
        slice_id: i64,
        chunks: &[ChunkDigestRow],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM digest_chunks WHERE slice_id = ?1", [slice_id])?;
        for c in chunks {
            tx.execute(
                "INSERT INTO digest_chunks
                   (slice_id, start_turn_index, end_turn_index, input_hash, digest_json, language,
                    engine)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    slice_id,
                    c.start_turn_index,
                    c.end_turn_index,
                    c.input_hash,
                    c.digest_json,
                    c.language,
                    c.engine
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn project_briefs(&self, local_date: &str) -> Result<Vec<ProjectBriefRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT project, input_hash, clusters_json, language, engine
             FROM project_briefs WHERE local_date = ?1 ORDER BY project",
        )?;
        let rows = stmt
            .query_map([local_date], |r| {
                Ok(ProjectBriefRow {
                    project: r.get(0)?,
                    input_hash: r.get(1)?,
                    clusters_json: r.get(2)?,
                    language: r.get(3)?,
                    engine: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Number of cached project briefs for days in an optional inclusive date range.
    pub fn count_project_briefs(&self, since: Option<&str>, until: Option<&str>) -> Result<u64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM project_briefs
             WHERE (?1 IS NULL OR local_date >= ?1) AND (?2 IS NULL OR local_date <= ?2)",
            params![since, until],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    pub fn save_project_brief(&self, local_date: &str, brief: &ProjectBriefRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO project_briefs
               (local_date, project, input_hash, clusters_json, language, engine)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(local_date, project) DO UPDATE SET
               input_hash = excluded.input_hash,
               clusters_json = excluded.clusters_json,
               language = excluded.language,
               engine = excluded.engine",
            params![
                local_date,
                brief.project,
                brief.input_hash,
                brief.clusters_json,
                brief.language,
                brief.engine
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NewSlice;
    use aj_ir::{AgentId, IrEvent};

    #[test]
    fn chunks_are_replaced_as_a_set_and_follow_their_slice() {
        let mut store = Store::open_in_memory().unwrap();
        let at = "2026-11-16T01:00:00Z".parse().unwrap();
        let events = vec![IrEvent::TurnStart {
            source_event_id: "u1".into(),
            timestamp: at,
            user_prompt: "hello".into(),
        }];
        let sid = store
            .ingest(&AgentId::CLAUDE, "s1", &events)
            .unwrap()
            .session_id;
        let slice = NewSlice {
            local_date: "2026-11-16".into(),
            start_turn_index: 0,
            end_turn_index: 0,
            slice_start_at: at,
            slice_end_at: at,
            active_minutes: 1,
            active_intervals_json: "[]".into(),
            files_modified_json: "[]".into(),
        };
        store.replace_slices(sid, &[slice]).unwrap();
        let slice_id = store.slices_for_session(sid).unwrap()[0].id;

        let chunk = |start, end, hash: &str| ChunkDigestRow {
            start_turn_index: start,
            end_turn_index: end,
            input_hash: hash.into(),
            digest_json: "{}".into(),
            language: "en".into(),
            engine: Some("mock".into()),
        };
        store
            .replace_chunk_digests(slice_id, &[chunk(0, 4, "a"), chunk(5, 9, "b")])
            .unwrap();
        store
            .replace_chunk_digests(slice_id, &[chunk(0, 4, "a"), chunk(5, 12, "c")])
            .unwrap();
        let rows = store.chunk_digests(slice_id).unwrap();
        assert_eq!(rows, vec![chunk(0, 4, "a"), chunk(5, 12, "c")]);

        // Deleting the slice removes its parts.
        store.replace_slices(sid, &[]).unwrap();
        assert!(store.chunk_digests(slice_id).unwrap().is_empty());
    }

    #[test]
    fn project_briefs_upsert_and_filter_by_date() {
        let store = Store::open_in_memory().unwrap();
        let brief = |project: &str, hash: &str, text: &str| ProjectBriefRow {
            project: project.into(),
            input_hash: hash.into(),
            clusters_json: text.into(),
            language: "en".into(),
            engine: Some(format!("engine-{hash}")),
        };
        store
            .save_project_brief("2026-11-16", &brief("alpha", "h1", "- old"))
            .unwrap();
        store
            .save_project_brief("2026-11-16", &brief("beta", "h2", "- beta"))
            .unwrap();
        store
            .save_project_brief("2026-11-15", &brief("alpha", "h0", "- yesterday"))
            .unwrap();
        // Updating the same (date, project) replaces hash and text in place.
        store
            .save_project_brief("2026-11-16", &brief("alpha", "h3", "- new"))
            .unwrap();

        assert_eq!(
            store.project_briefs("2026-11-16").unwrap(),
            vec![brief("alpha", "h3", "- new"), brief("beta", "h2", "- beta"),]
        );
        assert_eq!(
            store.project_briefs("2026-11-15").unwrap(),
            vec![brief("alpha", "h0", "- yesterday")]
        );
    }
}
