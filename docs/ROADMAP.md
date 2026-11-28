# Roadmap

What Lantai does today, what comes next, and what was set aside. The design of what exists is in
[SPEC.md](SPEC.md); the formats of each agent are in [ADAPTERS.md](ADAPTERS.md).

## Where it stands (0.4.0)

- **Sources:** Claude Code, Codex CLI, Antigravity, DeepSeek Harness, WorkBuddy and OpenCode 1.x,
  read into one versioned IR, with a compatibility check per source; agents are named by open
  identifiers, and `aj export` writes the IR exchange format.
- **Models:** any OpenAI-compatible endpoint (with presets, local servers included), or the Claude
  Code and Codex CLIs, found on request and run with the fewest tools each allows; no model is
  built in. Reasoning effort and temperature per alias; digests and reports record which model
  wrote them. Common failures are explained.
- **Evening review:** a day's sessions laid out by project, assignment rules and suggestions,
  ignoring sessions, batch operations across days, a configurable day boundary.
- **Reports:** daily, weekly and monthly reports as multi-level outlines with checked anchors;
  cached digests, staleness detection and per-project updates; batch generation in every sidebar
  list; several prompts per report type; Chinese and English reports.
- **Human journal:** day notes, project notes, annotations on report points, project-page notes;
  marks where a person edited the model's text; source filter (both, agent only, human only);
  copy as Markdown.
- **Around it:** project pages, a statistics dashboard (including token use), search over reports
  and session text, backup and restore, database upgrades that ask first and back up before
  migrating, a universal macOS app with the `aj` CLI inside, opt-in update checks.

## Next

### Custom session folders per agent (first)

