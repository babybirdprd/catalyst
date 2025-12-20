use super::db::CatalystDb;
use anyhow::{Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The machine-readable state of the project
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectState {
    /// Current phase of the project
    pub phase: String,
    /// Active agent working on the project
    pub active_agent: Option<String>,
    /// Last successful build timestamp
    pub last_build: Option<String>,
    /// List of pending tasks/tickets
    pub pending_tasks: Vec<String>,
    /// Known constraints formatted as strings
    pub constraints: Vec<String>,
    /// Arbitrary metadata store for agents
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ProjectState {
    /// Load project state from SQLite database
    pub fn load(db: &CatalystDb) -> Result<Self> {
        let conn = db.connection();
        let conn = conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let result: Option<String> = conn
            .query_row("SELECT data FROM project_state WHERE id = 1", [], |row| {
                row.get(0)
            })
            .ok();

        match result {
            Some(data) => Ok(serde_json::from_str(&data)?),
            None => Ok(ProjectState::default()),
        }
    }

    /// Save project state to SQLite database
    pub fn save(&self, db: &CatalystDb) -> Result<()> {
        let conn = db.connection();
        let conn = conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let data = serde_json::to_string(self)?;
        conn.execute(
            "INSERT OR REPLACE INTO project_state (id, data) VALUES (1, ?1)",
            params![data],
        )
        .context("Failed to save project state")?;

        Ok(())
    }

    /// Async load wrapper for compatibility
    pub async fn load_async(db: &CatalystDb) -> Result<Self> {
        Self::load(db)
    }

    /// Async save wrapper for compatibility
    pub async fn save_async(&self, db: &CatalystDb) -> Result<()> {
        self.save(db)
    }
}
