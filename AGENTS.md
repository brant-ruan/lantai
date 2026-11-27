# AGENTS.md

For coding agents (and people) working in this repository. Keep it current: when a new convention
forms, a pitfall is worth recording, or the state of the project changes, update this file.

## Purpose

**Lantai** reads the session records that coding agents leave on the machine, turns them into one
versioned intermediate representation (IR), and writes daily, weekly and monthly work journals
from it, together with notes written by the person. The IR is the core asset; reports are its
first consumer. Each agent's format knowledge lives in its own adapter.

- [docs/SPEC.md](docs/SPEC.md): the design, the single source of truth. Change it before changing
  the design in code.
- [docs/ADAPTERS.md](docs/ADAPTERS.md): agent formats and how to add an adapter.
- [docs/ROADMAP.md](docs/ROADMAP.md): what comes next, what was parked or dropped and why.
- [docs/RELEASING.md](docs/RELEASING.md): versioning and releases.

Machine-specific notes (local toolchain, the model used for testing) go in `AGENTS.local.md`,
which is not committed. Read it when present; it wins on conflicts. Personal information and
personal configuration must only go there, never into the repository.

## The evening review

Curating a day's journal is an interactive process, not a button: the user first sees the day's
sessions laid out (details in a drawer on demand), creates or picks projects and assigns sessions
to them, ignores what does not matter, and only then generates. A project is a concept the user
creates once and keeps using; rules, regexes or a model may suggest assignments, the user
confirms. Report prompts can be saved, edited and set as default, for users to refine over time.

## Two top principles

These come from the project owner and take precedence over everything else.

**1. Modular, layered, decoupled.** The goal is a codebase that does not rot.

- Each crate and module does one thing; the top of each file says in a sentence or two what.
- Keep pure logic apart from IO: rule matching, slicing, anchor checks and dehydration are pure
  functions that never touch files or the database.
- Dependencies point one way; lower layers do not know upper layers exist. The core never contains
  an agent's format knowledge; callers pass it in through interfaces or closures.
- Shells (CLI, desktop app) parse arguments, call libraries and display results. A rule appearing
  in a shell means it belongs lower down.
- Extract logic the second time it appears, not the third.
- When a file grows too large to see its structure at a glance, split it by responsibility.

**2. No backward compatibility inside the code; every breaking change that affects users is
decided by the owner.**

- Inside the code (functions, types, modules between crates), once a change is decided, delete the
  old code: no old interfaces, compatibility branches or "deprecated" markers.
- Breaking changes that affect users have no default handling. They include the database schema
  (`SCHEMA_VERSION` in `crates/aj-store/src/schema.rs`), the IR (`IR_VERSION`), the `config.toml`
  format, CLI arguments and output, and export formats. **Stop and tell the owner before making
  one**: what changes, what it does to existing data or usage (projects, rules, prompts, reports
  and edits live only in the database; deleting it loses them), and the options (write a
  migration, read the old format, or accept the break and tell users what to do). The owner
  decides each case.
- Then do it, and describe the impact and what users must do under Breaking in `CHANGELOG.md`.
- Never default to "delete the database and rescan": sessions can be rebuilt from agent files,
  the user's own data cannot.

## Hard constraints

1. **Agent data is read only.** Never write, move or delete anything under `~/.claude`,
   `~/.codex`, `~/.gemini`, `~/.dsh`, `~/.workbuddy` or `~/.local/share/opencode`. Read only the
   session files: `~/.dsh` holds credential files (`.credentials.yaml`), so only `sessions/` is
   read; `~/.workbuddy` holds an account key (`keyblob`), a device ID, memories and settings, so
   only `projects/`; OpenCode's folder holds `auth.json` (provider credentials), so only
   `opencode.db` and `opencode.db-wal`. **Never open OpenCode's database directly, not even
   read-only**: SQLite creates or rewrites `-shm` / `-wal` in the source folder. Copy it into
   Lantai's data directory first (see [ADAPTERS.md](docs/ADAPTERS.md#opencode)), and when debugging
   query the copy in `cache/opencode/`, never the original with `sqlite3`.
2. **Format knowledge stays in adapters.** No agent-specific field names in `aj-core` or above.
3. **Adapters tolerate everything.** Unknown record types are counted and skipped; a line that
   fails to parse never aborts the file.
