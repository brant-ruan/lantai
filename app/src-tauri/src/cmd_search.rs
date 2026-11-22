//! Thin Tauri command exposing `aj_report::search` to the Search view.

use aj_core::search::{FieldSnippet, SnippetPart};
use aj_ir::AgentId;
use aj_report::search::{search, ReportRef, SearchLayer, SearchRequest, DEFAULT_TURN_LIMIT};
use aj_store::SliceFilter;
use serde::{Deserialize, Serialize};

use crate::state::{cmd_err, AppState};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchParams {
    pub query: String,
    /// `all` | `reports` | `sessions`
    pub layer: String,
    pub since: Option<String>,
    pub until: Option<String>,
    pub project_id: Option<i64>,
    pub agent: Option<String>,
    pub include_ignored: bool,
    pub include_tool_output: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetPartDto {
    pub text: String,
    pub hit: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldSnippetDto {
    pub field: &'static str,
    pub parts: Vec<SnippetPartDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRefDto {
    pub kind: String,
    pub period_key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportHitDto {
    pub kind: String,
    pub period_key: String,
    pub status: String,
    pub section: Option<String>,
    pub snippets: Vec<Vec<SnippetPartDto>>,
    pub match_count: usize,
    /// A note people wrote rather than a report section.
    pub human: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnHitDto {
    pub turn_index: i64,
    pub started_at: String,
    pub snippets: Vec<FieldSnippetDto>,
    pub match_count: usize,
    pub cited_by: Vec<ReportRefDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceHitDto {
    pub slice_id: i64,
    pub local_date: String,
    pub session_id: i64,
    pub agent: String,
    pub agent_name: String,
    pub title: Option<String>,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub ignored: bool,
    pub title_snippets: Vec<FieldSnippetDto>,
    pub turns: Vec<TurnHitDto>,
    pub match_count: usize,
    /// Reports citing the whole slice.
    pub cited_by: Vec<ReportRefDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultsDto {
    pub terms: Vec<String>,
    pub reports: Vec<ReportHitDto>,
    pub reports_truncated: bool,
    pub sessions: Vec<SliceHitDto>,
    pub sessions_truncated: bool,
    pub elapsed_ms: u64,
}

fn parts(p: Vec<SnippetPart>) -> Vec<SnippetPartDto> {
    p.into_iter()
        .map(|p| SnippetPartDto {
            text: p.text,
            hit: p.hit,
        })
        .collect()
}

fn field_snippets(list: Vec<FieldSnippet>) -> Vec<FieldSnippetDto> {
    list.into_iter()
        .map(|s| FieldSnippetDto {
            field: s.field.as_str(),
            parts: parts(s.parts),
        })
        .collect()
}

fn refs(list: Vec<ReportRef>) -> Vec<ReportRefDto> {
    list.into_iter()
        .map(|r| ReportRefDto {
            kind: r.kind,
            period_key: r.period_key,
        })
        .collect()
}

#[tauri::command(async)]
pub fn search_all(
    state: tauri::State<'_, AppState>,
    params: SearchParams,
) -> Result<SearchResultsDto, String> {
    let agent = match params.agent.as_deref() {
        Some(key) => Some(AgentId::parse(key).map_err(|e| e.to_string())?),
        None => None,
    };
    let req = SearchRequest {
        query: params.query,
        layer: SearchLayer::parse(&params.layer).map_err(cmd_err)?,
        filter: SliceFilter {
            since: params.since,
            until: params.until,
            project_id: params.project_id,
            agent,
            include_ignored: params.include_ignored,
        },
        include_tool_output: params.include_tool_output,
        turn_limit: DEFAULT_TURN_LIMIT,
    };
    let started = std::time::Instant::now();
    let store = state.open_store().map_err(cmd_err)?;
    let res = search(&store, &req).map_err(cmd_err)?;

    let sessions = res
        .sessions
        .slices
        .iter()
        .map(|g| {
            let s = &g.slice;
            SliceHitDto {
                slice_id: s.slice_id,
                local_date: s.local_date.clone(),
                session_id: s.session_id,
                agent: s.agent.to_string(),
                agent_name: s.agent.product_name().to_string(),
                title: s.title.clone(),
                project_id: s.project_id,
                project_name: s.project_name.clone(),
                ignored: s.ignored,
                title_snippets: field_snippets(g.title_snippets.clone()),
                turns: g
                    .turns
                    .iter()
                    .map(|t| TurnHitDto {
                        turn_index: t.turn_index,
                        started_at: t.started_at.clone(),
                        snippets: field_snippets(t.snippets.clone()),
                        match_count: t.match_count,
                        cited_by: refs(res.cited_by(s.slice_id, Some(t.turn_index))),
                    })
                    .collect(),
                match_count: g.match_count,
                cited_by: refs(res.cited_by(s.slice_id, None)),
            }
        })
        .collect();
    let reports = res
        .reports
        .into_iter()
        .map(|h| ReportHitDto {
            kind: h.report.kind,
            period_key: h.report.period_key,
            status: h.status,
            section: h.section,
            snippets: h.snippets.into_iter().map(parts).collect(),
            match_count: h.match_count,
            human: h.human,
        })
        .collect();
    Ok(SearchResultsDto {
        terms: res.terms,
        reports,
        reports_truncated: res.reports_truncated,
        sessions,
        sessions_truncated: res.sessions.truncated,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}
