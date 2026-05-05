use rusqlite::Connection;
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, crate::error::Error> {
        let conn = Connection::open(path)?;

        // Enable WAL mode for better concurrent access
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", "5000")?;

        Ok(Self { conn })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn run_migrations(&self) -> Result<(), crate::error::Error> {
        self.conn.execute_batch(super::migrations::V1_INITIAL).map_err(|e| e.into())
    }

    pub fn into_inner(self) -> Connection {
        self.conn
    }
}