4. **IR changes bump `IR_VERSION`** and update the IR section of the spec and the fixtures'
   expected output. Whether to migrate is the owner's call (principle 2).
5. **Timestamps are stored in UTC.** Local dates are computed only when slicing and displaying.
6. **Privacy.** Sessions can contain secrets and private code.
   - Redact before storage and again before sending to a model (`aj-core/src/redact.rs`). The
     second pass is `Engine::complete`, which redacts the whole prompt; engines implement `send`
     and callers always use `complete`. When a new secret shape turns up, add a rule and a test.
   - Remote URLs in `.git/config` can embed access tokens (this is common). The raw URL may appear
     only inside `aj-core/src/project/git.rs`; everything else sees the normalized
     `host/owner/repo`.
   - Never put real session content in commits, docs, fixtures or outgoing requests; fixtures are
     redacted first.
   - Looking at real data while debugging is fine; keep only structure and counts in output, not
     content.
7. **Report generation runs with the fewest tools each engine allows and leaves nothing behind**
   (prompt on stdin, no session saved), so session content cannot act and Lantai's own calls never
   show up in scans. Nothing left may run commands, change files or go online; the bar and each
   CLI's invocation are in `docs/SPEC.md`, "Agent CLIs". For Claude Code, `--allowedTools ""` does
   not disable tools; `--tools ""` does.

## Layout

| Path | Responsibility |
| :--- | :--- |
| `crates/aj-ir` | IR types and version, agent identifiers and their registry, the exchange format header; no IO |
| `crates/aj-adapter` | `SourceAdapter` and shared helpers (line reading, zstd that tolerates an incomplete last frame, `text` for tag stripping and truncation, `json` for string fields and edit line counts, `limits` for truncation lengths) |
| `crates/aj-adapter-*` | one adapter per agent: `claude`, `codex`, `antigravity`, `dsh` (DeepSeek Harness), `workbuddy`, `opencode` |
| `crates/aj-sources` | per agent: session folders, detection, adapter construction; the only map from agent to adapter |
| `crates/aj-store` | SQLite persistence and statistics queries |
| `crates/aj-core` | scanning, daily slices, dehydration and redaction, statistics; `project/` splits rule matching (pure), Git detection (read-only file system) and assignment |
| `crates/aj-report` | digests, reports, engines, anchors, export, human journal |
| `crates/aj-backup` | backup archives, and upgrading the database after one (file names passed in by callers) |
| `crates/aj-update` | update check: parsing and comparison (pure) plus one request |
| `crates/aj-cli` | `aj`: `main.rs` only defines and dispatches; commands live in `cmd_*` modules by area |
| `app/` | Tauri v2 + React. `App.tsx` only composes views and cross-view actions; view state lives in `app/src/hooks/` (`usePeriodView` for day, week and month, `useProjectView`, `useStatsView`, …); the sidebar is composed from `components/sidebar/` |
| `fixtures/` | redacted session samples and expected output |
| `website/` | the project site (static); see [docs/WEBSITE.md](docs/WEBSITE.md) |

Dependencies: `aj-ir` ← adapters / `aj-store` ← `aj-core` ← `aj-report` ← `aj-cli` / `app`. Upper
layers never read an agent's raw files.

## Non-obvious notes

What the code does not make obvious. The design behind each is in the spec.

