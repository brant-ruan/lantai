//! Built-in prompt texts.
//!
//! Prompts are product content, not code: they live in `prompts/` so they can be edited
//! without touching Rust. Each report language with tuned prompts has its own directory
//! (`prompts/<language tag>/`) holding the same set of files; source files stay English-only.

/// System prompt shared by every engine and every report language. It sets the role only;
/// the output language is asked for in each request.
pub const SYSTEM: &str = include_str!("../prompts/system.md");

/// The prompts and report texts of one tuned report language.
#[derive(Debug)]
pub struct PromptSet {
    /// Language tag, also the directory name.
    pub tag: &'static str,
    /// Stage 1: turns one dehydrated slice into a structured digest.
    pub digest: &'static str,
    /// Stage 1 for long slices: merges the digests of the parts into one digest.
    pub digest_merge: &'static str,
    /// Stage 1 for a slice that grew: folds the new turns into the existing digest.
    pub digest_update: &'static str,
    /// Stage 2 when a project has much material: clusters it into tasks first.
    pub project_brief: &'static str,
    /// Daily report, user-editable part: the built-in baseline. `{date}` is replaced.
    pub daily_standard: &'static str,
    /// Daily report, user-editable part: a shorter alternative.
    pub daily_concise: &'static str,
    /// Weekly report, user-editable part: one project's week. `{period}` is the week key.
    pub weekly_standard: &'static str,
    /// Monthly report, user-editable part: one project's month. `{period}` is the month key.
    pub monthly_standard: &'static str,
    /// Report, fixed part: appended after the writing requirements so that anchor
    /// verification and the data-only rule cannot be edited away.
    pub report_rules: &'static str,
    /// Text the program writes into reports, as TOML (see [`crate::lang`]).
    pub locale: &'static str,
}

macro_rules! prompt_set {
    ($tag:literal) => {
        PromptSet {
            tag: $tag,
            digest: include_str!(concat!("../prompts/", $tag, "/digest.md")),
            digest_merge: include_str!(concat!("../prompts/", $tag, "/digest_merge.md")),
            digest_update: include_str!(concat!("../prompts/", $tag, "/digest_update.md")),
            project_brief: include_str!(concat!("../prompts/", $tag, "/project_brief.md")),
            daily_standard: include_str!(concat!("../prompts/", $tag, "/daily_standard.md")),
            daily_concise: include_str!(concat!("../prompts/", $tag, "/daily_concise.md")),
            weekly_standard: include_str!(concat!("../prompts/", $tag, "/weekly_standard.md")),
            monthly_standard: include_str!(concat!("../prompts/", $tag, "/monthly_standard.md")),
            report_rules: include_str!(concat!("../prompts/", $tag, "/report_rules.md")),
            locale: include_str!(concat!("../prompts/", $tag, "/locale.toml")),
        }
    };
}

pub static EN: PromptSet = prompt_set!("en");
pub static ZH_HANS: PromptSet = prompt_set!("zh-Hans");

/// Every tuned prompt set. `en` comes first: it is the fallback for other languages.
pub static SETS: [&PromptSet; 2] = [&EN, &ZH_HANS];

impl PromptSet {
    /// Every prompt text with its file name, for checks across sets.
    pub fn texts(&self) -> [(&'static str, &'static str); 9] {
        [
            ("digest.md", self.digest),
            ("digest_merge.md", self.digest_merge),
            ("digest_update.md", self.digest_update),
            ("project_brief.md", self.project_brief),
            ("daily_standard.md", self.daily_standard),
            ("daily_concise.md", self.daily_concise),
            ("weekly_standard.md", self.weekly_standard),
            ("monthly_standard.md", self.monthly_standard),
            ("report_rules.md", self.report_rules),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(text: &str) -> Vec<&str> {
        let mut out: Vec<&str> = ["{date}", "{period}", "{project}"]
            .into_iter()
            .filter(|p| text.contains(p))
            .collect();
        out.sort();
        out
    }

    #[test]
    fn every_set_matches_english_file_by_file() {
        for set in SETS {
            for ((name, text), (_, en)) in set.texts().into_iter().zip(EN.texts()) {
                assert!(!text.trim().is_empty(), "{}/{name} is empty", set.tag);
                assert_eq!(
                    placeholders(text),
                    placeholders(en),
                    "{}/{name} uses different placeholders than en/{name}",
                    set.tag
                );
            }
        }
    }

    #[test]
    fn no_prompt_names_its_output_language() {
        // The output language is appended by the program; a prompt that names one would
        // contradict it for languages that borrow the set.
        for set in SETS {
            for (name, text) in set.texts() {
                if name.starts_with("daily")
                    || name.starts_with("weekly")
                    || name.starts_with("monthly")
                {
                    continue; // User-editable bodies may say "Markdown" in their own words.
                }
                assert!(
                    !text.contains("Write all text values")
                        && !text.contains("JSON \u{4e2d}\u{7684}\u{6587}\u{5b57}"),
                    "{}/{name} hard-codes the output language",
                    set.tag
                );
            }
        }
    }
}
