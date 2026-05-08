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
    tracing::info!("Database migrations complete");
    Ok(())
}