Some people keep an agent's sessions somewhere other than the default (a moved home, a synced
drive, `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and the like). Today the only way to point Lantai there
is an `AJ_*_ROOT` environment variable, which the desktop app never sees.

- Settings → Agents lets the user add one or more folders per agent, replacing or adding to the
  defaults; stored as `[agents.<key>] roots = [...]` in `config.toml` (a new optional key).
  `aj_sources` takes them in place of the environment variable when set.
- Validate on save with the adapter's `probe` (folder exists, data in a version Lantai reads) and
  show the result next to each folder; a folder that does not look like that agent's sessions is
  warned about, not silently scanned.
- Changing folders rescans that agent; sessions already stored stay. The same session reached
  through two folders must not be stored twice (session IDs already deduplicate).
- Agent folders stay read only (hard constraint 1), whatever path the user picks.

### Human journal, phase 3: notes as report material

The rule: **notes on a period become material for the report one level up.** Annotations and day
notes stay for people only.

| Content | Daily report | Weekly report | Monthly report |
| :--- | :--- | :--- | :--- |
| Annotations | not sent | not sent | not sent |
| A day's project notes | shown on the canvas only | sent, by project and day | through the weekly report |
| A week's project notes | — | shown only | sent |
| Day notes, project-page notes | not sent | not sent | not sent |

- Notes enter the material marked as human-written, with anchors `[#H12]`; the valid anchor set
  includes the notes sent, and the anchor opens the note.
- `report_rules.md` changes: "describe sessions, don't judge progress" still holds, but progress
  stated in a human note (with an `[#H…]` anchor) may be reported.
- A project with notes but no sessions still gets a weekly section, without a stats line.
- Notes count in the input manifest, so editing a week's notes makes its weekly report stale.
  They are redacted before they are sent, like everything else.
- Export options for the human layer: include it or not, and how to mark authorship.

### Human journal, phase 4: images

Paste or drop images into notes; stored by content hash under `attachments/` in the data
directory and shown through Tauri's asset protocol; copied to `assets/` on export; only the alt
text is sent to a model. Backups must then include attachments.

### Sub-agent activity in the parent session

Today each adapter reads only main sessions; a sub-agent appears as one `Subagent` action in its
parent (the task and the returned summary). Its own tool calls, file changes and tokens are not
counted. The formats are surveyed in [ADAPTERS.md](ADAPTERS.md#sub-agents). The plan:

1. **Merge into the parent, never a session of its own.** Sub-agent events hang under the parent
   turn that started them; an asynchronous sub-agent that runs across turns or days still counts
   towards the day and project of that turn.
2. **Link by ID only**, never by time: Claude Code `meta.json.toolUseId` ↔ the `Subagent`
   action's `source_event_id`; Codex sub-agent thread ID ↔ the parent's
   `SubAgentActivity(started).agent_thread_id`.
3. **IR change (new IR version):** child events carry `parent_call_id` and `agent_id`;
   `source_event_id` gets a `sub:<agentId>:` prefix. Keep only `ToolAction`, `ToolOutcome` and
   `TokenUsage` from inside the sub-agent; its prompt and final message are already in the
   parent. Depth 1 only.
4. **Format knowledge and deduplication stay in adapters:** Claude skips copies inside forked
   sessions; Codex reads `thread_source == "subagent"` threads as attachments of their parent
   (still excluding `guardian_review`), and only `item_completed` and `token_count`.
5. **Ordering:** sub-agent files keep their own cursors; events whose parent call is not stored
   yet are kept pending and linked on a later scan, idempotently (unlike usage before a first
   turn, these must not be dropped).
6. **Storage (new schema version):** `turn_items` and `token_usage` record whether a row came
   through a sub-agent, and which.
7. **Counting:** the parent's `Subagent` action still counts once in the tool profile; the
   sub-agent's own tool calls count by type; changed files, lines and tokens count directly;
   report material lists the sub-agent's changes under the parent turn's `subagent:` line.
8. **Acceptance:** golden samples (Claude: synchronous, asynchronous across turns, a forked copy;
   Codex: a sub-agent with `started` / `interacted` / `completed`, `FileChange` and
   `token_count`); scanning twice, sub-agent before parent, and after appends must store the same
   rows; on real data, each Codex sub-agent's stored tokens equal its last cumulative count and
   the parent's total grows by exactly the sub-agents' own totals.

Attaching events to existing turns changes slice content hashes, so affected reports show as
stale once; that is expected.

### Smaller items

- **Regenerate digests.** A digest that came out wrong stays cached until the model, the turns or
  the note change, so there is no direct way to redo it. Add "Regenerate digest" to the session
  drawer, and an option on a daily report's Update to redo that day's digests too. It bypasses the
  cache once and stores the new digest; the report then shows as stale and Update rewrites only
  the affected project's section.
- **Replace `chrono`.** RustSec marks it unmaintained (RUSTSEC-2026-0543, informational, no known
  vulnerability); move date handling to `jiff` or `time`. Local dates and the day boundary depend
  on it, so the slicing tests must pass unchanged.
- A ledger of Lantai's own model calls (stage, model, tokens, time, outcome) for the dashboard's
  "Lantai itself" panel. Needs a new table.
- A model per stage: a cheap, fast model for digests and a stronger one for reports.
- Per-project privacy modes: `logical_projects.privacy_mode` already exists (`default`,
  `local_only`, `never_send`) but is not enforced or shown yet. `local_only` would allow only
  local engines for the project; `never_send` would show it without ever sending it to a model.

## Later

- **OpenCode 2.x storage.** Reported by third parties, not verified here: sessions move to
  `session_v2`, one row per turn in `session_message` with content in `data.content[]`; tools are
  renamed (`bash` → `shell`, `apply_patch` → `patch`) and outputs become `state.content` blocks.
  1.x and 2.x tables can share one database, and the same session may appear in both (2.x wins),
  so the deduplication key must match across them. Install 2.x locally to produce samples first.
- **Antigravity CLI:** sessions live in `conversations/*.pb` and `*.db` under
  `~/.gemini/antigravity-cli/brain`, with no transcript; the format is unknown.
- **Signing and one-click updates:** with an Apple Developer ID, sign and notarize releases, then
  consider `tauri-plugin-updater`. Show the commit hash on the About page of development builds.
- **Documentation site:** a user guide (install, first scan, the evening review, weekly and monthly
  reports, projects and rules, prompts, models including local ones, CLI reference, data
  locations, FAQ) published under `/docs/` on the website, probably with VitePress reading `docs/`
  directly; the hand-written home page stays as it is.
- **Search:** highlight hits after jumping to a report; semantic search with embeddings; "ask
  Lantai" (answer questions from search results with a model).
- **Extra digest instructions, not editable digest prompts.** Digests are material for the
  program, not prose for people: they must keep a fixed JSON shape (points with turn numbers, an
  outcome, a project suggestion) that report generation parses and anchors are checked against,
  so a free-form edit could break reports far from where it was made. A prompt edit must also
  enter the cache key, which would invalidate every digest at once. Tone, length and focus belong
  in report prompts, which are already editable. If a need shows up in real use, offer one field
  of extra instructions (e.g. "note performance numbers") appended after the fixed format rules,
  as report prompts get their anchor rules appended. It enters the cache key but applies to new
  digests only; redoing old ones is the user's choice of a date range, after seeing roughly how
  many model calls it takes.
- **Engines:** Anthropic's native API (for prompt caching). Antigravity (`agy`) and OpenCode as
  engines once they pass the bar in SPEC.md, "Agent CLIs" (no session left behind, and nothing
  left that can act); check again when they release new versions. For Codex CLI, drop the
  remaining declarations once it offers a switch that turns every tool off
  ([openai/codex#60069](https://github.com/openai/codex/issues/60069)); reading reasoning effort
  values from `GET /models` where a service returns them.
- **Desktop:** a menu bar item (today's stats, quick note / ignore / assign); file-system watching
  instead of scan triggers.
- **Other platforms:** agent data folders and path normalization on Windows and WSL.

## Parked and dropped

- **Git commits (parked).** It was built once and set aside: collect commits, link them to slices
  by time, add them to report material. For daily reports it added little (commits made
  by agents are already in the session) while changing the schema and lengthening the material.
  It may return for commit counts in weekly reports. Lessons: `git commit` output in sessions
  rarely contains the new hash, so linking works only by time (±10 minutes); deduplicating by
  (author email, author time, message) is cheaper than patch-id and matches amend semantics.
- **Persistent workstreams (dropped).** The first weekly design kept cross-day "workstream"
  entities and attached each day's slices to them. Attaching had to run day by day in order and
  was slow; streams came out fragmented (one project: 28 streams in 8 days, a third with a single
  slice); and it needed a UI to rename, merge and move them. Weekly reports only need grouping
  within one week, which one clustering call does at generation time, in parallel and without
  state; the monthly report merges the same work across weeks.

## Open questions

- Weekly clustering quality on real data: does it keep parallel work apart, and stay at three to
  five threads per project?
- Redaction coverage and false positives.
- Default chunk budget, and the drift limits of incremental digests (`max_incremental_updates`,
  the share of new turns), compared against full digests on long real sessions.
- Accuracy of assignment suggestions folded into digest requests, and how much input the project
  list costs when there are many projects.
- Whether regex rules need a helper ("make a rule from these sessions").
- Notes are per session, so editing one invalidates every day's digest of that session. Should
  they be per slice?
- After per-project updates, do sections read inconsistently? Should an update include unchanged
  sections as a style reference?
- English prompt quality against Chinese on real data; whether languages without tuned prompts
  are good enough.
- Search defaults after real use: both layers by default, tool output off, a separate view rather
  than a Cmd+K overlay.
- Whether the large text columns of `turns` need compression (that would also force a search
  index).
- Time zone changes: slices store `local_date` computed with the system time zone at the time and
  are not re-sliced when it changes.
