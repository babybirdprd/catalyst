//! # WebScraper Skill
//!
//! A2A-native utility skill for cleaning HTML into plain text.
//! Uses a cheaper/faster model for efficiency.
//! Called by ResearcherSkill to extract content from web pages.

use crate::models::ModelConfig;
use crate::run_llm_function;
use async_trait::async_trait;
use radkit::agent::{OnRequestResult, SkillHandler};
use radkit::errors::{AgentError, AgentResult};
use radkit::macros::{skill, LLMOutput};
use radkit::models::Content;
use radkit::runtime::context::{ProgressSender, State};
use radkit::runtime::AgentRuntime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Output from the WebScraper skill
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct ScrapedContent {
    /// Clean, extracted text from the HTML
    pub text: String,
    /// Main title if found
    #[serde(default)]
    pub title: Option<String>,
    /// Key points extracted
    #[serde(default)]
    pub key_points: Vec<String>,
    /// Whether the content is relevant to coding/development
    pub is_relevant: bool,
}

/// WebScraper skill for cleaning HTML
#[skill(
    id = "scrape",
    name = "WebScraper",
    description = "Cleans HTML into plain text for research. Extracts key points and filters irrelevant content.",
    tags = ["scraping", "html", "utility"],
    examples = ["Clean this HTML", "Extract content from webpage"],
    input_modes = ["text/plain", "text/html", "application/json"],
    output_modes = ["application/json"]
)]
pub struct WebScraperSkill {
    config: ModelConfig,
}

impl WebScraperSkill {
    pub fn new(config: ModelConfig) -> Self {
        Self { config }
    }

    pub fn with_model(model: &str) -> Self {
        Self::new(ModelConfig::new(model))
    }

    /// Uses a cheaper model by default for efficiency
    pub fn default() -> Self {
        Self::new(ModelConfig::new("claude-3-haiku-20240307"))
    }
}

#[async_trait]
impl SkillHandler for WebScraperSkill {
    async fn on_request(
        &self,
        _state: &mut State,
        progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        let html_input = content.first_text().unwrap_or_default();

        progress.send_update("Cleaning HTML...").await?;

        // Truncate if too long (save tokens)
        let max_chars = 50000;
        let input = if html_input.len() > max_chars {
            format!("{}... [TRUNCATED]", &html_input[..max_chars])
        } else {
            html_input.to_string()
        };

        progress.send_update("Extracting content...").await?;

        let result = run_llm_function!(&self.config, ScrapedContent, SYSTEM_PROMPT, input)
            .map_err(|e| AgentError::Internal {
                component: "webscraper_skill".to_string(),
                reason: e.to_string(),
            })?;

        progress.send_update("Scrape complete.").await?;

        let relevance = if result.is_relevant {
            "relevant"
        } else {
            "not relevant"
        };

        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text(&format!(
                "{} ({} key points, {})",
                result.title.as_deref().unwrap_or("Untitled"),
                result.key_points.len(),
                relevance
            ))),
            artifacts: vec![],
        })
    }
}

const SYSTEM_PROMPT: &str = r#"You are a content extraction assistant for developer research.

Your job is to:
1. Extract the main text content from HTML (remove nav, ads, headers, footers)
2. Identify the page title
3. Extract 3-5 key points from the content
4. Determine if the content is relevant to software development

Focus on:
- Technical documentation
- Code examples
- API references
- Tutorial content

Ignore:
- Cookie notices
- Navigation menus
- Advertisements
- Social media widgets

Return clean, readable text useful for a developer researching a topic.
"#;
