//! Model settings commands: providers (connections and keys), model aliases,
//! the default model, model discovery, local agent CLIs, connectivity tests and
//! diagnosing engine errors.
//!
//! Thin wrappers over `aj_report::AppConfig`; every change is a locked
//! load-modify-save of `config.toml`. Keys flow in only (when the user types
//! one), go straight to the key store, and are never returned to the frontend.

use std::time::Instant;

use aj_report::diagnose::{self, Failure};
use aj_report::local_tools::{self, Gap, ToolEngine};
use aj_report::presets::{self, PresetGroup, PresetRegion};
use aj_report::{AppConfig, KeyUpdate, ModelConfig, ModelInfo, ProviderConfig};
use serde::{Deserialize, Serialize};

use crate::state::{cmd_err, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDto {
    pub name: String,
    /// `openai_compat`, `claude_cli` or `codex_cli`.
    pub kind: String,
    pub base_url: Option<String>,
    pub timeout_secs: Option<u64>,
    pub binary: Option<String>,
    /// Whether a key is in the key store (always false for agent CLIs).
    pub has_key: bool,
    pub used_by: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDto {
    pub alias: String,
    pub provider: String,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub temperature: Option<f64>,
    pub is_default: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSettingsDto {
    pub config_path: String,
    pub key_store: String,
    /// `None` when no model is configured.
    pub default_model: Option<String>,
    pub providers: Vec<ProviderDto>,
    pub models: Vec<ModelDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    pub kind: String,
    pub base_url: Option<String>,
    pub timeout_secs: Option<u64>,
    pub binary: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInput {
    pub provider: String,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub temperature: Option<f64>,
}

/// `keep` leaves the stored key alone, `set` replaces it with `value`, `clear` removes it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", tag = "mode", content = "value")]
pub enum KeyUpdateInput {
    Keep,
    Set(String),
    Clear,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTestResultDto {
    pub ok: bool,
    pub elapsed_ms: u64,
    pub message: String,
    /// The known case a failure belongs to.
    pub failure: Option<Failure>,
}

impl ProviderInput {
    fn into_config(self) -> Result<ProviderConfig, String> {
        ProviderConfig::from_parts(&self.kind, self.base_url, self.timeout_secs, self.binary)
            .map_err(cmd_err)
    }
}

fn provider_dto(cfg: &AppConfig, name: &str, p: &ProviderConfig) -> ProviderDto {
    let used_by = cfg
        .models
        .iter()
        .filter(|(_, m)| m.provider == name)
        .map(|(alias, _)| alias.clone())
        .collect();
    let (base_url, timeout_secs) = match p {
        ProviderConfig::OpenaiCompat {
            base_url,
            timeout_secs,
        } => (Some(base_url.clone()), *timeout_secs),
        _ => (None, None),
    };
    ProviderDto {
        name: name.to_string(),
        kind: p.kind().to_string(),
        base_url,
        timeout_secs,
        binary: p.cli_binary().map(str::to_string),
        has_key: cfg.has_key(name),
        used_by,
    }
}

fn settings_dto(state: &AppState, cfg: &AppConfig) -> ModelSettingsDto {
    ModelSettingsDto {
        config_path: state.config_path().to_string_lossy().into_owned(),
        key_store: cfg.keystore().describe(),
        default_model: cfg.default_model_name().map(str::to_string),
        providers: cfg
            .providers
            .iter()
            .map(|(name, p)| provider_dto(cfg, name, p))
            .collect(),
        models: cfg
            .models
            .iter()
            .map(|(alias, m)| ModelDto {
                alias: alias.clone(),
                provider: m.provider.clone(),
                model: m.model.clone(),
                reasoning_effort: m.reasoning_effort.clone(),
                temperature: m.temperature,
                is_default: Some(alias.as_str()) == cfg.default_model_name(),
            })
            .collect(),
    }
}

/// Load, apply `edit`, save, and return the new settings.
fn modify(
    state: &AppState,
    edit: impl FnOnce(&mut AppConfig) -> anyhow::Result<()>,
) -> Result<ModelSettingsDto, String> {
    let cfg = state.modify_config(edit).map_err(cmd_err)?;
    Ok(settings_dto(state, &cfg))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPresetDto {
    pub id: String,
    pub name: String,
    pub group: PresetGroup,
    pub region: Option<PresetRegion>,
    pub base_url: String,
    pub efforts: Option<Vec<String>>,
}

/// The built-in provider presets (`aj_report::presets`), in display order.
#[tauri::command]
pub fn provider_presets() -> Vec<ProviderPresetDto> {
    presets::provider_presets()
        .iter()
        .map(|p| ProviderPresetDto {
            id: p.id.clone(),
            name: p.name.clone(),
            group: p.group,
            region: p.region,
            base_url: p.base_url.clone(),
            efforts: p.efforts.clone(),
        })
        .collect()
}

#[tauri::command]
pub fn model_settings(state: tauri::State<'_, AppState>) -> Result<ModelSettingsDto, String> {
    let cfg = state.load_config().map_err(cmd_err)?;
    Ok(settings_dto(&state, &cfg))
}

#[tauri::command]
pub fn save_provider(
    state: tauri::State<'_, AppState>,
    original_name: Option<String>,
    name: String,
    provider: ProviderInput,
    key: KeyUpdateInput,
) -> Result<ModelSettingsDto, String> {
    let provider = provider.into_config()?;
    let key = match key {
        KeyUpdateInput::Keep => KeyUpdate::Keep,
        KeyUpdateInput::Set(v) => KeyUpdate::Set(v),
        KeyUpdateInput::Clear => KeyUpdate::Clear,
    };
    modify(&state, |cfg| {
        cfg.upsert_provider(original_name.as_deref(), &name, provider, key)
    })
}

#[tauri::command]
pub fn delete_provider(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<ModelSettingsDto, String> {
    modify(&state, |cfg| cfg.remove_provider(&name))
}

#[tauri::command]
pub fn save_model(
    state: tauri::State<'_, AppState>,
    original_alias: Option<String>,
    alias: String,
    model: ModelInput,
) -> Result<ModelSettingsDto, String> {
    let model = ModelConfig {
        provider: model.provider,
        model: model.model,
        reasoning_effort: model.reasoning_effort,
        temperature: model.temperature,
    };
    modify(&state, |cfg| {
        cfg.upsert_model(original_alias.as_deref(), &alias, model)
    })
}

#[tauri::command]
pub fn delete_model(
    state: tauri::State<'_, AppState>,
    alias: String,
) -> Result<ModelSettingsDto, String> {
    modify(&state, |cfg| cfg.remove_model(&alias))
}

#[tauri::command]
pub fn set_default_model(
    state: tauri::State<'_, AppState>,
    alias: String,
) -> Result<ModelSettingsDto, String> {
    modify(&state, |cfg| cfg.set_default_model(&alias))
}

/// List the models a provider offers. `provider` is the form's current
/// (possibly unsaved) state; `api_key` is a key typed but not yet saved.
#[tauri::command]
pub async fn list_provider_models(
    state: tauri::State<'_, AppState>,
    saved_name: Option<String>,
    provider: ProviderInput,
    api_key: Option<String>,
) -> Result<Vec<ModelInfo>, String> {
    let cfg = state.load_config().map_err(cmd_err)?;
    let draft = provider.into_config()?;
    tauri::async_runtime::spawn_blocking(move || {
        cfg.list_provider_models(saved_name.as_deref(), draft, api_key.as_deref())
            .map_err(cmd_err)
    })
    .await
    .map_err(|e| format!("model listing task failed: {e}"))?
}

#[tauri::command]
pub async fn test_model(
    state: tauri::State<'_, AppState>,
    alias: String,
) -> Result<EngineTestResultDto, String> {
    let cfg = state.load_config().map_err(cmd_err)?;
    let runner_dir = state.data_dir.join("runner");
    tauri::async_runtime::spawn_blocking(move || {
        let engine = cfg
            .build_engine(Some(&alias), &runner_dir)
            .map_err(cmd_err)?;
        let started = Instant::now();
        Ok(match engine.complete("Reply with exactly one word: OK") {
            Ok(reply) => EngineTestResultDto {
                ok: true,
                elapsed_ms: started.elapsed().as_millis() as u64,
                message: reply.trim().chars().take(120).collect(),
                failure: None,
            },
            Err(e) => {
                let message = format!("{e:#}");
                EngineTestResultDto {
                    ok: false,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    failure: diagnose::diagnose(&message),
                    message,
                }
            }
        })
    })
    .await
    .map_err(|e| format!("model test task failed: {e}"))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalToolDto {
    /// Command name; also what `add_local_tool` takes.
    pub command: String,
    pub agent: String,
    pub product: String,
    pub path: String,
    pub version: Option<String>,
    /// `None` when the tool cannot generate reports; `gaps` says why.
    pub engine: Option<ToolEngine>,
    pub gaps: Vec<Gap>,
    /// The model alias already using this exact command, if any.
    pub configured_as: Option<String>,
}

/// Look for known agent CLIs on PATH (runs only their version commands).
#[tauri::command]
pub async fn find_local_tools(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<LocalToolDto>, String> {
    let cfg = state.load_config().map_err(cmd_err)?;
    let found = tauri::async_runtime::spawn_blocking(local_tools::find_on_path)
        .await
        .map_err(|e| format!("tool search failed: {e}"))?;
    Ok(found
        .into_iter()
        .map(|f| {
            let path = f.path.to_string_lossy().into_owned();
            let configured_as = cfg
                .providers
                .iter()
                .find(|(_, p)| p.cli_binary() == Some(path.as_str()))
                .and_then(|(name, _)| {
                    cfg.models
                        .iter()
                        .find(|(_, m)| m.provider == *name)
                        .map(|(alias, _)| alias.clone())
                });
            LocalToolDto {
                command: f.tool.command.to_string(),
                agent: f.tool.agent.as_str().to_string(),
                product: f.tool.agent.product_name().to_string(),
                path,
                version: f.version,
                engine: f.tool.engine,
                gaps: f.tool.gaps.to_vec(),
                configured_as,
            }
        })
        .collect())
}

/// Add a local agent CLI the user confirmed. It is looked up again so only a tool actually
/// found on PATH is written to `config.toml`.
#[tauri::command]
pub async fn add_local_tool(
    state: tauri::State<'_, AppState>,
    command: String,
) -> Result<ModelSettingsDto, String> {
    let found = tauri::async_runtime::spawn_blocking(local_tools::find_on_path)
        .await
        .map_err(|e| format!("tool search failed: {e}"))?;
    let found = found
        .into_iter()
        .find(|f| f.tool.command == command)
        .ok_or_else(|| format!("`{command}` was not found on PATH"))?;
    modify(&state, |cfg| {
        cfg.add_local_tool(found.tool, &found.path).map(|_| ())
    })
}

/// The known case an engine error message belongs to, for hints in the interface.
#[tauri::command]
pub fn diagnose_error(message: String) -> Option<Failure> {
    diagnose::diagnose(&message)
}
