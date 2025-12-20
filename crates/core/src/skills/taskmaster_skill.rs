//! # Taskmaster Skill
//!
//! A2A-native skill that bundles context into mission prompts.
//! The bridge between Compile-Time (planning) and Runtime (coding).
//! Generates precise, context-rich prompts for the Builder agent.

use crate::models::ModelConfig;
use crate::run_llm_function;
use crate::skills::artifact_registry::{MissionArtifact, TaskSummary};
use async_trait::async_trait;
use radkit::agent::{Artifact, OnRequestResult, SkillHandler};
use radkit::errors::{AgentError, AgentResult};
use radkit::macros::{skill, LLMOutput};
use radkit::models::Content;
use radkit::runtime::context::{ProgressSender, State};
use radkit::runtime::AgentRuntime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A task in the mission
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct MissionTask {
    /// Ordinal number
    pub number: u32,
    /// Action: "Create", "Modify", "Add"
    pub action: String,
    /// File path
    pub file_path: String,
    /// What to implement
    pub implementation: String,
    /// Code hints/patterns
    #[serde(default)]
    pub hints: Vec<String>,
}

/// Constraints for the mission
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct MissionConstraints {
    /// Max lines per file
    pub max_file_lines: u32,
    /// Max lines per function
    pub max_function_lines: u32,
    /// Required patterns
    #[serde(default)]
    pub required_patterns: Vec<String>,
    /// Forbidden patterns
    #[serde(default)]
    pub forbidden_patterns: Vec<String>,
}

/// The mission prompt output
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct MissionPrompt {
    /// Feature being implemented
    pub feature_name: String,
    /// High-level objective
    pub objective: String,
    /// List of tasks
    pub tasks: Vec<MissionTask>,
    /// Constraints to follow
    pub constraints: MissionConstraints,
    /// Drafting missions for parallel file generation (Speed Demon)
    #[serde(default)]
    pub drafting_missions: Vec<super::drafting_skill::DraftingMission>,
    /// Existing signatures to use (not recreate)
    #[serde(default)]
    pub existing_signatures: Vec<String>,
    /// Verification checklist
    #[serde(default)]
    pub verification: Vec<String>,
}

/// Taskmaster skill for generating mission prompts
#[skill(
    id = "mission",
    name = "Taskmaster",
    description = "Bundles context into mission prompts for coding agents. The bridge from compile-time planning to runtime execution.",
    tags = ["mission", "context", "bridge"],
    examples = ["Generate mission from atomic plan", "Create builder prompt"],
    input_modes = ["text/plain", "application/json"],
    output_modes = ["application/json"]
)]
pub struct TaskmasterSkill {
    config: ModelConfig,
}

impl TaskmasterSkill {
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
impl SkillHandler for TaskmasterSkill {
    async fn on_request(
        &self,
        _state: &mut State,
        progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        let context = content.first_text().unwrap_or_default();

        progress.send_update("Bundling context...").await?;

        progress.send_update("Generating mission prompt...").await?;

        let result = run_llm_function!(&self.config, MissionPrompt, SYSTEM_PROMPT, context)
            .map_err(|e| AgentError::Internal {
                component: "taskmaster_skill".to_string(),
                reason: e.to_string(),
            })?;

        progress.send_update("Mission ready.").await?;

        // Create artifact with mission data
        let artifact_data = MissionArtifact {
            feature_name: result.feature_name.clone(),
            objective: result.objective.clone(),
            tasks: result
                .tasks
                .iter()
                .map(|t| TaskSummary {
                    number: t.number,
                    action: t.action.clone(),
                    file_path: t.file_path.clone(),
                })
                .collect(),
            task_count: result.tasks.len(),
        };

        let artifact = Artifact::from_json("mission.json", &artifact_data).map_err(|e| {
            AgentError::Internal {
                component: "taskmaster_skill".to_string(),
                reason: format!("Failed to create artifact: {}", e),
            }
        })?;

        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text(&format!(
                "Mission '{}': {} tasks",
                result.feature_name,
                result.tasks.len()
            ))),
            artifacts: vec![artifact],
        })
    }
}

const SYSTEM_PROMPT: &str = include_str!("defaults/taskmaster.md");
