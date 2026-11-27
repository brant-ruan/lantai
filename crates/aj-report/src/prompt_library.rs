//! Report prompts as the report layer sees them: the built-in baselines, resolving
//! which prompt a generation uses, and assembling the final instruction text.
//!
//! A prompt has two parts. The body is what the user edits: structure, level of
//! detail, tone. The rules in `prompts/<language>/report_rules.md` are appended by the
//! program and cannot be edited, so anchor verification and the data-only rule always
//! hold. After them comes the instruction to write in the report language.

use aj_store::{BuiltinPrompt, PromptRow, Store};
use anyhow::{bail, Result};

use crate::lang::Language;
use crate::prompts;

pub const KIND_DAILY: &str = "daily";
pub const KIND_WEEKLY: &str = "weekly";
pub const KIND_MONTHLY: &str = "monthly";

/// Report kinds a prompt can be written for.
pub const KINDS: [&str; 3] = ["daily", "weekly", "monthly"];

/// Prompts shipped with the program, for every tuned language. Within a kind and
/// language, the first one is the fallback default.
pub fn builtins() -> Vec<BuiltinPrompt<'static>> {
    prompts::SETS
        .iter()
        .flat_map(|set| {
            [
                (KIND_DAILY, "standard", set.daily_standard),
                (KIND_DAILY, "concise", set.daily_concise),
                (KIND_WEEKLY, "standard", set.weekly_standard),
                (KIND_MONTHLY, "standard", set.monthly_standard),
            ]
            .map(|(kind, name, body)| BuiltinPrompt {
                kind,
                language: set.tag,
                name,
                body,
            })
        })
        .collect()
}

/// Bring the stored built-ins in line with the ones shipped. Call once at startup.
pub fn sync_builtins(store: &mut Store) -> Result<()> {
    store.sync_builtin_prompts(&builtins())
}

/// The default prompt for `kind` reports written in `lang`: the user's choice, or the
/// first built-in prompt used for that language.
pub fn default_prompt(store: &Store, kind: &str, lang: &Language) -> Result<PromptRow> {
    if let Some(p) = store.chosen_default_prompt(kind, lang.tag)? {
        return Ok(p);
    }
    match store.first_builtin_prompt(kind, lang.builtin_prompt_language())? {
        Some(p) => Ok(p),
        None => bail!("there is no {kind} prompt yet"),
    }
}

/// IDs of the default prompt of every kind for `lang`.
pub fn default_ids(store: &Store, lang: &Language) -> Result<Vec<i64>> {
    KINDS
        .iter()
        .map(|k| Ok(default_prompt(store, k, lang)?.id))
        .collect()
}

/// The prompts offered for `lang` (of one kind, or of every kind): its own, then those of the
/// language it takes built-in prompts from, when that is another one.
pub fn offered(store: &Store, kind: Option<&str>, lang: &Language) -> Result<Vec<PromptRow>> {
    let mut rows = store.list_prompts(kind, Some(lang.tag))?;
    if lang.builtin_prompt_language() != lang.tag {
        rows.extend(store.list_prompts(kind, Some(lang.builtin_prompt_language()))?);
    }
    Ok(rows)
}

/// Pick the prompt for a generation: the named one (looked up among the prompts for
/// `lang`, then among the built-ins it borrows, then the default), or the default.
pub fn resolve(
    store: &Store,
    kind: &str,
    lang: &Language,
    name: Option<&str>,
) -> Result<PromptRow> {
    let Some(name) = name else {
        return default_prompt(store, kind, lang);
    };
    let languages = [lang.tag, lang.builtin_prompt_language()];
    for language in languages {
        if let Some(p) = store.find_prompt(kind, language, name)? {
            return Ok(p);
        }
    }
    // The default may be a prompt of another language.
    let default = default_prompt(store, kind, lang)?;
    if default.name.eq_ignore_ascii_case(name.trim()) {
        return Ok(default);
    }
    let mut names: Vec<String> = Vec::new();
    for p in offered(store, Some(kind), lang)? {
        if !names.contains(&p.name) {
            names.push(p.name);
        }
    }
    bail!(
        "no {kind} prompt named \"{name}\" for {}; available: {}",
        lang.tag,
        names.join(", ")
    )
}

