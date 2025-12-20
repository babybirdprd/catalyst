//! # Builder Skill
//!
//! A2A-native skill that implements features in code.
//!
//! ## Two Execution Modes
//!
//! 1. **A2A Mode (`on_request`):** Uses radkit `ToolContext` state for worktree path.
//!    Tools read worktree from `ctx.state().get_state("worktree_path")`.
//!
//! 2. **SDK Mode (`run()`):** Uses `FunctionTool` closures that capture the worktree path.
//!    This allows direct calls from the Coordinator without A2A runtime.
//!
//! ## Reference Documentation
//! - See `radkit_docs/docs/guides/tool-execution.md` for `LlmWorker` + tools
//! - See `radkit_docs/docs/guides/stateful-tools.md` for `ToolContext` patterns

use crate::models::ModelConfig;
use crate::run_llm_worker;
use crate::skills::artifact_registry::{BuildArtifact, FileChange};
use crate::skills::tools::{build_tools, file_tools};
use async_trait::async_trait;
use radkit::agent::{Artifact, LlmWorker, OnRequestResult, SkillHandler};
use radkit::errors::{AgentError, AgentResult};
use radkit::macros::{skill, LLMOutput};
use radkit::models::Content;
use radkit::runtime::context::{ProgressSender, State};
use radkit::runtime::AgentRuntime;
use radkit::tools::{FunctionTool, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use std::process::Command;

/// Output from the builder skill
///
/// This is the canonical output type for the BuilderSkill.
/// It's designed to be more streamlined than the legacy `agents::BuilderOutput`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, LLMOutput)]
pub struct BuilderOutput {
    /// Whether the implementation was successful
    pub success: bool,
    /// Summary of changes made
    pub summary: String,
    /// List of files modified
    pub files_modified: Vec<String>,
    /// Any compiler errors encountered
    pub errors: Vec<String>,
    /// Whether cargo build passed
    #[serde(default)]
    pub build_passed: bool,
    /// Whether tests passed
    #[serde(default)]
    pub tests_passed: bool,
    /// Number of build/fix iterations
    #[serde(default)]
    pub iterations: u32,
}

/// Implementation skill for the Builder agent
#[skill(
    id = "implement",
    name = "Implementation",
    description = "Implements features in code. Reads files, makes changes, runs builds to verify.",
    tags = ["coding", "rust", "implementation"],
    examples = ["Implement feature X", "Add a new function to handle Y"],
    input_modes = ["text/plain", "application/json"],
    output_modes = ["application/json"]
)]
pub struct BuilderSkill {
    config: ModelConfig,
}

impl BuilderSkill {
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
    ///
    /// This method creates `FunctionTool` closures that capture the worktree path,
    /// allowing it to work without an A2A runtime context.
    pub async fn run(
        mission: &str,
        worktree_path: &Path,
        config: &ModelConfig,
    ) -> anyhow::Result<BuilderOutput> {
        Self::run_internal(mission, worktree_path, config).await
    }

    /// Internal implementation that uses FunctionTool closures with captured worktree path.
    ///
    /// Based on the pattern from `radkit_docs/docs/guides/tool-execution.md`:
    /// - Create tools using `FunctionTool::new()` with closures
    /// - Closures capture the worktree path at call time
    ///
    /// Uses provider matching (similar to `run_llm_worker!` macro) because
    /// `LlmWorker::builder()` requires a concrete type implementing `BaseLlm`,
    /// not a boxed trait object.
    async fn run_internal(
        mission: &str,
        worktree_path: &Path,
        config: &ModelConfig,
    ) -> anyhow::Result<BuilderOutput> {
        use crate::models::LlmProvider;
        use radkit::models::providers::{
            AnthropicLlm, DeepSeekLlm, GeminiLlm, GrokLlm, OpenAILlm, OpenRouterLlm,
        };

        // Create tools that capture the worktree path
        let tools = create_builder_tools(worktree_path);

        // Match on provider to get concrete LLM type
        match config.provider {
            LlmProvider::Anthropic => {
                let llm = AnthropicLlm::from_env(&config.model)?;
                run_with_tools(llm, mission, tools).await
            }
            LlmProvider::OpenAI => {
                let mut llm = OpenAILlm::from_env(&config.model)?;
                if let Some(base_url) = &config.base_url {
                    llm = llm.with_base_url(base_url);
                }
                run_with_tools(llm, mission, tools).await
            }
            LlmProvider::Gemini => {
                let llm = GeminiLlm::from_env(&config.model)?;
                run_with_tools(llm, mission, tools).await
            }
            LlmProvider::OpenRouter => {
                let llm = OpenRouterLlm::from_env(&config.model)?;
                run_with_tools(llm, mission, tools).await
            }
            LlmProvider::Grok => {
                let llm = GrokLlm::from_env(&config.model)?;
                run_with_tools(llm, mission, tools).await
            }
            LlmProvider::DeepSeek => {
                let llm = DeepSeekLlm::from_env(&config.model)?;
                run_with_tools(llm, mission, tools).await
            }
        }
    }
}

