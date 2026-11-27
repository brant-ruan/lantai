//! Thin Tauri command exposing analytics dashboard data from `aj_core::stats`.

use aj_core::slice::local_date;
use aj_core::stats::{load_stats, StatsRange, StatsReport, TokenStats};
use aj_ir::Usage;
use chrono::{Local, Utc};
use serde::Serialize;

use crate::error::CmdResult;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsTrendSegmentDto {
    pub project_id: Option<i64>,
    pub name: String,
    pub active_minutes: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageDto {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub total_tokens: u64,
}

impl From<Usage> for TokenUsageDto {
    fn from(u: Usage) -> Self {
        Self {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            cache_read_tokens: u.cache_read_tokens,
            cache_write_tokens: u.cache_write_tokens,
            total_tokens: u.total(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenModelDto {
    pub agent: String,
    pub agent_name: String,
    pub model: Option<String>,
    pub calls: u64,
    pub usage: TokenUsageDto,
    pub share_pct: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenAgentDto {
    pub agent: String,
    pub name: String,
    pub calls: u64,
    pub usage: TokenUsageDto,
    pub share_pct: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenStatsDto {
    pub calls: u64,
    pub usage: TokenUsageDto,
    pub cache_hit_pct: f64,
    pub models: Vec<TokenModelDto>,
    pub agents: Vec<TokenAgentDto>,
}

impl From<TokenStats> for TokenStatsDto {
    fn from(t: TokenStats) -> Self {
        Self {
            calls: t.calls,
            usage: t.usage.into(),
            cache_hit_pct: t.cache_hit_pct,
            models: t
                .models
                .into_iter()
                .map(|m| TokenModelDto {
                    agent: m.agent.as_str().to_string(),
                    agent_name: m.agent.product_name().to_string(),
                    model: m.model,
                    calls: m.calls,
                    usage: m.usage.into(),
                    share_pct: m.share_pct,
                })
                .collect(),
            agents: t
                .agents
                .into_iter()
                .map(|a| TokenAgentDto {
                    agent: a.agent.as_str().to_string(),
                    name: a.agent.product_name().to_string(),
                    calls: a.calls,
                    usage: a.usage.into(),
                    share_pct: a.share_pct,
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsTrendBucketDto {
    pub key: String,
    pub label: String,
    pub date_range: String,
    pub active_minutes: i64,
    pub sessions: u64,
    pub top_project: Option<String>,
    pub segments: Vec<StatsTrendSegmentDto>,
    pub tokens: TokenUsageDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsProjectItemDto {
    pub project_id: Option<i64>,
    pub name: String,
    pub active_minutes: i64,
    pub share_pct: f64,
    pub active_days: u64,
    pub sessions: u64,
    pub turns: u64,
    pub files_changed: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsAgentItemDto {
    pub agent: String,
    pub name: String,
    pub active_minutes: i64,
    pub share_pct: f64,
    pub sessions: u64,
    pub turns: u64,
    pub files_changed: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsToolCategoryDto {
    pub key: String,
    pub label: String,
    pub calls: u64,
    pub errors: u64,
    pub share_pct: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsHourlyBucketDto {
    pub hour: u8,
    pub active_minutes: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsReportDto {
    pub range: String,
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
    pub trend_granularity: String,
    pub trend: Vec<StatsTrendBucketDto>,
    pub projects: Vec<StatsProjectItemDto>,
    pub agents: Vec<StatsAgentItemDto>,
    pub total_tool_calls: u64,
    pub tools: Vec<StatsToolCategoryDto>,
    pub hourly: Vec<StatsHourlyBucketDto>,
    pub peak_hour: Option<u8>,
    pub late_night_pct: f64,
    pub tokens: TokenStatsDto,
}

impl From<StatsReport> for StatsReportDto {
    fn from(r: StatsReport) -> Self {
        Self {
            range: r.range.as_str().to_string(),
            since_date: r.since_date,
            until_date: r.until_date,
            range_days: r.range_days,
            active_minutes: r.active_minutes,
            active_days: r.active_days,
            current_streak_days: r.current_streak_days,
            longest_streak_days: r.longest_streak_days,
            sessions: r.sessions,
            slices: r.slices,
            turns: r.turns,
            files_changed: r.files_changed,
            lines_added: r.lines_added,
            lines_removed: r.lines_removed,
            trend_granularity: r.trend_granularity.as_str().to_string(),
            trend: r
                .trend
                .into_iter()
                .map(|b| StatsTrendBucketDto {
                    key: b.key,
                    label: b.label,
                    date_range: b.date_range,
                    active_minutes: b.active_minutes,
                    sessions: b.sessions,
                    top_project: b.top_project,
                    segments: b
                        .segments
                        .into_iter()
                        .map(|s| StatsTrendSegmentDto {
                            project_id: s.project_id,
                            name: s.name,
                            active_minutes: s.active_minutes,
                        })
                        .collect(),
                    tokens: b.tokens.into(),
                })
                .collect(),
            projects: r
                .projects
                .into_iter()
                .map(|p| StatsProjectItemDto {
                    project_id: p.project_id,
                    name: p.name,
                    active_minutes: p.active_minutes,
                    share_pct: p.share_pct,
                    active_days: p.active_days,
                    sessions: p.sessions,
                    turns: p.turns,
                    files_changed: p.files_changed,
                })
                .collect(),
            agents: r
                .agents
                .into_iter()
                .map(|a| StatsAgentItemDto {
                    agent: a.agent.as_str().to_string(),
                    name: a.agent.product_name().to_string(),
                    active_minutes: a.active_minutes,
                    share_pct: a.share_pct,
                    sessions: a.sessions,
                    turns: a.turns,
                    files_changed: a.files_changed,
                })
                .collect(),
            total_tool_calls: r.total_tool_calls,
            tools: r
                .tools
                .into_iter()
                .map(|t| StatsToolCategoryDto {
                    key: t.key.to_string(),
                    label: t.label.to_string(),
                    calls: t.calls,
                    errors: t.errors,
                    share_pct: t.share_pct,
                })
                .collect(),
            hourly: r
                .hourly
                .into_iter()
                .map(|h| StatsHourlyBucketDto {
                    hour: h.hour,
                    active_minutes: h.active_minutes,
                })
                .collect(),
            peak_hour: r.peak_hour,
            late_night_pct: r.late_night_pct,
            tokens: r.tokens.into(),
        }
    }
}

#[tauri::command(async)]
pub fn get_stats(
    state: tauri::State<'_, AppState>,
    range: String,
    include_ignored: Option<bool>,
) -> CmdResult<StatsReportDto> {
    let parsed_range =
        StatsRange::parse(&range).ok_or_else(|| format!("unknown stats range: {range}"))?;
    let store = state.open_store()?;
    let today = local_date(Utc::now(), &Local, state.core_config()?.day_boundary_hour);
    let report = load_stats(
        &store,
        parsed_range,
        include_ignored.unwrap_or(false),
        today,
        &Local,
    )?;
    Ok(report.into())
}
