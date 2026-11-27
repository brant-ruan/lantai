use super::*;
use crate::engine::ClaudeCliEngine;
use crate::openai::ApiKey;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aj-config-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn file_store(dir: &Path) -> KeyStore {
    KeyStore::File(dir.join(crate::keystore::SECRETS_FILE))
}

fn openai() -> ProviderConfig {
    ProviderConfig::OpenaiCompat {
        base_url: "https://example.com/v1/".into(),
        timeout_secs: None,
    }
}

fn model(provider: &str, id: &str) -> ModelConfig {
    ModelConfig {
        provider: provider.into(),
        model: Some(id.into()),
        reasoning_effort: Some("high".into()),
        temperature: None,
    }
}

#[test]
fn parses_config_without_builtins() {
    let dir = temp_dir("parse");
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        r#"
default_model = "flash"

[providers.school]
type = "openai_compat"
base_url = "https://example.com/v1/"

[models.flash]
provider = "school"
model = "m1"
reasoning_effort = "high"

[agents.codex]
enabled = true
pre_digest = true

[budgets]
chunk_chars = 1234
batch_concurrency = 4
"#,
    )
    .unwrap();
    let cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
    // Unlisted agents use their defaults: only Claude Code is on.
    assert!(cfg.agent(&AgentId::CLAUDE).enabled);
    assert!(!cfg.agent(&AgentId::ANTIGRAVITY).enabled);
    assert!(cfg.agent(&AgentId::CODEX).pre_digest);
    assert_eq!(cfg.budgets().chunk_chars, 1234);
    assert_eq!(cfg.budgets().batch_concurrency, 4);
    assert_eq!(cfg.budgets().stage2_chars, Budgets::default().stage2_chars);
    assert_eq!(cfg.default_model_name(), Some("flash"));
    assert_eq!(cfg.providers.len(), 1);
    assert!(cfg.describe_model("flash").contains("key none"));

    let engine = cfg.build_engine(None, &dir).unwrap();
    assert_eq!(engine.name(), "openai:m1:high");
    assert!(matches!(cfg.api_key("school"), ApiKey::None));
    assert!(cfg.build_engine(Some("nope"), &dir).is_err());

    // Without a config file there is no model, and generating says so; bad files are errors.
    let empty = AppConfig::load(&dir.join("missing.toml"), file_store(&dir)).unwrap();
    assert_eq!(empty.default_model_name(), None);
    let err = format!("{:#}", empty.build_engine(None, &dir).err().unwrap());
    assert_eq!(
        crate::diagnose::diagnose(&err),
        Some(crate::diagnose::Failure::NoModel)
    );
    // Estimates and dry runs still get an engine; it matches no cache and cannot complete.
    let est = empty.estimate_engine(None, &dir).unwrap();
    assert_eq!(est.name(), "");
    assert!(est.complete("x").is_err());
    assert!(empty.estimate_engine(Some("nope"), &dir).is_err());
    assert_eq!(empty.budgets().batch_concurrency, 2);
    std::fs::write(
        &path,
        "[providers.x]\ntype = \"openai_compat\"\nbase_url = \"http://h\"\napi_key_env = \"K\"",
    )
    .unwrap();
    assert!(AppConfig::load(&path, file_store(&dir)).is_err());
    std::fs::write(&path, "[agents.cursor]\nenabled = true").unwrap();
    assert!(AppConfig::load(&path, file_store(&dir)).is_err());
    std::fs::write(&path, "[models.x]\nprovider = \"ghost\"\nmodel = \"m\"").unwrap();
    assert!(AppConfig::load(&path, file_store(&dir)).is_err());
    std::fs::write(&path, "default_model = \"ghost\"").unwrap();
    assert!(AppConfig::load(&path, file_store(&dir)).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn day_boundary_defaults_to_midnight_and_round_trips() {
    let dir = temp_dir("slicing");
    let path = dir.join("config.toml");
    let mut cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
    assert_eq!(cfg.core_config().day_boundary_hour, 0);
    assert!(cfg
        .set_day_boundary_hour(MAX_DAY_BOUNDARY_HOUR + 1)
        .is_err());
    cfg.set_day_boundary_hour(4).unwrap();
    cfg.save(&path).unwrap();
    let loaded = AppConfig::load(&path, file_store(&dir)).unwrap();
    assert_eq!(loaded.core_config().day_boundary_hour, 4);
    std::fs::write(&path, "[slicing]\nday_boundary_hour = 13").unwrap();
    assert!(AppConfig::load(&path, file_store(&dir)).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn update_checks_are_off_by_default_and_round_trip() {
    let dir = temp_dir("updates");
    let path = dir.join("config.toml");
    let mut cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
    assert!(!cfg.check_updates());
    cfg.save(&path).unwrap();
    assert!(!std::fs::read_to_string(&path)
        .unwrap()
        .contains("check_updates"));
    cfg.set_check_updates(true);
    cfg.save(&path).unwrap();
    assert!(AppConfig::load(&path, file_store(&dir))
        .unwrap()
        .check_updates());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn edits_round_trip_and_keep_keys_out_of_config() {
    let dir = temp_dir("edit");
    let path = dir.join("config.toml");
    let mut cfg = AppConfig::load(&path, file_store(&dir)).unwrap();

    cfg.upsert_provider(None, "p", openai(), KeyUpdate::Set("sk-secret".into()))
        .unwrap();
    cfg.upsert_model(None, "fast", model("p", "m1"), &[])
        .unwrap();
    cfg.set_default_model("fast").unwrap();
    // Renaming a provider carries its key and repoints models; renaming a model keeps the default.
    cfg.upsert_provider(Some("p"), "q", openai(), KeyUpdate::Keep)
        .unwrap();
    cfg.upsert_model(None, "Fast Model (v2)", model("q", "m3"), &[])
        .unwrap();
    cfg.upsert_model(Some("fast"), "fast2", model("q", "m2"), &[])
        .unwrap();
    cfg.save(&path).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("sk-secret"));
    assert!(text.contains("base_url = \"https://example.com/v1\""));

    let mut cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
    assert_eq!(cfg.default_model_name(), Some("fast2"));
    assert_eq!(cfg.models["fast2"].provider, "q");
    assert_eq!(cfg.models["Fast Model (v2)"].model.as_deref(), Some("m3"));
    assert!(cfg
        .upsert_model(None, "two\nlines", model("q", "m"), &[])
        .is_err());
    assert!(cfg.has_key("q"));
    assert!(matches!(cfg.api_key("q"), ApiKey::Value(k) if k == "sk-secret"));
    assert!(cfg.describe_model("fast2").contains("key stored"));

    // A provider in use cannot be deleted; models need a known provider and a model id.
    assert!(cfg.remove_provider("q").is_err());
    assert!(cfg
        .upsert_model(None, "x", model("ghost", "m"), &[])
        .is_err());
    assert!(cfg
        .upsert_model(
            None,
            "x",
            ModelConfig {
                model: None,
                ..model("q", "m")
            },
            &[]
        )
        .is_err());
    assert!(cfg
        .upsert_provider(None, "bad name", openai(), KeyUpdate::Keep)
        .is_err());
    assert!(cfg.remove_model("ghost").is_err());

    cfg.remove_model("fast2").unwrap();
    cfg.remove_model("Fast Model (v2)").unwrap();
    assert_eq!(cfg.default_model_name(), None);
    cfg.upsert_provider(Some("q"), "q", openai(), KeyUpdate::Clear)
        .unwrap();
    assert!(!cfg.has_key("q"));
    cfg.remove_provider("q").unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn first_model_becomes_default_and_removal_falls_back() {
    let dir = temp_dir("default");
    let mut cfg = AppConfig::load(&dir.join("none.toml"), file_store(&dir)).unwrap();
    cfg.upsert_provider(None, "p", openai(), KeyUpdate::Keep)
        .unwrap();
    cfg.upsert_model(None, "b", model("p", "m"), &[]).unwrap();
    cfg.upsert_model(None, "a", model("p", "m"), &[]).unwrap();
    assert_eq!(cfg.default_model_name(), Some("b"));
    cfg.remove_model("b").unwrap();
    assert_eq!(cfg.default_model_name(), Some("a"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn engine_names_map_back_to_aliases_without_keys() {
    let dir = temp_dir("aliases");
    let mut cfg = AppConfig::load(&dir.join("none.toml"), file_store(&dir)).unwrap();
    cfg.upsert_provider(None, "p", openai(), KeyUpdate::Keep)
        .unwrap();
    cfg.upsert_model(None, "b", model("p", "m"), &[]).unwrap();
    cfg.upsert_model(None, "a", model("p", "m"), &[]).unwrap();
    cfg.upsert_model(None, "other", model("p", "n"), &[])
        .unwrap();
    let name = cfg
        .build_engine(Some("a"), Path::new("/tmp/runner"))
        .unwrap()
        .name();
    assert_eq!(cfg.aliases_for_engine(&name), vec!["a", "b"]);
    assert!(cfg.aliases_for_engine("openai:gone:high").is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn local_tools_are_added_once_and_only_if_usable() {
    use crate::local_tools::tool;
    let dir = temp_dir("tools");
    let mut cfg = AppConfig::load(&dir.join("none.toml"), file_store(&dir)).unwrap();
    let claude = tool("claude").unwrap();
    let alias = cfg
        .add_local_tool(claude, Path::new("/opt/bin/claude"))
        .unwrap();
    assert_eq!(alias, "Claude Code");
    assert_eq!(cfg.default_model_name(), Some("Claude Code"));
    assert_eq!(
        cfg.providers["claude"],
        ProviderConfig::ClaudeCli {
            binary: Some("/opt/bin/claude".into())
        }
    );
    // The same command again reuses its alias; another path gets suffixed names.
    assert_eq!(
        cfg.add_local_tool(claude, Path::new("/opt/bin/claude"))
            .unwrap(),
        "Claude Code"
    );
    assert_eq!(
        cfg.add_local_tool(claude, Path::new("/usr/bin/claude"))
            .unwrap(),
        "Claude Code 2"
    );
    assert_eq!(cfg.models["Claude Code 2"].provider, "claude-2");
    assert_eq!(cfg.default_model_name(), Some("Claude Code"));
    assert_eq!(
        cfg.add_local_tool(tool("codex").unwrap(), Path::new("/opt/bin/codex"))
            .unwrap(),
        "Codex CLI"
    );
    assert_eq!(
        cfg.providers["codex"],
        ProviderConfig::CodexCli {
            binary: Some("/opt/bin/codex".into())
        }
    );
    assert!(cfg
        .add_local_tool(tool("agy").unwrap(), Path::new("/opt/bin/agy"))
        .is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn claude_cli_lists_shorthand_models_with_efforts() {
    let dir = temp_dir("cli");
    let cfg = AppConfig::load(&dir.join("none.toml"), file_store(&dir)).unwrap();
    let models = cfg
        .list_provider_models(None, ProviderConfig::ClaudeCli { binary: None }, None)
        .unwrap();
    let sonnet = models.iter().find(|m| m.id == "sonnet").unwrap();
    assert!(sonnet.efforts.contains(&"max".to_string()));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn options_are_checked_against_the_provider() {
    let dir = temp_dir("options");
    let path = dir.join("config.toml");
    let mut cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
    cfg.upsert_provider(None, "api", openai(), KeyUpdate::Keep)
        .unwrap();
    cfg.upsert_provider(
        None,
        "claude",
        ProviderConfig::ClaudeCli { binary: None },
        KeyUpdate::Keep,
    )
    .unwrap();
    cfg.upsert_provider(
        None,
        "codex",
        ProviderConfig::CodexCli { binary: None },
        KeyUpdate::Keep,
    )
    .unwrap();
    let with = |provider: &str, effort: Option<&str>, temperature: Option<f64>| ModelConfig {
        provider: provider.into(),
        model: Some("m".into()),
        reasoning_effort: effort.map(str::to_string),
        temperature,
    };

    // A temperature is 0-2 and only for APIs.
    cfg.upsert_model(None, "warm", with("api", None, Some(0.3)), &[])
        .unwrap();
    assert!(cfg
        .upsert_model(None, "x", with("api", None, Some(2.5)), &[])
        .is_err());
    assert!(cfg
        .upsert_model(None, "x", with("codex", None, Some(0.3)), &[])
        .is_err());
    // Claude Code takes only its own effort values; Codex is checked against its list when
    // it can give one (`check_listed_effort`), here it cannot.
    cfg.upsert_model(
        None,
        "deep",
        with("claude", Some("max"), None),
        &ClaudeCliEngine::models(),
    )
    .unwrap();
    assert!(cfg
        .upsert_model(
            None,
            "x",
            with("claude", Some("ultra"), None),
            &ClaudeCliEngine::models()
        )
        .is_err());
    cfg.upsert_model(None, "cx", with("codex", Some("xhigh"), None), &[])
        .unwrap();
    cfg.save(&path).unwrap();

    let cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
    assert_eq!(cfg.models["warm"].temperature, Some(0.3));
    let runner = dir.join("runner");
    assert_eq!(
        cfg.build_engine(Some("warm"), &runner).unwrap().name(),
        "openai:m:default:t0.3"
    );
    assert_eq!(
        cfg.build_engine(Some("deep"), &runner).unwrap().name(),
        "claude-cli:m:max"
    );
    assert_eq!(
        cfg.build_engine(Some("cx"), &runner).unwrap().name(),
        "codex-cli:m:xhigh"
    );
    assert!(cfg
        .describe_model("cx")
        .contains("codex_cli, command codex"));
    assert!(cfg.describe_model("warm").contains("temperature 0.3"));

    // The same checks apply to a hand-edited file.
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        text.replace("reasoning_effort = \"xhigh\"", "temperature = 1.0"),
    )
    .unwrap();
    assert!(AppConfig::load(&path, file_store(&dir)).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn languages_are_chosen_once_saved_and_validated() {
    let dir = temp_dir("lang");
    let path = dir.join("config.toml");
    let mut cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
    assert!(cfg.chosen_report_language().is_none());
    assert_eq!(cfg.ui_language(), UI_LANGUAGE_AUTO);

    // An installation with reports keeps writing Chinese; the choice is made once.
    let (l, chosen) = cfg.report_language(true, &["en-US".to_string()]);
    assert_eq!((l.tag, chosen), ("zh-Hans", true));
    let (l, chosen) = cfg.report_language(false, &["en-US".to_string()]);
    assert_eq!((l.tag, chosen), ("zh-Hans", false));

    cfg.set_report_language("en").unwrap();
    assert!(cfg.set_report_language("xx").is_err());
    cfg.set_ui_language("zh-Hans").unwrap();
    cfg.save(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("[report]") && text.contains("[ui]"), "{text}");

    let mut cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
    assert_eq!(cfg.chosen_report_language().unwrap().tag, "en");
    assert_eq!(cfg.ui_language(), "zh-Hans");
    cfg.set_ui_language("auto").unwrap();
    assert!(cfg.ui.language.is_none());

    std::fs::write(&path, "[report]\nlanguage = \"xx\"\n").unwrap();
    assert!(AppConfig::load(&path, file_store(&dir)).is_err());
    std::fs::remove_dir_all(&dir).ok();
}
