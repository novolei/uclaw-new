pub const V1_INITIAL: &str = "
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS spaces (
    id        TEXT PRIMARY KEY,
    name      TEXT NOT NULL,
    icon      TEXT NOT NULL DEFAULT '📁',
    path      TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS conversations (
    id         TEXT PRIMARY KEY,
    space_id   TEXT NOT NULL,
    title      TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (space_id) REFERENCES spaces(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS messages (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role            TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS users (
    id          TEXT PRIMARY KEY,
    device_name TEXT NOT NULL DEFAULT 'unknown',
    device_id   TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    last_accessed TEXT
);

CREATE TABLE IF NOT EXISTS api_tokens (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL,
    token_hash  TEXT NOT NULL UNIQUE,
    label       TEXT NOT NULL DEFAULT 'API Token',
    expires_at  TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_conversations_space ON conversations(space_id);
CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id);
CREATE INDEX IF NOT EXISTS idx_users_device_id ON users(device_id);
CREATE INDEX IF NOT EXISTS idx_api_tokens_user ON api_tokens(user_id);
";

pub const V2_ARTIFACT_CACHE_AND_STARS: &str = "
ALTER TABLE conversations ADD COLUMN starred INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS artifact_cache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    space_id TEXT NOT NULL,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    is_dir INTEGER NOT NULL DEFAULT 0,
    parent_path TEXT NOT NULL DEFAULT '',
    size_bytes INTEGER,
    mime_type TEXT,
    modified_at TEXT,
    cached_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(space_id, path)
);

CREATE INDEX IF NOT EXISTS idx_artifact_cache_space ON artifact_cache(space_id);
CREATE INDEX IF NOT EXISTS idx_artifact_cache_parent ON artifact_cache(space_id, parent_path);
";

pub const V3_MEMORIES: &str = "
CREATE TABLE IF NOT EXISTS memories (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL DEFAULT 'global',
    namespace   TEXT NOT NULL DEFAULT 'default',
    key         TEXT NOT NULL,
    value       TEXT NOT NULL,
    kind        TEXT NOT NULL DEFAULT 'note',
    tags        TEXT NOT NULL DEFAULT '',
    metadata_json TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at  TEXT,
    UNIQUE(space_id, namespace, key)
);

CREATE INDEX IF NOT EXISTS idx_memories_space ON memories(space_id);
CREATE INDEX IF NOT EXISTS idx_memories_ns ON memories(space_id, namespace);
CREATE INDEX IF NOT EXISTS idx_memories_kind ON memories(kind);
CREATE INDEX IF NOT EXISTS idx_memories_expires ON memories(expires_at);
CREATE INDEX IF NOT EXISTS idx_memories_key ON memories(key);
";

pub const V4_MEMORY_GRAPH: &str = "
CREATE TABLE IF NOT EXISTS memory_nodes (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL,
    kind        TEXT NOT NULL DEFAULT 'reference',
    title       TEXT NOT NULL,
    metadata_json TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS memory_versions (
    id                    TEXT PRIMARY KEY,
    node_id               TEXT NOT NULL,
    supersedes_version_id TEXT,
    status                TEXT NOT NULL DEFAULT 'active',
    content               TEXT NOT NULL,
    metadata_json         TEXT,
    embedding_json        TEXT,
    created_at            TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (node_id) REFERENCES memory_nodes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS memory_edges (
    id              TEXT PRIMARY KEY,
    space_id        TEXT NOT NULL,
    parent_node_id  TEXT,
    child_node_id   TEXT NOT NULL,
    relation_kind   TEXT NOT NULL DEFAULT 'relates_to',
    visibility      TEXT NOT NULL DEFAULT 'private',
    priority        INTEGER NOT NULL DEFAULT 0,
    trigger_text    TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (parent_node_id) REFERENCES memory_nodes(id) ON DELETE SET NULL,
    FOREIGN KEY (child_node_id)  REFERENCES memory_nodes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS memory_routes (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL,
    edge_id     TEXT,
    node_id     TEXT NOT NULL,
    domain      TEXT NOT NULL,
    path        TEXT NOT NULL,
    is_primary  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (edge_id) REFERENCES memory_edges(id) ON DELETE SET NULL,
    FOREIGN KEY (node_id) REFERENCES memory_nodes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS memory_keywords (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL,
    node_id     TEXT NOT NULL,
    keyword     TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (node_id) REFERENCES memory_nodes(id) ON DELETE CASCADE
);

-- Indexes for memory_nodes
CREATE INDEX IF NOT EXISTS idx_memory_nodes_space ON memory_nodes(space_id);
CREATE INDEX IF NOT EXISTS idx_memory_nodes_kind ON memory_nodes(space_id, kind);
CREATE INDEX IF NOT EXISTS idx_memory_nodes_updated ON memory_nodes(space_id, updated_at DESC);

-- Indexes for memory_versions
CREATE INDEX IF NOT EXISTS idx_memory_versions_node ON memory_versions(node_id);
CREATE INDEX IF NOT EXISTS idx_memory_versions_status ON memory_versions(node_id, status);

-- Indexes for memory_edges
CREATE INDEX IF NOT EXISTS idx_memory_edges_parent ON memory_edges(parent_node_id);
CREATE INDEX IF NOT EXISTS idx_memory_edges_child ON memory_edges(child_node_id);
CREATE INDEX IF NOT EXISTS idx_memory_edges_space ON memory_edges(space_id);
CREATE INDEX IF NOT EXISTS idx_memory_edges_relation ON memory_edges(relation_kind);

-- Indexes for memory_routes
CREATE INDEX IF NOT EXISTS idx_memory_routes_uri ON memory_routes(space_id, domain, path);
CREATE INDEX IF NOT EXISTS idx_memory_routes_node ON memory_routes(node_id);
CREATE INDEX IF NOT EXISTS idx_memory_routes_primary ON memory_routes(node_id, is_primary);

-- Indexes for memory_keywords
CREATE INDEX IF NOT EXISTS idx_memory_keywords_node ON memory_keywords(node_id);
CREATE INDEX IF NOT EXISTS idx_memory_keywords_space ON memory_keywords(space_id, keyword);

-- FTS5 virtual table for full-text search
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    node_id UNINDEXED,
    title,
    content,
    tokenize='unicode61'
);
";

pub const V5_AGENT_SESSIONS: &str = "
ALTER TABLE conversations ADD COLUMN is_agent INTEGER NOT NULL DEFAULT 0;
ALTER TABLE conversations ADD COLUMN workspace_id TEXT;
";

// First batch: ALTER TABLE only (safe to fail on repeat runs — column already exists)
pub const V5_ALTER: &str = "
ALTER TABLE conversations ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}';
";

// Second batch: all CREATE TABLE IF NOT EXISTS (idempotent, safe to run every time)
pub const V5_TABLES: &str = "
-- Track active workspace in settings table (key = 'active_workspace_id')
-- No schema change needed — settings table already supports arbitrary keys.

-- Per-turn trajectory records for agent sessions
CREATE TABLE IF NOT EXISTS agent_turns (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    turn_index  INTEGER NOT NULL,
    role        TEXT NOT NULL,
    content     TEXT,
    tool_name   TEXT,
    tool_args   TEXT,
    tool_result TEXT,
    reasoning   TEXT,
    is_error    INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_turns_session ON agent_turns(session_id);
CREATE INDEX IF NOT EXISTS idx_agent_turns_tool ON agent_turns(tool_name);

-- FTS5 for full-text search over trajectory content
CREATE VIRTUAL TABLE IF NOT EXISTS agent_turns_fts USING fts5(
    session_id UNINDEXED,
    content,
    tool_result,
    reasoning,
    content='agent_turns',
    content_rowid='rowid',
    tokenize='unicode61'
);

-- Sync triggers for external content FTS table
CREATE TRIGGER IF NOT EXISTS agent_turns_fts_insert
AFTER INSERT ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(rowid, session_id, content, tool_result, reasoning)
  VALUES (new.rowid, new.session_id, new.content, new.tool_result, new.reasoning);
END;

CREATE TRIGGER IF NOT EXISTS agent_turns_fts_update
AFTER UPDATE ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(agent_turns_fts, rowid, session_id, content, tool_result, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.content, old.tool_result, old.reasoning);
  INSERT INTO agent_turns_fts(rowid, session_id, content, tool_result, reasoning)
  VALUES (new.rowid, new.session_id, new.content, new.tool_result, new.reasoning);
END;

CREATE TRIGGER IF NOT EXISTS agent_turns_fts_delete
AFTER DELETE ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(agent_turns_fts, rowid, session_id, content, tool_result, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.content, old.tool_result, old.reasoning);
END;

-- Self-evaluation records
CREATE TABLE IF NOT EXISTS session_evals (
    id           TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL,
    score        REAL NOT NULL,
    reasoning    TEXT,
    learnings    TEXT,
    created_at   INTEGER NOT NULL
);
";

pub const V6_AGENT_TEAMS: &str = "
CREATE TABLE IF NOT EXISTS team_runs (
    id           TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL,
    task         TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'running',
    result       TEXT,
    created_at   INTEGER NOT NULL,
    completed_at INTEGER
);

CREATE TABLE IF NOT EXISTS team_channel_messages (
    id         TEXT PRIMARY KEY,
    team_id    TEXT NOT NULL,
    from_role  TEXT NOT NULL,
    to_role    TEXT,
    message    TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_team_channel_team ON team_channel_messages(team_id);
CREATE INDEX IF NOT EXISTS idx_team_runs_session ON team_runs(session_id);
";

pub const V7_AUTOMATIONS: &str = "
CREATE TABLE IF NOT EXISTS automation_specs (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    toml_content TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS automation_activities (
    id           TEXT PRIMARY KEY,
    spec_id      TEXT NOT NULL,
    run_id       TEXT NOT NULL,
    trigger      TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'running',
    result       TEXT,
    error        TEXT,
    duration_ms  INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    completed_at INTEGER,
    FOREIGN KEY (spec_id) REFERENCES automation_specs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_automation_activities_spec ON automation_activities(spec_id);
CREATE INDEX IF NOT EXISTS idx_automation_activities_status ON automation_activities(status);
";

pub const V8_AGENT_SESSIONS: &str = "
CREATE TABLE IF NOT EXISTS agent_sessions (
    id           TEXT PRIMARY KEY,
    space_id     TEXT NOT NULL DEFAULT 'default',
    title        TEXT NOT NULL DEFAULT 'New session',
    metadata_json TEXT NOT NULL DEFAULT '{}',
    message_count INTEGER NOT NULL DEFAULT 0,
    pinned       INTEGER NOT NULL DEFAULT 0,
    archived     INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_messages (
    id           TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL,
    role         TEXT NOT NULL,
    content      TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_sessions_space ON agent_sessions(space_id);
CREATE INDEX IF NOT EXISTS idx_agent_sessions_updated ON agent_sessions(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_messages_session ON agent_messages(session_id);
";

/// V9: persist reasoning + tool activities + model on chat / agent messages
/// so historical messages can re-render thinking blocks and tool call cards.
/// Each ALTER is wrapped because rusqlite reports an error on duplicate
/// columns; the surrounding `let _ = conn.execute(...)` is what makes
/// re-running idempotent (same pattern V5_ALTER uses).
pub const V9_MESSAGE_PROCESS: &str = "
ALTER TABLE messages ADD COLUMN reasoning TEXT;
ALTER TABLE messages ADD COLUMN tool_activities_json TEXT;
ALTER TABLE messages ADD COLUMN model TEXT;
ALTER TABLE messages ADD COLUMN attachments_json TEXT;
ALTER TABLE agent_messages ADD COLUMN reasoning TEXT;
ALTER TABLE agent_messages ADD COLUMN tool_activities_json TEXT;
ALTER TABLE agent_messages ADD COLUMN events_json TEXT;
ALTER TABLE agent_messages ADD COLUMN model TEXT;
";

/// V10: FTS5 index over chat message content for the global search palette.
///
/// `messages.content` is stored as JSON (Vec<ContentBlock>), so we extract
/// just the text via json_extract in the triggers. The `messages_fts` table
/// uses external content (content='messages') so we don't duplicate storage —
/// SQLite reads back from messages.content_text on snippet/highlight calls.
///
/// `content_text` is a generated column maintained by the same triggers so the
/// FTS index doesn't have to deserialize JSON on every match.
pub const V10_MESSAGES_FTS: &str = "
ALTER TABLE messages ADD COLUMN content_text TEXT NOT NULL DEFAULT '';

CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    conversation_id UNINDEXED,
    role UNINDEXED,
    content_text,
    reasoning,
    content='messages',
    content_rowid='rowid',
    tokenize='unicode61'
);

-- Triggers populate content_text from JSON on every write; the FTS table syncs from there.
CREATE TRIGGER IF NOT EXISTS messages_content_text_insert
AFTER INSERT ON messages BEGIN
  UPDATE messages
  SET content_text = (
    SELECT COALESCE(group_concat(json_extract(value, '$.text'), ' '), '')
    FROM json_each(new.content)
    WHERE json_extract(value, '$.type') = 'text'
  )
  WHERE rowid = new.rowid;

  INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
  VALUES (
    new.rowid,
    new.conversation_id,
    new.role,
    (SELECT content_text FROM messages WHERE rowid = new.rowid),
    new.reasoning
  );
END;

CREATE TRIGGER IF NOT EXISTS messages_content_text_update
AFTER UPDATE ON messages BEGIN
  UPDATE messages
  SET content_text = (
    SELECT COALESCE(group_concat(json_extract(value, '$.text'), ' '), '')
    FROM json_each(new.content)
    WHERE json_extract(value, '$.type') = 'text'
  )
  WHERE rowid = new.rowid;

  INSERT INTO messages_fts(messages_fts, rowid, conversation_id, role, content_text, reasoning)
  VALUES ('delete', old.rowid, old.conversation_id, old.role, old.content_text, old.reasoning);
  INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
  VALUES (
    new.rowid,
    new.conversation_id,
    new.role,
    (SELECT content_text FROM messages WHERE rowid = new.rowid),
    new.reasoning
  );
END;

CREATE TRIGGER IF NOT EXISTS messages_content_text_delete
AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, conversation_id, role, content_text, reasoning)
  VALUES ('delete', old.rowid, old.conversation_id, old.role, old.content_text, old.reasoning);
END;
";

/// V11: switch FTS tokenizer from `unicode61` to `trigram` for messages_fts
/// and agent_turns_fts.
///
/// Drops + recreates both tables (FTS5 has no ALTER tokenizer). Backfills
/// from messages and agent_turns. Trigram gives substring + CJK + multi-
/// word implicit AND. Cost: ~3× index size — acceptable for desktop SQLite.
pub const V11_FTS_TRIGRAM: &str = "
DROP TRIGGER IF EXISTS messages_fts_insert;
DROP TRIGGER IF EXISTS messages_fts_update;
DROP TRIGGER IF EXISTS messages_fts_delete;
DROP TABLE IF EXISTS messages_fts;

DROP TRIGGER IF EXISTS agent_turns_fts_insert;
DROP TRIGGER IF EXISTS agent_turns_fts_update;
DROP TRIGGER IF EXISTS agent_turns_fts_delete;
DROP TABLE IF EXISTS agent_turns_fts;

CREATE VIRTUAL TABLE messages_fts USING fts5(
    conversation_id UNINDEXED,
    role UNINDEXED,
    content_text,
    reasoning,
    content='messages',
    content_rowid='rowid',
    tokenize='trigram'
);

CREATE TRIGGER messages_fts_insert
AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
  VALUES (new.rowid, new.conversation_id, new.role, new.content_text, new.reasoning);
END;

CREATE TRIGGER messages_fts_update
AFTER UPDATE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, conversation_id, role, content_text, reasoning)
  VALUES ('delete', old.rowid, old.conversation_id, old.role, old.content_text, old.reasoning);
  INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
  VALUES (new.rowid, new.conversation_id, new.role, new.content_text, new.reasoning);
END;

CREATE TRIGGER messages_fts_delete
AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, conversation_id, role, content_text, reasoning)
  VALUES ('delete', old.rowid, old.conversation_id, old.role, old.content_text, old.reasoning);
END;

CREATE VIRTUAL TABLE agent_turns_fts USING fts5(
    session_id UNINDEXED,
    content,
    tool_result,
    reasoning,
    content='agent_turns',
    content_rowid='rowid',
    tokenize='trigram'
);

CREATE TRIGGER agent_turns_fts_insert
AFTER INSERT ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(rowid, session_id, content, tool_result, reasoning)
  VALUES (new.rowid, new.session_id, new.content, new.tool_result, new.reasoning);
END;

CREATE TRIGGER agent_turns_fts_update
AFTER UPDATE ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(agent_turns_fts, rowid, session_id, content, tool_result, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.content, old.tool_result, old.reasoning);
  INSERT INTO agent_turns_fts(rowid, session_id, content, tool_result, reasoning)
  VALUES (new.rowid, new.session_id, new.content, new.tool_result, new.reasoning);
END;

CREATE TRIGGER agent_turns_fts_delete
AFTER DELETE ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(agent_turns_fts, rowid, session_id, content, tool_result, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.content, old.tool_result, old.reasoning);
END;
";

pub const V11_BACKFILL_MESSAGES: &str = "
INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
SELECT rowid, conversation_id, role, content_text, reasoning FROM messages
";

pub const V11_BACKFILL_AGENT_TURNS: &str = "
INSERT INTO agent_turns_fts(rowid, session_id, content, tool_result, reasoning)
SELECT rowid, session_id, content, tool_result, reasoning FROM agent_turns
";

/// V12: agent_messages FTS so the agent-domain conversation is searchable.
pub const V12_AGENT_MESSAGES_FTS: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS agent_messages_fts USING fts5(
    session_id UNINDEXED,
    role UNINDEXED,
    content,
    reasoning,
    content='agent_messages',
    content_rowid='rowid',
    tokenize='trigram'
);

CREATE TRIGGER IF NOT EXISTS agent_messages_fts_insert
AFTER INSERT ON agent_messages BEGIN
  INSERT INTO agent_messages_fts(rowid, session_id, role, content, reasoning)
  VALUES (new.rowid, new.session_id, new.role, new.content, new.reasoning);
END;

CREATE TRIGGER IF NOT EXISTS agent_messages_fts_update
AFTER UPDATE ON agent_messages BEGIN
  INSERT INTO agent_messages_fts(agent_messages_fts, rowid, session_id, role, content, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.role, old.content, old.reasoning);
  INSERT INTO agent_messages_fts(rowid, session_id, role, content, reasoning)
  VALUES (new.rowid, new.session_id, new.role, new.content, new.reasoning);
END;

CREATE TRIGGER IF NOT EXISTS agent_messages_fts_delete
AFTER DELETE ON agent_messages BEGIN
  INSERT INTO agent_messages_fts(agent_messages_fts, rowid, session_id, role, content, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.role, old.content, old.reasoning);
END;
";

/// V13: per-turn cost records for the usage dashboard.
pub const V13_COST_RECORDS: &str = "
CREATE TABLE IF NOT EXISTS cost_records (
    id            TEXT PRIMARY KEY,
    session_id    TEXT NOT NULL,
    model         TEXT NOT NULL,
    input_tokens  INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cost_usd      REAL NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cost_records_created ON cost_records(created_at);
CREATE INDEX IF NOT EXISTS idx_cost_records_session ON cost_records(session_id);
CREATE INDEX IF NOT EXISTS idx_cost_records_model   ON cost_records(model);
";

/// V14: tool permission rules + audit log.
///
/// `tool_permission_rules` extends the existing safety_policy.json model
/// (which stays as the "global tier") with two new scopes:
///   - 'session' — only for the named session_id; cleared on session delete
///   - 'pattern' — for tools whose first-arg / command matches `target` as a
///     simple prefix (kept simple on purpose; regex is YAGNI)
/// Resolution precedence in safety/permissions.rs: session > pattern > tool > global.
///
/// `permission_audit_log` records every decision the resolver makes so the
/// settings UI can show a per-session table.
pub const V14_PERMISSION_TABLES: &str = "
CREATE TABLE IF NOT EXISTS tool_permission_rules (
    id          TEXT PRIMARY KEY,
    scope       TEXT NOT NULL CHECK(scope IN ('session', 'pattern')),
    session_id  TEXT,
    tool_name   TEXT NOT NULL,
    target      TEXT,
    mode        TEXT NOT NULL CHECK(mode IN ('allow', 'block', 'ask')),
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tool_permission_rules_session
    ON tool_permission_rules(session_id, tool_name);
CREATE INDEX IF NOT EXISTS idx_tool_permission_rules_pattern
    ON tool_permission_rules(scope, tool_name)
    WHERE scope = 'pattern';

CREATE TABLE IF NOT EXISTS permission_audit_log (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    tool_name   TEXT NOT NULL,
    args_hash   TEXT NOT NULL,
    decision    TEXT NOT NULL CHECK(decision IN ('auto_approve', 'user_approve', 'user_deny', 'blocked')),
    rule_id     TEXT,
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_permission_audit_session ON permission_audit_log(session_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_permission_audit_tool    ON permission_audit_log(tool_name, created_at DESC);
";

/// V15: per-message metrics — duration_ms, token counts, cost stored on each assistant turn.
pub const V15_AGENT_MESSAGE_METRICS: &str = "
ALTER TABLE agent_messages ADD COLUMN duration_ms INTEGER;
ALTER TABLE agent_messages ADD COLUMN input_tokens INTEGER;
ALTER TABLE agent_messages ADD COLUMN output_tokens INTEGER;
ALTER TABLE agent_messages ADD COLUMN cost_usd REAL;
";

/// V16: persist the 'default' workspace as a real DB row (replaces the
/// synthetic in-memory fallback in list_spaces) and re-home agent_sessions
/// whose space_id points at a workspace that doesn't exist (orphan healing
/// from before this migration). Idempotent — safe to re-run.
pub const V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL: &str = "
INSERT OR IGNORE INTO spaces (id, name, icon, path, created_at, updated_at)
VALUES ('default', '默认工作区', '📁', NULL, datetime('now'), datetime('now'));

UPDATE agent_sessions
SET space_id = 'default'
WHERE space_id NOT IN (SELECT id FROM spaces);
";

/// V17: per-workspace + per-session attached directory lists (JSON columns),
/// workspace sort ordering (integer column), and a one-time backfill that
/// derives sort_order from created_at descending so the existing newest-first
/// order is preserved after the schema change.
///
/// All three ALTERs may fail on re-run with "duplicate column" — handled by
/// the per-statement tracing::warn! skip in run(), matching V9/V10 idiom.
pub const V17_WORKSPACE_PATH_SORT_ATTACHED: &str = "
ALTER TABLE spaces ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
ALTER TABLE spaces ADD COLUMN attached_dirs TEXT NOT NULL DEFAULT '[]';
ALTER TABLE agent_sessions ADD COLUMN attached_dirs TEXT NOT NULL DEFAULT '[]';

UPDATE spaces SET sort_order = (
    SELECT COUNT(*) FROM spaces s2 WHERE s2.created_at > spaces.created_at
) WHERE (SELECT COUNT(*) FROM spaces WHERE sort_order != 0) = 0;
";

pub fn run(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    tracing::debug!("Running migration V1: initial schema");
    conn.execute_batch(V1_INITIAL)?;
    // Run V2 migration (ignore error if column/table already exists)
    tracing::debug!("Running migration V2: artifact cache & stars");
    let _ = conn.execute_batch(V2_ARTIFACT_CACHE_AND_STARS);
    // Run V3 migration – memories table
    tracing::debug!("Running migration V3: memories");
    let _ = conn.execute_batch(V3_MEMORIES);
    // Run V4 migration – memory graph tables
    tracing::debug!("Running migration V4: memory graph");
    let _ = conn.execute_batch(V4_MEMORY_GRAPH);
    // Run V5 migration – agent session columns (is_agent, workspace_id)
    tracing::debug!("Running migration V5: agent sessions");
    let _ = conn.execute_batch(V5_AGENT_SESSIONS);
    // V5 additional: ALTER TABLE for metadata column (idempotent failure OK)
    tracing::debug!("Running migration V5a: metadata column");
    let _ = conn.execute_batch(V5_ALTER);
    // V5 additional: CREATE TABLE IF NOT EXISTS for harness tables (always safe)
    tracing::debug!("Running migration V5b: harness tables");
    let _ = conn.execute_batch(V5_TABLES);
    // V6: agent teams tables
    tracing::debug!("Running migration V6: agent teams tables");
    let _ = conn.execute_batch(V6_AGENT_TEAMS);
    // V7: automation tables
    tracing::debug!("Running migration V7: automation tables");
    let _ = conn.execute_batch(V7_AUTOMATIONS);
    // V8: agent sessions tables
    tracing::debug!("Running migration V8: agent sessions tables");
    let _ = conn.execute_batch(V8_AGENT_SESSIONS);
    // V9: per-message process columns (reasoning, tool_activities, model, attachments).
    // Each ALTER is run individually so an existing column doesn't abort the rest.
    tracing::debug!("Running migration V9: message process columns");
    for stmt in V9_MESSAGE_PROCESS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let _ = conn.execute(stmt, []);
    }
    // V10: messages FTS for chat search — must run individual ALTER first because
    // it can fail on re-runs if the column already exists. Subsequent CREATE TRIGGER
    // / CREATE VIRTUAL TABLE statements have IF NOT EXISTS guards.
    tracing::debug!("Running migration V10: messages FTS");
    for stmt in V10_MESSAGES_FTS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let _ = conn.execute(stmt, []);
    }
    // Backfill content_text + FTS index for messages that pre-date V10. Idempotent.
    let _ = conn.execute(
        "UPDATE messages SET content_text = (
            SELECT COALESCE(group_concat(json_extract(value, '$.text'), ' '), '')
            FROM json_each(messages.content)
            WHERE json_extract(value, '$.type') = 'text'
        ) WHERE content_text = ''",
        [],
    );
    let _ = conn.execute(
        "INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
         SELECT rowid, conversation_id, role, content_text, reasoning
         FROM messages
         WHERE rowid NOT IN (SELECT rowid FROM messages_fts)",
        [],
    );
    // V11: re-tokenize FTS5 with trigram for CJK + substring + typo-resilience.
    tracing::debug!("Running migration V11: FTS trigram tokenizer");
    for stmt in V11_FTS_TRIGRAM.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V11 stmt skipped: {} :: {}", e, stmt);
        }
    }
    if let Err(e) = conn.execute(V11_BACKFILL_MESSAGES, []) {
        tracing::warn!("V11 messages backfill failed: {}", e);
    }
    if let Err(e) = conn.execute(V11_BACKFILL_AGENT_TURNS, []) {
        tracing::warn!("V11 agent_turns backfill failed: {}", e);
    }
    // V12: agent_messages FTS so the agent-domain conversation is searchable.
    tracing::debug!("Running migration V12: agent_messages FTS");
    for stmt in V12_AGENT_MESSAGES_FTS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V12 stmt skipped: {} :: {}", e, stmt);
        }
    }
    if let Err(e) = conn.execute(
        "INSERT INTO agent_messages_fts(rowid, session_id, role, content, reasoning)
         SELECT rowid, session_id, role, content, reasoning
         FROM agent_messages
         WHERE rowid NOT IN (SELECT rowid FROM agent_messages_fts)",
        [],
    ) {
        tracing::warn!("V12 agent_messages backfill failed: {}", e);
    }
    // V13: per-turn cost records for the usage dashboard.
    tracing::debug!("Running migration V13: cost records");
    for stmt in V13_COST_RECORDS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V13 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V14: per-session + pattern rules + audit log.
    tracing::debug!("Running migration V14: permission tables");
    for stmt in V14_PERMISSION_TABLES.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V14 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V15: per-message metrics (duration, token counts, cost).
    tracing::debug!("Running migration V15: agent_message metrics columns");
    for stmt in V15_AGENT_MESSAGE_METRICS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V15 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V16: persist 'default' workspace + heal orphan agent_sessions.
    tracing::debug!("Running migration V16: workspace default + orphan heal");
    for stmt in V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V16 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V17: workspace sort + attached directories columns + backfill.
    tracing::debug!("Running migration V17: workspace path/sort/attached_dirs");
    for stmt in V17_WORKSPACE_PATH_SORT_ATTACHED.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V17 stmt skipped: {} :: {}", e, stmt);
        }
    }
    tracing::info!("Database migrations complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    /// Apply only the migrations needed to set up `spaces` and `agent_sessions`,
    /// stopping BEFORE V16 so tests can drive V16 themselves and observe
    /// pre/post state.
    fn db_pre_v16() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // V1 creates `spaces`. V8 creates `agent_sessions`. We don't need the
        // intermediate migrations because none of them touch the columns
        // we're testing here.
        conn.execute_batch(super::V1_INITIAL).unwrap();
        // V8 contains a multi-statement block; use execute_batch.
        conn.execute_batch(super::V8_AGENT_SESSIONS).unwrap();
        conn
    }

    fn run_v16(conn: &Connection) {
        for stmt in super::V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            conn.execute(stmt, []).unwrap();
        }
    }

    #[test]
    fn v16_inserts_default_idempotent() {
        let conn = db_pre_v16();

        // First run inserts 'default'.
        run_v16(&conn);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM spaces WHERE id = 'default'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "first V16 run should insert one 'default' row");

        // Second run is a no-op.
        run_v16(&conn);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM spaces WHERE id = 'default'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "second V16 run must not create a duplicate");
    }

    #[test]
    fn v16_heals_orphan_agent_sessions() {
        let conn = db_pre_v16();

        // Pre-V16: insert an agent_session pointing at a workspace that does
        // not exist in `spaces`.
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at)
             VALUES ('s-orphan', 'ghost-workspace', 'orphaned session', 0, 0)",
            [],
        )
        .unwrap();

        run_v16(&conn);

        // Post-V16: orphan should be re-homed to 'default'.
        let space_id: String = conn
            .query_row(
                "SELECT space_id FROM agent_sessions WHERE id = 's-orphan'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(space_id, "default", "orphan session must be re-homed to 'default'");
    }

    /// Apply migrations through V16 so V17 has a populated schema to extend.
    fn db_pre_v17() -> Connection {
        let conn = db_pre_v16();
        // V16 needs to run first so 'default' exists, otherwise the
        // V17 backfill counts can be confused by data that V16 would touch.
        run_v16(&conn);
        conn
    }

    /// Apply V17 statements via .unwrap(). **First-run only** — calling
    /// this twice on the same connection will panic with "duplicate column"
    /// because ALTER TABLE ADD COLUMN isn't idempotent in SQLite. The
    /// production `run()` uses warn-on-error to allow safe re-runs; tests
    /// that need to verify re-run behavior must inline the loop and
    /// swallow errors manually (see `v17_adds_sort_order_column_idempotent`).
    fn run_v17(conn: &Connection) {
        for stmt in super::V17_WORKSPACE_PATH_SORT_ATTACHED
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            conn.execute(stmt, []).unwrap();
        }
    }

    #[test]
    fn v17_adds_sort_order_column_idempotent() {
        let conn = db_pre_v17();
        run_v17(&conn);
        let mut stmt = conn.prepare("SELECT sort_order FROM spaces WHERE id = 'default'").unwrap();
        let val: i64 = stmt.query_row([], |r| r.get(0)).unwrap();
        assert_eq!(val, 0, "default workspace should be at sort_order 0 (only workspace)");

        for stmt in super::V17_WORKSPACE_PATH_SORT_ATTACHED.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            let _ = conn.execute(stmt, []);
        }
        let val2: i64 = conn.query_row("SELECT sort_order FROM spaces WHERE id = 'default'", [], |r| r.get(0)).unwrap();
        assert_eq!(val2, 0, "sort_order must remain 0 after re-run");
    }

    #[test]
    fn v17_adds_workspace_attached_dirs_column() {
        let conn = db_pre_v17();
        run_v17(&conn);
        let val: String = conn.query_row(
            "SELECT attached_dirs FROM spaces WHERE id = 'default'", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(val, "[]", "fresh workspace should have empty attached_dirs JSON");
    }

    #[test]
    fn v17_adds_session_attached_dirs_column() {
        let conn = db_pre_v17();
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at)
             VALUES ('s-1', 'default', 'test', 0, 0)",
            [],
        ).unwrap();
        run_v17(&conn);
        let val: String = conn.query_row(
            "SELECT attached_dirs FROM agent_sessions WHERE id = 's-1'", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(val, "[]", "fresh session should have empty attached_dirs JSON");
    }

    #[test]
    fn v17_backfills_sort_order_from_created_at() {
        let conn = db_pre_v17();
        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, created_at, updated_at)
             VALUES ('ws-a', 'A', '📁', NULL, '2026-05-01 00:00:00', '2026-05-01 00:00:00')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, created_at, updated_at)
             VALUES ('ws-b', 'B', '📁', NULL, '2099-01-01 00:00:00', '2099-01-01 00:00:00')",
            [],
        ).unwrap();
        run_v17(&conn);

        let mut stmt = conn.prepare(
            "SELECT id, sort_order FROM spaces ORDER BY sort_order ASC"
        ).unwrap();
        let rows: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        let ws_b_order = rows.iter().find(|(id, _)| id == "ws-b").map(|(_, o)| *o).unwrap();
        let ws_a_order = rows.iter().find(|(id, _)| id == "ws-a").map(|(_, o)| *o).unwrap();
        assert_eq!(ws_b_order, 0, "newest workspace ws-b should have sort_order 0");
        assert_eq!(ws_a_order, 2, "oldest workspace ws-a should have sort_order 2");
    }

    #[test]
    fn v17_backfill_skips_after_user_reorder() {
        let conn = db_pre_v17();
        // Insert 3 workspaces with non-trivial created_at ordering.
        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, created_at, updated_at)
             VALUES ('ws-a', 'A', '📁', NULL, '2026-05-01 00:00:00', '2026-05-01 00:00:00')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, created_at, updated_at)
             VALUES ('ws-b', 'B', '📁', NULL, '2026-05-11 00:00:00', '2026-05-11 00:00:00')",
            [],
        ).unwrap();

        // First V17 run does the initial backfill.
        run_v17(&conn);

        // Simulate user reorder: put ws-a at sort_order 0, ws-b at sort_order 5.
        conn.execute("UPDATE spaces SET sort_order = 0 WHERE id = 'ws-a'", []).unwrap();
        conn.execute("UPDATE spaces SET sort_order = 5 WHERE id = 'ws-b'", []).unwrap();

        // Re-run V17 (simulating app reboot). The backfill UPDATE should be a no-op
        // because at least one row has sort_order != 0.
        // Manually inline the V17 SQL with error-swallowing (run_v17 unwraps would
        // panic on the ALTERs because columns already exist).
        for stmt in super::V17_WORKSPACE_PATH_SORT_ATTACHED
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            let _ = conn.execute(stmt, []);
        }

        // User's reorder values should be preserved.
        let a_order: i64 = conn.query_row(
            "SELECT sort_order FROM spaces WHERE id = 'ws-a'", [], |r| r.get(0)
        ).unwrap();
        let b_order: i64 = conn.query_row(
            "SELECT sort_order FROM spaces WHERE id = 'ws-b'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(a_order, 0, "user-set sort_order=0 preserved across re-run");
        assert_eq!(b_order, 5, "user-set sort_order=5 preserved across re-run");
    }
}
