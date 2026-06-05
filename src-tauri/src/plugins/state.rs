//! Pi-3b — plugin enable/disable persistence (V59 `plugins` table).

use rusqlite::{params, Connection};
use std::collections::HashMap;

/// Load all plugin enabled-states. Missing/empty → empty map (a plugin with no
/// row is treated as enabled by callers — fail-open).
pub fn load_enabled_map(conn: &Connection) -> HashMap<String, bool> {
    let mut map = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, enabled FROM plugins") {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0))
        }) {
            for row in rows.flatten() {
                map.insert(row.0, row.1);
            }
        }
    }
    map
}

/// Insert a default-enabled row if absent (idempotent; never clobbers existing enabled).
pub fn ensure_plugin_row(conn: &Connection, id: &str, now_ms: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO plugins (id, enabled, updated_at) VALUES (?1, 1, ?2)
         ON CONFLICT(id) DO NOTHING",
        params![id, now_ms],
    )?;
    Ok(())
}

/// Record (or update) the install source for a plugin.
///
/// `source` is a tagged string: `"catalog:<slug>"`, `"git:<url>"`,
/// `"local:<path>"`, or `"agent"`.  Called after every install path so
/// `upgrade_plugin` can re-fetch from the same origin.
pub fn set_plugin_source(conn: &Connection, id: &str, source: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE plugins SET source = ?2 WHERE id = ?1",
        params![id, source],
    )?;
    Ok(())
}

/// Read back the remembered install source for a plugin.
///
/// Returns `None` when the row is absent or the source column is NULL
/// (e.g. plugins installed before V60).
pub fn get_plugin_source(conn: &Connection, id: &str) -> Option<String> {
    conn.query_row(
        "SELECT source FROM plugins WHERE id = ?1",
        params![id],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
}

// ─── V61 — per-plugin env DAO ─────────────────────────────────────────────────

/// Upsert a single env var for a plugin.
///
/// The value replaces any existing value for the same `(plugin_id, key)` pair.
/// Injected into `McpServerConfig.env` at boot so it reaches the MCP subprocess
/// after the sandbox's `env_clear()`.
pub fn set_plugin_env(
    conn: &Connection,
    plugin_id: &str,
    key: &str,
    value: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO plugin_env (plugin_id, key, value) VALUES (?1, ?2, ?3)
         ON CONFLICT(plugin_id, key) DO UPDATE SET value = ?3",
        params![plugin_id, key, value],
    )?;
    Ok(())
}

/// Return all env vars stored for `plugin_id` as a `key → value` map.
///
/// Returns an empty map (not an error) when the plugin has no rows.
pub fn get_plugin_env(conn: &Connection, plugin_id: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT key, value FROM plugin_env WHERE plugin_id = ?1")
    {
        if let Ok(rows) = stmt.query_map(params![plugin_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        }) {
            for row in rows.flatten() {
                map.insert(row.0, row.1);
            }
        }
    }
    map
}

/// Delete a single env var for a plugin.
///
/// Safe to call even if the key does not exist (changes == 0 is not an error).
pub fn delete_plugin_env(
    conn: &Connection,
    plugin_id: &str,
    key: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM plugin_env WHERE plugin_id = ?1 AND key = ?2",
        params![plugin_id, key],
    )?;
    Ok(())
}

/// Delete all env vars for a plugin (called by uninstall; best-effort).
///
/// Safe to call even if no rows exist.
pub fn delete_all_plugin_env(conn: &Connection, plugin_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM plugin_env WHERE plugin_id = ?1",
        params![plugin_id],
    )?;
    Ok(())
}

/// Delete a plugin's row from the `plugins` table (called by uninstall).
///
/// Safe to call even if the row does not exist (changes == 0 is not an error).
pub fn delete_plugin_row(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM plugins WHERE id = ?1", params![id])?;
    Ok(())
}

