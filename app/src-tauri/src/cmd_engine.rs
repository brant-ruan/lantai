//! Model settings commands: providers (connections and keys), model aliases,
//! the default model, model discovery and connectivity tests.
//!
//! Thin wrappers over `aj_report::AppConfig`; every change is a locked
//! load-modify-save of `config.toml`. Keys flow in only (when the user types
//! one), go straight to the key store, and are never returned to the frontend.

use std::time::Instant;

use aj_report::config::BUILTIN;
use aj_report::presets::{self, PresetGroup, PresetRegion};
use aj_report::{AppConfig, KeyUpdate, ModelConfig, ProviderConfig};
use serde::{Deserialize, Serialize};

use crate::state::{cmd_err, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDto {
    pub name: String,
    /// `openai_compat` or `claude_cli`.
    pub kind: String,
    pub base_url: Option<String>,
    pub timeout_secs: Option<u64>,
    pub binary: Option<String>,
    /// Whether a key is in the key store (always false for the CLI provider).
    pub has_key: bool,
    pub is_builtin: bool,
    pub used_by: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDto {
    pub alias: String,
    pub provider: String,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub is_default: bool,
    pub is_builtin: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSettingsDto {
    pub config_path: String,
    pub key_store: String,
    pub default_model: String,
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
}

impl ProviderInput {
    fn into_config(self) -> Result<ProviderConfig, String> {
        Ok(match self.kind.as_str() {
            "claude_cli" => ProviderConfig::ClaudeCli {
                binary: self.binary,
            },
            "openai_compat" => ProviderConfig::OpenaiCompat {
                base_url: self.base_url.unwrap_or_default(),
                timeout_secs: self.timeout_secs,
            },
            other => return Err(format!("unknown provider type: {other}")),
        })
    }
}

fn provider_dto(cfg: &AppConfig, name: &str, p: &ProviderConfig) -> ProviderDto {
    let used_by = cfg
        .models
        .iter()
        .filter(|(_, m)| m.provider == name)
        .map(|(alias, _)| alias.clone())
        .collect();
    let base = ProviderDto {
        name: name.to_string(),
        kind: String::new(),
        base_url: None,
        timeout_secs: None,
        binary: None,
        has_key: cfg.has_key(name),
        is_builtin: name == BUILTIN,
        used_by,
    };
    match p {
        ProviderConfig::ClaudeCli { binary } => ProviderDto {
            kind: "claude_cli".into(),
            binary: binary.clone(),
            ..base
        },
        ProviderConfig::OpenaiCompat {
            base_url,
            timeout_secs,
        } => ProviderDto {
            kind: "openai_compat".into(),
            base_url: Some(base_url.clone()),
            timeout_secs: *timeout_secs,
            ..base
        },
    }
}

fn settings_dto(state: &AppState, cfg: &AppConfig) -> ModelSettingsDto {
    ModelSettingsDto {
        config_path: state.config_path().to_string_lossy().into_owned(),
        key_store: cfg.keystore().describe(),
        default_model: cfg.default_model_name().to_string(),
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
                is_default: alias == cfg.default_model_name(),
                is_builtin: alias == BUILTIN,
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

/// List the model ids a provider offers. `provider` is the form's current
/// (possibly unsaved) state; `api_key` is a key typed but not yet saved.
#[tauri::command]
pub async fn list_provider_models(
    state: tauri::State<'_, AppState>,
    saved_name: Option<String>,
    provider: ProviderInput,
    api_key: Option<String>,
) -> Result<Vec<String>, String> {
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
            },
            Err(e) => EngineTestResultDto {
                ok: false,
                elapsed_ms: started.elapsed().as_millis() as u64,
                message: format!("{e:#}"),
            },
        })
    })
    .await
    .map_err(|e| format!("model test task failed: {e}"))?
}
