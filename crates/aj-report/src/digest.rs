//! The digest of a slice: its structure, the prompt for one request, and parsing the reply.
//!
//! Deciding how a slice is summarized (passthrough, one request, or part by part)
//! lives in [`crate::summarize`].

use aj_core::INTERNAL_TASK_FINGERPRINT;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::lang::Language;

/// Bump when a stage 1 prompt or the digest structure changes, to invalidate old caches.
pub const DIGEST_PROMPT_VERSION: &str = "5";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub text: String,
    #[serde(default)]
    pub turns: Vec<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Digest {
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub key_changes: Vec<Point>,
    #[serde(default)]
    pub decisions_and_pitfalls: Vec<Point>,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub open_items: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_project: Option<String>,
}

/// A project the model may suggest for an unassigned session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateProject {
    pub name: String,
    pub description: Option<String>,
}

/// What is stored in `digest_json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoredDigest {
    /// Structured summary distilled by the model.
    Summary(Digest),
    /// Short-slice passthrough, or the fallback when the model output cannot be parsed: the dehydrated text itself.
    Raw { text: String },
}

/// Context that accompanies the record in a digest request.
#[derive(Debug, Clone, Copy, Default)]
pub struct DigestContext<'a> {
    pub user_note: Option<&'a str>,
    /// Digest (JSON) of the previous slice of the same session, giving context to sessions that span days.
    pub previous_digest: Option<&'a str>,
    /// Key points of the earlier parts of the same slice, when the slice is summarized part by part.
    pub earlier_parts: Option<&'a str>,
    /// Existing projects, included only when the session has neither an assignment nor a suggestion yet.
    pub candidate_projects: &'a [CandidateProject],
}

impl DigestContext<'_> {
    fn write_to(&self, p: &mut String) {
        if let Some(note) = self.user_note.filter(|n| !n.trim().is_empty()) {
            p.push_str(&format!("\nUser note: {}\n", note.trim()));
        }
        if let Some(prev) = self.previous_digest {
            p.push_str(&format!("\nPrevious digest:\n{prev}\n"));
        }
        if let Some(earlier) = self.earlier_parts.filter(|e| !e.trim().is_empty()) {
            p.push_str(&format!("\nEarlier parts:\n{}\n", earlier.trim_end()));
        }
        if !self.candidate_projects.is_empty() {
            p.push_str("\nCandidate projects:\n");
            for project in self.candidate_projects {
                match project.description.as_deref().map(str::trim) {
                    Some(desc) if !desc.is_empty() => {
                        p.push_str(&format!("- \"{}\" — {desc}\n", project.name));
                    }
                    _ => p.push_str(&format!("- \"{}\"\n", project.name)),
                }
            }
        }
    }
}

