//! Sidebar batch commands for planning report work and setting report status.

use aj_report::{batch, rollup::Granularity};
use aj_store::Store;
use chrono::Local;
use serde::Serialize;

use crate::state::{cmd_err, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchItemDto {
    pub period_key: String,
    pub has_inputs: bool,
    pub missing_weeks: Vec<String>,
    pub status: String,
    pub stale: bool,
    pub action: String,
    pub can_regenerate: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchPlanDto {
    pub items: Vec<BatchItemDto>,
    pub missing_count: usize,
    pub stale_count: usize,
    pub regenerate_count: usize,
    pub batch_concurrency: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchStatusResultDto {
    pub updated: Vec<String>,
    pub skipped: Vec<String>,
}

#[tauri::command]
pub async fn plan_batch(
    state: tauri::State<'_, AppState>,
    report_type: String,
    period_keys: Vec<String>,
    include_final: bool,
) -> Result<BatchPlanDto, String> {
    let cfg = state.load_config().map_err(cmd_err)?;
    let data_dir = state.data_dir.clone();
    let granularity = Granularity::parse(&report_type).map_err(cmd_err)?;
    let language = state.report_language(&[]).map_err(cmd_err)?;
    tauri::async_runtime::spawn_blocking(move || {
        let store = Store::open(&data_dir.join(crate::state::DB_FILE)).map_err(cmd_err)?;
        let budgets = cfg.budgets();
        let plan = batch::plan_batch(
            &store,
            &Local,
            granularity,
            &period_keys,
            include_final,
            &budgets,
            language,
        )
        .map_err(cmd_err)?;
        Ok(BatchPlanDto {
            items: plan
                .items
                .into_iter()
                .map(|item| BatchItemDto {
                    period_key: item.period_key,
                    has_inputs: item.has_inputs,
                    missing_weeks: item.missing_weeks,
                    status: item.status.as_str().to_string(),
                    stale: item.stale,
                    action: item.action.as_str().to_string(),
                    can_regenerate: item.can_regenerate,
                })
                .collect(),
            missing_count: plan.missing_count,
            stale_count: plan.stale_count,
            regenerate_count: plan.regenerate_count,
            batch_concurrency: plan.batch_concurrency,
        })
    })
    .await
    .map_err(|e| format!("batch plan task failed: {e}"))?
}

#[tauri::command(async)]
pub fn set_reports_status(
    state: tauri::State<'_, AppState>,
    report_type: String,
    period_keys: Vec<String>,
    status: String,
) -> Result<BatchStatusResultDto, String> {
    let kind = Granularity::parse(&report_type)
        .map_err(cmd_err)?
        .report_kind();
    let mut store = state.open_store().map_err(cmd_err)?;
    let res = store
        .set_reports_status(kind, &period_keys, None, &status)
        .map_err(cmd_err)?;
    Ok(BatchStatusResultDto {
        updated: res.updated,
        skipped: res.skipped,
    })
}
