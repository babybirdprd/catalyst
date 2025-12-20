//! # Catalyst Memory
//!
//! Wrapper around memory systems for Catalyst-specific knowledge.
//! Supports both InMemory (radkit) and SQLite (persistent) providers.

use crate::state::db::CatalystDb;
use anyhow::{Context, Result};
use radkit::runtime::context::AuthContext;
use radkit::runtime::memory::{
    ContentSource, InMemoryMemoryService, MemoryContent, MemoryService, SearchOptions, SourceType,
};
use std::collections::HashMap;
use std::sync::Arc;

use super::sqlite_memory::SqliteMemoryService;

/// Memory provider selection
#[derive(Debug, Clone, Default)]
pub enum MemoryProvider {
    /// In-memory (radkit default, non-persistent)
    InMemory,
    /// SQLite file-based (persistent)
    #[default]
    Sqlite,
}

/// Configuration for memory service
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Application name for namespacing
    pub app_name: String,
    /// User name for namespacing
    pub user_name: String,
    /// Maximum search results
    pub max_results: usize,
    /// Memory provider to use
    pub provider: MemoryProvider,
    /// Path for SQLite database (only used with SQLite provider)
    pub sqlite_path: String,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            app_name: "catalyst".to_string(),
            user_name: "default".to_string(),
            max_results: 5,
            provider: MemoryProvider::Sqlite,
            sqlite_path: ".catalyst/memory.db".to_string(),
        }
    }
}

/// Internal storage for memory backends
enum MemoryBackend {
    InMemory {
        service: Arc<InMemoryMemoryService>,
        auth: AuthContext,
    },
    Sqlite(SqliteMemoryService),
}

/// Catalyst memory service wrapper
pub struct CatalystMemory {
    backend: MemoryBackend,
    config: MemoryConfig,
}

impl CatalystMemory {
    /// Create a new memory instance with SQLite backend from CatalystDb
    pub fn new_with_db(db: &CatalystDb, config: MemoryConfig) -> Self {
        let backend = match config.provider {
            MemoryProvider::InMemory => {
                let auth = AuthContext {
                    app_name: config.app_name.clone(),
                    user_name: config.user_name.clone(),
                };
                MemoryBackend::InMemory {
                    service: Arc::new(InMemoryMemoryService::new()),
                    auth,
                }
            }
            MemoryProvider::Sqlite => MemoryBackend::Sqlite(SqliteMemoryService::new(db)),
        };

        Self { backend, config }
    }

    /// Create a new memory instance (legacy, creates its own InMemory fallback)
    pub fn new(config: MemoryConfig) -> Self {
        let backend = match config.provider {
            MemoryProvider::InMemory => {
                let auth = AuthContext {
                    app_name: config.app_name.clone(),
                    user_name: config.user_name.clone(),
                };
                MemoryBackend::InMemory {
                    service: Arc::new(InMemoryMemoryService::new()),
                    auth,
                }
            }
            MemoryProvider::Sqlite => {
                // For legacy usage without CatalystDb, fallback to InMemory
                tracing::warn!("CatalystMemory::new() with Sqlite requires CatalystDb, falling back to InMemory");
                let auth = AuthContext {
                    app_name: config.app_name.clone(),
                    user_name: config.user_name.clone(),
                };
                MemoryBackend::InMemory {
                    service: Arc::new(InMemoryMemoryService::new()),
                    auth,
                }
            }
        };

        Self { backend, config }
    }

    /// Index all project documents from the database
    ///
    /// Reads prompts and project documents from CatalystDb and adds them to memory.
    pub async fn index_project_docs(&self, db: &CatalystDb) -> Result<usize> {
        let mut count = 0;

        // Index prompts
        let prompts = db.list_prompts()?;
        for (slug, _version) in prompts {
            if let Ok(content) = db.get_prompt(&slug) {
                let source_id = format!("prompt:{}", slug);
                match &self.backend {
                    MemoryBackend::InMemory { service, auth } => {
                        let memory_content = MemoryContent {
                            text: content,
                            source: ContentSource::Document {
                                document_id: source_id.clone(),
                                name: format!("{}.md", slug),
                                chunk_index: 0,
                                total_chunks: 1,
                            },
                            metadata: HashMap::new(),
                        };
                        service.add(auth, memory_content).await?;
                    }
                    MemoryBackend::Sqlite(service) => {
                        service.add(&content, "prompt", &source_id)?;
                    }
                }
                count += 1;
            }
        }

        // Index project documents
        let docs = db.list_documents()?;
        for slug in docs {
            if let Ok((title, content)) = db.get_document(&slug) {
                let source_id = format!("doc:{}", slug);
                match &self.backend {
                    MemoryBackend::InMemory { service, auth } => {
                        let memory_content = MemoryContent {
                            text: content,
                            source: ContentSource::Document {
                                document_id: source_id.clone(),
                                name: title,
                                chunk_index: 0,
                                total_chunks: 1,
                            },
                            metadata: HashMap::new(),
                        };
                        service.add(auth, memory_content).await?;
                    }
                    MemoryBackend::Sqlite(service) => {
                        service.add(&content, "document", &source_id)?;
                    }
                }
                count += 1;
            }
        }

        tracing::debug!("Indexed {} documents into memory", count);
        Ok(count)
    }

