//! Report prompts: several per report kind and report language.
//!
//! Built-in prompts are seeded at startup; editing a built-in prompt promotes it to
//! a customized prompt (`is_builtin = 0`) so startup sync preserves user changes.
//! The default for a kind and report language is either the prompt the user chose
//! (`prompt_defaults`, any language) or, without a choice, the first built-in prompt;
//! the fallback across languages is decided by the report layer.

use anyhow::{bail, Result};
use chrono::Utc;
use rusqlite::{params, OptionalExtension, Row};

use crate::sql::ts;
use crate::Store;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptRow {
    pub id: i64,
    /// `daily` | `weekly` | `monthly`
    pub kind: String,
    /// Report language the prompt is written for, as a language tag.
    pub language: String,
    pub name: String,
    pub body: String,
    pub is_builtin: bool,
    pub updated_at: String,
}

/// A prompt shipped with the program.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinPrompt<'a> {
    pub kind: &'a str,
    pub language: &'a str,
    pub name: &'a str,
    pub body: &'a str,
}

const COLS: &str = "id, kind, language, name, body, is_builtin, updated_at";

fn from_row(r: &Row) -> rusqlite::Result<PromptRow> {
    Ok(PromptRow {
        id: r.get(0)?,
        kind: r.get(1)?,
        language: r.get(2)?,
        name: r.get(3)?,
        body: r.get(4)?,
        is_builtin: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

impl Store {
    /// Make the built-in rows match `builtins` exactly. Called once at startup.
    pub fn sync_builtin_prompts(&mut self, builtins: &[BuiltinPrompt]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let now = ts(Utc::now());
        for b in builtins {
            // A user prompt that happens to share the name keeps it; the built-in is skipped
            // rather than silently overwriting user content.
            tx.execute(
                "INSERT INTO prompts (kind, language, name, body, is_builtin, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)
                 ON CONFLICT(kind, language, name) DO UPDATE SET
                   body = excluded.body,
                   updated_at = CASE WHEN body = excluded.body THEN updated_at ELSE excluded.updated_at END
                 WHERE is_builtin = 1",
                params![b.kind, b.language, b.name, b.body, now],
            )?;
        }
        // Drop built-ins that are no longer shipped.
        let existing: Vec<(i64, String, String, String)> = {
            let mut stmt =
                tx.prepare("SELECT id, kind, language, name FROM prompts WHERE is_builtin = 1")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<rusqlite::Result<_>>()?;
            rows
        };
        for (id, kind, language, name) in existing {
            let shipped = builtins.iter().any(|b| {
                b.kind == kind && b.language == language && b.name.eq_ignore_ascii_case(&name)
            });
            if !shipped {
                tx.execute("DELETE FROM prompts WHERE id = ?1", [id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Prompts, optionally of one kind and one language, built-ins first.
    pub fn list_prompts(
        &self,
        kind: Option<&str>,
        language: Option<&str>,
    ) -> Result<Vec<PromptRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLS} FROM prompts
             WHERE (?1 IS NULL OR kind = ?1) AND (?2 IS NULL OR language = ?2)
             ORDER BY kind, language, is_builtin DESC, name"
        ))?;
        let rows = stmt
            .query_map(params![kind, language], from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn prompt(&self, id: i64) -> Result<Option<PromptRow>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLS} FROM prompts WHERE id = ?1"),
                [id],
                from_row,
            )
            .optional()?)
    }

    /// Find a prompt by name within a kind and language, case-insensitively.
    pub fn find_prompt(&self, kind: &str, language: &str, name: &str) -> Result<Option<PromptRow>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {COLS} FROM prompts WHERE kind = ?1 AND language = ?2 AND name = ?3"
                ),
                params![kind, language, name.trim()],
                from_row,
            )
            .optional()?)
    }

    /// The prompt the user chose as the default for `kind` reports written in `language`.
    pub fn chosen_default_prompt(&self, kind: &str, language: &str) -> Result<Option<PromptRow>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {} FROM prompt_defaults d JOIN prompts p ON p.id = d.prompt_id
                     WHERE d.kind = ?1 AND d.language = ?2",
                    COLS.split(", ")
                        .map(|c| format!("p.{c}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                params![kind, language],
                from_row,
            )
            .optional()?)
    }

    /// The first built-in prompt of `kind` written for `language`.
    pub fn first_builtin_prompt(&self, kind: &str, language: &str) -> Result<Option<PromptRow>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {COLS} FROM prompts WHERE kind = ?1 AND language = ?2 AND is_builtin = 1
                     ORDER BY id LIMIT 1"
                ),
                params![kind, language],
                from_row,
            )
            .optional()?)
    }

    pub fn create_prompt(&self, kind: &str, language: &str, name: &str, body: &str) -> Result<i64> {
        let name = name.trim();
        if name.is_empty() {
            bail!("prompt name must not be empty");
        }
        if body.trim().is_empty() {
            bail!("prompt body must not be empty");
        }
        if self.find_prompt(kind, language, name)?.is_some() {
            bail!("a {kind} prompt named \"{name}\" already exists for {language}");
        }
        let now = ts(Utc::now());
        self.conn.execute(
            "INSERT INTO prompts (kind, language, name, body, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![kind, language, name, body, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn require_editable(&self, id: i64) -> Result<PromptRow> {
        let Some(prompt) = self.prompt(id)? else {
            bail!("no prompt with ID {id}");
        };
        if prompt.is_builtin {
            bail!(
                "\"{}\" is a built-in prompt and cannot be changed; copy it to a new prompt first",
                prompt.name
            );
        }
        Ok(prompt)
    }

    pub fn update_prompt_body(&self, id: i64, body: &str) -> Result<()> {
        if self.prompt(id)?.is_none() {
            bail!("no prompt with ID {id}");
        }
        if body.trim().is_empty() {
            bail!("prompt body must not be empty");
        }
        self.conn.execute(
            "UPDATE prompts SET body = ?2, is_builtin = 0, updated_at = ?3 WHERE id = ?1",
            params![id, body, ts(Utc::now())],
        )?;
        Ok(())
    }

    pub fn rename_prompt(&self, id: i64, new_name: &str) -> Result<()> {
        let prompt = self.require_editable(id)?;
        let new_name = new_name.trim();
        if new_name.is_empty() {
            bail!("prompt name must not be empty");
        }
        if self
            .find_prompt(&prompt.kind, &prompt.language, new_name)?
            .is_some_and(|p| p.id != id)
        {
            bail!(
                "a {} prompt named \"{new_name}\" already exists for {}",
                prompt.kind,
                prompt.language
            );
        }
        self.conn.execute(
            "UPDATE prompts SET name = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, new_name, ts(Utc::now())],
        )?;
        Ok(())
    }

    /// Delete a user prompt. Any default it was falls back to the built-in one.
    pub fn delete_prompt(&mut self, id: i64) -> Result<()> {
        self.require_editable(id)?;
        self.conn
            .execute("DELETE FROM prompts WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Make prompt `id` the default for its kind's reports written in `language`.
    pub fn set_default_prompt(&mut self, id: i64, language: &str) -> Result<()> {
        let Some(prompt) = self.prompt(id)? else {
            bail!("no prompt with ID {id}");
        };
        self.conn.execute(
            "INSERT INTO prompt_defaults (kind, language, prompt_id) VALUES (?1, ?2, ?3)
             ON CONFLICT(kind, language) DO UPDATE SET prompt_id = excluded.prompt_id",
            params![prompt.kind, language, id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILTINS: [BuiltinPrompt; 3] = [
        BuiltinPrompt {
            kind: "daily",
            language: "zh-Hans",
            name: "standard",
            body: "standard body",
        },
        BuiltinPrompt {
            kind: "daily",
            language: "zh-Hans",
            name: "concise",
            body: "concise body",
        },
        BuiltinPrompt {
            kind: "daily",
            language: "en",
            name: "standard",
            body: "english body",
        },
    ];

    fn store() -> Store {
        let mut store = Store::open_in_memory().unwrap();
        store.sync_builtin_prompts(&BUILTINS).unwrap();
        store
    }

    #[test]
    fn sync_seeds_builtins_per_language_and_is_repeatable() {
        let mut store = store();
        store.sync_builtin_prompts(&BUILTINS).unwrap();
        assert_eq!(store.list_prompts(Some("daily"), None).unwrap().len(), 3);
        assert_eq!(
            store.list_prompts(Some("daily"), Some("en")).unwrap().len(),
            1
        );
        assert_eq!(
            store
                .first_builtin_prompt("daily", "zh-Hans")
                .unwrap()
                .unwrap()
                .name,
            "standard"
        );
        assert!(store.first_builtin_prompt("daily", "ja").unwrap().is_none());

        // A changed text replaces the built-in body; a dropped built-in disappears.
        let updated = [BuiltinPrompt {
            kind: "daily",
            language: "zh-Hans",
            name: "standard",
            body: "new body",
        }];
        store.sync_builtin_prompts(&updated).unwrap();
        let all = store.list_prompts(None, None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].body, "new body");
    }

    #[test]
    fn builtins_cannot_be_renamed_or_deleted_but_body_edits_persist() {
        let mut store = store();
        let id = store
            .find_prompt("daily", "zh-Hans", "Standard")
            .unwrap()
            .unwrap()
            .id;
        assert!(store.rename_prompt(id, "y").is_err());
        assert!(store.delete_prompt(id).is_err());

        store
            .update_prompt_body(id, "custom standard body")
            .unwrap();
        let edited = store.prompt(id).unwrap().unwrap();
        assert!(!edited.is_builtin);
        assert_eq!(edited.body, "custom standard body");

        // Startup sync does not overwrite user-edited prompts.
        store.sync_builtin_prompts(&BUILTINS).unwrap();
        let persisted = store.prompt(id).unwrap().unwrap();
        assert_eq!(persisted.body, "custom standard body");
    }

    #[test]
    fn names_are_unique_per_kind_and_language() {
        let store = store();
        let id = store
            .create_prompt("daily", "zh-Hans", "mine", "my body")
            .unwrap();
        assert!(store
            .create_prompt("daily", "zh-Hans", "MINE", "dup")
            .is_err());
        assert!(store
            .create_prompt("daily", "zh-Hans", "standard", "clash")
            .is_err());
        assert!(store
            .create_prompt("daily", "zh-Hans", "empty", "  ")
            .is_err());
        // The same name is fine under another kind or language.
        store
            .create_prompt("weekly", "zh-Hans", "mine", "weekly body")
            .unwrap();
        store
            .create_prompt("daily", "en", "mine", "english body")
            .unwrap();

        store.update_prompt_body(id, "edited").unwrap();
        store.rename_prompt(id, "tuned").unwrap();
        assert!(store.rename_prompt(id, "concise").is_err());
    }

    #[test]
    fn defaults_are_chosen_per_language_and_cleared_with_their_prompt() {
        let mut store = store();
        assert!(store
            .chosen_default_prompt("daily", "zh-Hans")
            .unwrap()
            .is_none());

        // A prompt written for English may be the default for Chinese reports.
        let en = store
            .create_prompt("daily", "en", "mine", "my body")
            .unwrap();
        store.set_default_prompt(en, "zh-Hans").unwrap();
        assert_eq!(
            store
                .chosen_default_prompt("daily", "zh-Hans")
                .unwrap()
                .unwrap()
                .id,
            en
        );
        assert!(store
            .chosen_default_prompt("daily", "en")
            .unwrap()
            .is_none());

        let zh = store
            .find_prompt("daily", "zh-Hans", "concise")
            .unwrap()
            .unwrap()
            .id;
        store.set_default_prompt(zh, "zh-Hans").unwrap();
        assert_eq!(
            store
                .chosen_default_prompt("daily", "zh-Hans")
                .unwrap()
                .unwrap()
                .id,
            zh
        );

        store.set_default_prompt(en, "en").unwrap();
        store.delete_prompt(en).unwrap();
        assert!(store
            .chosen_default_prompt("daily", "en")
            .unwrap()
            .is_none());
    }

    #[test]
    fn sync_does_not_overwrite_a_user_prompt_with_the_same_name() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .create_prompt("daily", "zh-Hans", "standard", "user text")
            .unwrap();
        store.sync_builtin_prompts(&BUILTINS).unwrap();
        let p = store
            .find_prompt("daily", "zh-Hans", "standard")
            .unwrap()
            .unwrap();
        assert_eq!((p.body.as_str(), p.is_builtin), ("user text", false));
    }
}
