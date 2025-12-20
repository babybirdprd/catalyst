//! # Critic Skill
//!
//! A2A-native skill that reviews decisions and code.
//! Provides feedback and approval/rejection verdicts.

use crate::models::ModelConfig;
use crate::run_llm_function;
use crate::skills::artifact_registry::{ConcernSummary, ReviewArtifact};
use async_trait::async_trait;
use radkit::agent::{Artifact, OnRequestResult, SkillHandler};
use radkit::errors::{AgentError, AgentResult};
use radkit::macros::{skill, LLMOutput};
use radkit::models::Content;
use radkit::runtime::context::{ProgressSender, State};
use radkit::runtime::AgentRuntime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A single concern raised by the critic
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct Concern {
    /// Severity: "blocking", "major", "minor", "suggestion"
    pub severity: String,
    /// Description of the concern
    pub description: String,
    /// Suggested fix if applicable
    #[serde(default)]
    pub suggested_fix: Option<String>,
}

/// Output from the critic skill
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct CriticOutput {
    /// Overall verdict: "approved", "needs_changes", "rejected"
    pub verdict: String,
    /// Summary of the review
    pub summary: String,
    /// List of concerns found
    pub concerns: Vec<Concern>,
    /// Confidence in the verdict (0.0 - 1.0)
    pub confidence: f32,
}

/// Critic skill for reviewing decisions and code
#[skill(
    id = "review",
    name = "Review",
    description = "Reviews architectural decisions and code changes for quality and correctness.",
    tags = ["review", "quality", "feedback"],
    examples = ["Review this database decision", "Check code quality"],
    input_modes = ["text/plain", "application/json"],
    output_modes = ["application/json"]
)]
pub struct CriticSkill {
    config: ModelConfig,
}

impl CriticSkill {
    pub fn new(config: ModelConfig) -> Self {
        Self { config }
    }

    pub fn with_model(model: &str) -> Self {
        Self::new(ModelConfig::new(model))
    }

    pub fn default() -> Self {
        Self::new(ModelConfig::default())
    }

    /// SDK-style call for direct Coordinator integration.
    pub async fn run(
        decision_json: &str,
        spec_context: &str,
        mode: &str,
        config: &ModelConfig,
    ) -> anyhow::Result<CriticOutput> {
        let prompt = format!(
            "Decision to Review:\n{}\n\nSpec Context:\n{}\n\nMode: {}",
            decision_json, spec_context, mode
        );
        run_llm_function!(config, CriticOutput, SYSTEM_PROMPT, prompt)
    }
}

#[async_trait]
impl SkillHandler for CriticSkill {
    async fn on_request(
        &self,
        _state: &mut State,
        progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        let input = content.first_text().unwrap_or_default();

        progress.send_update("Reviewing decision...").await?;

        let result =
            run_llm_function!(&self.config, CriticOutput, SYSTEM_PROMPT, input).map_err(|e| {
                AgentError::Internal {
                    component: "critic_skill".to_string(),
                    reason: e.to_string(),
                }
            })?;

        progress.send_update("Review complete.").await?;

        // Create artifact with review data
        let blocking_count = result
            .concerns
            .iter()
            .filter(|c| c.severity == "blocking")
            .count();

        let artifact_data = ReviewArtifact {
            verdict: result.verdict.clone(),
            confidence: result.confidence,
            concerns: result
                .concerns
                .iter()
                .map(|c| ConcernSummary {
                    severity: c.severity.clone(),
                    description: c.description.clone(),
                })
                .collect(),
            blocking_count,
        };

        let artifact = Artifact::from_json("review.json", &artifact_data).map_err(|e| {
            AgentError::Internal {
                component: "critic_skill".to_string(),
                reason: format!("Failed to create artifact: {}", e),
            }
        })?;

        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text(&format!(
                "Verdict: {} ({} concerns)",
                result.verdict,
                result.concerns.len()
            ))),
            artifacts: vec![artifact],
        })
    }
}

const SYSTEM_PROMPT: &str = include_str!("defaults/critic.md");
