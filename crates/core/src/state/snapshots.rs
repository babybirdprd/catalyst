//! # State Snapshots
//!
//! Checkpoint management for pipeline state rollback and replay.
//! Snapshots are stored in SQLite rather than individual JSON files.

use super::db::CatalystDb;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// A snapshot of pipeline state at a specific stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unique snapshot ID
    pub id: String,
    /// Pipeline stage when snapshot was taken
    pub stage: String,
    /// Timestamp of snapshot
    pub timestamp: DateTime<Utc>,
    /// JSON state data
    pub state: serde_json::Value,
    /// Optional description/notes
    pub description: Option<String>,
    /// Parent snapshot ID (for rollback lineage)
    pub parent_id: Option<String>,
    /// Whether this snapshot is a rollback point
    pub is_rollback_point: bool,
}

impl Snapshot {
    /// Create a new snapshot
    pub fn new(stage: &str, state: serde_json::Value) -> Self {
        let timestamp = Utc::now();
        let id = format!(
            "{}_{}",
            stage.to_lowercase().replace(' ', "_"),
            timestamp.format("%Y%m%d_%H%M%S")
        );

        Self {
            id,
            stage: stage.to_string(),
            timestamp,
            state,
            description: None,
            parent_id: None,
            is_rollback_point: false,
        }
    }

    /// Add a description to the snapshot
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }
}

/// Snapshot manager for saving/loading checkpoints using SQLite
pub struct SnapshotManager {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl SnapshotManager {
    /// Create a new snapshot manager from a CatalystDb
    pub fn new(db: &CatalystDb) -> Self {
        Self {
            conn: db.connection(),
        }
    }

    /// Take a new snapshot
    pub fn take(&self, stage: &str, state: serde_json::Value) -> Result<Snapshot> {
        let snapshot = Snapshot::new(stage, state);

        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let state_json = serde_json::to_string(&snapshot.state)?;

        conn.execute(
            r#"
            INSERT INTO snapshots (id, stage, timestamp, state, description, parent_id, is_rollback_point)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                snapshot.id,
                snapshot.stage,
                snapshot.timestamp.to_rfc3339(),
                state_json,
                snapshot.description,
                snapshot.parent_id,
                snapshot.is_rollback_point as i32,
            ],
        )
        .context("Failed to save snapshot")?;

        tracing::info!(snapshot_id = %snapshot.id, stage = %stage, "Snapshot taken");

        Ok(snapshot)
    }

    /// Load a snapshot by ID
    pub fn load(&self, id: &str) -> Result<Snapshot> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let snapshot = conn
            .query_row(
                r#"
            SELECT id, stage, timestamp, state, description, parent_id, is_rollback_point
            FROM snapshots WHERE id = ?1
            "#,
                params![id],
                |row| Ok(Self::row_to_snapshot(row)?),
            )
            .context("Snapshot not found")?;

