//! # Interaction State Manager
//!
//! Persistent storage for human-in-the-loop interactions using SQLite.
//! Enables blocking questions/decisions that survive server restarts.

use super::db::CatalystDb;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Kind of interaction required from the user
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionKind {
    /// A blocking question (e.g., "Postgres or SQLite?")
    Decision,
    /// A request for text input (e.g., "Provide API Key")
    Input,
    /// A blocking alert (e.g., "Build Failed 5 times")
    Alert,
}

impl InteractionKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::Input => "input",
            Self::Alert => "alert",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "decision" => Self::Decision,
            "input" => Self::Input,
            "alert" => Self::Alert,
            _ => Self::Decision,
        }
    }
}

/// Status of an interaction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionStatus {
    Pending,
    Responded,
    Ignored,
}

impl InteractionStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Responded => "responded",
            Self::Ignored => "ignored",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "responded" => Self::Responded,
            "ignored" => Self::Ignored,
            _ => Self::Pending,
        }
    }
}

/// A human-in-the-loop interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interaction {
    pub id: String,
    pub thread_id: String,
    pub kind: InteractionKind,
    pub status: InteractionStatus,
    pub from_agent: String,
    pub title: String,
    pub description: String,
    pub options: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<InteractionResponse>,
}

/// Response to an interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_option: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_input: Option<String>,
    #[serde(default)]
    pub attachments: Vec<String>,
    pub responded_by: String,
}

/// SQLite-backed interaction manager
pub struct InteractionManager {
    conn: Arc<Mutex<Connection>>,
}

impl InteractionManager {
    /// Create from shared CatalystDb connection
    pub fn new(db: &CatalystDb) -> Self {
        Self {
            conn: db.connection(),
        }
    }

    /// Save an interaction
    pub fn save(&self, interaction: &Interaction) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let options_json = serde_json::to_string(&interaction.options)?;
        let schema_json = interaction
            .schema
            .as_ref()
            .map(|s| serde_json::to_string(s))
            .transpose()?;
        let response_json = interaction
            .response
            .as_ref()
            .map(|r| serde_json::to_string(r))
            .transpose()?;

        conn.execute(
            r#"
            INSERT OR REPLACE INTO interactions 
            (id, thread_id, kind, status, from_agent, title, description, 
             options_json, schema_json, created_at, resolved_at, response_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                interaction.id,
                interaction.thread_id,
                interaction.kind.as_str(),
                interaction.status.as_str(),
                interaction.from_agent,
                interaction.title,
                interaction.description,
                options_json,
                schema_json,
                interaction.created_at.to_rfc3339(),
                interaction.resolved_at.map(|t| t.to_rfc3339()),
                response_json,
            ],
        )
        .context("Failed to save interaction")?;