    /// Search memory for relevant context
    pub async fn search(&self, query: &str) -> Result<Vec<String>> {
        match &self.backend {
            MemoryBackend::InMemory { service, auth } => {
                let options = SearchOptions::default().with_limit(self.config.max_results);
                let results = service.search(auth, query, options).await?;
                Ok(results.into_iter().map(|entry| entry.text).collect())
            }
            MemoryBackend::Sqlite(service) => {
                let entries = service.search(query, self.config.max_results)?;
                Ok(entries.into_iter().map(|e| e.text).collect())
            }
        }
    }

    /// Search only documents (specs, prompts)
    pub async fn search_knowledge(&self, query: &str) -> Result<Vec<String>> {
        match &self.backend {
            MemoryBackend::InMemory { service, auth } => {
                let options = SearchOptions::default()
                    .with_limit(self.config.max_results)
                    .with_source_types(vec![SourceType::Document]);
                let results = service.search(auth, query, options).await?;
                Ok(results.into_iter().map(|entry| entry.text).collect())
            }
            MemoryBackend::Sqlite(service) => {
                // For SQLite, we filter by source_type in Rust (simple approach)
                let entries = service.search(query, self.config.max_results * 2)?;
                Ok(entries
                    .into_iter()
                    .filter(|e| e.source_type == "document")
                    .take(self.config.max_results)
                    .map(|e| e.text)
                    .collect())
            }
        }
    }

    /// Save a fact (e.g., resolved unknown, decision)
    pub async fn save_fact(&self, fact: &str, category: Option<&str>) -> Result<String> {
        match &self.backend {
            MemoryBackend::InMemory { service, auth } => {
                let content = MemoryContent {
                    text: fact.to_string(),
                    source: ContentSource::UserFact {
                        category: category.map(|s| s.to_string()),
                    },
                    metadata: HashMap::new(),
                };
                let id = service.add(auth, content).await?;
                Ok(id)
            }
            MemoryBackend::Sqlite(service) => {
                let source_data = category.unwrap_or("general");
                let id = service.add(fact, "fact", source_data)?;
                Ok(id.to_string())
            }
        }
    }

    /// Save a conversation turn
    pub async fn save_conversation(
        &self,
        context_id: &str,
        message_id: &str,
        role: &str,
        text: &str,
    ) -> Result<String> {
        match &self.backend {
            MemoryBackend::InMemory { service, auth } => {
                let content = MemoryContent {
                    text: text.to_string(),
                    source: ContentSource::PastConversation {
                        context_id: context_id.to_string(),
                        message_id: message_id.to_string(),
                        role: role.to_string(),
                    },
                    metadata: HashMap::new(),
                };
                let id = service.add(auth, content).await?;
                Ok(id)
            }
            MemoryBackend::Sqlite(service) => {
                let source_data = format!("{}:{}:{}", context_id, message_id, role);
                let id = service.add(text, "conversation", &source_data)?;
                Ok(id.to_string())
            }
        }
    }

    /// Check if using persistent storage
    pub fn is_persistent(&self) -> bool {
        matches!(self.backend, MemoryBackend::Sqlite(_))
    }

    /// Get memory count (SQLite only)
    pub fn count(&self) -> Result<i64> {
        match &self.backend {
            MemoryBackend::Sqlite(service) => service.count(),
            MemoryBackend::InMemory { .. } => Ok(0), // InMemory doesn't expose count
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_config_default() {
        let config = MemoryConfig::default();
        assert_eq!(config.app_name, "catalyst");
        assert_eq!(config.max_results, 5);
        assert!(matches!(config.provider, MemoryProvider::Sqlite));
    }

    #[test]
    fn test_memory_provider_inmemory() {
        let config = MemoryConfig {
            provider: MemoryProvider::InMemory,
            ..Default::default()
        };
        let memory = CatalystMemory::new(config);
        assert!(!memory.is_persistent());
    }
}
