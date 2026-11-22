//! Engine for OpenAI-compatible APIs (`/chat/completions`).
//!
//! Covers compatible services and local models. The key is resolved by the caller
//! (stored key or environment variable, see [`crate::config`]) and only ever sent
//! in the `Authorization` header.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::engine::Engine;
use crate::prompts;

/// The key to authenticate with, already resolved.
#[derive(Clone)]
pub enum ApiKey {
    /// No key is needed (local models).
    None,
    Value(String),
    /// A key is expected but unavailable; the text explains why and contains no secret.
    Missing(String),
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiKey::None => f.write_str("None"),
            ApiKey::Value(_) => f.write_str("Value(<redacted>)"),
            ApiKey::Missing(why) => write!(f, "Missing({why})"),
        }
    }
}

impl ApiKey {
    /// The `Authorization` header value, if any.
    fn header(&self) -> Result<Option<String>> {
        match self {
            ApiKey::None => Ok(None),
            ApiKey::Value(k) => Ok(Some(format!("Bearer {}", k.trim()))),
            ApiKey::Missing(why) => bail!("{why}"),
        }
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

/// List the model ids served at `base_url` (`GET /models`).
pub fn list_models(base_url: &str, key: &ApiKey) -> Result<Vec<String>> {
    let url = format!("{}/models", base_url.trim().trim_end_matches('/'));
    let mut request = agent(Duration::from_secs(20)).get(&url);
    if let Some(auth) = key.header()? {
        request = request.header("Authorization", auth);
    }
    let mut response = request
        .call()
        .map_err(|e| anyhow::anyhow!("request to {url} failed: {e}"))?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .context("failed to read the response")?;
    if status >= 400 {
        let snippet: String = text.chars().take(300).collect();
        bail!("{url} returned {status}: {snippet}");
    }
    let value: Value = serde_json::from_str(&text).context("the response is not valid JSON")?;
    let mut ids: Vec<String> = value["data"]
        .as_array()
        .or_else(|| value["models"].as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|m| m["id"].as_str().or_else(|| m["name"].as_str()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

pub struct OpenAiCompatEngine {
    /// Like `https://host/v1`, without `/chat/completions`.
    pub base_url: String,
    pub api_key: ApiKey,
    pub model: String,
    /// Reasoning effort; valid values are defined by the server (e.g. `none`, `low`, `high`, `max`).
    pub reasoning_effort: Option<String>,
    pub timeout: Duration,
}

/// Network errors, rate limits and server errors are worth retrying; anything else is not.
enum Failure {
    Retryable(anyhow::Error),
    Fatal(anyhow::Error),
}

impl OpenAiCompatEngine {
    fn request(&self, prompt: &str) -> Result<String, Failure> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": prompts::SYSTEM.trim() },
                { "role": "user", "content": prompt },
            ],
        });
        if let Some(effort) = &self.reasoning_effort {
            body["reasoning_effort"] = json!(effort);
        }

        // Read the body of 4xx/5xx responses too, so the server's error message reaches the user.
        let mut request = agent(self.timeout).post(&url);
        if let Some(auth) = self.api_key.header().map_err(Failure::Fatal)? {
            request = request.header("Authorization", auth);
        }
        let mut response = request
            .send_json(&body)
            .map_err(|e| Failure::Retryable(anyhow::anyhow!("request to {url} failed: {e}")))?;

        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| Failure::Retryable(anyhow::anyhow!("failed to read the response: {e}")))?;
        if status >= 400 {
            let snippet: String = text.chars().take(400).collect();
            let err = anyhow::anyhow!("{url} returned {status}: {snippet}");
            return Err(if status == 429 || status >= 500 {
                Failure::Retryable(err)
            } else {
                Failure::Fatal(err)
            });
        }

        let value: Value = serde_json::from_str(&text)
            .map_err(|e| Failure::Fatal(anyhow::anyhow!("the response is not valid JSON: {e}")))?;
        let choice = &value["choices"][0];
        let content = choice["message"]["content"].as_str().unwrap_or("").trim();
        if content.is_empty() {
            let reason = choice["finish_reason"].as_str().unwrap_or("unknown");
            return Err(Failure::Fatal(anyhow::anyhow!(
                "the model returned empty content (finish_reason: {reason})"
            )));
        }
        Ok(content.to_string())
    }
}

impl Engine for OpenAiCompatEngine {
    fn name(&self) -> String {
        format!(
            "openai:{}:{}",
            self.model,
            self.reasoning_effort.as_deref().unwrap_or("default")
        )
    }

    fn complete(&self, prompt: &str) -> Result<String> {
        const ATTEMPTS: u32 = 3;
        let mut last = None;
        for attempt in 1..=ATTEMPTS {
            match self.request(prompt) {
                Ok(text) => return Ok(text),
                Err(Failure::Fatal(e)) => return Err(e),
                Err(Failure::Retryable(e)) => {
                    last = Some(e);
                    if attempt < ATTEMPTS {
                        std::thread::sleep(Duration::from_secs(2 * attempt as u64));
                    }
                }
            }
        }
        match last {
            Some(e) => Err(e).with_context(|| format!("still failing after {ATTEMPTS} attempts")),
            None => bail!("no request was sent"),
        }
    }
}
