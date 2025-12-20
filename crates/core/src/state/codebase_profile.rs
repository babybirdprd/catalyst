//! # Codebase Profile
//!
//! Stores detected patterns from brownfield project scanning.
//! Used by agents to understand and match existing code style.

use super::db::CatalystDb;
use anyhow::{Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Type of project detected
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    #[default]
    Unknown,
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Mixed,
}

/// Detected framework/library
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkInfo {
    pub name: String,
    pub version: Option<String>,
    pub category: String, // "web", "database", "testing", etc.
}

/// Module/file information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub path: PathBuf,
    pub name: String,
    pub loc: u32,
    pub is_public: bool,
    pub exports: Vec<String>,
}

/// Public API signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSignature {
    pub name: String,
    pub kind: String, // "function", "struct", "trait", "type"
    pub signature: String,
    pub file: PathBuf,
    pub line: u32,
}

/// Detected style patterns
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StylePatterns {
    /// Naming convention (snake_case, camelCase, etc.)
    pub naming_convention: Option<String>,
    /// Error handling pattern (Result, anyhow, thiserror, etc.)
    pub error_handling: Option<String>,
    /// Async runtime (tokio, async-std, etc.)
    pub async_runtime: Option<String>,
    /// Test framework (built-in, proptest, etc.)
    pub test_framework: Option<String>,
    /// Logging/tracing library
    pub logging: Option<String>,
}

/// Detailed naming convention detection
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NamingConventions {
    /// Function naming (snake_case, camelCase)
    pub functions: Option<String>,
    /// Struct naming (PascalCase)
    pub structs: Option<String>,
    /// Module naming
    pub modules: Option<String>,
    /// Detected error patterns (Result usage, unwrap usage, ? operator)
    #[serde(default)]
    pub error_patterns: Vec<String>,
}

/// Complete codebase profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebaseProfile {
    /// Project type based on files detected
    pub project_type: ProjectType,
    /// Root path of the project
    pub root_path: PathBuf,
    /// Detected frameworks and libraries
    pub frameworks: Vec<FrameworkInfo>,
    /// Total number of source files
    pub total_files: u32,
    /// Total lines of code
    pub total_loc: u32,
    /// Test coverage percentage (if detectable)
    pub test_coverage: Option<f32>,
    /// Module breakdown
    pub modules: Vec<ModuleInfo>,
    /// Public API signatures
    pub public_apis: Vec<ApiSignature>,
    /// Detected style patterns
    pub style_patterns: StylePatterns,
    /// Detailed naming conventions detected from AST
    #[serde(default)]
    pub naming_conventions: NamingConventions,
    /// Timestamp of scan
    pub scanned_at: chrono::DateTime<chrono::Utc>,
}

impl CodebaseProfile {
    /// Create a new empty profile
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            project_type: ProjectType::Unknown,
            root_path,
            frameworks: Vec::new(),
            total_files: 0,
            total_loc: 0,
            test_coverage: None,
            modules: Vec::new(),
            public_apis: Vec::new(),
            style_patterns: StylePatterns::default(),
            naming_conventions: NamingConventions::default(),
            scanned_at: chrono::Utc::now(),
        }
    }

    /// Generate a summary for agent prompts
    pub fn to_summary(&self) -> String {
        let mut summary = String::new();

        summary.push_str(&format!("Project Type: {:?}\n", self.project_type));
        summary.push_str(&format!(
            "Files: {}, LOC: {}\n",
            self.total_files, self.total_loc
        ));

        if !self.frameworks.is_empty() {
            summary.push_str("Frameworks: ");
            let names: Vec<_> = self.frameworks.iter().map(|f| f.name.as_str()).collect();
            summary.push_str(&names.join(", "));
            summary.push('\n');
        }

        if let Some(ref naming) = self.style_patterns.naming_convention {
            summary.push_str(&format!("Naming: {}\n", naming));
        }

        if let Some(ref errors) = self.style_patterns.error_handling {
            summary.push_str(&format!("Error handling: {}\n", errors));
        }

        summary
    }

    /// Save profile to SQLite database
    pub fn save(&self, db: &CatalystDb) -> Result<()> {
        let conn = db.connection();
        let conn = conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let data = serde_json::to_string(self)?;
        let scanned_at = self.scanned_at.to_rfc3339();

        conn.execute(
            "INSERT OR REPLACE INTO codebase_profile (id, data, scanned_at) VALUES (1, ?1, ?2)",
            params![data, scanned_at],
        )
        .context("Failed to save codebase profile")?;

        Ok(())
    }

    /// Load profile from SQLite database
    pub fn load(db: &CatalystDb) -> Result<Self> {
        let conn = db.connection();
        let conn = conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let data: String = conn
            .query_row(
                "SELECT data FROM codebase_profile WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .context("Codebase profile not found")?;

        let profile: Self = serde_json::from_str(&data)?;
        Ok(profile)
    }

    /// Async save wrapper for compatibility
    pub async fn save_async(&self, db: &CatalystDb) -> Result<()> {
        self.save(db)
    }

    /// Async load wrapper for compatibility
    pub async fn load_async(db: &CatalystDb) -> Result<Self> {
        Self::load(db)
    }

    /// Check if a profile exists in the database
    pub fn exists(db: &CatalystDb) -> bool {
        let conn_arc = db.connection();
        let conn = match conn_arc.lock() {
            Ok(c) => c,
            Err(_) => return false,
        };

        conn.query_row(
            "SELECT 1 FROM codebase_profile WHERE id = 1",
            [],
            |_| Ok(()),
        )
        .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_serialization() {
        let profile = CodebaseProfile::new(PathBuf::from("/test"));
        let json = serde_json::to_string(&profile).unwrap();
        assert!(json.contains("project_type"));
        assert!(json.contains("total_files"));
    }

    #[test]
    fn test_summary_generation() {
        let mut profile = CodebaseProfile::new(PathBuf::from("/test"));
        profile.project_type = ProjectType::Rust;
        profile.total_files = 47;
        profile.total_loc = 12340;
        profile.frameworks.push(FrameworkInfo {
            name: "Axum".to_string(),
            version: Some("0.7".to_string()),
            category: "web".to_string(),
        });

        let summary = profile.to_summary();
        assert!(summary.contains("Rust"));
        assert!(summary.contains("47"));
        assert!(summary.contains("Axum"));
    }
}
