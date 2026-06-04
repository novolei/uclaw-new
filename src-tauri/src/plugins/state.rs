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
}