- **Report pipeline:** versions, input manifests, staleness and Update for daily, weekly and monthly
  reports are shared in `aj-report/src/report_io.rs`, as are their run options (`ReportOptions<P>`,
  where only the period type differs); `daily/`, `weekly/` and `monthly/` only orchestrate. Showing
  versions and saving a user's edit are `aj_report::versions`. Stage 1's worker pool is `stage1.rs`;
  only the main thread touches the database. Titles come from `structure::report_title` (rewritten
  on validation and on read; the model's title is dropped); weekly and monthly stats lines (`> `)
  come from `report_io::stats_line`. Daily generation checks each new section's anchors against that
  project's slices before splicing in sections kept by Update.
- **Prompts:** built-in prompts are synced into the database at startup; once a user edits one it
  becomes theirs and is no longer overwritten. Which prompts a language offers (its own plus the
  built-in language's) and which are defaults are `prompt_library::offered` and `default_ids`.
  Changing the digest or cluster prompts or their
  structure bumps `DIGEST_PROMPT_VERSION` or `BRIEF_PROMPT_VERSION`.
- **Edit marks:** `aj_report::edits` finds the model's original text per project section (the
  manifest's `section_hash` carries through edits and through Update for sections not rewritten).
  Never diff against "the latest generated version" as a whole: Update copies edited sections into
  generated versions. Line numbers refer to the displayed Markdown (rewritten title, sections
  reordered).
- **Human journal, data:** nodes in `human_nodes`, read and written by `aj-store/src/human.rs`;
  positions, moves and annotation attachment in `aj_report::human` (`attach.rs` is pure and
  reattaches at read time by anchors and text, never writing back); sort keys from
  `aj_core::order`. Human content is not sent to models, not counted in statistics, not exported
  yet. App commands take a scope (a period, or a project page plus granularity, handled in
  `aj_report::human::project`, which translates changes to the node's own period) and return all
  nodes of the scope after each change; the front end's `useHumanJournal` queues changes so a
  structural change never overtakes an unsaved edit. Project-page notes use `period_kind =
  "project"` and the project ID as `period_key` (`aj_report::human::PROJECT_PAGE`).
- **Human journal, positions:** a project note's `position` (`aj_store::HumanPosition`, top or
  bottom) is part of its sibling group; children copy the root. Only a root's Alt+↑/↓ crosses
  groups (`set_human_position` moves the subtree). Day notes, annotations and project-page notes
  are always bottom.
- **Human journal, dragging** (`hooks/useNoteDrag.ts`): groups declare themselves drop zones
  through `HumanBlocks`' `zone` (empty groups show a placeholder while dragging); the drop is
  computed from row positions and a 30 px indent step (matching `.human-children`) into
  `aj_report::human::Drop`; the back end's `move_to` moves several blocks in one transaction and
  checks same period, same project, and annotations only within one point. The drag handle's
  mousedown must `stopPropagation`, or marquee selection takes it. During drags and marquee
  selection `hooks/pointerDrag.ts` adds `is-pointer-dragging` to the body and report content takes
  no pointer events (WebKit otherwise leaves hover styles on rows the pointer crossed), so hit tests
  compare rectangles and cannot use `elementFromPoint`.
- **Human journal, spacing:** note rows and report rows both use 2 px margin plus 4 px padding. On
  the project page the report body is in a flex container where margins do not collapse, so
  `styles.css` zeroes them under `.rollup-entry`; measure both places when changing spacing.
- **Human journal, marquee and undo** (`hooks/useNoteMarquee.tsx`) is the one exception to
  "shortcuts live only on the menu": it handles Backspace / Delete / Esc / Cmd+Z only when focus is
  not in an input; undo inside an editor belongs to the editor. Delete commands return the deleted
  rows for undo. It prevents default on mousedown in report whitespace (to avoid selecting text
  while dragging), so focus never leaves the editor and it must blur the editor itself.
- **Human journal, remounting:** when the block being edited unmounts (a report finishes and the
  view remounts), `HumanBlocks.tsx`'s layout cleanup saves the draft and the caret; the same block
  remounting in the same commit resumes editing in its layout effect. The new copy renders before
  the old one's cleanup runs, so restoring cannot happen in a `useState` initializer.
- **Human journal, hover buttons:** "Annotate" on report points and × on notes float absolutely
  at the end of the line with an opaque background, taking no width and no height. Do not turn them
  back into a column: line spacing and wrapping would then differ between source filters. "Expand
  details" controls both the report outline and note folding (`hooks/useDetailsToggle.ts`).
- **Copy:** the copy buttons build Markdown synchronously on click
  (`components/human/journalMarkdown.ts`; clipboard writes cannot follow an `await`). Where notes
  go is computed in `components/human/layout.ts`, shared by `HumanLayer.tsx`, `ProjectNotes.tsx`
  and copying; change note placement there so display and copy stay in step.
- **Backup:** the archive format is `aj-backup`'s `FORMAT`, a user-facing format. Restore writes
  the whole database into the existing one with SQLite's online backup API
  (`Store::replace_with`); never replace the file instead, which breaks WAL consistency with
  `-wal` / `-shm`. Keychain secrets are not backed up. Configuration files included are
  `aj_core::data_files::CONFIG_FILES`; add new user config files in the data directory there.
- **Upgrading the database:** `Store::open` never migrates; it fails with `aj_store::NotReady`
  for any other schema version. Only `aj_backup::upgrade` migrates (`Store::migrate`), after
  writing a backup archive to `backups/`. The app checks first: `DatabaseGate.tsx` wraps
  everything, including `LanguageProvider`, and calls `database_status` (`cmd_database.rs`), which
  also syncs built-in prompts, so no other command opens the database before that; `AppState::new`
  does not open it. `aj` refuses an outdated database and points to `aj migrate`. A new migration
  needs nothing else: the dialog and the backup come with it.
- **Archived projects** (`logical_projects.is_archived`): rules pause through
  `aj_core::project::ActiveRules`; sessions already assigned by a rule stay; `candidate_projects`
  and model suggestion parsing skip archived projects; front-end assignment menus filter through
  `assignableProjects` in `projectOptions.ts`.
- **Creating projects and rules:** shells validate the whole rule set first
  (`aj_core::project::validate_rules`, pure), then write it in one transaction
  (`Store::create_project_with_rules`, `Store::add_rules`), so a bad rule creates nothing.
- **Renaming a project:** `aj_store::rename_project` rewrites `## <old name>` in every report
  version in one transaction (callers pass `structure::rename_project_heading`), so per-project
  roll-ups keep working.
- **Agents:** an adapter's `probe` judges by the version of the local data; the Agents settings
  page never claims support just because a folder exists. Agents are identified by
  `aj_ir::AgentId`, an open string; registered identifiers and product names are in `aj_ir`'s
  registry, and unknown but well-formed identifiers are stored, counted and shown as is (never an
  error); only registered agents with an adapter are scanned.
  `[agents.<key>]` in `config.toml` decides which agents are scanned and pre-digested (only Claude
  Code by default); re-evaluating assignments uses every supported adapter.
- **Day boundary:** changing `[slicing] day_boundary_hour` calls `aj_core::slice::reslice_all`;
  slices moving to another date inherit ignore state and project override. The front end's
  `hooks/useDateRollover.ts` refreshes "today" on the hour (the app lives in the Dock and would
  otherwise stay on an old date).
- **Parallel generation:** `AppState.report_jobs` has one job per key (date, `date|project`, week
  key, month key), each with its own cancel flag; `report-progress` events carry the key.
  `Store::open` sets a 15-second `busy_timeout` so concurrent writers wait.
- **Sidebar report batches:** `useReportBatch` owns each view's selection and queue, and
  `aj_report::batch` plans from the same inputs as single reports. Monthly generation requires
  usable weekly sections, and editing or generating a weekly report can invalidate a monthly
  plan. The app refreshes batch previews after report metadata changes.
- **Models and keys:** `config.toml` has providers `[providers.*]`, model aliases `[models.*]` and
  `default_model`. Keys are stored in the macOS keychain by `keystore.rs` (through
  `/usr/bin/security`); keys are never read from environment variables. Nothing is built in: with
  no alias, `build_engine` fails with `diagnose::NO_MODEL`. Agent CLIs are found only on request
  (`aj_report::local_tools`, which runs only `--version`); a CLI becomes an engine only if it
  leaves no session and nothing left can act (`docs/SPEC.md`, "Agent CLIs"), so do not add one
  just because it has a print mode. Engines live in `aj_report::engine` (one module per CLI,
  sharing `aj-report/src/process.rs` with finding local tools). Codex's switches go through
  `--disable` and `--strict-config`, which fail on an unknown name: keep it that way (fail
  closed), and after a CLI update run `cargo test -p aj-report -- --ignored tool_audit` and
  refresh the README table from its output. Codex echoes the prompt on stderr, so its errors keep
  only error lines. Engine error text is sorted by `aj_report::diagnose`; when changing an
  engine's error messages, keep its markers (`COMMAND_NOT_FOUND`) and its tests.
- **Packaging:** `npm run build:app` (in `app/`) builds only the `.app` and copies it to `build/`
  (ignored). Tauri's `beforeBuildCommand` runs `scripts/bundle-cli.sh`, which builds `aj` and
  places it at `Contents/MacOS/aj` through `bundle.macOS.files`. The whole app is ad-hoc signed
  (`signingIdentity: "-"`); the embedded `aj` must be signed first or the bundle's signature does
  not verify. Pushing a `v*` tag runs `.github/workflows/release.yml` (universal `.dmg`, draft
  release). The CLI installer (`cmd_cli.rs`) only creates a symlink in `~/.local/bin` and never
  replaces a file Lantai did not create. Update checks (`aj-update`, `cmd_update.rs`) are off by
  default and only read the public GitHub Releases API.
- **Running:** `shell_env.rs` imports the login shell's environment so the app finds `claude` when
  launched from Finder. The app is single instance, and `tauri dev` shares the identifier
  (`me.wohin.lantai`) with installed builds, so only one can run at a time.
- **Native menu:** `menu.rs` replaces Tauri's default menu entirely, so Edit, Hide, Minimize, Full
  Screen and other standard items must be listed there; without Edit, Cmd+C/V/Z stop working in
  inputs. Custom items send `menu-action` events handled by `hooks/useMenuActions.ts`. Shortcuts
  are defined only on the menu; the front end does not listen for keys.
- **App icons:** candidates are in `app/src-tauri/app-icons/`; add a PNG and register it in
  `ICONS` in `app_icon.rs`.
- **Search:** no index; plain `LIKE` scans (the spec says when to switch to FTS5).
  `aj-store/src/search.rs` only produces candidates, term splitting and snippets are in
  `aj_core::search`, the report layer and anchor back-references in `aj_report::search`. Matching
  is case-insensitive for ASCII only, like SQLite `LIKE`, so snippet byte offsets line up.
- **Languages:** interface language and report language are separate settings. Report languages
  are registered in `aj_report::lang`; text the program writes into reports is in
  `prompts/<language>/locale.toml`. Prompt hashes exclude the output language instruction; the
  language is stored separately on report versions and caches. Interface strings are in
  `app/src/locales/`: `en.ts` is the source of keys, so add a string there first and then to the
  other languages (`zh-Hans.ts` ends with `satisfies Messages`, so a missing key fails to compile).
  Components use `useT()`; dates and weekdays use the `Intl`-based helpers in `i18n.tsx`.
- **Names and the About page:** the app is 兰台 / Lantai (`productName` in `tauri.conf.json` and
  `macos/*.lproj/InfoPlist.strings`). The internal crate prefix `aj-`, the `aj` command and `AJ_*`
  variables keep their names for now. The About text is `app/src/content/about.<language>.md` (the
  two languages correspond section by section). It is justified, and every paragraph's last line
  must be at least half the width: `hooks/useFullLastLines.ts` adjusts letter spacing (Chinese) or
  word spacing (English) within limits; paragraphs it cannot fix need different wording. The page
  has a fixed width (584 px); measure in WebKit after changing the text.
- **Active time** summed across sessions always takes the union of intervals
  (`aj_core::slice::union_minutes`), never the sum of slices' `active_minutes`, or parallel
  sessions count twice. The sidebar's date list uses `aj_core::overview::day_overviews`; the
  store's `day_counts` has no active time for that reason.
- **Slice conditions in SQL** (included in reports, effective project, unassigned) are written
  once in `aj-store/src/sql.rs` and mirror `SliceRow::is_ignored` and `effective_project`;
  change both together. `Store::project_summaries` counts over every slice; use
  `Store::projects` or `project_names` when only names are needed.

## Conventions

- **Languages in the repository:**
  - Source code contains no Chinese: identifiers, comments, strings (CLI output, errors, test
    data) are English. Exceptions are product content: interface language files
    (`app/src/locales/*.ts`), the About pages (`app/src/content/about.*.md`) and prompts.
  - Documentation is in English (`docs/`, this file, `CHANGELOG.md`). The README exists in English
    (`README.md`) and Chinese (`README.zh-CN.md`); they correspond section by section, link to each
    other, and match the website. Change both together.
  - Commit messages are in English.
  - `fixtures/` session samples are data and may contain any language.
- **Prompts** are product content, per language in `crates/aj-report/prompts/<language>/*.md`
  (the system prompt `prompts/system.md` is a single English file), included with `include_str!`.
  When changing one language's prompt structure, change the others too (tests check that files and
  placeholders match).
- **README screenshots** `docs/images/demo-{light,dark}.png` come from the website's demo with
  made-up data (headless Chrome rendering only `#lantai-demo`, 2× resolution, transparent
  background), never from real sessions. Retake them when the demo changes noticeably.
- **Website demo:** after any change to the app's interface (layout, labels, controls, flows),
  check whether the website's interactive demo (`website/assets/demo*.js`, `demo.css`, both
  pages) still matches, and update it by hand if not; it shares no code with `app/` (see
  [docs/WEBSITE.md](docs/WEBSITE.md), "The interactive demo").
