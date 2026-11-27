//! Running a local command: optional input on stdin, both outputs collected, an optional time
//! limit. Shared by the agent CLI engines, their model lists and finding local tools.

use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};

use crate::diagnose;

/// What a finished command left behind.
pub struct Finished {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Finished {
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// Run `cmd`, writing `input` to its stdin (or giving it none), and wait for it to finish.
/// With a `timeout`, a command still running after it is killed and `Ok(None)` is returned.
/// `product` names the tool in the error when the command cannot be started.
pub fn run(
    cmd: &mut Command,
    input: Option<&[u8]>,
    timeout: Option<Duration>,
    product: &str,
) -> Result<Option<Finished>> {
    cmd.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => anyhow!(
            "command `{program}` {}; is {product} still installed there?",
            diagnose::COMMAND_NOT_FOUND
        ),
        _ => anyhow::Error::new(e).context(format!("failed to start `{program}`")),
    })?;

    // Feed stdin and drain both outputs on their own threads, so a large prompt or a chatty
    // command cannot deadlock against a full pipe.
    let stdin = child.stdin.take();
    let mut stdout = child
        .stdout
        .take()
        .context("failed to get the child's stdout")?;
    let mut stderr = child
        .stderr
        .take()
        .context("failed to get the child's stderr")?;
    std::thread::scope(|scope| {
        if let (Some(mut pipe), Some(bytes)) = (stdin, input) {
            scope.spawn(move || {
                let _ = pipe.write_all(bytes);
            });
        }
        let out = scope.spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout.read_to_end(&mut buf);
            buf
        });
        let err = scope.spawn(move || {
            let mut buf = Vec::new();
            let _ = stderr.read_to_end(&mut buf);
            buf
        });
        let status = wait(&mut child, timeout)?;
        let stdout = out.join().unwrap_or_default();
        let stderr = err.join().unwrap_or_default();
        Ok(status.map(|status| Finished {
            status,
            stdout,
            stderr,
        }))
    })
}

/// Wait for `child`; past `timeout`, kill it and return `None`.
fn wait(child: &mut Child, timeout: Option<Duration>) -> Result<Option<ExitStatus>> {
    let Some(limit) = timeout else {
        return Ok(Some(
            child.wait().context("failed to wait for the command")?,
        ));
    };
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().context("failed to wait for the command")? {
            return Ok(Some(status));
        }
        if started.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_input_and_collects_both_outputs() {
        let done = run(
            Command::new("/bin/sh").args(["-c", "cat; echo err >&2"]),
            Some(b"hello"),
            None,
            "sh",
        )
        .unwrap()
        .unwrap();
        assert!(done.status.success());
        assert_eq!(done.stdout_text(), "hello");
        assert_eq!(done.stderr_text(), "err\n");
    }

    #[test]
    fn times_out_and_reports_missing_commands() {
        let slow = run(
            Command::new("/bin/sleep").arg("5"),
            None,
            Some(Duration::from_millis(100)),
            "sleep",
        )
        .unwrap();
        assert!(slow.is_none());
        let err = run(&mut Command::new("/nonexistent/tool"), None, None, "Tool")
            .err()
            .unwrap();
        assert_eq!(
            diagnose::diagnose(&format!("{err:#}")),
            Some(diagnose::Failure::CommandNotFound)
        );
    }
}
