//! # Unified Catalyst Database
//!
//! Single SQLite database for all Catalyst state persistence.
//! Consolidates JSON files and separate `.db` files into `.catalyst/catalyst.db`.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::skills::prompts;

/// Schema version for migrations
const SCHEMA_VERSION: i32 = 1;

/// Unified database manager for all Catalyst state
pub struct CatalystDb {
    conn: Arc<Mutex<Connection>>,
}

impl CatalystDb {
    /// Open or create the unified database at `.catalyst/catalyst.db`
    pub fn open() -> Result<Self> {
        Self::open_at(".catalyst/catalyst.db")
    }

    /// Open database at a specific path (useful for testing)
    pub fn open_at<P: AsRef<Path>>(path: P) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(path.as_ref()).context("Failed to open catalyst database")?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        db.run_migrations()?;

        Ok(db)
    }

    /// Get a shared connection for use by other modules
    pub fn connection(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }

    /// Run schema migrations
    fn run_migrations(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Create schema version table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)",
            [],
        )?;

        // Get current version
        let current_version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Run migrations incrementally
        if current_version < 1 {
            self.migrate_v1(&conn)?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                [1],
            )?;
        }

        Ok(())
    }

    /// Migration to version 1 - complete schema
    fn migrate_v1(&self, conn: &Connection) -> Result<()> {
        // Project state (single row with JSON)
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS project_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                data TEXT NOT NULL DEFAULT '{}'
            )
            "#,
            [],
        )?;

        // Codebase profile (single row with JSON)
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS codebase_profile (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                data TEXT NOT NULL DEFAULT '{}',
                scanned_at TEXT
            )
            "#,
            [],
        )?;

        // Context manifest (single row with JSON for file list)
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS context_manifest (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                files_json TEXT NOT NULL DEFAULT '[]'
            )
            "#,
            [],
        )?;

        // Ideas table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS ideas (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                source_file TEXT,
                tags_json TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL
            )
            "#,
            [],
        )?;

        // Features table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS features (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                stage TEXT NOT NULL DEFAULT 'idea',
                description TEXT,
                worktree_path TEXT,
                error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
            [],
        )?;

        // Snapshots table (with lineage columns)
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS snapshots (
                id TEXT PRIMARY KEY,
                stage TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                state TEXT NOT NULL,
                description TEXT,
                parent_id TEXT,
                is_rollback_point INTEGER NOT NULL DEFAULT 0
            )
            "#,
            [],
        )?;

        // Memories table (with audit columns)
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                text TEXT NOT NULL,
                source_type TEXT NOT NULL,
                source_data TEXT NOT NULL DEFAULT '',
                metadata_json TEXT NOT NULL DEFAULT '{}',
                tokens_used INTEGER,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
            [],
        )?;

        // Interactions table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS interactions (
                id TEXT PRIMARY KEY,
                thread_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                from_agent TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                options_json TEXT NOT NULL DEFAULT '[]',
                schema_json TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                resolved_at TEXT,
                response_json TEXT
            )
            "#,
            [],
        )?;

        // Prompt templates (agent system prompts with version control)
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS prompt_templates (
                slug TEXT PRIMARY KEY,
                version INTEGER NOT NULL DEFAULT 1,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
            [],
        )?;

        // Project documents (spec fragments, architecture, etc.)
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS project_documents (
                slug TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
            [],
        )?;

        // Create indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_ideas_created ON ideas(created_at)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_features_stage ON features(stage)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_snapshots_stage ON snapshots(stage)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_text ON memories(text)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_source_type ON memories(source_type)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_interactions_status ON interactions(status)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_prompts_slug ON prompt_templates(slug)",
            [],
        )?;

        tracing::info!(
            "CatalystDb initialized with schema version {}",
            SCHEMA_VERSION
        );

        Ok(())
    }

    // =========================================================================
    // Prompt Template Methods
    // =========================================================================

    /// Seed default prompts if the table is empty
    pub fn seed_prompts(&self) -> Result<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Check if already seeded
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM prompt_templates", [], |row| {
            row.get(0)
        })?;

        if count > 0 {
            tracing::debug!("Prompts already seeded ({} found)", count);
            return Ok(0);
        }

        // Insert defaults
        let defaults = prompts::all_defaults();
        let mut inserted = 0;

        for (slug, content) in defaults {
            conn.execute(
                "INSERT INTO prompt_templates (slug, version, content) VALUES (?1, 1, ?2)",
                params![slug, content],
            )?;
            inserted += 1;
        }

        tracing::info!("Seeded {} default prompts", inserted);
        Ok(inserted)
    }

    /// Get a prompt by slug
    pub fn get_prompt(&self, slug: &str) -> Result<String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        conn.query_row(
            "SELECT content FROM prompt_templates WHERE slug = ?1",
            params![slug],
            |row| row.get(0),
        )
        .with_context(|| format!("Prompt '{}' not found", slug))
    }

    /// Get a prompt with its version
    pub fn get_prompt_versioned(&self, slug: &str) -> Result<(String, i32)> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        conn.query_row(
            "SELECT content, version FROM prompt_templates WHERE slug = ?1",
            params![slug],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .with_context(|| format!("Prompt '{}' not found", slug))
    }

    /// Update a prompt (increments version automatically)
    pub fn set_prompt(&self, slug: &str, content: &str) -> Result<i32> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Get current version or start at 0
        let current_version: i32 = conn
            .query_row(
                "SELECT version FROM prompt_templates WHERE slug = ?1",
                params![slug],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let new_version = current_version + 1;

        conn.execute(
            r#"
            INSERT INTO prompt_templates (slug, version, content, updated_at)
            VALUES (?1, ?2, ?3, datetime('now'))
            ON CONFLICT(slug) DO UPDATE SET
                version = ?2,
                content = ?3,
                updated_at = datetime('now')
            "#,
            params![slug, new_version, content],
        )?;

        tracing::debug!("Updated prompt '{}' to version {}", slug, new_version);
        Ok(new_version)
    }

    /// List all prompt slugs
    pub fn list_prompts(&self) -> Result<Vec<(String, i32)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare("SELECT slug, version FROM prompt_templates ORDER BY slug")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;

        let mut prompts = Vec::new();
        for row in rows {
            prompts.push(row?);
        }
        Ok(prompts)
    }

    // =========================================================================
    // Project Document Methods
    // =========================================================================

    /// Get a document by slug
    pub fn get_document(&self, slug: &str) -> Result<(String, String)> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        conn.query_row(
            "SELECT title, content FROM project_documents WHERE slug = ?1",
            params![slug],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .with_context(|| format!("Document '{}' not found", slug))
    }

    /// Set a document (upsert)
    pub fn set_document(&self, slug: &str, title: &str, content: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        conn.execute(
            r#"
            INSERT INTO project_documents (slug, title, content, updated_at)
            VALUES (?1, ?2, ?3, datetime('now'))
            ON CONFLICT(slug) DO UPDATE SET
                title = ?2,
                content = ?3,
                updated_at = datetime('now')
            "#,
            params![slug, title, content],
        )?;

        Ok(())
    }

    /// List all document slugs
    pub fn list_documents(&self) -> Result<Vec<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare("SELECT slug FROM project_documents ORDER BY slug")?;
        let rows = stmt.query_map([], |row| row.get(0))?;

        let mut docs = Vec::new();
        for row in rows {
            docs.push(row?);
        }
        Ok(docs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_catalyst_db_open_creates_tables() {
        let path = ".catalyst/test_catalyst.db";
        let _ = fs::remove_file(path);

        let db = CatalystDb::open_at(path).unwrap();
        let conn = db.connection();
        let conn = conn.lock().unwrap();

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"project_state".to_string()));
        assert!(tables.contains(&"codebase_profile".to_string()));
        assert!(tables.contains(&"ideas".to_string()));
        assert!(tables.contains(&"features".to_string()));
        assert!(tables.contains(&"snapshots".to_string()));
        assert!(tables.contains(&"memories".to_string()));
        assert!(tables.contains(&"interactions".to_string()));
        assert!(tables.contains(&"prompt_templates".to_string()));
        assert!(tables.contains(&"project_documents".to_string()));

        drop(conn);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_schema_version_tracking() {
        let path = ".catalyst/test_catalyst_version.db";
        let _ = fs::remove_file(path);

        // Open twice - should not fail on second open
        let _db1 = CatalystDb::open_at(path).unwrap();
        drop(_db1);

        let db2 = CatalystDb::open_at(path).unwrap();
        let conn = db2.connection();
        let conn = conn.lock().unwrap();

        let version: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(version, SCHEMA_VERSION);

        drop(conn);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_prompt_seeding() {
        let path = ".catalyst/test_prompts.db";
        let _ = fs::remove_file(path);

        let db = CatalystDb::open_at(path).unwrap();

        // First seed should insert all defaults
        let count = db.seed_prompts().unwrap();
        assert!(count > 0, "Should seed default prompts");

        // Second seed should be no-op
        let count2 = db.seed_prompts().unwrap();
        assert_eq!(count2, 0, "Should not re-seed");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_prompt_crud() {
        let path = ".catalyst/test_prompt_crud.db";
        let _ = fs::remove_file(path);

        let db = CatalystDb::open_at(path).unwrap();
        db.seed_prompts().unwrap();

        // Read a prompt
        let content = db.get_prompt("architect").unwrap();
        assert!(
            content.to_lowercase().contains("architect"),
            "Should contain prompt content"
        );

        // Update a prompt (version should increment)
        let new_version = db
            .set_prompt("architect", "New architect prompt v2")
            .unwrap();
        assert_eq!(new_version, 2, "Version should increment");

        // Read updated
        let (content, version) = db.get_prompt_versioned("architect").unwrap();
        assert_eq!(content, "New architect prompt v2");
        assert_eq!(version, 2);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_document_crud() {
        let path = ".catalyst/test_docs.db";
        let _ = fs::remove_file(path);

        let db = CatalystDb::open_at(path).unwrap();

        // Create
        db.set_document("architecture", "Architecture", "# System Architecture")
            .unwrap();

        // Read
        let (title, content) = db.get_document("architecture").unwrap();
        assert_eq!(title, "Architecture");
        assert_eq!(content, "# System Architecture");

        // List
        let docs = db.list_documents().unwrap();
        assert!(docs.contains(&"architecture".to_string()));

        let _ = fs::remove_file(path);
    }
}