- **Website deployment:** keep the Pages upload and publish steps in separate jobs. The publish
  job waits for the upload job to avoid querying artifact metadata before it becomes visible
  (see [docs/WEBSITE.md](docs/WEBSITE.md), "Deployment").
- **Design first:** change [docs/SPEC.md](docs/SPEC.md) before the code. Reference spec sections
  by title (`docs/SPEC.md, "Distribution"`), not by number.
- **Adapters** need fixtures and golden tests for their parsing behaviour.
- **Versioning** (details in [docs/RELEASING.md](docs/RELEASING.md)): semantic versioning, single
  source in `[workspace.package] version` of the root `Cargo.toml` (`tauri.conf.json` and
  `package.json` carry none). Never edit the version by hand; release with
  `scripts/release.sh <version>`. User-visible changes add an entry under `## [Unreleased]` in
  `CHANGELOG.md`; bumping `IR_VERSION` or `SCHEMA_VERSION` updates that section's "IR x · Schema
  y" line and adds what users must do under Breaking.
- **Before committing:** `cargo fmt --all`, `cargo clippy --workspace --all-targets` (no
  warnings), `cargo test --workspace`; for front-end changes also `npx tsc --noEmit` in `app/`.
- **Tauri commands** that touch the database or call a model are `#[tauri::command(async)]` or
  `async fn`: synchronous commands run on the main thread in Tauri v2, and a query or a wait on the
  write lock (`busy_timeout` of 15 seconds) would freeze the UI. Only commands that need the main
  thread (windows, Dock icon) stay synchronous. Commands return `CmdResult<T>`
  (`app/src-tauri/src/error.rs`): `?` works on `anyhow` results and on message strings, and the
  error reaches the front end as a string with its cause chain.

