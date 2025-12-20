//! # Context State Management (Braindump)
//!
//! Manages the `.catalyst/context/` directory for ingested files and ideas.
//! Ideas are stored in SQLite, while ingested files remain on disk.

use super::db::CatalystDb;
use super::io;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use ignore::WalkBuilder;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// An idea or thought captured in the braindump
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Idea {
    /// Unique identifier
    pub id: String,
    /// The idea content
    pub content: String,
    /// Source file if ingested from file
    #[serde(default)]
    pub source_file: Option<String>,
    /// Tags for organization
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

/// A file ingested into the context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFile {
    /// Relative path within context
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// File extension
    #[serde(default)]
    pub extension: Option<String>,
    /// When it was ingested
    pub ingested_at: DateTime<Utc>,
}

/// Manager for the braindump/context system
pub struct ContextManager {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl ContextManager {
    /// Create a new ContextManager from a CatalystDb
    pub fn new(db: &CatalystDb) -> Self {
        Self {
            conn: db.connection(),
        }
    }

    /// Ingest a single file into the context (files still stored on disk)
    pub async fn ingest_file(&self, source: &Path) -> Result<ContextFile> {
        let filename = source
            .file_name()
            .context("No filename")?
            .to_string_lossy()
            .to_string();

        let content = tokio::fs::read(source)
            .await
            .with_context(|| format!("Failed to read file: {:?}", source))?;

        let dest_path = format!("context/files/{}", filename);
        let content_str = String::from_utf8_lossy(&content);
        io::write_runtime_file(&dest_path, &content_str).await?;

        let ctx_file = ContextFile {
            path: filename.clone(),
            size: content.len() as u64,
            extension: source.extension().map(|e| e.to_string_lossy().to_string()),
            ingested_at: Utc::now(),
        };

        // Update the manifest in SQLite
        self.add_to_manifest(&ctx_file)?;

        Ok(ctx_file)
    }

    /// Recursively ingest a directory (respects .gitignore)
    pub async fn ingest_path(&self, source: &Path) -> Result<Vec<ContextFile>> {
        let mut files = Vec::new();

        if source.is_file() {
            files.push(self.ingest_file(source).await?);
            return Ok(files);
        }

        // Use ignore crate to respect .gitignore
        let walker = WalkBuilder::new(source)
            .hidden(false) // Don't ignore hidden files
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .filter_entry(|e| {
                // Skip common noise directories
                let name = e.file_name().to_string_lossy();
                !matches!(
                    name.as_ref(),
                    "node_modules" | "target" | ".git" | "dist" | "build"
                )
            })
            .build();

        for entry in walker {
            let entry = entry?;
            if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                // Calculate relative path
                let rel_path = entry.path().strip_prefix(source).unwrap_or(entry.path());

                // Read and write to context
                let content = std::fs::read(entry.path())?;
                let dest_path = format!("context/files/{}", rel_path.to_string_lossy());
                let content_str = String::from_utf8_lossy(&content);
                io::write_runtime_file(&dest_path, &content_str).await?;

                let ctx_file = ContextFile {
                    path: rel_path.to_string_lossy().to_string(),
                    size: content.len() as u64,
                    extension: entry
                        .path()
                        .extension()
                        .map(|e| e.to_string_lossy().to_string()),
                    ingested_at: Utc::now(),
                };

                self.add_to_manifest(&ctx_file)?;
                files.push(ctx_file);
            }
        }

        Ok(files)
    }

    /// Create a new idea from text
    pub fn create_idea(&self, content: &str) -> Result<Idea> {
        let id = format!("idea-{}", Utc::now().format("%Y%m%d-%H%M%S-%3f"));
        let now = Utc::now();

        let idea = Idea {
            id: id.clone(),
            content: content.to_string(),
            source_file: None,
            tags: Vec::new(),
            created_at: now,
        };

        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        conn.execute(
            r#"
            INSERT INTO ideas (id, content, source_file, tags_json, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                idea.id,
                idea.content,
                idea.source_file,
                serde_json::to_string(&idea.tags)?,
                idea.created_at.to_rfc3339(),
            ],
        )
        .context("Failed to create idea")?;

        Ok(idea)
    }

    /// List all ideas
    pub fn list_ideas(&self) -> Result<Vec<Idea>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT id, content, source_file, tags_json, created_at
            FROM ideas
            ORDER BY created_at DESC
            "#,
        )?;

        let ideas = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let content: String = row.get(1)?;
                let source_file: Option<String> = row.get(2)?;
                let tags_json: String = row.get(3)?;
                let created_at_str: String = row.get(4)?;

                Ok(Idea {
                    id,
                    content,
                    source_file,
                    tags: serde_json::from_str(&tags_json).unwrap_or_default(),
                    created_at: DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|t| t.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to list ideas")?;

        Ok(ideas)
    }

    /// List all context files from manifest
    pub fn list_files(&self) -> Result<Vec<ContextFile>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let result: Option<String> = conn
            .query_row(
                "SELECT files_json FROM context_manifest WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .ok();

        match result {
            Some(json) => Ok(serde_json::from_str(&json)?),
            None => Ok(Vec::new()),
        }
    }

    /// Promote an idea to a feature
    pub async fn promote_to_feature(
        &self,
        idea_id: &str,
        db: &CatalystDb,
    ) -> Result<super::feature_state::Feature> {
        let idea = self.load_idea(idea_id)?;

        // Create a feature from the idea
        let feature = super::feature_state::FeatureManager::new(db).create(&idea.content)?;

        // Delete the idea
        self.delete_idea(idea_id)?;

        Ok(feature)
    }

    /// Load a specific idea
    pub fn load_idea(&self, id: &str) -> Result<Idea> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let idea = conn
            .query_row(
                r#"
            SELECT id, content, source_file, tags_json, created_at
            FROM ideas WHERE id = ?1
            "#,
                params![id],
                |row| {
                    let id: String = row.get(0)?;
                    let content: String = row.get(1)?;
                    let source_file: Option<String> = row.get(2)?;
                    let tags_json: String = row.get(3)?;
                    let created_at_str: String = row.get(4)?;

                    Ok(Idea {
                        id,
                        content,
                        source_file,
                        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
                        created_at: DateTime::parse_from_rfc3339(&created_at_str)
                            .map(|t| t.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                    })
                },
            )
            .context("Idea not found")?;

        Ok(idea)
    }

    /// Delete an idea
    fn delete_idea(&self, id: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        conn.execute("DELETE FROM ideas WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Add a file to the manifest (stored in SQLite as JSON blob)
    fn add_to_manifest(&self, file: &ContextFile) -> Result<()> {
        let mut files = self.list_files()?;

        // Remove if already exists (update)
        files.retain(|f| f.path != file.path);
        files.push(file.clone());

        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let json = serde_json::to_string(&files)?;
        conn.execute(
            "INSERT OR REPLACE INTO context_manifest (id, files_json) VALUES (1, ?1)",
            params![json],
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idea_serialization() {
        let idea = Idea {
            id: "test-1".to_string(),
            content: "Build a feature".to_string(),
            source_file: None,
            tags: vec!["urgent".to_string()],
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&idea).unwrap();
        assert!(json.contains("test-1"));
    }
}
