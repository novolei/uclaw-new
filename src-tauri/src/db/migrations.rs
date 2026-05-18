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

-- FTS5 virtual table for full-text search (trigram for CJK + substring support)
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    node_id UNINDEXED,
    title,
    content,
    tokenize='trigram'
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

/// V18: pin state for agent sessions. Nullable INTEGER stores the ms
/// timestamp when the session was pinned; NULL means unpinned. Defaults
/// to NULL for new + existing rows. The pre-existing `pinned` INTEGER
/// column is unrelated (used by chat conversations) and left untouched.
///
/// ALTER may fail on re-run with "duplicate column" — handled by the
/// per-statement tracing::warn! skip in run(), matching V9/V10/V17 idiom.
pub const V18_AGENT_SESSIONS_PINNED_AT: &str = "
ALTER TABLE agent_sessions ADD COLUMN pinned_at INTEGER NULL;
CREATE INDEX IF NOT EXISTS idx_agent_sessions_pinned_at ON agent_sessions(pinned_at);
";

/// V19 — per-workspace skill tag scoping (architecture brief item #3).
///
/// Stores a JSON array of lowercased tag strings. Empty array (the
/// default) means "no filter" — the workspace sees every enabled skill,
/// matching pre-V19 behavior. Non-empty means the skills_manifest filter
/// applies: a skill is included iff its own tags intersect the workspace's
/// tags, OR the skill has no tags (untagged = global, like a fresh-extracted
/// learned skill).
///
/// ALTER may fail on re-run with "duplicate column" — handled by the
/// per-statement tracing::warn! skip in run(), matching V9/V10/V17/V18 idiom.
pub const V19_SPACES_SKILL_TAGS: &str = "
ALTER TABLE spaces ADD COLUMN skill_tags TEXT NOT NULL DEFAULT '[]';
";

// ---------------------------------------------------------------------------
// V20 — rewrite automation_specs + automation_activities to Humane schema
// ---------------------------------------------------------------------------
//
// Three sub-steps executed inside a single transaction:
//
// V20a: create automation_specs_new with the Humane schema (spec_yaml + spec_json
//       + flat identity columns + source/source_ref/source_version for marketplace).
//
// V20b: create automation_activities_new with trigger_source_type +
//       trigger_payload_json + runtime metric columns + resumption chain columns.
//       NOTE: FK columns referencing tables created in V21
//       (subscription_id → automation_subscriptions,
//        escalation_id → automation_escalations,
//        resumed_from_escalation_id → automation_escalations)
//       are present as plain TEXT columns WITHOUT REFERENCES clauses. SQLite does
//       not support adding FK constraints to existing columns via ALTER TABLE, so
//       these will remain without FK enforcement. Application layer enforces
//       referential integrity for these three columns. Self-reference
//       resumed_from_activity_id → automation_activities is safe to add now (same
//       table) and is included with ON DELETE SET NULL.
//
// V20c: data fixup — migrates legacy toml_content rows to Humane YAML via
//       migrate_legacy_toml(), marks source='toml-migrated'. Failures per-row
//       produce status='error' stub rows and do not abort the transaction.
//       Legacy automation_activities rows are mapped with a trigger heuristic.
//       Final swap drops the old tables and renames the new ones.

const SQL_V20A_CREATE_SPECS: &str = "
CREATE TABLE IF NOT EXISTS automation_specs_new (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    version             TEXT NOT NULL,
    author              TEXT NOT NULL,
    description         TEXT NOT NULL,
    system_prompt       TEXT NOT NULL,

    spec_format         TEXT NOT NULL DEFAULT 'humane-yaml-v1',
    spec_yaml           TEXT NOT NULL,
    spec_json           TEXT NOT NULL,

    user_config_values  TEXT NOT NULL DEFAULT '{}',
    permissions_granted TEXT NOT NULL DEFAULT '[]',
    permissions_denied  TEXT NOT NULL DEFAULT '[]',

    status              TEXT NOT NULL DEFAULT 'active',
    enabled             INTEGER NOT NULL DEFAULT 1,
    space_id            TEXT,

    source              TEXT NOT NULL DEFAULT 'local',
    source_ref          TEXT,
    source_version      TEXT,

    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    last_run_at         INTEGER,
    last_run_outcome    TEXT
);
CREATE INDEX IF NOT EXISTS idx_specs_status    ON automation_specs_new(status);
CREATE INDEX IF NOT EXISTS idx_specs_space     ON automation_specs_new(space_id);
CREATE INDEX IF NOT EXISTS idx_specs_enabled   ON automation_specs_new(enabled);
CREATE INDEX IF NOT EXISTS idx_specs_source    ON automation_specs_new(source, source_version);
";

