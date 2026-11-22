//! Pure aggregation and loader for the analytics dashboard.
//!
//! Computes deduplicated active time (via per-day interval union), cross-agent canonical
//! file counts, streaks, period trends, project/agent/tool breakdowns, 24-hour local rhythm,
//! and token usage (see [`tokens`]).

mod tokens;

pub use tokens::{TokenAgentStat, TokenModelStat, TokenStats};

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use aj_ir::{AgentId, ToolKind, Usage};
use aj_store::{StatsSliceRow, Store, TokenAggregateRow, ToolAggregateRow};
use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Timelike, Utc};

use crate::slice::union_minutes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsRange {
    Days7,
    Days30,
    Days90,
    All,
}

impl StatsRange {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "7d" => Some(Self::Days7),
            "30d" => Some(Self::Days30),
            "90d" => Some(Self::Days90),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Days7 => "7d",
            Self::Days30 => "30d",
            Self::Days90 => "90d",
            Self::All => "all",
        }
    }

    pub fn since_date(self, today: NaiveDate) -> Option<NaiveDate> {
        match self {
            Self::Days7 => Some(today - Duration::days(6)),
            Self::Days30 => Some(today - Duration::days(29)),
            Self::Days90 => Some(today - Duration::days(89)),
            Self::All => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendGranularity {
    Day,
    Week,
}

impl TrendGranularity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatsReport {
    pub range: StatsRange,
    pub since_date: Option<String>,
    pub until_date: String,
    pub range_days: u64,
    pub active_minutes: i64,
    pub active_days: u64,
    pub current_streak_days: u64,
    pub longest_streak_days: u64,
    pub sessions: u64,
    pub slices: u64,
    pub turns: u64,
    pub files_changed: u64,
    pub lines_added: u64,
    pub lines_removed: u64,
    pub trend_granularity: TrendGranularity,
    pub trend: Vec<TrendBucket>,
    pub projects: Vec<ProjectStat>,
    pub agents: Vec<AgentStat>,
    pub total_tool_calls: u64,
    pub tools: Vec<ToolCategoryStat>,
    pub hourly: Vec<HourlyBucket>,
    pub peak_hour: Option<u8>,
    pub late_night_pct: f64,
    pub tokens: TokenStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrendSegment {
    pub project_id: Option<i64>,
    pub name: String,
    pub active_minutes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrendBucket {
    pub key: String,
    pub label: String,
    pub date_range: String,
    pub active_minutes: i64,
    pub sessions: u64,
    pub top_project: Option<String>,
    pub segments: Vec<TrendSegment>,
    /// Token usage of the bucket's days.
    pub tokens: Usage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectStat {
    pub project_id: Option<i64>,
    pub name: String,
    pub active_minutes: i64,
    pub share_pct: f64,
    pub active_days: u64,
    pub sessions: u64,
    pub turns: u64,
    pub files_changed: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentStat {
    pub agent: AgentId,
    pub active_minutes: i64,
    pub share_pct: f64,
    pub sessions: u64,
    pub turns: u64,
    pub files_changed: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCategoryStat {
    pub key: &'static str,
    pub label: &'static str,
    pub calls: u64,
    pub errors: u64,
    pub share_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HourlyBucket {
    pub hour: u8,
    pub active_minutes: i64,
}

/// Load statistics from SQLite for the given range and local day boundary.
pub fn load_stats<Tz: TimeZone>(
    store: &Store,
    range: StatsRange,
    include_ignored: bool,
    today: NaiveDate,
    tz: &Tz,
) -> Result<StatsReport> {
    let since_str = range.since_date(today).map(|d| d.to_string());
    let slices = store.stats_slices(since_str.as_deref(), None, include_ignored)?;
    let tools = store.stats_tool_aggregates(since_str.as_deref(), None, include_ignored)?;
    let token_rows = store.stats_token_aggregates(since_str.as_deref(), None, include_ignored)?;
    let mut report = compute_stats(&slices, &tools, &token_rows, range, today, tz);
    // Streaks describe the user's habit, not the selected window: a 7-day view must not cap them at 7.
    if range != StatsRange::All {
        let dates: BTreeSet<NaiveDate> = store
            .stats_active_dates(include_ignored)?
            .iter()
            .filter_map(|d| d.parse().ok())
            .collect();
        (report.current_streak_days, report.longest_streak_days) = compute_streaks(&dates, today);
    }
    Ok(report)
}

type Interval = (DateTime<Utc>, DateTime<Utc>);
type IntervalsByDay = BTreeMap<NaiveDate, Vec<Interval>>;
type ProjectIntervals = BTreeMap<(Option<i64>, String), IntervalsByDay>;

#[derive(Default)]
struct ProjectAcc {
    name: String,
    by_day: IntervalsByDay,
    sessions: BTreeSet<i64>,
    turns: u64,
    files: BTreeSet<String>,
}

#[derive(Default)]
struct AgentAcc {
    by_day: IntervalsByDay,
    sessions: BTreeSet<i64>,
    turns: u64,
    files: BTreeSet<String>,
}

#[derive(Default)]
struct BucketAcc {
    by_day: IntervalsByDay,
    sessions: BTreeSet<i64>,
    project_by_day: ProjectIntervals,
}

impl BucketAcc {
    fn push(&mut self, date: NaiveDate, s: &StatsSliceRow) {
        self.by_day
            .entry(date)
            .or_default()
            .extend(s.active_intervals.iter().copied());
        self.sessions.insert(s.session_id);
        let pname = s
            .project_name
            .clone()
            .unwrap_or_else(|| "Unassigned".to_string());
        self.project_by_day
            .entry((s.project_id, pname))
            .or_default()
            .entry(date)
            .or_default()
            .extend(s.active_intervals.iter().copied());
    }
}

/// Compute the full analytics summary from pre-loaded slice rows and tool aggregates.
pub fn compute_stats<Tz: TimeZone>(
    slices: &[StatsSliceRow],
    tools: &[ToolAggregateRow],
    token_rows: &[TokenAggregateRow],
    range: StatsRange,
    today: NaiveDate,
    tz: &Tz,
) -> StatsReport {
    let mut all_by_day: IntervalsByDay = BTreeMap::new();
    let mut all_sessions: BTreeSet<i64> = BTreeSet::new();
    let mut all_files: BTreeSet<String> = BTreeSet::new();
    let mut total_turns: u64 = 0;

    let mut project_accs: HashMap<Option<i64>, ProjectAcc> = HashMap::new();
    let mut agent_accs: HashMap<AgentId, AgentAcc> = HashMap::new();
    let mut all_intervals: Vec<Interval> = Vec::new();
    let mut parsed_slices: Vec<(NaiveDate, &StatsSliceRow)> = Vec::with_capacity(slices.len());

    for s in slices {
        let Ok(date) = s.local_date.parse::<NaiveDate>() else {
            continue;
        };
        parsed_slices.push((date, s));
        total_turns += s.turn_count;
        all_sessions.insert(s.session_id);

        let files = canonical_files(s);
        all_files.extend(files.iter().cloned());

        all_by_day
            .entry(date)
            .or_default()
            .extend(s.active_intervals.iter().copied());
        all_intervals.extend(s.active_intervals.iter().copied());

        let proj_name = s
            .project_name
            .clone()
            .unwrap_or_else(|| "Unassigned".to_string());
        let p_acc = project_accs
            .entry(s.project_id)
            .or_insert_with(|| ProjectAcc {
                name: proj_name,
                ..Default::default()
            });
        p_acc
            .by_day
            .entry(date)
            .or_default()
            .extend(s.active_intervals.iter().copied());
        p_acc.sessions.insert(s.session_id);
        p_acc.turns += s.turn_count;
        p_acc.files.extend(files.iter().cloned());

        let a_acc = agent_accs.entry(s.agent.clone()).or_default();
        a_acc
            .by_day
            .entry(date)
            .or_default()
            .extend(s.active_intervals.iter().copied());
        a_acc.sessions.insert(s.session_id);
        a_acc.turns += s.turn_count;
        a_acc.files.extend(files);
    }

    let active_dates: BTreeSet<NaiveDate> = all_by_day.keys().copied().collect();
    let active_days = active_dates.len() as u64;
    let active_minutes = sum_daily_union_minutes(&all_by_day);
    let (current_streak_days, longest_streak_days) = compute_streaks(&active_dates, today);

    let (since_date, range_days) = match range.since_date(today) {
        Some(start) => {
            let span = (today - start).num_days().max(0) as u64 + 1;
            (Some(start.to_string()), span)
        }
        None => match active_dates.iter().next().copied() {
            Some(earliest) => {
                let end = today.max(earliest);
                let span = (end - earliest).num_days().max(0) as u64 + 1;
                (Some(earliest.to_string()), span)
            }
            None => (None, 0),
        },
    };

    let (trend_granularity, mut trend) = compute_trend(&parsed_slices, range, today);
    add_trend_tokens(&mut trend, trend_granularity, token_rows);

    let projects = build_projects(project_accs);
    let agents = build_agents(agent_accs);
    let (total_tool_calls, lines_added, lines_removed, tool_cats) = build_tools(tools);
    let (hourly, peak_hour, late_night_pct) = compute_hourly_rhythm(&all_intervals, tz);

    StatsReport {
        range,
        since_date,
        until_date: today.to_string(),
        range_days,
        active_minutes,
        active_days,
        current_streak_days,
        longest_streak_days,
        sessions: all_sessions.len() as u64,
        slices: parsed_slices.len() as u64,
        turns: total_turns,
        files_changed: all_files.len() as u64,
        lines_added,
        lines_removed,
        trend_granularity,
        trend,
        projects,
        agents,
        total_tool_calls,
        tools: tool_cats,
        hourly,
        peak_hour,
        late_night_pct,
        tokens: tokens::compute_token_stats(token_rows),
    }
}

/// Add each day's token usage to the trend bucket covering it; days outside the buckets are dropped.
fn add_trend_tokens(
    trend: &mut [TrendBucket],
    granularity: TrendGranularity,
    rows: &[TokenAggregateRow],
) {
    let index: HashMap<String, usize> = trend
        .iter()
        .enumerate()
        .map(|(i, b)| (b.key.clone(), i))
        .collect();
    for r in rows {
        let Ok(date) = r.local_date.parse::<NaiveDate>() else {
            continue;
        };
        let key = match granularity {
            TrendGranularity::Day => date.to_string(),
            TrendGranularity::Week => week_key(monday_of(date)),
        };
        if let Some(&i) = index.get(&key) {
            tokens::add_usage(&mut trend[i].tokens, &r.usage);
        }
    }
}

fn canonical_files(slice: &StatsSliceRow) -> BTreeSet<String> {
    let mut roots: Vec<&str> = Vec::new();
    if let Some(cwd) = slice.detected_cwd.as_deref().filter(|s| !s.is_empty()) {
        roots.push(cwd);
    }
    if let Some(root) = slice.git_root.as_deref().filter(|s| !s.is_empty()) {
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    let git_prefix = slice
        .git_root
        .as_deref()
        .map(|r| r.trim_end_matches('/'))
        .filter(|r| !r.is_empty());

    slice
        .files_modified
        .iter()
        .map(|f| {
            let rel = roots.iter().find_map(|r| {
                f.strip_prefix(r.trim_end_matches('/'))
                    .and_then(|rest| rest.strip_prefix('/'))
            });
            match (rel, git_prefix) {
                (Some(rel), Some(gr)) => format!("{gr}/{rel}"),
                _ => f.clone(),
            }
        })
        .collect()
}

fn sum_daily_union_minutes(by_day: &IntervalsByDay) -> i64 {
    by_day.values().map(|iv| union_minutes(iv.clone())).sum()
}

fn compute_streaks(active_dates: &BTreeSet<NaiveDate>, today: NaiveDate) -> (u64, u64) {
    let mut longest = 0u64;
    let mut run = 0u64;
    let mut prev: Option<NaiveDate> = None;
    for &d in active_dates {
        if prev.is_some_and(|p| d == p + Duration::days(1)) {
            run += 1;
        } else {
            run = 1;
        }
        longest = longest.max(run);
        prev = Some(d);
    }

    let mut cur_date = if active_dates.contains(&today) {
        Some(today)
    } else {
        let yest = today - Duration::days(1);
        active_dates.contains(&yest).then_some(yest)
    };
    let mut current = 0u64;
    while let Some(d) = cur_date {
        if active_dates.contains(&d) {
            current += 1;
            cur_date = Some(d - Duration::days(1));
        } else {
            break;
        }
    }
    (current, longest)
}

fn monday_of(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

fn week_key(monday: NaiveDate) -> String {
    let iso = monday.iso_week();
    format!("{:04}-W{:02}", iso.year(), iso.week())
}

fn compute_trend(
    slices: &[(NaiveDate, &StatsSliceRow)],
    range: StatsRange,
    today: NaiveDate,
) -> (TrendGranularity, Vec<TrendBucket>) {
    let (granularity, start_date) = match range {
        StatsRange::Days7 => (TrendGranularity::Day, today - Duration::days(6)),
        StatsRange::Days30 => (TrendGranularity::Day, today - Duration::days(29)),
        StatsRange::Days90 => (TrendGranularity::Week, today - Duration::days(89)),
        StatsRange::All => {
            let Some(earliest) = slices.iter().map(|(d, _)| *d).min() else {
                return (TrendGranularity::Day, Vec::new());
            };
            let gran = if (today - earliest).num_days() <= 34 {
                TrendGranularity::Day
            } else {
                TrendGranularity::Week
            };
            (gran, earliest)
        }
    };

    match granularity {
        TrendGranularity::Day => {
            let mut by_key: HashMap<NaiveDate, BucketAcc> = HashMap::new();
            for &(date, s) in slices {
                by_key.entry(date).or_default().push(date, s);
            }

            let end_date = today.max(start_date);
            let mut out = Vec::new();
            let mut d = start_date;
            while d <= end_date {
                let (active_minutes, sessions, segments) = match by_key.get(&d) {
                    Some(acc) => (
                        sum_daily_union_minutes(&acc.by_day),
                        acc.sessions.len() as u64,
                        build_trend_segments(&acc.project_by_day),
                    ),
                    None => (0, 0, Vec::new()),
                };
                let top_project = segments.first().map(|s| s.name.clone());
                out.push(TrendBucket {
                    key: d.to_string(),
                    label: d.format("%m-%d").to_string(),
                    date_range: format!("{d} ({})", d.format("%a")),
                    active_minutes,
                    sessions,
                    top_project,
                    segments,
                    tokens: Usage::default(),
                });
                d += Duration::days(1);
            }
            (TrendGranularity::Day, out)
        }
        TrendGranularity::Week => {
            let mut by_monday: HashMap<NaiveDate, BucketAcc> = HashMap::new();
            for &(date, s) in slices {
                let mon = monday_of(date);
                by_monday.entry(mon).or_default().push(date, s);
            }

            let start_mon = monday_of(start_date);
            let end_mon = monday_of(today).max(start_mon);
            let mut out = Vec::new();
            let mut mon = start_mon;
            while mon <= end_mon {
                let sun = mon + Duration::days(6);
                let iso = mon.iso_week();
                let (active_minutes, sessions, segments) = match by_monday.get(&mon) {
                    Some(acc) => (
                        sum_daily_union_minutes(&acc.by_day),
                        acc.sessions.len() as u64,
                        build_trend_segments(&acc.project_by_day),
                    ),
                    None => (0, 0, Vec::new()),
                };
                let top_project = segments.first().map(|s| s.name.clone());
                out.push(TrendBucket {
                    key: week_key(mon),
                    label: format!("W{:02}", iso.week()),
                    date_range: format!("{} ~ {}", mon.format("%m-%d"), sun.format("%m-%d")),
                    active_minutes,
                    sessions,
                    top_project,
                    segments,
                    tokens: Usage::default(),
                });
                mon += Duration::days(7);
            }
            (TrendGranularity::Week, out)
        }
    }
}

fn build_trend_segments(by_proj: &ProjectIntervals) -> Vec<TrendSegment> {
    let mut segs: Vec<TrendSegment> = by_proj
        .iter()
        .filter_map(|((project_id, name), days)| {
            let active_minutes = sum_daily_union_minutes(days);
            (active_minutes > 0).then(|| TrendSegment {
                project_id: *project_id,
                name: name.clone(),
                active_minutes,
            })
        })
        .collect();
    segs.sort_by(|a, b| {
        b.active_minutes
            .cmp(&a.active_minutes)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.project_id.cmp(&b.project_id))
    });
    segs
}

fn build_projects(accs: HashMap<Option<i64>, ProjectAcc>) -> Vec<ProjectStat> {
    let mut rows: Vec<(Option<i64>, ProjectAcc, i64)> = accs
        .into_iter()
        .map(|(id, acc)| {
            let mins = sum_daily_union_minutes(&acc.by_day);
            (id, acc, mins)
        })
        .collect();
    let total_mins: i64 = rows.iter().map(|(_, _, m)| *m).sum();
    rows.sort_by(|(_, acc_a, ma), (_, acc_b, mb)| {
        mb.cmp(ma)
            .then_with(|| acc_b.sessions.len().cmp(&acc_a.sessions.len()))
            .then_with(|| acc_a.name.cmp(&acc_b.name))
    });
    rows.into_iter()
        .map(|(project_id, acc, active_minutes)| ProjectStat {
            project_id,
            name: acc.name,
            active_minutes,
            share_pct: pct(active_minutes as f64, total_mins as f64),
            active_days: acc.by_day.len() as u64,
            sessions: acc.sessions.len() as u64,
            turns: acc.turns,
            files_changed: acc.files.len() as u64,
        })
        .collect()
}

fn build_agents(accs: HashMap<AgentId, AgentAcc>) -> Vec<AgentStat> {
    let mut rows: Vec<(AgentId, AgentAcc, i64)> = accs
        .into_iter()
        .map(|(agent, acc)| {
            let mins = sum_daily_union_minutes(&acc.by_day);
            (agent, acc, mins)
        })
        .collect();
    let total_mins: i64 = rows.iter().map(|(_, _, m)| *m).sum();
    rows.sort_by(|(aa, acc_a, ma), (ab, acc_b, mb)| {
        mb.cmp(ma)
            .then_with(|| acc_b.sessions.len().cmp(&acc_a.sessions.len()))
            .then_with(|| aa.as_str().cmp(ab.as_str()))
    });
    rows.into_iter()
        .map(|(agent, acc, active_minutes)| AgentStat {
            agent,
            active_minutes,
            share_pct: pct(active_minutes as f64, total_mins as f64),
            sessions: acc.sessions.len() as u64,
            turns: acc.turns,
            files_changed: acc.files.len() as u64,
        })
        .collect()
}

fn build_tools(rows: &[ToolAggregateRow]) -> (u64, u64, u64, Vec<ToolCategoryStat>) {
    const CATEGORIES: [(&str, &str); 5] = [
        ("code", "Edit & Write"),
        ("shell", "Shell Commands"),
        ("read", "File Reads"),
        ("web", "Web Search & Fetch"),
        ("other", "Subagent, MCP & Other"),
    ];
    let mut calls = [0u64; 5];
    let mut errors = [0u64; 5];
    let mut lines_added = 0u64;
    let mut lines_removed = 0u64;

    for row in rows {
        let idx = match row.tool_kind {
            ToolKind::FileEdit | ToolKind::FileWrite => {
                lines_added += row.added;
                lines_removed += row.removed;
                0
            }
            ToolKind::BashCmd => 1,
            ToolKind::FileRead => 2,
            ToolKind::WebSearch | ToolKind::WebFetch => 3,
            ToolKind::Subagent | ToolKind::Mcp | ToolKind::Other => 4,
        };
        calls[idx] += row.calls;
        errors[idx] += row.errors;
    }

    let total_calls: u64 = calls.iter().sum();
    let stats = CATEGORIES
        .iter()
        .enumerate()
        .map(|(i, &(key, label))| ToolCategoryStat {
            key,
            label,
            calls: calls[i],
            errors: errors[i],
            share_pct: pct(calls[i] as f64, total_calls as f64),
        })
        .collect();
    (total_calls, lines_added, lines_removed, stats)
}

fn compute_hourly_rhythm<Tz: TimeZone>(
    raw_intervals: &[(DateTime<Utc>, DateTime<Utc>)],
    tz: &Tz,
) -> (Vec<HourlyBucket>, Option<u8>, f64) {
    let mut spans: Vec<(DateTime<Utc>, DateTime<Utc>)> = raw_intervals
        .iter()
        .map(|&(s, e)| (s, e.max(s + Duration::minutes(1))))
        .collect();
    spans.sort();

    let mut merged: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for (s, e) in spans {
        match merged.last_mut() {
            Some((_, end)) if s <= *end => *end = (*end).max(e),
            _ => merged.push((s, e)),
        }
    }

    let mut hour_secs = [0i64; 24];
    for (s, e) in merged {
        let mut cur = s;
        while cur < e {
            let local_cur = cur.with_timezone(tz);
            let hour = local_cur.hour() as usize;
            let secs_into_hour = (local_cur.minute() * 60 + local_cur.second()) as i64;
            let step_secs = (3600 - secs_into_hour).max(1);
            let next_boundary = cur + Duration::seconds(step_secs);
            let step_end = e.min(next_boundary);
            if step_end <= cur {
                break;
            }
            hour_secs[hour] += (step_end - cur).num_seconds().max(0);
            cur = step_end;
        }
    }

    let hourly: Vec<HourlyBucket> = (0..24u8)
        .map(|h| {
            let secs = hour_secs[h as usize];
            let active_minutes = if secs == 0 {
                0
            } else {
                ((secs + 30) / 60).max(1)
            };
            HourlyBucket {
                hour: h,
                active_minutes,
            }
        })
        .collect();

    let peak_hour = hourly
        .iter()
        .filter(|b| b.active_minutes > 0)
        .max_by(|a, b| match a.active_minutes.cmp(&b.active_minutes) {
            Ordering::Equal => b.hour.cmp(&a.hour),
            ord => ord,
        })
        .map(|b| b.hour);

    let total_hourly: i64 = hourly.iter().map(|b| b.active_minutes).sum();
    let late_night: i64 = hourly
        .iter()
        .filter(|b| matches!(b.hour, 22 | 23 | 0..=5))
        .map(|b| b.active_minutes)
        .sum();
    let late_night_pct = pct(late_night as f64, total_hourly as f64);

    (hourly, peak_hour, late_night_pct)
}

fn pct(part: f64, total: f64) -> f64 {
    if total <= 0.0 {
        0.0
    } else {
        (part / total) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn slice(
        id: i64,
        session_id: i64,
        local_date: &str,
        agent: AgentId,
        project: Option<(i64, &str)>,
        cwd: Option<&str>,
        git_root: Option<&str>,
        turns: u64,
        intervals: Vec<(&str, &str)>,
        files: Vec<&str>,
    ) -> StatsSliceRow {
        StatsSliceRow {
            slice_id: id,
            session_id,
            local_date: local_date.into(),
            agent,
            project_id: project.map(|(pid, _)| pid),
            project_name: project.map(|(_, name)| name.to_string()),
            detected_cwd: cwd.map(str::to_string),
            git_root: git_root.map(str::to_string),
            turn_count: turns,
            active_intervals: intervals
                .into_iter()
                .map(|(a, b)| (utc(a), utc(b)))
                .collect(),
            files_modified: files.into_iter().map(str::to_string).collect(),
            ignored: false,
        }
    }

    #[test]
    fn deduplicates_parallel_sessions_and_worktree_files() {
        let tz = FixedOffset::east_opt(8 * 3600).unwrap();
        let today = d("2026-11-18");
        let slices = vec![
            // Claude in main repo: 10:00..11:00 UTC+8 (02:00..03:00Z), edits src/a.rs
            slice(
                1,
                10,
                "2026-11-18",
                AgentId::CLAUDE,
                Some((1, "App")),
                Some("/work/app"),
                Some("/work/app"),
                4,
                vec![("2026-11-18T02:00:00Z", "2026-11-18T03:00:00Z")],
                vec!["/work/app/src/a.rs", "/work/app/src/b.rs"],
            ),
            // Codex in worktree: 10:30..11:30 UTC+8 (02:30..03:30Z), also edits src/a.rs
            slice(
                2,
                20,
                "2026-11-18",
                AgentId::CODEX,
                Some((1, "App")),
                Some("/work/app/.claude/worktrees/wt1"),
                Some("/work/app"),
                3,
                vec![("2026-11-18T02:30:00Z", "2026-11-18T03:30:00Z")],
                vec![
                    "/work/app/.claude/worktrees/wt1/src/a.rs",
                    "/work/app/.claude/worktrees/wt1/src/c.rs",
                ],
            ),
            // Antigravity in a different repo editing a file with the same relative path src/a.rs
            slice(
                3,
                30,
                "2026-11-18",
                AgentId::ANTIGRAVITY,
                Some((2, "Other")),
                Some("/work/other"),
                Some("/work/other"),
                2,
                vec![("2026-11-18T04:00:00Z", "2026-11-18T04:20:00Z")],
                vec!["/work/other/src/a.rs"],
            ),
        ];
        let report = compute_stats(&slices, &[], &[], StatsRange::Days7, today, &tz);
        // 02:00..03:30 (90m) + 04:00..04:20 (20m) = 110m union.
        assert_eq!(report.active_minutes, 110);
        // App has a.rs, b.rs, c.rs (3); Other has /work/other/src/a.rs (1) => 4 distinct files.
        assert_eq!(report.files_changed, 4);
        assert_eq!(report.sessions, 3);
        assert_eq!(report.turns, 9);
        assert_eq!(report.projects[0].name, "App");
        assert_eq!(report.projects[0].active_minutes, 90);
        assert_eq!(report.projects[0].files_changed, 3);
        assert_eq!(report.projects[1].name, "Other");
        assert_eq!(report.projects[1].active_minutes, 20);
        assert_eq!(report.projects[1].files_changed, 1);
        assert_eq!(report.agents.len(), 3);
        let day_bucket = report.trend.last().unwrap();
        assert_eq!(
            day_bucket.segments,
            vec![
                TrendSegment {
                    project_id: Some(1),
                    name: "App".into(),
                    active_minutes: 90,
                },
                TrendSegment {
                    project_id: Some(2),
                    name: "Other".into(),
                    active_minutes: 20,
                },
            ]
        );
    }

    #[test]
    fn computes_streaks_across_today_and_gaps() {
        let dates_a: BTreeSet<NaiveDate> = [
            "2026-11-10",
            "2026-11-11",
            "2026-11-12",
            "2026-11-13",
            "2026-11-16",
            "2026-11-17",
        ]
        .into_iter()
        .map(d)
        .collect();
        // Today (11-18) idle, yesterday (11-17) active -> current streak 2, longest 4.
        assert_eq!(compute_streaks(&dates_a, d("2026-11-18")), (2, 4));
        // Today (11-19) and yesterday (11-18) both idle -> current streak 0, longest 4.
        assert_eq!(compute_streaks(&dates_a, d("2026-11-19")), (0, 4));
        // Today active (11-17) -> current streak 2.
        assert_eq!(compute_streaks(&dates_a, d("2026-11-17")), (2, 4));
    }

    #[test]
    fn trend_zero_fills_days_and_switches_to_weeks_on_long_ranges() {
        let tz = Utc;
        let today = d("2026-11-18");
        let slices = vec![
            slice(
                1,
                1,
                "2026-11-13",
                AgentId::CLAUDE,
                Some((1, "Alpha")),
                None,
                None,
                1,
                vec![("2026-11-13T10:00:00Z", "2026-11-13T10:15:00Z")],
                vec![],
            ),
            slice(
                2,
                2,
                "2026-11-18",
                AgentId::CLAUDE,
                Some((2, "Beta")),
                None,
                None,
                1,
                vec![("2026-11-18T10:00:00Z", "2026-11-18T10:30:00Z")],
                vec![],
            ),
        ];

        let r7 = compute_stats(&slices, &[], &[], StatsRange::Days7, today, &tz);
        assert_eq!(r7.trend_granularity, TrendGranularity::Day);
        assert_eq!(r7.trend.len(), 7);
        assert_eq!(r7.trend[0].key, "2026-11-12");
        assert_eq!(r7.trend[0].active_minutes, 0);
        assert_eq!(r7.trend[1].key, "2026-11-13");
        assert_eq!(r7.trend[1].active_minutes, 15);
        assert_eq!(r7.trend[1].top_project.as_deref(), Some("Alpha"));
        assert_eq!(r7.trend[6].key, "2026-11-18");
        assert_eq!(r7.trend[6].active_minutes, 30);

        let r90 = compute_stats(&slices, &[], &[], StatsRange::Days90, today, &tz);
        assert_eq!(r90.trend_granularity, TrendGranularity::Week);
        assert!(r90
            .trend
            .iter()
            .any(|b| b.key == "2026-W47" && b.active_minutes == 30));

        let long_slices = vec![
            slice(
                1,
                1,
                "2026-10-01",
                AgentId::CLAUDE,
                None,
                None,
                None,
                1,
                vec![("2026-10-01T10:00:00Z", "2026-10-01T10:10:00Z")],
                vec![],
            ),
            slice(
                2,
                2,
                "2026-11-18",
                AgentId::CLAUDE,
                None,
                None,
                None,
                1,
                vec![("2026-11-18T10:00:00Z", "2026-11-18T10:10:00Z")],
                vec![],
            ),
        ];
        let r_all = compute_stats(&long_slices, &[], &[], StatsRange::All, today, &tz);
        assert_eq!(r_all.trend_granularity, TrendGranularity::Week);
        assert_eq!(r_all.since_date.as_deref(), Some("2026-10-01"));
    }

    #[test]
    fn groups_tools_and_splits_hourly_rhythm_in_local_tz() {
        let tz = FixedOffset::east_opt(8 * 3600).unwrap();
        let today = d("2026-11-18");
        // 22:45..23:30 local (14:45..15:30Z), 01:00..01:15 local (17:00..17:15Z),
        // 05:00..05:15 local (21:00..21:15Z) in night window (22..=05); 06:00..06:20 local (22:00..22:20Z) outside.
        let slices = vec![slice(
            1,
            1,
            "2026-11-18",
            AgentId::CLAUDE,
            None,
            None,
            None,
            2,
            vec![
                ("2026-11-18T14:45:00Z", "2026-11-18T15:30:00Z"),
                ("2026-11-17T17:00:00Z", "2026-11-17T17:15:00Z"),
                ("2026-11-17T21:00:00Z", "2026-11-17T21:15:00Z"),
                ("2026-11-17T22:00:00Z", "2026-11-17T22:20:00Z"),
            ],
            vec![],
        )];
        let tools = vec![
            ToolAggregateRow {
                tool_kind: ToolKind::FileEdit,
                calls: 5,
                errors: 0,
                added: 40,
                removed: 12,
            },
            ToolAggregateRow {
                tool_kind: ToolKind::FileWrite,
                calls: 2,
                errors: 0,
                added: 30,
                removed: 0,
            },
            ToolAggregateRow {
                tool_kind: ToolKind::BashCmd,
                calls: 10,
                errors: 2,
                added: 0,
                removed: 0,
            },
            ToolAggregateRow {
                tool_kind: ToolKind::WebSearch,
                calls: 3,
                errors: 0,
                added: 0,
                removed: 0,
            },
        ];

        let r = compute_stats(&slices, &tools, &[], StatsRange::Days7, today, &tz);
        assert_eq!(r.total_tool_calls, 20);
        assert_eq!((r.lines_added, r.lines_removed), (70, 12));
        assert_eq!(r.tools.len(), 5);
        assert_eq!((r.tools[0].key, r.tools[0].calls), ("code", 7));
        assert_eq!(
            (r.tools[1].key, r.tools[1].calls, r.tools[1].errors),
            ("shell", 10, 2)
        );
        assert_eq!((r.tools[3].key, r.tools[3].calls), ("web", 3));

        assert_eq!(r.hourly.len(), 24);
        assert_eq!(r.hourly[22].active_minutes, 15);
        assert_eq!(r.hourly[23].active_minutes, 30);
        assert_eq!(r.hourly[1].active_minutes, 15);
        assert_eq!(r.hourly[5].active_minutes, 15);
        assert_eq!(r.hourly[6].active_minutes, 20);
        assert_eq!(r.peak_hour, Some(23));
        // Night window (22..=05): 15 + 30 + 15 + 15 = 75 min out of 95 total min.
        assert!((r.late_night_pct - (75.0 / 95.0) * 100.0).abs() < 1e-6);
    }

    #[test]
    fn empty_stats_produce_clean_zero_baseline() {
        let r = compute_stats(&[], &[], &[], StatsRange::All, d("2026-11-18"), &Utc);
        assert_eq!(r.active_minutes, 0);
        assert_eq!(r.active_days, 0);
        assert_eq!(r.range_days, 0);
        assert!(r.since_date.is_none());
        assert!(r.trend.is_empty());
        assert!(r.projects.is_empty());
        assert!(r.agents.is_empty());
        assert_eq!(r.tools.len(), 5);
        assert_eq!(r.hourly.len(), 24);
        assert_eq!(r.peak_hour, None);
        assert_eq!(r.late_night_pct, 0.0);
    }

    #[test]
    fn trend_buckets_carry_token_usage() {
        let today = d("2026-11-18");
        let row = |date: &str, input| TokenAggregateRow {
            local_date: date.into(),
            agent: AgentId::CLAUDE,
            model: None,
            calls: 1,
            usage: Usage {
                input_tokens: input,
                ..Usage::default()
            },
        };
        let rows = [
            row("2026-11-17", 10),
            row("2026-11-17", 5),
            row("2026-11-01", 99),
        ];
        let r = compute_stats(&[], &[], &rows, StatsRange::Days7, today, &Utc);
        let by_key: HashMap<&str, u64> = r
            .trend
            .iter()
            .map(|b| (b.key.as_str(), b.tokens.input_tokens))
            .collect();
        assert_eq!(by_key["2026-11-17"], 15);
        assert_eq!(by_key["2026-11-18"], 0);
        // A row outside the range's buckets still counts in the totals the store filtered for.
        assert_eq!(r.tokens.usage.input_tokens, 114);
    }
}