/// Set a plugin's enabled-state (upsert).
pub fn set_plugin_enabled(conn: &Connection, id: &str, enabled: bool, now_ms: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO plugins (id, enabled, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET enabled = ?2, updated_at = ?3",
        params![id, enabled as i64, now_ms],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&c).unwrap();
        c
    }
    #[test]
    fn ensure_defaults_enabled_and_does_not_clobber() {
        let c = conn();
        ensure_plugin_row(&c, "p", 1).unwrap();
        assert_eq!(load_enabled_map(&c).get("p"), Some(&true));
        set_plugin_enabled(&c, "p", false, 2).unwrap();
        ensure_plugin_row(&c, "p", 3).unwrap();
        assert_eq!(load_enabled_map(&c).get("p"), Some(&false));
    }
    #[test]
    fn set_toggles_and_load_reflects() {
        let c = conn();
        set_plugin_enabled(&c, "p", false, 1).unwrap();
        assert_eq!(load_enabled_map(&c).get("p"), Some(&false));
        set_plugin_enabled(&c, "p", true, 2).unwrap();
        assert_eq!(load_enabled_map(&c).get("p"), Some(&true));
    }

    // ── V60 source DAO ──────────────────────────────────────────────────────

    #[test]
    fn set_and_get_source_round_trips() {
        let c = conn();
        ensure_plugin_row(&c, "plug1", 1).unwrap();
        // source is NULL before being set
        assert_eq!(get_plugin_source(&c, "plug1"), None);
        set_plugin_source(&c, "plug1", "git:https://example.com/plugin").unwrap();
        assert_eq!(
            get_plugin_source(&c, "plug1").as_deref(),
            Some("git:https://example.com/plugin")
        );
        // overwrite with a different source
        set_plugin_source(&c, "plug1", "catalog:demo").unwrap();
        assert_eq!(
            get_plugin_source(&c, "plug1").as_deref(),
            Some("catalog:demo")
        );
    }

    #[test]
    fn get_source_returns_none_for_missing_row() {
        let c = conn();
        assert_eq!(get_plugin_source(&c, "no-such-id"), None);
    }

    #[test]
    fn delete_plugin_row_removes_and_is_idempotent() {
        let c = conn();
        ensure_plugin_row(&c, "plug2", 1).unwrap();
        set_plugin_source(&c, "plug2", "local:/tmp/demo").unwrap();
        // Row exists
        assert_eq!(load_enabled_map(&c).get("plug2"), Some(&true));
        delete_plugin_row(&c, "plug2").unwrap();
        // Row gone
        assert_eq!(load_enabled_map(&c).get("plug2"), None);
        assert_eq!(get_plugin_source(&c, "plug2"), None);
        // Second delete is a no-op (not an error)
        delete_plugin_row(&c, "plug2").unwrap();
    }

    #[test]
    fn source_preserved_after_enabled_toggle() {
        let c = conn();
        ensure_plugin_row(&c, "plug3", 1).unwrap();
        set_plugin_source(&c, "plug3", "catalog:my-plugin").unwrap();
        set_plugin_enabled(&c, "plug3", false, 2).unwrap();
        // source must survive a set_plugin_enabled upsert
        assert_eq!(
            get_plugin_source(&c, "plug3").as_deref(),
            Some("catalog:my-plugin")
        );
        assert_eq!(load_enabled_map(&c).get("plug3"), Some(&false));
    }

    // ── V61 plugin_env DAO ─────────────────────────────────────────────────────

    #[test]
    fn plugin_env_set_and_get_round_trips() {
        let c = conn();
        set_plugin_env(&c, "p1", "API_KEY", "secret123").unwrap();
        set_plugin_env(&c, "p1", "BASE_URL", "https://example.com").unwrap();
        let env = get_plugin_env(&c, "p1");
        assert_eq!(env.len(), 2);
        assert_eq!(env.get("API_KEY").map(|s| s.as_str()), Some("secret123"));
        assert_eq!(
            env.get("BASE_URL").map(|s| s.as_str()),
            Some("https://example.com")
        );
    }

    #[test]
    fn plugin_env_upsert_overwrites_value() {
        let c = conn();
        set_plugin_env(&c, "p2", "TOKEN", "old_value").unwrap();
        set_plugin_env(&c, "p2", "TOKEN", "new_value").unwrap();
        let env = get_plugin_env(&c, "p2");
        assert_eq!(env.len(), 1);
        assert_eq!(env.get("TOKEN").map(|s| s.as_str()), Some("new_value"));
    }

    #[test]
    fn plugin_env_delete_one_key() {
        let c = conn();
        set_plugin_env(&c, "p3", "KEY_A", "val_a").unwrap();
        set_plugin_env(&c, "p3", "KEY_B", "val_b").unwrap();
        delete_plugin_env(&c, "p3", "KEY_A").unwrap();
        let env = get_plugin_env(&c, "p3");
        assert_eq!(env.len(), 1);
        assert!(env.get("KEY_A").is_none());
        assert_eq!(env.get("KEY_B").map(|s| s.as_str()), Some("val_b"));
    }

    #[test]
    fn plugin_env_delete_all_clears() {
        let c = conn();
        set_plugin_env(&c, "p4", "K1", "v1").unwrap();
        set_plugin_env(&c, "p4", "K2", "v2").unwrap();
        delete_all_plugin_env(&c, "p4").unwrap();
        let env = get_plugin_env(&c, "p4");
        assert_eq!(env.len(), 0);
    }

    #[test]
    fn plugin_env_get_returns_empty_for_unknown_plugin() {
        let c = conn();
        let env = get_plugin_env(&c, "no-such-plugin");
        assert!(env.is_empty());
    }

    #[test]
    fn plugin_env_delete_is_idempotent() {
        let c = conn();
        // no rows yet — should not error
        delete_plugin_env(&c, "p5", "NONEXISTENT").unwrap();
        delete_all_plugin_env(&c, "p5").unwrap();
    }

    #[test]
    fn v61_plugin_env_table_exists_after_migration() {
        let c = conn();
        let count: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'plugin_env'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
