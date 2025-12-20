//! # Parse Skill (UnknownsParser)
//!
//! A2A-native skill that parses user goals and identifies ambiguities.
//! This is the first step in "Compile-Time Intelligence" - identifying
//! unknowns that must be resolved BEFORE code generation.

use crate::models::ModelConfig;
use crate::run_llm_function;
use crate::skills::artifact_registry::{AmbiguitySummary, InferredKnownSummary, UnknownsArtifact};
use crate::state::CodebaseProfile;
use async_trait::async_trait;
use radkit::agent::{Artifact, OnRequestResult, SkillHandler};
use radkit::errors::{AgentError, AgentResult};
use radkit::macros::{skill, LLMOutput};
use radkit::models::Content;
use radkit::runtime::context::{ProgressSender, State};
use radkit::runtime::AgentRuntime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Category of ambiguity
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput, PartialEq, Eq)]
pub enum AmbiguityCategory {
    Infrastructure,
    Logic,
    Security,
    #[serde(rename = "UX")]
    Ux,
}

/// Criticality level of an ambiguity
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput, PartialEq, Eq)]
pub enum Criticality {
    #[serde(rename = "BLOCKER")]
    Blocker,
    #[serde(rename = "HIGH")]
    High,
    #[serde(rename = "LOW")]
    Low,
}

/// A single ambiguity identified by the parser
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct Ambiguity {
    pub id: String,
    pub category: AmbiguityCategory,
    pub question: String,
    pub criticality: Criticality,
    #[serde(default)]
    pub context: Option<String>,
}

/// An ambiguity that was auto-resolved from codebase analysis
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InferredKnown {
    pub id: String,
    pub category: AmbiguityCategory,
    /// The question that was resolved
    pub original_question: String,
    /// How it was resolved
    pub resolution: String,
    /// Evidence from codebase (file path, line, symbol)
    pub evidence: String,
}

/// Output from the parse skill
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct ParseOutput {
    pub ambiguities: Vec<Ambiguity>,
}

/// Extended output including inferred knowns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseOutputWithKnowns {
    pub ambiguities: Vec<Ambiguity>,
    pub inferred_knowns: Vec<InferredKnown>,
}

/// Alias for backward compatibility with coordinator and legacy code
pub type UnknownsParserOutput = ParseOutput;

/// Parse skill for identifying ambiguities in user goals
#[skill(
    id = "parse",
    name = "Parse Unknowns",
    description = "Parses user goals and identifies ambiguities that must be resolved before code generation. First step in Compile-Time Intelligence.",
    tags = ["parsing", "analysis", "compile-time"],
    examples = ["Analyze goal for unknowns", "Identify ambiguities"],
    input_modes = ["text/plain", "application/json"],
    output_modes = ["application/json"]
)]
pub struct ParseSkill {
    config: ModelConfig,
}

impl ParseSkill {
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
    /// Bypasses radkit runtime, calls LLM directly.
    pub async fn run(goal: &str, config: &ModelConfig) -> anyhow::Result<ParseOutput> {
        run_llm_function!(config, ParseOutput, SYSTEM_PROMPT, goal)
    }

    /// Run with codebase profile to auto-resolve knowns
    pub async fn run_with_profile(
        goal: &str,
        config: &ModelConfig,
        profile: Option<&CodebaseProfile>,
    ) -> anyhow::Result<ParseOutputWithKnowns> {
        // Get LLM-identified ambiguities
        let output = Self::run(goal, config).await?;

        // Try to resolve ambiguities from profile
        let inferred_knowns = if let Some(profile) = profile {
            Self::resolve_from_profile(&output.ambiguities, profile)
        } else {
            Vec::new()
        };

        // Remove resolved ambiguities
        let resolved_ids: Vec<_> = inferred_knowns.iter().map(|k| k.id.as_str()).collect();
        let remaining: Vec<_> = output
            .ambiguities
            .into_iter()
            .filter(|a| !resolved_ids.contains(&a.id.as_str()))
            .collect();

        Ok(ParseOutputWithKnowns {
            ambiguities: remaining,
            inferred_knowns,
        })
    }