/// SHA-256 over length-prefixed parts, so that different splits of the same bytes never collide.
pub fn hash_parts(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for part in parts {
        h.update((part.len() as u64).to_le_bytes());
        h.update(part.as_bytes());
    }
    hex(&h.finalize())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The fingerprint, a stage 1 prompt of `lang`'s set and the instruction to write in `lang`.
fn head(prompt: &str, lang: &Language) -> String {
    format!(
        "{INTERNAL_TASK_FINGERPRINT}\n{}\n{}\n",
        prompt.trim_end(),
        lang.json_instruction()
    )
}

/// Prompt asking for the digest of one record (a whole slice, or one part of it).
pub fn build_prompt(record: &str, ctx: &DigestContext, lang: &Language) -> String {
    let mut p = head(lang.prompts().digest, lang);
    ctx.write_to(&mut p);
    p.push_str("\nSession record:\n<<<\n");
    p.push_str(record);
    p.push_str("\n>>>\n");
    p
}

/// Prompt asking to merge the digests of the parts of one slice. `parts` is the rendered list.
pub fn build_merge_prompt(parts: &str, ctx: &DigestContext, lang: &Language) -> String {
    let mut p = head(lang.prompts().digest_merge, lang);
    ctx.write_to(&mut p);
    p.push_str("\nPart digests:\n<<<\n");
    p.push_str(parts.trim_end());
    p.push_str("\n>>>\n");
    p
}

/// Prompt asking to fold newly appended turns into the existing digest of a slice.
pub fn build_update_prompt(
    current: &Digest,
    new_turns: &str,
    ctx: &DigestContext,
    lang: &Language,
) -> String {
    let mut p = head(lang.prompts().digest_update, lang);
    ctx.write_to(&mut p);
    p.push_str("\nCurrent digest:\n<<<\n");
    p.push_str(&serde_json::to_string(current).unwrap_or_default());
    p.push_str("\n>>>\n\nNew turns:\n<<<\n");
    p.push_str(new_turns);
    p.push_str("\n>>>\n");
    p
}

/// The outermost JSON object in model output. Tolerates code fences and extra text around it.
pub fn extract_json_object(output: &str) -> Result<&str> {
    let start = output.find('{').context("no JSON object in the output")?;
    let end = output.rfind('}').context("no JSON object in the output")?;
    anyhow::ensure!(end > start, "no complete JSON object in the output");
    Ok(&output[start..=end])
}

/// Parse a digest from model output.
pub fn parse_digest(output: &str) -> Result<Digest> {
    let digest: Digest = serde_json::from_str(extract_json_object(output)?)
        .context("failed to parse the digest JSON")?;
    anyhow::ensure!(
        !digest.goal.trim().is_empty() || !digest.key_changes.is_empty(),
        "the digest is empty"
    );
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{EN, ZH_HANS};

    #[test]
    fn parses_fenced_json_with_missing_fields() {
        let out = "```json\n{\"goal\": \"fix login\", \"key_changes\": [{\"text\": \"add a lock\", \"turns\": [3]}]}\n```";
        let d = parse_digest(out).unwrap();
        assert_eq!(d.goal, "fix login");
        assert_eq!(d.key_changes[0].turns, vec![3]);
        assert!(d.open_items.is_empty());
        assert!(parse_digest("Sorry, I cannot handle this").is_err());
        assert!(parse_digest("{}").is_err());
    }

    #[test]
    fn hash_respects_part_boundaries() {
        assert_ne!(hash_parts(&["ab", "c"]), hash_parts(&["a", "bc"]));
        assert_ne!(hash_parts(&["abc"]), hash_parts(&["abc", ""]));
        assert_eq!(hash_parts(&["a", "b"]), hash_parts(&["a", "b"]));
    }

    #[test]
    fn context_sections_appear_only_when_present() {
        let bare = build_prompt("### T0\nUser: hi", &DigestContext::default(), &EN);
        assert!(bare.contains(&EN.json_instruction()));
        assert!(
            !bare.contains("User note:")
                && !bare.contains("Earlier parts:")
                && !bare.contains("Candidate projects:")
        );
        let candidates = vec![
            CandidateProject {
                name: "Atlas".into(),
                description: Some("  paper & eval  ".into()),
            },
            CandidateProject {
                name: "Lantai".into(),
                description: None,
            },
        ];
        let ctx = DigestContext {
            user_note: Some("  the real fix was X "),
            previous_digest: None,
            earlier_parts: Some("Part 1 (T0-T4): set up the project"),
            candidate_projects: &candidates,
        };
        let full = build_prompt("### T5\nUser: next", &ctx, &ZH_HANS);
        assert!(full.contains(crate::prompts::ZH_HANS.digest.trim_end()));
        assert!(full.contains(&ZH_HANS.json_instruction()));
        assert!(full.contains("User note: the real fix was X\n"));
        assert!(full.contains("Earlier parts:\nPart 1 (T0-T4): set up the project\n"));
        assert!(full.contains("Candidate projects:\n- \"Atlas\" — paper & eval\n- \"Lantai\"\n"));
        assert!(full.ends_with("Session record:\n<<<\n### T5\nUser: next\n>>>\n"));
    }

    #[test]
    fn parses_suggested_project_when_present_or_null() {
        let out = r#"{"goal":"fix login","suggested_project":"Atlas"}"#;
        let d = parse_digest(out).unwrap();
        assert_eq!(d.suggested_project.as_deref(), Some("Atlas"));

        let out_null = r#"{"goal":"fix login","suggested_project":null}"#;
        assert_eq!(parse_digest(out_null).unwrap().suggested_project, None);
    }
}