        Ok(snapshot)
    }

    /// Restore state from a snapshot, creating a rollback point
    pub fn restore(&self, snapshot_id: &str) -> Result<RollbackResult> {
        let snapshot = self.load(snapshot_id)?;

        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Begin transaction for atomicity
        conn.execute("BEGIN TRANSACTION", [])?;

        let result = (|| -> Result<()> {
            // Restore project_state if present in snapshot
            if let Some(project_state) = snapshot.state.get("project_state") {
                let data = serde_json::to_string(project_state)?;
                conn.execute(
                    "INSERT OR REPLACE INTO project_state (id, data) VALUES (1, ?1)",
                    params![data],
                )?;
                tracing::info!("Restored project_state from snapshot");
            }

            // Restore features if present in snapshot
            if let Some(features) = snapshot.state.get("features") {
                if let Some(features_arr) = features.as_array() {
                    // Clear existing features
                    conn.execute("DELETE FROM features", [])?;

                    // Restore each feature
                    for feature in features_arr {
                        let id = feature.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let title = feature.get("title").and_then(|v| v.as_str()).unwrap_or("");
                        let stage = feature
                            .get("stage")
                            .and_then(|v| v.as_str())
                            .unwrap_or("idea");
                        let description = feature.get("description").and_then(|v| v.as_str());
                        let worktree_path = feature.get("worktree_path").and_then(|v| v.as_str());
                        let error = feature.get("error").and_then(|v| v.as_str());
                        let created_at = feature
                            .get("created_at")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let updated_at = feature
                            .get("updated_at")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        conn.execute(
                            r#"
                            INSERT INTO features (id, title, stage, description, worktree_path, error, created_at, updated_at)
                            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                            "#,
                            params![id, title, stage, description, worktree_path, error, created_at, updated_at],
                        )?;
                    }
                    tracing::info!(
                        count = features_arr.len(),
                        "Restored features from snapshot"
                    );
                }
            }

            // Create rollback-point snapshot
            let rb_timestamp = Utc::now();
            let rb_id = format!("rollback_{}", rb_timestamp.format("%Y%m%d_%H%M%S"));
            let rb_state = serde_json::json!({
                "rolled_back_to": snapshot_id,
            });

            conn.execute(
                r#"
                INSERT INTO snapshots (id, stage, timestamp, state, description, parent_id, is_rollback_point)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    rb_id,
                    "Rollback",
                    rb_timestamp.to_rfc3339(),
                    serde_json::to_string(&rb_state)?,
                    format!("Rolled back to {}", snapshot_id),
                    snapshot_id,
                    1,
                ],
            )?;

            Ok(())
        })();

        match result {
            Ok(_) => {
                conn.execute("COMMIT", [])?;
                tracing::info!(snapshot_id = %snapshot_id, stage = %snapshot.stage, "State restored from snapshot");
                Ok(RollbackResult {
                    success: true,
                    snapshot_id: snapshot_id.to_string(),
                    stage: snapshot.stage,
                    message: "State successfully restored".to_string(),
                })
            }
            Err(e) => {
                conn.execute("ROLLBACK", []).ok();
                Err(e)
            }
        }
    }

    /// List all snapshots (sorted by timestamp, newest first)
    pub fn list(&self) -> Result<Vec<Snapshot>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT id, stage, timestamp, state, description, parent_id, is_rollback_point
            FROM snapshots
            ORDER BY timestamp DESC
            "#,
        )?;

        let snapshots = stmt
            .query_map([], |row| Ok(Self::row_to_snapshot(row)?))?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to list snapshots")?;

        Ok(snapshots)
    }

    /// Get snapshots for a specific stage
    pub fn list_by_stage(&self, stage: &str) -> Result<Vec<Snapshot>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT id, stage, timestamp, state, description, parent_id, is_rollback_point
            FROM snapshots
            WHERE stage = ?1
            ORDER BY timestamp DESC
            "#,
        )?;

        let snapshots = stmt
            .query_map(params![stage], |row| Ok(Self::row_to_snapshot(row)?))?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to list snapshots by stage")?;

        Ok(snapshots)
    }

    /// Delete a snapshot by ID
    pub fn delete(&self, id: &str) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let affected = conn.execute("DELETE FROM snapshots WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// Get the most recent snapshot
    pub fn latest(&self) -> Result<Option<Snapshot>> {
        let snapshots = self.list()?;
        Ok(snapshots.into_iter().next())
    }

    /// Get the most recent snapshot for a stage
    pub fn latest_for_stage(&self, stage: &str) -> Result<Option<Snapshot>> {
        let snapshots = self.list_by_stage(stage)?;
        Ok(snapshots.into_iter().next())
    }

    fn row_to_snapshot(row: &rusqlite::Row) -> rusqlite::Result<Snapshot> {
        let id: String = row.get(0)?;
        let stage: String = row.get(1)?;
        let timestamp_str: String = row.get(2)?;
        let state_json: String = row.get(3)?;
        let description: Option<String> = row.get(4)?;
        let parent_id: Option<String> = row.get(5)?;
        let is_rollback_point: i32 = row.get(6)?;

        Ok(Snapshot {
            id,
            stage,
            timestamp: DateTime::parse_from_rfc3339(&timestamp_str)
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            state: serde_json::from_str(&state_json).unwrap_or(serde_json::Value::Null),
            description,
            parent_id,
            is_rollback_point: is_rollback_point != 0,
        })
    }
}

/// Rollback result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackResult {
    pub success: bool,
    pub snapshot_id: String,
    pub stage: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_new() {
        let state = serde_json::json!({ "unknowns": ["UNK-001"] });
        let snap = Snapshot::new("UnknownsParsing", state.clone());

        assert!(snap.id.contains("unknownsparsing"));
        assert_eq!(snap.stage, "UnknownsParsing");
        assert_eq!(snap.state, state);
    }

    #[test]
    fn test_snapshot_with_description() {
        let state = serde_json::json!({});
        let snap = Snapshot::new("Test", state).with_description("Test snapshot");

        assert_eq!(snap.description, Some("Test snapshot".to_string()));
    }
}