        Ok(())
    }

    /// Load an interaction by ID
    pub fn load(&self, id: &str) -> Result<Interaction> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT id, thread_id, kind, status, from_agent, title, description,
                   options_json, schema_json, created_at, resolved_at, response_json
            FROM interactions WHERE id = ?1
            "#,
        )?;

        let interaction = stmt
            .query_row(params![id], |row| Ok(Self::row_to_interaction(row)?))
            .context("Interaction not found")?;

        Ok(interaction)
    }

    /// Resolve an interaction with a response
    pub fn resolve(&self, id: &str, response: InteractionResponse) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let response_json = serde_json::to_string(&response)?;
        let now = Utc::now().to_rfc3339();

        let affected = conn.execute(
            r#"
            UPDATE interactions 
            SET status = 'responded', resolved_at = ?1, response_json = ?2
            WHERE id = ?3
            "#,
            params![now, response_json, id],
        )?;

        if affected == 0 {
            anyhow::bail!("Interaction not found: {}", id);
        }

        Ok(())
    }

    /// List all pending interactions
    pub fn list_pending(&self) -> Result<Vec<Interaction>> {
        self.list_by_status("pending", 100)
    }

    /// List resolved interactions (history)
    pub fn list_history(&self, limit: usize) -> Result<Vec<Interaction>> {
        self.list_by_status("responded", limit)
    }

    fn list_by_status(&self, status: &str, limit: usize) -> Result<Vec<Interaction>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT id, thread_id, kind, status, from_agent, title, description,
                   options_json, schema_json, created_at, resolved_at, response_json
            FROM interactions 
            WHERE status = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )?;

        let interactions = stmt
            .query_map(params![status, limit as i64], |row| {
                Ok(Self::row_to_interaction(row)?)
            })?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to list interactions")?;

        Ok(interactions)
    }

    fn row_to_interaction(row: &rusqlite::Row) -> rusqlite::Result<Interaction> {
        let options_json: String = row.get(7)?;
        let schema_json: Option<String> = row.get(8)?;
        let created_at_str: String = row.get(9)?;
        let resolved_at_str: Option<String> = row.get(10)?;
        let response_json: Option<String> = row.get(11)?;

        Ok(Interaction {
            id: row.get(0)?,
            thread_id: row.get(1)?,
            kind: InteractionKind::from_str(&row.get::<_, String>(2)?),
            status: InteractionStatus::from_str(&row.get::<_, String>(3)?),
            from_agent: row.get(4)?,
            title: row.get(5)?,
            description: row.get(6)?,
            options: serde_json::from_str(&options_json).unwrap_or_default(),
            schema: schema_json.and_then(|s| serde_json::from_str(&s).ok()),
            created_at: DateTime::parse_from_rfc3339(&created_at_str)
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            resolved_at: resolved_at_str.and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|t| t.with_timezone(&Utc))
                    .ok()
            }),
            response: response_json.and_then(|s| serde_json::from_str(&s).ok()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_interaction() -> Interaction {
        Interaction {
            id: "test-001".to_string(),
            thread_id: "thread-001".to_string(),
            kind: InteractionKind::Decision,
            status: InteractionStatus::Pending,
            from_agent: "architect".to_string(),
            title: "Database Selection".to_string(),
            description: "Which database should we use?".to_string(),
            options: vec!["PostgreSQL".to_string(), "SQLite".to_string()],
            schema: None,
            created_at: Utc::now(),
            resolved_at: None,
            response: None,
        }
    }

    #[test]
    fn test_interaction_save_and_load() {
        let path = ".catalyst/test_interactions.db";
        fs::create_dir_all(".catalyst").ok();
        let _ = fs::remove_file(path);

        let db = CatalystDb::open_at(path).unwrap();
        let manager = InteractionManager::new(&db);
        let interaction = test_interaction();

        // Save
        manager.save(&interaction).unwrap();

        // Load
        let loaded = manager.load(&interaction.id).unwrap();
        assert_eq!(loaded.id, interaction.id);
        assert_eq!(loaded.title, interaction.title);
        assert_eq!(loaded.kind, InteractionKind::Decision);
        assert_eq!(loaded.status, InteractionStatus::Pending);

        // Cleanup
        drop(db);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_interaction_resolve() {
        let path = ".catalyst/test_interactions_resolve.db";
        fs::create_dir_all(".catalyst").ok();
        let _ = fs::remove_file(path);

        let db = CatalystDb::open_at(path).unwrap();
        let manager = InteractionManager::new(&db);
        let interaction = test_interaction();

        manager.save(&interaction).unwrap();

        // Resolve
        let response = InteractionResponse {
            selected_option: Some("PostgreSQL".to_string()),
            text_input: None,
            attachments: vec![],
            responded_by: "user".to_string(),
        };
        manager.resolve(&interaction.id, response).unwrap();

        // Verify
        let loaded = manager.load(&interaction.id).unwrap();
        assert_eq!(loaded.status, InteractionStatus::Responded);
        assert!(loaded.resolved_at.is_some());
        assert_eq!(
            loaded.response.unwrap().selected_option,
            Some("PostgreSQL".to_string())
        );

        drop(db);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_list_pending_and_history() {
        let path = ".catalyst/test_interactions_list.db";
        fs::create_dir_all(".catalyst").ok();
        let _ = fs::remove_file(path);

        let db = CatalystDb::open_at(path).unwrap();
        let manager = InteractionManager::new(&db);

        // Create two interactions
        let mut int1 = test_interaction();
        int1.id = "pending-001".to_string();

        let mut int2 = test_interaction();
        int2.id = "resolved-001".to_string();
        int2.status = InteractionStatus::Responded;

        manager.save(&int1).unwrap();
        manager.save(&int2).unwrap();

        // List pending
        let pending = manager.list_pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "pending-001");

        // List history
        let history = manager.list_history(10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, "resolved-001");

        drop(db);
        let _ = fs::remove_file(path);
    }
}