    /// Try to resolve ambiguities against codebase profile
    fn resolve_from_profile(
        ambiguities: &[Ambiguity],
        profile: &CodebaseProfile,
    ) -> Vec<InferredKnown> {
        let mut knowns = Vec::new();
        let question_lower = |q: &str| q.to_lowercase();

        for ambiguity in ambiguities {
            let q = question_lower(&ambiguity.question);

            // Check for database questions
            if q.contains("database") || q.contains("db") || q.contains("storage") {
                if let Some(db_framework) =
                    profile.frameworks.iter().find(|f| f.category == "database")
                {
                    knowns.push(InferredKnown {
                        id: ambiguity.id.clone(),
                        category: ambiguity.category.clone(),
                        original_question: ambiguity.question.clone(),
                        resolution: format!(
                            "Using {} (detected from dependencies)",
                            db_framework.name
                        ),
                        evidence: format!("Cargo.toml contains {}", db_framework.name),
                    });
                    continue;
                }
            }

            // Check for web framework questions
            if q.contains("framework") || q.contains("web") || q.contains("api") {
                if let Some(web_framework) = profile.frameworks.iter().find(|f| f.category == "web")
                {
                    knowns.push(InferredKnown {
                        id: ambiguity.id.clone(),
                        category: ambiguity.category.clone(),
                        original_question: ambiguity.question.clone(),
                        resolution: format!(
                            "Using {} (detected from dependencies)",
                            web_framework.name
                        ),
                        evidence: format!("Cargo.toml contains {}", web_framework.name),
                    });
                    continue;
                }
            }

            // Check for async runtime questions
            if q.contains("async") || q.contains("runtime") {
                if let Some(ref runtime) = profile.style_patterns.async_runtime {
                    knowns.push(InferredKnown {
                        id: ambiguity.id.clone(),
                        category: ambiguity.category.clone(),
                        original_question: ambiguity.question.clone(),
                        resolution: format!("Using {} async runtime", runtime),
                        evidence: format!("Detected {} in project dependencies", runtime),
                    });
                    continue;
                }
            }

            // Check for error handling questions
            if q.contains("error") || q.contains("handling") {
                if let Some(ref error_handling) = profile.style_patterns.error_handling {
                    knowns.push(InferredKnown {
                        id: ambiguity.id.clone(),
                        category: ambiguity.category.clone(),
                        original_question: ambiguity.question.clone(),
                        resolution: format!("Using {} error handling pattern", error_handling),
                        evidence: "Detected from existing error handling patterns".to_string(),
                    });
                }
            }
        }

        knowns
    }
}

#[async_trait]
impl SkillHandler for ParseSkill {
    async fn on_request(
        &self,
        _state: &mut State,
        progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        let goal = content.first_text().unwrap_or_default();

        progress
            .send_update("Analyzing goal for unknowns...")
            .await?;

        progress.send_update("Identifying ambiguities...").await?;

        // Use the macro for multi-provider support, convert error type
        let result =
            run_llm_function!(&self.config, ParseOutput, SYSTEM_PROMPT, goal).map_err(|e| {
                AgentError::Internal {
                    component: "parse_skill".to_string(),
                    reason: e.to_string(),
                }
            })?;

        let count = result.ambiguities.len();
        let blockers = result
            .ambiguities
            .iter()
            .filter(|a| a.criticality == Criticality::Blocker)
            .count();

        progress.send_update("Parse complete.").await?;

        // Create artifact with summary data
        let artifact_data = UnknownsArtifact {
            ambiguities: result
                .ambiguities
                .iter()
                .map(|a| AmbiguitySummary {
                    id: a.id.clone(),
                    category: format!("{:?}", a.category),
                    question: a.question.clone(),
                    criticality: format!("{:?}", a.criticality),
                })
                .collect(),
            total_count: count,
            blocker_count: blockers,
            inferred_count: 0, // A2A handler doesn't use profile resolution
        };

        let artifact = Artifact::from_json("unknowns.json", &artifact_data).map_err(|e| {
            AgentError::Internal {
                component: "parse_skill".to_string(),
                reason: format!("Failed to create artifact: {}", e),
            }
        })?;

        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text(&format!(
                "Found {} ambiguities ({} blockers)",
                count, blockers
            ))),
            artifacts: vec![artifact],
        })
    }
}

const SYSTEM_PROMPT: &str = include_str!("defaults/unknowns_parser.md");
