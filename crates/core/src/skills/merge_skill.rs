//! # Merge Skill
//!
//! A2A-native skill for resolving git merge conflicts using 3-Truth synthesis.
//! Wraps the `tools/merge.rs` extraction with LLM-powered semantic resolution.
//!
//! ## Input Format
//!
//! ```json
//! {
//!   "repo_path": "/path/to/repo",
//!   "file_path": "src/lib.rs"
//! }
//! ```
//!
//! ## 3-Truth Synthesis
//!
//! Uses git plumbing to extract clean versions (no conflict markers):
//! - **Base**: Common ancestor (`:1:path`)
//! - **Ours**: Current branch (`:2:path`)
//! - **Theirs**: Incoming branch (`:3:path`)

use crate::models::ModelConfig;
use crate::run_llm_function;
use crate::skills::artifact_registry::{MergeArtifact, MergeResolutionSummary};
use crate::tools::merge::{
    apply_resolution, extract_conflict_versions, verify_resolution, VerifyResult,
};
use async_trait::async_trait;
use radkit::agent::{Artifact, OnRequestResult, SkillHandler};
use radkit::errors::{AgentError, AgentResult};
use radkit::macros::{skill, LLMOutput};
use radkit::models::Content;
use radkit::runtime::context::{ProgressSender, State};
use radkit::runtime::AgentRuntime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ============================================================================
// Types
// ============================================================================

/// LLM output for merge resolution
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct MergeResolution {
    /// The resolved/merged code content
    pub resolved_content: String,
    /// Strategy used: "semantic_merge", "prefer_ours", "prefer_theirs"
    pub strategy: String,
    /// Explanation of merge decisions
    pub explanation: String,
}

/// Input for merge skill
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MergeInput {
    pub repo_path: String,
    pub file_path: String,
}

// ============================================================================
// Skill Definition
// ============================================================================

/// Merge conflict resolution skill
#[skill(
    id = "merge",
    name = "Merge Resolver",
    description = "Resolves git merge conflicts using 3-Truth synthesis (base/ours/theirs). Uses LLM for semantic understanding and automatic verification.",
    tags = ["merge", "conflict", "git", "safety", "cyborg"],
    examples = [
        "Resolve merge conflict in src/lib.rs",
        "Combine parallel changes from feature branches"
    ],
    input_modes = ["application/json"],
    output_modes = ["application/json"]
)]
pub struct MergeSkill {
    config: ModelConfig,
}

impl MergeSkill {
    pub fn new(config: ModelConfig) -> Self {
        Self { config }
    }

    pub fn with_model(model: &str) -> Self {
        Self::new(ModelConfig::new(model))
    }

    pub fn default() -> Self {
        Self::new(ModelConfig::default())
    }
}