const SQL_V20B_CREATE_ACTIVITIES: &str = "
CREATE TABLE IF NOT EXISTS automation_activities_new (
    id                          TEXT PRIMARY KEY,
    spec_id                     TEXT NOT NULL,
    -- NOTE: subscription_id references automation_subscriptions which does not
    -- exist until V21. FK clause omitted intentionally — see module-level comment.
    subscription_id             TEXT,
    trigger_source_type         TEXT NOT NULL DEFAULT 'manual',
    trigger_payload_json        TEXT NOT NULL DEFAULT '{}',

    status                      TEXT NOT NULL DEFAULT 'queued',
    error_text                  TEXT,
    queued_at                   INTEGER NOT NULL,
    started_at                  INTEGER,
    completed_at                INTEGER,
    duration_ms                 INTEGER NOT NULL DEFAULT 0,

    llm_iterations              INTEGER NOT NULL DEFAULT 0,
    llm_tokens_in               INTEGER NOT NULL DEFAULT 0,
    llm_tokens_out              INTEGER NOT NULL DEFAULT 0,

    tool_calls_json             TEXT NOT NULL DEFAULT '[]',  -- dropped by V24

    report_text                 TEXT,
    report_outcome              TEXT,

    -- NOTE: escalation_id references automation_escalations which does not
    -- exist until V21. FK clause omitted intentionally — see module-level comment.
    escalation_id               TEXT,

    -- Self-reference FK is safe: same table. SET NULL on delete.
    resumed_from_activity_id    TEXT REFERENCES automation_activities_new(id) ON DELETE SET NULL,
    -- NOTE: resumed_from_escalation_id references automation_escalations which
    -- does not exist until V21. FK clause omitted intentionally.
    resumed_from_escalation_id  TEXT,

    FOREIGN KEY (spec_id) REFERENCES automation_specs_new(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_act_spec      ON automation_activities_new(spec_id);
CREATE INDEX IF NOT EXISTS idx_act_status    ON automation_activities_new(status);
CREATE INDEX IF NOT EXISTS idx_act_queued_at ON automation_activities_new(queued_at DESC);
CREATE INDEX IF NOT EXISTS idx_act_resumed   ON automation_activities_new(resumed_from_activity_id);
CREATE INDEX IF NOT EXISTS idx_act_sub       ON automation_activities_new(subscription_id);
";

const SQL_V20_SWAP: &str = "
DROP TABLE IF EXISTS automation_activities;
DROP TABLE IF EXISTS automation_specs;
ALTER TABLE automation_specs_new RENAME TO automation_specs;
ALTER TABLE automation_activities_new RENAME TO automation_activities;
";

/// Map a legacy trigger string to a trigger_source_type string.
fn map_trigger_source_type(trigger: &str) -> &'static str {
    match trigger.to_ascii_lowercase().trim() {
        "cron" => "cron",
        "manual" => "manual",
        _ => "manual",
    }
}

/// V20c — migrate legacy automation_specs rows into automation_specs_new.
/// Each row is processed independently: on success, the Humane YAML is
/// inserted with source='toml-migrated'. On failure, a stub row with
/// status='error' is inserted and the error is logged. Neither outcome
/// aborts the transaction.
fn migrate_specs_data(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    use crate::automation::protocol::{
        migrate_toml_v1::migrate_legacy_toml,
        normalize::normalize_to_json,
    };

    // Fetch all legacy rows. We read everything upfront to avoid borrow
    // conflicts between the SELECT statement and the INSERT statements.
    let mut stmt = conn.prepare(
        "SELECT id, name, description, toml_content, enabled, created_at, updated_at
         FROM automation_specs"
    )?;

    struct LegacyRow {
        id: String,
        name: String,
        description: String,
        toml_content: String,
        enabled: i64,
        created_at: i64,
        updated_at: i64,
    }

    let rows: Vec<LegacyRow> = stmt
        .query_map([], |row| {
            Ok(LegacyRow {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                toml_content: row.get(3)?,
                enabled: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for row in rows {
        match migrate_legacy_toml(&row.toml_content) {
            Ok(migrated) => {
                let spec_json = normalize_to_json(&migrated.spec).unwrap_or_else(|e| {
                    tracing::warn!("V20c: failed to normalize spec_json for {}: {}", row.id, e);
                    "{}".to_string()
                });
                if let Err(e) = conn.execute(
                    "INSERT INTO automation_specs_new
                     (id, name, version, author, description, system_prompt,
                      spec_format, spec_yaml, spec_json,
                      status, enabled, source,
                      created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                             'humane-yaml-v1', ?7, ?8,
                             'active', ?9, 'toml-migrated',
                             ?10, ?11)",
                    rusqlite::params![
                        row.id,
                        migrated.spec.name,
                        migrated.spec.version,
                        migrated.spec.author,
                        migrated.spec.description,
                        migrated.spec.system_prompt,
                        migrated.yaml,
                        spec_json,
                        row.enabled,
                        row.created_at,
                        row.updated_at,
                    ],
                ) {
                    tracing::error!("V20c: failed to insert migrated spec {}: {}", row.id, e);
                }
            }
            Err(e) => {
                // Migration failed — insert a stub so the row is not silently lost.
                tracing::warn!("V20c: migration failed for spec {}: {} — inserting error stub", row.id, e);
                let stub_yaml = format!("# migration-error: {}", e);
                let _ = conn.execute(
                    "INSERT INTO automation_specs_new
                     (id, name, version, author, description, system_prompt,
                      spec_format, spec_yaml, spec_json,
                      status, enabled, source,
                      created_at, updated_at)
                     VALUES (?1, ?2, '0.0.0', 'uclaw-migrated', ?3, '',
                             'humane-yaml-v1', ?4, '{}',
                             'error', ?5, 'toml-migrated',
                             ?6, ?7)",
                    rusqlite::params![
                        row.id,
                        row.name,
                        row.description,
                        stub_yaml,
                        row.enabled,
                        row.created_at,
                        row.updated_at,
                    ],
                );
            }
        }
    }

    Ok(())
}

/// V20c — migrate legacy automation_activities rows into automation_activities_new.
fn migrate_activities_data(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // Fetch all legacy rows upfront.
    let mut stmt = conn.prepare(
        "SELECT id, spec_id, trigger, status, result, error, duration_ms, created_at, completed_at
         FROM automation_activities"
    )?;

    struct LegacyActivity {
        id: String,
        spec_id: String,
        trigger: String,
        status: String,
        result: Option<String>,
        error: Option<String>,
        duration_ms: i64,
        created_at: i64,
        completed_at: Option<i64>,
    }

    let rows: Vec<LegacyActivity> = stmt
        .query_map([], |row| {
            Ok(LegacyActivity {
                id: row.get(0)?,
                spec_id: row.get(1)?,
                trigger: row.get(2)?,
                status: row.get(3)?,
                result: row.get(4)?,
                error: row.get(5)?,
                duration_ms: row.get(6)?,
                created_at: row.get(7)?,
                completed_at: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for row in rows {
        let trigger_source_type = map_trigger_source_type(&row.trigger);
        let new_status = row.status.to_ascii_lowercase();
        // Map legacy status → new status vocabulary.
        let mapped_status = match new_status.as_str() {
            "completed" | "success" => "completed",
            "failed" | "error" => "failed",
            "running" => "running",
            _ => "completed",
        };
        // report_outcome: 'useful' if the legacy run was successful.
        let report_outcome: Option<&str> = match new_status.as_str() {
            "completed" | "success" => Some("useful"),
            _ => None,
        };
        // error_text from legacy error column.
        let error_text: Option<&str> = row.error.as_deref();

        if let Err(e) = conn.execute(
            "INSERT INTO automation_activities_new
             (id, spec_id, trigger_source_type, status, error_text,
              queued_at, completed_at, duration_ms,
              report_text, report_outcome)
             VALUES (?1, ?2, ?3, ?4, ?5,
                     ?6, ?7, ?8,
                     ?9, ?10)",
            rusqlite::params![
                row.id,
                row.spec_id,
                trigger_source_type,
                mapped_status,
                error_text,
                row.created_at,
                row.completed_at,
                row.duration_ms,
                row.result,
                report_outcome,
            ],
        ) {
            tracing::error!("V20c: failed to migrate activity {}: {}", row.id, e);
        }
    }

    Ok(())
}

/// V20 — rewrite automation_specs + automation_activities to the Humane schema.
///
/// All three sub-steps (V20a table creation, V20b table creation, V20c data
/// fixup + final swap) run inside a single transaction so any failure leaves
/// the DB in its pre-V20 state.
pub fn run_v20(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // Idempotency check: if automation_specs already has the new-schema
    // `spec_yaml` column, V20 has already been applied — skip the whole
    // migration. Without this guard, a successful V20 followed by a failed
    // V21 (where the .ok() at app.rs:248 swallowed the error pre-fix)
    // leaves us with: automation_specs at new schema, V21 tables missing,
    // and the next startup retrying V20 → SELECT toml_content FROM
    // (new-schema) automation_specs → "no such column" → V20 fails again
    // → V21 never gets to run → automation_escalations never created.
    let v20_already_applied: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('automation_specs') WHERE name = 'spec_yaml'",
        [],
        |row| row.get::<_, i64>(0),
    ).map(|n| n > 0).unwrap_or(false);
    if v20_already_applied {
        tracing::info!("V20 skipped: automation_specs already on Humane schema (V20 was applied previously)");
        return Ok(());
    }

    let tx = conn.unchecked_transaction()?;

    // V20a — create automation_specs_new
    tracing::debug!("V20a: creating automation_specs_new");
    tx.execute_batch(SQL_V20A_CREATE_SPECS)?;

    // V20b — create automation_activities_new
    tracing::debug!("V20b: creating automation_activities_new");
    tx.execute_batch(SQL_V20B_CREATE_ACTIVITIES)?;

    // V20c — migrate existing legacy rows
    tracing::debug!("V20c: migrating legacy automation_specs rows");
    migrate_specs_data(&tx)?;
    tracing::debug!("V20c: migrating legacy automation_activities rows");
    migrate_activities_data(&tx)?;

    // Final swap — drop old tables, rename new ones
    tracing::debug!("V20c: swapping tables");
    tx.execute_batch(SQL_V20_SWAP)?;

    tx.commit()
}

const SQL_V21: &str = "
CREATE TABLE IF NOT EXISTS automation_subscriptions (
    id            TEXT PRIMARY KEY,
    spec_id       TEXT NOT NULL REFERENCES automation_specs(id) ON DELETE CASCADE,
    source_type   TEXT NOT NULL,
    config_json   TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    last_fired_at INTEGER,
    created_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sub_spec        ON automation_subscriptions(spec_id);
CREATE INDEX IF NOT EXISTS idx_sub_source_type ON automation_subscriptions(source_type);

CREATE TABLE IF NOT EXISTS automation_memory (
    spec_id                 TEXT PRIMARY KEY REFERENCES automation_specs(id) ON DELETE CASCADE,
    last_updated_at         INTEGER NOT NULL,
    compacted_archives_json TEXT NOT NULL DEFAULT '[]',
    bytes                   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS automation_escalations (
    id           TEXT PRIMARY KEY,
    spec_id      TEXT NOT NULL REFERENCES automation_specs(id) ON DELETE CASCADE,
    activity_id  TEXT NOT NULL REFERENCES automation_activities(id) ON DELETE CASCADE,
    question     TEXT NOT NULL,
    choices_json TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'waiting',
    user_choice  TEXT,
    user_note    TEXT,
    created_at   INTEGER NOT NULL,
    responded_at INTEGER
);
CREATE INDEX IF NOT EXISTS idx_escalation_spec   ON automation_escalations(spec_id);
CREATE INDEX IF NOT EXISTS idx_escalation_status ON automation_escalations(status);
";

pub fn run_v21(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SQL_V21)
}

/// V22 — automation_installed_skills.
///
/// Records which bundled skills each marketplace-installed automation pulled
/// in. Read by AppsView to enumerate "what got installed alongside this
/// automation" and by uninstall to delete the right files.
///
/// file_count is a cheap drift detector — diagnostic only in this PR.
const SQL_V22: &str = "
CREATE TABLE IF NOT EXISTS automation_installed_skills (
    automation_slug TEXT NOT NULL,
    skill_id        TEXT NOT NULL,
    installed_at    INTEGER NOT NULL,
    file_count      INTEGER NOT NULL,
    PRIMARY KEY (automation_slug, skill_id)
);
CREATE INDEX IF NOT EXISTS idx_aut_inst_skills_slug
    ON automation_installed_skills(automation_slug);
";

const V23A_MARKETPLACE_CACHE: &str = "
CREATE TABLE IF NOT EXISTS automation_marketplace_items (
    registry_id      TEXT NOT NULL,
    slug             TEXT NOT NULL,
    name             TEXT NOT NULL,
    version          TEXT NOT NULL,
    author           TEXT NOT NULL,
    description      TEXT NOT NULL,
    item_type        TEXT NOT NULL,
    category         TEXT NOT NULL DEFAULT 'other',
    icon             TEXT,
    tags_json        TEXT NOT NULL DEFAULT '[]',
    locale           TEXT,
    min_app_version  TEXT,
    size_bytes       INTEGER,
    checksum         TEXT,
    requires_json    TEXT NOT NULL DEFAULT '{}',
    i18n_json        TEXT NOT NULL DEFAULT '{}',
    spec_yaml        TEXT,
    updated_at_index TEXT,
    cached_at        INTEGER NOT NULL,
    PRIMARY KEY (registry_id, slug)
);

CREATE INDEX IF NOT EXISTS idx_marketplace_type     ON automation_marketplace_items(item_type);
CREATE INDEX IF NOT EXISTS idx_marketplace_category ON automation_marketplace_items(category);

CREATE VIRTUAL TABLE IF NOT EXISTS automation_marketplace_fts USING fts5(
    slug UNINDEXED,
    registry_id UNINDEXED,
    name,
    description,
    author,
    tags,
    tokenize = 'trigram'
);

CREATE TABLE IF NOT EXISTS automation_registry_sync (
    registry_id    TEXT PRIMARY KEY,
    last_synced_at INTEGER,
    last_etag      TEXT,
    last_modified  TEXT,
    last_error     TEXT,
    item_count     INTEGER NOT NULL DEFAULT 0
);
";

/// V25 — marketplace_standalone_installs.
///
/// Tracks standalone (non-bundled) skill and MCP marketplace installs so the
/// AppsTab can list them and uninstall can find what to remove. `mcp_server_id`
/// links a `type: mcp` install to its mcp_servers.json entry; NULL for skills.
/// (V24 is claimed by the parallel Automation Phase 2a branch.)
const SQL_V25: &str = "
CREATE TABLE IF NOT EXISTS marketplace_standalone_installs (
    slug          TEXT PRIMARY KEY,
    item_type     TEXT NOT NULL,
    version       TEXT NOT NULL,
    installed_at  INTEGER NOT NULL,
    mcp_server_id TEXT
);
";

/// V26 — conversations gains `archived` + `archived_at` for general session archiving.
const SQL_V26: &str = "
ALTER TABLE conversations ADD COLUMN archived  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE conversations ADD COLUMN archived_at INTEGER;
";

/// V27 — user-customizable system prompts for chat/agent paths.
const SQL_V27: &str = "
CREATE TABLE IF NOT EXISTS system_prompts (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    content    TEXT NOT NULL DEFAULT '',
    is_builtin INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_system_prompts_sort ON system_prompts(sort_order);

INSERT OR IGNORE INTO system_prompts (id, name, content, is_builtin, sort_order, created_at, updated_at)
VALUES ('builtin-default', '默认', 'You are a helpful assistant.', 1, 0,
        CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        CAST(strftime('%s', 'now') AS INTEGER) * 1000);
";

/// V28 — version history for system prompts.
/// Records a snapshot every time a prompt is created or updated.
const SQL_V28: &str = "
CREATE TABLE IF NOT EXISTS system_prompt_versions (
    id         TEXT PRIMARY KEY,
    prompt_id  TEXT NOT NULL,
    name       TEXT NOT NULL,
    content    TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    FOREIGN KEY (prompt_id) REFERENCES system_prompts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_sp_versions_prompt ON system_prompt_versions(prompt_id, created_at DESC);
";

/// V29 — logical compaction support.
///
/// Replaces physical DELETE with logical marking:
/// - agent_messages.compacted: 0 = active, 1 = logically removed
/// - compaction_markers: records each compaction event's metadata
///
/// This aligns with openhanako's appendCompaction() pattern — messages
/// persist in the database and can be re-rendered by the UI, while the
/// LLM context builder skips compacted messages.
const V29_COMPACTION_SUPPORT: &str = "
ALTER TABLE agent_messages ADD COLUMN compacted INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS compaction_markers (
    id                    TEXT PRIMARY KEY,
    session_id            TEXT NOT NULL,
    summary               TEXT NOT NULL DEFAULT '',
    removed_count         INTEGER NOT NULL DEFAULT 0,
    tokens_before         INTEGER,
    tokens_after          INTEGER,
    first_kept_message_id TEXT,
    model_window          INTEGER,
    created_at            INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_compaction_markers_session ON compaction_markers(session_id);
";

// ---------------------------------------------------------------------------
// V30 — fragment_reviews + daily_summaries for the memory fragment system
// ---------------------------------------------------------------------------
const V30_FRAGMENT_TABLES: &str = "
CREATE TABLE IF NOT EXISTS fragment_reviews (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL,
    session_id TEXT,
    review_count INTEGER DEFAULT 0,
    next_review_at INTEGER,
    last_reviewed_at INTEGER,
    completed INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_fragment_reviews_next ON fragment_reviews(next_review_at);
CREATE INDEX IF NOT EXISTS idx_fragment_reviews_node ON fragment_reviews(node_id);

CREATE TABLE IF NOT EXISTS daily_summaries (
    id TEXT PRIMARY KEY,
    summary_date TEXT NOT NULL,
    content TEXT NOT NULL,
    fragment_count INTEGER DEFAULT 0,
    fragment_ids_json TEXT,
    created_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_daily_summaries_date ON daily_summaries(summary_date);
";

// V31 — rebuild memory_fts with trigram tokenizer for CJK + substring search.
// Drops the old unicode61 table, recreates with trigram, then backfills from
// memory_nodes + active memory_versions. Same pattern as V11 for messages_fts.
const V31_MEMORY_FTS_TRIGRAM: &str = "
DROP TABLE IF EXISTS memory_fts;

CREATE VIRTUAL TABLE memory_fts USING fts5(
    node_id UNINDEXED,
    title,
    content,
    tokenize='trigram'
);
";

const V31_BACKFILL_MEMORY_FTS: &str = "
INSERT INTO memory_fts(node_id, title, content)
SELECT n.id, n.title, v.content
FROM memory_nodes n
INNER JOIN memory_versions v ON v.node_id = n.id
WHERE v.status = 'active'
  AND v.content IS NOT NULL AND v.content != '';
";

/// V32 — IM channel infrastructure: instances, sessions, spec bindings,
/// and three new columns on automation_specs (trigger_phrase, system_prompt override, description).
const SQL_V32: &str = "
CREATE TABLE IF NOT EXISTS im_channel_instances (
    id                   TEXT PRIMARY KEY,
    space_id             TEXT NOT NULL,
    channel_type         TEXT NOT NULL,
    name                 TEXT NOT NULL,
    config_json          TEXT NOT NULL DEFAULT '{}',
    credentials_json     TEXT NOT NULL DEFAULT '{}',
    enabled              INTEGER NOT NULL DEFAULT 1,
    streaming            INTEGER NOT NULL DEFAULT 0,
    reply_scope          TEXT NOT NULL DEFAULT 'all',
    permission_enabled   INTEGER NOT NULL DEFAULT 0,
    owners_json          TEXT NOT NULL DEFAULT '[]',
    guest_policy_json    TEXT NOT NULL DEFAULT '{}',
    created_at           INTEGER NOT NULL,
    updated_at           INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_im_channel_instances_space
    ON im_channel_instances(space_id, enabled);

CREATE TABLE IF NOT EXISTS im_sessions (
    id               TEXT PRIMARY KEY,
    space_id         TEXT NOT NULL,
    channel_type     TEXT NOT NULL,
    chat_id          TEXT NOT NULL,
    agent_session_id TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    last_active_at   INTEGER NOT NULL,
    UNIQUE(space_id, channel_type, chat_id)
);

CREATE TABLE IF NOT EXISTS spec_channel_bindings (
    spec_id             TEXT NOT NULL,
    channel_instance_id TEXT NOT NULL,
    enabled             INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (spec_id, channel_instance_id)
);
";

/// V32b — ALTER TABLE additions to automation_specs (separate statements for idempotency).
const SQL_V32B: &str = "
ALTER TABLE automation_specs ADD COLUMN trigger_phrase TEXT;
ALTER TABLE automation_specs ADD COLUMN system_prompt_override TEXT;
ALTER TABLE automation_specs ADD COLUMN spec_description TEXT;
";
pub fn run_v23a(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(V23A_MARKETPLACE_CACHE)
}

/// V24 — automation run = agent_session ownership model.
/// `automation_activities` gains `session_id` (nullable link to the run's
/// agent_session) + `report_artifacts_json` (declared products), and drops
/// `tool_calls_json` (per-tool breakdown now lives in agent_messages).
/// `agent_sessions` gains `archived_at` for retention ordering.
/// All statements are individually error-tolerant: a re-run hits
/// "duplicate column" / "no such column" and is skipped (same pattern as
/// V9–V19). DROP COLUMN requires SQLite >= 3.35 (rusqlite bundles a newer one).
const V24_AUTOMATION_RUN_SESSIONS: &str = "
ALTER TABLE automation_activities ADD COLUMN session_id TEXT;
ALTER TABLE automation_activities ADD COLUMN report_artifacts_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE automation_activities DROP COLUMN tool_calls_json;
ALTER TABLE agent_sessions ADD COLUMN archived_at INTEGER;
CREATE INDEX IF NOT EXISTS idx_act_session ON automation_activities(session_id);
";

/// V33 — Symphony runtime schema. Four tables backing the DAG-of-agent-runs
/// orchestrator described in `docs/superpowers/specs/2026-05-17-symphony-runtime-design.md` §8.2:
///
/// - `symphony_workflows`         one row per workflow (current version pointer).
/// - `symphony_workflow_versions` immutable snapshots; runs pin a version.
/// - `symphony_runs`              one row per run.
/// - `symphony_node_runs`         one row per node attempt (retries get new rows).
///
/// Plus a seeded `spaces.id = 'symphonies'` home for the per-run agent_sessions.
///
/// All statements are individually error-tolerant (`IF NOT EXISTS` + `INSERT OR
/// IGNORE`), matching the V25/V26 style.
const SQL_V33_SYMPHONY: &str = "
CREATE TABLE IF NOT EXISTS symphony_workflows (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    description     TEXT,
    space_id        TEXT,
    current_version INTEGER NOT NULL DEFAULT 1,
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    FOREIGN KEY (space_id) REFERENCES spaces(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS symphony_workflow_versions (
    workflow_id     TEXT NOT NULL,
    version         INTEGER NOT NULL,
    definition_yaml TEXT NOT NULL,
    definition_md   TEXT NOT NULL,
    nodes_json      TEXT NOT NULL,
    edges_json      TEXT NOT NULL,
    created_at      INTEGER NOT NULL,
    PRIMARY KEY (workflow_id, version),
    FOREIGN KEY (workflow_id) REFERENCES symphony_workflows(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS symphony_runs (
    id              TEXT PRIMARY KEY,
    workflow_id     TEXT NOT NULL,
    workflow_version INTEGER NOT NULL,
    trigger_kind    TEXT NOT NULL,
    trigger_payload_json TEXT NOT NULL DEFAULT '{}',
    status          TEXT NOT NULL,
    outcome         TEXT,
    inputs_json     TEXT NOT NULL DEFAULT '{}',
    outputs_json    TEXT,
    total_cost_usd  REAL NOT NULL DEFAULT 0,
    error_text      TEXT,
    queued_at       INTEGER NOT NULL,
    started_at      INTEGER,
    completed_at    INTEGER,
    FOREIGN KEY (workflow_id) REFERENCES symphony_workflows(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_symphony_runs_workflow ON symphony_runs(workflow_id, queued_at DESC);
CREATE INDEX IF NOT EXISTS idx_symphony_runs_status ON symphony_runs(status);

CREATE TABLE IF NOT EXISTS symphony_node_runs (
    id              TEXT PRIMARY KEY,
    run_id          TEXT NOT NULL,
    node_id         TEXT NOT NULL,
    attempt         INTEGER NOT NULL DEFAULT 1,
    status          TEXT NOT NULL,
    session_id      TEXT,
    cost_usd        REAL NOT NULL DEFAULT 0,
    iterations      INTEGER NOT NULL DEFAULT 0,
    started_at_ms   INTEGER,
    last_heartbeat_ms INTEGER,
    completed_at_ms INTEGER,
    error_text      TEXT,
    output_json     TEXT,
    FOREIGN KEY (run_id) REFERENCES symphony_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_symphony_node_runs_run ON symphony_node_runs(run_id, node_id);
CREATE INDEX IF NOT EXISTS idx_symphony_node_runs_status ON symphony_node_runs(status);
CREATE INDEX IF NOT EXISTS idx_symphony_node_runs_heartbeat ON symphony_node_runs(last_heartbeat_ms);

INSERT OR IGNORE INTO spaces (id, name, icon, path, created_at, updated_at)
VALUES ('symphonies', 'Symphonies', '🎼', NULL, datetime('now'), datetime('now'));
";

// V34: plan_suggest_events — telemetry for plan-mode auto-suggest.
// Each row is one "we showed the banner" event with its eventual outcome.
const SQL_V34_PLAN_SUGGEST_EVENTS: &str = "
CREATE TABLE IF NOT EXISTS plan_suggest_events (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    message_id      TEXT NOT NULL,
    source          TEXT NOT NULL,
    matched_pattern TEXT,
    reason          TEXT,
    user_msg_preview TEXT NOT NULL,
    outcome         TEXT NOT NULL DEFAULT 'pending',
    decline_reason  TEXT,
    fired_at        INTEGER NOT NULL,
    decided_at      INTEGER,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_plan_suggest_session ON plan_suggest_events(session_id);
CREATE INDEX IF NOT EXISTS idx_plan_suggest_pattern ON plan_suggest_events(matched_pattern)
    WHERE matched_pattern IS NOT NULL;
CREATE TABLE IF NOT EXISTS mode_suggest_overrides (
    pattern         TEXT PRIMARY KEY,
    disabled_until  INTEGER NOT NULL,
    reason          TEXT,
    updated_at      INTEGER NOT NULL
);
";

/// V35 — Memory OS Foundation Phase 1.
///
/// (Originally authored as V34; renumbered to V35 during rebase since PR
/// #185 landed V34 (plan_suggest_events) first. Sandbox's tip commit
/// `6871587` did this rename; we foreshadow it here to keep the rebase
/// cascade clean.)
///
/// Adds three additive tables backing the Memory OS Foundation layer
/// (`docs/superpowers/specs/2026-05-18-agent-memory-os-design.md` §4.2.4):
///
/// - `memory_edge_audit`        — records whether each memory_edges row was
///   created by the auto-link post-hook (Phase 2) or an explicit user/agent
///   action. Enables stale-link reconciliation without ever touching edges
///   that humans manually asserted.
/// - `wiki_artifacts`           — derived AI Wiki content (overview.md,
///   index.md, and later: hot/purpose/log/synthesis kinds). One row per
///   `(space_id, kind)` snapshot; LLM-regenerated by `wiki_overview`
///   scenario (Phase 3) on accumulation threshold.
/// - `memory_health_findings`   — issues surfaced by the Health (Phase 4)
///   and Lint (Phase 5) scenarios. `is_lint=0` for free-tier zero-LLM
///   checks; `is_lint=1` for paid checks with LLM cost.
///
/// All three are pure additive: no ALTER TABLE on existing schemas, no
/// dependency on tables outside V1-V34. Cascading deletes via FK on
/// `memory_edge_audit.edge_id` mirror the existing memory_edges → other
/// memory_* cascade pattern.
pub const V35_MEMORY_OS_PHASE_1: &str = "
CREATE TABLE IF NOT EXISTS memory_edge_audit (
    edge_id     TEXT PRIMARY KEY REFERENCES memory_edges(id) ON DELETE CASCADE,
    source      TEXT NOT NULL,
    inferred_by TEXT,
    confidence  REAL,
    extracted_from_version_id TEXT REFERENCES memory_versions(id),
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memory_edge_audit_src ON memory_edge_audit(source);

CREATE TABLE IF NOT EXISTS wiki_artifacts (
    id              TEXT PRIMARY KEY,
    space_id        TEXT NOT NULL,
    kind            TEXT NOT NULL,
    content         TEXT NOT NULL,
    generated_at    INTEGER NOT NULL,
    source_node_ids TEXT NOT NULL,
    llm_model       TEXT,
    token_cost      INTEGER
);
CREATE INDEX IF NOT EXISTS idx_wiki_artifacts_space_kind ON wiki_artifacts(space_id, kind);
CREATE INDEX IF NOT EXISTS idx_wiki_artifacts_generated ON wiki_artifacts(generated_at);

CREATE TABLE IF NOT EXISTS memory_health_findings (
    id            TEXT PRIMARY KEY,
    space_id      TEXT NOT NULL,
    severity      TEXT NOT NULL,
    check_kind    TEXT NOT NULL,
    subject       TEXT NOT NULL,
    payload_json  TEXT,
    is_lint       INTEGER NOT NULL DEFAULT 0,
    dismissed     INTEGER NOT NULL DEFAULT 0,
    discovered_at INTEGER NOT NULL,
    dismissed_at  INTEGER
);
CREATE INDEX IF NOT EXISTS idx_health_findings_active
    ON memory_health_findings(space_id, dismissed, discovered_at);
CREATE INDEX IF NOT EXISTS idx_health_findings_subject
    ON memory_health_findings(subject);
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
    // V18: agent_sessions.pinned_at — canonical pin state for the agent UI.
    tracing::debug!("Running migration V18: agent_sessions.pinned_at");
    for stmt in V18_AGENT_SESSIONS_PINNED_AT.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V18 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V19: per-workspace skill_tags JSON column for the manifest filter.
    tracing::debug!("Running migration V19: spaces.skill_tags");
    for stmt in V19_SPACES_SKILL_TAGS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V19 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V20: rewrite automation_specs + automation_activities to Humane schema.
    // Wrapped in its own transaction inside run_v20 — failure here is fatal
    // (unlike the ALTER-only migrations above) because V20 replaces the tables
    // entirely and partial execution would leave an inconsistent schema.
    //
    // info! (not debug!) so the line is visible at the default WARN+ tracing
    // filter — silent V20 failures are a documented pain point.
    tracing::info!("Running migration V20: Humane automation schema rewrite");
    if let Err(e) = run_v20(conn) {
        tracing::error!(error = %e, "V20 FAILED — Humane automation features will not work");
        return Err(e);
    }
    // V21: three Humane behavior tables that FK into the V20 schema.
    tracing::info!("Running migration V21: automation_subscriptions, automation_memory, automation_escalations");
    if let Err(e) = run_v21(conn) {
        tracing::error!(error = %e, "V21 FAILED — automation escalations / subscriptions / memory tables missing");
        return Err(e);
    }
    // V22: automation_installed_skills — tracks bundled skills per automation.
    tracing::debug!("Running migration V22: automation_installed_skills");
    for stmt in SQL_V22.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V22 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V23a: Marketplace cache (Phase 3a). Phase 3b extends to add the
    // automation_registries table for multi-source support.
    tracing::info!("Running migration V23a: marketplace cache (items + FTS + sync state)");
    if let Err(e) = run_v23a(conn) {
        tracing::error!(error = %e, "V23a FAILED — marketplace cache unavailable");
        return Err(e);
    }
    // V24: automation run = agent_session. Statement-split tolerant style —
    // ADD/DROP COLUMN are not transactional-schema-replacing, so partial
    // application is fine and a re-run's "duplicate/no such column" is benign.
    tracing::debug!("Running migration V24: automation run-session columns");
    for stmt in V24_AUTOMATION_RUN_SESSIONS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V24 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V25: marketplace_standalone_installs — tracks standalone skill/MCP installs.
    tracing::debug!("Running migration V25: marketplace_standalone_installs");
    for stmt in SQL_V25.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V25 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V26: conversations.archived + archived_at
    tracing::debug!("Running migration V26: conversations archived columns");
    for stmt in SQL_V26.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V26 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V27: system_prompts table for user-customizable system prompts.
    tracing::debug!("Running migration V27: system_prompts");
    for stmt in SQL_V27.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V27 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V28: system_prompt_versions table for prompt version history.
    tracing::debug!("Running migration V28: system_prompt_versions");
    for stmt in SQL_V28.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V28 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V29: compaction support — logical marking (compacted column + compaction_markers table).
    // Replaces the old physical-deletion approach so compacted messages stay in the DB
    // and can be replayed in the UI with "compacted" visual treatment.
    tracing::debug!("Running migration V29: compaction support (logical marking)");
    for stmt in V29_COMPACTION_SUPPORT.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V29 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V30: fragment_reviews + daily_summaries for the memory fragment system.
    tracing::debug!("Running migration V30: fragment tables");
    for stmt in V30_FRAGMENT_TABLES.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V30 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V31: rebuild memory_fts with trigram tokenizer for CJK + substring search.
    // Like V11 for messages_fts, drops the old unicode61 table and recreates
    // with trigram, then backfills from memory_nodes + active memory_versions.
    tracing::info!("Running migration V31: memory_fts trigram tokenizer");
    for stmt in V31_MEMORY_FTS_TRIGRAM.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V31 stmt skipped: {} :: {}", e, stmt);
        }
    }
    if let Err(e) = conn.execute(V31_BACKFILL_MEMORY_FTS, []) {
        tracing::warn!("V31 memory_fts backfill failed: {}", e);
    }
    // V32: IM channel infrastructure (im_channel_instances, im_sessions, spec_channel_bindings).
    tracing::debug!("Running migration V32: IM channel tables");
    for stmt in SQL_V32.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute_batch(stmt) {
            tracing::warn!("V32 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V32b: automation_specs additional columns (ALTER TABLE — idempotent, ignore if column exists).
    tracing::debug!("Running migration V32b: automation_specs IM columns");
    for stmt in SQL_V32B.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute_batch(stmt) {
            tracing::warn!("V32b stmt skipped (likely already exists): {} :: {}", e, stmt);
        }
    }
    // V33: Symphony runtime schema (workflows + versions + runs + node-runs + seed space).
    tracing::debug!("Running migration V33: Symphony tables");
    for stmt in SQL_V33_SYMPHONY.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V33 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V34: plan_suggest_events
    tracing::debug!("Running migration V34: plan_suggest_events");
    for stmt in SQL_V34_PLAN_SUGGEST_EVENTS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V34 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V35: Memory OS Foundation Phase 1 — edge audit + wiki artifacts + health findings.
    tracing::debug!("Running migration V35: Memory OS Phase 1");
    for stmt in V35_MEMORY_OS_PHASE_1.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V35 stmt skipped: {} :: {}", e, stmt);
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

    /// V18 adds a nullable pinned_at column to agent_sessions. Verifies:
    /// (1) column exists after migration, (2) existing rows default to NULL,
    /// (3) re-running the migration is idempotent.
    #[test]
    fn v18_adds_pinned_at_column_nullable_with_null_default() {
        let conn = db_pre_v16();
        // Pre-V18: insert a session row to make sure backfill is non-destructive.
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, metadata_json,
                                          message_count, pinned, archived,
                                          created_at, updated_at)
             VALUES ('s1', 'default', 't', '{}', 0, 0, 0, 0, 0)",
            [],
        ).unwrap();

        // Drive V18.
        for stmt in super::V18_AGENT_SESSIONS_PINNED_AT
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            // Match the run() per-stmt skip pattern.
            let _ = conn.execute(stmt, []);
        }

        // Column exists + pre-existing row has NULL.
        let pinned_at: Option<i64> = conn.query_row(
            "SELECT pinned_at FROM agent_sessions WHERE id = 's1'",
            [],
            |row| row.get::<_, Option<i64>>(0),
        ).unwrap();
        assert!(pinned_at.is_none());

        // Idempotent re-run (duplicate column ALTER fails per-stmt; ignored).
        for stmt in super::V18_AGENT_SESSIONS_PINNED_AT
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            let _ = conn.execute(stmt, []);
        }
        // Row still present, still NULL.
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_sessions",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 1);
    }

    /// V19 adds spaces.skill_tags as a TEXT NOT NULL column with default
    /// '[]'. Existing rows must get the default; new rows can override.
    /// Re-running V19 must be safe (duplicate-column ALTER skipped per
    /// run()'s per-stmt error-tolerance idiom).
    #[test]
    fn v19_adds_skill_tags_column_with_empty_array_default() {
        let conn = db_pre_v16();
        // Pre-V19: insert a workspace row to test that existing rows
        // get the default value.
        conn.execute(
            "INSERT INTO spaces (id, name, icon)
             VALUES ('engineering', 'Engineering', '📁')",
            [],
        ).unwrap();

        // Drive V19.
        for stmt in super::V19_SPACES_SKILL_TAGS
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            let _ = conn.execute(stmt, []);
        }

        // Existing row got the default.
        let tags: String = conn.query_row(
            "SELECT skill_tags FROM spaces WHERE id = 'engineering'",
            [],
            |row| row.get::<_, String>(0),
        ).unwrap();
        assert_eq!(tags, "[]",
            "existing spaces rows must get skill_tags='[]' default");

        // New rows can override.
        conn.execute(
            "INSERT INTO spaces (id, name, icon, skill_tags)
             VALUES ('research', 'Research', '📁', '[\"research\",\"academic\"]')",
            [],
        ).unwrap();
        let research_tags: String = conn.query_row(
            "SELECT skill_tags FROM spaces WHERE id = 'research'",
            [],
            |row| row.get::<_, String>(0),
        ).unwrap();
        assert_eq!(research_tags, "[\"research\",\"academic\"]");

        // Idempotent re-run.
        for stmt in super::V19_SPACES_SKILL_TAGS
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            let _ = conn.execute(stmt, []);
        }
        // Data still intact after re-run.
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM spaces",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 2);
    }

    // -----------------------------------------------------------------------
    // V20 helpers
    // -----------------------------------------------------------------------

    /// Apply the minimal set of migrations needed before V20 can run:
    /// V1 (spaces), V7 (automation_specs + automation_activities).
    /// We skip the intermediate migrations because none of them touch the
    /// tables that V20 operates on — this keeps the helper fast.
    fn db_pre_v20() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(super::V1_INITIAL).unwrap();
        conn.execute_batch(super::V7_AUTOMATIONS).unwrap();
        conn
    }

    /// V20 migrates a legacy TOML spec row into the Humane schema, setting
    /// source='toml-migrated' and populating spec_yaml / spec_json.
    #[test]
    fn v20_migrates_legacy_toml_specs() {
        let conn = db_pre_v20();

        // Seed a legacy row using the real legacy TOML shape.
        conn.execute(
            "INSERT INTO automation_specs (id, name, description, toml_content, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                "s1", "Test", "desc",
                "name = \"Test\"\ndescription = \"desc\"\ntask = \"Do thing\"\n[trigger]\nkind = \"manual\"\n",
                1i64, 1i64, 1i64,
            ],
        ).unwrap();

        super::run_v20(&conn).unwrap();

        // spec_yaml must contain Humane YAML markers.
        let yaml: String = conn.query_row(
            "SELECT spec_yaml FROM automation_specs WHERE id = 's1'", [], |r| r.get(0)
        ).unwrap();
        assert!(yaml.contains("type: automation"), "expected 'type: automation' in yaml: {}", yaml);
        assert!(yaml.contains("system_prompt: Do thing"), "expected 'system_prompt: Do thing' in yaml: {}", yaml);

        // source must be 'toml-migrated'.
        let source: String = conn.query_row(
            "SELECT source FROM automation_specs WHERE id = 's1'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(source, "toml-migrated");

        // spec_json must be valid JSON and non-empty.
        let spec_json: String = conn.query_row(
            "SELECT spec_json FROM automation_specs WHERE id = 's1'", [], |r| r.get(0)
        ).unwrap();
        let v: serde_json::Value = serde_json::from_str(&spec_json)
            .expect("spec_json must be valid JSON");
        assert!(v.get("name").is_some(), "spec_json must have 'name' key");

        // Identity columns must be populated from the migrated spec.
        let name: String = conn.query_row(
            "SELECT name FROM automation_specs WHERE id = 's1'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(name, "Test");

        let status: String = conn.query_row(
            "SELECT status FROM automation_specs WHERE id = 's1'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(status, "active");
    }

    /// V20 on an empty legacy table should succeed and produce new tables
    /// with the correct schema columns.
    #[test]
    fn v20_is_idempotent_on_empty_legacy() {
        let conn = db_pre_v20();
        super::run_v20(&conn).unwrap(); // no legacy data — must succeed

        // Verify new schema columns are present via pragma_table_info.
        let columns: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('automation_specs')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        for expected in &["spec_yaml", "spec_json", "source_version", "permissions_granted"] {
            assert!(
                columns.contains(&expected.to_string()),
                "missing column '{}' in automation_specs; columns present: {:?}",
                expected,
                columns
            );
        }

        // Verify automation_activities new columns too.
        let act_columns: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('automation_activities')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        for expected in &["trigger_source_type", "trigger_payload_json", "llm_iterations", "resumed_from_activity_id"] {
            assert!(
                act_columns.contains(&expected.to_string()),
                "missing column '{}' in automation_activities; columns present: {:?}",
                expected,
                act_columns
            );
        }
    }

    /// V20 correctly maps legacy automation_activities rows, preserving
    /// trigger_source_type and report_outcome heuristics.
    #[test]
    fn v20_migrates_legacy_activities() {
        let conn = db_pre_v20();

        // Seed a spec (required for FK).
        conn.execute(
            "INSERT INTO automation_specs (id, name, description, toml_content, enabled, created_at, updated_at)
             VALUES ('sp1', 'Spec', '', 'name = \"Spec\"\ntask = \"t\"\n[trigger]\nkind = \"manual\"\n', 1, 0, 0)",
            [],
        ).unwrap();

        // Seed a completed activity and a failed one.
        conn.execute(
            "INSERT INTO automation_activities (id, spec_id, run_id, trigger, status, result, error, duration_ms, created_at)
             VALUES ('a1', 'sp1', 'r1', 'cron', 'Completed', 'Done', NULL, 500, 42)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO automation_activities (id, spec_id, run_id, trigger, status, result, error, duration_ms, created_at)
             VALUES ('a2', 'sp1', 'r2', 'manual', 'failed', NULL, 'oops', 100, 99)",
            [],
        ).unwrap();

        super::run_v20(&conn).unwrap();

        // Verify completed activity mapping.
        let (trigger_type, status, outcome, queued_at): (String, String, Option<String>, i64) =
            conn.query_row(
                "SELECT trigger_source_type, status, report_outcome, queued_at
                 FROM automation_activities WHERE id = 'a1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            ).unwrap();
        assert_eq!(trigger_type, "cron");
        assert_eq!(status, "completed");
        assert_eq!(outcome.as_deref(), Some("useful"), "completed activity should have report_outcome='useful'");
        assert_eq!(queued_at, 42, "queued_at should map from legacy created_at");

        // Verify failed activity mapping.
        let (trigger_type2, status2, outcome2): (String, String, Option<String>) =
            conn.query_row(
                "SELECT trigger_source_type, status, report_outcome
                 FROM automation_activities WHERE id = 'a2'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            ).unwrap();
        assert_eq!(trigger_type2, "manual");
        assert_eq!(status2, "failed");
        assert!(outcome2.is_none(), "failed activity should have NULL report_outcome");
    }

    /// V21 creates automation_subscriptions, automation_memory, and
    /// automation_escalations after V20 has established the parent tables.
    #[test]
    fn v21_creates_three_behavior_tables() {
        let conn = Connection::open_in_memory().unwrap();
        // Stand up the minimal schema V21 depends on: V1 (spaces/agent_sessions
        // foundation) + V7 (legacy automation tables V20 needs to migrate from).
        conn.execute_batch(super::V1_INITIAL).unwrap();
        conn.execute_batch(super::V7_AUTOMATIONS).unwrap();
        // Apply V20 to produce the Humane-shaped parent tables.
        super::run_v20(&conn).unwrap();
        // Apply V21 under test.
        super::run_v21(&conn).unwrap();

        for table in [
            "automation_subscriptions",
            "automation_memory",
            "automation_escalations",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "table {} missing after V21", table);
        }
    }

    /// V20 produces a status='error' stub for a spec whose TOML content is
    /// unparseable, but does NOT abort the migration for other rows.
    #[test]
    fn v20_handles_bad_toml_with_error_stub() {
        let conn = db_pre_v20();

        // One valid spec and one broken spec.
        conn.execute(
            "INSERT INTO automation_specs (id, name, description, toml_content, enabled, created_at, updated_at)
             VALUES ('good', 'Good', '', 'name = \"Good\"\ntask = \"t\"\n[trigger]\nkind = \"manual\"\n', 1, 0, 0)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO automation_specs (id, name, description, toml_content, enabled, created_at, updated_at)
             VALUES ('bad', 'Bad', '', 'not valid toml [[[', 1, 0, 0)",
            [],
        ).unwrap();

        super::run_v20(&conn).unwrap();

        // Both rows should appear in the new table.
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM automation_specs", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 2, "both rows (good + error stub) must be present after V20");

        // Good spec is 'active', bad spec is 'error'.
        let good_status: String = conn.query_row(
            "SELECT status FROM automation_specs WHERE id = 'good'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(good_status, "active");

        let bad_status: String = conn.query_row(
            "SELECT status FROM automation_specs WHERE id = 'bad'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(bad_status, "error");
    }

    #[test]
    fn v23a_creates_marketplace_cache_tables() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        // Tables exist
        for tbl in ["automation_marketplace_items", "automation_marketplace_fts", "automation_registry_sync"] {
            let count: i64 = conn
                .query_row(
                    &format!("SELECT count(*) FROM sqlite_master WHERE type IN ('table','virtual table') AND name = '{}'", tbl),
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            assert!(count >= 1, "{} should exist after V23a", tbl);
        }
        // FTS5 trigram tokenizer works
        conn.execute("INSERT INTO automation_marketplace_fts(slug, registry_id, name, description, author, tags) VALUES('s', 'halo', 'AI News', 'curates news', 'a', 'ai,news')", []).unwrap();
        let hits: i64 = conn.query_row(
            "SELECT count(*) FROM automation_marketplace_fts WHERE automation_marketplace_fts MATCH 'news'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(hits, 1, "FTS5 trigram should match");
    }

    #[test]
    fn v22_creates_automation_installed_skills_table() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).expect("migrations run");

        // Inserting a row should succeed.
        conn.execute(
            "INSERT INTO automation_installed_skills \
                (automation_slug, skill_id, installed_at, file_count) \
                VALUES (?, ?, ?, ?)",
            rusqlite::params!["xhs-monitor", "xhs-search", 1715000000_i64, 2_i64],
        )
        .expect("insert ok");

        // PK collision should error.
        let dup = conn.execute(
            "INSERT INTO automation_installed_skills \
                (automation_slug, skill_id, installed_at, file_count) \
                VALUES (?, ?, ?, ?)",
            rusqlite::params!["xhs-monitor", "xhs-search", 1715000000_i64, 2_i64],
        );
        assert!(dup.is_err(), "PK should reject duplicate");

        // The companion index must exist.
        let idx_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='index' AND name='idx_aut_inst_skills_slug'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx_count, 1);
    }

    #[test]
    fn v22_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error (CREATE IF NOT EXISTS)");
    }

    #[test]
    fn v24_adds_session_columns_and_drops_tool_calls_json() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        let has_session_id: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('automation_activities') WHERE name = 'session_id'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_session_id, 1, "session_id column missing");

        let has_artifacts: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('automation_activities') WHERE name = 'report_artifacts_json'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_artifacts, 1, "report_artifacts_json column missing");

        let has_tool_calls: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('automation_activities') WHERE name = 'tool_calls_json'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_tool_calls, 0, "tool_calls_json should have been dropped");

        let has_archived_at: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('agent_sessions') WHERE name = 'archived_at'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_archived_at, 1, "agent_sessions.archived_at column missing");

        let has_idx: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_act_session'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_idx, 1, "idx_act_session missing");
    }

    #[test]
    fn v24_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        super::run(&conn).unwrap();
    }

    #[test]
    fn v25_creates_marketplace_standalone_installs_table() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).expect("migrations run");

        conn.execute(
            "INSERT INTO marketplace_standalone_installs \
                (slug, item_type, version, installed_at, mcp_server_id) \
                VALUES (?, ?, ?, ?, ?)",
            rusqlite::params!["my-skill", "skill", "1.0.0", 1715000000_i64, Option::<String>::None],
        ).expect("skill row insert ok");

        conn.execute(
            "INSERT INTO marketplace_standalone_installs \
                (slug, item_type, version, installed_at, mcp_server_id) \
                VALUES (?, ?, ?, ?, ?)",
            rusqlite::params!["my-mcp", "mcp", "2.0.0", 1715000000_i64, Some("srv-uuid-123")],
        ).expect("mcp row insert ok");

        // slug is PK — duplicate must error.
        let dup = conn.execute(
            "INSERT INTO marketplace_standalone_installs \
                (slug, item_type, version, installed_at, mcp_server_id) \
                VALUES (?, ?, ?, ?, ?)",
            rusqlite::params!["my-skill", "skill", "1.0.1", 1715000001_i64, Option::<String>::None],
        );
        assert!(dup.is_err(), "slug PK must reject duplicate");
    }

    #[test]
    fn v25_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error");
    }

    #[test]
    fn v26_conversations_archived_columns_exist() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        let archived: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('conversations') WHERE name = 'archived'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(archived, 1, "conversations.archived column missing");

        let archived_at: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('conversations') WHERE name = 'archived_at'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(archived_at, 1, "conversations.archived_at column missing");
    }

    #[test]
    fn v32_im_tables_created() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        // im_channel_instances table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='im_channel_instances'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "im_channel_instances table must exist after V32");

        // im_sessions table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='im_sessions'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "im_sessions table must exist after V32");

        // spec_channel_bindings table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='spec_channel_bindings'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "spec_channel_bindings table must exist after V32");

        // automation_specs gains trigger_phrase column
        conn.execute(
            "INSERT INTO automation_specs (id, name, version, author, description, system_prompt, \
             spec_yaml, spec_json, trigger_phrase, created_at, updated_at) \
             VALUES ('t1','n','1','a','d','s','y','{}', '/test', 1, 1)",
            [],
        )
        .unwrap();
    }

    // ─── V33: Symphony schema ─────────────────────────────────────────────

    #[test]
    fn v33_creates_symphony_tables_and_indexes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        for table in [
            "symphony_workflows",
            "symphony_workflow_versions",
            "symphony_runs",
            "symphony_node_runs",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {} must exist after V33", table);
        }

        for index in [
            "idx_symphony_runs_workflow",
            "idx_symphony_runs_status",
            "idx_symphony_node_runs_run",
            "idx_symphony_node_runs_status",
            "idx_symphony_node_runs_heartbeat",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    [index],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "index {} must exist after V33", index);
        }

        // Seeded 'symphonies' home space.
        let seeded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM spaces WHERE id = 'symphonies'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(seeded, 1, "'symphonies' home space must be seeded");
    }

    #[test]
    fn v33_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error");

        // Seed is INSERT OR IGNORE — second run mustn't duplicate.
        let seeded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM spaces WHERE id = 'symphonies'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(seeded, 1, "'symphonies' must not duplicate on re-run");
    }

    #[test]
    fn v33_run_fk_cascades_to_node_runs_on_workflow_delete() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        // Enable FK enforcement (off by default in rusqlite in-memory).
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        let now = 1_700_000_000_000_i64;
        conn.execute(
            "INSERT INTO symphony_workflows \
             (id, name, current_version, enabled, created_at, updated_at) \
             VALUES ('wf-1', 'demo', 1, 1, ?1, ?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symphony_workflow_versions \
             (workflow_id, version, definition_yaml, definition_md, nodes_json, edges_json, created_at) \
             VALUES ('wf-1', 1, 'kind: symphony', '---\\nkind: symphony\\n---', '[]', '[]', ?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symphony_runs \
             (id, workflow_id, workflow_version, trigger_kind, status, queued_at) \
             VALUES ('run-1', 'wf-1', 1, 'manual', 'running', ?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symphony_node_runs \
             (id, run_id, node_id, attempt, status) \
             VALUES ('nr-1', 'run-1', 'a', 1, 'ready')",
            [],
        )
        .unwrap();

        // Sanity: rows present.
        let n_runs_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM symphony_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_runs_before, 1);
        let n_node_runs_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM symphony_node_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_node_runs_before, 1);

        // Delete the workflow → cascade through versions → runs → node_runs.
        conn.execute("DELETE FROM symphony_workflows WHERE id = 'wf-1'", [])
            .unwrap();

        let n_versions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symphony_workflow_versions",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n_versions, 0, "versions must cascade on workflow delete");
        let n_runs: i64 = conn
            .query_row("SELECT COUNT(*) FROM symphony_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_runs, 0, "runs must cascade on workflow delete");
        let n_node_runs: i64 = conn
            .query_row("SELECT COUNT(*) FROM symphony_node_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_node_runs, 0, "node_runs must cascade through runs");
    }

    // ─── V34: Memory OS Foundation Phase 1 ────────────────────────────────

    #[test]
    fn v35_creates_memory_os_phase_1_tables_and_indexes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        for table in [
            "memory_edge_audit",
            "wiki_artifacts",
            "memory_health_findings",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {} must exist after V34", table);
        }

        for index in [
            "idx_memory_edge_audit_src",
            "idx_wiki_artifacts_space_kind",
            "idx_wiki_artifacts_generated",
            "idx_health_findings_active",
            "idx_health_findings_subject",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    [index],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "index {} must exist after V34", index);
        }
    }

    #[test]
    fn v35_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error");
        // Should still have exactly one of each table; CREATE TABLE IF NOT
        // EXISTS guarantees no duplicate-create errors on re-apply.
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='wiki_artifacts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn v35_edge_audit_cascades_on_edge_delete() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        // memory_nodes / memory_edges already exist from V4.
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) VALUES \
             ('p1', 'default', 'entity_page', 'Parent'), \
             ('c1', 'default', 'entity_page', 'Child')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memory_edges \
             (id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority) \
             VALUES ('e1', 'default', 'p1', 'c1', 'relates_to', 'private', 0)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memory_edge_audit (edge_id, source, inferred_by, confidence, created_at) \
             VALUES ('e1', 'auto_link', 'heuristic', 0.6, 1)",
            [],
        ).unwrap();

        // Delete the edge — audit row must cascade away.
        conn.execute("DELETE FROM memory_edges WHERE id = 'e1'", []).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_edge_audit WHERE edge_id = 'e1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "audit row must cascade on edge delete");
    }

    #[test]
    fn v35_health_findings_dismissible_with_indexes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        let now = 1_700_000_000_000_i64;
        conn.execute(
            "INSERT INTO memory_health_findings \
             (id, space_id, severity, check_kind, subject, discovered_at) \
             VALUES ('f1', 'default', 'warn', 'orphan', 'n1', ?1), \
                    ('f2', 'default', 'error', 'phantom_slug', 'n2', ?1)",
            [now],
        ).unwrap();

        // Only-active filter should hit the index on (space_id, dismissed, discovered_at).
        let active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_health_findings \
                 WHERE space_id = 'default' AND dismissed = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active, 2);

        // Dismiss one and re-check.
        conn.execute(
            "UPDATE memory_health_findings SET dismissed = 1, dismissed_at = ?1 WHERE id = 'f1'",
            [now + 1000],
        ).unwrap();
        let active_after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_health_findings \
                 WHERE space_id = 'default' AND dismissed = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active_after, 1);
    }
}
