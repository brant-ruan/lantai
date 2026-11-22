//! Report languages: which languages reports can be written in, which of them have
//! their own tuned prompts, and the text the program itself writes for each.
//!
//! A language is identified by a BCP-47 tag (`en`, `zh-Hans`). A tuned language has a
//! prompt set in `prompts/<tag>/`; any other listed language borrows the English set,
//! and the output-language instruction the program appends makes the model write in it.

use std::sync::OnceLock;

use anyhow::{bail, Result};
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;

use crate::prompts::{self, PromptSet};

/// A language reports can be written in.
#[derive(Debug)]
pub struct Language {
    /// BCP-47 tag, stored in the database and `config.toml`.
    pub tag: &'static str,
    /// The language's name in itself, for menus.
    pub native_name: &'static str,
    /// The language's name in English, used in instructions to the model.
    pub english_name: &'static str,
    /// Its own tuned prompts, if any.
    tuned: Option<&'static PromptSet>,
}

pub static EN: Language = Language {
    tag: "en",
    native_name: "English",
    english_name: "English",
    tuned: Some(&prompts::EN),
};

pub static ZH_HANS: Language = Language {
    tag: "zh-Hans",
    native_name: "\u{7b80}\u{4f53}\u{4e2d}\u{6587}",
    english_name: "Simplified Chinese",
    tuned: Some(&prompts::ZH_HANS),
};

/// Every report language, in menu order.
pub static LANGUAGES: [&Language; 2] = [&EN, &ZH_HANS];

/// Language of everything written before reports had a language setting.
pub static LEGACY: &Language = &ZH_HANS;

/// Text the program writes for one prompt set, parsed from its `locale.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Locale {
    weekdays: [String; 7],
    stats_line: String,
    day_unit: [String; 2],
    session_unit: [String; 2],
    report_language: String,
    json_language: String,
    name: String,
}

fn locale_of(set: &'static PromptSet) -> &'static Locale {
    static PARSED: [OnceLock<Locale>; 2] = [OnceLock::new(), OnceLock::new()];
    let i = prompts::SETS
        .iter()
        .position(|s| std::ptr::eq(*s, set))
        .expect("every prompt set is listed in prompts::SETS");
    PARSED[i].get_or_init(|| {
        toml::from_str(set.locale)
            .unwrap_or_else(|e| panic!("prompts/{}/locale.toml is invalid: {e}", set.tag))
    })
}

impl Language {
    /// The language with this tag, if it is one of [`LANGUAGES`].
    pub fn find(tag: &str) -> Option<&'static Language> {
        LANGUAGES.iter().copied().find(|l| l.tag == tag)
    }

    /// Like [`Language::find`], with an error naming the known tags.
    pub fn get(tag: &str) -> Result<&'static Language> {
        match Self::find(tag) {
            Some(l) => Ok(l),
            None => bail!(
                "unknown report language \"{tag}\"; available: {}",
                LANGUAGES
                    .iter()
                    .map(|l| l.tag)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    /// Whether the language has its own tuned prompts.
    pub fn is_tuned(&self) -> bool {
        self.tuned.is_some()
    }

    /// The prompts used for this language: its own, or the English ones.
    pub fn prompts(&self) -> &'static PromptSet {
        self.tuned.unwrap_or(&prompts::EN)
    }

    /// Language tag the built-in prompts used for this language are stored under.
    pub fn builtin_prompt_language(&self) -> &'static str {
        self.prompts().tag
    }

    fn locale(&self) -> &'static Locale {
        locale_of(self.prompts())
    }

    /// The name the prompt set's own instructions use for this language.
    fn name_in_prompts(&self) -> &str {
        match self.tuned {
            Some(_) => &self.locale().name,
            None => self.english_name,
        }
    }

    /// Instruction appended after the report rules: write the report in this language.
    pub fn report_instruction(&self) -> String {
        self.locale()
            .report_language
            .replace("{language}", self.name_in_prompts())
    }

    /// Instruction appended to requests that return JSON: write its text in this language.
    pub fn json_instruction(&self) -> String {
        self.locale()
            .json_language
            .replace("{language}", self.name_in_prompts())
    }

    /// The canonical daily report title: `# 2026-11-08 · <weekday>`, or `# <date>` when
    /// `date` is not a `YYYY-MM-DD` date.
    pub fn daily_title(&self, date: &str) -> String {
        match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
            Ok(d) => format!(
                "# {date} \u{00b7} {}",
                self.locale().weekdays[d.weekday().num_days_from_monday() as usize]
            ),
            Err(_) => format!("# {date}"),
        }
    }

    /// The statistics line under a project in weekly and monthly reports.
    pub fn stats_line(&self, days: usize, minutes: i64, sessions: usize) -> String {
        let l = self.locale();
        let unit = |forms: &[String; 2], n: usize| forms[usize::from(n != 1)].clone();
        l.stats_line
            .replace("{days}", &days.to_string())
            .replace("{hours}", &format!("{:.1}", minutes as f64 / 60.0))
            .replace("{sessions}", &sessions.to_string())
            .replace("{day_unit}", &unit(&l.day_unit, days))
            .replace("{session_unit}", &unit(&l.session_unit, sessions))
    }
}