/// Builder tools tuple type for cleaner code
type BuilderTools = (
    FunctionTool,
    FunctionTool,
    FunctionTool,
    FunctionTool,
    FunctionTool,
    FunctionTool,
);

/// Create all builder tools with captured worktree path
fn create_builder_tools(worktree_path: &Path) -> BuilderTools {
    let wt_read = worktree_path.to_path_buf();
    let wt_write = worktree_path.to_path_buf();
    let wt_list = worktree_path.to_path_buf();
    let wt_build = worktree_path.to_path_buf();
    let wt_test = worktree_path.to_path_buf();
    let wt_check = worktree_path.to_path_buf();

    // Tool: read_file (inline logic like legacy agents/builder.rs)
    let read_file = FunctionTool::new(
        "read_file",
        "Read a file from the worktree. Args: {\"path\": \"relative/path\"}",
        move |args, _ctx| {
            let wt = wt_read.clone();
            Box::pin(async move {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let full_path = wt.join(path);

                // Security: ensure path is within worktree
                if !full_path.starts_with(&wt) {
                    return ToolResult::error("Path escapes worktree sandbox");
                }

                match std::fs::read_to_string(&full_path) {
                    Ok(content) => ToolResult::success(json!({
                        "path": path,
                        "content": content,
                        "lines": content.lines().count()
                    })),
                    Err(e) => ToolResult::error(format!("Failed to read {}: {}", path, e)),
                }
            })
        },
    );

    // Tool: write_file
    let write_file = FunctionTool::new(
        "write_file",
        "Write content to a file. Args: {\"path\": \"relative/path\", \"content\": \"...\"}",
        move |args, _ctx| {
            let wt = wt_write.clone();
            Box::pin(async move {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let full_path = wt.join(path);

                if !full_path.starts_with(&wt) {
                    return ToolResult::error("Path escapes worktree sandbox");
                }

                if let Some(parent) = full_path.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return ToolResult::error(format!("Failed to create dirs: {}", e));
                    }
                }

                match std::fs::write(&full_path, content) {
                    Ok(_) => ToolResult::success(json!({
                        "path": path,
                        "bytes_written": content.len()
                    })),
                    Err(e) => ToolResult::error(format!("Failed to write {}: {}", path, e)),
                }
            })
        },
    );

    // Tool: list_dir
    let list_dir = FunctionTool::new(
        "list_dir",
        "List files in a directory. Args: {\"path\": \"relative/path\"} or {}",
        move |args, _ctx| {
            let wt = wt_list.clone();
            Box::pin(async move {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                let full_path = if path.is_empty() || path == "." {
                    wt.clone()
                } else {
                    wt.join(path)
                };

                if !full_path.starts_with(&wt) {
                    return ToolResult::error("Path escapes worktree sandbox");
                }

                match std::fs::read_dir(&full_path) {
                    Ok(entries) => {
                        let mut files = Vec::new();
                        let mut dirs = Vec::new();
                        for entry in entries.flatten() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if entry.path().is_dir() {
                                dirs.push(name);
                            } else {
                                files.push(name);
                            }
                        }
                        ToolResult::success(json!({
                            "path": path,
                            "files": files,
                            "directories": dirs
                        }))
                    }
                    Err(e) => ToolResult::error(format!("Failed to list directory: {}", e)),
                }
            })
        },
    );

    // Tool: run_build
    let run_build = FunctionTool::new(
        "run_build",
        "Run cargo build. Returns structured compiler errors.",
        move |_args, _ctx| {
            let wt = wt_build.clone();
            Box::pin(async move {
                let output = Command::new("cargo")
                    .args(["build", "--message-format=json"])
                    .current_dir(&wt)
                    .output();

                match output {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        let success = out.status.success();

                        let errors: Vec<serde_json::Value> = stdout
                            .lines()
                            .filter_map(|line| serde_json::from_str(line).ok())
                            .filter(|v: &serde_json::Value| {
                                v.get("reason").and_then(|r| r.as_str()) == Some("compiler-message")
                            })
                            .collect();

                        ToolResult::success(json!({
                            "success": success,
                            "error_count": errors.len(),
                            "errors": errors,
                            "stderr": stderr.to_string()
                        }))
                    }
                    Err(e) => ToolResult::error(format!("Build failed: {}", e)),
                }
            })
        },
    );

    // Tool: run_test
    let run_test = FunctionTool::new(
        "run_test",
        "Run cargo test. Returns test summary.",
        move |_args, _ctx| {
            let wt = wt_test.clone();
            Box::pin(async move {
                let output = Command::new("cargo")
                    .args(["test", "--", "--format=terse"])
                    .current_dir(&wt)
                    .output();

                match output {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let success = out.status.success();

                        ToolResult::success(json!({
                            "success": success,
                            "output": stdout.to_string()
                        }))
                    }
                    Err(e) => ToolResult::error(format!("Test failed: {}", e)),
                }
            })
        },
    );

    // Tool: run_check
    let run_check = FunctionTool::new(
        "run_check",
        "Run cargo check. Faster than build.",
        move |_args, _ctx| {
            let wt = wt_check.clone();
            Box::pin(async move {
                let output = Command::new("cargo")
                    .args(["check", "--message-format=json"])
                    .current_dir(&wt)
                    .output();

                match output {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let success = out.status.success();

                        let errors: Vec<serde_json::Value> = stdout
                            .lines()
                            .filter_map(|line| serde_json::from_str(line).ok())
                            .filter(|v: &serde_json::Value| {
                                v.get("reason").and_then(|r| r.as_str()) == Some("compiler-message")
                            })
                            .collect();

                        ToolResult::success(json!({
                            "success": success,
                            "error_count": errors.len(),
                            "errors": errors
                        }))
                    }
                    Err(e) => ToolResult::error(format!("Check failed: {}", e)),
                }
            })
        },
    );

    (
        read_file, write_file, list_dir, run_build, run_test, run_check,
    )
}

