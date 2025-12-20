//! # Researcher Skill
//!
//! A2A-native skill that researches solutions for unknowns.
//! Uses search_tools to find crates and documentation.

use crate::models::ModelConfig;
use crate::run_llm_worker;
use crate::skills::artifact_registry::{OptionSummary, ResearchArtifact};
use crate::skills::tools::search_tools;
use async_trait::async_trait;
use radkit::agent::{Artifact, OnRequestResult, SkillHandler};
use radkit::errors::{AgentError, AgentResult};
use radkit::macros::{skill, LLMOutput};
use radkit::models::Content;
use radkit::runtime::context::{ProgressSender, State};
use radkit::runtime::AgentRuntime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A single research option
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct ResearchOption {
    /// Name of the solution
    pub name: String,
    /// Brief description
    pub description: String,
    /// Pros of this option
    pub pros: Vec<String>,
    /// Cons of this option
    pub cons: Vec<String>,
    /// Estimated complexity (1-10)
    pub complexity: u32,
}

/// Output from the researcher skill
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct ResearchOutput {
    /// ID of the unknown being researched
    pub unknown_id: String,
    /// Research options found
    pub options: Vec<ResearchOption>,
    /// Summary of the research
    pub summary: String,
    /// Recommended option (if any)
    #[serde(default)]
    pub recommended: Option<String>,
}

/// Researcher skill for exploring solutions
#[skill(
    id = "research",
    name = "Research",
    description = "Researches solutions for unknowns using web search and crate search. Returns options with pros/cons.",
    tags = ["research", "search", "analysis"],
    examples = ["Research database options", "Find crates for authentication"],
    input_modes = ["text/plain", "application/json"],
    output_modes = ["application/json"]
)]
pub struct ResearcherSkill {
    config: ModelConfig,
}

impl ResearcherSkill {
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
    /// Bypasses radkit runtime, calls LLM with tools directly.
    pub async fn run(
        unknown_id: &str,
        question: &str,
        context: &str,
        config: &ModelConfig,
    ) -> anyhow::Result<ResearchOutput> {
        let prompt = format!(
            "Research Unknown: {}\n\nQuestion: {}\n\nContext: {}",
            unknown_id, question, context
        );
        run_llm_worker!(
            config,
            ResearchOutput,
            SYSTEM_PROMPT,
            prompt,
            search_tools::search_crates,
            search_tools::search_web,
        )
    }
}

#[async_trait]
impl SkillHandler for ResearcherSkill {
    async fn on_request(
        &self,
        _state: &mut State,
        progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        let research_question = content.first_text().unwrap_or_default();

        progress.send_update("Starting research...").await?;

        progress.send_update("Searching for solutions...").await?;

        let result = run_llm_worker!(
            &self.config,
            ResearchOutput,
            SYSTEM_PROMPT,
            research_question,
            search_tools::search_crates,
            search_tools::search_web,
        )
        .map_err(|e| AgentError::Internal {
            component: "researcher_skill".to_string(),
            reason: e.to_string(),
        })?;

        progress.send_update("Research complete.").await?;

        // Create artifact with summary data
        let artifact_data = ResearchArtifact {
            unknown_id: result.unknown_id.clone(),
            options: result
                .options
                .iter()
                .map(|o| OptionSummary {
                    name: o.name.clone(),
                    description: o.description.clone(),
                    complexity: o.complexity,
                    pros_count: o.pros.len(),
                    cons_count: o.cons.len(),
                })
                .collect(),
            recommended: result.recommended.clone(),
            summary: result.summary.clone(),
        };

        let artifact = Artifact::from_json("research.json", &artifact_data).map_err(|e| {
            AgentError::Internal {
                component: "researcher_skill".to_string(),
                reason: format!("Failed to create artifact: {}", e),
            }
        })?;

        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text(&result.summary)),
            artifacts: vec![artifact],
        })
    }
}

const SYSTEM_PROMPT: &str = include_str!("defaults/researcher.md");
