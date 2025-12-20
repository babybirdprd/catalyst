//! # Project State
//!
//! Types and functions for managing the `state.json` file.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Project mode determines agent composition and strictness
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectMode {
    /// Fast prototyping, minimal checks
    SpeedRun,
    /// Production-grade, standard checks
    Lab,
    /// Enterprise-grade, full audit
    Fortress,
}

/// Status of an unknown
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UnknownStatus {
    Open,
    Researching,
    Resolved,
}

/// Status of a feature
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FeatureStatus {
    Pending,
    InProgress,
    Complete,
    Blocked,
}

/// An unknown that needs resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unknown {
    pub id: String,
    pub category: String,
    pub question: String,
    pub criticality: String,
    pub status: UnknownStatus,
    #[serde(default)]
    pub resolution: Option<String>,
}

/// A feature being tracked
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub id: String,
    pub name: String,
    pub status: FeatureStatus,
    #[serde(default)]
    pub modules: Vec<String>,
}

/// Record of an agent action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub agent: String,
    pub action: String,
    #[serde(default)]
    pub input_hash: Option<String>,
    #[serde(default)]
    pub output_hash: Option<String>,
}

/// Metadata about the state file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMetadata {
    pub created_at: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
    pub schema_version: String,
}

/// The complete project state
///
/// Note: Features are now managed separately via `FeatureManager` in sharded files.
/// See `feature_state.rs` for the new feature management system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectState {
    pub project_id: String,
    pub project_name: String,
    pub mode: ProjectMode,
    pub current_phase: String,
    pub stack: serde_json::Value,
    #[serde(default)]
    pub unknowns: Vec<Unknown>,
    #[serde(default)]
    pub agent_history: Vec<AgentHistoryEntry>,
    pub metadata: StateMetadata,
}

/// Load the project state from a JSON file
pub fn load_state(path: &Path) -> Result<ProjectState> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read state file: {}", path.display()))?;

    let state: ProjectState =
        serde_json::from_str(&content).with_context(|| "Failed to parse state.json")?;

    Ok(state)
}

/// Save the project state to a JSON file
pub fn save_state(path: &Path, state: &ProjectState) -> Result<()> {
    let mut state = state.clone();
    state.metadata.last_modified = Utc::now();

    let content =
        serde_json::to_string_pretty(&state).with_context(|| "Failed to serialize state")?;

    std::fs::write(path, content)
        .with_context(|| format!("Failed to write state file: {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_mode_serialization() {
        let mode = ProjectMode::Fortress;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"fortress\"");
    }
}