#[async_trait]
impl SkillHandler for MergeSkill {
    async fn on_request(
        &self,
        _state: &mut State,
        progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        let input = content.first_text().unwrap_or_default();

        // Parse input
        let merge_input: MergeInput = serde_json::from_str(input).map_err(|e| {
            AgentError::Internal {
                component: "merge_skill".to_string(),
                reason: format!("Invalid JSON input: {}. Expected {{ \"repo_path\": \"...\", \"file_path\": \"...\" }}", e),
            }
        })?;

        let repo_path = Path::new(&merge_input.repo_path);
        let file_path = Path::new(&merge_input.file_path);

        progress
            .send_update(&format!(
                "Extracting conflict versions for {}...",
                merge_input.file_path
            ))
            .await?;

        // Extract 3-way versions using git plumbing
        let conflict =
            extract_conflict_versions(repo_path, file_path).map_err(|e| AgentError::Internal {
                component: "merge_skill".to_string(),
                reason: format!("Failed to extract conflict versions: {}", e),
            })?;

        progress
            .send_update("Synthesizing resolution with LLM...")
            .await?;

        // Build prompt for LLM
        let prompt = format!(
            "File: {}\n\n\
             BASE (common ancestor):\n```\n{}\n```\n\n\
             OURS (current branch):\n```\n{}\n```\n\n\
             THEIRS (incoming branch):\n```\n{}\n```",
            merge_input.file_path,
            if conflict.base.is_empty() {
                "(new file)"
            } else {
                &conflict.base
            },
            conflict.ours,
            conflict.theirs
        );

        let resolution = run_llm_function!(&self.config, MergeResolution, MERGE_PROMPT, prompt)
            .map_err(|e| AgentError::Internal {
                component: "merge_skill".to_string(),
                reason: format!("LLM resolution failed: {}", e),
            })?;

        progress
            .send_update("Applying and verifying resolution...")
            .await?;

        // Apply the resolution
        apply_resolution(repo_path, file_path, &resolution.resolved_content).map_err(|e| {
            AgentError::Internal {
                component: "merge_skill".to_string(),
                reason: format!("Failed to apply resolution: {}", e),
            }
        })?;

        // Verify the resolution compiles/validates
        match verify_resolution(repo_path, file_path) {
            Ok(VerifyResult::Passed) => {
                progress
                    .send_update("Resolution verified successfully.")
                    .await?;

                let artifact_data = MergeArtifact {
                    resolutions: vec![MergeResolutionSummary {
                        file_path: merge_input.file_path.clone(),
                        strategy: resolution.strategy.clone(),
                        success: true,
                    }],
                    total_conflicts: 1,
                    resolved_count: 1,
                    success: true,
                };

                let artifact =
                    Artifact::from_json("merge_result.json", &artifact_data).map_err(|e| {
                        AgentError::Internal {
                            component: "merge_skill".to_string(),
                            reason: format!("Failed to create artifact: {}", e),
                        }
                    })?;

                Ok(OnRequestResult::Completed {
                    message: Some(Content::from_text(&format!(
                        "✅ Merge resolved using **{}** strategy\n\n**Explanation:** {}",
                        resolution.strategy, resolution.explanation
                    ))),
                    artifacts: vec![artifact],
                })
            }
            Ok(VerifyResult::Failed { errors }) => {
                let error_summary = errors.join("\n");

                let artifact_data = MergeArtifact {
                    resolutions: vec![MergeResolutionSummary {
                        file_path: merge_input.file_path.clone(),
                        strategy: resolution.strategy.clone(),
                        success: false,
                    }],
                    total_conflicts: 1,
                    resolved_count: 0,
                    success: false,
                };

                let artifact =
                    Artifact::from_json("merge_result.json", &artifact_data).map_err(|e| {
                        AgentError::Internal {
                            component: "merge_skill".to_string(),
                            reason: format!("Failed to create artifact: {}", e),
                        }
                    })?;

                Ok(OnRequestResult::Completed {
                    message: Some(Content::from_text(&format!(
                        "⚠️ Merge applied but verification failed:\n\n```\n{}\n```",
                        error_summary
                    ))),
                    artifacts: vec![artifact],
                })
            }
            Err(e) => Ok(OnRequestResult::Failed {
                error: Content::from_text(&format!("Verification error: {}", e)),
            }),
        }
    }
}

const MERGE_PROMPT: &str = r#"You are an expert code merge resolver. Given a 3-way merge conflict with:
- BASE: The common ancestor version
- OURS: The current branch changes
- THEIRS: The incoming branch changes

Produce a merged result that:
1. Preserves the intent of BOTH branches when possible
2. Uses semantic understanding of code, not just text manipulation
3. Maintains code correctness and consistency

Output:
- resolved_content: The final merged code (complete, ready to save)
- strategy: One of "semantic_merge" (combined both), "prefer_ours", "prefer_theirs"
- explanation: Brief explanation of your merge decisions

If the changes are to different parts of the file, combine them.
If the changes conflict directly, prefer the approach that is more complete or correct.
Always ensure the output is valid code."#;
