//! # Orchestrator Skill
//!
//! A2A-native skill that coordinates the entire agent pipeline.
//! This is the meta-skill that manages the high-level state machine,
//! delegating to specialized skills as needed.
//!
//! ## Pipeline Stages
//!
//! 1. Parse → 2. Research → 3. Architect → 4. Critic → 5. Atomize → 6. Build

use crate::models::ModelConfig;
use crate::skills::{ArchitectSkill, CriticSkill, ParseSkill, ResearcherSkill};
use async_trait::async_trait;
use radkit::agent::{Artifact, OnRequestResult, SkillHandler, SkillSlot};
use radkit::errors::{AgentError, AgentResult};
use radkit::macros::skill;
use radkit::models::Content;
use radkit::runtime::context::{ProgressSender, State};
use radkit::runtime::AgentRuntime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Pipeline stage for orchestration state machine
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationStage {
    /// Initial: parsing user goal for unknowns
    Parsing,
    /// Researching solutions for identified unknowns  
    Researching,
    /// Making architectural decisions
    Architecting,
    /// Critic reviewing decisions
    Critiquing,
    /// Breaking features into modules
    Atomizing,
    /// Generating mission prompts
    Tasking,
    /// Building/implementing code
    Building,
    /// Pipeline complete
    Complete,
    /// Waiting for human input
    WaitingForHuman,
    /// Pipeline failed
    Failed,
}

/// Input slot for human approval during critiquing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrchestratorSlot {
    /// Waiting for human to approve rejected decisions
    PendingCriticApproval,
}

/// Saved state when awaiting approval
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingState {
    pub stage: OrchestrationStage,
    pub feature_id: String,
    pub goal: String,
    pub unknowns: Option<serde_json::Value>,
    pub research: Option<serde_json::Value>,
    pub decisions: Option<serde_json::Value>,
}

/// Output from the orchestrator
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrchestratorOutput {
    /// Current stage
    pub stage: OrchestrationStage,
    /// Feature ID being processed
    pub feature_id: String,
    /// Summary of what was accomplished
    pub summary: String,
    /// Whether pipeline completed successfully
    pub success: bool,
}

/// Orchestrator skill that coordinates the entire agent pipeline
#[skill(
    id = "orchestrator",
    name = "Pipeline Orchestrator",
    description = "Coordinates the agent pipeline from user goal to completed feature. \
                   Manages state transitions and delegates to specialized skills: \
                   Parse → Research → Architect → Critic → Atomize → Build",
    tags = ["orchestration", "pipeline", "meta-agent"],
    examples = [
        "Build a REST API with user authentication",
        "Add WebSocket support to the server",
        "Implement caching layer for database queries"
    ],
    input_modes = ["text/plain", "application/json"],
    output_modes = ["application/json"]
)]
pub struct OrchestratorSkill {
    config: ModelConfig,
}

impl OrchestratorSkill {
    pub fn new(config: ModelConfig) -> Self {
        Self { config }
    }

    /// Generate a feature ID
    fn generate_feature_id() -> String {
        format!(
            "feat-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        )
    }

    /// Create a completed result with artifact
    fn create_result(
        stage: OrchestrationStage,
        feature_id: String,
        summary: &str,
    ) -> AgentResult<OnRequestResult> {
        let output = OrchestratorOutput {
            stage,
            feature_id,
            summary: summary.to_string(),
            success: true,
        };

        let artifact = Artifact::from_json("orchestrator_output.json", &output).map_err(|e| {
            AgentError::Internal {
                component: "orchestrator".to_string(),
                reason: e.to_string(),
            }
        })?;

        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text(summary)),
            artifacts: vec![artifact],
        })
    }
}