/// The supported language a system locale names, if any. Accepts BCP 47 tags (`zh-Hans-CN`,
/// `en-GB`) and POSIX locales (`zh_CN.UTF-8`). Only Simplified Chinese counts as Chinese:
/// Traditional Chinese (`zh-Hant`, or `zh-TW` / `zh-HK` / `zh-MO` without a script) is not
/// supported, so it matches nothing. The interface (`app/src/i18n.tsx`) follows the same rule.
pub fn match_locale(locale: &str) -> Option<&'static Language> {
    let base = locale.split(['.', '@']).next().unwrap_or_default();
    let lower = base.to_ascii_lowercase().replace('_', "-");
    let mut parts = lower.split('-');
    let primary = parts.next()?;
    let rest: Vec<&str> = parts.collect();
    match primary {
        "en" => Some(&EN),
        "zh" => {
            let has = |p: &str| rest.contains(&p);
            let traditional =
                has("hant") || (!has("hans") && (has("tw") || has("hk") || has("mo")));
            (!traditional).then_some(&ZH_HANS)
        }
        _ => None,
    }
}

/// The report language for an installation that has not chosen one: Chinese when it
/// already has reports (they were all written in Chinese), otherwise the first supported
/// language among the system's preferred languages (most preferred first), otherwise English.
pub fn initial(has_reports: bool, system_locales: &[String]) -> &'static Language {
    if has_reports {
        return LEGACY;
    }
    system_locales
        .iter()
        .find_map(|l| match_locale(l))
        .unwrap_or(&EN)
}

