//! Thin Tauri command exposing Lantai's own state (report coverage, digest cache) from
//! `aj_report::self_stats`. Loaded apart from `get_stats`, since it collects every day's material.

use aj_core::slice::local_date;
use aj_core::stats::StatsRange;
use aj_report::material::MaterialContext;
use aj_report::self_stats::{load_self_stats, DigestTally, KindTotals, PeriodCell, SelfStats};
use chrono::{Local, Utc};
use serde::Serialize;

use crate::state::{cmd_err, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodCellDto {
    pub key: String,
    pub first_day: String,
    pub last_day: String,
    pub status: &'static str,
    pub stale: bool,
    pub stale_projects: Vec<String>,
    pub versions: u64,
    pub edited: bool,
}

impl From<PeriodCell> for PeriodCellDto {
    fn from(c: PeriodCell) -> Self {
        Self {
            key: c.key,
            first_day: c.first_day.to_string(),
            last_day: c.last_day.to_string(),
            status: c.status.as_str(),
            stale: c.stale,
            stale_projects: c.stale_projects,
            versions: c.versions,
            edited: c.edited,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindTotalsDto {
    pub periods: u64,
    pub reports: u64,
    pub finals: u64,
    pub stale: u64,
    pub edited: u64,
}

impl From<KindTotals> for KindTotalsDto {
    fn from(t: KindTotals) -> Self {
        Self {
            periods: t.periods,
            reports: t.reports,
            finals: t.finals,
            stale: t.stale,
            edited: t.edited,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DigestTallyDto {
    pub fresh: u64,
    pub behind: u64,
    pub missing: u64,
    pub ignored: u64,
}

impl From<DigestTally> for DigestTallyDto {
    fn from(t: DigestTally) -> Self {
        Self {
            fresh: t.fresh,
            behind: t.behind,
            missing: t.missing,
            ignored: t.ignored,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayDigestDto {
    pub date: String,
    pub tally: DigestTallyDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDigestDto {
    pub agent: String,
    pub name: String,
    pub tally: DigestTallyDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LantaiStatsDto {
    pub from: String,
    pub to: String,
    pub days: Vec<PeriodCellDto>,
    pub weeks: Vec<PeriodCellDto>,
    pub months: Vec<PeriodCellDto>,
    pub daily: KindTotalsDto,
    pub weekly: KindTotalsDto,
    pub monthly: KindTotalsDto,
    pub digest_days: Vec<DayDigestDto>,
    pub digest: DigestTallyDto,
    pub agents: Vec<AgentDigestDto>,
    pub split_slices: u64,
    pub incremental_updates: u64,
    pub project_briefs: u64,
    /// Engine the "reusable" check was made for (digests are cached per engine).
    pub engine: String,
}

fn cells(v: Vec<PeriodCell>) -> Vec<PeriodCellDto> {
    v.into_iter().map(Into::into).collect()
}

fn to_dto(s: SelfStats, engine: String) -> LantaiStatsDto {
    LantaiStatsDto {
        from: s.from.to_string(),
        to: s.to.to_string(),
        days: cells(s.days),
        weeks: cells(s.weeks),
        months: cells(s.months),
        daily: s.daily.into(),
        weekly: s.weekly.into(),
        monthly: s.monthly.into(),
        digest_days: s
            .digest_days
            .into_iter()
            .map(|d| DayDigestDto {
                date: d.date,
                tally: d.tally.into(),
            })
            .collect(),
        digest: s.digest.into(),
        agents: s
            .agents
            .into_iter()
            .map(|a| AgentDigestDto {
                agent: a.agent.as_str().to_string(),
                name: a.agent.display_name().to_string(),
                tally: a.tally.into(),
            })
            .collect(),
        split_slices: s.split_slices,
        incremental_updates: s.incremental_updates,
        project_briefs: s.project_briefs,
        engine,
    }
}

#[tauri::command(async)]
pub fn get_lantai_stats(
    state: tauri::State<'_, AppState>,
    range: String,
) -> Result<LantaiStatsDto, String> {
    let parsed =
        StatsRange::parse(&range).ok_or_else(|| format!("unknown stats range: {range}"))?;
    let store = state.open_store().map_err(cmd_err)?;
    let cfg = state.load_config().map_err(cmd_err)?;
    let today = local_date(
        Utc::now(),
        &Local,
        state.core_config().map_err(cmd_err)?.day_boundary_hour,
    );
    // Without a usable default model nothing counts as reusable, which is also what generating would find.
    let engine = cfg
        .build_engine(None, &state.data_dir.join("runner"))
        .map(|e| e.name())
        .unwrap_or_default();
    let budgets = cfg.budgets();
    let ctx = MaterialContext {
        engine_name: &engine,
        budgets: &budgets,
        now: Utc::now(),
        language: state.report_language(&[]).map_err(cmd_err)?,
    };
    let stats =
        load_self_stats(&store, &Local, parsed.since_date(today), today, &ctx).map_err(cmd_err)?;
    Ok(to_dto(stats, engine))
}
