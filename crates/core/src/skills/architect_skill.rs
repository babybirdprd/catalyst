//! # Architect Skill
//!
//! A2A-native skill that makes architectural decisions.
//! Takes research options and current context to decide on the best approach.
//!
//! ## Approval Flow (A2A Pattern)
//!
//! When `require_approval` is true:
//! 1. `on_request` generates decision, saves to state, returns `InputRequired`
//! 2. User responds with "approve", "reject", or feedback
//! 3. `on_input_received` handles response and completes or rejects

use crate::models::ModelConfig;
use crate::run_llm_function;
use crate::skills::artifact_registry::{DecisionArtifact, SpecUpdateSummary};
use async_trait::async_trait;
use radkit::agent::{Artifact, OnInputResult, OnRequestResult, SkillHandler, SkillSlot};
use radkit::errors::{AgentError, AgentResult};
use radkit::macros::{skill, LLMOutput};
use radkit::models::Content;
use radkit::runtime::context::{ProgressSender, State};
use radkit::runtime::AgentRuntime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Slots for multi-turn approval flow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArchitectSlot {
    /// Waiting for user approval of a decision
    ApprovalRequired,
}

/// A spec update from an architectural decision
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct SpecUpdate {
    /// Section of the spec to update
    pub section: String,
    /// New content for that section
    pub content: String,
}

/// Output from the architect skill
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct ArchitectOutput {
    /// ID of the unknown being decided
    pub unknown_id: String,
    /// The chosen option
    pub chosen_option: String,
    /// Rationale for the decision
    pub rationale: String,
    /// Spec updates needed
    pub spec_updates: Vec<SpecUpdate>,
    /// Any dependencies to add
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Architect skill for making design decisions
#[skill(
    id = "architect",
    name = "Architect",
    description = "Makes architectural decisions based on research and project context.",
    tags = ["architecture", "design", "decisions"],
    examples = ["Decide between database options", "Choose authentication approach"],
    input_modes = ["text/plain", "application/json"],
    output_modes = ["application/json"]
)]
pub struct ArchitectSkill {
    config: ModelConfig,
    /// Whether to require human approval before completing
    require_approval: bool,
}

impl ArchitectSkill {
    pub fn new(config: ModelConfig) -> Self {
        Self {
            config,
            require_approval: false,
        }
    }

    pub fn with_model(model: &str) -> Self {
        Self::new(ModelConfig::new(model))
    }

    pub fn default() -> Self {
        Self::new(ModelConfig::default())
    }

    /// Enable approval requirement for this skill
    pub fn with_approval(mut self, require: bool) -> Self {
        self.require_approval = require;
        self
    }

    /// SDK-style call for direct Coordinator integration.
    pub async fn run(
        unknown_id: &str,
        research_json: &str,
        spec_context: &str,
        mode: &str,
        config: &ModelConfig,
    ) -> anyhow::Result<ArchitectOutput> {
        let prompt = format!(
            "Unknown ID: {}\n\nResearch Results:\n{}\n\nSpec Context:\n{}\n\nMode: {}",
            unknown_id, research_json, spec_context, mode
        );
        run_llm_function!(config, ArchitectOutput, SYSTEM_PROMPT, prompt)
    }
}

#[async_trait]
impl SkillHandler for ArchitectSkill {
    async fn on_request(
        &self,
        state: &mut State,
        progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        let input = content.first_text().unwrap_or_default();

        progress.send_update("Analyzing options...").await?;

        let result = run_llm_function!(&self.config, ArchitectOutput, SYSTEM_PROMPT, input)
            .map_err(|e| AgentError::Internal {
                component: "architect_skill".to_string(),
                reason: e.to_string(),
            })?;

        progress.send_update("Decision made.").await?;

        // If approval required, save decision and return InputRequired
        if self.require_approval {
            // Save pending decision to task state
            state.task().save("pending_decision", &result)?;

            return Ok(OnRequestResult::InputRequired {
                message: Content::from_text(&format!(
                    "**Approval Required**\n\n\
                    **Decision:** {}\n\n\
                    **Rationale:** {}\n\n\
                    **Spec Updates:** {} sections\n\n\
                    Reply with 'approve' to accept, 'reject' to decline, or provide feedback.",
                    result.chosen_option,
                    result.rationale,
                    result.spec_updates.len()
                )),
                slot: SkillSlot::new(ArchitectSlot::ApprovalRequired),
            });
        }

        // No approval needed - complete directly
        complete_with_decision(result)
    }

    async fn on_input_received(
        &self,
        state: &mut State,
        _progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<OnInputResult> {
        // Get the slot we're waiting on
        let slot: ArchitectSlot = state
            .slot()?
            .ok_or_else(|| AgentError::SkillSlot("No slot found".to_string()))?;

        match slot {
            ArchitectSlot::ApprovalRequired => {
                // Load pending decision
                let decision: ArchitectOutput =
                    state.task().load("pending_decision")?.ok_or_else(|| {
                        AgentError::ContextError("No pending decision found".to_string())
                    })?;

                let response = content.first_text().unwrap_or_default().to_lowercase();

                if response.contains("approve") || response.contains("yes") || response == "y" {
                    // Approved - create artifact and complete
                    let artifact_data = DecisionArtifact {
                        unknown_id: decision.unknown_id.clone(),
                        chosen_option: decision.chosen_option.clone(),
                        rationale: decision.rationale.clone(),
                        spec_updates: decision
                            .spec_updates
                            .iter()
                            .map(|s| SpecUpdateSummary {
                                section: s.section.clone(),
                                action: "modify".to_string(),
                            })
                            .collect(),
                        dependencies_added: decision.dependencies.clone(),
                    };

                    let artifact = Artifact::from_json("decision.json", &artifact_data)?;

                    Ok(OnInputResult::Completed {
                        message: Some(Content::from_text(&format!(
                            "Decision approved: {}",
                            decision.chosen_option
                        ))),
                        artifacts: vec![artifact],
                    })
                } else if response.contains("reject") || response.contains("no") || response == "n"
                {
                    // Rejected
                    Ok(OnInputResult::Failed {
                        error: Content::from_text("Decision rejected by user."),
                    })
                } else {
                    // Feedback provided - could loop back, for now just reject
                    Ok(OnInputResult::Failed {
                        error: Content::from_text(&format!(
                            "Decision not approved. User feedback: {}",
                            response
                        )),
                    })
                }
            }
        }
    }
}

/// Helper to create completed result with decision artifact
fn complete_with_decision(result: ArchitectOutput) -> AgentResult<OnRequestResult> {
    let artifact_data = DecisionArtifact {
        unknown_id: result.unknown_id.clone(),
        chosen_option: result.chosen_option.clone(),
        rationale: result.rationale.clone(),
        spec_updates: result
            .spec_updates
            .iter()
            .map(|s| SpecUpdateSummary {
                section: s.section.clone(),
                action: "modify".to_string(),
            })
            .collect(),
        dependencies_added: result.dependencies.clone(),
    };

    let artifact =
        Artifact::from_json("decision.json", &artifact_data).map_err(|e| AgentError::Internal {
            component: "architect_skill".to_string(),
            reason: format!("Failed to create artifact: {}", e),
        })?;

    Ok(OnRequestResult::Completed {
        message: Some(Content::from_text(&format!(
            "Decision: {} ({})",
            result.chosen_option, result.rationale
        ))),
        artifacts: vec![artifact],
    })
}

const SYSTEM_PROMPT: &str = include_str!("defaults/architect.md");
