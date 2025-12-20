//! # LLM Helpers
//!
//! Shared utilities for creating LLM clients from ModelConfig.
//! Eliminates duplicate provider matching code across skills.

/// Macro to run an LlmFunction with any provider.
/// Handles the provider matching once in a central place.
#[macro_export]
macro_rules! run_llm_function {
    ($config:expr, $output_type:ty, $system_prompt:expr, $input:expr) => {{
        use radkit::agent::LlmFunction;
        use radkit::models::providers::{
            AnthropicLlm, DeepSeekLlm, GeminiLlm, GrokLlm, OpenAILlm, OpenRouterLlm,
        };
        use $crate::models::LlmProvider;

        let config = $config;
        let result: anyhow::Result<$output_type> = match config.provider {
            LlmProvider::Anthropic => {
                let llm = AnthropicLlm::from_env(&config.model)?;
                let func =
                    LlmFunction::<$output_type>::new_with_system_instructions(llm, $system_prompt);
                func.run($input).await.map_err(Into::into)
            }
            LlmProvider::OpenAI => {
                let mut llm = OpenAILlm::from_env(&config.model)?;
                if let Some(base_url) = &config.base_url {
                    llm = llm.with_base_url(base_url);
                }
                let func =
                    LlmFunction::<$output_type>::new_with_system_instructions(llm, $system_prompt);
                func.run($input).await.map_err(Into::into)
            }
            LlmProvider::Gemini => {
                let llm = GeminiLlm::from_env(&config.model)?;
                let func =
                    LlmFunction::<$output_type>::new_with_system_instructions(llm, $system_prompt);
                func.run($input).await.map_err(Into::into)
            }
            LlmProvider::OpenRouter => {
                let llm = OpenRouterLlm::from_env(&config.model)?;
                let func =
                    LlmFunction::<$output_type>::new_with_system_instructions(llm, $system_prompt);
                func.run($input).await.map_err(Into::into)
            }
            LlmProvider::Grok => {
                let llm = GrokLlm::from_env(&config.model)?;
                let func =
                    LlmFunction::<$output_type>::new_with_system_instructions(llm, $system_prompt);
                func.run($input).await.map_err(Into::into)
            }
            LlmProvider::DeepSeek => {
                let llm = DeepSeekLlm::from_env(&config.model)?;
                let func =
                    LlmFunction::<$output_type>::new_with_system_instructions(llm, $system_prompt);
                func.run($input).await.map_err(Into::into)
            }
        };
        result
    }};
}

/// Macro to run an LlmWorker with any provider.
/// Use this for skills that need tools (like Researcher).
#[macro_export]
macro_rules! run_llm_worker {
    ($config:expr, $output_type:ty, $system_prompt:expr, $input:expr, $($tool:expr),* $(,)?) => {{
        use $crate::models::LlmProvider;
        use radkit::agent::LlmWorker;
        use radkit::models::providers::{
            AnthropicLlm, DeepSeekLlm, GeminiLlm, GrokLlm, OpenAILlm, OpenRouterLlm,
        };

        let config = $config;
        let result: anyhow::Result<$output_type> = match config.provider {
            LlmProvider::Anthropic => {
                let llm = AnthropicLlm::from_env(&config.model)?;
                let worker = LlmWorker::<$output_type>::builder(llm)
                    .with_system_instructions($system_prompt)
                    $(.with_tool($tool))*
                    .build();
                worker.run($input).await.map_err(Into::into)
            }
            LlmProvider::OpenAI => {
                let mut llm = OpenAILlm::from_env(&config.model)?;
                if let Some(base_url) = &config.base_url {
                    llm = llm.with_base_url(base_url);
                }
                let worker = LlmWorker::<$output_type>::builder(llm)
                    .with_system_instructions($system_prompt)
                    $(.with_tool($tool))*
                    .build();
                worker.run($input).await.map_err(Into::into)
            }
            LlmProvider::Gemini => {
                let llm = GeminiLlm::from_env(&config.model)?;
                let worker = LlmWorker::<$output_type>::builder(llm)
                    .with_system_instructions($system_prompt)
                    $(.with_tool($tool))*
                    .build();
                worker.run($input).await.map_err(Into::into)
            }
            LlmProvider::OpenRouter => {
                let llm = OpenRouterLlm::from_env(&config.model)?;
                let worker = LlmWorker::<$output_type>::builder(llm)
                    .with_system_instructions($system_prompt)
                    $(.with_tool($tool))*
                    .build();
                worker.run($input).await.map_err(Into::into)
            }
            LlmProvider::Grok => {
                let llm = GrokLlm::from_env(&config.model)?;
                let worker = LlmWorker::<$output_type>::builder(llm)
                    .with_system_instructions($system_prompt)
                    $(.with_tool($tool))*
                    .build();
                worker.run($input).await.map_err(Into::into)
            }
            LlmProvider::DeepSeek => {
                let llm = DeepSeekLlm::from_env(&config.model)?;
                let worker = LlmWorker::<$output_type>::builder(llm)
                    .with_system_instructions($system_prompt)
                    $(.with_tool($tool))*
                    .build();
                worker.run($input).await.map_err(Into::into)
            }
        };
        result
    }};
}

pub use run_llm_function;
pub use run_llm_worker;
