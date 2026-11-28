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

### Breaking

- **`aj report show` prints a report as the app shows it:** the program's title and project
  sections in your sidebar order. A report generated with `aj report daily` / `weekly` /
  `monthly` is printed and written with `--out` in the same form; `--dry-run` output is
  unchanged. If a script of yours needs the text as stored (what `aj report show` printed
  before), add `--raw`.

### Fixed

- `aj days` matches the app's date list: days whose sessions are all ignored are left out,
  slice counts and active time leave ignored slices out, and active time counts overlapping
  sessions once instead of adding them up (so it can be lower than before). A new column counts
  the day's ignored slices.
- Creating a project with rules, or adding several rules at once (in the app or with `aj project
  new` / `aj rule add`), no longer leaves a partial result when one rule is invalid or already
  exists: nothing is created.

## [0.3.0] - 2026-11-27

IR 0.3.0 · Schema 12.

### Breaking

- **The database moves to schema 12** (digests, digest parts and project briefs record which model
  wrote them). On first start, Lantai asks before upgrading and backs your data up first; on the
  command line, run `aj migrate`. Nothing is lost: existing digests stay valid and show their model
  as unknown. An older Lantai cannot open the upgraded database; to go back, restore the backup
  in it.

### Added

- **Which model wrote what:** the slice drawer names the model that wrote a digest and says when
  generating with the selected model would digest the slice again; the report panel names the
  model of each version (status tooltip, version lists, compare), and after an Update with
  another model, which model wrote which project's section. The estimate line says how many
  pending digests are pending only because another model wrote them. `aj digest` prints the
  model too.
- Codex CLI can write reports. **Find tools** (or `aj models --add codex`) adds it; it runs
  `codex exec` without saving a session, with every feature that adds a tool turned off, a
  read-only sandbox and nothing approved. The README lists the few declarations Codex still sends.
- Agent CLI providers list their models in **Settings → Models**: Claude Code's shorthands, and
  the models in Codex's picker with the reasoning efforts each accepts.
- Reasoning effort for Claude Code and Codex aliases, and a temperature (0 to 2) for
  OpenAI-compatible aliases. Both are optional; left blank, nothing is sent and caches stay valid.
  For agent CLIs the effort is picked from the levels the chosen model lists, and saving refuses
  any other; for APIs it is typed, with a warning when the model lists levels that do not include
  it.
- When a new version changes how data is stored, Lantai asks before upgrading it and first saves
  a backup (database and settings, the same zip as **Back up data**) to `backups/` in the data
  folder, or a folder you choose. A database from a newer Lantai now shows a message instead of
  the app failing to start. `aj migrate` does the same on the command line.

### Changed

- `aj` no longer upgrades the database on its own: on data from an older Lantai it stops and
  asks you to run `aj migrate` (or open the app).
- Backups before an upgrade are now zip archives in `backups/` instead of
  `journal.sqlite.bak-schema-<n>` files next to the database. Existing `.bak-schema-` files are
  left as they are; delete them when you no longer need them.
- The rule for using an agent CLI is now "the fewest tools it allows, and nothing left that can
  run commands, change files or go online" instead of "every tool off". The README has a table of
  what each CLI still declares.
- `aj models` shows the reasoning effort of agent CLI aliases and the temperature of
  OpenAI-compatible ones.
- Claude Code's model list adds `fable`.

### Fixed

- **Delete** on models and providers in **Settings → Models** did nothing in the app; it now asks
  in the app's own dialog. A provider that models still use names them instead of a greyed-out
  button.

## [0.2.0] - 2026-11-25

IR 0.3.0 · Schema 11 (unchanged; no action needed when upgrading).

### Added

- Batch generation, updating and draft / final actions in the weekly and monthly sidebar lists.
  The monthly batch shows which weekly reports are missing before generation.

### Fixed

- Generated daily report sections reject anchors to sessions from another project.
- The built-in monthly prompts name minor outcomes worth revisiting and group multiple outcomes
  under an expandable point with source links, replacing count-only summaries.

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

[Unreleased]: https://github.com/brant-ruan/lantai/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/brant-ruan/lantai/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/brant-ruan/lantai/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/brant-ruan/lantai/releases/tag/v0.1.0
