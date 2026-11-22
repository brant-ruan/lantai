-- Schema version 8, kept to test the migration to version 9.
CREATE TABLE logical_projects (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    description TEXT,
    color       TEXT,
    is_archived INTEGER NOT NULL DEFAULT 0,
    privacy_mode TEXT NOT NULL DEFAULT 'default',
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);

CREATE TABLE project_match_rules (
    id         INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES logical_projects(id) ON DELETE CASCADE,
    rule_type  TEXT NOT NULL,
    target_field TEXT,
    pattern    TEXT NOT NULL,
    auto_apply INTEGER NOT NULL DEFAULT 0,
    UNIQUE (rule_type, pattern)
);

CREATE TABLE sessions (
    id                INTEGER PRIMARY KEY,
    agent             TEXT NOT NULL,
    source_session_id TEXT NOT NULL,
    parent_session_id TEXT,
    project_id        INTEGER REFERENCES logical_projects(id) ON DELETE SET NULL,
    assignment_source TEXT NOT NULL DEFAULT 'unassigned',
    suggested_project_id INTEGER REFERENCES logical_projects(id) ON DELETE SET NULL,
    suggestion_source TEXT,
    suggestion_reason TEXT,
    git_root          TEXT,
    git_remote        TEXT,
    detected_cwd      TEXT,
    git_branch        TEXT,
    agent_version     TEXT,
    title             TEXT,
    fallback_title    TEXT,
    user_note         TEXT,
    tags_json         TEXT NOT NULL DEFAULT '[]',
    ignored           INTEGER NOT NULL DEFAULT 0,
    first_active_at   TEXT,
    last_active_at    TEXT,
    UNIQUE (agent, source_session_id)
);

CREATE TABLE turns (
    id              INTEGER PRIMARY KEY,
    session_id      INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    turn_index      INTEGER NOT NULL,
    source_event_id TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    ended_at        TEXT NOT NULL,
    user_prompt     TEXT NOT NULL,
    UNIQUE (session_id, source_event_id),
    UNIQUE (session_id, turn_index)
);

-- Items inside a turn, in order: agent text (message) or tool call (tool).
CREATE TABLE turn_items (
    id              INTEGER PRIMARY KEY,
    session_id      INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    turn_id         INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    seq             INTEGER NOT NULL,
    source_event_id TEXT NOT NULL,
    timestamp       TEXT NOT NULL,
    item_type       TEXT NOT NULL,
    text            TEXT,
    tool_kind       TEXT,
    tool_name       TEXT,
    target          TEXT,
    summary         TEXT,
    is_error        INTEGER,
    added           INTEGER,
    removed         INTEGER,
    output_summary  TEXT,
    UNIQUE (session_id, source_event_id)
);
CREATE INDEX turn_items_by_turn ON turn_items (turn_id, seq);

-- Token usage of each model call, attached to the turn current when it was reported.
CREATE TABLE token_usage (
    id                 INTEGER PRIMARY KEY,
    session_id         INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    turn_id            INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    source_event_id    TEXT NOT NULL,
    timestamp          TEXT NOT NULL,
    model              TEXT,
    input_tokens       INTEGER NOT NULL,
    output_tokens      INTEGER NOT NULL,
    cache_read_tokens  INTEGER NOT NULL,
    cache_write_tokens INTEGER NOT NULL,
    UNIQUE (session_id, source_event_id)
);
CREATE INDEX token_usage_by_turn ON token_usage (turn_id);

CREATE TABLE session_daily_slices (
    id                    INTEGER PRIMARY KEY,
    session_id            INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    local_date            TEXT NOT NULL,
    start_turn_index      INTEGER NOT NULL,
    end_turn_index        INTEGER NOT NULL,
    slice_start_at        TEXT NOT NULL,
    slice_end_at          TEXT NOT NULL,
    active_minutes        INTEGER NOT NULL,
    active_intervals_json TEXT NOT NULL,
    files_modified_json   TEXT NOT NULL,
    ignore_state          TEXT NOT NULL DEFAULT 'inherit',
    project_id_override   INTEGER REFERENCES logical_projects(id) ON DELETE SET NULL,
    digest_json           TEXT,
    digest_input_hash     TEXT,
    -- Last turn the digest covers, and how many incremental updates it has had since a full digest.
    digest_covered_turn   INTEGER,
    digest_update_count   INTEGER NOT NULL DEFAULT 0,
    UNIQUE (session_id, local_date)
);
CREATE INDEX slices_by_date ON session_daily_slices (local_date);

-- Digests of the parts of a slice that was too long to summarize in one request.
CREATE TABLE digest_chunks (
    id               INTEGER PRIMARY KEY,
    slice_id         INTEGER NOT NULL REFERENCES session_daily_slices(id) ON DELETE CASCADE,
    start_turn_index INTEGER NOT NULL,
    end_turn_index   INTEGER NOT NULL,
    input_hash       TEXT NOT NULL,
    digest_json      TEXT NOT NULL,
    UNIQUE (slice_id, start_turn_index)
);

-- A project's daily material condensed into task clusters (JSON) before the report is written.
CREATE TABLE project_briefs (
    id            INTEGER PRIMARY KEY,
    local_date    TEXT NOT NULL,
    project       TEXT NOT NULL,
    input_hash    TEXT NOT NULL,
    clusters_json TEXT NOT NULL,
    UNIQUE (local_date, project)
);

CREATE TABLE source_files (
    id                INTEGER PRIMARY KEY,
    agent             TEXT NOT NULL,
    source_session_id TEXT NOT NULL,
    current_path      TEXT NOT NULL,
    size              INTEGER NOT NULL,
    mtime             INTEGER NOT NULL,
    byte_offset       INTEGER NOT NULL,
    head_hash         TEXT NOT NULL,
    error_count       INTEGER NOT NULL DEFAULT 0,
    unknown_count     INTEGER NOT NULL DEFAULT 0,
    last_error        TEXT,
    UNIQUE (agent, source_session_id)
);

CREATE TABLE reports (
    id                 INTEGER PRIMARY KEY,
    report_type        TEXT NOT NULL,
    period_key         TEXT NOT NULL,
    project_id         INTEGER REFERENCES logical_projects(id) ON DELETE CASCADE,
    status             TEXT NOT NULL DEFAULT 'draft',
    current_version_id INTEGER
);
CREATE UNIQUE INDEX reports_key ON reports (report_type, period_key, COALESCE(project_id, 0));

-- Report prompts. Built-in rows mirror the texts shipped with the program; the rest belong to the user.
CREATE TABLE prompts (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,
    name       TEXT NOT NULL COLLATE NOCASE,
    body       TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0,
    is_builtin INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (kind, name)
);
CREATE UNIQUE INDEX prompts_one_default ON prompts (kind) WHERE is_default = 1;

CREATE TABLE report_versions (
    id               INTEGER PRIMARY KEY,
    report_id        INTEGER NOT NULL REFERENCES reports(id) ON DELETE CASCADE,
    origin           TEXT NOT NULL,
    markdown_content TEXT NOT NULL,
    prompt_id        INTEGER REFERENCES prompts(id) ON DELETE SET NULL,
    prompt_name      TEXT,
    prompt_snapshot  TEXT,
    engine           TEXT,
    input_hash       TEXT,
    -- Per-project record of the inputs, for staleness checks and incremental updates.
    input_manifest_json TEXT,
    created_at       TEXT NOT NULL
);
