# Changelog

All notable changes to Lantai are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). While Lantai is at `0.x`, a minor
release may include breaking changes; those are always listed under **Breaking**, together with
what, if anything, you need to do.

Each release also notes the internal format versions it ships with, so you can tell whether
stored data or exported IR is affected:

- **IR**: the intermediate representation of sessions (`aj_ir::IR_VERSION`).
- **Schema**: the local SQLite database (`aj_store::SCHEMA_VERSION`).

## [Unreleased]

## [0.1.0] - 2026-11-22

The first public release. IR 0.3.0 · Schema 11.

### Added

- **Six agents, one format.** Reads Claude Code, Codex CLI, Antigravity, DeepSeek Harness,
  WorkBuddy and OpenCode (1.x storage) into one versioned intermediate representation (IR).
  Agent folders are only read; OpenCode's database is copied before it is opened. Settings →
  Agents shows which agents were found, which versions wrote their data, and whether that format
  is fully, partly or not yet supported. Turning an agent off stops scanning it and keeps what is
  already stored. Agents are named by open identifiers from a registry, so sessions from agents
  without an adapter can still be stored and counted; `aj export` writes the IR exchange format
  (a header line per session, then its events).
- **The evening review.** Each day's sessions laid out with time, turns and changed files;
  details in a drawer; user-defined projects with folder, remote and regex rules plus model
  suggestions; ignore, include and bulk moves; batch generate or update across days; a
  configurable day boundary. Projects can be archived or deleted (with a summary of what goes
  with them).
- **Reports.** Daily, weekly and monthly reports written as outlines by project, with anchors
  checked against the source turns. Digests are cached, long sessions are summarized in parts,
  and **Update** rewrites only the projects whose sessions changed, keeping your edits elsewhere.
  Reports describe what happened in sessions and do not judge progress. Every version is kept,
  with the prompt it was written with; export to Markdown with anchors kept, as footnotes, or
  removed.
- **Prompts.** Several prompts per report type, built-in ones you can edit, and a default per
  language; the rules that keep anchors valid are always appended.
- **Human journal.** Write day notes, project notes (at the top or bottom of a project's
  section, including projects no agent touched) and annotations on report points, in outlines of
  plain-text blocks with links to turns, days and weeks. Drag, fold, select and delete with undo.
  Regenerating never touches what you wrote; annotations follow their point across versions.
  Lines you changed in the model's text are marked. A source switch shows agents and people
  together, or either alone, and copy as Markdown follows it.
- **Project pages** gather a project's sections from daily, weekly or monthly reports without
  another model call, with its notes for every period and notes of its own.
- **Statistics:** active time (parallel sessions counted once), streaks, changed files across
  agents and worktrees, tool profile, hourly rhythm, token usage by model and agent, and how much
  of your history has reports.
- **Search** across reports, notes and session text (Cmd+F or `aj search`), filtered by project,
  agent and dates, with links from session hits to the reports that cite them.
- **Models:** any OpenAI-compatible endpoint, including local servers, with presets for common
  services, or the signed-in Claude Code CLI, which **Find tools** (or `aj models --find`) picks
  up from your `PATH` when you ask. Nothing is set up behind your back: the first model you add
  becomes the default. Codex CLI, Antigravity and OpenCode are recognised but not used to write
  reports, since they cannot run with every tool off and without leaving a session behind. Common
  failures (no model, command not found, not signed in, quota reached) say what to do next and
  keep the full error under Details. API keys are stored in the macOS keychain.
- **Languages:** English and Simplified Chinese, chosen separately for the interface and for
  reports, each with its own tuned prompts.
- **Data:** everything stays on your Mac; text is redacted before it is stored and again before
  it is sent to a model. Back up and restore all data and settings from Settings → System (API
  keys excepted).
- **The app:** a universal disk image (Apple silicon and Intel) on GitHub Releases, not yet
  signed with an Apple Developer ID (the README explains the first open). The `aj` command ships
  inside the app and can be linked into `~/.local/bin` from Settings → System. Update checks are
  off by default; when on, Lantai only reads GitHub's public release list and never downloads or
  installs anything.

[Unreleased]: https://github.com/brant-ruan/lantai/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/brant-ruan/lantai/releases/tag/v0.1.0
