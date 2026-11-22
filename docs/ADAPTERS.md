# Adapters

How each coding agent stores its sessions, and how Lantai maps them to the IR. The interface,
cursors and incremental scanning are described in [SPEC.md](SPEC.md#scanning-and-adapters); this
file holds the format knowledge, which by design lives only in the adapter crates.

The facts below were measured on macOS against the agent versions named in each section. Agents
change their formats without notice; the golden tests are meant to notice first.

## Adding an adapter

1. **Survey the format first.** Generate real sessions that cover the cases that matter: plain
   chat, file reads, writes and edits, shell commands (including failures), web search, images,
   sub-agents, interruption, resume, forking, context compaction. Note where the agent keeps
   credentials or other private files next to its sessions; the adapter must read only the
   session files.
2. **Add the agent kind** to `aj_ir::AgentKind`. Its `as_str` is the stable key stored in the
   database and used in `config.toml` (`[agents.<key>]`); it never changes once released.
3. **Create `crates/aj-adapter-<name>`** implementing `aj_adapter::SourceAdapter`:
   `discover` lists one `SourceFile` per session, `parse` turns records from an offset into IR
   events, `normalize_cwd` optionally maps agent-made worktrees back to their repository, and
   `probe` reports which agent versions wrote the local data and whether they can be read.
4. **Register it in `aj-sources`**: default session roots, an `AJ_<NAME>_ROOT` override, and one
   arm that constructs the adapter. Nothing above `aj-sources` names a concrete adapter.
5. **Add fixtures and golden tests.** A redacted sample under `fixtures/<name>/` and its expected
   events (`*.events.jsonl`); `UPDATE_GOLDEN=1` rewrites the expectation, which must be reviewed
   by hand. Never commit real session content.
6. **Document it here**, add the read-only rule to `AGENTS.md`, and mention it in the README.

Rules every adapter follows:

- **Read only.** Never write, move or delete anything in the agent's folders. If opening a file
  would make a library create side files (SQLite does), copy it into Lantai's data directory
  first (see [OpenCode](#opencode)).
- **Tolerate everything.** Unknown record types are counted and skipped; a line that fails to
  parse is counted and recorded as the last error; a file is never abandoned because of one bad
  line.
- **Allowlist, and skip early.** Handle only known record types. Where possible, read the type
  from the start of the line without deserializing it; some lines are megabytes of tool output or
  images.
- **Stable event IDs.** Every event carries a `source_event_id` that is the same on every parse,
  so reparsing a whole file is idempotent.
- **Advance only to the last newline.** The last line may still be being written.
- **Do not claim support you have not verified.** If an agent is known to have incompatible
  major versions, implement `probe` so the settings page and `aj doctor` can say "partly
  supported" or "version not supported" instead of silently reading nothing.

Common mappings used by all adapters:

- A **turn** starts at real human input only. Injected context (system reminders, environment
  blocks, skill catalogs, task notifications, heartbeats) is stripped or skipped. Images and
  attached files become `[image xN]` / `[file: name]`.
- **Tools** map to `ToolKind`: `bash_cmd`, `file_read`, `file_write`, `file_edit`, `web_search`,
  `web_fetch`, `mcp`, `subagent`, `other`. Search tools (grep, glob) are `other` with the pattern
  as the target. Writes and edits carry line counts.
- **Token usage** is recorded once per model call as `input` (excluding cache), `cache_read`,
  `cache_write` and `output` (including reasoning). Sources with OpenAI semantics, where input
  includes cached tokens, are converted.
- **Sessions in agent-made scratch folders** (desktop apps that run without a chosen folder) are
  ordinary sessions, not ignored by default; they get project suggestions from the model since
  folder rules cannot match them.
- **Sub-agent sessions** produce no events of their own today; the parent's `subagent` tool call
  stands for them. See [Sub-agents](#sub-agents) for the formats and
  [ROADMAP.md](ROADMAP.md#sub-agent-activity-in-the-parent-session) for the plan.

## Claude Code

Measured on Claude Code 2.1.x.

| Item | Format |
| :--- | :--- |
| Sessions | `~/.claude/projects/<encoded-path>/<sessionId>.jsonl` |
| Sub-agents | `<sessionId>/subagents/agent-*.jsonl` next to the session; not read yet |
| Records used | `user`, `assistant`, some `system`, `custom-title` |
| Records skipped | `api-request*`, `file-history-*`, `queue-operation`, `attachment`, `cost-state` and others; together they are more than half the lines and most of the bytes |
| Session metadata | every message record has `uuid`, `timestamp` (UTC), `cwd`, `gitBranch`, `sessionId`, `version` |
| Turn start | `type == "user"`, not `isMeta`, not `isSidechain`, content is not a `tool_result` |
| Tools | `tool_use` blocks in `assistant` records, paired with later `tool_result` blocks by `tool_use_id` |
| Title | `custom-title` if present, else the first real input |
| Token usage | `message.usage` on `assistant` records; one `TokenUsage` per `message.id` (`usage:<message.id>`); skip `model == "<synthetic>"` and all-zero usage |
| Size | single files reach tens of megabytes; read line by line |

Details:

- **User records come in three kinds:** human input (`content` is a string, or an array of
  `text` / `image` blocks), tool results (`tool_result` blocks) and metadata (`isMeta: true`).
  Only the first starts a turn. Human input has `origin.kind == "human"`; task notifications are
  `task-notification` and messages from other sessions are `peer`. Old records have no `origin`
  and are recognized by content prefixes.
- **One content block per assistant record** (`thinking`, `text` or `tool_use`); the records of
  one reply share `message.id`, and every record of a `message.id` repeats the same usage, so
  usage is counted once per ID.
- **Skipping without parsing:** lines starting with `{"type":"` are side records (only
  `custom-title` is useful; the known ones are skipped, unseen ones are counted as unknown);
  message records start with `{"parentUuid":`.
- **Forked sessions are common** (about two thirds of files in one sample). A forked file starts
  with history copied from the original session, and those records keep the original
  `sessionId` and `uuid`. The adapter skips records whose `sessionId` differs from the file name
  and records `parent_session_id`. If the original file was cleaned up before Lantai saw it, that
  history is lost.
- **Lantai keeps its own copy.** Claude Code deletes old sessions after `cleanupPeriodDays`, so
  the full dehydrated content is stored at scan time.
- **Scratch folders:** desktop conversations without a folder run in
  `~/Library/Application Support/Claude/.../scratch-*`.
- **Self-invocation:** reports generated through `claude -p` must not leave sessions behind; see
  [SPEC.md](SPEC.md#engines). `--allowedTools ""` does not disable tools; `--tools ""` does.

## Codex CLI

Measured on Codex Desktop and CLI (rollout `cli_version` 0.94 to 0.169; Codex migrates old files
to the current format itself).

| Item | Format |
| :--- | :--- |
| Sessions | `~/.codex/sessions/YYYY/MM/DD/rollout-<time>-<threadId>.jsonl` and `~/.codex/archived_sessions/rollout-*.jsonl`; both also hold `.jsonl.zst` files (most files, in practice) |
| Session ID | the ID at the end of the file name. A name like `<threadId>_<segmentId>` is a continuation segment of the same thread; it is stored as its own session with `parent_session_id` set to the thread |
| Line format | `{"timestamp","ordinal","type","payload"}`, timestamps in UTC |
| Session metadata | first line `session_meta`: `cwd`, `git.branch`, `cli_version`, `forked_from_id`, `thread_source`, `source` |
| Content | **only `event_msg` lines of type `item_completed`**, whose `item` is an already structured thread item. `response_item` (raw model I/O including injected developer context), `token_usage_record`, `world_state`, `compacted` and the rest are skipped. `turn_context.model` and `event_msg` `token_count` are read for usage only |
| Turn start | `UserMessage` items. `<heartbeat>` automation pings are not human input; `<in-app-browser-context>` blocks are stripped; `<send_user_message_question_reply>` keeps its content without the tag |
| Output and tools | `AgentMessage` → agent message; `CommandExecution` → `bash_cmd` (failed or non-zero exit is an error, output tail kept); `FileChange` → one `file_edit` / `file_write` per file with line counts from `unified_diff` or content; `McpToolCall` → `mcp`; `WebSearch` and the `web.search` extension → `web_search`; `ImageView` → `file_read`; `SubAgentActivity` (`started`) → `subagent`; `Reasoning`, `ContextCompaction`, `CollabAgentToolCall`, `clock.sleep` skipped |
| Token usage | `token_count.info.last_token_usage` is the call's usage (it equals the difference of consecutive `total_token_usage`); snapshots repeat, so the event ID is `usage:<cumulative total_tokens>`; empty `info` is skipped. `input = input_tokens - cached_input_tokens - cache_write_input_tokens`, `cache_read = cached_input_tokens`, `output = output_tokens` (already includes reasoning). The model is the latest `turn_context.model` |
| Excluded sessions | `thread_source` of `guardian_review` (automatic approval review) or `subagent`, or any `source.subagent`: no events |

Details:

- **zstd files cannot be resumed at a byte offset.** Every change decompresses and reparses the
  whole file; the cursor offset is the compressed size, and event IDs make it idempotent. A move
  from `sessions/` to `archived_sessions/` changes path and head hash and is handled as a
  rewrite.
- **One session, several files** (a `.jsonl` and `.jsonl.zst` side by side while Codex
  compresses): discovery keeps only the newest by modification time, so the cursor does not flip
  between them.
- **Prefiltering:** each line starts with `{"timestamp":…,"ordinal":…,"type":"…"`; the record
  type and `payload.type` are read from that window and anything other than `item_completed`
  (and the usage records) is not deserialized.
- **Resuming an uncompressed file** still reads its first line to apply the exclusion rules. A
  file that yields no events only gets a cursor; no empty session is created.
- **Forks and segments:** cumulative counters include the parent thread, but per-call deltas are
  stored, so a fork counts only the calls in its own file. Stored totals match each file's final
  cumulative count.
- Codex's own worktrees (`~/.codex/worktrees/<id>/<repo>`) are not mapped back to the main
  repository yet; path and remote rules handle them like any folder.

## Antigravity

Measured on AGY 2.0 and the Antigravity IDE.

| Item | Format |
| :--- | :--- |
| Roots | `~/.gemini/antigravity/brain` and `~/.gemini/antigravity-ide/brain` (`AJ_ANTIGRAVITY_ROOT` replaces both) |
| Sessions | `<convId>/.system_generated/logs/transcript_full.jsonl`, falling back to `transcript.jsonl` in the same folder |
| Full vs short | the full transcript has untruncated fields and native JSON arguments; the short one double-encodes `tool_calls[].args` values as JSON strings and truncates long text with `\n<truncated N bytes>` |
| Line format | `{"step_index":N,"source":"…","type":"…","status":"…","created_at":"…"}`, UTC |
| Turn start | `type == "USER_INPUT"` and `source == "USER_EXPLICIT"`: the text inside `<USER_REQUEST>…</USER_REQUEST>` (else strip `<ADDITIONAL_METADATA>` / `<USER_SETTINGS_CHANGE>`); `media` counts as images |
| Output and tools | `PLANNER_RESPONSE` `content` → agent message; `tool_calls[]` → tool actions by meaning (thinking and internal scheduling tools ignored) |
| Tool results | paired by position (FIFO) with the latest `PLANNER_RESPONSE`'s calls. AGY 2.0 uses `GENERIC` steps (exit code in the text `Command exited with code N`); the IDE uses typed steps (`VIEW_FILE`, `CODE_ACTION`, `GREP_SEARCH`, `RUN_COMMAND` with an integer `exit_code`, `LIST_DIRECTORY`…). `status == "RUNNING"` is a background run and yields no outcome; `ERROR` / `INVALID`, a non-empty `error` or a non-zero exit is a failure |
| Token usage | every `PLANNER_RESPONSE` with `input_tokens` / `output_tokens` (`step:<idx>:usage`), whether or not it has visible content (most planning steps have none); `input_tokens` excludes cache; `model` appears occasionally and carries forward |
| Skipped | `SYSTEM_MESSAGE`, `CHECKPOINT`, `ERROR_MESSAGE`, `CONVERSATION_HISTORY`, `KNOWLEDGE_ARTIFACTS` |

Details:

- **Artifacts are not code changes.** Writes that carry `ArtifactMetadata` or target the agent's
  own `brain/` folders become `other` (target reduced to the file name, summary from
  `ArtifactMetadata.Summary`); they do not count as changed files or feed folder inference.
- **No explicit working directory.** It is inferred in order from the most frequent
  `run_command.Cwd`, then read/write tool paths (excluding `brain/`, `/tmp`, `/Library/` and other
  system or internal folders), then `Active Document` in `ADDITIONAL_METADATA`: the deepest
  common ancestor covering 80% of paths, rejecting shallow ones such as `/Users/<name>`.
- **Two roots:** conversations are deduplicated by `<convId>`, keeping the newest file and
  preferring the full transcript on equal times.
- **Prefiltering and resuming:** only the first 512 bytes of lines starting with
  `{"step_index":` are inspected for `type` and `status`; uninteresting `DONE` results are not
  deserialized. Resuming at an offset first rescans earlier lines to rebuild the queue of tool
  calls still waiting for results.
- **Antigravity CLI** keeps sessions as `conversations/*.pb` and `*.db` under
  `~/.gemini/antigravity-cli/brain`, without a transcript; it is not supported.

## DeepSeek Harness

Measured on the DeepSeek Harness desktop app, session format version 4.

| Item | Format |
| :--- | :--- |
| Root | `~/.dsh/sessions` (`AJ_DSH_ROOT`). `~/.dsh` also holds credential files such as `.credentials.yaml`; only `sessions/` is read |
| Sessions | `<encoded cwd>/<sessionId>/session.v<N>.jsonl.zstd` next to an empty `session.lock`. The session ID is the folder name (`session-<uuid>` for main sessions, a bare `<uuid>` for sub-agents) and equals the first line's `id`; with several versions in one folder, the highest N wins |
| Compression | zstd frames are appended while the session runs; the last frame of a running session is often incomplete, so lines read before a decompression error at the end are kept and no error is counted |
| First line | `{"type":"session","version":4,"id","createdAt","cwd","isSeeded","delegationDepth","agentPreset","parentSession"?,"origin"?}`; other versions are not parsed and one error is recorded |
| Line format | `{"type":"…","seq":N,"time":<ms>,"data":{…}}`, `type` always first; `time` is UTC |
| Turn start | `user/message` with `data.source.kind == "user"`; text blocks joined, `image` blocks counted. Injected `agent-instructions`, `runtime-context`, `skill-catalog` and `time-context` messages are skipped; `agent/inbox/spliced` is the queued copy of a user message and is skipped |
| Output | `text` blocks of `assistant/message` → agent message; `reasoning` skipped; model in `message.source.model` |
| Token usage | `data.usage` of each `assistant/message` (`usage:<seq>`); `inputTokens` excludes cache; the four counts map directly |
| Tools | `tool/call` (`callId`, `name`, JSON-string `arguments`): `bash` → `bash_cmd`; `read`, `read_image` → `file_read`; `grep` → `other`; `skill` → `other`; `write` → `file_write`; `edit` → `file_edit` (lines from `old_string` / `new_string`); `web_search`; `web_fetch`; `subagent` → `subagent`; everything else (`present`, `job_*`, `ask_user_question`…) → `other` |
| Tool results | `tool/result` paired by `message.toolCallId`; `message.isError` marks failure |
| Title | `session/title`, preferring source `user` over `provider` over `fallback`, last one within a level |
| Metadata | `cwd` from the first line; `parentSession` → `parent_session_id`; no Git branch or agent version |
| Skipped | `step/*`, `turn/*`, `job/*`, `request/*`, `system/message`, `web/deepseek-search-llm-request` (large), `session-log-deepseek/*`, `session/title-llm-request`, `session/end-seed`, `sandbox/mode`, `approval/policy`, `permission/preset`, `model/selection`, `workspace/changes`, `deliverables/presented`, `subagent/*`, `team/mode` |

Details:

- **Forks:** a session with `isSeeded == true` starts with the parent's history, up to the first
  `session/end-seed` with `data.inherited == true` (those records predate `createdAt`). The
  adapter skips them and records `parent_session_id`. An `end-seed` without `inherited` only marks
  a pause.
- **Sub-agent sessions** (`origin == "subagent"` or `delegationDepth > 0`) produce no events.
- **Whole-file reparse** on every change, shared zstd reading with Codex
  (`aj_adapter::read_zstd_lines`).
- **Default workspace:** sessions without a project land in
  `~/Documents/deepseek-harness/default-workspace` (the folder encoding turns both `/` and `-`
  into `-`); treated like any scratch folder.

## WorkBuddy

WorkBuddy is Tencent's desktop agent built on the CodeBuddy runtime. Measured on WorkBuddy 5.11,
with samples covering writes, edits, search, commands, sub-agents, web access, to-dos, images,
interruption, resume, compaction, regeneration and edit-and-resend.

| Item | Format |
| :--- | :--- |
| Root | `~/.workbuddy/projects` (`AJ_WORKBUDDY_ROOT`). `~/.workbuddy` also holds an account key (`keyblob`), a device ID, memories and settings; only `projects/` is read |
| Sessions | `<encoded cwd>/<sessionId>.jsonl`, append-only, resumed at byte offsets. `<sessionId>.file-rollback.ndjson` and `<sessionId>/tool-results/` (copies of large outputs that are already inline) are not read |
| Sub-agents | `<sessionId>/subagents/agent-<uuid>.jsonl`; not read. The parent's usage does not include them |
| Line format | one object per line, `type` not necessarily first; `timestamp` in ms (UTC); `id` and `parentId` form a tree; every record except `session-meta` and `resend-fork-notice` has `cwd`; `providerData` carries `conversationRequestId`, `stepSeq`, `model` |
| Turn start | `type == "message"`, `role == "user"`, `providerData.startsNewUserRequest == true`. The body starts with large injected context (`<system-reminder>`, `<user_info>`, `<project_context>`, `<current_time>`, `<panel_tabs>`…); **the real input is only the text inside `<user_query>…</user_query>`** (fallback: everything after stripping `<system-reminder>`); `image_blob_ref` blocks counted |
| Output | `output_text` blocks of assistant messages; `reasoning` records (and their copy in `function_call.providerData.reasoning`) skipped |
| Token usage | exactly one record per model step has `providerData.usage` (`usage:<record id>`, model from `providerData.model`). OpenAI semantics: `input = inputTokens - cached_tokens`, `cache_read = inputTokensDetails[0].cached_tokens`, `cache_write = rawUsage.cache_creation_input_tokens`, `output = outputTokens` |
| Tools | `function_call` (`callId`, `name`, JSON-string `arguments`): `Bash` → `bash_cmd`; `Read`, `ViewImage` → `file_read`; `Write` → `file_write`; `Edit`, `MultiEdit` → `file_edit`; `Grep`, `Glob` → `other`; `WebSearch`; `WebFetch`; `Agent` → `subagent`; the rest (to-do tools, `present_files`, connector tools without a common prefix) → `other` |
| Tool results | `function_call_result` by `callId`. `status` stays `completed` even on failure; failure is `providerData.toolResult.rawResponse.is_error == true` or a non-zero `exitCode`. `output` may be text or an array of `input_text` blocks |
| Title | `aiTitle` of `ai-title` records. Renaming in the sidebar writes only to WorkBuddy's own database, which is not read |
| Metadata | `cwd` from records (updated when it changes); no Git branch or agent version |
| Skipped | `session-meta`, `reasoning`, `file-history-snapshot`, `resend-fork-notice` |

Details:

- **Compaction:** `/compact` writes a user message with `providerData.isCompacted == true` and no
  `startsNewUserRequest`, holding a `<cb_summary>` that embeds every earlier `<user_query>`. It
  must be skipped or it creates a run of fake turns.
- **Interruption** leaves `incomplete` reasoning and an assistant message
  (`providerData.isPartialAborted`; the partial text is kept), then a short assistant message with
  `providerData.skipRun` and `providerData.error`, which is skipped. The interrupted step has no
  usage record.
- **Regenerate and edit-and-resend** are the same mechanism: a `resend-fork-notice`
  (`editedUserItemId` names the original input), then a new user message with the same
  `parentId`. The old branch stays in the file. Its tools did run and its tokens were spent, so
  both branches are kept as turns.
- **Resume** appends to the same file.
- **Scratch folders:** without a chosen folder, each conversation gets a new
  `~/WorkBuddy/<YYYY-MM-DD-HH-MM-SS>` folder; most WorkBuddy conversations live there.

## OpenCode

Only OpenCode 1.x is read. Measured on OpenCode 1.18.x (session `version` 1.18.18 to 1.18.35,
plus development builds reporting `local`).

**Unlike the other sources, OpenCode keeps every session in one SQLite database.** That changes
how files are opened, discovered and resumed.

### Read only: copy, then open

The database is `~/.local/share/opencode/opencode.db` (honouring `XDG_DATA_HOME`;
`AJ_OPENCODE_ROOT` points at the folder that contains it), in WAL mode, next to `opencode.db-wal`
and `opencode.db-shm`. The same folder holds `auth.json` (provider credentials), `snapshot/`,
`repos/`, `tool-output/` and `log/`. The adapter reads only `opencode.db` and `opencode.db-wal`.

**The source database must never be opened, not even read-only:**

| How it is opened | Effect on the source folder | Sees recent writes |
| :--- | :--- | :--- |
| `mode=ro` | creates `-wal` / `-shm` for a cleanly closed database; touches an existing `-shm` | yes |
| `immutable=1` | writes nothing | **no**: the WAL is ignored, so uncheckpointed commits are invisible |
| copy `db` and `db-wal` elsewhere, then open | none | yes, SQLite recovers from the copied WAL |

So each scan takes a snapshot:

- Copy `opencode.db` and `opencode.db-wal` (if present) to `cache/opencode/` in Lantai's data
  directory, without `-shm` (rebuilt from the WAL), and open the copy read-only.
- **Consistency:** compare size and modification time of both files before and after copying; if
  they changed, OpenCode was writing or checkpointing, so retry (up to three times), then skip
  OpenCode for this scan with one recorded error. A half-copied trailing WAL frame fails its
  checksum and is discarded by SQLite.
- **Reuse:** if neither source file changed since the last copy, reuse it.
- The snapshot folder is passed in by `aj-sources`; the adapter does not know where the data
  directory is.

Do not inspect the source with the `sqlite3` shell either; use the copy in `cache/opencode/`.

### Discovery and cursors

- **One `SourceFile` per session**, all with the database's path; `source_session_id` is the
  `ses_…` ID; `mtime` is the latest `time_updated` of the session, its messages and parts.
- **No byte offsets.** The database header changes on every write, so file-based change
  detection would mark every session as rewritten. The adapter sets `SourceFile.revision` instead:
  `<session time_updated>:<message count>:<part count>:<max message time_updated>:<max part
  time_updated>`. Parts are updated in place (streamed text, tools moving from `running` to
  `completed`), so counting rows is not enough. When the revision changes, the core reparses the
  whole session; the revision is stored in the cursor's `head_hash` column.
- **Only finished content.** Deduplication keeps the first stored version, so partial content
  would never be replaced: assistant messages without `time.completed` are skipped entirely, and
  `pending` / `running` tool parts produce nothing. They are picked up when the revision changes.

### Format and mapping

| Item | Format |
| :--- | :--- |
| Tables | `session`, `message`, `part`, `project`; `message.data` and `part.data` are JSON text. `session_message`, `session_input` (2.x tables, empty in 1.x), `event`, `todo`, `workspace`, `permission`, `account*`, `credential` are not read |
| IDs and times | `ses_` / `msg_` / `prt_` plus 26 time-ordered characters; times in ms (UTC) |
| Metadata | `session.directory` is the working directory (often a subfolder of `project.worktree`, or a non-Git folder under the `global` project); `session.version` is the agent version; `session.parent_id` is the parent session; no Git branch |
| Title | `session.title`, except OpenCode's placeholder (starting with `New session`), which falls back to the first input |
| Turn start | `role == "user"` messages: non-`synthetic` `text` parts joined (synthetic ones are expansions such as commands); `file` parts become `[image xN]` or `[file: name]` by `mime`. `summary.diffs` is not read; changes come from tool calls |
| Output | `text` parts of assistant messages (not `synthetic`); `reasoning`, `step-start`, `step-finish`, `snapshot`, `patch` (a file snapshot, not the patch tool) skipped |
| Token usage | one per assistant message (`usage:<message id>`) from `message.data.tokens` (identical to `step-finish.tokens`). `input` excludes cache: `input_tokens = input`, `cache_read = cache.read`, `cache_write = cache.write`, `output_tokens = output + reasoning`; model from `modelID`; all-zero usage (aborted messages) skipped. Per-message sums match the session's `tokens_*` totals |
| Errors | an assistant message's `error` (`APIError`, `MessageAbortedError`, `UnknownError`) produces no event; partial output is kept |
| Tools | `type == "tool"` parts: `tool` is the name, `callID` the call ID, `state.input` the arguments. `bash` → `bash_cmd`; `read` → `file_read`; `write` → `file_write`; `edit` → `file_edit` (lines from `oldString` / `newString`); `patch` / `apply_patch` → one `file_edit` / `file_write` per file parsed from the patch; `glob`, `grep`, `list` → `other`; `webfetch` → `web_fetch`; `websearch` → `web_search`; `task` → `subagent`; `skill` → `other`; `question`, `todowrite`, `todoread`, `invalid` and MCP tools (named `<server>_<tool>`, not reliably distinguishable) → `other` |
| Tool results | in the same part: `state.status == "completed"` with `state.output` and, for bash, the exit code in `state.metadata.exit`; `"error"` with `state.error`. A `ToolAction` is followed immediately by its `ToolOutcome` |

Details:

- **Sub-agent sessions:** `task` creates a child session whose `parent_id` is the parent
  (`state.metadata.sessionId` names it); the child produces no events.
- **Compaction:** `/compact` should write an assistant message with `summary == true` and a
  `compaction` part; both are skipped. The field names have not been confirmed with a real
  sample yet.
- **Revert:** `session.revert` marks an undo point; OpenCode deletes the reverted messages only
  on the next prompt. Lantai never deletes stored content, so messages still present are read.
- **Versions and `probe`:** `probe` lists every `session.version` found. Any version not starting
  with `1.` (other than `local`) makes the result "partly supported" (unverified version), and
  parsing is still attempted. If a `session_v2` table exists, OpenCode 2.x has been installed:
  sessions only in `session_v2` make it "partly supported"; a database without 1.x tables makes
  `discover` return nothing and `probe` report "not supported", without errors.
- The fixture `fixtures/opencode/basic.sql` is hand-written to the 1.18.35 schema.

## Sub-agents

Formats surveyed for merging sub-agent activity into the parent session (planned, see the
[roadmap](ROADMAP.md#sub-agent-activity-in-the-parent-session)). DeepSeek Harness and WorkBuddy
sub-agent files are listed in their sections above.

### Claude Code

| Item | Finding |
| :--- | :--- |
| Files | under `<sessionId>/subagents/`, three per sub-agent: `agent-<agentId>.jsonl` (conversation), `agent-<agentId>.meta.json` (metadata), `agent-<agentId>.prefix.json` (prompt-cache diagnostics, irrelevant) |
| Records | same format as main sessions; each has `isSidechain: true` and `agentId`, and `sessionId` is the parent's; usage is on the sub-agent's own `assistant` records, with `message.id`s that never overlap the parent's |
| Metadata | `meta.json`: `agentType`, `description`, `toolUseId`, `spawnDepth`, `finished` |
| Link to parent | `meta.json.toolUseId` equals the `tool_use.id` of the parent's `Agent` call; sidechain records never appear in the parent file |
| Synchronous | the parent's `tool_result` has `toolUseResult.status = completed` with `handback` and the result; the parent waits |
| Asynchronous | `toolUseResult.status = async_launched` returns at once; the result comes back as a `task-notification`, possibly turns later |
| Forked copies | a forked session copies the original's `subagents/` files; copies keep the original `sessionId` (not matching their folder) and have no `meta.json` |
| Usage | the parent's `toolUseResult` has no sub-agent usage, so reading sub-agent files cannot double count |

### Codex CLI

| Item | Finding |
| :--- | :--- |
| Files | each sub-agent is its own rollout file |
| Recognition | first line `session_meta`: `thread_source = "subagent"`; `source.subagent.thread_spawn` has `parent_thread_id`, `depth`, `agent_path`, `agent_nickname`; `forked_from_id` and `session_id` equal the parent thread ID; `subagent_history_start_ordinal` marks where its own history starts |
| Look-alike | automatic approval review (`guardian_review`) also has `source.subagent`; tell them apart by `thread_source` (guardian threads far outnumber real sub-agents) |
| In the parent | `SubAgentActivity` items (`kind`, `agent_thread_id`): exactly one `started` per sub-agent, plus `interacted` (messages between agents) and `completed` |
| Own content | its own `item_completed` items, including `FileChange`, none of which are counted today; it also emits `SubAgentActivity interacted` for messages it sends, which are not new sub-agents |
| Copied parent history | only in `response_item` (never read); the sub-agent's `item_completed` items all postdate its start and share no IDs with the parent |
| Usage | cumulative `token_count` starts at zero and excludes the parent |

### Double-counting risks

| Risk | Present | Handling |
| :--- | :--- | :--- |
| A Claude forked copy reads the same sub-agent twice | yes | read only sub-agent files whose `sessionId` matches their session folder; no `meta.json` means a copy |
| The parent's `Subagent` action and the sub-agent's edits both count changes | no (`Subagent` has no diff stats) | keep `Subagent` without changes |
| The Codex parent restates a sub-agent's `FileChange` | no (no shared IDs) | add a reconciliation check |
| Copied parent history counted as sub-agent activity | no (only in `response_item`) | keep reading only `item_completed` |
| Tokens counted twice | no | add a reconciliation check |
| Messages between agents counted as sub-agents or turns | must be handled | ignore Codex `interacted`, `completed` and cross-agent messages; Claude `task-notification` already does not start a turn |
| Active time counted twice | covered | interval union |
| One file counted twice | covered | canonical path deduplication |
