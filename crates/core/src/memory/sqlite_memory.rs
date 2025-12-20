//! # SQLite Memory Service
//!
//! Persistent memory storage using SQLite for Catalyst knowledge.
//! Now uses shared connection from CatalystDb.
//!
//! ### TODO:
//! - [ ] Implement Full-Text Search (FTS5) for lexical matching.
//! - [ ] Integrate **Candle** google/embeddinggemma-300m (Quantized?) or **ONNX** framework with **https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX**.
//! - [ ] Add `sqlite-vec` extension for semantic vector search.
//! - [ ] Note: This may be moved to a new crate because of the Candle introduction.
//!
//! Implements a simple keyword-based search until upgrade.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

use crate::state::db::CatalystDb;

/// A memory entry stored in SQLite
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub id: i64,
    pub text: String,
    pub source_type: String,
    pub source_data: String,
    pub created_at: String,
}

/// SQLite-backed memory service using shared CatalystDb connection
pub struct SqliteMemoryService {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteMemoryService {
    /// Create from shared CatalystDb connection
    pub fn new(db: &CatalystDb) -> Self {
        Self {
            conn: db.connection(),
        }
    }

    /// Add a memory entry
    pub fn add(&self, text: &str, source_type: &str, source_data: &str) -> Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        conn.execute(
            "INSERT INTO memories (text, source_type, source_data) VALUES (?1, ?2, ?3)",
            params![text, source_type, source_data],
        )
        .context("Failed to insert memory")?;

        Ok(conn.last_insert_rowid())
    }

    /// Search memories by keyword (simple LIKE search)
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let search_pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            r#"
            SELECT id, text, source_type, source_data, created_at
            FROM memories
            WHERE text LIKE ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )?;

        let entries = stmt
            .query_map(params![search_pattern, limit as i64], |row| {
                Ok(MemoryEntry {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    source_type: row.get(2)?,
                    source_data: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to collect search results")?;

        Ok(entries)
    }

    /// Get all memories (for debugging/export)
    pub fn list_all(&self, limit: usize) -> Result<Vec<MemoryEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT id, text, source_type, source_data, created_at
            FROM memories
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )?;

        let entries = stmt
            .query_map(params![limit as i64], |row| {
                Ok(MemoryEntry {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    source_type: row.get(2)?,
                    source_data: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to list memories")?;

        Ok(entries)
    }

    /// Delete a memory by ID
    pub fn delete(&self, id: i64) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let affected = conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// Get count of memories
    pub fn count(&self) -> Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_sqlite_memory_with_catalyst_db() {
        let path = ".catalyst/test_catalyst_memory.db";
        let _ = fs::remove_file(path);

        let db = CatalystDb::open_at(path).unwrap();
        let service = SqliteMemoryService::new(&db);

        // Add some memories
        let id1 = service
            .add(
                "Rust is a systems programming language",
                "document",
                "rust_docs",
            )
            .unwrap();
        let id2 = service
            .add("Python is great for scripting", "document", "python_docs")
            .unwrap();
        let id3 = service
            .add("Rust has great memory safety", "fact", "user")
            .unwrap();

        assert!(id1 > 0);
        assert!(id2 > 0);
        assert!(id3 > 0);

        // Search for Rust
        let results = service.search("Rust", 10).unwrap();
        assert_eq!(results.len(), 2);

        // Search for Python
        let results = service.search("Python", 10).unwrap();
        assert_eq!(results.len(), 1);

        // Count
        assert_eq!(service.count().unwrap(), 3);

        // Delete
        assert!(service.delete(id2).unwrap());
        assert_eq!(service.count().unwrap(), 2);

        // Cleanup
        drop(db);
        let _ = fs::remove_file(path);
    }
}
