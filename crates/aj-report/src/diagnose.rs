//! Sorts an engine error's text into a few known cases so the interface can say what happened and
//! what to do next. Pure; the raw text is always kept for "Details". The cases and their markers
//! are listed in `docs/SPEC.md`, "Failures".

use serde::Serialize;

/// Marker in the error raised when no model alias is configured.
pub const NO_MODEL: &str = "no model is configured";
/// Marker in the error raised when a CLI engine's command cannot be started.
pub const COMMAND_NOT_FOUND: &str = "was not found";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    NoModel,
    CommandNotFound,
    /// The CLI is not signed in, or the provider rejected the key.
    NotSignedIn,
    /// A usage limit, quota, empty balance or rate limit.
    Quota,
}

const NOT_SIGNED_IN: &[&str] = &[
    "not logged in",
    "/login",
    "invalid api key",
    "invalid_api_key",
    "unauthorized",
    "authentication",
    "oauth token has expired",
    "returned 401",
    "returned 403",
];

const QUOTA: &[&str] = &[
    "usage limit",
    "rate limit",
    "rate_limit",
    "quota",
    "credit balance",
    "insufficient_balance",
    "returned 429",
];

/// The known case an engine error belongs to, or `None` for anything else.
pub fn diagnose(message: &str) -> Option<Failure> {
    let m = message.to_ascii_lowercase();
    let any = |markers: &[&str]| markers.iter().any(|k| m.contains(k));
    if m.contains(NO_MODEL) {
        Some(Failure::NoModel)
    } else if m.contains(COMMAND_NOT_FOUND) && m.contains("command") {
        Some(Failure::CommandNotFound)
    } else if any(NOT_SIGNED_IN) {
        Some(Failure::NotSignedIn)
    } else if any(QUOTA) {
        Some(Failure::Quota)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_cases() {
        let cases = [
            (
                "2026-11-20: no model is configured; add one in Settings → Models",
                Some(Failure::NoModel),
            ),
            (
                "command `/usr/local/bin/claude` was not found",
                Some(Failure::CommandNotFound),
            ),
            (
                "claude exited with code Some(1): Invalid API key · Please run /login",
                Some(Failure::NotSignedIn),
            ),
            (
                "https://h/v1/chat/completions returned 401: {\"error\":\"bad key\"}",
                Some(Failure::NotSignedIn),
            ),
            (
                "claude exited with code Some(1): Claude AI usage limit reached|1700000000",
                Some(Failure::Quota),
            ),
            (
                "https://h/v1/chat/completions returned 429: slow down",
                Some(Failure::Quota),
            ),
            ("the response is not valid JSON", None),
            ("the report was not found", None),
        ];
        for (text, want) in cases {
            assert_eq!(diagnose(text), want, "{text}");
        }
    }
}
