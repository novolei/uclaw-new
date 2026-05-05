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
    tracing::info!("Database migrations complete");
    Ok(())
}

