//! The error type Tauri commands return: an error's message with its whole cause chain,
//! sent to the front end as a plain string.
//!
//! It converts from `anyhow::Error`, `String` and `&str`, so commands use `?` on library
//! calls and on their own messages alike.

use serde::{Serialize, Serializer};

/// A command's error message, as the front end shows it.
#[derive(Debug)]
pub struct CmdError(String);

/// What Tauri commands return.
pub type CmdResult<T> = Result<T, CmdError>;

impl From<anyhow::Error> for CmdError {
    fn from(err: anyhow::Error) -> Self {
        CmdError(format!("{err:#}"))
    }
}

impl From<String> for CmdError {
    fn from(message: String) -> Self {
        CmdError(message)
    }
}

impl From<&str> for CmdError {
    fn from(message: &str) -> Self {
        CmdError(message.to_string())
    }
}

impl std::fmt::Display for CmdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for CmdError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn the_cause_chain_is_kept_and_sent_as_a_string() {
        let err: CmdError = Err::<(), _>(anyhow::anyhow!("disk full"))
            .context("failed to save")
            .unwrap_err()
            .into();
        assert_eq!(err.to_string(), "failed to save: disk full");
        assert_eq!(
            serde_json::to_string(&err).unwrap(),
            "\"failed to save: disk full\""
        );
    }
}