/// Run the LlmWorker with tools for any concrete LLM type
async fn run_with_tools<L: radkit::models::BaseLlm + 'static>(
    llm: L,
    mission: &str,
    tools: BuilderTools,
) -> anyhow::Result<BuilderOutput> {
    let (read_file, write_file, list_dir, run_build, run_test, run_check) = tools;

    let worker = LlmWorker::<BuilderOutput>::builder(llm)
        .with_system_instructions(SYSTEM_PROMPT)
        .with_tool(read_file)
        .with_tool(write_file)
        .with_tool(list_dir)
        .with_tool(run_build)
        .with_tool(run_test)
        .with_tool(run_check)
        .build();

    let result = worker.run(mission).await?;
    Ok(result)
}

#[async_trait]
impl SkillHandler for BuilderSkill {
    async fn on_request(
        &self,
        state: &mut State,
        progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        // Get the task content
        let task_description = content.first_text().unwrap_or_default();

        progress.send_update("Starting implementation...").await?;

        // Get worktree path from session state (set by coordinator)
        let worktree_path: String = state
            .session()
            .load::<String>("worktree_path")
            .ok()
            .flatten()
            .unwrap_or_else(|| ".".to_string());

        // Store for tools that use ToolContext state
        state.task().save("worktree_path", &worktree_path)?;

        progress.send_update("Analyzing codebase...").await?;

        progress.send_update("Implementing changes...").await?;

        // Use the macro for A2A mode with tools that read from ToolContext
        let result = run_llm_worker!(
            &self.config,
            BuilderOutput,
            SYSTEM_PROMPT,
            task_description,
            file_tools::read_file,
            file_tools::write_file,
            file_tools::list_dir,
            build_tools::run_build,
            build_tools::run_check,
            build_tools::run_test,
        )
        .map_err(|e| AgentError::Internal {
            component: "builder_skill".to_string(),
            reason: e.to_string(),
        })?;

        progress.send_update("Build complete.").await?;

        // Create artifact with build data (hybrid approach: paths + metrics)
        let artifact_data = BuildArtifact {
            success: result.success,
            files: result
                .files_modified
                .iter()
                .map(|path| FileChange {
                    path: path.clone(),
                    action: "modified".to_string(),
                    lines_added: 0,   // Would need git diff to calculate
                    lines_removed: 0, // Would need git diff to calculate
                    change_summary: "File modified".to_string(),
                })
                .collect(),
            build_passed: result.build_passed,
            tests_passed: result.tests_passed,
            iterations: result.iterations,
            error_count: result.errors.len(),
        };

        let artifact = Artifact::from_json("build.json", &artifact_data).map_err(|e| {
            AgentError::Internal {
                component: "builder_skill".to_string(),
                reason: format!("Failed to create artifact: {}", e),
            }
        })?;

        // Return completed with result
        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text(&result.summary)),
            artifacts: vec![artifact],
        })
    }
}

const SYSTEM_PROMPT: &str = include_str!("defaults/builder.md");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_output_serialization() {
        let output = BuilderOutput {
            success: true,
            summary: "Implemented feature".to_string(),
            files_modified: vec!["src/lib.rs".to_string()],
            errors: vec![],
            build_passed: true,
            tests_passed: true,
            iterations: 3,
        };

        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("success"));
        assert!(json.contains("src/lib.rs"));
    }
}