/// Values substituted into a prompt body.
#[derive(Debug, Clone, Default)]
pub struct PromptVars<'a> {
    /// Replaces `{date}`.
    pub date: Option<&'a str>,
    /// Replaces `{period}`.
    pub period: Option<&'a str>,
    /// Replaces `{project}`.
    pub project: Option<&'a str>,
}

/// The instructions of a generation: the body with variables filled in, followed by the
/// fixed rules of `lang`'s prompt set. This is what a report version's prompt hash covers;
/// the language is recorded on the version separately.
pub fn assemble(body: &str, vars: &PromptVars, lang: &Language) -> String {
    let mut text = body.trim().to_string();
    for (placeholder, value) in [
        ("{date}", vars.date),
        ("{period}", vars.period),
        ("{project}", vars.project),
    ] {
        if let Some(value) = value {
            text = text.replace(placeholder, value);
        }
    }
    format!("{text}\n\n{}", lang.prompts().report_rules.trim())
}

/// `instructions` from [`assemble`] with the instruction to write in `lang` appended:
/// the text that is sent to the model.
pub fn with_language(instructions: &str, lang: &Language) -> String {
    format!("{instructions}\n{}", lang.report_instruction())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{EN, ZH_HANS};

    #[test]
    fn assemble_fills_variables_and_always_appends_the_rules() {
        let vars = PromptVars {
            date: Some("2026-11-16"),
            ..Default::default()
        };
        let out = assemble(
            "Write the report for {date}. Project: {project}.\n",
            &vars,
            &EN,
        );
        assert!(out.starts_with("Write the report for 2026-11-16. Project: {project}."));
        assert!(out.ends_with(prompts::EN.report_rules.trim()));
        // Even an empty body still carries the rules.
        assert!(assemble("", &PromptVars::default(), &ZH_HANS).contains("[#S12.T5]"));

        let sent = with_language(&out, &EN);
        assert!(sent.starts_with(&out) && sent.ends_with(&EN.report_instruction()));
    }

    #[test]
    fn resolve_prefers_the_named_prompt_and_falls_back_to_the_default() {
        let mut store = Store::open_in_memory().unwrap();
        assert!(resolve(&store, KIND_DAILY, &EN, None).is_err());
        sync_builtins(&mut store).unwrap();

        let en = resolve(&store, KIND_DAILY, &EN, None).unwrap();
        assert_eq!((en.name.as_str(), en.language.as_str()), ("standard", "en"));
        let zh = resolve(&store, KIND_DAILY, &ZH_HANS, Some("Concise")).unwrap();
        assert_eq!(
            (zh.name.as_str(), zh.language.as_str()),
            ("concise", "zh-Hans")
        );
        let err = resolve(&store, KIND_DAILY, &EN, Some("nope"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("standard") && err.contains("concise"), "{err}");

        // A prompt written for another language can be the default.
        let id = store
            .create_prompt(KIND_DAILY, "en", "mine", "body")
            .unwrap();
        store.set_default_prompt(id, "zh-Hans").unwrap();
        assert_eq!(resolve(&store, KIND_DAILY, &ZH_HANS, None).unwrap().id, id);
        assert_eq!(
            resolve(&store, KIND_DAILY, &EN, None).unwrap().name,
            "standard"
        );
    }

    #[test]
    fn every_tuned_language_ships_every_kind() {
        let all = builtins();
        for set in prompts::SETS {
            for kind in KINDS {
                assert!(
                    all.iter().any(|b| b.kind == kind && b.language == set.tag),
                    "{} has no built-in {kind} prompt",
                    set.tag
                );
            }
        }
    }
}
