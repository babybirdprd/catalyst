//! # Catalyst Models
//!
//! Centralized LLM configuration types for the Catalyst system.
//! These types were extracted from the agents module to provide
//! a clean dependency for both skills and swarm orchestration.
//!
//! ## Reference Documentation
//! See `radkit_docs/docs/core-concepts/llm-providers.md` for Radkit LLM provider details.

use radkit::models::providers::{
    AnthropicLlm, DeepSeekLlm, GeminiLlm, GrokLlm, OpenAILlm, OpenRouterLlm,
};
use radkit::models::BaseLlm;
use serde::{Deserialize, Serialize};

/// Supported LLM providers
///
/// Maps to the providers documented in `radkit_docs/docs/core-concepts/llm-providers.md`:
/// - Anthropic (Claude) - `ANTHROPIC_API_KEY`
/// - OpenAI (GPT) - `OPENAI_API_KEY`
/// - Gemini (Google) - `GEMINI_API_KEY`
/// - OpenRouter (Gateway) - `OPENROUTER_API_KEY`
/// - Grok (xAI) - `XAI_API_KEY`
/// - DeepSeek - `DEEPSEEK_API_KEY`
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    #[default]
    Anthropic,
    #[serde(rename = "openai")]
    OpenAI,
    Gemini,
    OpenRouter,
    Grok,
    DeepSeek,
}

impl LlmProvider {
    /// Get all available providers
    pub fn all() -> Vec<LlmProvider> {
        vec![
            LlmProvider::Anthropic,
            LlmProvider::OpenAI,
            LlmProvider::Gemini,
            LlmProvider::OpenRouter,
            LlmProvider::Grok,
            LlmProvider::DeepSeek,
        ]
    }

    /// Display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            LlmProvider::Anthropic => "Anthropic",
            LlmProvider::OpenAI => "OpenAI",
            LlmProvider::Gemini => "Gemini",
            LlmProvider::OpenRouter => "OpenRouter",
            LlmProvider::Grok => "Grok",
            LlmProvider::DeepSeek => "DeepSeek",
        }
    }

    /// Whether this provider supports custom base URL
    pub fn supports_base_url(&self) -> bool {
        matches!(self, LlmProvider::OpenAI)
    }
}

/// Configuration for LLM model selection
///
/// Used throughout the Catalyst system to configure which LLM provider and model
/// to use for agent operations. Supports per-agent configuration overrides.
///
/// ## Example
/// ```rust,ignore
/// use catalyst_core::models::{ModelConfig, LlmProvider};
///
/// // Default Anthropic
/// let config = ModelConfig::default();
///
/// // Specific provider and model
/// let config = ModelConfig::with_provider(LlmProvider::OpenAI, "gpt-4o");
///
/// // Create LLM client
/// let llm = config.create_llm()?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// LLM provider to use
    #[serde(default)]
    pub provider: LlmProvider,
    /// Model name (e.g., "claude-sonnet-4-20250514", "gpt-4o")
    pub model: String,
    /// Optional base URL override for OpenAI-compatible APIs
    pub base_url: Option<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::Anthropic,
            model: "claude-sonnet-4-20250514".to_string(),
            base_url: None,
        }
    }
}

impl ModelConfig {
    /// Create a new model config with default provider (Anthropic)
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            provider: LlmProvider::Anthropic,
            model: model.into(),
            base_url: None,
        }
    }

    /// Create config for a specific provider
    pub fn with_provider(provider: LlmProvider, model: impl Into<String>) -> Self {
        Self {
            provider,
            model: model.into(),
            base_url: None,
        }
    }

    /// Set base URL (for OpenAI-compatible endpoints)
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Create an LLM client based on the configured provider
    ///
    /// This follows the pattern from `radkit_docs/docs/core-concepts/llm-providers.md`:
    /// each provider uses `from_env()` to load API keys from environment variables.
    pub fn create_llm(&self) -> anyhow::Result<Box<dyn BaseLlm + Send + Sync>> {
        match self.provider {
            LlmProvider::Anthropic => Ok(Box::new(AnthropicLlm::from_env(&self.model)?)),
            LlmProvider::OpenAI => {
                let llm = if let Some(base_url) = &self.base_url {
                    OpenAILlm::from_env(&self.model)?.with_base_url(base_url)
                } else {
                    OpenAILlm::from_env(&self.model)?
                };
                Ok(Box::new(llm))
            }
            LlmProvider::Gemini => Ok(Box::new(GeminiLlm::from_env(&self.model)?)),
            LlmProvider::OpenRouter => Ok(Box::new(OpenRouterLlm::from_env(&self.model)?)),
            LlmProvider::Grok => Ok(Box::new(GrokLlm::from_env(&self.model)?)),
            LlmProvider::DeepSeek => Ok(Box::new(DeepSeekLlm::from_env(&self.model)?)),
        }
    }

    /// Legacy: Create an Anthropic LLM client (for backward compatibility)
    pub fn to_anthropic_llm(&self) -> anyhow::Result<AnthropicLlm> {
        Ok(AnthropicLlm::from_env(&self.model)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ModelConfig::default();
        assert_eq!(config.provider, LlmProvider::Anthropic);
        assert!(config.model.contains("claude"));
    }

    #[test]
    fn test_provider_display_names() {
        assert_eq!(LlmProvider::Anthropic.display_name(), "Anthropic");
        assert_eq!(LlmProvider::OpenAI.display_name(), "OpenAI");
    }

    #[test]
    fn test_base_url_support() {
        assert!(LlmProvider::OpenAI.supports_base_url());
        assert!(!LlmProvider::Anthropic.supports_base_url());
    }

    #[test]
    fn test_model_config_serialization() {
        let config = ModelConfig::with_provider(LlmProvider::OpenAI, "gpt-4o");
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("openai"));
        assert!(json.contains("gpt-4o"));
    }
}
