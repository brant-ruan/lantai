//! Tool audit: runs an agent CLI exactly as its engine does, against a local stand-in for the
//! model server, and prints the tools the request declares. Ignored by default because it needs
//! the CLI installed; run it after updating one and copy the result into the README table:
//!
//! ```text
//! cargo test -p aj-report -- --ignored tool_audit --nocapture
//! ```
//!
//! Codex gets a throwaway `CODEX_HOME`, so neither sign-in nor the user's own files are used or
//! touched. The test fails when a tool outside the expected set is declared.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::codex_cli::{CodexCliEngine, INSTRUCTIONS_FILE};
use crate::process;

/// Accept requests on a local port, keep each body, answer every POST with a short
/// Responses-API stream ("OK") and everything else with 404.
fn stand_in_server() -> (u16, Arc<Mutex<Vec<Vec<u8>>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let kept = bodies.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((k, v)) = line.split_once(':') {
                    if k.eq_ignore_ascii_case("content-length") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let mut stream = stream;
            if !request_line.starts_with("POST") {
                let _ = stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                continue;
            }
            kept.lock().unwrap().push(body);
            let events = [
                r#"{"type":"response.created","response":{"id":"r1"}}"#,
                r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"message","id":"m1","role":"assistant","status":"completed","content":[{"type":"output_text","text":"OK","annotations":[]}]}}"#,
                r#"{"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}}"#,
            ];
            let sse: String = events
                .iter()
                .map(|e| {
                    let kind = serde_json::from_str::<Value>(e).unwrap()["type"]
                        .as_str()
                        .unwrap()
                        .to_string();
                    format!("event: {kind}\ndata: {e}\n\n")
                })
                .collect();
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",
                    sse.len()
                )
                .as_bytes(),
            );
        }
    });
    (port, bodies)
}

/// Callable tools a Responses request declares, as `namespace.name`, with tools nested in code
/// mode's `exec` as `exec/name`.
fn declared_tools(body: &Value) -> Vec<String> {
    fn walk(tool: &Value, prefix: &str, out: &mut Vec<String>) {
        let name = tool["name"]
            .as_str()
            .unwrap_or(tool["type"].as_str().unwrap_or("?"));
        match tool["tools"].as_array() {
            Some(children) => {
                for c in children {
                    walk(c, &format!("{prefix}{name}."), out);
                }
            }
            None => {
                out.push(format!("{prefix}{name}"));
                if name == "exec" {
                    let description = tool["description"].as_str().unwrap_or("");
                    for line in description.lines() {
                        if let Some(nested) =
                            line.strip_prefix("### `").and_then(|l| l.strip_suffix('`'))
                        {
                            out.push(format!("exec/{nested}"));
                        }
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for t in body["tools"].as_array().into_iter().flatten() {
        walk(t, "", &mut out);
    }
    for item in body["input"].as_array().into_iter().flatten() {
        if item["type"] == "additional_tools" {
            for t in item["tools"].as_array().into_iter().flatten() {
                walk(t, "", &mut out);
            }
        }
    }
    out
}

fn audit_codex(model: Option<&str>) -> Vec<String> {
    let (port, bodies) = stand_in_server();
    let scratch = std::env::temp_dir().join(format!(
        "aj-tool-audit-{}-{}",
        std::process::id(),
        model.unwrap_or("default")
    ));
    let home = scratch.join("codex-home");
    let runner: PathBuf = scratch.join("runner");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&runner).unwrap();

    let mut engine = CodexCliEngine::new(
        "codex".into(),
        model.map(str::to_string),
        None,
        runner.clone(),
    );
    engine.extra_config = vec![
        "model_provider=\"stand_in\"".into(),
        format!(
            "model_providers.stand_in={{name=\"stand-in\",base_url=\"http://127.0.0.1:{port}/v1\",\
             wire_api=\"responses\",env_key=\"AJ_AUDIT_KEY\",request_max_retries=0,stream_max_retries=0}}"
        ),
    ];
    let instructions = runner.join(INSTRUCTIONS_FILE);
    std::fs::write(&instructions, "audit").unwrap();
    let mut cmd = engine.command(&instructions);
    cmd.env("CODEX_HOME", &home).env("AJ_AUDIT_KEY", "audit");
    let done = process::run(&mut cmd, Some(b"Reply with OK"), None, "Codex CLI")
        .unwrap()
        .unwrap();
    assert!(
        done.status.success(),
        "codex failed: {}",
        done.stderr_text()
    );
    assert_eq!(done.stdout_text().trim(), "OK");
    let bodies = bodies.lock().unwrap();
    let body: Value =
        serde_json::from_slice(bodies.first().expect("no request reached the stand-in")).unwrap();
    std::fs::remove_dir_all(&scratch).ok();
    declared_tools(&body)
}

#[test]
#[ignore = "needs the codex CLI; run after updating it"]
fn tool_audit_codex() {
    let version = std::process::Command::new("codex")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    // Code-mode models (Codex's default) keep `exec`, which fails because its host is off, and two
    // harmless functions; other models keep `apply_patch`, which the read-only sandbox stops.
    let cases: [(Option<&str>, &[&str]); 2] = [
        (
            None,
            &[
                "functions.exec",
                "exec/apply_patch",
                "exec/clock__curr_time",
                "functions.wait",
                "functions.request_user_input_async",
            ],
        ),
        (Some("gpt-5.5"), &["apply_patch"]),
    ];
    for (model, allowed) in cases {
        let tools = audit_codex(model);
        println!(
            "{version}, model {}: {}",
            model.unwrap_or("default"),
            if tools.is_empty() {
                "no tools".to_string()
            } else {
                tools.join(", ")
            }
        );
        let unexpected: Vec<&String> = tools
            .iter()
            .filter(|t| !allowed.contains(&t.as_str()))
            .collect();
        assert!(unexpected.is_empty(), "unexpected tools: {unexpected:?}");
    }
}
