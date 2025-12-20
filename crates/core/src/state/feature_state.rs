//! # Feature State Management
//!
//! Feature storage using SQLite. Each feature is a row in the `features` table.

use super::db::CatalystDb;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Pipeline stage for a feature
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    /// Initial idea, not yet processed
    #[default]
    Idea,
    /// UnknownsParser is analyzing
    Parsing,
    /// Researcher is investigating
    Researching,
    /// Architect is designing
    Architecting,
    /// Builder is implementing
    Building,
    /// RedTeam is testing
    Testing,
    /// Merging back to main
    Merging,
    /// Successfully completed
    Complete,
    /// Failed with error
    Failed,
}

impl PipelineStage {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Idea => "idea",
            Self::Parsing => "parsing",
            Self::Researching => "researching",
            Self::Architecting => "architecting",
            Self::Building => "building",
            Self::Testing => "testing",
            Self::Merging => "merging",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "idea" => Self::Idea,
            "parsing" => Self::Parsing,
            "researching" => Self::Researching,
            "architecting" => Self::Architecting,
            "building" => Self::Building,
            "testing" => Self::Testing,
            "merging" => Self::Merging,
            "complete" => Self::Complete,
            "failed" => Self::Failed,
            _ => Self::Idea,
        }
    }
}

/// A feature being developed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    /// Unique feature identifier
    pub id: String,
    /// Human-readable title
    pub title: String,
    /// Current pipeline stage
    pub stage: PipelineStage,
    /// Description or goal
    #[serde(default)]
    pub description: Option<String>,
    /// Path to worktree if in Building stage
    #[serde(default)]
    pub worktree_path: Option<PathBuf>,
    /// Error message if Failed
    #[serde(default)]
    pub error: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

/// Manager for feature storage in SQLite
pub struct FeatureManager {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl FeatureManager {
    /// Create a new FeatureManager from a CatalystDb
    pub fn new(db: &CatalystDb) -> Self {
        Self {
            conn: db.connection(),
        }
    }

    /// Create a new feature
    pub fn create(&self, title: &str) -> Result<Feature> {
        let id = generate_feature_id();
        let now = Utc::now();

        let feature = Feature {
            id: id.clone(),
            title: title.to_string(),
            stage: PipelineStage::Idea,
            description: None,
            worktree_path: None,
            error: None,
            created_at: now,
            updated_at: now,
        };

        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        conn.execute(
            r#"
            INSERT INTO features (id, title, stage, description, worktree_path, error, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                feature.id,
                feature.title,
                feature.stage.as_str(),
                feature.description,
                feature.worktree_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                feature.error,
                feature.created_at.to_rfc3339(),
                feature.updated_at.to_rfc3339(),
            ],
        )
        .context("Failed to create feature")?;

        Ok(feature)
    }

    /// Load a feature by ID
    pub fn load(&self, id: &str) -> Result<Feature> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let feature = conn
            .query_row(
                r#"
            SELECT id, title, stage, description, worktree_path, error, created_at, updated_at
            FROM features WHERE id = ?1
            "#,
                params![id],
                |row| Ok(Self::row_to_feature(row)?),
            )
            .context("Feature not found")?;

        Ok(feature)
    }

    /// Update a feature's stage
    pub fn update_stage(&self, id: &str, stage: PipelineStage) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let now = Utc::now().to_rfc3339();
        let affected = conn.execute(
            "UPDATE features SET stage = ?1, updated_at = ?2 WHERE id = ?3",
            params![stage.as_str(), now, id],
        )?;

        if affected == 0 {
            anyhow::bail!("Feature not found: {}", id);
        }

        Ok(())
    }

    /// Set worktree path for a feature
    pub fn set_worktree(&self, id: &str, path: PathBuf) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let now = Utc::now().to_rfc3339();
        let path_str = path.to_string_lossy().to_string();
        let affected = conn.execute(
            "UPDATE features SET worktree_path = ?1, updated_at = ?2 WHERE id = ?3",
            params![path_str, now, id],
        )?;

        if affected == 0 {
            anyhow::bail!("Feature not found: {}", id);
        }

        Ok(())
    }

    /// Mark a feature as failed
    pub fn set_failed(&self, id: &str, error: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let now = Utc::now().to_rfc3339();
        let affected = conn.execute(
            "UPDATE features SET stage = 'failed', error = ?1, updated_at = ?2 WHERE id = ?3",
            params![error, now, id],
        )?;

        if affected == 0 {
            anyhow::bail!("Feature not found: {}", id);
        }

        Ok(())
    }

    /// Save/update a feature
    pub fn save(&self, feature: &Feature) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        conn.execute(
            r#"
            INSERT OR REPLACE INTO features 
            (id, title, stage, description, worktree_path, error, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                feature.id,
                feature.title,
                feature.stage.as_str(),
                feature.description,
                feature
                    .worktree_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                feature.error,
                feature.created_at.to_rfc3339(),
                feature.updated_at.to_rfc3339(),
            ],
        )
        .context("Failed to save feature")?;

        Ok(())
    }

    /// List all features
    pub fn list_all(&self) -> Result<Vec<Feature>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT id, title, stage, description, worktree_path, error, created_at, updated_at
            FROM features
            ORDER BY created_at DESC
            "#,
        )?;

        let features = stmt
            .query_map([], |row| Ok(Self::row_to_feature(row)?))?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to list features")?;

        Ok(features)
    }

    /// List features by stage
    pub fn list_by_stage(&self, stage: PipelineStage) -> Result<Vec<Feature>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT id, title, stage, description, worktree_path, error, created_at, updated_at
            FROM features
            WHERE stage = ?1
            ORDER BY created_at DESC
            "#,
        )?;

        let features = stmt
            .query_map(params![stage.as_str()], |row| {
                Ok(Self::row_to_feature(row)?)
            })?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to list features by stage")?;

        Ok(features)
    }

    /// Delete a feature
    pub fn delete(&self, id: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        conn.execute("DELETE FROM features WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn row_to_feature(row: &rusqlite::Row) -> rusqlite::Result<Feature> {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let stage: String = row.get(2)?;
        let description: Option<String> = row.get(3)?;
        let worktree_path: Option<String> = row.get(4)?;
        let error: Option<String> = row.get(5)?;
        let created_at_str: String = row.get(6)?;
        let updated_at_str: String = row.get(7)?;

        Ok(Feature {
            id,
            title,
            stage: PipelineStage::from_str(&stage),
            description,
            worktree_path: worktree_path.map(PathBuf::from),
            error,
            created_at: DateTime::parse_from_rfc3339(&created_at_str)
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}

/// Generate a unique feature ID (timestamp-based)
fn generate_feature_id() -> String {
    let now = Utc::now();
    format!("f-{}", now.format("%Y%m%d-%H%M%S"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_stage_serialization() {
        let stage = PipelineStage::Building;
        let json = serde_json::to_string(&stage).unwrap();
        assert_eq!(json, "\"building\"");
    }

    #[test]
    fn test_feature_id_generation() {
        let id = generate_feature_id();
        assert!(id.starts_with("f-"));
    }
}
