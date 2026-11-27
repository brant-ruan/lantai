//! Reports and their versions. Generating again or saving an edit adds a version;
//! nothing is ever overwritten.

use std::collections::HashSet;

use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};

use crate::sql::ts;
use crate::{ReportOrigin, ReportStatus, Store};

/// A report version to be saved.
#[derive(Debug, Clone)]
pub struct NewReportVersion<'a> {
    /// `daily` | `weekly` | `monthly`
    pub report_type: &'a str,
    /// e.g. `2026-11-16`, `2026-W47`, `2026-11`
    pub period_key: &'a str,
    /// `None` for a report covering every project.
    pub project_id: Option<i64>,
    pub origin: ReportOrigin,
    pub markdown: &'a str,
    pub engine: Option<&'a str>,
    pub input_hash: Option<&'a str>,
    pub prompt_id: Option<i64>,
    pub prompt_name: Option<&'a str>,
    /// Full text of the prompt at generation time, so the version stays reproducible
    /// after the prompt is edited or deleted.
    pub prompt_snapshot: Option<&'a str>,
    /// What the version was generated from, per project (JSON owned by `aj-report`).
    /// User edits carry over the manifest of the version they were based on.
    pub input_manifest: Option<&'a str>,
    /// Report language the version is written in, as a language tag.
    pub language: &'a str,
}

