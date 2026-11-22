//! Project progress rollups: one project's sections taken from the period reports (daily,
//! weekly or monthly) within a date range, the periods people wrote notes about the project
//! in, and the active periods that have no report yet.

use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap};

use aj_store::Store;
use anyhow::{bail, Result};
use chrono::NaiveDate;

use crate::human;
use crate::period::{Month, Week};
use crate::prompt_library::{KIND_DAILY, KIND_MONTHLY, KIND_WEEKLY};
use crate::structure::{build_project_rollup, extract_project_section};

/// The period a rollup is summarised by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Daily,
    Weekly,
    Monthly,
}

impl Granularity {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "daily" => Ok(Self::Daily),
            "weekly" => Ok(Self::Weekly),
            "monthly" => Ok(Self::Monthly),
            other => {
                bail!("unknown rollup granularity {other:?}; expected daily, weekly, or monthly")
            }
        }
    }

    /// The report kind whose sections the rollup is made of.
    pub fn report_kind(self) -> &'static str {
        match self {
            Self::Daily => KIND_DAILY,
            Self::Weekly => KIND_WEEKLY,
            Self::Monthly => KIND_MONTHLY,
        }
    }

    /// The period key a date belongs to.
    pub fn period_of(self, date: NaiveDate) -> String {
        match self {
            Self::Daily => date.to_string(),
            Self::Weekly => Week::containing(date).key(),
            Self::Monthly => Week::containing(date).month().key(),
        }
    }

    /// The period key of a `YYYY-MM-DD` bound; `None` when absent or unparsable.
    pub(crate) fn bound(self, date: Option<&str>) -> Option<String> {
        date.and_then(|d| d.parse::<NaiveDate>().ok())
            .map(|d| self.period_of(d))
    }

    /// A readable label: the date, `2026-W47 · 11-16 ~ 11-22`, or `2026-11 · W45 ~ W48`.
    pub fn label(self, key: &str) -> String {
        match self {
            Self::Daily => key.to_string(),
            Self::Weekly => match Week::parse(key) {
                Ok(w) => w.title().trim_start_matches("# ").to_string(),
                Err(_) => key.to_string(),
            },
            Self::Monthly => match Month::parse(key) {
                Ok(m) => m.title().trim_start_matches("# ").to_string(),
                Err(_) => key.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollupEntry {
    pub period_key: String,
    pub label: String,
    /// The report's status; empty when the period has notes but no section of the project.
    pub status: String,
    pub markdown: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRollup {
    /// Newest period first.
    pub entries: Vec<RollupEntry>,
    /// Periods where the project was active but no report covers it, newest first.
    pub unreported: Vec<String>,
    pub combined_markdown: String,
}

/// Collect the project's sections from the `granularity` reports between the two dates
/// (inclusive, either open). `range_label` names the range in the combined title.
pub fn project_rollup(
    store: &Store,
    project_id: i64,
    project_name: &str,
    granularity: Granularity,
    since_date: Option<&str>,
    until_date: Option<&str>,
    range_label: &str,
) -> Result<ProjectRollup> {
    let since_key = granularity.bound(since_date);
    let until_key = granularity.bound(until_date);
    let mut entries = Vec::new();
    let mut reported = BTreeSet::new();
    for r in store.period_reports_in_range(
        granularity.report_kind(),
        since_key.as_deref(),
        until_key.as_deref(),
    )? {
        if let Some(body) = extract_project_section(&r.markdown, project_name) {
            reported.insert(r.period_key.clone());
            entries.push(RollupEntry {
                label: granularity.label(&r.period_key),
                period_key: r.period_key,
                status: r.status,
                markdown: body,
                updated_at: r.created_at,
            });
        }
    }
    add_note_periods(
        &mut entries,
        &store.human_nodes_of_project(project_id)?,
        granularity,
        since_key.as_deref(),
        until_key.as_deref(),
    );

    let mut active = BTreeSet::new();
    let mut sessions = HashMap::new();
    for slice in store.slices_for_project(project_id, since_date, until_date)? {
        let session = match sessions.entry(slice.session_id) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(store.session(slice.session_id)?),
        };
        let ignored = session
            .as_ref()
            .is_none_or(|session| slice.is_ignored(session));
        if ignored {
            continue;
        }
        if let Ok(date) = slice.local_date.parse::<NaiveDate>() {
            active.insert(granularity.period_of(date));
        }
    }
    let unreported = active
        .into_iter()
        .rev()
        .filter(|k| !reported.contains(k))
        .collect();

    let pairs: Vec<(String, String)> = entries
        .iter()
        .map(|e| (e.label.clone(), e.markdown.clone()))
        .collect();
    let combined_markdown = build_project_rollup(project_name, range_label, &pairs);
    Ok(ProjectRollup {
        entries,
        unreported,
        combined_markdown,
    })
}

/// Add entries for the periods in range where the project has notes but no section, so the
/// project page can show (and edit) them; keep the entries newest first. The notes themselves
/// come from the human journal ([`crate::human::project`]).
fn add_note_periods(
    entries: &mut Vec<RollupEntry>,
    nodes: &[aj_store::HumanNodeRow],
    granularity: Granularity,
    since_key: Option<&str>,
    until_key: Option<&str>,
) {
    let kind = human::period_kind(granularity.report_kind()).unwrap_or_default();
    let keys: BTreeSet<&str> = nodes
        .iter()
        .filter(|n| n.period_kind == kind)
        .map(|n| n.period_key.as_str())
        .filter(|k| since_key.is_none_or(|s| *k >= s) && until_key.is_none_or(|u| *k <= u))
        .collect();
    for key in keys {
        if !entries.iter().any(|e| e.period_key == key) {
            entries.push(RollupEntry {
                period_key: key.to_string(),
                label: granularity.label(key),
                status: String::new(),
                markdown: String::new(),
                updated_at: String::new(),
            });
        }
    }
    entries.sort_by(|a, b| b.period_key.cmp(&a.period_key));
}

#[cfg(test)]
mod tests {
    use super::*;
    use aj_core::{slice::reslice_session, Config};
    use aj_ir::{AgentKind, IrEvent};
    use aj_store::NewReportVersion;
    use chrono::FixedOffset;

    fn save(store: &mut Store, kind: &str, key: &str, markdown: &str) {
        store
            .save_report_version(&NewReportVersion {
                report_type: kind,
                period_key: key,
                project_id: None,
                origin: "generated",
                markdown,
                engine: None,
                input_hash: None,
                prompt_id: None,
                prompt_name: None,
                prompt_snapshot: None,
                input_manifest: None,
                language: "zh-Hans",
            })
            .unwrap();
    }

    fn fixture() -> (Store, i64) {
        let tz = FixedOffset::east_opt(8 * 3600).unwrap();
        let mut store = Store::open_in_memory().unwrap();
        let app = store.create_project("app", None).unwrap();
        for (sid, at) in [("a", "2026-11-10T02:00:00Z"), ("b", "2026-11-17T02:00:00Z")] {
            let id = store
                .ingest(
                    AgentKind::Claude,
                    sid,
                    &[IrEvent::TurnStart {
                        source_event_id: "u1".into(),
                        timestamp: at.parse().unwrap(),
                        user_prompt: "work".into(),
                    }],
                )
                .unwrap()
                .session_id;
            store.assign_manual(id, Some(app)).unwrap();
            reslice_session(&mut store, id, &Config::default(), &tz).unwrap();
        }
        save(
            &mut store,
            KIND_DAILY,
            "2026-11-10",
            "# d\n\n## app\n- daily line",
        );
        save(
            &mut store,
            KIND_WEEKLY,
            "2026-W46",
            "# w\n\n## app\n> stats\n- weekly line\n\n## other\n- x",
        );
        (store, app)
    }

    #[test]
    fn weekly_rollup_takes_week_sections_and_lists_unreported_weeks() {
        let (store, app) = fixture();
        let r = project_rollup(&store, app, "app", Granularity::Weekly, None, None, "all").unwrap();
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].period_key, "2026-W46");
        assert_eq!(r.entries[0].label, "2026-W46 \u{00b7} 11-09 ~ 11-15");
        assert!(r.entries[0].markdown.contains("- weekly line"));
        assert_eq!(r.unreported, vec!["2026-W47".to_string()]);
        assert!(r
            .combined_markdown
            .starts_with("# app \u{00b7} all\n\n## 2026-W46 \u{00b7} 11-09 ~ 11-15\n"));

        // A date bound inside a week still includes that week's report.
        let bounded = project_rollup(
            &store,
            app,
            "app",
            Granularity::Weekly,
            Some("2026-11-12"),
            None,
            "x",
        )
        .unwrap();
        assert_eq!(bounded.entries.len(), 1);
    }

    #[test]
    fn daily_rollup_lists_note_only_days() {
        let (mut store, app) = fixture();
        let note = |store: &mut Store, key: &str, project: Option<i64>, body: &str| {
            store
                .insert_human_root(&aj_store::NewHumanRoot {
                    period_kind: "day",
                    period_key: key,
                    project_id: project,
                    attach_json: None,
                    position: aj_store::HumanPosition::Bottom,
                    sort_key: "V",
                    body,
                })
                .unwrap()
        };
        let root = note(&mut store, "2026-11-10", Some(app), "called the vendor");
        store.insert_human_child(root, "V", "they agreed").unwrap();
        note(&mut store, "2026-11-12", Some(app), "design review");
        note(&mut store, "2026-11-12", None, "a day note");
        note(&mut store, "2026-11-20", Some(app), "out of range");

        let r = project_rollup(
            &store,
            app,
            "app",
            Granularity::Daily,
            None,
            Some("2026-11-18"),
            "x",
        )
        .unwrap();
        let got: Vec<_> = r
            .entries
            .iter()
            .map(|e| (e.period_key.as_str(), e.status.as_str()))
            .collect();
        assert_eq!(got, vec![("2026-11-12", ""), ("2026-11-10", "draft")]);
        assert!(!r.combined_markdown.contains("vendor"));
    }

    #[test]
    fn daily_rollup_takes_day_sections() {
        let (store, app) = fixture();
        let r = project_rollup(&store, app, "app", Granularity::Daily, None, None, "all").unwrap();
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].label, "2026-11-10");
        assert_eq!(r.unreported, vec!["2026-11-17".to_string()]);
    }

    #[test]
    fn monthly_rollup_takes_month_sections_and_lists_unreported_months() {
        let tz = FixedOffset::east_opt(8 * 3600).unwrap();
        let (mut store, app) = fixture();
        // 2026-11-30 is Monday of 2026-W49 (Thursday 2026-12-03 -> December).
        let id = store
            .ingest(
                AgentKind::Claude,
                "c",
                &[IrEvent::TurnStart {
                    source_event_id: "u1".into(),
                    timestamp: "2026-11-30T02:00:00Z".parse().unwrap(),
                    user_prompt: "december work".into(),
                }],
            )
            .unwrap()
            .session_id;
        store.assign_manual(id, Some(app)).unwrap();
        reslice_session(&mut store, id, &Config::default(), &tz).unwrap();

        save(
            &mut store,
            KIND_MONTHLY,
            "2026-11",
            "# m\n\n## app\n> stats\n- monthly line\n\n## other\n- x",
        );

        let r =
            project_rollup(&store, app, "app", Granularity::Monthly, None, None, "all").unwrap();
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].period_key, "2026-11");
        assert_eq!(r.entries[0].label, "2026-11 \u{00b7} W45 ~ W48");
        assert!(r.entries[0].markdown.contains("- monthly line"));
        assert_eq!(r.unreported, vec!["2026-12".to_string()]);
        assert!(r
            .combined_markdown
            .starts_with("# app \u{00b7} all\n\n## 2026-11 \u{00b7} W45 ~ W48\n"));
    }
}