## Commands

```bash
cargo test --workspace                 # all tests
cargo build --release                  # target/release/aj
UPDATE_GOLDEN=1 cargo test -p aj-adapter-claude --test golden   # rewrite expected output (review the diff first)
```

Keep debugging data out of the real database with these variables:

- `AJ_DATA_DIR`: data directory (default: `lantai` in the system's application data directory)
- `AJ_CLAUDE_ROOT`: Claude Code sessions (default `~/.claude/projects`)
- `AJ_CODEX_ROOT`: Codex sessions (default: both `~/.codex/sessions` and
  `~/.codex/archived_sessions`; when set, only this folder)
- `AJ_ANTIGRAVITY_ROOT`: Antigravity sessions (default: both `~/.gemini/antigravity/brain` and
  `~/.gemini/antigravity-ide/brain`; when set, only this folder)
- `AJ_DSH_ROOT`: DeepSeek Harness sessions (default `~/.dsh/sessions`)
- `AJ_WORKBUDDY_ROOT`: WorkBuddy sessions (default `~/.workbuddy/projects`)
- `AJ_OPENCODE_ROOT`: the folder containing `opencode.db` (default `$XDG_DATA_HOME/opencode`, else
  `~/.local/share/opencode`, also on macOS)
- `AJ_MODEL`: model alias for generating reports (a `[models.*]` name in `config.toml`)
- `AJ_KEYSTORE=file`: store keys in `secrets.toml` in the data directory (tests and debugging,
  avoids the keychain)

Providers and model aliases are configured in `<data dir>/config.toml` (format at the top of
`crates/aj-report/src/config/mod.rs`); keys are entered in the settings page. `aj models --test`
checks connectivity. `aj report daily --dry-run` prints the material that would be sent without
calling a model, which is handy for debugging dehydration.
