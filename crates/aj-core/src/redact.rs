//! Redaction, applied before storage and before text is sent to a model. Pattern based; better to mask too much than too little.

use std::sync::OnceLock;

use aj_ir::IrEvent;
use regex::Regex;

struct Rule {
    re: Regex,
    replacement: &'static str,
}

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let rule = |pattern: &str, replacement: &'static str| Rule {
            re: Regex::new(pattern).expect("redaction rules must be valid regexes"),
            replacement,
        };
        vec![
            rule(
                r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?(?:-----END [A-Z ]*PRIVATE KEY-----|\z)",
                "[REDACTED:private-key]",
            ),
            // Vendor keys with a fixed prefix
            rule(r"\bsk-[A-Za-z0-9_\-]{16,}", "[REDACTED:key]"),
            rule(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}", "[REDACTED:key]"),
            rule(r"\bgithub_pat_[A-Za-z0-9_]{20,}", "[REDACTED:key]"),
            rule(r"\bxox[abprs]-[A-Za-z0-9\-]{10,}", "[REDACTED:key]"),
            rule(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b", "[REDACTED:key]"),
            rule(r"\bAIza[0-9A-Za-z_\-]{30,}", "[REDACTED:key]"),
            // JWT
            rule(r"\beyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}", "[REDACTED:token]"),
            rule(r"(?i)\b(bearer)\s+[A-Za-z0-9._~+/\-]{16,}=*", "$1 [REDACTED:token]"),
            // Credentials inside a URL
            rule(r"(?i)\b([a-z][a-z0-9+.\-]*://[^\s:/@]+):[^\s@/]+@", "$1:[REDACTED]@"),
            // Assignments such as PASSWORD=xxx or api_key: "xxx"; the key name is kept
            rule(
                r#"(?i)\b([A-Za-z0-9_\-]*(?:password|passwd|secret|token|api[_\-]?key|access[_\-]?key|private[_\-]?key)[A-Za-z0-9_\-]*)("?\s*[=:]\s*)("[^"\s]{6,}"|'[^'\s]{6,}'|[^\s"',;]{6,})"#,
                "$1$2[REDACTED]",
            ),
        ]
    })
}

pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    for rule in rules() {
        if rule.re.is_match(&out) {
            out = rule.re.replace_all(&out, rule.replacement).into_owned();
        }
    }
    out
}

/// Redact every text field of an event that comes from session content. Called before ingestion so that recognizable secrets never reach the database.
pub fn redact_event(event: &mut IrEvent) {
    fn apply(text: &mut String) {
        let cleaned = redact(text);
        if cleaned != *text {
            *text = cleaned;
        }
    }
    fn apply_opt(text: &mut Option<String>) {
        if let Some(t) = text {
            apply(t);
        }
    }
    match event {
        IrEvent::TurnStart { user_prompt, .. } => apply(user_prompt),
        IrEvent::AgentMessage { text, .. } => apply(text),
        IrEvent::ToolAction {
            target, summary, ..
        } => {
            apply_opt(target);
            apply_opt(summary);
        }
        IrEvent::ToolOutcome { output_summary, .. } => apply_opt(output_summary),
        IrEvent::SessionTitle { title } => apply(title),
        IrEvent::SessionMeta { .. } | IrEvent::TokenUsage { .. } | IrEvent::Unknown { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_common_secrets() {
        let cases = [
            (
                "key is sk-ant-api03-abcdefghijklmnopqrstuvwxyz ok",
                "sk-ant",
            ),
            (
                "export GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123",
                "ghp_",
            ),
            ("AWS AKIAABCDEFGHIJKLMNOP here", "AKIAABCDEFGHIJKLMNOP"),
            (
                "Authorization: Bearer abcdefghijklmnopqrstuvwxyz123456",
                "abcdefghijklmnopqrstuvwxyz123456",
            ),
            ("DB_PASSWORD=hunter2hunter2", "hunter2"),
            (r#"{"api_key": "abcdef123456"}"#, "abcdef123456"),
            ("postgres://admin:s3cretpw@db.internal:5432/app", "s3cretpw"),
            (
                "-----BEGIN RSA PRIVATE KEY-----\nMIIabc\n-----END RSA PRIVATE KEY-----",
                "MIIabc",
            ),
        ];
        for (input, secret) in cases {
            let out = redact(input);
            assert!(!out.contains(secret), "not redacted: {input} -> {out}");
            assert!(out.contains("REDACTED"), "missing marker: {out}");
        }
    }

    #[test]
    fn keeps_ordinary_text_and_key_names() {
        let s = "fix the token refresh logic, add a mutex in token.ts; rename the password field";
        assert_eq!(redact(s), s);
        assert_eq!(
            redact("DB_PASSWORD=hunter2hunter2"),
            "DB_PASSWORD=[REDACTED]"
        );
        assert_eq!(
            redact("postgres://u:pw123456@h/db"),
            "postgres://u:[REDACTED]@h/db"
        );
    }

    #[test]
    fn events_are_redacted_before_storage() {
        let mut e = IrEvent::ToolOutcome {
            tool_source_id: "t".into(),
            is_error: false,
            diff_stat: None,
            output_summary: Some(
                "origin https://me:ghp_abcdefghijklmnopqrstuvwxyz0123@github.com/o/r".into(),
            ),
        };
        redact_event(&mut e);
        let IrEvent::ToolOutcome {
            output_summary: Some(out),
            ..
        } = e
        else {
            unreachable!()
        };
        assert!(!out.contains("ghp_abcdefghijklmnopqrstuvwxyz0123"), "{out}");
        assert!(out.contains("github.com/o/r"));
    }
}