#[async_trait]
impl SkillHandler for OrchestratorSkill {
    async fn on_request(
        &self,
        state: &mut State,
        progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        let goal = content
            .first_text()
            .ok_or_else(|| AgentError::MissingInput("No goal provided".to_string()))?;

        let feature_id = Self::generate_feature_id();

        progress
            .send_update("Starting pipeline: parsing goal for unknowns...")
            .await?;

        // === Stage 1: Parse ===
        let parse_result =
            ParseSkill::run(goal, &self.config)
                .await
                .map_err(|e| AgentError::Internal {
                    component: "orchestrator".to_string(),
                    reason: format!("Parse failed: {}", e),
                })?;

        let unknown_count = parse_result.ambiguities.len();

        if unknown_count == 0 {
            // No unknowns - can skip to building
            progress
                .send_update("No ambiguities found. Ready to build.")
                .await?;

            return Self::create_result(
                OrchestrationStage::Building,
                feature_id,
                "No ambiguities found. Ready to build.",
            );
        }

        progress
            .send_update(&format!(
                "Found {} ambiguities. Researching solutions...",
                unknown_count
            ))
            .await?;

        // === Stage 2: Research ===
        let mut research_results = Vec::new();

        for ambiguity in &parse_result.ambiguities {
            progress
                .send_update(&format!("Researching: {}", ambiguity.question))
                .await
                .ok();

            let result = ResearcherSkill::run(
                &ambiguity.id,
                &ambiguity.question,
                ambiguity.context.as_deref().unwrap_or(""),
                &self.config,
            )
            .await
            .map_err(|e| AgentError::Internal {
                component: "orchestrator".to_string(),
                reason: format!("Research failed for {}: {}", ambiguity.id, e),
            })?;

            research_results.push(result);
        }

        progress
            .send_update("Research complete. Making architectural decisions...")
            .await?;

        // === Stage 3: Architect ===
        let mut decisions = Vec::new();

        for (ambiguity, research_result) in
            parse_result.ambiguities.iter().zip(research_results.iter())
        {
            progress
                .send_update(&format!("Deciding: {}", ambiguity.question))
                .await
                .ok();

            let research_json = serde_json::to_string_pretty(research_result).unwrap_or_default();

            let decision = ArchitectSkill::run(
                &ambiguity.id,
                &research_json,
                "", // Would load spec here
                "lab",
                &self.config,
            )
            .await
            .map_err(|e| AgentError::Internal {
                component: "orchestrator".to_string(),
                reason: format!("Architect failed for {}: {}", ambiguity.id, e),
            })?;

            decisions.push(decision);
        }

        progress
            .send_update("Decisions made. Running critic review...")
            .await?;

        // === Stage 4: Critic ===
        let mut all_approved = true;
        let mut rejected_count = 0;

        for decision in &decisions {
            let decision_json = serde_json::to_string_pretty(decision).unwrap_or_default();

            let verdict = CriticSkill::run(&decision_json, "", "lab", &self.config)
                .await
                .map_err(|e| AgentError::Internal {
                    component: "orchestrator".to_string(),
                    reason: format!("Critic failed: {}", e),
                })?;

            if verdict.verdict != "approved" {
                all_approved = false;
                rejected_count += 1;
            }
        }

        if all_approved {
            progress
                .send_update("All decisions approved. Ready to atomize and build.")
                .await?;

            Self::create_result(
                OrchestrationStage::Atomizing,
                feature_id,
                &format!(
                    "Pipeline complete: {} unknowns resolved, {} decisions approved. Ready to atomize.",
                    unknown_count,
                    decisions.len()
                ),
            )
        } else {
            // Save state for human review
            let pending = PendingState {
                stage: OrchestrationStage::WaitingForHuman,
                feature_id: feature_id.clone(),
                goal: goal.to_string(),
                unknowns: serde_json::to_value(&parse_result).ok(),
                research: serde_json::to_value(&research_results).ok(),
                decisions: serde_json::to_value(&decisions).ok(),
            };

            state
                .task()
                .save("pending_state", &pending)
                .map_err(|e| AgentError::Internal {
                    component: "orchestrator".to_string(),
                    reason: format!("Failed to save state: {}", e),
                })?;

            state
                .set_slot(OrchestratorSlot::PendingCriticApproval)
                .map_err(|e| AgentError::Internal {
                    component: "orchestrator".to_string(),
                    reason: format!("Failed to set slot: {}", e),
                })?;

            progress
                .send_update("Waiting for human approval...")
                .await?;

            Ok(OnRequestResult::InputRequired {
                message: Content::from_text(&format!(
                    "⚠️ **Critic rejected {} decisions**\n\n\
                     Please review and reply **approve** to proceed anyway, or **reject** to start over.",
                    rejected_count
                )),
                slot: SkillSlot::new(OrchestratorSlot::PendingCriticApproval),
            })
        }
    }

    async fn on_input_received(
        &self,
        state: &mut State,
        progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<radkit::agent::OnInputResult> {
        let input = content.first_text().unwrap_or_default().to_lowercase();

        // Load pending state
        let pending: Option<PendingState> =
            state
                .task()
                .load("pending_state")
                .map_err(|e| AgentError::Internal {
                    component: "orchestrator".to_string(),
                    reason: format!("Failed to load state: {}", e),
                })?;

        let pending = pending.ok_or_else(|| AgentError::Internal {
            component: "orchestrator".to_string(),
            reason: "No pending state found".to_string(),
        })?;

        if input.contains("approve") || input.contains("yes") || input.contains("proceed") {
            state.clear_slot();

            progress
                .send_update("Approved - proceeding to atomize and build...")
                .await?;

            let output = OrchestratorOutput {
                stage: OrchestrationStage::Atomizing,
                feature_id: pending.feature_id,
                summary: "Human approved: proceeding to atomize and build.".to_string(),
                success: true,
            };

            let artifact =
                Artifact::from_json("orchestrator_output.json", &output).map_err(|e| {
                    AgentError::Internal {
                        component: "orchestrator".to_string(),
                        reason: e.to_string(),
                    }
                })?;

            Ok(radkit::agent::OnInputResult::Completed {
                message: Some(Content::from_text(&output.summary)),
                artifacts: vec![artifact],
            })
        } else if input.contains("reject") || input.contains("no") || input.contains("stop") {
            state.clear_slot();

            progress.send_update("Rejected - pipeline stopped.").await?;

            Ok(radkit::agent::OnInputResult::Failed {
                error: Content::from_text(
                    "Pipeline rejected by user. Please refine the goal and try again.",
                ),
            })
        } else {
            // Unclear response, ask again
            Ok(radkit::agent::OnInputResult::InputRequired {
                message: Content::from_text(
                    "Please reply **approve** to proceed with the rejected decisions, or **reject** to stop the pipeline.",
                ),
                slot: SkillSlot::new(OrchestratorSlot::PendingCriticApproval),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_id_generation() {
        let id = OrchestratorSkill::generate_feature_id();
        assert!(id.starts_with("feat-"));
    }

    #[test]
    fn test_output_serialization() {
        let output = OrchestratorOutput {
            stage: OrchestrationStage::Complete,
            feature_id: "feat-123".to_string(),
            summary: "Done".to_string(),
            success: true,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("complete"));
    }
}