/// The preferred locales from the environment, most preferred first: the colon-separated
/// `LANGUAGE` list, then the first of `LC_ALL`, `LC_MESSAGES`, `LANG` that is set.
pub fn env_locales() -> Vec<String> {
    let usable = |v: &str| !v.is_empty() && v != "C" && v != "POSIX";
    let mut out: Vec<String> = std::env::var("LANGUAGE")
        .unwrap_or_default()
        .split(':')
        .filter(|v| usable(v))
        .map(str::to_string)
        .collect();
    if let Some(v) = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| usable(v))
    {
        out.push(v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A listed language without its own prompts.
    static GENERIC: Language = Language {
        tag: "ja",
        native_name: "\u{65e5}\u{672c}\u{8a9e}",
        english_name: "Japanese",
        tuned: None,
    };

    #[test]
    fn every_locale_parses() {
        for l in LANGUAGES {
            assert!(!l.report_instruction().contains('{'), "{}", l.tag);
            assert!(!l.json_instruction().contains('{'), "{}", l.tag);
            assert!(!l.daily_title("2026-11-20").contains('{'));
        }
    }

    #[test]
    fn tuned_languages_use_their_own_text() {
        assert_eq!(EN.daily_title("2026-11-20"), "# 2026-11-20 \u{00b7} Fri");
        assert_eq!(
            ZH_HANS.daily_title("2026-11-09"),
            "# 2026-11-09 \u{00b7} \u{5468}\u{4e00}"
        );
        assert_eq!(EN.daily_title("not a date"), "# not a date");
        assert!(EN.report_instruction().contains("in English"));
        assert!(ZH_HANS
            .report_instruction()
            .contains("\u{7b80}\u{4f53}\u{4e2d}\u{6587}"));
        assert_eq!(ZH_HANS.builtin_prompt_language(), "zh-Hans");
    }

    #[test]
    fn a_language_without_prompts_borrows_english() {
        assert!(!GENERIC.is_tuned());
        assert_eq!(GENERIC.builtin_prompt_language(), "en");
        assert!(std::ptr::eq(GENERIC.prompts(), &prompts::EN));
        assert!(GENERIC.report_instruction().contains("in Japanese"));
        assert!(GENERIC.json_instruction().contains("in Japanese"));
        assert_eq!(
            GENERIC.daily_title("2026-11-20"),
            "# 2026-11-20 \u{00b7} Fri"
        );
    }

    #[test]
    fn stats_line_handles_singular_and_rounding() {
        assert_eq!(
            EN.stats_line(1, 90, 1),
            "> 1 active day \u{00b7} ~1.5 h \u{00b7} 1 session"
        );
        assert_eq!(
            EN.stats_line(3, 270, 7),
            "> 3 active days \u{00b7} ~4.5 h \u{00b7} 7 sessions"
        );
        assert_eq!(
            ZH_HANS.stats_line(3, 270, 7),
            "> \u{6d3b}\u{8dc3} 3 \u{5929} \u{00b7} \u{7ea6} 4.5 \u{5c0f}\u{65f6} \u{00b7} 7 \u{4e2a}\u{4f1a}\u{8bdd}"
        );
    }

    #[test]
    fn lookup_and_initial_choice() {
        assert_eq!(Language::get("zh-Hans").unwrap().tag, "zh-Hans");
        assert!(Language::get("xx")
            .unwrap_err()
            .to_string()
            .contains("en, zh-Hans"));
        let v = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(initial(true, &v(&["en-US"])).tag, "zh-Hans");
        assert_eq!(initial(false, &v(&["zh-CN"])).tag, "zh-Hans");
        assert_eq!(initial(false, &v(&["de-DE"])).tag, "en");
        assert_eq!(initial(false, &[]).tag, "en");
        // The whole preference list counts, not only its first entry.
        assert_eq!(
            initial(false, &v(&["ja-JP", "zh-Hans-CN", "en"])).tag,
            "zh-Hans"
        );
        // Traditional Chinese is not supported: skipped, then English as the fallback.
        assert_eq!(initial(false, &v(&["zh_TW.UTF-8"])).tag, "en");
        assert_eq!(initial(false, &v(&["zh-Hant-HK", "zh-CN"])).tag, "zh-Hans");
    }

    #[test]
    fn locales_match_by_language_and_chinese_script() {
        let tag = |l: &str| match_locale(l).map(|x| x.tag);
        assert_eq!(tag("en_GB.UTF-8"), Some("en"));
        assert_eq!(tag("zh"), Some("zh-Hans"));
        assert_eq!(tag("zh-SG"), Some("zh-Hans"));
        assert_eq!(tag("zh-Hans-TW"), Some("zh-Hans"));
        assert_eq!(tag("zh-Hant"), None);
        assert_eq!(tag("zh-HK"), None);
        assert_eq!(tag("zh_MO.UTF-8"), None);
        assert_eq!(tag("fr-FR"), None);
        assert_eq!(tag(""), None);
    }
}
