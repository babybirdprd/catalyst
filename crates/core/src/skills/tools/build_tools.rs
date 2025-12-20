//! # Build Tools
//!
//! Tools for running cargo commands with structured output.
//! Uses ToolContext to maintain worktree_path state across calls.

use radkit::macros::tool;
use radkit::tools::ToolResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::process::Command;

/// Structured compiler error
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompilerError {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub code: Option<String>,
    pub level: String,
}

/// Empty args for tools that get worktree from context
#[derive(Deserialize, JsonSchema)]
pub struct BuildArgs {}

/// Run cargo build and return structured errors
#[tool(description = "Run cargo build in the worktree. Returns structured compiler errors if any.")]
pub async fn run_build(args: BuildArgs, ctx: &ToolContext<'_>) -> ToolResult {
    let _ = args; // Acknowledge empty args
    let worktree: PathBuf = ctx
        .state()
        .get_state("worktree_path")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let output = Command::new("cargo")
        .args(["build", "--message-format=json"])
        .current_dir(&worktree)
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
        Err(e) => ToolResult::error(format!("Failed to run cargo build: {}", e)),
    }
}

/// Run cargo test and return results
#[tool(description = "Run cargo test in the worktree. Returns test results summary.")]
pub async fn run_test(args: BuildArgs, ctx: &ToolContext<'_>) -> ToolResult {
    let _ = args;
    let worktree: PathBuf = ctx
        .state()
        .get_state("worktree_path")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let output = Command::new("cargo")
        .args(["test", "--", "--format=terse"])
        .current_dir(&worktree)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let success = out.status.success();

            ToolResult::success(json!({
                "success": success,
                "output": stdout.to_string(),
                "stderr": stderr.to_string()
            }))
        }
        Err(e) => ToolResult::error(format!("Failed to run cargo test: {}", e)),
    }
}

/// Run cargo clippy and return lint warnings
#[tool(description = "Run cargo clippy in the worktree. Returns lint warnings as structured data.")]
pub async fn run_clippy(args: BuildArgs, ctx: &ToolContext<'_>) -> ToolResult {
    let _ = args;
    let worktree: PathBuf = ctx
        .state()
        .get_state("worktree_path")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let output = Command::new("cargo")
        .args(["clippy", "--message-format=json", "--", "-D", "warnings"])
        .current_dir(&worktree)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let success = out.status.success();

            let warnings: Vec<serde_json::Value> = stdout
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .filter(|v: &serde_json::Value| {
                    v.get("reason").and_then(|r| r.as_str()) == Some("compiler-message")
                })
                .collect();

            ToolResult::success(json!({
                "success": success,
                "warning_count": warnings.len(),
                "warnings": warnings
            }))
        }
        Err(e) => ToolResult::error(format!("Failed to run cargo clippy: {}", e)),
    }
}

/// Run cargo check (faster than build, no artifacts)
#[tool(description = "Run cargo check in the worktree. Faster than build, just checks for errors.")]
pub async fn run_check(args: BuildArgs, ctx: &ToolContext<'_>) -> ToolResult {
    let _ = args;
    let worktree: PathBuf = ctx
        .state()
        .get_state("worktree_path")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let output = Command::new("cargo")
        .args(["check", "--message-format=json"])
        .current_dir(&worktree)
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
        Err(e) => ToolResult::error(format!("Failed to run cargo check: {}", e)),
    }
}
