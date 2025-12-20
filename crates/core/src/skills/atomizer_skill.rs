//! # Atomizer Skill
//!
//! A2A-native skill that breaks features into agent-sized modules.
//! Follows the "Rule of 100" - each module should be completable
//! in one agent conversation.

use crate::models::ModelConfig;
use crate::run_llm_function;
use crate::skills::artifact_registry::{AtomizationArtifact, ModuleSummary};
use async_trait::async_trait;
use radkit::agent::{Artifact, OnRequestResult, SkillHandler};
use radkit::errors::{AgentError, AgentResult};
use radkit::macros::{skill, LLMOutput};
use radkit::models::Content;
use radkit::runtime::context::{ProgressSender, State};
use radkit::runtime::AgentRuntime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A single atomic module in the plan
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct AtomicModule {
    /// File path relative to workspace root
    pub path: String,
    /// What this module is responsible for
    pub responsibility: String,
    /// Maximum allowed lines
    pub max_lines: u32,
    /// Public interface signatures
    pub public_interface: Vec<String>,
    /// Dependencies on other modules
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// A test module in the plan
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct TestModule {
    /// File path for the test module
    pub path: String,
    /// Which modules this test covers
    pub covers: Vec<String>,
    /// Estimated number of tests
    pub test_count_estimate: u32,
}

/// An integration point with existing code
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct IntegrationPoint {
    /// The module to modify
    pub module: String,
    /// The change to make
    pub change: String,
}

/// Output from the atomizer skill
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct AtomizerOutput {
    /// Feature ID being atomized
    pub feature_id: String,
    /// Feature name
    pub feature_name: String,
    /// List of atomic modules to create
    pub modules: Vec<AtomicModule>,
    /// Test modules to create
    #[serde(default)]
    pub test_modules: Vec<TestModule>,
    /// Integration points with existing code
    #[serde(default)]
    pub integration_points: Vec<IntegrationPoint>,
}

/// Atomizer skill for breaking features into modules
#[skill(
    id = "atomize",
    name = "Atomizer",
    description = "Breaks features into agent-sized modules following the Rule of 100. Each module is completable in one agent conversation.",
    tags = ["planning", "modular", "compile-time"],
    examples = ["Break down feature into modules", "Atomize this component"],
    input_modes = ["text/plain", "application/json"],
    output_modes = ["application/json"]
)]
pub struct AtomizerSkill {
    config: ModelConfig,
}

impl AtomizerSkill {
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
impl SkillHandler for AtomizerSkill {
    async fn on_request(
        &self,
        _state: &mut State,
        progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        let feature_request = content.first_text().unwrap_or_default();

        progress
            .send_update("Analyzing feature complexity...")
            .await?;

        progress
            .send_update("Breaking into atomic modules...")
            .await?;

        let result =
            run_llm_function!(&self.config, AtomizerOutput, SYSTEM_PROMPT, feature_request)
                .map_err(|e| AgentError::Internal {
                    component: "atomizer_skill".to_string(),
                    reason: e.to_string(),
                })?;

        let total_lines: u32 = result.modules.iter().map(|m| m.max_lines).sum();

        progress.send_update("Atomization complete.").await?;

        // Create artifact with atomization data
        let artifact_data = AtomizationArtifact {
            feature_id: result.feature_id.clone(),
            feature_name: result.feature_name.clone(),
            modules: result
                .modules
                .iter()
                .map(|m| ModuleSummary {
                    path: m.path.clone(),
                    responsibility: m.responsibility.clone(),
                    max_lines: m.max_lines,
                })
                .collect(),
            total_estimated_lines: total_lines,
        };

        let artifact = Artifact::from_json("atomization.json", &artifact_data).map_err(|e| {
            AgentError::Internal {
                component: "atomizer_skill".to_string(),
                reason: format!("Failed to create artifact: {}", e),
            }
        })?;

        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text(&format!(
                "Atomized into {} modules (~{} lines total)",
                result.modules.len(),
                total_lines
            ))),
            artifacts: vec![artifact],
        })
    }
}

const SYSTEM_PROMPT: &str = include_str!("defaults/atomizer.md");
