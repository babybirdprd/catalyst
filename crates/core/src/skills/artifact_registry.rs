//! # A2A Artifact Registry
//!
//! Shared artifact types for inter-agent communication.
//! These types are serialized to JSON and returned in `OnRequestResult::Completed`.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use radkit::agent::Artifact;
//! use crate::skills::artifact_registry::UnknownsArtifact;
//!
//! let artifact = Artifact::from_json("unknowns.json", &my_unknowns)?;
//! Ok(OnRequestResult::Completed {
//!     message: Some(Content::from_text("...")),
//!     artifacts: vec![artifact],
//! })
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ============================================================================
// Parse Skill Artifacts
// ============================================================================

/// Summary of an ambiguity for artifact output
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AmbiguitySummary {
    pub id: String,
    pub category: String,
    pub question: String,
    pub criticality: String,
}

/// Summary of an inferred known (auto-resolved ambiguity)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InferredKnownSummary {
    pub id: String,
    pub category: String,
    pub original_question: String,
    pub resolution: String,
    pub evidence: String,
}

/// Artifact from ParseSkill containing identified unknowns
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UnknownsArtifact {
    pub ambiguities: Vec<AmbiguitySummary>,
    pub total_count: usize,
    pub blocker_count: usize,
    /// Number of ambiguities auto-resolved from codebase profile
    #[serde(default)]
    pub inferred_count: usize,
}

// ============================================================================
// Researcher Skill Artifacts
// ============================================================================

/// Summary of a research option
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OptionSummary {
    pub name: String,
    pub description: String,
    pub complexity: u32,
    pub pros_count: usize,
    pub cons_count: usize,
}

/// Artifact from ResearcherSkill containing research findings
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResearchArtifact {
    pub unknown_id: String,
    pub options: Vec<OptionSummary>,
    pub recommended: Option<String>,
    pub summary: String,
}

// ============================================================================
// Architect Skill Artifacts
// ============================================================================

/// A spec update from an architectural decision
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpecUpdateSummary {
    pub section: String,
    pub action: String, // "add", "modify", "remove"
}

/// Artifact from ArchitectSkill containing the decision
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DecisionArtifact {
    pub unknown_id: String,
    pub chosen_option: String,
    pub rationale: String,
    pub spec_updates: Vec<SpecUpdateSummary>,
    pub dependencies_added: Vec<String>,
}

// ============================================================================
// Critic Skill Artifacts
// ============================================================================

/// Summary of a concern raised by the critic
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConcernSummary {
    pub severity: String,
    pub description: String,
}

/// Artifact from CriticSkill containing the review verdict
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewArtifact {
    pub verdict: String,
    pub confidence: f32,
    pub concerns: Vec<ConcernSummary>,
    pub blocking_count: usize,
}

// ============================================================================
// Atomizer Skill Artifacts
// ============================================================================

/// Summary of an atomic module
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleSummary {
    pub path: String,
    pub responsibility: String,
    pub max_lines: u32,
}

/// Artifact from AtomizerSkill containing the module breakdown
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AtomizationArtifact {
    pub feature_id: String,
    pub feature_name: String,
    pub modules: Vec<ModuleSummary>,
    pub total_estimated_lines: u32,
}

// ============================================================================
// Taskmaster Skill Artifacts
// ============================================================================

/// Summary of a mission task
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskSummary {
    pub number: u32,
    pub action: String,
    pub file_path: String,
}

/// Artifact from TaskmasterSkill containing the mission
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MissionArtifact {
    pub feature_name: String,
    pub objective: String,
    pub tasks: Vec<TaskSummary>,
    pub task_count: usize,
}

// ============================================================================
// Builder Skill Artifacts (Hybrid approach)
// ============================================================================

/// Summary of a file change (hybrid: paths + metrics, not full diffs)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileChange {
    pub path: String,
    pub action: String, // "created", "modified", "deleted"
    pub lines_added: u32,
    pub lines_removed: u32,
    pub change_summary: String,
}

/// Artifact from BuilderSkill containing build results
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BuildArtifact {
    pub success: bool,
    pub files: Vec<FileChange>,
    pub build_passed: bool,
    pub tests_passed: bool,
    pub iterations: u32,
    pub error_count: usize,
}

// ============================================================================
// Constraint Skill Artifacts
// ============================================================================

/// A constraint violation summary for artifacts
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConstraintViolationSummary {
    pub kind: String,
    pub location: String,
    pub actual: u32,
    pub limit: u32,
    pub message: String,
}

/// Artifact from ConstraintSkill containing validation results
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConstraintArtifact {
    pub passed: bool,
    pub violations: Vec<ConstraintViolationSummary>,
    pub warnings_count: usize,
    pub errors_count: usize,
}

// ============================================================================
// Merge Skill Artifacts
// ============================================================================

/// Resolution result for a single merge conflict
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MergeResolutionSummary {
    pub file_path: String,
    pub strategy: String,
    pub success: bool,
}

/// Artifact from MergeSkill containing resolution results
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MergeArtifact {
    pub resolutions: Vec<MergeResolutionSummary>,
    pub total_conflicts: u32,
    pub resolved_count: u32,
    pub success: bool,
}