#[derive(Debug, Clone)]
pub struct ReportVersionRow {
    pub id: i64,
    /// 1-based position among the versions of the same report.
    pub number: u64,
    pub origin: ReportOrigin,
    pub markdown: String,
    pub engine: Option<String>,
    pub prompt_name: Option<String>,
    pub prompt_snapshot: Option<String>,
    pub input_manifest: Option<String>,
    pub language: String,
    pub created_at: String,
    pub is_current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportMeta {
    pub id: i64,
    pub status: ReportStatus,
    pub current_version_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeriodReportRow {
    /// The period key: a date for daily reports, a week key for weekly ones.
    pub period_key: String,
    pub status: ReportStatus,
    pub markdown: String,
    pub language: String,
    pub created_at: String,
}

/// One global report with a current version, summarized for the dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportSummaryRow {
    pub period_key: String,
    pub status: ReportStatus,
    pub versions: u64,
    /// Whether any version was saved by hand.
    pub edited: bool,
    /// Whether the current version records its inputs, so staleness can be checked.
    pub has_manifest: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchStatusResult {
    pub updated: Vec<String>,
    pub skipped: Vec<String>,
}

impl Store {
    /// Save a new version and make it the current one.
    pub fn save_report_version(&mut self, v: &NewReportVersion) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let report_id: Option<i64> = tx
            .query_row(
                "SELECT id FROM reports WHERE report_type = ?1 AND period_key = ?2 AND project_id IS ?3",
                params![v.report_type, v.period_key, v.project_id],
                |r| r.get(0),
            )
            .optional()?;
        let report_id = match report_id {
            Some(id) => id,
            None => {
                tx.execute(
                    "INSERT INTO reports (report_type, period_key, project_id) VALUES (?1, ?2, ?3)",
                    params![v.report_type, v.period_key, v.project_id],
                )?;
                tx.last_insert_rowid()
            }
        };
        tx.execute(
            "INSERT INTO report_versions
               (report_id, origin, markdown_content, engine, input_hash,
                prompt_id, prompt_name, prompt_snapshot, input_manifest_json, language, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                report_id,
                v.origin,
                v.markdown,
                v.engine,
                v.input_hash,
                v.prompt_id,
                v.prompt_name,
                v.prompt_snapshot,
                v.input_manifest,
                v.language,
                ts(Utc::now()),
            ],
        )?;
        let version_id = tx.last_insert_rowid();
        tx.execute(
            "UPDATE reports SET current_version_id = ?2 WHERE id = ?1",
            params![report_id, version_id],
        )?;
        tx.commit()?;
        Ok(version_id)
    }

    /// All versions of a report, oldest first.
    pub fn report_versions(
        &self,
        report_type: &str,
        period_key: &str,
        project_id: Option<i64>,
    ) -> Result<Vec<ReportVersionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT v.id, v.origin, v.markdown_content, v.engine, v.prompt_name, v.prompt_snapshot,
                    v.input_manifest_json, v.created_at, v.id = r.current_version_id, v.language
             FROM reports r JOIN report_versions v ON v.report_id = r.id
             WHERE r.report_type = ?1 AND r.period_key = ?2 AND r.project_id IS ?3
             ORDER BY v.id",
        )?;
        let mut rows = stmt
            .query_map(params![report_type, period_key, project_id], |r| {
                Ok(ReportVersionRow {
                    id: r.get(0)?,
                    number: 0,
                    origin: r.get(1)?,
                    markdown: r.get(2)?,
                    engine: r.get(3)?,
                    prompt_name: r.get(4)?,
                    prompt_snapshot: r.get(5)?,
                    input_manifest: r.get(6)?,
                    created_at: r.get(7)?,
                    is_current: r.get(8)?,
                    language: r.get(9)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (i, row) in rows.iter_mut().enumerate() {
            row.number = i as u64 + 1;
        }
        Ok(rows)
    }

    /// Whether any report version has been saved.
    pub fn has_reports(&self) -> Result<bool> {
        Ok(self
            .conn
            .query_row("SELECT EXISTS (SELECT 1 FROM report_versions)", [], |r| {
                r.get(0)
            })?)
    }

    /// The current version of a report, if it has one.
    pub fn current_report_version(
        &self,
        report_type: &str,
        period_key: &str,
        project_id: Option<i64>,
    ) -> Result<Option<ReportVersionRow>> {
        Ok(self
            .report_versions(report_type, period_key, project_id)?
            .into_iter()
            .find(|v| v.is_current))
    }

    pub fn report_meta(
        &self,
        report_type: &str,
        period_key: &str,
        project_id: Option<i64>,
    ) -> Result<Option<ReportMeta>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, status, current_version_id FROM reports
                 WHERE report_type = ?1 AND period_key = ?2 AND project_id IS ?3",
                params![report_type, period_key, project_id],
                |r| {
                    Ok(ReportMeta {
                        id: r.get(0)?,
                        status: r.get(1)?,
                        current_version_id: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn set_report_status(
        &self,
        report_type: &str,
        period_key: &str,
        project_id: Option<i64>,
        status: ReportStatus,
    ) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE reports SET status = ?4
             WHERE report_type = ?1 AND period_key = ?2 AND project_id IS ?3",
            params![report_type, period_key, project_id, status],
        )?;
        Ok(changed > 0)
    }

    /// Update the status of several reports of one type in a single transaction.
    /// Periods without a current report version are recorded as skipped.
    pub fn set_reports_status(
        &mut self,
        report_type: &str,
        period_keys: &[String],
        project_id: Option<i64>,
        status: ReportStatus,
    ) -> Result<BatchStatusResult> {
        let tx = self.conn.transaction()?;
        let mut seen = HashSet::with_capacity(period_keys.len());
        let mut result = BatchStatusResult::default();
        for key in period_keys {
            if !seen.insert(key.as_str()) {
                continue;
            }
            let changed = tx.execute(
                "UPDATE reports SET status = ?4
                 WHERE report_type = ?1 AND period_key = ?2 AND project_id IS ?3
                   AND current_version_id IS NOT NULL",
                params![report_type, key, project_id, status],
            )?;
            if changed > 0 {
                result.updated.push(key.clone());
            } else {
                result.skipped.push(key.clone());
            }
        }
        tx.commit()?;
        Ok(result)
    }

    pub fn set_current_report_version(
        &self,
        report_type: &str,
        period_key: &str,
        project_id: Option<i64>,
        version_id: i64,
    ) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE reports SET current_version_id = ?4
             WHERE report_type = ?1 AND period_key = ?2 AND project_id IS ?3
               AND EXISTS (
                   SELECT 1 FROM report_versions v
                   WHERE v.id = ?4 AND v.report_id = reports.id
               )",
            params![report_type, period_key, project_id, version_id],
        )?;
        Ok(changed > 0)
    }

    /// Summarize every global report of one type that has a current version, within an
    /// optional inclusive range of period keys, oldest first.
    pub fn report_summaries(
        &self,
        report_type: &str,
        since_key: Option<&str>,
        until_key: Option<&str>,
    ) -> Result<Vec<ReportSummaryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.period_key, r.status, COUNT(v.id),
                    MAX(v.origin = 'user_edit'),
                    MAX(v.id = r.current_version_id AND v.input_manifest_json IS NOT NULL)
             FROM reports r
             JOIN report_versions v ON v.report_id = r.id
             WHERE r.report_type = ?3
               AND r.project_id IS NULL
               AND r.current_version_id IS NOT NULL
               AND (?1 IS NULL OR r.period_key >= ?1)
               AND (?2 IS NULL OR r.period_key <= ?2)
             GROUP BY r.id
             ORDER BY r.period_key",
        )?;
        let rows = stmt
            .query_map(params![since_key, until_key, report_type], |r| {
                Ok(ReportSummaryRow {
                    period_key: r.get(0)?,
                    status: r.get(1)?,
                    versions: r.get::<_, i64>(2)? as u64,
                    edited: r.get::<_, i64>(3)? != 0,
                    has_manifest: r.get::<_, i64>(4)? != 0,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Fetch the current version of every global report of one type within an optional
    /// inclusive range of period keys, newest first. Keys of one type sort chronologically.
    pub fn period_reports_in_range(
        &self,
        report_type: &str,
        since_key: Option<&str>,
        until_key: Option<&str>,
    ) -> Result<Vec<PeriodReportRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.period_key, r.status, v.markdown_content, v.created_at, v.language
             FROM reports r
             JOIN report_versions v ON v.id = r.current_version_id
             WHERE r.report_type = ?3
               AND r.project_id IS NULL
               AND (?1 IS NULL OR r.period_key >= ?1)
               AND (?2 IS NULL OR r.period_key <= ?2)
             ORDER BY r.period_key DESC",
        )?;
        let rows = stmt
            .query_map(params![since_key, until_key, report_type], |r| {
                Ok(PeriodReportRow {
                    period_key: r.get(0)?,
                    status: r.get(1)?,
                    markdown: r.get(2)?,
                    created_at: r.get(3)?,
                    language: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version<'a>(markdown: &'a str, project_id: Option<i64>) -> NewReportVersion<'a> {
        NewReportVersion {
            report_type: "daily",
            period_key: "2026-11-16",
            project_id,
            origin: ReportOrigin::Generated,
            markdown,
            engine: Some("mock"),
            input_hash: None,
            prompt_id: None,
            prompt_name: Some("standard"),
            prompt_snapshot: Some("body"),
            input_manifest: Some("{}"),
            language: "en",
        }
    }

    #[test]
    fn versions_accumulate_and_the_latest_is_current() {
        let mut store = Store::open_in_memory().unwrap();
        store.save_report_version(&version("v1", None)).unwrap();
        store.save_report_version(&version("v2", None)).unwrap();

        let versions = store.report_versions("daily", "2026-11-16", None).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!((versions[0].number, versions[0].is_current), (1, false));
        assert_eq!(
            (versions[1].markdown.as_str(), versions[1].is_current),
            ("v2", true)
        );
        assert_eq!(versions[1].prompt_snapshot.as_deref(), Some("body"));
    }

    #[test]
    fn project_reports_are_separate_from_the_global_one() {
        let mut store = Store::open_in_memory().unwrap();
        let pid = store.create_project("P", None).unwrap();
        store.save_report_version(&version("global", None)).unwrap();
        store
            .save_report_version(&version("project", Some(pid)))
            .unwrap();
        assert_eq!(
            store
                .report_versions("daily", "2026-11-16", None)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .report_versions("daily", "2026-11-16", Some(pid))
                .unwrap()[0]
                .markdown,
            "project"
        );
    }

    #[test]
    fn summaries_count_versions_and_hand_edits_of_global_reports() {
        let mut store = Store::open_in_memory().unwrap();
        let pid = store.create_project("P", None).unwrap();
        store.save_report_version(&version("v1", None)).unwrap();
        store
            .save_report_version(&NewReportVersion {
                origin: ReportOrigin::UserEdit,
                input_manifest: None,
                ..version("v2", None)
            })
            .unwrap();
        store
            .save_report_version(&version("project", Some(pid)))
            .unwrap();
        store
            .save_report_version(&NewReportVersion {
                period_key: "2026-11-17",
                ..version("other day", None)
            })
            .unwrap();

        let rows = store.report_summaries("daily", None, None).unwrap();
        assert_eq!(rows.len(), 2, "project reports are left out");
        assert_eq!(
            rows[0],
            ReportSummaryRow {
                period_key: "2026-11-16".into(),
                status: ReportStatus::Draft,
                versions: 2,
                edited: true,
                has_manifest: false,
            }
        );
        assert!(!rows[1].edited && rows[1].has_manifest);
        let only = store
            .report_summaries("daily", Some("2026-11-17"), None)
            .unwrap();
        assert_eq!(only.len(), 1);
    }

    #[test]
    fn report_status_and_current_version_can_be_updated() {
        let mut store = Store::open_in_memory().unwrap();
        assert_eq!(
            store.report_meta("daily", "2026-11-16", None).unwrap(),
            None
        );

        let v1 = store.save_report_version(&version("v1", None)).unwrap();
        let v2 = store.save_report_version(&version("v2", None)).unwrap();
        let meta = store
            .report_meta("daily", "2026-11-16", None)
            .unwrap()
            .unwrap();
        assert_eq!(meta.status, ReportStatus::Draft);
        assert_eq!(meta.current_version_id, Some(v2));

        assert!(store
            .set_report_status("daily", "2026-11-16", None, ReportStatus::Final)
            .unwrap());
        assert!(store
            .set_current_report_version("daily", "2026-11-16", None, v1)
            .unwrap());

        let meta = store
            .report_meta("daily", "2026-11-16", None)
            .unwrap()
            .unwrap();
        assert_eq!(meta.status, ReportStatus::Final);
        assert_eq!(meta.current_version_id, Some(v1));

        // Setting a non-existent version ID does not alter current_version_id.
        assert!(!store
            .set_current_report_version("daily", "2026-11-16", None, 9999)
            .unwrap());
    }

    #[test]
    fn period_reports_in_range_returns_current_versions_descending() {
        let mut store = Store::open_in_memory().unwrap();
        let pid = store.create_project("App", None).unwrap();

        let mut v_day1 = version("d1-v1", None);
        v_day1.period_key = "2026-11-14";
        store.save_report_version(&v_day1).unwrap();

        let mut v_day2_a = version("d2-v1", None);
        v_day2_a.period_key = "2026-11-15";
        store.save_report_version(&v_day2_a).unwrap();
        let mut v_day2_b = version("d2-v2", None);
        v_day2_b.period_key = "2026-11-15";
        store.save_report_version(&v_day2_b).unwrap();
        store
            .set_report_status("daily", "2026-11-15", None, ReportStatus::Final)
            .unwrap();

        let mut v_day3 = version("d3-v1", None);
        v_day3.period_key = "2026-11-16";
        store.save_report_version(&v_day3).unwrap();

        // Project-scoped reports and other report types must not appear.
        let mut v_weekly = version("weekly", None);
        v_weekly.report_type = "weekly";
        v_weekly.period_key = "2026-W47";
        store.save_report_version(&v_weekly).unwrap();
        let mut v_scoped = version("scoped", Some(pid));
        v_scoped.period_key = "2026-11-16";
        store.save_report_version(&v_scoped).unwrap();

        let all = store.period_reports_in_range("daily", None, None).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(
            (all[0].period_key.as_str(), all[0].markdown.as_str()),
            ("2026-11-16", "d3-v1")
        );
        assert_eq!(
            (
                all[1].period_key.as_str(),
                all[1].status,
                all[1].markdown.as_str()
            ),
            ("2026-11-15", ReportStatus::Final, "d2-v2")
        );
        assert_eq!(
            (all[2].period_key.as_str(), all[2].markdown.as_str()),
            ("2026-11-14", "d1-v1")
        );

        let bounded = store
            .period_reports_in_range("daily", Some("2026-11-15"), Some("2026-11-15"))
            .unwrap();
        assert_eq!(bounded.len(), 1);
        assert_eq!(bounded[0].period_key, "2026-11-15");
    }

    #[test]
    fn set_reports_status_updates_in_one_transaction_and_reports_skipped() {
        let mut store = Store::open_in_memory().unwrap();
        let mut d1 = version("d1", None);
        d1.period_key = "2026-11-16";
        store.save_report_version(&d1).unwrap();
        let mut d2 = version("d2", None);
        d2.period_key = "2026-11-17";
        store.save_report_version(&d2).unwrap();

        let keys = vec![
            "2026-11-16".to_string(),
            "2026-11-18".to_string(),
            "2026-11-17".to_string(),
            "2026-11-16".to_string(),
        ];
        let res = store
            .set_reports_status("daily", &keys, None, ReportStatus::Final)
            .unwrap();
        assert_eq!(res.updated, vec!["2026-11-16", "2026-11-17"]);
        assert_eq!(res.skipped, vec!["2026-11-18"]);
        assert_eq!(
            store
                .report_meta("daily", "2026-11-16", None)
                .unwrap()
                .unwrap()
                .status,
            ReportStatus::Final
        );
        assert_eq!(
            store
                .report_meta("daily", "2026-11-17", None)
                .unwrap()
                .unwrap()
                .status,
            ReportStatus::Final
        );

        let res_draft = store
            .set_reports_status(
                "daily",
                &["2026-11-16".to_string()],
                None,
                ReportStatus::Draft,
            )
            .unwrap();
        assert_eq!(res_draft.updated, vec!["2026-11-16"]);
        assert_eq!(
            store
                .report_meta("daily", "2026-11-16", None)
                .unwrap()
                .unwrap()
                .status,
            ReportStatus::Draft
        );
    }
}
