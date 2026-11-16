//! SQLite persistent memory for Ruby Assistant (F15).
//! Uses SQLite FTS5 for fast full-text keyword indexing and ranking.

use rusqlite::{params, Connection};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub trait MemoryStore: Send + Sync {
    fn add_memory(&self, text: &str) -> Result<i64, MemoryError>;
    fn search_memories(&self, query: &str, limit: usize) -> Result<Vec<String>, MemoryError>;
    fn get_recent_memories(&self, limit: usize) -> Result<Vec<String>, MemoryError>;
}

pub struct SqliteMemoryStore {
    conn: std::sync::Mutex<Connection>,
}

impl SqliteMemoryStore {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, MemoryError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                text TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(text, content='memories', content_rowid='id');
            CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                INSERT INTO memories_fts(rowid, text) VALUES (new.id, new.text);
            END;
            CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, text) VALUES('delete', old.id, old.text);
            END;",
        )?;
        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }
}

impl MemoryStore for SqliteMemoryStore {
    fn add_memory(&self, text: &str) -> Result<i64, MemoryError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT INTO memories (text) VALUES (?1)", params![text])?;
        Ok(conn.last_insert_rowid())
    }

    fn search_memories(&self, query: &str, limit: usize) -> Result<Vec<String>, MemoryError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT memories.text FROM memories_fts
             JOIN memories ON memories.id = memories_fts.rowid
             WHERE memories_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![query, limit as i64], |row| row.get(0))?;
        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    fn get_recent_memories(&self, limit: usize) -> Result<Vec<String>, MemoryError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT text FROM memories ORDER BY id DESC LIMIT ?1")?;
        let rows = stmt.query_map(params![limit as i64], |row| row.get(0))?;
        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }
}
